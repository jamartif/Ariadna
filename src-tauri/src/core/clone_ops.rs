//! Clonar desde URL y credenciales (`specs/rust/ipc.md`, D20).
//!
//! # El secreto no se guarda
//!
//! El clonado admite tres métodos y **ninguno guarda nada**:
//!
//! * **PAT**: `username` y `secret` **opcionales**. El secreto entra por IPC como [`Secret`] (sin
//!   `Clone`/`Serialize`, `Debug` redactado, se borra de memoria al soltarse).
//! * **Sesión de `gh`/`glab`**: la UI manda solo el método (`"gh"`/`"glab"`); Rust pide el token
//!   al CLI en ese momento ([`CliSession`]) y **el token no cruza el IPC**.
//! * **OAuth device code**: la UI autoriza en el navegador y recibe un `flowId` opaco; el token
//!   vive en memoria del núcleo y se consume al clonar (`take_oauth_token`).
//!
//! En los tres casos el token llega a git **solo** por `GIT_ASKPASS` temporal + variables de
//! entorno del proceso hijo (ver `ariadna_git::askpass`), nunca en `argv`, en la URL, en
//! `.git/config`, en un registro ni en un helper de credenciales, y todo texto de error se limpia
//! antes de volver a la UI. Guardarlo en el gestor del sistema es un acto distinto y explícito
//! (`store_credential`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use ariadna_git::{
    CloneAuth, ClonePhase, CloneProgress, CloneRequest, CloneService, CredentialStore,
    GitCliEngine, GitError, Secret,
};
use ariadna_hosting::{CliSession, CliTool, Provider, RemoteInfo};
use ariadna_process::CancellationToken;

use super::api::{AppEvent, CloneProgressEvent, CredentialTest, OpenResult};
use super::app::AppCore;
use super::error::{ApiError, ApiResult};

/// Los ids de clonado viven en su propio rango para no chocar con los de `session-event`.
const CLONE_OP_BASE: u64 = 1 << 40;
static NEXT_CLONE_OP: AtomicU64 = AtomicU64::new(CLONE_OP_BASE);

fn registry() -> &'static Mutex<HashMap<u64, CancellationToken>> {
    static REG: OnceLock<Mutex<HashMap<u64, CancellationToken>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registra un clonado en curso y lo da de baja al soltarlo.
struct CloneRegistration {
    id: u64,
}

impl CloneRegistration {
    fn new(token: CancellationToken) -> Self {
        let id = NEXT_CLONE_OP.fetch_add(1, Ordering::Relaxed);
        registry()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, token);
        Self { id }
    }
}

impl Drop for CloneRegistration {
    fn drop(&mut self) {
        registry()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

/// Cancela un clonado en curso. `false` si no existe (ya terminó o no es de clonado).
pub(crate) fn cancel_clone(operation_id: u64) -> bool {
    let token = registry()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&operation_id)
        .cloned();
    match token {
        Some(t) => {
            t.cancel();
            true
        }
        None => false,
    }
}

/// Texto de la fase para la UI (español).
fn phase_label(p: ClonePhase) -> &'static str {
    match p {
        ClonePhase::Starting => "Preparando el clonado",
        ClonePhase::Counting => "Contando objetos",
        ClonePhase::Compressing => "Comprimiendo objetos",
        ClonePhase::Receiving => "Recibiendo objetos",
        ClonePhase::Resolving => "Resolviendo deltas",
        ClonePhase::CheckingOut => "Extrayendo archivos",
    }
}

/// Método de autenticación del clonado (lo elige el usuario en el diálogo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloneMethod {
    /// Token de acceso personal escrito en el formulario.
    Pat,
    /// Sesión ya iniciada en GitHub CLI.
    Gh,
    /// Sesión ya iniciada en GitLab CLI.
    Glab,
    /// Token obtenido con el flujo OAuth device code (por `flowId`).
    OAuth,
}

impl CloneMethod {
    /// `""`/`"pat"` → PAT; `"gh"`, `"glab"`, `"oauth"`. Cualquier otra cosa es un error.
    pub fn parse(raw: &str) -> ApiResult<Self> {
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "" | "pat" => CloneMethod::Pat,
            "gh" => CloneMethod::Gh,
            "glab" => CloneMethod::Glab,
            "oauth" => CloneMethod::OAuth,
            other => {
                return Err(ApiError::new(
                    "INVALID_REFERENCE",
                    format!("Método de autenticación desconocido «{other}»."),
                    None,
                ))
            }
        })
    }

    /// CLI que usa este método, si es de sesión.
    fn cli(self) -> Option<CliTool> {
        match self {
            CloneMethod::Gh => Some(CliTool::Gh),
            CloneMethod::Glab => Some(CliTool::Glab),
            CloneMethod::Pat | CloneMethod::OAuth => None,
        }
    }
}

/// Lo que manda la UI: método + (solo para PAT) usuario y token + (solo OAuth) `flowId`.
#[derive(Debug)]
pub struct CloneAuthArg {
    pub method: CloneMethod,
    pub username: Option<String>,
    /// Token del PAT. Nunca se guarda; se borra al soltarlo.
    pub secret: Option<Secret>,
    /// Identificador opaco del flujo OAuth ya autorizado.
    pub flow_id: Option<String>,
}

impl CloneAuthArg {
    /// PAT con usuario y token.
    pub fn pat(username: Option<String>, secret: Secret) -> Self {
        Self {
            method: CloneMethod::Pat,
            username,
            secret: Some(secret),
            flow_id: None,
        }
    }

    /// Sesión del CLI indicado.
    pub fn cli(method: CloneMethod) -> Self {
        Self {
            method,
            username: None,
            secret: None,
            flow_id: None,
        }
    }

    /// Token OAuth de un flujo ya autorizado.
    pub fn oauth(flow_id: impl Into<String>) -> Self {
        Self {
            method: CloneMethod::OAuth,
            username: None,
            secret: None,
            flow_id: Some(flow_id.into()),
        }
    }
}

impl AppCore {
    /// Resuelve el método elegido a una credencial de un solo uso.
    ///
    /// Comprueba que el método encaja con el proveedor de la URL: un token de GitLab no se envía
    /// nunca a GitHub (ni al revés) aunque la UI se equivoque.
    async fn resolve_clone_auth(
        &self,
        url: &str,
        arg: Option<CloneAuthArg>,
    ) -> ApiResult<Option<CloneAuth>> {
        let Some(arg) = arg else {
            return Ok(None);
        };
        let info = RemoteInfo::parse(url);
        let https = url.trim().to_ascii_lowercase().starts_with("https://");
        if !https && arg.method != CloneMethod::Pat {
            return Err(ApiError::new(
                "INVALID_REFERENCE",
                "Las credenciales solo se usan con direcciones HTTPS.",
                None,
            ));
        }
        let auth = match arg.method {
            CloneMethod::Pat => {
                let secret = arg.secret.filter(|s| !s.is_empty()).ok_or_else(|| {
                    ApiError::new(
                        "INVALID_REFERENCE",
                        "Escribe el token de acceso (PAT) o elige otro método.",
                        None,
                    )
                })?;
                CloneAuth::new(arg.username.as_deref().unwrap_or(""), secret)
                    .map_err(ApiError::from)?
            }
            CloneMethod::Gh | CloneMethod::Glab => {
                let tool = arg.method.cli().unwrap_or(CliTool::Gh);
                let expected = match tool {
                    CliTool::Gh => Provider::GitHub,
                    CliTool::Glab => Provider::GitLab,
                };
                if info.provider != expected {
                    return Err(ApiError::new(
                        "POLICY",
                        format!(
                            "La sesión de {} no sirve para este repositorio ({}). Elige otro método.",
                            tool.label(),
                            info.provider.label()
                        ),
                        None,
                    ));
                }
                let token = CliSession::new(tool)
                    .token()
                    .await
                    .map_err(ApiError::from)?;
                CloneAuth::new(tool.default_username(), Secret::new(token.expose()))
                    .map_err(ApiError::from)?
            }
            CloneMethod::OAuth => {
                let flow_id = arg
                    .flow_id
                    .map(|f| f.trim().to_owned())
                    .filter(|f| !f.is_empty())
                    .ok_or_else(|| {
                        ApiError::new(
                            "AUTH",
                            "Falta la autorización OAuth: inicia sesión con el navegador.",
                            None,
                        )
                    })?;
                let token = self.take_oauth_token(&flow_id)?;
                CloneAuth::new("oauth2", Secret::new(token.expose())).map_err(ApiError::from)?
            }
        };
        Ok(Some(auth))
    }

    /// Clona `url` en `destination` y abre el repositorio en una pestaña nueva. Emite
    /// `clone-progress` y se cancela con `cancel_operation` (id del primer evento).
    ///
    /// `auth` elige el método (PAT, sesión de CLI u OAuth); los secretos que no son el PAT no
    /// pasan por aquí: se piden a su fuente en este momento.
    pub async fn clone_repository(
        &self,
        url: String,
        destination: String,
        branch: Option<String>,
        auth: Option<CloneAuthArg>,
    ) -> ApiResult<OpenResult> {
        let runner = self.runner()?;
        // La credencial se resuelve antes de crear nada: si falta la sesión, no se toca el disco.
        let auth = self.resolve_clone_auth(&url, auth).await?;

        let token = CancellationToken::new();
        let reg = CloneRegistration::new(token.clone());
        let operation_id = reg.id;

        let req = CloneRequest {
            url,
            destination: PathBuf::from(destination.trim()),
            branch: branch
                .map(|b| b.trim().to_owned())
                .filter(|b| !b.is_empty()),
        };
        let core = self.clone();
        let progress = move |p: CloneProgress| {
            core.emit(AppEvent::CloneProgress(CloneProgressEvent {
                operation_id,
                phase: phase_label(p.phase).to_owned(),
                percent: p.percent,
            }));
        };

        let engine = GitCliEngine::new(runner);
        let repo = match auth {
            Some(a) => {
                engine
                    .clone_repository_with_auth(req, a, progress, token)
                    .await
            }
            None => engine.clone_repository(req, progress, token).await,
        }
        .map_err(ApiError::from)?;
        drop(reg);
        let path = dunce::simplified(&repo.path).to_string_lossy().into_owned();
        self.open_repository(&path).await
    }

    /// Entrega el token a `git credential approve` (el gestor del sistema decide si lo guarda).
    /// Acción explícita del usuario; **no** forma parte del clonado.
    pub async fn store_credential(
        &self,
        url: &str,
        username: &str,
        secret: Secret,
    ) -> ApiResult<()> {
        let engine = GitCliEngine::new(self.runner()?);
        engine
            .store_credential(url, username, secret)
            .await
            .map_err(ApiError::from)
    }

    /// `git ls-remote` sin interacción y **sin abrir** el repositorio: ¿hay acceso con lo que ya
    /// conoce git (`auth: None`) o con el método elegido en el diálogo?
    pub async fn test_credential(
        &self,
        url: &str,
        auth: Option<CloneAuthArg>,
    ) -> ApiResult<CredentialTest> {
        let engine = GitCliEngine::new(self.runner()?);
        let resolved = self.resolve_clone_auth(url, auth).await?;
        let res = match resolved {
            Some(a) => engine.test_credential_with_auth(url, a).await,
            None => engine.test_credential(url).await,
        };
        Ok(match res {
            Ok(()) => CredentialTest {
                ok: true,
                code: None,
            },
            Err(GitError::Cancelled) => return Err(ApiError::from(GitError::Cancelled)),
            Err(e) => CredentialTest {
                ok: false,
                code: Some(ApiError::from(e).code),
            },
        })
    }

    pub async fn forget_credential(&self, url: &str, username: &str) -> ApiResult<()> {
        let engine = GitCliEngine::new(self.runner()?);
        engine
            .forget_credential(url, username)
            .await
            .map_err(ApiError::from)
    }
}
