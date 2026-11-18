//! Conflictos de una pull request resueltos con IA.
//!
//! Flujo, en tres pasos separados para que nada se escriba sin permiso:
//!
//! 1. [`AppCore::pr_conflicts`]: baja las referencias de la PR (`git fetch`) y **lee** el merge
//!    que git calcularía (`git merge-tree`), devolviendo los ficheros en conflicto con sus tres
//!    versiones. No toca el árbol de trabajo ni el índice.
//! 2. [`AppCore::ai_pr_conflict_options`]: pide a la IA hasta tres alternativas de resolución de
//!    un fichero. Solo propone.
//! 3. [`AppCore::pr_apply_conflicts`]: con **confirmación nativa**, deja la PR preparada en local
//!    (checkout de la rama, merge de la base, ficheros escritos y preparados). El commit y el
//!    push los hace el usuario desde la interfaz normal.

use std::path::Path;

use ariadna_git::{CommandKind, GitError, GitPath, MergeFastForward, MergeResult};

use super::api::{PrConflictApplied, PrConflictFile, PrConflictResolution, PrConflicts};
use super::app::AppCore;
use super::error::{ApiError, ApiResult};

/// Tope de ficheros en conflicto que se leen de una vez (el resto se avisa, no se oculta).
const MAX_CONFLICT_FILES: usize = 40;
/// Tope de bytes de cada versión que se manda a la IA.
const MAX_SIDE_BYTES: usize = 60_000;

impl AppCore {
    /// Ficheros en conflicto de una PR, con sus tres versiones. **Solo lectura** (salvo el
    /// `fetch`, que se hace por la cola del repositorio).
    pub async fn pr_conflicts(&self, tab_id: &str, id: u64) -> ApiResult<PrConflicts> {
        let c = self.pr_ctx(tab_id).await?;
        let detail = c.provider.detail(id).await?;
        let summary = detail.summary.clone();
        let head_ref = if detail.checkout_ref.trim().is_empty() {
            match c.info.provider {
                ariadna_hosting::Provider::AzureDevOps => {
                    format!("refs/heads/{}", summary.source_branch)
                }
                _ => format!("refs/pull/{}/head", summary.id),
            }
        } else {
            detail.checkout_ref.clone()
        };
        let base_branch = summary.target_branch.clone();
        let (remote, tab_label) = (c.remote_name.clone(), c.tab_label.clone());

        // 1) Bajar la PR (por la cola del repo, como el resto de operaciones de red).
        let (remote2, head2) = (remote.clone(), head_ref.clone());
        self.mutate_op(
            tab_id,
            "bajar las referencias de la pull request",
            (remote2, head2),
            |e, r, (remote, _head)| Box::pin(async move { e.fetch(&r, &remote).await }),
        )
        .await?;

        // 2) SHAs de los dos lados: los de la API si vienen, si no `origin/<rama>`.
        let Some(runner) = self.git_runner() else {
            return Err(ApiError::new(
                "GIT_NOT_FOUND",
                "No hay un git utilizable para leer la pull request.",
                None,
            ));
        };
        let cwd = std::path::PathBuf::from(&c.repo_path);
        let head_sha = match detail.source_commit.as_deref() {
            Some(s) if !s.trim().is_empty() => s.trim().to_owned(),
            _ => format!("refs/remotes/{remote}/{}", summary.source_branch),
        };
        let base_sha = match detail.target_commit.as_deref() {
            Some(s) if !s.trim().is_empty() => s.trim().to_owned(),
            _ => format!("refs/remotes/{remote}/{base_branch}"),
        };
        let Some(head) = rev_parse(&runner, &cwd, &head_sha).await else {
            return Err(ApiError::new(
                "NOT_FOUND",
                "No encuentro la rama de la pull request en local. Baja la PR («Bajar la PR») y vuelve a intentarlo.",
                Some(summary.source_branch.clone()),
            ));
        };
        let Some(base) = rev_parse(&runner, &cwd, &base_sha).await else {
            return Err(ApiError::new(
                "NOT_FOUND",
                format!("No encuentro la rama base «{base_branch}» en local."),
                Some(base_branch.clone()),
            ));
        };
        let ancestor = rev_parse(&runner, &cwd, &format!("{base}^{{}}"))
            .await
            .map(|_| ());
        let _ = ancestor;

        // 3) Merge de prueba: `merge-tree --write-tree` no toca índice ni árbol de trabajo.
        let merged = git_text(
            &runner,
            &cwd,
            &["merge-tree", "--write-tree", "--name-only", &base, &head],
        )
        .await;
        let Some(merged) = merged else {
            // Sin conflictos: merge-tree sale con 0 y solo imprime el árbol.
            return Ok(PrConflicts {
                number: summary.id,
                base: base_branch,
                head: summary.source_branch,
                base_sha: base,
                head_sha: head,
                files: Vec::new(),
                truncated: false,
                tab: tab_label,
            });
        };
        let mut paths: Vec<String> = merged
            .lines()
            .skip(1)
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
        let truncated = paths.len() > MAX_CONFLICT_FILES;
        paths.truncate(MAX_CONFLICT_FILES);

        // 4) Tres versiones de cada fichero (ancestro común + cada lado).
        let mb = git_text(&runner, &cwd, &["merge-base", &base, &head])
            .await
            .unwrap_or_default();
        let mut files = Vec::new();
        for path in paths {
            let ours = blob_at(&runner, &cwd, &base, &path).await;
            let theirs = blob_at(&runner, &cwd, &head, &path).await;
            let base_text = if mb.is_empty() {
                None
            } else {
                blob_at(&runner, &cwd, &mb, &path).await
            };
            let (ours, binary_ours) = split_text(ours);
            let (theirs, binary_theirs) = split_text(theirs);
            let (base_text, _) = split_text(base_text);
            files.push(PrConflictFile {
                path,
                ours,
                theirs,
                base: base_text,
                binary: binary_ours || binary_theirs,
            });
        }
        Ok(PrConflicts {
            number: summary.id,
            base: base_branch,
            head: summary.source_branch,
            base_sha: base,
            head_sha: head,
            files,
            truncated,
            tab: tab_label,
        })
    }

    /// Deja la PR preparada en local: checkout de la rama, merge de la base y ficheros resueltos
    /// escritos y preparados. **Pide confirmación nativa** y no hace commit ni push.
    pub async fn pr_apply_conflicts(
        &self,
        tab_id: &str,
        id: u64,
        resolutions: Vec<PrConflictResolution>,
    ) -> ApiResult<PrConflictApplied> {
        if resolutions.is_empty() {
            return Err(ApiError::new(
                "INVALID_REFERENCE",
                "No hay ninguna resolución que aplicar.",
                None,
            ));
        }
        let c = self.pr_ctx(tab_id).await?;
        let detail = c.provider.detail(id).await?;
        let summary = detail.summary.clone();
        let head_ref = if detail.checkout_ref.trim().is_empty() {
            match c.info.provider {
                ariadna_hosting::Provider::AzureDevOps => {
                    format!("refs/heads/{}", summary.source_branch)
                }
                _ => format!("refs/pull/{}/head", summary.id),
            }
        } else {
            detail.checkout_ref.clone()
        };
        let branch = summary.source_branch.clone();
        let base_sha = detail
            .target_commit
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("refs/remotes/{}/{}", c.remote_name, summary.target_branch));
        let files: Vec<String> = resolutions.iter().map(|r| r.path.clone()).collect();
        for r in &resolutions {
            if r.path.trim().is_empty() || r.path.contains("..") || r.content.trim().is_empty() {
                return Err(ApiError::new(
                    "INVALID_REFERENCE",
                    "La resolución de un fichero no es válida.",
                    Some(r.path.clone()),
                ));
            }
        }

        let ok = self
            .inner
            .prompter
            .confirm(
                "Preparar la resolución de los conflictos de la pull request",
                &format!(
                    "En {}: se bajará la rama «{branch}», se fusionará la base y se escribirán {} fichero(s) resueltos, ya preparados. No se hace commit ni se sube nada: eso lo decides tú después.",
                    c.where_(),
                    resolutions.len()
                ),
                &files.join("\n"),
            )
            .await;
        if !ok {
            return Ok(PrConflictApplied {
                done: false,
                branch,
                files: Vec::new(),
                merge_ready: false,
            });
        }

        let (remote, head_ref2, branch2, base) = (
            c.remote_name.clone(),
            head_ref.clone(),
            branch.clone(),
            base_sha.clone(),
        );
        let payload: Vec<(GitPath, Vec<u8>)> = resolutions
            .iter()
            .map(|r| {
                (
                    GitPath::new(r.path.clone().into_bytes()),
                    r.content.clone().into_bytes(),
                )
            })
            .collect();
        let applied = self
            .mutate_op(
                tab_id,
                "preparar los conflictos de la pull request",
                (remote, head_ref2, branch2, base, payload),
                |e, r, (remote, head_ref, branch, base, payload)| {
                    Box::pin(async move {
                        // 1) Rama de la PR, al día.
                        e.checkout_remote_ref(&r, &remote, &head_ref, &branch, true)
                            .await?;
                        // 2) Merge de la base: se esperan conflictos; si no hay, no hay nada que aplicar.
                        let merged = e.merge(&r, &base, MergeFastForward::Never).await?;
                        if matches!(merged, MergeResult::UpToDate) {
                            return Err(GitError::InvalidReference(format!(
                                "la rama ya tiene la base; no hay conflictos que preparar ({branch})"
                            )));
                        }
                        // 3) Escribir las resoluciones elegidas y prepararlas.
                        let confirmation = ariadna_git::Confirmation::granted("resolver conflictos de la pull request");
                        let mut paths = Vec::new();
                        for (path, contents) in payload {
                            e.write_working_tree_file(&r, &path, &contents, &confirmation)
                                .await?;
                            paths.push(path);
                        }
                        e.stage(&r, &paths).await?;
                        Ok(branch)
                    })
                },
            )
            .await?;
        Ok(PrConflictApplied {
            done: true,
            branch: applied,
            files,
            merge_ready: true,
        })
    }
}

/// `git rev-parse` de una referencia; `None` si no existe.
async fn rev_parse(runner: &ariadna_git::GitRunner, cwd: &Path, reference: &str) -> Option<String> {
    git_text(
        runner,
        cwd,
        &["rev-parse", "--verify", "--quiet", reference],
    )
    .await
}

/// Contenido de `rev:path` como texto (`None` si no existe o es binario).
async fn blob_at(
    runner: &ariadna_git::GitRunner,
    cwd: &Path,
    rev: &str,
    path: &str,
) -> Option<String> {
    git_text(runner, cwd, &["show", &format!("{rev}:{path}")]).await
}

/// Texto recortado al tope y aviso de binario (contiene un byte nulo).
fn split_text(raw: Option<String>) -> (String, bool) {
    match raw {
        None => (String::new(), false),
        Some(t) => {
            let binary = t.contains('\u{0}');
            let cut = if t.len() > MAX_SIDE_BYTES {
                let mut end = 0;
                for (i, ch) in t.char_indices() {
                    if i >= MAX_SIDE_BYTES {
                        break;
                    }
                    end = i + ch.len_utf8();
                }
                format!("{}\n…(recortado)\n", &t[..end])
            } else {
                t
            };
            (cut, binary)
        }
    }
}

/// Ejecuta git en modo lectura y devuelve la salida recortada.
async fn git_text(runner: &ariadna_git::GitRunner, cwd: &Path, args: &[&str]) -> Option<String> {
    let args: Vec<std::ffi::OsString> = args.iter().map(std::ffi::OsString::from).collect();
    let out = runner
        .run(cwd, CommandKind::Read, &args, None)
        .await
        .ok()
        .filter(|o| o.success())?;
    let text = out.stdout_lossy().trim().to_owned();
    (!text.is_empty()).then_some(text)
}
