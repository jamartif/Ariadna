//! Clonar desde URL y credenciales (`specs/rust/ipc.md`, D20).
//!
//! # El PAT no se guarda
//!
//! `clone_repository` recibe `username` y `secret` **opcionales**. El secreto entra por IPC como
//! [`Secret`] (sin `Clone`/`Serialize`, `Debug` redactado, se borra de memoria al soltarse), se
//! entrega a git **solo** por `GIT_ASKPASS` temporal + variables de entorno del proceso hijo (ver
//! `ariadna_git::askpass`), nunca va en `argv`, en la URL, en `.git/config`, en un registro ni en un
//! helper de credenciales, y todo texto de error se limpia antes de volver a la UI. Guardarlo en el
//! gestor del sistema es un acto distinto y explícito (`store_credential`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use ariadna_git::{
    CloneAuth, ClonePhase, CloneProgress, CloneRequest, CloneService, CredentialStore,
    GitCliEngine, GitError, Secret,
};
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

impl AppCore {
    /// Clona `url` en `destination` y abre el repositorio en una pestaña nueva. Emite
    /// `clone-progress` y se cancela con `cancel_operation` (id del primer evento).
    ///
    /// `secret` es un PAT de un solo uso (ver la documentación del módulo).
    pub async fn clone_repository(
        &self,
        url: String,
        destination: String,
        branch: Option<String>,
        username: Option<String>,
        secret: Option<Secret>,
    ) -> ApiResult<OpenResult> {
        let runner = self.runner()?;
        let token = CancellationToken::new();
        let reg = CloneRegistration::new(token.clone());
        let operation_id = reg.id;

        let auth = match secret.filter(|s| !s.is_empty()) {
            Some(s) => {
                Some(CloneAuth::new(username.as_deref().unwrap_or(""), s).map_err(ApiError::from)?)
            }
            None => None,
        };
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

    /// `git ls-remote` sin interacción: ¿hay acceso con lo que ya conoce git?
    pub async fn test_credential(&self, url: &str) -> ApiResult<CredentialTest> {
        let engine = GitCliEngine::new(self.runner()?);
        Ok(match engine.test_credential(url).await {
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
