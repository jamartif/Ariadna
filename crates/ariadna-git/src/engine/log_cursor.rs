//! Página del historial por cursor (frontera), sin `--skip`. Ver [`crate::cursor`].

use std::ffi::OsString;

use super::core_ops::{LOG_FORMAT, MAX_LOG};
use super::GitCliEngine;
use crate::error::{GitError, GitResult};
use crate::listings::parse_commit_log;
use crate::models::{Commit, Repository};
use crate::runner::CommandKind;

/// Máximo de commits en una frontera (un historial con cientos de ramas abiertas cabe de sobra).
pub const MAX_CURSOR: usize = 1_000;

/// SHA completo (SHA-1 o SHA-256): nada que git pueda tomar por opción o por una expresión.
fn check_full_oid(oid: &str) -> GitResult<()> {
    if matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(GitError::InvalidReference(oid.to_owned()))
    }
}

impl GitCliEngine {
    /// Siguiente página del historial: `limit` commits recorriendo desde la frontera `cursor`
    /// (ver [`crate::history_cursor`]). El orden es el de `git log` desde HEAD, igual que con
    /// `--skip`, pero el coste no depende de la profundidad. Cursor vacío = sin más historial.
    pub async fn log_after(
        &self,
        repo: &Repository,
        cursor: &[String],
        limit: usize,
    ) -> GitResult<Vec<Commit>> {
        if cursor.is_empty() {
            return Ok(Vec::new());
        }
        if cursor.len() > MAX_CURSOR {
            return Err(GitError::InvalidReference(format!(
                "cursor con {} commits (máximo {MAX_CURSOR})",
                cursor.len()
            )));
        }
        for oid in cursor {
            check_full_oid(oid)?;
        }
        let capped = limit.clamp(1, MAX_LOG);
        let mut args: Vec<OsString> = ["log", "-n"].iter().map(OsString::from).collect();
        args.push(OsString::from(capped.to_string()));
        args.extend(
            ["--no-color", "--no-show-signature", LOG_FORMAT]
                .iter()
                .map(OsString::from),
        );
        args.extend(cursor.iter().map(OsString::from));
        args.push(OsString::from("--"));
        let out = self
            .runner
            .run_ok(&repo.path, CommandKind::Read, &args, None)
            .await?;
        Ok(parse_commit_log(&out.stdout))
    }
}
