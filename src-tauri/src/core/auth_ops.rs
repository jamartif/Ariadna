//! Métodos de autenticación del clonado: PAT, sesión de `gh`/`glab` y OAuth (device code).
//!
//! # Regla de seguridad
//!
//! **El token nunca viaja a la interfaz** salvo el PAT, que el usuario escribe y que se borra al
//! usarlo. Los tokens de la sesión del CLI y del flujo OAuth viven **solo en memoria de Rust**:
//! el flujo OAuth se identifica con un `flowId` opaco que la UI usa para sondear y para clonar.
//! Ninguno se guarda, se registra ni aparece en un mensaje de error (`Debug` redactado y
//! `zeroize` al soltarlo).
//!
//! # Flujo OAuth (inactivo sin `client_id`)
//!
//! Requiere que el usuario registre su propia OAuth App y pegue el `client_id` en
//! `settings.json` (`oauthGitHubClientId` / `oauthGitLabClientId`), porque Ariadna no trae una y
//! no puede registrar aplicaciones en su nombre. Sin `client_id`, [`AppCore::auth_options`]
//! devuelve `oauth.available = false` con el motivo y la UI oculta la opción. Los pasos están en
//! `docs/oauth-cli.md`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ariadna_hosting::{
    CliSession, CliTool, DeviceCode, DeviceFlow, DeviceFlowConfig, DevicePoll, OAuthProvider,
    Provider, RemoteInfo, Secret, SLOWDOWN_STEP,
};
use serde::{Deserialize, Serialize};

use super::app::AppCore;
use super::error::{ApiError, ApiResult};

/// Cuánto tiempo se guarda un token ya autorizado para el `clone_repository` que lo usa.
const AUTHORIZED_TTL: Duration = Duration::from_secs(10 * 60);
/// Tope de flujos vivos (una UI normal tiene uno; el tope evita crecer sin fin).
const MAX_FLOWS: usize = 8;

/// Estado de la sesión de un CLI, para el selector de método.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliAuthStatus {
    /// `gh` o `glab`.
    pub id: String,
    /// `GitHub CLI` / `GitLab CLI`.
    pub label: String,
    pub installed: bool,
    pub logged_in: bool,
    pub version: Option<String>,
    /// Por qué no sirve, en español (incluye el comando a ejecutar).
    pub detail: Option<String>,
    /// Comando de inicio de sesión (`gh auth login`).
    pub login_command: String,
    /// Usuario que se enviará al remoto (`x-access-token`, `oauth2`).
    pub username: String,
}

/// Disponibilidad del flujo OAuth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthAvailability {
    /// `github` o `gitlab`.
    pub provider: String,
    /// `true` solo si hay `client_id` configurado.
    pub available: bool,
    /// Motivo cuando no está disponible (qué falta y dónde se configura).
    pub reason: Option<String>,
    /// Permisos que se pedirán.
    pub scopes: Vec<String>,
}

/// Respuesta de `clone_auth_options`: qué métodos ofrece la URL dada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthOptions {
    /// Proveedor reconocido (`github`, `gitlab`, `azureDevOps`, `bitbucket`, `other`).
    pub provider: String,
    /// La URL usa HTTPS (las credenciales solo aplican ahí).
    pub https: bool,
    /// Sesiones de CLI relevantes para este proveedor (vacío si no hay ninguna aplicable).
    pub cli: Vec<CliAuthStatus>,
    /// Flujo OAuth del proveedor (`null` si el proveedor no lo tiene).
    pub oauth: Option<OAuthAvailability>,
}

/// Datos para iniciar el flujo device code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLoginStart {
    /// Identificador opaco del flujo (no contiene el `device_code`).
    pub flow_id: String,
    /// Código que el usuario teclea en la web.
    pub user_code: String,
    /// Página donde se introduce.
    pub verification_uri: String,
    /// Segundos hasta que caduca el código.
    pub expires_in: u64,
    /// Segundos entre sondeos.
    pub interval: u64,
    pub provider: String,
}

/// Resultado de un sondeo del flujo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLoginPoll {
    /// `pending` | `slowDown` | `authorized` | `expired` | `denied`.
    pub state: String,
    /// Intervalo a usar en el siguiente sondeo (segundos): cambia con `slow_down`.
    pub interval: Option<u64>,
}

/// Flujo vivo en memoria: código, caducidad y (si ya se autorizó) el token.
struct PendingFlow {
    provider: OAuthProvider,
    flow: Arc<DeviceFlow>,
    code: DeviceCode,
    expires_at: Instant,
    interval: Duration,
    token: Option<Secret>,
    /// Cuándo caduca el token ya autorizado (para poder clonar justo después).
    authorized_until: Option<Instant>,
}

impl std::fmt::Debug for PendingFlow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingFlow")
            .field("provider", &self.provider)
            .field("expires_at", &self.expires_at)
            .field("interval", &self.interval)
            .field("authorized", &self.token.is_some())
            .finish_non_exhaustive()
    }
}

fn flows() -> &'static Mutex<HashMap<String, PendingFlow>> {
    static FLOWS: OnceLock<Mutex<HashMap<String, PendingFlow>>> = OnceLock::new();
    FLOWS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_flows() -> std::sync::MutexGuard<'static, HashMap<String, PendingFlow>> {
    flows().lock().unwrap_or_else(PoisonError::into_inner)
}

static NEXT_FLOW: AtomicU64 = AtomicU64::new(1);

/// Identificador opaco del flujo: contador + nanosegundos (sin secretos, no adivinable por la UI
/// de turno, que es quien lo recibe).
fn new_flow_id() -> String {
    let n = NEXT_FLOW.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("oauth-{n}-{nanos:09}")
}

/// Borra los flujos caducados y los tokens autorizados que ya nadie va a usar.
fn prune(map: &mut HashMap<String, PendingFlow>) {
    let now = Instant::now();
    map.retain(|_, f| match f.authorized_until {
        Some(until) => until > now,
        None => f.expires_at > now,
    });
}

/// Identificador del proveedor tal como lo serializa `ariadna-hosting` (camelCase).
fn provider_id(p: Provider) -> &'static str {
    match p {
        Provider::AzureDevOps => "azureDevOps",
        Provider::GitHub => "github",
        Provider::GitLab => "gitlab",
        Provider::Bitbucket => "bitbucket",
        Provider::Other => "other",
    }
}

/// Proveedor OAuth que corresponde a un proveedor de alojamiento y a su host.
fn oauth_provider_for(info: &RemoteInfo) -> Option<OAuthProvider> {
    match info.provider {
        Provider::GitHub => Some(OAuthProvider::GitHub),
        Provider::GitLab => Some(OAuthProvider::GitLab),
        _ => None,
    }
}

impl AppCore {
    /// Qué métodos de autenticación ofrecer para esta URL, con el estado **real** de cada CLI.
    ///
    /// Comprueba `gh`/`glab` con `auth status` (sin pedir el token) y el `client_id` de los
    /// ajustes. Nunca devuelve un token.
    pub async fn auth_options(&self, url: &str) -> ApiResult<AuthOptions> {
        let info = RemoteInfo::parse(url);
        let provider = info.provider;
        let https = url.trim().to_ascii_lowercase().starts_with("https://");
        let settings = self.settings_snapshot();

        let tools: Vec<CliTool> = match provider {
            Provider::GitHub => vec![CliTool::Gh],
            Provider::GitLab => vec![CliTool::Glab],
            // Otros proveedores no tienen CLI propio que Ariadna sepa usar.
            _ => Vec::new(),
        };
        let mut cli = Vec::with_capacity(tools.len());
        for tool in tools {
            let st = CliSession::new(tool).status().await;
            cli.push(CliAuthStatus {
                id: tool.id().to_owned(),
                label: tool.label().to_owned(),
                installed: st.installed,
                logged_in: st.logged_in,
                version: st.version,
                detail: st.detail,
                login_command: tool.login_command().to_owned(),
                username: tool.default_username().to_owned(),
            });
        }

        let oauth = oauth_provider_for(&info).map(|p| {
            let client_id = match p {
                OAuthProvider::GitHub => settings.oauth_github_client_id.clone(),
                OAuthProvider::GitLab => settings.oauth_gitlab_client_id.clone(),
            };
            let available = client_id.is_some();
            OAuthAvailability {
                provider: p.id().to_owned(),
                available,
                reason: if available {
                    None
                } else {
                    Some(format!(
                        "Falta el client_id de tu aplicación OAuth de {}. Regístrala en {} y pégalo en Ajustes › Cuentas (ver docs/oauth-cli.md).",
                        p.label(),
                        if p == OAuthProvider::GitHub {
                            "GitHub › Settings › Developer settings › OAuth Apps"
                        } else {
                            "GitLab › Preferences › Applications"
                        }
                    ))
                },
                scopes: p.default_scopes().iter().map(|s| (*s).to_owned()).collect(),
            }
        });

        Ok(AuthOptions {
            provider: provider_id(provider).to_owned(),
            https,
            cli,
            oauth,
        })
    }

    /// Paso 1 del flujo device code: pide el código y lo guarda en memoria hasta que caduque.
    pub async fn device_login_start(&self, url: &str) -> ApiResult<DeviceLoginStart> {
        let info = RemoteInfo::parse(url);
        let provider = oauth_provider_for(&info).ok_or_else(|| {
            ApiError::new(
                "POLICY",
                "El inicio de sesión OAuth solo está disponible para GitHub y GitLab.",
                None,
            )
        })?;
        let settings = self.settings_snapshot();
        let client_id = match provider {
            OAuthProvider::GitHub => settings.oauth_github_client_id.clone(),
            OAuthProvider::GitLab => settings.oauth_gitlab_client_id.clone(),
        };
        let client_id = client_id.ok_or_else(|| {
            ApiError::new(
                "POLICY",
                format!(
                    "Falta el client_id de tu aplicación OAuth de {}. Configúralo en Ajustes › Cuentas (ver docs/oauth-cli.md).",
                    provider.label()
                ),
                None,
            )
        })?;
        let flow =
            DeviceFlow::new(DeviceFlowConfig::new(provider, client_id)).map_err(ApiError::from)?;
        let code = flow.request().await.map_err(ApiError::from)?;
        let flow_id = new_flow_id();
        let start = DeviceLoginStart {
            flow_id: flow_id.clone(),
            user_code: code.user_code.clone(),
            verification_uri: code.verification_uri.clone(),
            expires_in: code.expires_in.as_secs(),
            interval: code.interval.as_secs(),
            provider: provider.id().to_owned(),
        };
        let mut map = lock_flows();
        prune(&mut map);
        while map.len() >= MAX_FLOWS {
            // Se descarta el más antiguo sin token; si todos tienen token, el primero.
            let victim = map
                .iter()
                .find(|(_, f)| f.token.is_none())
                .or_else(|| map.iter().next())
                .map(|(k, _)| k.clone());
            match victim {
                Some(k) => {
                    map.remove(&k);
                }
                None => break,
            }
        }
        map.insert(
            flow_id,
            PendingFlow {
                provider,
                flow: Arc::new(flow),
                expires_at: Instant::now() + code.expires_in,
                interval: code.interval,
                token: None,
                code,
                authorized_until: None,
            },
        );
        Ok(start)
    }

    /// Paso 3: un sondeo. La UI repite cada `interval` hasta `authorized`/`expired`/`denied`.
    pub async fn device_login_poll(&self, flow_id: &str) -> ApiResult<DeviceLoginPoll> {
        // El flujo se saca del mapa mientras se habla con el servidor (no se retiene el candado
        // del `Mutex` en un `await`).
        let pending = lock_flows().remove(flow_id).ok_or_else(|| {
            ApiError::new(
                "POLICY",
                "El inicio de sesión ya no está en curso. Vuelve a empezar.",
                None,
            )
        })?;
        if pending.authorized_until.is_some() {
            // Ya autorizado: sigue disponible para clonar sin volver a sondear.
            let interval = pending.interval.as_secs();
            lock_flows().insert(flow_id.to_owned(), pending);
            return Ok(DeviceLoginPoll {
                state: "authorized".into(),
                interval: Some(interval),
            });
        }
        if Instant::now() >= pending.expires_at {
            return Ok(DeviceLoginPoll {
                state: "expired".into(),
                interval: None,
            });
        }
        let polled = pending.flow.poll(&pending.code).await;
        match polled {
            Ok(DevicePoll::Pending) => {
                let interval = pending.interval.as_secs();
                lock_flows().insert(flow_id.to_owned(), pending);
                Ok(DeviceLoginPoll {
                    state: "pending".into(),
                    interval: Some(interval),
                })
            }
            Ok(DevicePoll::SlowDown) => {
                let interval = (pending.interval + SLOWDOWN_STEP).min(Duration::from_secs(60));
                let mut pending = pending;
                pending.interval = interval;
                lock_flows().insert(flow_id.to_owned(), pending);
                Ok(DeviceLoginPoll {
                    state: "slowDown".into(),
                    interval: Some(interval.as_secs()),
                })
            }
            Ok(DevicePoll::Authorized(secret)) => {
                let mut pending = pending;
                let until = Instant::now() + AUTHORIZED_TTL;
                pending.token = Some(secret);
                pending.authorized_until = Some(until);
                let interval = pending.interval.as_secs();
                lock_flows().insert(flow_id.to_owned(), pending);
                Ok(DeviceLoginPoll {
                    state: "authorized".into(),
                    interval: Some(interval),
                })
            }
            Ok(DevicePoll::Expired) => Ok(DeviceLoginPoll {
                state: "expired".into(),
                interval: None,
            }),
            Ok(DevicePoll::Denied) => Ok(DeviceLoginPoll {
                state: "denied".into(),
                interval: None,
            }),
            Err(e) => {
                // Un fallo de red no invalida el código: el flujo se conserva para reintentar.
                let interval = pending.interval.as_secs();
                lock_flows().insert(flow_id.to_owned(), pending);
                let mut err = ApiError::from(e);
                // La UI aprovecha el detalle para no perder el intervalo pactado.
                err.detail = Some(interval.to_string());
                Err(err)
            }
        }
    }

    /// Cancela un flujo en curso y olvida su token (si lo había).
    pub async fn device_login_cancel(&self, flow_id: &str) -> ApiResult<()> {
        lock_flows().remove(flow_id);
        Ok(())
    }

    /// Token ya autorizado de un flujo (se **consume**: solo vale para un clonado).
    pub(crate) fn take_oauth_token(&self, flow_id: &str) -> ApiResult<Secret> {
        let mut map = lock_flows();
        prune(&mut map);
        let flow = map.remove(flow_id).ok_or_else(|| {
            ApiError::new(
                "AUTH",
                "La autorización OAuth ha caducado. Vuelve a iniciar sesión con el navegador.",
                None,
            )
        })?;
        flow.token.ok_or_else(|| {
            ApiError::new(
                "AUTH",
                "Todavía no has autorizado la aplicación en el navegador.",
                None,
            )
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn prune_quita_lo_caducado_y_conserva_lo_vivo() {
        // Sin tokens: un flujo vivo y otro caducado.
        let mut map = HashMap::new();
        let cfg = DeviceFlowConfig::new(OAuthProvider::GitHub, "abc");
        let mk = |expires: Duration| PendingFlow {
            provider: OAuthProvider::GitHub,
            flow: Arc::new(DeviceFlow::new(cfg.clone()).unwrap()),
            code: DeviceCode {
                device_code: Secret::new("dc-1"),
                user_code: "AAAA-1111".into(),
                verification_uri: "https://github.com/login/device".into(),
                expires_in: expires,
                interval: Duration::from_secs(5),
            },
            expires_at: Instant::now() + expires,
            interval: Duration::from_secs(5),
            token: None,
            authorized_until: None,
        };
        map.insert("vivo".to_owned(), mk(Duration::from_secs(60)));
        map.insert("muerto".to_owned(), mk(Duration::from_secs(0)));
        prune(&mut map);
        assert!(map.contains_key("vivo"));
        assert!(!map.contains_key("muerto"));
    }

    #[test]
    fn el_id_de_flujo_no_lleva_el_codigo() {
        let id = new_flow_id();
        assert!(id.starts_with("oauth-"));
        assert_ne!(id, new_flow_id());
    }

    #[test]
    fn oauth_solo_para_github_y_gitlab() {
        assert_eq!(
            oauth_provider_for(&RemoteInfo::parse("https://github.com/o/r.git")),
            Some(OAuthProvider::GitHub)
        );
        assert_eq!(
            oauth_provider_for(&RemoteInfo::parse("https://gitlab.com/o/r.git")),
            Some(OAuthProvider::GitLab)
        );
        assert_eq!(
            oauth_provider_for(&RemoteInfo::parse("https://dev.azure.com/o/p/_git/r")),
            None
        );
    }
}
