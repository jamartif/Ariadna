//! Tipos que cruzan el IPC y no existen en `ariadna-git` (spec `specs/rust/ipc.md`).
//! Todo en camelCase. Los modelos de dominio se reenvían tal cual.

use std::sync::Arc;

use ariadna_git::{
    Branch, ConflictFile, DiffHunk, DiffLine, GitStatus, IntegrationState, RecentRepository,
    Remote, SessionEvent, SessionEventKind, SessionState, Snapshot,
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

/// Eventos de la app hacia la UI. La capa Tauri los traduce a `snapshot`, `tab-updated`,
/// `session-event` y `tabs-changed`.
#[derive(Debug, Clone)]
pub enum AppEvent {
    Snapshot(SnapshotEvent),
    TabUpdated(TabInfo),
    Session(ApiSessionEvent),
    TabsChanged(AppState),
}

/// Destino de los eventos (Tauri en producción, un colector en los tests).
pub trait EventSink: Send + Sync {
    fn emit(&self, event: AppEvent);
}
