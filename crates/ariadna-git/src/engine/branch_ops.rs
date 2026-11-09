//! Ramas, remotos, merge, integración (merge/rebase/cherry-pick) y conflictos.
//!
//! Todos los métodos son inherentes a [`GitCliEngine`] y tienen la misma firma que
//! [`GitServing`](crate::GitServing). Decisiones de diseño (spec §4.1): no se interpretan
//! mensajes de git; se decide por código de salida, comparando commits antes/después y
//! mirando los archivos del git dir (`rev-parse --git-path`, que respeta worktrees).

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use ariadna_process::ProcessOutput;

use super::GitCliEngine;
use crate::conflict::conflict_hunks;
use crate::error::{truncate, GitError, GitResult};
use crate::listings::{parse_branches, parse_remotes};
use crate::models::{
    Branch, ConflictFile, ConflictKind, ConflictSide, GitPath, IntegrationState, MergeFastForward,
    MergeResult, Remote, Repository,
};
use crate::path_safety::{
    git_pathspec_args, validate_ref, validate_repo_path_components, validate_revision_arg,
};
use crate::runner::{redact_credentials, CommandKind};
use crate::serving::Confirmation;

// ---------------------------------------------------------------------------
// Utilidades privadas
// ---------------------------------------------------------------------------

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

fn check_ref(name: &str) -> GitResult<()> {
    validate_ref(name).map_err(|_| GitError::InvalidReference(name.to_owned()))
}

fn check_rev(name: &str) -> GitResult<()> {
    validate_revision_arg(name).map_err(|_| GitError::InvalidReference(name.to_owned()))
}

/// Error `CommandFailed` a partir de una salida con código ≠ 0 (sin interpretar el texto;
/// solo se redactan credenciales). Respeta un `index.lock` ajeno como [`GitError::Locked`].
fn command_error(repo: &Repository, subcommand: &str, out: &ProcessOutput) -> GitError {
    let lock = repo.path.join(".git").join("index.lock");
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

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8_000).any(|b| *b == 0)
}

/// Divide una salida `-z` en campos no vacíos.
fn nul_fields(bytes: &[u8]) -> Vec<Vec<u8>> {
    bytes
        .split(|b| *b == 0)
        .filter(|f| !f.is_empty())
        .map(<[u8]>::to_vec)
        .collect()
}

impl GitCliEngine {
    /// Ejecuta git y exige código 0.
    async fn bo_ok(
        &self,
        repo: &Repository,
        kind: CommandKind,
        args: &[&str],
    ) -> GitResult<ProcessOutput> {
        self.runner.run_ok_str(&repo.path, kind, args).await
    }

    /// Ejecuta git y devuelve la salida sea cual sea el código.
    async fn bo_raw(
        &self,
        repo: &Repository,
        kind: CommandKind,
        args: &[&str],
    ) -> GitResult<ProcessOutput> {
        self.runner.run(&repo.path, kind, &os(args), None).await
    }

    /// `rev-parse --verify --quiet`: `Some(sha)` si la revisión existe.
    async fn bo_rev(&self, repo: &Repository, spec: &str) -> GitResult<Option<String>> {
        let out = self
            .bo_raw(
                repo,
                CommandKind::Read,
                &["rev-parse", "--verify", "--quiet", spec],
            )
            .await?;
        Ok(out
            .success()
            .then(|| out.stdout_lossy().trim().to_owned())
            .filter(|s| !s.is_empty()))
    }

    async fn bo_ref_exists(&self, repo: &Repository, full_ref: &str) -> GitResult<bool> {
        let out = self
            .bo_raw(
                repo,
                CommandKind::Read,
                &["show-ref", "--verify", "--quiet", full_ref],
            )
            .await?;
        Ok(out.success())
    }

    /// Rutas (absolutas) de archivos del git dir, correctas también en worktrees.
    async fn bo_git_paths(&self, repo: &Repository, names: &[&str]) -> GitResult<Vec<PathBuf>> {
        let mut args = vec!["rev-parse"];
        for n in names {
            args.push("--git-path");
            args.push(n);
        }
        let out = self.bo_ok(repo, CommandKind::Read, &args).await?;
        let text = out.stdout_lossy();
        Ok(text
            .lines()
            .map(|l| repo.path.join(l.trim_end_matches('\r')))
            .collect())
    }

    /// Rutas con entradas sin resolver en el índice (sin duplicados, en orden).
    async fn bo_unmerged(&self, repo: &Repository) -> GitResult<Vec<GitPath>> {
        Ok(self
            .bo_unmerged_entries(repo)
            .await?
            .into_iter()
            .map(|(p, _)| p)
            .collect())
    }

    /// `ls-files -u -z` agrupado por ruta: `[base, ours, theirs]` como oid.
    async fn bo_unmerged_entries(
        &self,
        repo: &Repository,
    ) -> GitResult<Vec<(GitPath, [Option<String>; 3])>> {
        let out = self
            .bo_ok(repo, CommandKind::Read, &["ls-files", "-u", "-z"])
            .await?;
        let mut result: Vec<(GitPath, [Option<String>; 3])> = Vec::new();
        for entry in nul_fields(&out.stdout) {
            // `<modo> <oid> <etapa>\t<ruta>`
            let Some(tab) = entry.iter().position(|b| *b == b'\t') else {
                continue;
            };
            let meta = String::from_utf8_lossy(&entry[..tab]).into_owned();
            let path = GitPath::new(entry[tab + 1..].to_vec());
            let mut parts = meta.split(' ');
            let (_mode, oid, stage) = (parts.next(), parts.next(), parts.next());
            let (Some(oid), Some(stage)) = (oid, stage) else {
                continue;
            };
            let slot = match stage {
                "1" => 0,
                "2" => 1,
                "3" => 2,
                _ => continue,
            };
            if let Some((_, sides)) = result.iter_mut().find(|(p, _)| *p == path) {
                sides[slot] = Some(oid.to_owned());
            } else {
                let mut sides: [Option<String>; 3] = [None, None, None];
                sides[slot] = Some(oid.to_owned());
                result.push((path, sides));
            }
        }
        Ok(result)
    }

    /// Falla con `LocalChangesBlockCheckout` si algún archivo con cambios locales también
    /// difiere entre HEAD y `target`. No depende del texto de git.
    async fn bo_guard_local_changes(&self, repo: &Repository, target: &str) -> GitResult<()> {
        let Some(head) = self.bo_rev(repo, "HEAD").await? else {
            return Ok(());
        };
        if head == target {
            return Ok(());
        }
        let names = |out: ProcessOutput| -> HashSet<Vec<u8>> {
            nul_fields(&out.stdout).into_iter().collect()
        };
        let modified = names(
            self.bo_ok(
                repo,
                CommandKind::Read,
                &["diff", "--name-only", "-z", "--no-renames", "HEAD", "--"],
            )
            .await?,
        );
        let mut blocking: Vec<Vec<u8>> = Vec::new();
        let differing = nul_fields(
            &self
                .bo_ok(
                    repo,
                    CommandKind::Read,
                    &[
                        "diff",
                        "--name-only",
                        "-z",
                        "--no-renames",
                        "HEAD",
                        target,
                        "--",
                    ],
                )
                .await?
                .stdout,
        );
        for path in &differing {
            if modified.contains(path) {
                blocking.push(path.clone());
            }
        }
        // Archivos sin seguimiento que el destino traería.
        let untracked = names(
            self.bo_ok(
                repo,
                CommandKind::Read,
                &["ls-files", "--others", "--exclude-standard", "-z"],
            )
            .await?,
        );
        if !untracked.is_empty() {
            let added = nul_fields(
                &self
                    .bo_ok(
                        repo,
                        CommandKind::Read,
                        &[
                            "diff",
                            "--name-only",
                            "-z",
                            "--no-renames",
                            "--diff-filter=A",
                            "HEAD",
                            target,
                            "--",
                        ],
                    )
                    .await?
                    .stdout,
            );
            for path in added {
                if untracked.contains(&path) && !blocking.contains(&path) {
                    blocking.push(path);
                }
            }
        }
        if blocking.is_empty() {
            return Ok(());
        }
        let detail = blocking
            .iter()
            .map(|p| format!("- {}", String::from_utf8_lossy(p)))
            .collect::<Vec<_>>()
            .join("\n");
        Err(GitError::LocalChangesBlockCheckout { detail })
    }

    // -----------------------------------------------------------------------
    // Ramas
    // -----------------------------------------------------------------------

    async fn bo_list_refs(&self, repo: &Repository, namespace: &str) -> GitResult<Vec<Branch>> {
        let format = "--format=%(refname:lstrip=2)%00%(objectname)%00%(HEAD)%00%(upstream:short)\
                      %00%(upstream:track)%00%(symref)";
        let out = self
            .bo_ok(
                repo,
                CommandKind::Read,
                &["for-each-ref", format, namespace],
            )
            .await?;
        // Se descartan las referencias simbólicas (`origin/HEAD`) por su campo `symref`.
        let mut kept: Vec<u8> = Vec::new();
        for line in out.stdout.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
            let symref_empty = line
                .rsplit(|b| *b == 0)
                .next()
                .is_some_and(<[u8]>::is_empty);
            if symref_empty {
                kept.extend_from_slice(line);
                kept.push(b'\n');
            }
        }
        Ok(parse_branches(&kept))
    }

    /// Ramas locales.
    pub async fn branches(&self, repo: &Repository) -> GitResult<Vec<Branch>> {
        self.bo_list_refs(repo, "refs/heads").await
    }

    /// Ramas de seguimiento (`origin/main`). No incluye `origin/HEAD`.
    pub async fn remote_tracking_branches(&self, repo: &Repository) -> GitResult<Vec<Branch>> {
        self.bo_list_refs(repo, "refs/remotes").await
    }

    pub async fn create_branch(
        &self,
        repo: &Repository,
        name: &str,
        start_point: Option<&str>,
    ) -> GitResult<()> {
        check_ref(name)?;
        let mut args = vec!["branch", name];
        if let Some(start) = start_point {
            check_rev(start)?;
            args.push(start);
        }
        self.bo_ok(repo, CommandKind::Mutate, &args).await?;
        Ok(())
    }

    pub async fn rename_branch(&self, repo: &Repository, old: &str, new: &str) -> GitResult<()> {
        check_ref(old)?;
        check_ref(new)?;
        self.bo_ok(repo, CommandKind::Mutate, &["branch", "-m", old, new])
            .await?;
        Ok(())
    }

    /// Cambia de rama o deja HEAD en un commit/tag (detached). Ver el contrato de
    /// `DivergedLocalBranch` y `LocalChangesBlockCheckout` en el spec.
    pub async fn checkout(
        &self,
        repo: &Repository,
        reference: &str,
        allow_diverged_local: bool,
    ) -> GitResult<()> {
        check_rev(reference)?;
        let local_exists = self
            .bo_ref_exists(repo, &format!("refs/heads/{reference}"))
            .await?;
        let remote_exists = !local_exists
            && self
                .bo_ref_exists(repo, &format!("refs/remotes/{reference}"))
                .await?;

        if remote_exists {
            if let Some((_remote, local)) = reference.split_once('/') {
                check_ref(local)?;
                return self
                    .bo_checkout_remote(repo, reference, local, allow_diverged_local)
                    .await;
            }
        }

        if let Some(target) = self
            .bo_rev(repo, &format!("{reference}^{{commit}}"))
            .await?
        {
            self.bo_guard_local_changes(repo, &target).await?;
        }
        self.bo_ok(repo, CommandKind::Mutate, &["checkout", reference, "--"])
            .await?;
        Ok(())
    }

    async fn bo_checkout_remote(
        &self,
        repo: &Repository,
        remote_ref: &str,
        local: &str,
        allow_diverged_local: bool,
    ) -> GitResult<()> {
        let local_full = format!("refs/heads/{local}");
        let remote_sha = self
            .bo_rev(repo, &format!("{remote_ref}^{{commit}}"))
            .await?;
        if self.bo_ref_exists(repo, &local_full).await? {
            let local_sha = self.bo_rev(repo, &format!("{local}^{{commit}}")).await?;
            let upstream = self
                .bo_ok(
                    repo,
                    CommandKind::Read,
                    &["for-each-ref", "--format=%(upstream:short)", &local_full],
                )
                .await?
                .stdout_lossy()
                .trim()
                .to_owned();
            let follows_other = !upstream.is_empty() && upstream != remote_ref;
            if (local_sha != remote_sha || follows_other) && !allow_diverged_local {
                return Err(GitError::DivergedLocalBranch {
                    local: local.to_owned(),
                    remote: remote_ref.to_owned(),
                });
            }
            if let Some(target) = &local_sha {
                self.bo_guard_local_changes(repo, target).await?;
            }
            self.bo_ok(repo, CommandKind::Mutate, &["checkout", local, "--"])
                .await?;
            return Ok(());
        }
        if let Some(target) = &remote_sha {
            self.bo_guard_local_changes(repo, target).await?;
        }
        self.bo_ok(
            repo,
            CommandKind::Mutate,
            &["checkout", "-b", local, "--track", remote_ref],
        )
        .await?;
        Ok(())
    }

    /// Borra una rama. Sin `force` usa `-d` (git se niega si no está fusionada).
    pub async fn delete_branch(&self, repo: &Repository, name: &str, force: bool) -> GitResult<()> {
        check_ref(name)?;
        let flag = if force { "-D" } else { "-d" };
        self.bo_ok(repo, CommandKind::Mutate, &["branch", flag, name])
            .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Merge e integración
    // -----------------------------------------------------------------------

    pub async fn merge(
        &self,
        repo: &Repository,
        branch: &str,
        fast_forward: MergeFastForward,
    ) -> GitResult<MergeResult> {
        check_rev(branch)?;
        let before = self.bo_rev(repo, "HEAD").await?;
        let tip = self.bo_rev(repo, &format!("{branch}^{{commit}}")).await?;
        let ff_flag = match fast_forward {
            MergeFastForward::Allow => "--ff",
            MergeFastForward::Only => "--ff-only",
            MergeFastForward::Never => "--no-ff",
        };
        let out = self
            .bo_raw(
                repo,
                CommandKind::Mutate,
                &["merge", "--no-edit", ff_flag, branch],
            )
            .await?;
        if !out.success() {
            return self.bo_conflicts_or_error(repo, "merge", &out).await;
        }
        let after = self.bo_rev(repo, "HEAD").await?;
        match (before, after) {
            (b, Some(a)) if b.as_deref() == Some(a.as_str()) => Ok(MergeResult::UpToDate),
            (_, Some(a)) if tip.as_deref() == Some(a.as_str()) => {
                Ok(MergeResult::FastForward { new_head: a })
            }
            (_, Some(a)) => Ok(MergeResult::Merged { new_head: a }),
            (_, None) => Err(GitError::Parse("HEAD no resuelve tras el merge".into())),
        }
    }

    /// Un comando de integración falló: si hay una integración en curso con rutas sin
    /// resolver es un conflicto (no un error); si no, se devuelve el error de git.
    async fn bo_conflicts_or_error(
        &self,
        repo: &Repository,
        subcommand: &str,
        out: &ProcessOutput,
    ) -> GitResult<MergeResult> {
        if self.integration_state(repo).await? != IntegrationState::None {
            let paths = self.bo_unmerged(repo).await?;
            if !paths.is_empty() {
                return Ok(MergeResult::Conflicts { paths });
            }
        }
        Err(command_error(repo, subcommand, out))
    }

    pub async fn integration_state(&self, repo: &Repository) -> GitResult<IntegrationState> {
        let paths = self
            .bo_git_paths(
                repo,
                &[
                    "rebase-merge",
                    "rebase-apply",
                    "MERGE_HEAD",
                    "CHERRY_PICK_HEAD",
                    "sequencer",
                ],
            )
            .await?;
        let [rebase_merge, rebase_apply, merge_head, cherry_head, sequencer] = &paths[..] else {
            return Err(GitError::Parse(
                "rev-parse --git-path devolvió menos rutas".into(),
            ));
        };
        // `rebase-apply/applying` indica `git am`, que no es una integración de Ariadna.
        let applying = rebase_apply.join("applying").exists();
        if rebase_merge.is_dir() || (rebase_apply.is_dir() && !applying) {
            Ok(IntegrationState::Rebasing)
        } else if merge_head.exists() {
            Ok(IntegrationState::Merging)
        } else if cherry_head.exists() || sequencer.is_dir() {
            Ok(IntegrationState::CherryPicking)
        } else {
            Ok(IntegrationState::None)
        }
    }

    pub async fn abort_integration(&self, repo: &Repository) -> GitResult<()> {
        let sub = match self.integration_state(repo).await? {
            IntegrationState::None => return Err(GitError::NoIntegrationInProgress),
            IntegrationState::Merging => "merge",
            IntegrationState::Rebasing => "rebase",
            IntegrationState::CherryPicking => "cherry-pick",
        };
        self.bo_ok(repo, CommandKind::Mutate, &[sub, "--abort"])
            .await?;
        Ok(())
    }

    /// Continúa la integración en curso. En merge hace el commit (sin editor). En rebase y
    /// cherry-pick, si el siguiente paso vuelve a chocar, devuelve `Ok`: el llamador ve el
    /// conflicto con `integration_state` + `conflicts`.
    pub async fn continue_integration(&self, repo: &Repository) -> GitResult<()> {
        let state = self.integration_state(repo).await?;
        let args: &[&str] = match state {
            IntegrationState::None => return Err(GitError::NoIntegrationInProgress),
            IntegrationState::Merging => &["commit", "--no-edit"],
            IntegrationState::Rebasing => &["rebase", "--continue"],
            IntegrationState::CherryPicking => &["cherry-pick", "--continue"],
        };
        let out = self.bo_raw(repo, CommandKind::Mutate, args).await?;
        if out.success() {
            return Ok(());
        }
        if state != IntegrationState::Merging
            && self.integration_state(repo).await? == state
            && !self.bo_unmerged(repo).await?.is_empty()
        {
            return Ok(());
        }
        Err(command_error(repo, args[0], &out))
    }

    // -----------------------------------------------------------------------
    // Conflictos
    // -----------------------------------------------------------------------

    async fn bo_blob(&self, repo: &Repository, oid: &str) -> GitResult<Vec<u8>> {
        Ok(self
            .bo_ok(repo, CommandKind::Read, &["cat-file", "blob", oid])
            .await?
            .stdout)
    }

    async fn bo_side(&self, repo: &Repository, oid: Option<&String>) -> GitResult<ConflictSide> {
        let Some(oid) = oid else {
            return Ok(ConflictSide::absent());
        };
        let content = self.bo_blob(repo, oid).await?;
        Ok(ConflictSide {
            oid: Some(oid.clone()),
            is_binary: is_binary(&content),
            content: Some(content),
        })
    }

    /// Archivos en conflicto con sus tres lados (etapas 1/2/3 del índice) y el archivo de
    /// trabajo con marcadores. Los binarios traen sus lados pero ningún hunk.
    pub async fn conflicts(&self, repo: &Repository) -> GitResult<Vec<ConflictFile>> {
        let mut files = Vec::new();
        for (path, [base, ours, theirs]) in self.bo_unmerged_entries(repo).await? {
            let kind = match (base.is_some(), ours.is_some(), theirs.is_some()) {
                (true, true, true) => ConflictKind::BothModified,
                (false, true, true) => ConflictKind::BothAdded,
                (true, false, false) => ConflictKind::BothDeleted,
                (true, true, false) => ConflictKind::DeletedByThem,
                (true, false, true) => ConflictKind::DeletedByUs,
                (false, true, false) => ConflictKind::AddedByUs,
                (false, false, true) => ConflictKind::AddedByThem,
                (false, false, false) => ConflictKind::Unknown,
            };
            let base = self.bo_side(repo, base.as_ref()).await?;
            let ours = self.bo_side(repo, ours.as_ref()).await?;
            let theirs = self.bo_side(repo, theirs.as_ref()).await?;
            let working_tree = read_working_file(repo.path.clone(), path.clone()).await;
            let binary = base.is_binary
                || ours.is_binary
                || theirs.is_binary
                || working_tree.as_deref().is_some_and(is_binary);
            let hunks = match (&working_tree, binary) {
                (Some(wt), false) => conflict_hunks(wt),
                _ => Vec::new(),
            };
            files.push(ConflictFile {
                path,
                kind,
                ours,
                base,
                theirs,
                working_tree,
                hunks,
            });
        }
        Ok(files)
    }

    /// Escribe `contents` tal cual (bytes) en el árbol de trabajo, de forma atómica
    /// (temporal + rename en el mismo directorio). No hace `git add`. La
    /// [`Confirmation`] es la prueba de que la capa de comandos obtuvo permiso.
    pub async fn write_working_tree_file(
        &self,
        repo: &Repository,
        relative_path: &GitPath,
        contents: &[u8],
        _confirmation: &Confirmation,
    ) -> GitResult<()> {
        let root = repo.path.clone();
        let rel = relative_path.clone();
        let data = contents.to_vec();
        tokio::task::spawn_blocking(move || write_atomic(&root, &rel, &data))
            .await
            .map_err(|e| GitError::Io(e.to_string()))?
    }

    /// Borra un archivo del árbol de trabajo (resolver «borrado vs. modificado» quedándose con
    /// el lado borrado). No sigue enlaces fuera del repo y no falla si ya no existe. No hace
    /// `git add`. La [`Confirmation`] es la prueba de que la capa de comandos obtuvo permiso.
    pub async fn remove_working_tree_file(
        &self,
        repo: &Repository,
        relative_path: &GitPath,
        _confirmation: &Confirmation,
    ) -> GitResult<()> {
        let root = repo.path.clone();
        let rel = relative_path.clone();
        tokio::task::spawn_blocking(move || remove_checked(&root, &rel))
            .await
            .map_err(|e| GitError::Io(e.to_string()))?
    }

    /// `git add -- <ruta>`.
    pub async fn mark_resolved(&self, repo: &Repository, path: &GitPath) -> GitResult<()> {
        let mut args = os(&["add"]);
        args.extend(git_pathspec_args([path.as_bytes()]).map_err(|_| {
            GitError::PathEscapesRepository {
                path: path.to_string_lossy().into_owned(),
            }
        })?);
        self.runner
            .run_ok(&repo.path, CommandKind::Mutate, &args, None)
            .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Remotos
    // -----------------------------------------------------------------------

    pub async fn list_remotes(&self, repo: &Repository) -> GitResult<Vec<Remote>> {
        let out = self
            .bo_ok(repo, CommandKind::Read, &["remote", "-v"])
            .await?;
        // Las URLs con credenciales embebidas nunca salen del motor.
        Ok(parse_remotes(&out.stdout)
            .into_iter()
            .map(|r| Remote {
                name: r.name,
                fetch_url: redact_credentials(&r.fetch_url),
                push_url: redact_credentials(&r.push_url),
            })
            .collect())
    }

    pub async fn add_remote(&self, repo: &Repository, name: &str, url: &str) -> GitResult<()> {
        check_ref(name)?;
        let bad_url = url.is_empty()
            || url.starts_with('-')
            || url.chars().any(|c| (c as u32) < 0x20 || c == '\u{7f}');
        if bad_url {
            return Err(GitError::InvalidReference(redact_credentials(url)));
        }
        self.bo_ok(
            repo,
            CommandKind::Mutate,
            &["remote", "add", "--", name, url],
        )
        .await?;
        Ok(())
    }

    pub async fn fetch(&self, repo: &Repository, remote: &str) -> GitResult<()> {
        check_ref(remote)?;
        self.bo_ok(repo, CommandKind::Network, &["fetch", remote])
            .await?;
        Ok(())
    }

    /// `pull` con merge (`--no-rebase`) o rebase. Un conflicto no es un error: devuelve
    /// `Conflicts` y deja la integración en curso.
    pub async fn pull(
        &self,
        repo: &Repository,
        remote: &str,
        refspec: Option<&str>,
        rebase: bool,
    ) -> GitResult<MergeResult> {
        check_ref(remote)?;
        let mut args = vec![
            "pull",
            "--no-edit",
            if rebase { "--rebase" } else { "--no-rebase" },
        ];
        args.push(remote);
        if let Some(spec) = refspec {
            check_rev(spec)?;
            args.push(spec);
        }
        let before = self.bo_rev(repo, "HEAD").await?;
        let out = self.bo_raw(repo, CommandKind::Network, &args).await?;
        if !out.success() {
            // Fallo de credenciales (única excepción documentada en `auth`): antes que conflictos.
            let os_args: Vec<OsString> = args.iter().map(OsString::from).collect();
            if let Some(e) = self.runner.auth_error(&repo.path, &os_args, &out).await {
                return Err(e);
            }
            return self.bo_conflicts_or_error(repo, "pull", &out).await;
        }
        let Some(after) = self.bo_rev(repo, "HEAD").await? else {
            return Err(GitError::Parse("HEAD no resuelve tras el pull".into()));
        };
        let Some(before) = before else {
            return Ok(MergeResult::FastForward { new_head: after });
        };
        if before == after {
            return Ok(MergeResult::UpToDate);
        }
        let fast_forward = if rebase {
            let anc = self
                .bo_raw(
                    repo,
                    CommandKind::Read,
                    &["merge-base", "--is-ancestor", &before, &after],
                )
                .await?;
            anc.success()
        } else {
            let fetched = self.bo_rev(repo, "FETCH_HEAD^{commit}").await?;
            fetched.as_deref() == Some(after.as_str())
        };
        Ok(if fast_forward {
            MergeResult::FastForward { new_head: after }
        } else {
            MergeResult::Merged { new_head: after }
        })
    }

    /// `push`. Con `set_upstream` y sin `refspec` empuja la rama actual.
    pub async fn push(
        &self,
        repo: &Repository,
        remote: &str,
        refspec: Option<&str>,
        set_upstream: bool,
    ) -> GitResult<()> {
        check_ref(remote)?;
        let mut current: Option<String> = None;
        if let Some(spec) = refspec {
            check_rev(spec)?;
        } else if set_upstream {
            let out = self
                .bo_raw(
                    repo,
                    CommandKind::Read,
                    &["symbolic-ref", "--short", "-q", "HEAD"],
                )
                .await?;
            let name = out.stdout_lossy().trim().to_owned();
            if !out.success() || name.is_empty() {
                return Err(GitError::InvalidReference("HEAD".into()));
            }
            current = Some(name);
        }
        let mut args = vec!["push"];
        if set_upstream {
            args.push("--set-upstream");
        }
        args.push(remote);
        if let Some(spec) = refspec.or(current.as_deref()) {
            args.push(spec);
        }
        self.bo_ok(repo, CommandKind::Network, &args).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Sistema de archivos (bloqueante; se llama desde `spawn_blocking`)
// ---------------------------------------------------------------------------

fn escapes(rel: &GitPath) -> GitError {
    GitError::PathEscapesRepository {
        path: rel.to_string_lossy().into_owned(),
    }
}

/// Canonicaliza el ancestro existente más cercano de `dir` (sin seguir enlaces rotos) y
/// comprueba que está dentro de `root`.
fn ensure_inside(root: &Path, dir: &Path, rel: &GitPath) -> GitResult<()> {
    let mut cur = dir;
    loop {
        match std::fs::symlink_metadata(cur) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                cur = cur.parent().ok_or_else(|| escapes(rel))?;
            }
            Err(e) => return Err(e.into()),
        }
    }
    // Un enlace roto no se puede canonicalizar: se trata como escape.
    let canon = dunce::canonicalize(cur).map_err(|_| escapes(rel))?;
    if canon.starts_with(root) {
        Ok(())
    } else {
        Err(escapes(rel))
    }
}

fn remove_checked(root: &Path, rel: &GitPath) -> GitResult<()> {
    validate_repo_path_components(rel.as_bytes()).map_err(|_| escapes(rel))?;
    let root = dunce::canonicalize(root)?;
    let target = root.join(rel.to_path_buf());
    let parent = target.parent().ok_or_else(|| escapes(rel))?.to_path_buf();
    ensure_inside(&root, &parent, rel)?;
    match std::fs::symlink_metadata(&target) {
        Ok(m) if m.is_dir() => Err(GitError::Io(format!(
            "{} es un directorio",
            rel.to_string_lossy()
        ))),
        Ok(_) => Ok(std::fs::remove_file(&target)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn write_atomic(root: &Path, rel: &GitPath, contents: &[u8]) -> GitResult<()> {
    validate_repo_path_components(rel.as_bytes()).map_err(|_| escapes(rel))?;
    let root = dunce::canonicalize(root)?;
    let target = root.join(rel.to_path_buf());
    let parent = target.parent().ok_or_else(|| escapes(rel))?.to_path_buf();
    ensure_inside(&root, &parent, rel)?;
    std::fs::create_dir_all(&parent)?;
    ensure_inside(&root, &parent, rel)?;

    let existing = std::fs::symlink_metadata(&target).ok();
    if existing.as_ref().is_some_and(|m| m.is_dir()) {
        return Err(GitError::Io(format!(
            "{} es un directorio",
            rel.to_string_lossy()
        )));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let tmp = parent.join(format!(".ariadna-write-{}-{nanos}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        // Conserva los permisos (p. ej. el bit ejecutable) de un archivo regular existente.
        if let Some(m) = existing.as_ref().filter(|m| m.is_file()) {
            std::fs::set_permissions(&tmp, m.permissions())?;
        }
        // `rename` reemplaza el destino; si era un enlace simbólico, reemplaza el enlace
        // y nunca escribe en lo que apunta.
        std::fs::rename(&tmp, &target)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

/// Lee el archivo de trabajo si es un archivo regular dentro del repo (no sigue enlaces).
async fn read_working_file(root: PathBuf, rel: GitPath) -> Option<Vec<u8>> {
    tokio::task::spawn_blocking(move || {
        validate_repo_path_components(rel.as_bytes()).ok()?;
        let root = dunce::canonicalize(root).ok()?;
        let target = root.join(rel.to_path_buf());
        let parent = dunce::canonicalize(target.parent()?).ok()?;
        if !parent.starts_with(&root) {
            return None;
        }
        let meta = std::fs::symlink_metadata(&target).ok()?;
        if !meta.is_file() {
            return None;
        }
        std::fs::read(&target).ok()
    })
    .await
    .ok()
    .flatten()
}
