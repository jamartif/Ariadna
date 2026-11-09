//! Tipos que cruzan el IPC y no existen en `ariadna-git` (spec `specs/rust/ipc.md`).
//! Todo en camelCase. Los modelos de dominio se reenvían tal cual.

use std::sync::Arc;

use ariadna_ai::{CommitLanguage, CommitStyle, ProfileKind, ProviderChoice, ProviderId};
use ariadna_git::{
    Branch, ConflictFile, DiffHunk, DiffLine, GitPath, GitStatus, IntegrationState,
    RecentRepository, Remote, SessionEvent, SessionEventKind, SessionState, Snapshot,
};
use serde::{Deserialize, Serialize};

use super::error::ApiError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabInfo {
    pub tab_id: String,
    pub path: String,
    pub name: String,
    pub branch: Option<String>,
    pub detached: bool,
    pub change_count: u32,
    pub conflict_count: u32,
    pub busy: Option<String>,
    pub missing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitInfo {
    pub path: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub tabs: Vec<TabInfo>,
    pub active_tab_id: Option<String>,
    pub recents: Vec<RecentRepository>,
    pub git: Option<GitInfo>,
    pub git_problem: Option<String>,
}

/// Resultado de `open_repository`. `warning` es una ampliación del contrato (solo aparece si
/// hay algo que avisar: tope blando de pestañas o worktree de un repo ya abierto).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenResult {
    pub tab: TabInfo,
    pub already_open: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Closed {
    pub closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Done {
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitResult {
    pub done: bool,
    pub sha: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmendDraft {
    pub message: String,
    pub can_amend: bool,
    pub reason: Option<String>,
}

/// `Snapshot` serializable (el de `ariadna-git` no deriva `Serialize`). Comparte el `Arc`.
#[derive(Debug, Clone)]
pub struct ApiSnapshot(pub Arc<Snapshot>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotWire<'a> {
    revision: u64,
    status: &'a GitStatus,
    branches: &'a [Branch],
    remote_branches: &'a [Branch],
    remotes: &'a [Remote],
    integration: &'a IntegrationState,
    conflicts: &'a [ConflictFile],
}

impl Serialize for ApiSnapshot {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let v = &*self.0;
        SnapshotWire {
            revision: v.revision,
            status: &v.status,
            branches: &v.branches,
            remote_branches: &v.remote_branches,
            remotes: &v.remotes,
            integration: &v.integration,
            conflicts: &v.conflicts,
        }
        .serialize(s)
    }
}

/// Carga del evento `snapshot`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotEvent {
    pub tab_id: String,
    pub snapshot: ApiSnapshot,
}

/// Evento de sesión en el IPC: `{ sessionId, revision, kind: { type, ... } }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiSessionEvent {
    pub session_id: String,
    pub revision: u64,
    pub kind: ApiSessionEventKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ApiSessionEventKind {
    #[serde(rename_all = "camelCase")]
    StateChanged { state: String },
    #[serde(rename_all = "camelCase")]
    OperationStarted { operation_id: u64, label: String },
    #[serde(rename_all = "camelCase")]
    OperationRetrying {
        operation_id: u64,
        label: String,
        attempt: u32,
        delay_ms: u64,
    },
    #[serde(rename_all = "camelCase")]
    OperationFinished {
        operation_id: u64,
        label: String,
        ok: bool,
        error: Option<ApiError>,
    },
    #[serde(rename_all = "camelCase")]
    SnapshotPublished { revision: u64 },
    #[serde(rename_all = "camelCase")]
    RefreshFailed { error: ApiError },
}

fn state_name(s: SessionState) -> &'static str {
    match s {
        SessionState::Opening => "opening",
        SessionState::Active => "active",
        SessionState::Closing => "closing",
        SessionState::Closed => "closed",
    }
}

impl From<&SessionEvent> for ApiSessionEvent {
    fn from(e: &SessionEvent) -> Self {
        let kind = match &e.kind {
            SessionEventKind::StateChanged { state } => ApiSessionEventKind::StateChanged {
                state: state_name(*state).to_owned(),
            },
            SessionEventKind::OperationStarted {
                operation_id,
                label,
            } => ApiSessionEventKind::OperationStarted {
                operation_id: operation_id.get(),
                label: label.clone(),
            },
            SessionEventKind::OperationRetrying {
                operation_id,
                label,
                attempt,
                delay,
            } => ApiSessionEventKind::OperationRetrying {
                operation_id: operation_id.get(),
                label: label.clone(),
                attempt: *attempt,
                delay_ms: u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
            },
            SessionEventKind::OperationFinished {
                operation_id,
                label,
                result,
            } => ApiSessionEventKind::OperationFinished {
                operation_id: operation_id.get(),
                label: label.clone(),
                ok: result.is_ok(),
                error: result.as_ref().err().map(|e| ApiError::from(e.clone())),
            },
            SessionEventKind::SnapshotPublished { revision } => {
                ApiSessionEventKind::SnapshotPublished {
                    revision: *revision,
                }
            }
            SessionEventKind::RefreshFailed { error } => ApiSessionEventKind::RefreshFailed {
                error: ApiError::from(error.clone()),
            },
        };
        Self {
            session_id: e.session_id.clone(),
            revision: e.revision,
            kind,
        }
    }
}

/// Hunk tal y como lo devuelve `diff` y lo reenvía la UI. `DiffHunk` de `ariadna-git` no es
/// `Deserialize` (sus bytes crudos no viajan); el motor localiza el hunk por sus rangos.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HunkArg {
    #[serde(default)]
    pub id: String,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    #[serde(default)]
    pub heading: String,
    #[serde(default)]
    pub lines: Vec<DiffLine>,
}

impl From<HunkArg> for DiffHunk {
    fn from(h: HunkArg) -> Self {
        DiffHunk {
            id: h.id,
            old_start: h.old_start,
            old_count: h.old_count,
            new_start: h.new_start,
            new_count: h.new_count,
            heading: h.heading,
            lines: h.lines,
            raw: Vec::new(),
        }
    }
}

/// Evento `clone-progress`: `phase` ya viene en español; `percent` es `None` si git no da porcentaje.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneProgressEvent {
    pub operation_id: u64,
    pub phase: String,
    pub percent: Option<u8>,
}

/// Resultado de `test_credential`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialTest {
    pub ok: bool,
    pub code: Option<String>,
}

/// Eventos de la app hacia la UI. La capa Tauri los traduce a `snapshot`, `tab-updated`,
/// `session-event`, `tabs-changed` y `clone-progress`.
#[derive(Debug, Clone)]
pub enum AppEvent {
    CloneProgress(CloneProgressEvent),
    Snapshot(SnapshotEvent),
    TabUpdated(TabInfo),
    Session(ApiSessionEvent),
    TabsChanged(AppState),
}

/// Destino de los eventos (Tauri en producción, un colector en los tests).
pub trait EventSink: Send + Sync {
    fn emit(&self, event: AppEvent);
}

// --- IA (specs/rust/ipc.md «Comandos de IA») ----------------------------------------------

/// Perfil activo de IA.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProfileInfo {
    pub name: String,
    pub kind: ProfileKind,
}

/// Estado de un proveedor para Ajustes y para el botón de IA.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderState {
    pub id: ProviderId,
    pub display_name: String,
    /// `available` | `missing` | `failed`.
    pub availability: String,
    pub version: Option<String>,
    pub hint: Option<String>,
    pub enabled: bool,
    pub consented: bool,
    /// Comando que hay que ejecutar para iniciar sesión (`claude`, `codex login`…).
    pub login_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiState {
    pub enabled: bool,
    pub profile: AiProfileInfo,
    pub choice: ProviderChoice,
    pub providers: Vec<AiProviderState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEnabledPatch {
    pub id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryPathPatch {
    pub id: String,
    pub path: Option<String>,
}

/// Cambios de ajustes de IA; todo opcional. Los textos se validan en el núcleo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiSettingsPatch {
    pub enabled: Option<bool>,
    pub choice: Option<String>,
    pub profile: Option<String>,
    pub provider_enabled: Option<ProviderEnabledPatch>,
    pub binary_path: Option<BinaryPathPatch>,
}

/// Opciones de `ai_generate_commit_message`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiCommitOptions {
    pub template: Option<String>,
    pub language: CommitLanguage,
    pub style: CommitStyle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPreviewFile {
    pub path: GitPath,
    pub bytes: usize,
}

/// Por qué un archivo no va al contexto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiExcludeReason {
    Sensitive,
    Binary,
    TooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiExcludedFile {
    pub path: GitPath,
    pub reason: AiExcludeReason,
}

/// Lo que se enviaría al proveedor, sin enviarlo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiContextPreview {
    pub provider: Option<String>,
    pub bytes: usize,
    pub max_bytes: usize,
    pub files: Vec<AiPreviewFile>,
    pub excluded: Vec<AiExcludedFile>,
    pub redactions: usize,
    pub recent_subjects: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCommitSuggestion {
    pub subject: String,
    pub body: String,
    /// Id del proveedor que respondió (`claude`, `codex`, `copilot`).
    pub provider: String,
    /// Resumen del comando lanzado; nunca incluye el prompt.
    pub command_summary: String,
    pub repo_revision: u64,
}
