//! Errores del motor git. El texto es para la UI (español); el `stderr` original de
//! `git` se conserva tal cual en `CommandFailed`.

/// Fallo de una operación git. `Clone + PartialEq` para poder enviarlo por IPC y compararlo en tests.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GitError {
    #[error("No hay un repositorio Git en {path}.")]
    NotARepository { path: String },
    #[error("No encuentro el ejecutable de git ({0}). Instálalo o indica la ruta en Ajustes.")]
    GitNotFound(String),
    #[error("{}", command_failed_text(.command, *.exit_code, .stderr))]
    CommandFailed {
        command: String,
        exit_code: i32,
        stderr: String,
    },
    #[error("No hay cambios preparados para el commit.")]
    NothingToCommit,
    #[error("El mensaje de commit está vacío.")]
    EmptyCommitMessage,
    /// HEAD detached: el commit no se hace hasta que el usuario lo confirme.
    #[error(
        "HEAD no está en una rama. El commit quedaría suelto. Confírmalo o crea una rama antes."
    )]
    DetachedHeadCommit,
    #[error("No corrijo el último commit. {reason}")]
    CannotAmend { reason: String },
    #[error("Hay cambios locales que impedirían el cambio de rama. Haz commit, descártalos o guárdalos antes.{}", detail_suffix(.detail))]
    LocalChangesBlockCheckout { detail: String },
    #[error("La rama local «{local}» no coincide con {remote}. No cambio de rama hasta que lo confirmes.")]
    DivergedLocalBranch { local: String, remote: String },
    #[error("La referencia «{0}» no es válida.")]
    InvalidReference(String),
    #[error("Ese hunk ya no está en {path}. Actualiza el diff e inténtalo de nuevo.")]
    HunkNotFound { path: String },
    #[error("No hay un merge, rebase o cherry-pick en curso.")]
    NoIntegrationInProgress,
    #[error("Hace falta confirmación explícita para {action}.")]
    ConfirmationRequired { action: String },
    #[error("La ruta {path} sale del repositorio. No se ha escrito nada.")]
    PathEscapesRepository { path: String },
    #[error("La operación tardó demasiado y se canceló: {command}.")]
    Timeout { command: String },
    #[error("Operación cancelada.")]
    Cancelled,
    #[error("No pude interpretar la salida de git: {0}")]
    Parse(String),
    /// Otro proceso (otra terminal, un IDE) tiene `index.lock`.
    #[error("El repositorio está bloqueado por otro proceso de git ({lock}). Si no hay ninguno en marcha, borra ese archivo a mano.")]
    Locked { lock: String },
    /// `safe.directory`: el propietario de la carpeta no coincide con el usuario.
    #[error("Git no se fía de la carpeta {path} (propietario distinto). Confírmalo para añadirla a las carpetas seguras.")]
    DubiousOwnership { path: String },
    #[error("Error de entrada/salida: {0}")]
    Io(String),
}

fn command_failed_text(command: &str, exit_code: i32, stderr: &str) -> String {
    let detail = stderr.trim();
    if detail.is_empty() {
        format!("git terminó con código {exit_code} al ejecutar {command}.")
    } else {
        format!(
            "git terminó con código {exit_code}.\n{}",
            truncate(detail, 4_000)
        )
    }
}

fn detail_suffix(detail: &str) -> String {
    let d = detail.trim();
    if d.is_empty() {
        String::new()
    } else {
        format!("\n{d}")
    }
}

/// Recorta por caracteres (nunca por bytes) y añade `…`.
pub fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut s: String = text.chars().take(limit).collect();
    s.push('…');
    s
}

impl From<std::io::Error> for GitError {
    fn from(e: std::io::Error) -> Self {
        GitError::Io(e.to_string())
    }
}

/// Resultado estándar del motor.
pub type GitResult<T> = Result<T, GitError>;
