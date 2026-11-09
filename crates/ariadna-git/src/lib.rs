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
//! * [`amend`]: reglas del borrador de mensaje al enmendar.
//! * [`git_locator`]: localización y validación de `git`.
//! * [`recents`]: recientes y favoritos en JSON compatible con el Swift.
//!
//! * [`runner`], [`engine`], [`serving`] y [`error`]: motor git por CLI (M2/M3).

pub mod amend;
pub mod conflict;
pub mod diff;
pub mod engine;
pub mod error;
pub mod git_locator;
pub mod listings;
pub mod models;
pub mod path_safety;
pub mod porcelain;
pub mod recents;
pub mod runner;
pub mod serving;
pub mod session;

#[cfg(test)]
mod testutil;

pub use conflict::{
    conflict_hunks, has_conflict_markers, parse_conflicts, resolve_all, resolve_hunks,
    ConflictError, ConflictRegion, Eol, ParsedConflicts,
};
pub use diff::{parse_diff, rebuild_diff, unquote_c_style, DiffError};
pub use engine::GitCliEngine;
pub use error::{GitError, GitResult};
pub use git_locator::{
    find_git, parse_git_version, GitInstall, GitLocateError, GitVersion, LocatorOptions, TargetOs,
    MIN_GIT_VERSION,
};
pub use listings::{parse_branches, parse_commit_log, parse_remotes};
pub use models::*;
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
