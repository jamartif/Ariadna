//! Contrato del motor git. La app (sesiones, UI) y los dobles de test dependen de este
//! trait, no de `GitCliEngine`. Equivale a `GitServing.swift`.

use std::path::Path;

use async_trait::async_trait;

use crate::error::{GitError, GitResult};
use crate::models::{
    Branch, Commit, ConflictFile, DiffHunk, DiffScope, FileDiff, GitPath, GitStatus,
    IntegrationState, MergeFastForward, MergeResult, Remote, Repository,
};
use crate::stash::{StashApplyResult, StashEntry, StashPushOptions};
use crate::tag::Tag;

fn unsupported_stash() -> GitError {
    GitError::Io("el stash no está soportado por este motor".into())
}

fn unsupported_tags() -> GitError {
    GitError::Io("las etiquetas no están soportadas por este motor".into())
}

/// Permiso explícito para una escritura que pisa trabajo del usuario.
///
/// **No tiene `Default` a propósito.** Solo debe construirla la capa de comandos (Tauri)
/// después de que el usuario acepte un diálogo nativo abierto por Rust. En el núcleo, quien
/// llama tiene que decidirlo en el sitio de la llamada. El enlace con sesión, operación y
/// revisión (token de un solo uso, spec §6) lo añade esa capa.
#[derive(Debug, Clone)]
pub struct Confirmation {
    action: String,
}

impl Confirmation {
    /// Concede el permiso para `action` (texto para el usuario, p. ej. «descartar cambios»).
    pub fn granted(action: impl Into<String>) -> Self {
        Self {
            action: action.into(),
        }
    }
    pub fn action(&self) -> &str {
        &self.action
    }
}

/// Todas las operaciones git que necesita Ariadna. Los nombres siguen `GitServing.swift`.
///
/// Reglas del contrato:
/// * Ningún método interpreta mensajes humanos de `git` salvo excepción documentada.
/// * Las rutas son `GitPath` (bytes), nunca `String`.
/// * Las mutaciones se serializan fuera de aquí (en `RepositorySession`); el motor no
///   necesita bloquear.
#[async_trait]
pub trait GitServing: Send + Sync {
    async fn open_repository(&self, path: &Path) -> GitResult<Repository>;
    async fn status(&self, repo: &Repository, include_ignored: bool) -> GitResult<GitStatus>;
    async fn stage(&self, repo: &Repository, paths: &[GitPath]) -> GitResult<()>;
    async fn unstage(&self, repo: &Repository, paths: &[GitPath]) -> GitResult<()>;
    async fn stage_hunk(&self, repo: &Repository, hunk: &DiffHunk, path: &GitPath)
        -> GitResult<()>;
    async fn unstage_hunk(
        &self,
        repo: &Repository,
        hunk: &DiffHunk,
        path: &GitPath,
    ) -> GitResult<()>;
    /// Prepara solo las líneas `lines` (índices en `hunk.lines`) del hunk. Por defecto no está
    /// soportado: lo implementa el motor real.
    async fn stage_lines(
        &self,
        _repo: &Repository,
        _hunk: &DiffHunk,
        _lines: &[u32],
        path: &GitPath,
    ) -> GitResult<()> {
        Err(GitError::Io(format!(
            "preparar líneas de {} no está soportado por este motor",
            path.to_string_lossy()
        )))
    }
    /// Quita de preparados solo las líneas `lines` del hunk preparado.
    async fn unstage_lines(
        &self,
        _repo: &Repository,
        _hunk: &DiffHunk,
        _lines: &[u32],
        path: &GitPath,
    ) -> GitResult<()> {
        Err(GitError::Io(format!(
            "quitar líneas de {} no está soportado por este motor",
            path.to_string_lossy()
        )))
    }
    /// Devuelve el SHA del commit creado.
    async fn commit(
        &self,
        repo: &Repository,
        message: &str,
        allow_detached: bool,
    ) -> GitResult<String>;
    /// `git commit --amend` con comprobaciones: nunca reescribe un commit ya publicado.
    async fn amend_last_commit(&self, repo: &Repository, message: &str) -> GitResult<String>;
    async fn log(&self, repo: &Repository, limit: usize) -> GitResult<Vec<Commit>>;
    /// Página del historial de HEAD (`skip` commits saltados, como mucho `limit`). Por defecto
    /// se apoya en [`log`](Self::log); el motor real usa `git log --skip`.
    async fn log_page(
        &self,
        repo: &Repository,
        skip: usize,
        limit: usize,
    ) -> GitResult<Vec<Commit>> {
        let all = self.log(repo, skip.saturating_add(limit)).await?;
        Ok(all.into_iter().skip(skip).collect())
    }
    /// Página del historial recorriendo desde la frontera `cursor` (ver
    /// [`history_cursor`](crate::history_cursor)): mismo orden que `log_page`, pero sin `--skip`.
    /// Por defecto no está soportado: lo implementa el motor real.
    async fn log_after(
        &self,
        _repo: &Repository,
        _cursor: &[String],
        _limit: usize,
    ) -> GitResult<Vec<Commit>> {
        Err(GitError::Io(
            "el cursor de historial no está soportado por este motor".into(),
        ))
    }
    /// Busca por mensaje, autor o hash en todo el historial de HEAD.
    async fn search_log(
        &self,
        repo: &Repository,
        query: &str,
        limit: usize,
    ) -> GitResult<Vec<Commit>>;
    async fn diff(
        &self,
        repo: &Repository,
        scope: &DiffScope,
        paths: &[GitPath],
    ) -> GitResult<Vec<FileDiff>>;
    async fn branches(&self, repo: &Repository) -> GitResult<Vec<Branch>>;
    /// Ramas de seguimiento (`origin/main`). No incluye `origin/HEAD`.
    async fn remote_tracking_branches(&self, repo: &Repository) -> GitResult<Vec<Branch>>;
    async fn create_branch(
        &self,
        repo: &Repository,
        name: &str,
        start_point: Option<&str>,
    ) -> GitResult<()>;
    async fn rename_branch(&self, repo: &Repository, old: &str, new: &str) -> GitResult<()>;
    async fn checkout(
        &self,
        repo: &Repository,
        reference: &str,
        allow_diverged_local: bool,
    ) -> GitResult<()>;
    async fn delete_branch(&self, repo: &Repository, name: &str, force: bool) -> GitResult<()>;
    async fn merge(
        &self,
        repo: &Repository,
        branch: &str,
        fast_forward: MergeFastForward,
    ) -> GitResult<MergeResult>;
    async fn integration_state(&self, repo: &Repository) -> GitResult<IntegrationState>;
    async fn abort_integration(&self, repo: &Repository) -> GitResult<()>;
    async fn continue_integration(&self, repo: &Repository) -> GitResult<()>;
    async fn conflicts(&self, repo: &Repository) -> GitResult<Vec<ConflictFile>>;
    /// Escribe el archivo de trabajo. No hace `git add`.
    async fn write_working_tree_file(
        &self,
        repo: &Repository,
        relative_path: &GitPath,
        contents: &[u8],
        confirmation: &Confirmation,
    ) -> GitResult<()>;
    /// Borra el archivo de trabajo (resolución «quedarse con el borrado»). No hace `git add`.
    /// Por defecto no está soportado: lo implementa el motor real.
    async fn remove_working_tree_file(
        &self,
        _repo: &Repository,
        relative_path: &GitPath,
        _confirmation: &Confirmation,
    ) -> GitResult<()> {
        Err(GitError::Io(format!(
            "borrar {} no está soportado por este motor",
            relative_path.to_string_lossy()
        )))
    }
    async fn mark_resolved(&self, repo: &Repository, path: &GitPath) -> GitResult<()>;
    /// Descarta cambios del árbol de trabajo (`git restore`). Destructivo.
    async fn restore_worktree(
        &self,
        repo: &Repository,
        paths: &[GitPath],
        confirmation: &Confirmation,
    ) -> GitResult<()>;
    /// Lista de stashes, del más reciente al más antiguo.
    async fn stash_list(&self, _repo: &Repository) -> GitResult<Vec<StashEntry>> {
        Err(unsupported_stash())
    }
    /// Guarda los cambios en un stash y devuelve su entrada.
    async fn stash_push(
        &self,
        _repo: &Repository,
        _options: &StashPushOptions,
    ) -> GitResult<StashEntry> {
        Err(unsupported_stash())
    }
    /// Aplica el stash `oid` sin borrarlo. Con conflictos los deja en el repo.
    async fn stash_apply(&self, _repo: &Repository, _oid: &str) -> GitResult<StashApplyResult> {
        Err(unsupported_stash())
    }
    /// Aplica el stash `oid` y lo borra si no hubo conflictos.
    async fn stash_pop(&self, _repo: &Repository, _oid: &str) -> GitResult<StashApplyResult> {
        Err(unsupported_stash())
    }
    /// Borra el stash `oid`. Destructivo: pide [`Confirmation`].
    async fn stash_drop(
        &self,
        _repo: &Repository,
        _oid: &str,
        _confirmation: &Confirmation,
    ) -> GitResult<()> {
        Err(unsupported_stash())
    }
    /// Diff del stash `oid` (cambios guardados; los archivos sin seguimiento como nuevos).
    async fn stash_diff(&self, _repo: &Repository, _oid: &str) -> GitResult<Vec<FileDiff>> {
        Err(unsupported_stash())
    }
    /// Etiquetas (ligeras y anotadas), de la más reciente a la más antigua.
    async fn tags(&self, _repo: &Repository) -> GitResult<Vec<Tag>> {
        Err(unsupported_tags())
    }
    /// Crea una etiqueta en `target` (HEAD si `None`); con `message` no vacío es anotada.
    async fn create_tag(
        &self,
        _repo: &Repository,
        _name: &str,
        _target: Option<&str>,
        _message: Option<&str>,
    ) -> GitResult<Tag> {
        Err(unsupported_tags())
    }
    /// Borra una etiqueta local. Destructivo: pide [`Confirmation`].
    async fn delete_tag(
        &self,
        _repo: &Repository,
        _name: &str,
        _confirmation: &Confirmation,
    ) -> GitResult<()> {
        Err(unsupported_tags())
    }
    /// Publica una etiqueta en `remote`.
    async fn push_tag(&self, _repo: &Repository, _remote: &str, _name: &str) -> GitResult<()> {
        Err(unsupported_tags())
    }
    /// Borra una etiqueta del remoto. Destructivo: pide [`Confirmation`].
    async fn delete_remote_tag(
        &self,
        _repo: &Repository,
        _remote: &str,
        _name: &str,
        _confirmation: &Confirmation,
    ) -> GitResult<()> {
        Err(unsupported_tags())
    }
    async fn list_remotes(&self, repo: &Repository) -> GitResult<Vec<Remote>>;
    async fn add_remote(&self, repo: &Repository, name: &str, url: &str) -> GitResult<()>;
    /// Renombra un remoto (git reescribe sus ramas de seguimiento).
    async fn rename_remote(&self, _repo: &Repository, old: &str, new: &str) -> GitResult<()> {
        Err(GitError::Io(format!(
            "renombrar el remoto {old} a {new} no está soportado por este motor"
        )))
    }
    /// Quita un remoto y sus ramas de seguimiento.
    async fn remove_remote(&self, _repo: &Repository, name: &str) -> GitResult<()> {
        Err(GitError::Io(format!(
            "quitar el remoto {name} no está soportado por este motor"
        )))
    }
    async fn fetch(&self, repo: &Repository, remote: &str) -> GitResult<()>;
    async fn pull(
        &self,
        repo: &Repository,
        remote: &str,
        refspec: Option<&str>,
        rebase: bool,
    ) -> GitResult<MergeResult>;
    async fn push(
        &self,
        repo: &Repository,
        remote: &str,
        refspec: Option<&str>,
        set_upstream: bool,
    ) -> GitResult<()>;
}
