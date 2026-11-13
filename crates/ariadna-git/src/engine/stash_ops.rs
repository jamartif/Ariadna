//! Stash: guardar, listar, aplicar, sacar, borrar y ver el diff.
//!
//! Un stash se identifica por su SHA; aquí se traduce a `stash@{n}` justo antes de usarlo, con
//! la lista leída en ese instante, para que un cambio de orden entre medias no afecte a otro
//! stash. No se interpretan mensajes de git: se decide por códigos de salida, por comparar
//! `refs/stash` antes y después, y por las entradas sin resolver del índice.

use std::ffi::OsString;

use ariadna_process::ProcessOutput;

use super::GitCliEngine;
use crate::diff::parse_diff;
use crate::error::{truncate, GitError, GitResult};
use crate::models::{FileDiff, GitPath, Repository};
use crate::runner::{redact_credentials, CommandKind};
use crate::serving::Confirmation;
use crate::stash::{parse_stash_list, StashApplyResult, StashEntry, StashPushOptions};

const LIST_FORMAT: &str = "--format=%H%x1f%gs%x1f%cI%x1e";

/// Mismas opciones que el diff de `core_ops`: salida estable y sin programas externos.
const DIFF_COMMON: [&str; 6] = [
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--unified=3",
];

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

fn failed(repo: &Repository, subcommand: &str, out: &ProcessOutput) -> GitError {
    let lock = repo.git_dir.join("index.lock");
    if lock.exists() {
        return GitError::Locked {
            lock: lock.display().to_string(),
        };
    }
    GitError::CommandFailed {
        command: format!("git {subcommand}"),
        exit_code: out.code.unwrap_or(-1),
        stderr: truncate(&redact_credentials(&out.stderr_lossy()), 8_000),
    }
}

/// Solo hexadecimal (4–64): nada que git pueda tomar por opción o por otra revisión.
fn check_oid(oid: &str) -> GitResult<()> {
    if (4..=64).contains(&oid.len()) && oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(GitError::InvalidReference(oid.to_owned()))
    }
}

impl GitCliEngine {
    async fn so_run(
        &self,
        repo: &Repository,
        kind: CommandKind,
        args: &[OsString],
    ) -> GitResult<ProcessOutput> {
        // El runner fija `GIT_LITERAL_PATHSPECS=1`; `stash -u` usa pathspecs mágicos por dentro y
        // con eso no limpia los archivos sin seguimiento. Aquí no se pasa ninguna ruta del
        // usuario, así que es seguro desactivarlo.
        self.runner
            .run_with_env(
                &repo.path,
                kind,
                args,
                None,
                &[("GIT_LITERAL_PATHSPECS", "0")],
            )
            .await
    }

    /// `refs/stash` actual, si hay alguno.
    async fn so_top(&self, repo: &Repository) -> GitResult<Option<String>> {
        let out = self
            .so_run(
                repo,
                CommandKind::Read,
                &os(&["rev-parse", "--verify", "--quiet", "refs/stash"]),
            )
            .await?;
        Ok(out
            .success()
            .then(|| out.stdout_lossy().trim().to_owned())
            .filter(|s| !s.is_empty()))
    }

    /// Entrada del stash con ese SHA (completo o prefijo único).
    async fn so_find(&self, repo: &Repository, oid: &str) -> GitResult<StashEntry> {
        check_oid(oid)?;
        let needle = oid.to_ascii_lowercase();
        let list = self.stash_list(repo).await?;
        let mut matches = list.into_iter().filter(|e| e.oid.starts_with(&needle));
        match (matches.next(), matches.next()) {
            (Some(entry), None) => Ok(entry),
            _ => Err(GitError::StashNotFound {
                oid: oid.to_owned(),
            }),
        }
    }

    async fn so_unmerged(&self, repo: &Repository) -> GitResult<Vec<GitPath>> {
        let out = self
            .so_run(
                repo,
                CommandKind::Read,
                &os(&["diff", "--name-only", "--diff-filter=U", "-z"]),
            )
            .await?;
        if !out.success() {
            return Err(failed(repo, "diff", &out));
        }
        let mut paths: Vec<GitPath> = out
            .stdout
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| GitPath::from(p.to_vec()))
            .collect();
        paths.dedup();
        Ok(paths)
    }

    pub async fn stash_list(&self, repo: &Repository) -> GitResult<Vec<StashEntry>> {
        let mut args = os(&["stash", "list", "--no-color"]);
        args.push(OsString::from(LIST_FORMAT));
        let out = self.so_run(repo, CommandKind::Read, &args).await?;
        if !out.success() {
            return Err(failed(repo, "stash", &out));
        }
        Ok(parse_stash_list(&out.stdout))
    }

    pub async fn stash_push(
        &self,
        repo: &Repository,
        options: &StashPushOptions,
    ) -> GitResult<StashEntry> {
        if options.staged_only {
            // `stash push --staged` sale con 1 si no hay nada preparado: se ve antes.
            let quiet = os(&["diff", "--cached", "--quiet", "--no-ext-diff"]);
            if self
                .so_run(repo, CommandKind::Read, &quiet)
                .await?
                .success()
            {
                return Err(GitError::NothingToStash);
            }
        }
        let before = self.so_top(repo).await?;
        let mut args = os(&["stash", "push"]);
        if options.staged_only {
            args.push(OsString::from("--staged"));
        } else if options.include_untracked {
            args.push(OsString::from("--include-untracked"));
        }
        let message = options
            .message
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty());
        if let Some(m) = message {
            if m.contains('\0') {
                return Err(GitError::InvalidReference("mensaje del stash".into()));
            }
            args.push(OsString::from("--message"));
            args.push(OsString::from(m));
        }
        let out = self.so_run(repo, CommandKind::Mutate, &args).await?;
        if !out.success() {
            return Err(failed(repo, "stash push", &out));
        }
        // Sin cambios que guardar git sale con 0 y no crea nada: se nota porque `refs/stash` no se mueve.
        let after = self.so_top(repo).await?;
        if after.is_none() || after == before {
            return Err(GitError::NothingToStash);
        }
        let list = self.stash_list(repo).await?;
        list.into_iter().next().ok_or(GitError::NothingToStash)
    }

    async fn so_apply(
        &self,
        repo: &Repository,
        oid: &str,
        pop: bool,
    ) -> GitResult<StashApplyResult> {
        let entry = self.so_find(repo, oid).await?;
        let sub = if pop { "pop" } else { "apply" };
        let reference = format!("stash@{{{}}}", entry.index);
        let mut args = os(&["stash", sub]);
        args.push(OsString::from(reference));
        let out = self.so_run(repo, CommandKind::Mutate, &args).await?;
        if out.success() {
            return Ok(StashApplyResult {
                conflicts: Vec::new(),
                dropped: pop,
            });
        }
        // Con conflictos git sale con 1, deja las entradas sin resolver y NO borra el stash.
        let conflicts = self.so_unmerged(repo).await?;
        if conflicts.is_empty() {
            return Err(failed(repo, &format!("stash {sub}"), &out));
        }
        Ok(StashApplyResult {
            conflicts,
            dropped: false,
        })
    }

    pub async fn stash_apply(&self, repo: &Repository, oid: &str) -> GitResult<StashApplyResult> {
        self.so_apply(repo, oid, false).await
    }

    pub async fn stash_pop(&self, repo: &Repository, oid: &str) -> GitResult<StashApplyResult> {
        self.so_apply(repo, oid, true).await
    }

    /// Borra el stash. La [`Confirmation`] es la puerta: sin ella no se puede llamar.
    pub async fn stash_drop(
        &self,
        repo: &Repository,
        oid: &str,
        _confirmation: &Confirmation,
    ) -> GitResult<()> {
        let entry = self.so_find(repo, oid).await?;
        let mut args = os(&["stash", "drop", "--quiet"]);
        args.push(OsString::from(format!("stash@{{{}}}", entry.index)));
        let out = self.so_run(repo, CommandKind::Mutate, &args).await?;
        if out.success() {
            Ok(())
        } else {
            Err(failed(repo, "stash drop", &out))
        }
    }

    /// Cambios del stash contra su base (`<stash>^1`) más, si los hay, los archivos sin
    /// seguimiento (tercer padre) como archivos nuevos. Todo en un solo diff, así los ids de
    /// línea no se repiten entre archivos.
    pub async fn stash_diff(&self, repo: &Repository, oid: &str) -> GitResult<Vec<FileDiff>> {
        let entry = self.so_find(repo, oid).await?;
        let full = entry.oid;
        let mut args = os(&["diff"]);
        args.extend(DIFF_COMMON.iter().map(OsString::from));
        args.push(OsString::from("--end-of-options"));
        args.push(OsString::from(format!("{full}^1")));
        args.push(OsString::from(full.clone()));
        let out = self.so_run(repo, CommandKind::Read, &args).await?;
        if !out.success() {
            return Err(failed(repo, "diff", &out));
        }
        let mut bytes = out.stdout;
        let untracked = format!("{full}^3");
        let has_untracked = self
            .so_run(
                repo,
                CommandKind::Read,
                &os(&[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{untracked}^{{commit}}"),
                ]),
            )
            .await?
            .success();
        if has_untracked {
            let mut show = os(&["show", "--format=", "--patch", "--root"]);
            show.extend(DIFF_COMMON.iter().map(OsString::from));
            show.push(OsString::from("--end-of-options"));
            show.push(OsString::from(untracked));
            let out = self.so_run(repo, CommandKind::Read, &show).await?;
            if !out.success() {
                return Err(failed(repo, "show", &out));
            }
            bytes.extend_from_slice(&out.stdout);
        }
        Ok(parse_diff(&bytes))
    }
}
