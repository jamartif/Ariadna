//! Comandos de pestaña (todos con `tabId`) del contrato `specs/rust/ipc.md`. Los destructivos
//! preguntan por el [`Prompter`](super::prompter::Prompter) y solo después construyen la
//! `Confirmation` de `ariadna-git`.

use std::sync::Arc;

use ariadna_git::session::OpFuture;
use ariadna_git::{
    amend, Commit, Confirmation, ConflictFile, DiffScope, FileDiff, GitPath, GitServing,
    IntegrationState, MergeFastForward, MergeResult, RefreshOutcome, Repository, RepositorySession,
};

use super::api::{AmendDraft, ApiSnapshot, CommitResult, Done, HunkArg};
use super::app::{AppCore, TabMeta};
use super::error::{ApiError, ApiResult};

/// Rutas para el diálogo: las primeras, y cuántas más hay.
fn paths_text(paths: &[GitPath]) -> String {
    const SHOWN: usize = 12;
    let mut lines: Vec<String> = paths
        .iter()
        .take(SHOWN)
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    if paths.len() > SHOWN {
        lines.push(format!("… y {} más", paths.len() - SHOWN));
    }
    lines.join("\n")
}

impl AppCore {
    /// Sesión viva y datos de la pestaña. Falla si la pestaña no existe, está ausente o no hay git.
    pub(crate) fn live(&self, tab_id: &str) -> ApiResult<(RepositorySession, TabMeta)> {
        let st = self.inner.lock();
        let tab = st
            .tabs
            .iter()
            .find(|t| t.meta.id == tab_id)
            .ok_or_else(|| ApiError::unknown_tab(tab_id))?;
        if tab.missing {
            return Err(ApiError::repo_missing(&tab.meta.path));
        }
        match &tab.live {
            Some(l) => Ok((l.session.clone(), tab.meta.clone())),
            None => {
                drop(st);
                Err(self
                    .runner()
                    .err()
                    .unwrap_or_else(|| ApiError::repo_missing(tab_id)))
            }
        }
    }

    /// Rama actual y nombre del repo, para los diálogos.
    pub(crate) fn context(session: &RepositorySession, meta: &TabMeta) -> String {
        let snap = session.snapshot();
        let branch = match (&snap.status.branch_name, snap.status.is_detached) {
            (Some(b), _) => format!("rama {b}"),
            (None, true) => "HEAD separado".to_owned(),
            (None, false) => "sin rama".to_owned(),
        };
        format!("{} ({branch})", meta.name)
    }

    pub(crate) async fn mutate_op<T, A, F>(
        &self,
        tab_id: &str,
        label: &str,
        args: A,
        f: F,
    ) -> ApiResult<T>
    where
        T: Send + 'static,
        A: Clone + Send + Sync + 'static,
        F: Fn(Arc<dyn GitServing>, Repository, A) -> OpFuture<T> + Send + Sync + 'static,
    {
        let (s, _) = self.live(tab_id)?;
        s.mutate(label, move |e, r| f(e, r, args.clone()))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn read_op<T, F>(&self, tab_id: &str, label: &str, f: F) -> ApiResult<T>
    where
        T: Send + 'static,
        F: FnOnce(Arc<dyn GitServing>, Repository) -> OpFuture<T> + Send + 'static,
    {
        let (s, _) = self.live(tab_id)?;
        s.read(label, f).await.map_err(Into::into)
    }

    // --- snapshot --------------------------------------------------------------------------

    /// Último snapshot. Si una mutación acaba de subir la revisión y el refresco aún no ha
    /// publicado, espera a ese refresco para no devolver un estado viejo.
    pub(crate) async fn fresh_snapshot(s: &RepositorySession) -> Arc<ariadna_git::Snapshot> {
        let snap = s.snapshot();
        if snap.revision == 0 || snap.revision != s.revision() {
            let _ = s.refresh().await;
            return s.snapshot();
        }
        snap
    }

    pub async fn get_snapshot(&self, tab_id: &str) -> ApiResult<ApiSnapshot> {
        let (s, _) = self.live(tab_id)?;
        Ok(ApiSnapshot(Self::fresh_snapshot(&s).await))
    }

    pub async fn refresh(&self, tab_id: &str) -> ApiResult<Option<ApiSnapshot>> {
        let (s, _) = self.live(tab_id)?;
        match s.refresh().await? {
            RefreshOutcome::Published(snap) => Ok(Some(ApiSnapshot(snap))),
            RefreshOutcome::Skipped => Ok(None),
        }
    }

    // --- índice ----------------------------------------------------------------------------

    pub async fn stage(&self, tab_id: &str, paths: Vec<GitPath>) -> ApiResult<()> {
        self.mutate_op(tab_id, "preparar", paths, |e, r, p| {
            Box::pin(async move { e.stage(&r, &p).await })
        })
        .await
    }

    pub async fn unstage(&self, tab_id: &str, paths: Vec<GitPath>) -> ApiResult<()> {
        self.mutate_op(tab_id, "quitar de preparados", paths, |e, r, p| {
            Box::pin(async move { e.unstage(&r, &p).await })
        })
        .await
    }

    pub async fn stage_hunk(&self, tab_id: &str, path: GitPath, hunk: HunkArg) -> ApiResult<()> {
        let hunk: ariadna_git::DiffHunk = hunk.into();
        self.mutate_op(tab_id, "preparar hunk", (path, hunk), |e, r, (p, h)| {
            Box::pin(async move { e.stage_hunk(&r, &h, &p).await })
        })
        .await
    }

    pub async fn unstage_hunk(&self, tab_id: &str, path: GitPath, hunk: HunkArg) -> ApiResult<()> {
        let hunk: ariadna_git::DiffHunk = hunk.into();
        self.mutate_op(
            tab_id,
            "quitar hunk de preparados",
            (path, hunk),
            |e, r, (p, h)| Box::pin(async move { e.unstage_hunk(&r, &h, &p).await }),
        )
        .await
    }

    /// Descartar cambios: confirmación nativa que nombra repo, rama y rutas.
    pub async fn discard(&self, tab_id: &str, paths: Vec<GitPath>) -> ApiResult<Done> {
        let (s, meta) = self.live(tab_id)?;
        if paths.is_empty() {
            return Ok(Done { done: true });
        }
        let ok = self
            .inner
            .prompter
            .confirm(
                "Descartar cambios",
                &format!(
                    "En {}: se descartarán los cambios de {} archivo(s). No se puede deshacer.",
                    Self::context(&s, &meta),
                    paths.len()
                ),
                &paths_text(&paths),
            )
            .await;
        if !ok {
            return Ok(Done { done: false });
        }
        let confirmation = Confirmation::granted("descartar cambios");
        s.mutate("descartar cambios", move |e, r| {
            let (p, c) = (paths.clone(), confirmation.clone());
            Box::pin(async move { e.restore_worktree(&r, &p, &c).await })
        })
        .await?;
        Ok(Done { done: true })
    }

    // --- commit ----------------------------------------------------------------------------

    pub async fn commit(
        &self,
        tab_id: &str,
        message: String,
        allow_detached: bool,
    ) -> ApiResult<CommitResult> {
        let attempt = |allow: bool, msg: String| {
            self.mutate_op(tab_id, "commit", (msg, allow), |e, r, (m, a)| {
                Box::pin(async move { e.commit(&r, &m, a).await })
            })
        };
        match attempt(allow_detached, message.clone()).await {
            Ok(sha) => Ok(CommitResult {
                done: true,
                sha: Some(sha),
            }),
            Err(e) if e.code == "DETACHED_HEAD_COMMIT" && !allow_detached => {
                let (s, meta) = self.live(tab_id)?;
                let ok = self
                    .inner
                    .prompter
                    .confirm(
                        "Commit en HEAD separado",
                        &format!(
                            "En {}: HEAD no está en una rama. El commit quedaría suelto y podrías perderlo al cambiar de rama.",
                            Self::context(&s, &meta)
                        ),
                        "Si prefieres, cancela y crea una rama antes.",
                    )
                    .await;
                if !ok {
                    return Ok(CommitResult {
                        done: false,
                        sha: None,
                    });
                }
                let sha = attempt(true, message).await?;
                Ok(CommitResult {
                    done: true,
                    sha: Some(sha),
                })
            }
            Err(e) => Err(e),
        }
    }

    pub async fn amend(&self, tab_id: &str, message: String) -> ApiResult<String> {
        self.mutate_op(tab_id, "corregir último commit", message, |e, r, m| {
            Box::pin(async move { e.amend_last_commit(&r, &m).await })
        })
        .await
    }

    /// Mensaje del último commit y si se puede corregir. Las reglas que no necesitan git
    /// (sin commits, HEAD separado, integración en curso) se comprueban aquí; que el commit ya
    /// esté en un remoto lo decide `amend` y se devuelve como `CANNOT_AMEND`.
    pub async fn amend_draft(&self, tab_id: &str) -> ApiResult<AmendDraft> {
        let (s, _) = self.live(tab_id)?;
        let snap = Self::fresh_snapshot(&s).await;
        let reason = if snap.status.is_unborn {
            Some("Todavía no hay ningún commit que corregir.")
        } else if snap.status.is_detached {
            Some("HEAD no está en una rama. Crea una rama antes de corregir el commit.")
        } else if snap.integration != IntegrationState::None {
            Some("Hay un merge, rebase o cherry-pick en curso. Termínalo o abórtalo antes.")
        } else {
            None
        };
        if let Some(r) = reason {
            return Ok(AmendDraft {
                message: String::new(),
                can_amend: false,
                reason: Some(r.to_owned()),
            });
        }
        let last: Vec<Commit> = self
            .read_op(tab_id, "leer último commit", |e, r| {
                Box::pin(async move { e.log(&r, 1).await })
            })
            .await?;
        Ok(match last.first() {
            Some(c) => AmendDraft {
                message: amend::message_for(c),
                can_amend: true,
                reason: amend::merge_warning(c),
            },
            None => AmendDraft {
                message: String::new(),
                can_amend: false,
                reason: Some("Todavía no hay ningún commit que corregir.".to_owned()),
            },
        })
    }

    // --- lecturas --------------------------------------------------------------------------

    pub async fn diff(
        &self,
        tab_id: &str,
        scope: DiffScope,
        paths: Vec<GitPath>,
    ) -> ApiResult<Vec<FileDiff>> {
        self.read_op(tab_id, "diff", move |e, r| {
            Box::pin(async move { e.diff(&r, &scope, &paths).await })
        })
        .await
    }

    pub async fn log(
        &self,
        tab_id: &str,
        limit: usize,
        query: Option<String>,
    ) -> ApiResult<Vec<Commit>> {
        self.read_op(tab_id, "historial", move |e, r| {
            Box::pin(async move {
                match query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
                    Some(q) => e.search_log(&r, q, limit).await,
                    None => e.log(&r, limit).await,
                }
            })
        })
        .await
    }

    pub async fn get_conflicts(&self, tab_id: &str) -> ApiResult<Vec<ConflictFile>> {
        self.read_op(tab_id, "conflictos", |e, r| {
            Box::pin(async move { e.conflicts(&r).await })
        })
        .await
    }

    // --- ramas -----------------------------------------------------------------------------

    pub async fn create_branch(
        &self,
        tab_id: &str,
        name: String,
        start_point: Option<String>,
    ) -> ApiResult<()> {
        self.mutate_op(tab_id, "crear rama", (name, start_point), |e, r, (n, s)| {
            Box::pin(async move { e.create_branch(&r, &n, s.as_deref()).await })
        })
        .await
    }

    pub async fn rename_branch(&self, tab_id: &str, old: String, new: String) -> ApiResult<()> {
        self.mutate_op(tab_id, "renombrar rama", (old, new), |e, r, (o, n)| {
            Box::pin(async move { e.rename_branch(&r, &o, &n).await })
        })
        .await
    }

    /// Cambiar de rama. Si la rama local diverge de la remota, pregunta antes de seguir.
    pub async fn checkout(&self, tab_id: &str, reference: String) -> ApiResult<Done> {
        let attempt = |allow: bool| {
            self.mutate_op(
                tab_id,
                "cambiar de rama",
                (reference.clone(), allow),
                |e, r, (n, a)| Box::pin(async move { e.checkout(&r, &n, a).await }),
            )
        };
        match attempt(false).await {
            Ok(()) => Ok(Done { done: true }),
            Err(e) if e.code == "DIVERGED_LOCAL_BRANCH" => {
                let (s, meta) = self.live(tab_id)?;
                let ok = self
                    .inner
                    .prompter
                    .confirm(
                        "Cambiar a una rama que ha divergido",
                        &format!("En {}: {}", Self::context(&s, &meta), e.message),
                        &format!("Referencia: {reference}"),
                    )
                    .await;
                if !ok {
                    return Ok(Done { done: false });
                }
                attempt(true).await?;
                Ok(Done { done: true })
            }
            Err(e) => Err(e),
        }
    }

    // --- integración -----------------------------------------------------------------------

    pub async fn merge(
        &self,
        tab_id: &str,
        branch: String,
        fast_forward: MergeFastForward,
    ) -> ApiResult<MergeResult> {
        self.mutate_op(tab_id, "merge", (branch, fast_forward), |e, r, (b, f)| {
            Box::pin(async move { e.merge(&r, &b, f).await })
        })
        .await
    }

    pub async fn continue_integration(&self, tab_id: &str) -> ApiResult<()> {
        self.mutate_op(tab_id, "continuar integración", (), |e, r, ()| {
            Box::pin(async move { e.continue_integration(&r).await })
        })
        .await
    }

    pub async fn abort_integration(&self, tab_id: &str) -> ApiResult<Done> {
        let (s, meta) = self.live(tab_id)?;
        // Sin integración en curso no hay nada que confirmar: el motor lo dirá. Se consulta al
        // motor y no al snapshot, que puede ir un refresco por detrás.
        let state = self
            .read_op(tab_id, "estado de la integración", |e, r| {
                Box::pin(async move { e.integration_state(&r).await })
            })
            .await?;
        if state == IntegrationState::None {
            return self
                .mutate_op(tab_id, "abortar integración", (), |e, r, ()| {
                    Box::pin(async move { e.abort_integration(&r).await })
                })
                .await
                .map(|()| Done { done: true });
        }
        let ok = self
            .inner
            .prompter
            .confirm(
                "Abortar la integración",
                &format!(
                    "En {}: se abortará el merge, rebase o cherry-pick en curso y se perderá lo resuelto hasta ahora.",
                    Self::context(&s, &meta)
                ),
                &meta.path,
            )
            .await;
        if !ok {
            return Ok(Done { done: false });
        }
        self.mutate_op(tab_id, "abortar integración", (), |e, r, ()| {
            Box::pin(async move { e.abort_integration(&r).await })
        })
        .await?;
        Ok(Done { done: true })
    }

    /// Escribe la resolución en el árbol de trabajo (bytes tal cual). No hace `git add`.
    pub async fn write_conflict_resolution(
        &self,
        tab_id: &str,
        path: GitPath,
        contents: Vec<u8>,
    ) -> ApiResult<Done> {
        let (s, meta) = self.live(tab_id)?;
        let ok = self
            .inner
            .prompter
            .confirm(
                "Escribir la resolución del conflicto",
                &format!(
                    "En {}: se sobrescribirá el archivo con la resolución elegida.",
                    Self::context(&s, &meta)
                ),
                &path.to_string_lossy(),
            )
            .await;
        if !ok {
            return Ok(Done { done: false });
        }
        let confirmation = Confirmation::granted("escribir la resolución del conflicto");
        let contents = Arc::new(contents);
        s.mutate("escribir resolución", move |e, r| {
            let (p, c, k) = (path.clone(), Arc::clone(&contents), confirmation.clone());
            Box::pin(async move { e.write_working_tree_file(&r, &p, &c, &k).await })
        })
        .await?;
        Ok(Done { done: true })
    }

    pub async fn mark_resolved(&self, tab_id: &str, path: GitPath) -> ApiResult<()> {
        self.mutate_op(tab_id, "marcar resuelto", path, |e, r, p| {
            Box::pin(async move { e.mark_resolved(&r, &p).await })
        })
        .await
    }

    // --- remotos ---------------------------------------------------------------------------

    pub async fn fetch(&self, tab_id: &str, remote: String) -> ApiResult<()> {
        self.mutate_op(tab_id, "fetch", remote, |e, r, n| {
            Box::pin(async move { e.fetch(&r, &n).await })
        })
        .await
    }

    pub async fn pull(
        &self,
        tab_id: &str,
        remote: String,
        refspec: Option<String>,
        rebase: bool,
    ) -> ApiResult<MergeResult> {
        self.mutate_op(
            tab_id,
            "pull",
            (remote, refspec, rebase),
            |e, r, (n, s, b)| Box::pin(async move { e.pull(&r, &n, s.as_deref(), b).await }),
        )
        .await
    }

    pub async fn push(
        &self,
        tab_id: &str,
        remote: String,
        refspec: Option<String>,
        set_upstream: bool,
    ) -> ApiResult<()> {
        self.mutate_op(
            tab_id,
            "push",
            (remote, refspec, set_upstream),
            |e, r, (n, s, u)| Box::pin(async move { e.push(&r, &n, s.as_deref(), u).await }),
        )
        .await
    }

    /// Cancela una operación en curso o en cola de la pestaña (por el id de `session-event`).
    pub fn cancel_operation(&self, tab_id: &str, operation_id: u64) -> ApiResult<bool> {
        // Los clonados no pertenecen a ninguna pestaña: se cancelan por id.
        if super::clone_ops::cancel_clone(operation_id) {
            return Ok(true);
        }
        let (s, _) = self.live(tab_id)?;
        // Las operaciones de IA llevan su propio rango de ids (`session-event`).
        if self.ai_cancel_operation(tab_id, operation_id) {
            return Ok(true);
        }
        let found = s
            .pending_operations()
            .into_iter()
            .find(|p| p.operation_id.get() == operation_id);
        Ok(found.is_some_and(|p| s.cancel_operation(p.operation_id)))
    }
}
