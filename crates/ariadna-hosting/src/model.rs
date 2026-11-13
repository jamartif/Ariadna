//! Tipos de pull request que cruzan el IPC (camelCase). Son los mismos para Azure DevOps y
//! GitHub; lo que un proveedor no tiene queda en `None`/vacío y se anuncia en
//! [`Capabilities`].

use serde::{Deserialize, Serialize};

use crate::remote::Provider;

/// Persona (autor, revisor, comentarista).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrUser {
    /// Id estable del proveedor (GUID en Azure DevOps, id numérico en GitHub).
    pub id: String,
    pub display_name: String,
    /// Nombre de usuario (`login` en GitHub, `uniqueName` en Azure DevOps).
    pub login: String,
}

/// Estado de una PR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrState {
    /// Abierta (GitHub `open`, Azure DevOps `active`).
    Active,
    /// Completada / fusionada.
    Completed,
    /// Abandonada / cerrada sin fusionar.
    Abandoned,
}

/// Filtro del listado (PR-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrFilter {
    Open,
    Mine,
    Reviewing,
    Closed,
}

/// Voto de un revisor (PR-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Vote {
    /// Azure DevOps 10 · GitHub `APPROVE`.
    Approve,
    /// Azure DevOps 5 (GitHub no lo tiene).
    ApproveWithSuggestions,
    /// Azure DevOps 0 (GitHub no puede retirar una revisión).
    NoVote,
    /// Azure DevOps -5 (GitHub no lo tiene).
    WaitForAuthor,
    /// Azure DevOps -10 · GitHub `REQUEST_CHANGES`.
    Reject,
    /// GitHub `COMMENT` (Azure DevOps no lo tiene).
    Comment,
}

impl Vote {
    /// ¿El voto bloquea la PR? Las interfaces lo confirman con diálogo nativo.
    pub fn is_rejection(self) -> bool {
        matches!(self, Vote::Reject | Vote::WaitForAuthor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reviewer {
    pub user: PrUser,
    pub vote: Vote,
    pub required: bool,
}

/// Estado de fusión según el proveedor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeStatus {
    Clean,
    Conflicts,
    Unknown,
}

/// Resultado de una política (Azure DevOps) o comprobación (GitHub).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckStatus {
    Passing,
    Failing,
    Pending,
    NotApplicable,
    Broken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyCheck {
    pub name: String,
    pub status: CheckStatus,
    /// `Some(true)` si bloquea la fusión; `None` si el proveedor no lo dice (GitHub REST).
    pub blocking: Option<bool>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecksSummary {
    pub passing: u32,
    pub failing: u32,
    pub pending: u32,
}

impl ChecksSummary {
    pub fn of(checks: &[PolicyCheck]) -> Self {
        let mut s = Self::default();
        for c in checks {
            match c.status {
                CheckStatus::Passing => s.passing += 1,
                CheckStatus::Failing | CheckStatus::Broken => s.failing += 1,
                CheckStatus::Pending => s.pending += 1,
                CheckStatus::NotApplicable => {}
            }
        }
        s
    }
}

/// Fila del listado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestSummary {
    pub id: u64,
    pub title: String,
    pub author: PrUser,
    /// Nombre corto de la rama (sin `refs/heads/`).
    pub source_branch: String,
    pub target_branch: String,
    pub state: PrState,
    pub is_draft: bool,
    /// ISO 8601 tal como lo da el proveedor.
    pub created_at: String,
    pub updated_at: Option<String>,
    /// Revisores y votos. En el listado de GitHub solo traen los solicitados (sin voto).
    pub reviewers: Vec<Reviewer>,
    /// Resumen de políticas/comprobaciones; `None` cuando el listado no lo trae (se pide en
    /// el detalle).
    pub checks: Option<ChecksSummary>,
    pub merge_status: MergeStatus,
    pub web_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemRef {
    pub id: u64,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrCommit {
    pub sha: String,
    /// Primera línea del mensaje.
    pub subject: String,
    pub author: String,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Other,
}

/// Archivo cambiado por la PR (sin contenido).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrFileChange {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
    pub binary: bool,
}

/// Parche de un archivo: texto unificado **sin** línea `diff --git` (empieza en `--- a/…`).
/// `patch` es `None` si el proveedor no lo da (binario, demasiado grande…); `omitted` dice por qué.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePatch {
    pub change: PrFileChange,
    pub patch: Option<String>,
    pub omitted: Option<String>,
}

/// Detalle (PR-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestDetail {
    #[serde(flatten)]
    pub summary: PullRequestSummary,
    /// Markdown. La interfaz lo renderiza desde el AST, **nunca** con `{@html}`.
    pub description: String,
    pub policies: Vec<PolicyCheck>,
    pub work_items: Vec<WorkItemRef>,
    pub commits: Vec<PrCommit>,
    pub files: Vec<PrFileChange>,
    /// Cabeza de la rama origen (para votar/completar sin condiciones de carrera).
    pub source_commit: Option<String>,
    pub target_commit: Option<String>,
    /// Referencia para bajar la rama: `refs/heads/x` (Azure DevOps) o `refs/pull/N/head`.
    pub checkout_ref: String,
    /// La PR viene de un fork (GitHub).
    pub is_fork: bool,
    /// Finalización automática activada (Azure DevOps).
    pub auto_complete: bool,
    /// Texto del estado de fusión del proveedor (`dirty`, `blocked`, `rejectedByPolicy`…).
    pub merge_detail: Option<String>,
}

/// Datos para crear una PR (PR-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewPullRequest {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub source_branch: String,
    pub target_branch: String,
    #[serde(default)]
    pub draft: bool,
    /// Azure DevOps: ids (GUID) o correos de revisores; GitHub: logins.
    #[serde(default)]
    pub reviewers: Vec<String>,
    /// Solo Azure DevOps.
    #[serde(default)]
    pub work_item_ids: Vec<u64>,
}

/// Página del listado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestPage {
    pub items: Vec<PullRequestSummary>,
    /// Número de página (empieza en 1).
    pub page: u32,
    pub has_more: bool,
}

/// Consulta del listado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListQuery {
    pub filter: PrFilter,
    /// Empieza en 1.
    pub page: u32,
    pub per_page: u32,
}

impl ListQuery {
    pub const DEFAULT_PER_PAGE: u32 = 30;

    pub fn new(filter: PrFilter, page: u32) -> Self {
        Self {
            filter,
            page: page.max(1),
            per_page: Self::DEFAULT_PER_PAGE,
        }
    }
}

// --- hilos de revisión (PR-06) -------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThreadStatus {
    Active,
    Resolved,
    /// Solo informativo (Azure DevOps `closed`, `wontFix`, `byDesign`…).
    Closed,
}

/// Lado del diff al que apunta un comentario en línea.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffSide {
    Old,
    New,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadAnchor {
    pub path: String,
    /// Línea (empieza en 1) en el lado indicado.
    pub line: u32,
    pub side: DiffSide,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadComment {
    pub id: String,
    pub author: PrUser,
    /// Markdown (nunca `{@html}`).
    pub body: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewThread {
    pub id: String,
    pub status: ThreadStatus,
    /// `None` = comentario general de la PR.
    pub anchor: Option<ThreadAnchor>,
    /// El comentario ya no apunta a código vigente (GitHub `isOutdated`).
    pub outdated: bool,
    pub comments: Vec<ThreadComment>,
    /// ¿Se puede resolver/reabrir desde Ariadna?
    pub can_resolve: bool,
}

/// Hilo nuevo (comentario general si `anchor` es `None`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewThread {
    pub body: String,
    #[serde(default)]
    pub anchor: Option<ThreadAnchor>,
}

// --- completar (PR-08) ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeStrategy {
    /// Merge sin fast-forward (Azure DevOps `noFastForward`, GitHub `merge`).
    Merge,
    Squash,
    Rebase,
    /// Solo Azure DevOps (`rebaseMerge`).
    RebaseMerge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteOptions {
    pub strategy: MergeStrategy,
    #[serde(default)]
    pub delete_source_branch: bool,
    /// Azure DevOps: pasar los work items vinculados a completados.
    #[serde(default)]
    pub complete_work_items: bool,
    /// Azure DevOps: completar solo cuando se cumplan las políticas.
    #[serde(default)]
    pub auto_complete: bool,
    #[serde(default)]
    pub commit_message: Option<String>,
}

/// Qué admite el proveedor (para que la interfaz muestre solo lo posible).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub provider: Provider,
    pub votes: Vec<Vote>,
    pub strategies: Vec<MergeStrategy>,
    pub auto_complete: bool,
    pub resolve_threads: bool,
    pub work_items: bool,
    pub draft: bool,
}
