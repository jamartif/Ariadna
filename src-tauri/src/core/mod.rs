//! Núcleo de la aplicación, **sin** dependencia de la ventana de Tauri: toda la lógica de los
//! comandos vive aquí y se prueba con git real y un `Prompter` falso.

pub mod ai;
pub mod api;
mod app;
mod clone_ops;
mod commands;
pub mod error;
pub mod external;
mod history_ops;
pub mod menu;
mod merge_ops;
pub mod prompter;
mod remote_ops;
pub mod session_file;
pub mod settings;
pub mod watch;

#[cfg(test)]
mod tests;

pub use ai::{AiSettingsLoad, AiStore, StoredAi};
pub use api::{
    AiCommitOptions, AiCommitSuggestion, AiContextPreview, AiExcludeReason, AiExcludedFile,
    AiPreviewFile, AiProfileInfo, AiProviderState, AiSettingsPatch, AiState, AmendDraft,
    ApiSessionEvent, ApiSessionEventKind, ApiSnapshot, AppEvent, AppState, CloneProgressEvent,
    Closed, CommitResult, CredentialTest, Done, EventSink, GitInfo, HunkArg, NoticeEvent,
    OpenResult, SnapshotEvent, TabInfo,
};
pub use app::{default_data_dir, locate_git, AppConfig, AppCore, CLOSE_GRACE, SOFT_TAB_LIMIT};
pub use error::{ApiError, ApiResult};
pub use external::{validate_external_url, validate_remote_web_url};
pub use merge_ops::{
    ConflictResolution, ConflictSummary, ContinueResult, HunkChoice, HunkSide, MergeStartResult,
    MergeState,
};
pub use prompter::Prompter;
pub use remote_ops::{RemoteView, WebTarget};
pub use session_file::{SavedTab, SessionData, SessionLoad, SessionStore};
pub use settings::{
    AboutInfo, CliStatus, GitDetect, GitIdentity, ProfileRule, Settings, SettingsLoadInfo,
    SettingsLoadStatus, SettingsStore, SettingsView, Theme,
};
