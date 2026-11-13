//! Núcleo git de Ariadna: parsers y utilidades **puras** (sin lanzar procesos git salvo el
//! localizador, que solo ejecuta `git --version`).
//!
//! Módulos:
//! * [`models`]: tipos de dominio (estado, commits, ramas, diffs, conflictos, recientes).
//! * [`path_safety`]: validación cerrada de rutas y referencias (spec §4.1/§4.2).
//! * [`porcelain`]: `git status --porcelain=v2 -z --branch`.
//! * [`diff`]: diff unificado → [`FileDiff`] y construcción de parches para `git apply --cached`.
//! * [`conflict`]: marcadores de conflicto (merge/diff3) y resolución sin E/S.
//! * [`listings`]: log, ramas y remotos.
//! * [`partial`]: parches de líneas sueltas dentro de un hunk (preparar/quitar líneas).
//! * [`stash`]: tipos y parser del stash.
//! * [`amend`]: reglas del borrador de mensaje al enmendar.
//! * [`git_locator`]: localización y validación de `git`.
//! * [`auth`], [`credentials`] y [`clone`]: error de autenticación, credenciales por PAT
//!   (credential helper de git) y clonado desde URL con progreso (spec §4.3).
//! * [`recents`]: recientes y favoritos en JSON compatible con el Swift.
//! * [`safe_directory`]: añadir **una** carpeta concreta a `safe.directory` (propietario distinto).
//!
//! * [`runner`], [`engine`], [`serving`] y [`error`]: motor git por CLI (M2/M3).

pub mod amend;
pub mod askpass;
pub mod auth;
pub mod clone;
pub mod conflict;
pub mod credentials;
pub mod cursor;
pub mod diff;
pub mod engine;
pub mod error;
pub mod git_locator;
pub mod listings;
pub mod models;
pub mod partial;
pub mod path_safety;
pub mod porcelain;
pub mod recents;
mod remote_url;
pub mod runner;
pub mod safe_directory;
pub mod serving;
pub mod session;
pub mod stash;
pub mod tag;

#[cfg(test)]
mod testutil;

pub use askpass::{scrub_error, CloneAuth};
pub use auth::{classify_auth_failure, AUTH_MARKERS};
pub use clone::{parse_progress_line, ClonePhase, CloneProgress, CloneRequest, CloneService};
pub use conflict::{
    conflict_hunks, has_conflict_markers, parse_conflicts, resolve_all, resolve_hunks,
    ConflictError, ConflictRegion, Eol, ParsedConflicts,
};
pub use credentials::{CredentialStore, FilledCredential, Secret};
pub use cursor::history_cursor;
pub use diff::{parse_diff, rebuild_diff, unquote_c_style, DiffError};
pub use engine::GitCliEngine;
pub use error::{GitError, GitResult};
pub use git_locator::{
    find_git, parse_git_version, GitInstall, GitLocateError, GitVersion, LocatorOptions, TargetOs,
    MIN_GIT_VERSION,
};
pub use listings::{parse_branches, parse_commit_log, parse_remotes};
pub use models::*;
pub use partial::LinePatchError;
pub use path_safety::{
    git_literal_arg, git_pathspec_args, pathspec_stdin, validate_ref, validate_repo_os_path,
    validate_repo_path, validate_revision_arg, PathSafetyError,
};
pub use porcelain::parse_status;
pub use recents::{LoadOutcome, RecentStore, RecentsError, MAX_RECENTS};
pub use runner::{redact_credentials, CommandKind, GitRunner};
pub use serving::{Confirmation, GitServing};
pub use session::{
    repo_key, CloseOutcome, OperationId, RefreshOutcome, RepoKey, RepositorySession, SessionConfig,
    SessionEvent, SessionEventKind, SessionState, Snapshot,
};
pub use stash::{parse_stash_list, StashApplyResult, StashEntry, StashPushOptions};
pub use tag::{parse_tag_list, tag_name_problem, Tag, MAX_TAG_NAME};
