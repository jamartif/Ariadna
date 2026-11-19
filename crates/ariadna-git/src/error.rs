//! Errores del motor git. El texto es para la UI (español); el `stderr` original de
//! `git` se conserva tal cual en `CommandFailed`.

use serde::{Deserialize, Serialize};

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
    #[error("No corrijo el último commit. {}", .reason.explanation())]
    CannotAmend { reason: AmendRefusal },
    #[error("Hay cambios locales que impedirían el cambio de rama. Haz commit, descártalos o guárdalos antes.{}", detail_suffix(.detail))]
    LocalChangesBlockCheckout { detail: String },
    #[error("La rama local «{local}» no coincide con {remote}. No cambio de rama hasta que lo confirmes.")]
    DivergedLocalBranch { local: String, remote: String },
    #[error("La referencia «{0}» no es válida.")]
    InvalidReference(String),
    #[error("Ese hunk ya no está en {path}. Actualiza el diff e inténtalo de nuevo.")]
    HunkNotFound { path: String },
    /// El parche de líneas sueltas no se pudo construir o git lo rechazó.
    #[error("No se pueden preparar esas líneas: {reason}")]
    PatchRejected { path: String, reason: String },
    /// El stash ya no existe (otro proceso lo borró o lo aplicó).
    #[error("Ese stash ya no existe. Actualiza la lista e inténtalo de nuevo.")]
    StashNotFound { oid: String },
    #[error("No hay cambios que guardar en el stash.")]
    NothingToStash,
    /// El nombre de etiqueta incumple las reglas de `git check-ref-format`.
    #[error("El nombre de etiqueta «{name}» no es válido: {reason}.")]
    InvalidTagName { name: String, reason: String },
    #[error("Ya existe una etiqueta llamada «{name}». Elige otro nombre o bórrala antes.")]
    TagExists { name: String },
    #[error("La etiqueta «{name}» ya no existe. Actualiza la lista e inténtalo de nuevo.")]
    TagNotFound { name: String },
    #[error("No hay un merge, rebase o cherry-pick en curso.")]
    NoIntegrationInProgress,
    /// La rama local y la del remoto divergieron: traerla no sería un avance rápido.
    #[error("La rama «{branch}» tiene commits que no están en el remoto: traerla no sería un avance rápido. Fusiónala o rebasa desde la rama activa.")]
    BranchNotFastForward { branch: String },
    /// Se pidió traer una rama que es la que está activa: git lo rechaza y hay otra vía.
    #[error("La rama «{branch}» es la que tienes activa: git no la puede traer así. Usa «Bajar la rama actual» o cambia de rama antes.")]
    BranchIsCurrent { branch: String },
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
    /// Fallo de autenticación contra el remoto (credenciales ausentes, caducadas o sin permiso).
    /// `host` es el servidor; `hint` explica qué hacer (token, `ssh-add`, permisos del PAT…).
    #[error("No se pudo autenticar en {host}. {hint}")]
    Auth { host: String, hint: String },
    /// `safe.directory`: el propietario de la carpeta no coincide con el usuario.
    #[error("Git no se fía de la carpeta {path} (propietario distinto). Confírmalo para añadirla a las carpetas seguras.")]
    DubiousOwnership { path: String },
    /// El servidor no tiene esa referencia: la PR se borró, o viene de un fork cuyo remoto no
    /// está configurado (Azure DevOps no publica `refs/pull/N/head` en el repositorio destino).
    #[error("El servidor no tiene la referencia «{reference}» en el remoto «{remote}». Si la pull request viene de un fork, añade el remoto del fork y vuelve a intentarlo.")]
    MissingRemoteRef { remote: String, reference: String },
    /// La ruta no vale para `safe.directory` (vacía, relativa, con controles o un `*`).
    #[error("No se puede añadir {path} a las carpetas seguras de git: {reason}.")]
    SafeDirectoryRejected { path: String, reason: String },
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

/// Por qué no se puede corregir el último commit.
///
/// Es un **valor** y no un texto: la UI traduce la variante (`amendReason.<kind>`) y los tests
/// comparan motivos en lugar de redacciones. El valor de cable es `snake_case` y estable
/// (`{"kind":"no_commits"}`, `{"kind":"already_published","value":"origin/main"}`): reescribir el
/// texto de [`AmendRefusal::explanation`] no cambia el contrato con la interfaz.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum AmendRefusal {
    /// El repositorio todavía no tiene commits.
    NoCommits,
    /// HEAD no está en una rama.
    DetachedHead,
    /// Hay un merge, rebase, cherry-pick o revert a medias.
    IntegrationInProgress,
    /// El commit ya está en una rama remota (nombre sin `refs/remotes/`).
    AlreadyPublished(String),
}

impl AmendRefusal {
    /// Texto en español para logs y para los sitios que todavía no lo traducen.
    pub fn explanation(&self) -> String {
        match self {
            AmendRefusal::NoCommits => "Todavía no hay ningún commit que corregir.".to_owned(),
            AmendRefusal::DetachedHead => {
                "HEAD no está en una rama. Crea una rama antes de corregir el commit.".to_owned()
            }
            AmendRefusal::IntegrationInProgress => {
                "Hay un merge, rebase o cherry-pick en curso. Termínalo o abórtalo antes."
                    .to_owned()
            }
            AmendRefusal::AlreadyPublished(remote_ref) => {
                format!("Ya está en {remote_ref}: corregirlo obligaría a un push forzado.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// El valor de cable es el contrato con la UI: snake_case, estable y sin el texto en español.
    #[test]
    fn cada_motivo_del_amend_tiene_su_valor_de_cable() {
        let casos = [
            (AmendRefusal::NoCommits, r#"{"kind":"no_commits"}"#),
            (AmendRefusal::DetachedHead, r#"{"kind":"detached_head"}"#),
            (
                AmendRefusal::IntegrationInProgress,
                r#"{"kind":"integration_in_progress"}"#,
            ),
            (
                AmendRefusal::AlreadyPublished("origin/main".into()),
                r#"{"kind":"already_published","value":"origin/main"}"#,
            ),
        ];
        for (motivo, esperado) in casos {
            let json = serde_json::to_string(&motivo).unwrap();
            assert_eq!(json, esperado);
            let vuelta: AmendRefusal = serde_json::from_str(&json).unwrap();
            assert_eq!(vuelta, motivo);
        }
    }

    #[test]
    fn el_texto_del_error_sale_del_motivo_y_no_se_duplica() {
        let e = GitError::CannotAmend {
            reason: AmendRefusal::AlreadyPublished("origin/main".into()),
        };
        let texto = e.to_string();
        assert!(
            texto.starts_with("No corrijo el último commit. "),
            "{texto}"
        );
        assert!(texto.contains("origin/main"), "{texto}");
        assert_eq!(
            texto,
            format!(
                "No corrijo el último commit. {}",
                AmendRefusal::AlreadyPublished("origin/main".into()).explanation()
            )
        );
    }
}
