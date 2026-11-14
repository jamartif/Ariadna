//! Flujo OAuth **device code** de GitHub y GitLab, listo pero **inactivo** hasta que el usuario
//! configure un `client_id` propio (ver `docs/oauth-cli.md`).
//!
//! Ariadna no trae `client_id` ni puede registrar aplicaciones OAuth por el usuario: el
//! `client_id` sale de los Ajustes (`settings.json`, `oauthGitHubClientId` /
//! `oauthGitLabClientId`). Sin él, la interfaz oculta esta opción.
//!
//! El flujo es el del RFC 8628:
//!
//! 1. [`DeviceFlow::request`] pide un código de dispositivo (`device_code` + `user_code` +
//!    `verification_uri` + `expires_in` + `interval`).
//! 2. La UI muestra `user_code` y abre `verification_uri` en el navegador del sistema.
//! 3. La UI llama a [`DeviceFlow::poll`] cada `interval` segundos. Las respuestas del servidor
//!    se traducen a [`DevicePoll`]: `authorization_pending` → [`DevicePoll::Pending`],
//!    `slow_down` → [`DevicePoll::SlowDown`] (sumar 5 s al intervalo), token → [`DevicePoll::Authorized`],
//!    `expired_token` → [`DevicePoll::Expired`], `access_denied` → [`DevicePoll::Denied`].
//!
//! El `device_code` y el `access_token` son [`Secret`]: no se serializan, no salen en `Debug` y
//! se borran de memoria al soltarlos. El token **nunca** se escribe en un registro ni en un
//! fichero; en la app vive solo en memoria del núcleo hasta el `clone_repository` que lo usa.

use std::fmt;
use std::time::Duration;

use serde::Deserialize;

use crate::auth::Secret;
use crate::error::{HostingError, HostingResult};

/// Paso que hay que sumar al intervalo cuando el servidor responde `slow_down`.
pub const SLOWDOWN_STEP: Duration = Duration::from_secs(5);
/// Intervalo mínimo entre sondeos (el servidor suele pedir 5 s).
pub const MIN_INTERVAL: Duration = Duration::from_secs(1);
/// Intervalo máximo que se acepta del servidor.
pub const MAX_INTERVAL: Duration = Duration::from_secs(60);
/// Tope de `expires_in` (15 min); por encima se recorta.
pub const MAX_EXPIRES: Duration = Duration::from_secs(900);

/// Proveedor del flujo device code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OAuthProvider {
    GitHub,
    GitLab,
}

/// Endpoints del flujo (se pueden sustituir en las pruebas).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    /// `POST` que devuelve `device_code`/`user_code`.
    pub device_code: String,
    /// `POST` que canjea `device_code` por `access_token`.
    pub token: String,
}

impl OAuthProvider {
    /// Identificador estable (`github`, `gitlab`).
    pub fn id(self) -> &'static str {
        match self {
            OAuthProvider::GitHub => "github",
            OAuthProvider::GitLab => "gitlab",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OAuthProvider::GitHub => "GitHub",
            OAuthProvider::GitLab => "GitLab",
        }
    }

    /// Permisos mínimos para clonar y publicar: `repo` en GitHub y los dos de repositorio en GitLab.
    pub fn default_scopes(self) -> &'static [&'static str] {
        match self {
            OAuthProvider::GitHub => &["repo"],
            OAuthProvider::GitLab => &["read_repository", "write_repository"],
        }
    }

    /// Endpoints públicos por defecto.
    pub fn endpoints(self) -> Endpoints {
        match self {
            OAuthProvider::GitHub => Endpoints {
                device_code: "https://github.com/login/device/code".into(),
                token: "https://github.com/login/oauth/access_token".into(),
            },
            OAuthProvider::GitLab => Endpoints {
                device_code: "https://gitlab.com/oauth/authorize_device".into(),
                token: "https://gitlab.com/oauth/token".into(),
            },
        }
    }
}

/// Configuración del flujo: proveedor, `client_id` del usuario y permisos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFlowConfig {
    pub provider: OAuthProvider,
    /// `client_id` de la OAuth App del usuario. **No** es un secreto, pero es su configuración.
    pub client_id: String,
    /// Permisos a pedir; vacío = [`OAuthProvider::default_scopes`].
    pub scopes: Vec<String>,
    /// Endpoints (solo las pruebas los cambian).
    pub endpoints: Endpoints,
}

impl DeviceFlowConfig {
    /// Configuración con los endpoints del proveedor.
    pub fn new(provider: OAuthProvider, client_id: impl Into<String>) -> Self {
        Self {
            provider,
            client_id: client_id.into(),
            scopes: Vec::new(),
            endpoints: provider.endpoints(),
        }
    }

    /// Configuración con endpoints propios (pruebas contra un servidor local).
    pub fn with_endpoints(mut self, endpoints: Endpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    fn scope_string(&self) -> String {
        if self.scopes.is_empty() {
            self.provider.default_scopes().join(" ")
        } else {
            self.scopes.join(" ")
        }
    }

    fn validate(&self) -> HostingResult<()> {
        let id = self.client_id.trim();
        if id.is_empty() {
            return Err(HostingError::policy(
                "Falta el client_id de la aplicación OAuth. Configúralo en Ajustes › Cuentas (ver docs/oauth-cli.md).",
            ));
        }
        if id.len() > 200 || id.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(HostingError::invalid("El client_id no es válido."));
        }
        for scope in &self.scopes {
            if scope.len() > 64 || scope.chars().any(|c| c.is_control() || c.is_whitespace()) {
                return Err(HostingError::invalid("Algún permiso OAuth no es válido."));
            }
        }
        Ok(())
    }
}

/// Código de dispositivo ya emitido. Se pide uno por intento de inicio de sesión.
pub struct DeviceCode {
    /// Código que el servidor espera en el sondeo (secreto).
    pub device_code: Secret,
    /// Código corto que el usuario teclea en la web (`ABCD-1234`).
    pub user_code: String,
    /// Página donde se introduce el `user_code`.
    pub verification_uri: String,
    /// Caducidad del código.
    pub expires_in: Duration,
    /// Intervalo mínimo entre sondeos.
    pub interval: Duration,
}

impl fmt::Debug for DeviceCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceCode")
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .field("device_code", &"***")
            .finish()
    }
}

/// Resultado de un sondeo.
pub enum DevicePoll {
    /// El usuario aún no ha autorizado (`authorization_pending`).
    Pending,
    /// Hay que esperar más entre sondeos (`slow_down`).
    SlowDown,
    /// Autorizado: token de acceso, en memoria.
    Authorized(Secret),
    /// El código caducó: hay que pedir otro.
    Expired,
    /// El usuario denegó el acceso.
    Denied,
}

impl fmt::Debug for DevicePoll {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DevicePoll::Pending => f.write_str("Pending"),
            DevicePoll::SlowDown => f.write_str("SlowDown"),
            DevicePoll::Authorized(_) => f.write_str("Authorized(***)"),
            DevicePoll::Expired => f.write_str("Expired"),
            DevicePoll::Denied => f.write_str("Denied"),
        }
    }
}

/// Respuesta común de los dos endpoints (`error` y `error_description` como en la RFC 8628 /
/// la documentación de GitHub y GitLab).
#[derive(Debug, Deserialize)]
struct Reply {
    device_code: Option<String>,
    user_code: Option<String>,
    verification_uri: Option<String>,
    expires_in: Option<u64>,
    interval: Option<u64>,
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

impl Reply {
    fn error_message(&self, fallback: &str) -> String {
        let code = self.error.as_deref().unwrap_or(fallback);
        let detail = self
            .error_description
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty());
        match detail {
            Some(d) => format!(
                "El servidor rechazó el inicio de sesión ({code}: {}).",
                short(d)
            ),
            None => format!("El servidor rechazó el inicio de sesión ({code})."),
        }
    }
}

/// Texto de error del servidor recortado (nunca lleva el token: los errores de este flujo no lo
/// incluyen, y aun así se limita a 200 caracteres).
fn short(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(200).collect()
}

/// Cliente del flujo device code.
#[derive(Debug, Clone)]
pub struct DeviceFlow {
    http: reqwest::Client,
    cfg: DeviceFlowConfig,
}

impl DeviceFlow {
    /// Construye el cliente HTTP (TLS del sistema, 30 s, sin redirecciones).
    pub fn new(cfg: DeviceFlowConfig) -> HostingResult<Self> {
        cfg.validate()?;
        let http = crate::net::apply_system_networking(
            reqwest::Client::builder()
                .user_agent(concat!("Ariadna/", env!("CARGO_PKG_VERSION")))
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none()),
        )?
        .build()
        .map_err(|_| HostingError::network("No se pudo preparar la conexión segura."))?;
        Ok(Self { http, cfg })
    }

    /// Con un cliente ya construido (pruebas contra un servidor local en `http://`).
    pub fn with_client(http: reqwest::Client, cfg: DeviceFlowConfig) -> HostingResult<Self> {
        cfg.validate()?;
        Ok(Self { http, cfg })
    }

    pub fn config(&self) -> &DeviceFlowConfig {
        &self.cfg
    }

    /// Paso 1: pide el código que el usuario teclea en la web.
    pub async fn request(&self) -> HostingResult<DeviceCode> {
        let form = [
            ("client_id", self.cfg.client_id.trim()),
            ("scope", &self.cfg.scope_string()),
        ];
        let reply = self.post(&self.cfg.endpoints.device_code, &form).await?;
        if let Some(err) = reply.error.as_deref() {
            let code = err.to_ascii_lowercase();
            return Err(match code.as_str() {
                "device_flow_disabled" | "unauthorized_client" | "invalid_client" => {
                    HostingError::policy(
                        "La aplicación OAuth no admite el flujo de código de dispositivo. Actívalo en su configuración (ver docs/oauth-cli.md).",
                    )
                }
                _ => HostingError::policy(reply.error_message("device_code")),
            });
        }
        let device_code = reply
            .device_code
            .filter(|c| !c.trim().is_empty())
            .ok_or_else(|| {
                HostingError::bad("El servidor no devolvió un código de dispositivo válido.")
            })?;
        let user_code = reply
            .user_code
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty())
            .ok_or_else(|| {
                HostingError::bad("El servidor no devolvió el código para el usuario.")
            })?;
        let verification_uri = reply
            .verification_uri
            .map(|u| u.trim().to_owned())
            .filter(|u| u.starts_with("https://") || u.starts_with("http://127.0.0.1"))
            .ok_or_else(|| {
                HostingError::bad("El servidor devolvió una dirección de verificación no segura.")
            })?;
        Ok(DeviceCode {
            device_code: Secret::new(device_code),
            user_code,
            verification_uri,
            expires_in: clamp_duration(reply.expires_in, MAX_EXPIRES, Duration::from_secs(900)),
            interval: clamp_duration(reply.interval, MAX_INTERVAL, Duration::from_secs(5)),
        })
    }

    /// Paso 3: un sondeo. El llamador repite cada `interval` (más [`SLOWDOWN_STEP`] si sale
    /// [`DevicePoll::SlowDown`]) hasta que llegue un token, caduque el código o se deniegue.
    pub async fn poll(&self, code: &DeviceCode) -> HostingResult<DevicePoll> {
        let form = [
            ("client_id", self.cfg.client_id.trim()),
            ("device_code", code.device_code.expose()),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ];
        let reply = self.post(&self.cfg.endpoints.token, &form).await?;
        match reply
            .error
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("authorization_pending") => return Ok(DevicePoll::Pending),
            Some("slow_down") => return Ok(DevicePoll::SlowDown),
            Some("expired_token") => return Ok(DevicePoll::Expired),
            Some("access_denied") => return Ok(DevicePoll::Denied),
            Some("incorrect_device_code") | Some("invalid_grant") => {
                return Ok(DevicePoll::Expired)
            }
            Some(other) => return Err(HostingError::policy(reply.error_message(other))),
            None => {}
        }
        let token = reply
            .access_token
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| {
                HostingError::bad("El servidor no devolvió el token de acceso esperado.")
            })?;
        Ok(DevicePoll::Authorized(Secret::new(token.trim())))
    }

    /// `POST` con formulario y `Accept: application/json`, con tope de tamaño.
    async fn post(&self, url: &str, form: &[(&str, &str)]) -> HostingResult<Reply> {
        // `x-www-form-urlencoded` a mano: el cliente de este crate no compila la feature `form`
        // de reqwest (no hace falta para nada más).
        let body = {
            let mut enc = url::form_urlencoded::Serializer::new(String::new());
            for (k, v) in form {
                enc.append_pair(k, v);
            }
            enc.finish()
        };
        let resp = self
            .http
            .post(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    HostingError::network(format!(
                        "El servidor de {} no respondió a tiempo.",
                        self.cfg.provider.label()
                    ))
                } else {
                    HostingError::network(format!(
                        "No se pudo conectar con el servidor de {}.",
                        self.cfg.provider.label()
                    ))
                }
            })?;
        let status = resp.status();
        let body = resp
            .bytes()
            .await
            .map_err(|_| HostingError::network("No se pudo leer la respuesta del servidor."))?;
        if body.len() > 64 * 1024 {
            return Err(HostingError::bad(
                "La respuesta del servidor es demasiado grande.",
            ));
        }
        let reply: Reply = serde_json::from_slice(&body).map_err(|_| {
            // Nunca se repite el cuerpo: en un error podría venir el token.
            HostingError::bad(format!(
                "El servidor de {} respondió con un formato inesperado ({}).",
                self.cfg.provider.label(),
                status.as_u16()
            ))
        })?;
        if !status.is_success() && reply.error.is_none() {
            return Err(HostingError::network(format!(
                "El servidor de {} respondió {}.",
                self.cfg.provider.label(),
                status.as_u16()
            )));
        }
        Ok(reply)
    }
}

fn clamp_duration(value: Option<u64>, max: Duration, default: Duration) -> Duration {
    let secs = value
        .unwrap_or(default.as_secs())
        .max(MIN_INTERVAL.as_secs());
    Duration::from_secs(secs.min(max.as_secs()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn los_endpoints_y_permisos_por_defecto_son_los_documentados() {
        let gh = OAuthProvider::GitHub;
        assert_eq!(gh.default_scopes(), ["repo"]);
        assert_eq!(
            gh.endpoints().device_code,
            "https://github.com/login/device/code"
        );
        assert_eq!(
            gh.endpoints().token,
            "https://github.com/login/oauth/access_token"
        );
        let gl = OAuthProvider::GitLab;
        assert_eq!(gl.default_scopes(), ["read_repository", "write_repository"]);
        assert_eq!(
            gl.endpoints().device_code,
            "https://gitlab.com/oauth/authorize_device"
        );
        assert_eq!(gl.endpoints().token, "https://gitlab.com/oauth/token");
    }

    #[test]
    fn sin_client_id_el_flujo_no_se_construye() {
        let err = DeviceFlow::new(DeviceFlowConfig::new(OAuthProvider::GitHub, "   ")).unwrap_err();
        assert_eq!(err.code(), "POLICY");
        assert!(format!("{err}").contains("client_id"));
        assert!(DeviceFlow::new(DeviceFlowConfig::new(OAuthProvider::GitHub, "abc-123")).is_ok());
    }

    #[test]
    fn los_intervalos_se_recortan_y_tienen_minimo() {
        assert_eq!(
            clamp_duration(Some(0), MAX_INTERVAL, Duration::from_secs(5)),
            MIN_INTERVAL
        );
        assert_eq!(
            clamp_duration(Some(9999), MAX_INTERVAL, Duration::from_secs(5)),
            MAX_INTERVAL
        );
        assert_eq!(
            clamp_duration(None, MAX_INTERVAL, Duration::from_secs(5)),
            Duration::from_secs(5)
        );
    }

    #[test]
    fn debug_no_muestra_el_device_code_ni_el_token() {
        let code = DeviceCode {
            device_code: Secret::new("dev-SECRETO-123"),
            user_code: "ABCD-1234".into(),
            verification_uri: "https://github.com/login/device".into(),
            expires_in: Duration::from_secs(900),
            interval: Duration::from_secs(5),
        };
        let authorized = DevicePoll::Authorized(Secret::new("tok-SECRETO"));
        let text = format!("{code:?} {authorized:?}");
        assert!(!text.contains("SECRETO"), "{text}");
        assert!(text.contains("ABCD-1234"));
    }
}
