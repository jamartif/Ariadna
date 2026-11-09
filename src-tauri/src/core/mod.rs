//! Núcleo de la aplicación, **sin** dependencia de la ventana de Tauri: toda la lógica de los
//! comandos vive aquí y se prueba con git real y un `Prompter` falso.

pub mod api;
mod app;
mod commands;
pub mod error;
pub mod prompter;
pub mod session_file;

#[cfg(test)]
mod tests;

pub use api::{
    AmendDraft, ApiSessionEvent, ApiSessionEventKind, ApiSnapshot, AppEvent, AppState, Closed,
    CommitResult, Done, EventSink, GitInfo, HunkArg, OpenResult, SnapshotEvent, TabInfo,
};
pub use app::{default_data_dir, locate_git, AppConfig, AppCore, CLOSE_GRACE, SOFT_TAB_LIMIT};
pub use error::{ApiError, ApiResult};
pub use prompter::Prompter;
pub use session_file::{SavedTab, SessionData, SessionLoad, SessionStore};
