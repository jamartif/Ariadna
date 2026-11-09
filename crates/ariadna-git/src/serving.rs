//! Contrato del motor git. La app (sesiones, UI) y los dobles de test dependen de este
//! trait, no de `GitCliEngine`. Equivale a `GitServing.swift`.

use std::path::Path;

use async_trait::async_trait;

use crate::error::GitResult;
use crate::models::{
    Branch, Commit, ConflictFile, DiffHunk, DiffScope, FileDiff, GitPath, GitStatus,
    IntegrationState, MergeFastForward, MergeResult, Remote, Repository,
};

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
    async fn mark_resolved(&self, repo: &Repository, path: &GitPath) -> GitResult<()>;
    /// Descarta cambios del árbol de trabajo (`git restore`). Destructivo.
    async fn restore_worktree(
        &self,
        repo: &Repository,
        paths: &[GitPath],
        confirmation: &Confirmation,
    ) -> GitResult<()>;
    async fn list_remotes(&self, repo: &Repository) -> GitResult<Vec<Remote>>;
    async fn add_remote(&self, repo: &Repository, name: &str, url: &str) -> GitResult<()>;
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
