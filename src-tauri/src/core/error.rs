//! `GitError` → `ApiError` (contrato `specs/rust/ipc.md`). Los códigos son estables y la UI
//! nunca interpreta `message`.

use ariadna_git::GitError;
use serde::{Deserialize, Serialize};

/// Error que recibe la UI al rechazarse la promesa de un comando.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub type ApiResult<T> = Result<T, ApiError>;

/// Todos los códigos del contrato (para tests y documentación).
pub const ALL_CODES: [&str; 21] = [
    "NOT_A_REPOSITORY",
    "GIT_NOT_FOUND",
    "COMMAND_FAILED",
    "NOTHING_TO_COMMIT",
    "EMPTY_COMMIT_MESSAGE",
    "DETACHED_HEAD_COMMIT",
    "CANNOT_AMEND",
    "LOCAL_CHANGES_BLOCK_CHECKOUT",
    "DIVERGED_LOCAL_BRANCH",
    "INVALID_REFERENCE",
    "HUNK_NOT_FOUND",
    "NO_INTEGRATION",
    "CONFIRMATION_REQUIRED",
    "PATH_ESCAPES_REPOSITORY",
    "TIMEOUT",
    "CANCELLED",
    "PARSE",
    "LOCKED",
    "DUBIOUS_OWNERSHIP",
    "AUTH",
    "IO",
    "REPO_MISSING",
];

impl ApiError {
    pub fn new(code: &str, message: impl Into<String>, detail: Option<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            detail,
        }
    }

    /// La pestaña no existe (cerrada mientras la UI aún la mostraba).
    pub fn unknown_tab(tab_id: &str) -> Self {
        Self::new(
            "REPO_MISSING",
            "Esa pestaña ya no existe.",
            Some(tab_id.to_owned()),
        )
    }

    /// La carpeta de la pestaña ya no existe o dejó de ser un repositorio.
    pub fn repo_missing(path: &str) -> Self {
        Self::new(
            "REPO_MISSING",
            format!("La carpeta {path} ya no existe o no es un repositorio. Localízala o cierra la pestaña."),
            Some(path.to_owned()),
        )
    }

    pub fn io(message: impl Into<String>) -> Self {
        let m = message.into();
        Self::new("IO", format!("Error de entrada/salida: {m}"), Some(m))
    }
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_owned())
}

impl From<GitError> for ApiError {
    fn from(e: GitError) -> Self {
        let message = e.to_string();
        // `match` exhaustivo a propósito: una variante nueva en `ariadna-git` rompe la
        // compilación aquí y obliga a darle un código.
        let (code, detail) = match &e {
            GitError::NotARepository { path } => ("NOT_A_REPOSITORY", non_empty(path)),
            GitError::GitNotFound(d) => ("GIT_NOT_FOUND", non_empty(d)),
            GitError::CommandFailed {
                command, stderr, ..
            } => (
                "COMMAND_FAILED",
                non_empty(stderr).or_else(|| non_empty(command)),
            ),
            GitError::NothingToCommit => ("NOTHING_TO_COMMIT", None),
            GitError::EmptyCommitMessage => ("EMPTY_COMMIT_MESSAGE", None),
            GitError::DetachedHeadCommit => ("DETACHED_HEAD_COMMIT", None),
            GitError::CannotAmend { reason } => ("CANNOT_AMEND", non_empty(reason)),
            GitError::LocalChangesBlockCheckout { detail } => {
                ("LOCAL_CHANGES_BLOCK_CHECKOUT", non_empty(detail))
            }
            GitError::DivergedLocalBranch { remote, .. } => {
                ("DIVERGED_LOCAL_BRANCH", non_empty(remote))
            }
            GitError::InvalidReference(r) => ("INVALID_REFERENCE", non_empty(r)),
            GitError::HunkNotFound { path } => ("HUNK_NOT_FOUND", non_empty(path)),
            GitError::NoIntegrationInProgress => ("NO_INTEGRATION", None),
            GitError::ConfirmationRequired { action } => {
                ("CONFIRMATION_REQUIRED", non_empty(action))
            }
            GitError::PathEscapesRepository { path } => {
                ("PATH_ESCAPES_REPOSITORY", non_empty(path))
            }
            GitError::Timeout { command } => ("TIMEOUT", non_empty(command)),
            GitError::Cancelled => ("CANCELLED", None),
            GitError::Parse(d) => ("PARSE", non_empty(d)),
            GitError::Locked { lock } => ("LOCKED", non_empty(lock)),
            GitError::DubiousOwnership { path } => ("DUBIOUS_OWNERSHIP", non_empty(path)),
            GitError::Auth { host, hint } => ("AUTH", non_empty(&format!("{host}: {hint}"))),
            GitError::Io(d) => ("IO", non_empty(d)),
        };
        Self {
            code: code.to_owned(),
            message,
            detail,
        }
    }
}
