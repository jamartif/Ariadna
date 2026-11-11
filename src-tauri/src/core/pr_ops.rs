//! Pull requests (spec §4.4, M8): comandos del núcleo sobre `ariadna-hosting`.
//!
//! * Solo hay red cuando la UI lo pide (sin sondeo). El proveedor lo elige el remoto de la
//!   pestaña (`origin`, o el primero que admita PR) y **nunca** viene de la UI.
//! * El token no pasa por aquí: lo obtiene `ariadna-hosting` con `git credential fill` en cada
//!   operación, lo usa en memoria y lo borra. Ningún comando lo recibe, lo guarda ni lo devuelve.
//! * Las acciones destructivas (abandonar, completar, votar con rechazo) abren una
//!   **confirmación nativa** desde Rust que nombra repo, PR y rama destino; la UI no manda
//!   ningún «confirmed». Se valida todo antes de preguntar.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use ariadna_git::{parse_diff, FileDiff, Remote};
use ariadna_hosting::{
    provider_for, Capabilities, CompleteOptions, FilePatch, FileStatus, GitCredentialFill,
    HostingError, HostingResult, ListQuery, MergeStrategy, NewPullRequest, NewThread, PrFileChange,
    PrFilter, PrState, Provider, ProviderSettings, PullRequestDetail, PullRequestPage,
    PullRequestProvider, PullRequestSummary, RemoteInfo, ReviewThread, ThreadAnchor, Vote,
};
use serde::{Deserialize, Serialize};

use super::app::AppCore;
use super::error::{ApiError, ApiResult};

/// Tamaño máximo de una plantilla de PR del repositorio.
const MAX_TEMPLATE_BYTES: u64 = 64 * 1024;

impl From<HostingError> for ApiError {
    fn from(e: HostingError) -> Self {
        // Los mensajes de `HostingError` ya salen redactados (el token nunca llega aquí).
        ApiError::new(e.code(), e.to_string(), e.detail())
    }
}

// --- fábrica de proveedores (sustituible en pruebas) -----------------------------------------

/// Crea el cliente de PR de un remoto. En producción: credenciales por `git credential fill`
/// con el `git` configurado. Las pruebas ponen un doble.
pub trait PrProviderFactory: Send + Sync {
    fn provider(
        &self,
        info: &RemoteInfo,
        git: &Path,
    ) -> HostingResult<Arc<dyn PullRequestProvider>>;
}

struct DefaultFactory;

impl PrProviderFactory for DefaultFactory {
    fn provider(
        &self,
        info: &RemoteInfo,
        git: &Path,
    ) -> HostingResult<Arc<dyn PullRequestProvider>> {
        let creds = Arc::new(GitCredentialFill::new(git));
        provider_for(info, creds, ProviderSettings::default()).map(Arc::from)
    }
}

/// Estado de PR de la aplicación: solo la fábrica (no hay caché de nada).
pub(crate) struct PrService {
    factory: Mutex<Arc<dyn PrProviderFactory>>,
}

impl PrService {
    pub(crate) fn new() -> Self {
        Self {
            factory: Mutex::new(Arc::new(DefaultFactory)),
        }
    }

    fn factory(&self) -> Arc<dyn PrProviderFactory> {
        self.factory
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

// --- tipos del IPC ---------------------------------------------------------------------------

/// `pull_request_support`: qué se puede hacer con las PR de esta pestaña.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrSupport {
    pub supported: bool,
    /// Remoto elegido (`origin` o el primero que admita PR).
    pub remote: Option<String>,
    pub provider: Provider,
    pub provider_label: String,
    pub host: String,
    /// `host/propietario[/proyecto]/repo`, para mostrar.
    pub repo_label: String,
    /// Qué admite el proveedor (votos, estrategias…). `None` si no hay soporte.
    pub capabilities: Option<Capabilities>,
    /// Por qué no hay soporte (en español); la UI ofrece entonces «Abrir en el navegador».
    pub reason: Option<String>,
}

/// Resultado de una acción con confirmación: `done: false` si el usuario cancela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrAction {
    pub done: bool,
    pub detail: Option<PullRequestDetail>,
}

/// Archivo sin diff mostrable y por qué.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrOmitted {
    pub path: String,
    pub reason: String,
}

/// `pull_request_diff`: archivos cambiados y su diff en el mismo formato que `diff`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrDiff {
    pub files: Vec<PrFileChange>,
    pub diffs: Vec<FileDiff>,
    pub omitted: Vec<PrOmitted>,
}

/// Entrada de `add_pull_request_comment`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CommentInput {
    /// Hilo nuevo (general si no hay `anchor`).
    #[serde(rename_all = "camelCase")]
    New {
        body: String,
        #[serde(default)]
        anchor: Option<ThreadAnchor>,
    },
    /// Respuesta a un hilo.
    #[serde(rename_all = "camelCase")]
    Reply { thread_id: String, body: String },
}

// --- utilidades ------------------------------------------------------------------------------

fn repo_label(info: &RemoteInfo) -> String {
    let mut parts = vec![info.host.clone(), info.owner.clone()];
    if let Some(p) = info.project.as_ref().filter(|p| **p != info.repo) {
        parts.push(p.clone());
    }
    parts.push(info.repo.clone());
    parts.retain(|p| !p.is_empty());
    parts.join("/")
}

/// `origin` si admite PR; si no, el primer remoto que las admita; si ninguno, `origin` o el
/// primero (para explicar por qué no hay soporte).
fn pick_remote(remotes: &[Remote]) -> Option<&Remote> {
    let supports = |r: &&Remote| {
        RemoteInfo::parse(&r.fetch_url)
            .provider
            .supports_in_app_pull_requests()
    };
    remotes
        .iter()
        .find(|r| r.name == "origin")
        .filter(|r| supports(r))
        .or_else(|| remotes.iter().find(supports))
        .or_else(|| remotes.iter().find(|r| r.name == "origin"))
        .or_else(|| remotes.first())
}

fn strategy_label(s: MergeStrategy) -> &'static str {
    match s {
        MergeStrategy::Merge => "merge (sin fast-forward)",
        MergeStrategy::Squash => "squash",
        MergeStrategy::Rebase => "rebase",
        MergeStrategy::RebaseMerge => "rebase y merge",
    }
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "sí"
    } else {
        "no"
    }
}

/// Convierte los parches del proveedor al `FileDiff` del visor de diff de la app. El parche
/// llega sin la línea `diff --git`; se antepone y se pasa por `parse_diff`.
pub(crate) fn to_file_diffs(patches: &[FilePatch]) -> (Vec<FileDiff>, Vec<PrOmitted>) {
    let mut text = String::new();
    let mut omitted = Vec::new();
    for p in patches {
        let c = &p.change;
        match &p.patch {
            Some(patch) => {
                let old = c.old_path.as_deref().unwrap_or(&c.path);
                text.push_str(&format!("diff --git a/{old} b/{}\n", c.path));
                match c.status {
                    FileStatus::Added => text.push_str("new file mode 100644\n"),
                    FileStatus::Deleted => text.push_str("deleted file mode 100644\n"),
                    FileStatus::Renamed => {
                        text.push_str(&format!("rename from {old}\nrename to {}\n", c.path));
                    }
                    _ => {}
                }
                text.push_str(patch);
                if !patch.ends_with('\n') {
                    text.push('\n');
                }
            }
            None => omitted.push(PrOmitted {
                path: c.path.clone(),
                reason: p
                    .omitted
                    .clone()
                    .unwrap_or_else(|| "Sin diff disponible.".into()),
            }),
        }
    }
    (parse_diff(text.as_bytes()), omitted)
}

/// Plantilla de PR del repo (PR-04). Busca sin distinguir mayúsculas en las carpetas del
/// proveedor; solo ficheros regulares (ni enlaces ni carpetas) de hasta 64 KiB.
fn find_template(root: &Path, provider: Provider) -> Option<String> {
    let dirs: &[&str] = match provider {
        Provider::AzureDevOps => &[".azuredevops", "", "docs"],
        _ => &[".github", "", "docs"],
    };
    for d in dirs {
        let dir: PathBuf = if d.is_empty() {
            root.to_path_buf()
        } else {
            root.join(d)
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut names: Vec<_> = entries
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("pull_request_template.md")
            })
            .collect();
        names.sort_by_key(|e| e.file_name());
        for e in names {
            let Ok(meta) = std::fs::symlink_metadata(e.path()) else {
                continue;
            };
            if !meta.is_file() || meta.len() > MAX_TEMPLATE_BYTES {
                continue;
            }
            if let Ok(bytes) = std::fs::read(e.path()) {
                return Some(String::from_utf8_lossy(&bytes).into_owned());
            }
        }
    }
    None
}

/// Lo necesario para una operación de PR de una pestaña.
struct PrCtx {
    provider: Arc<dyn PullRequestProvider>,
    info: RemoteInfo,
    /// «repo (rama x)» de la pestaña, para los diálogos.
    tab_label: String,
    repo_path: String,
}

impl PrCtx {
    fn where_(&self) -> String {
        format!(
            "{} — {} ({})",
            self.tab_label,
            repo_label(&self.info),
            self.info.provider.label()
        )
    }
}

impl AppCore {
    /// Sustituye la fábrica de proveedores (pruebas).
    #[cfg(test)]
    pub(crate) fn set_pr_factory(&self, factory: Arc<dyn PrProviderFactory>) {
        *self
            .inner
            .pr
            .factory
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = factory;
    }

    async fn pr_remote(&self, tab_id: &str) -> ApiResult<(Option<Remote>, String, String)> {
        let (s, meta) = self.live(tab_id)?;
        let snap = Self::fresh_snapshot(&s).await;
        Ok((
            pick_remote(&snap.remotes).cloned(),
            Self::context(&s, &meta),
            meta.path,
        ))
    }

    async fn pr_ctx(&self, tab_id: &str) -> ApiResult<PrCtx> {
        let (remote, tab_label, repo_path) = self.pr_remote(tab_id).await?;
        let Some(remote) = remote else {
            return Err(ApiError::new(
                "POLICY",
                "Este repositorio no tiene ningún remoto: no hay pull requests que gestionar.",
                None,
            ));
        };
        let info = RemoteInfo::parse(&remote.fetch_url);
        if !info.provider.supports_in_app_pull_requests() || !info.is_complete() {
            return Err(ApiError::new(
                "POLICY",
                format!(
                    "Las pull requests de «{}» ({}) no se gestionan dentro de Ariadna: ábrelas en el navegador.",
                    remote.name,
                    info.provider.label()
                ),
                None,
            ));
        }
        let git = self.runner()?.git_path().to_path_buf();
        let provider = self.inner.pr.factory().provider(&info, &git)?;
        Ok(PrCtx {
            provider,
            info,
            tab_label,
            repo_path,
        })
    }

    /// PR-01: ¿esta pestaña admite pull requests dentro de la app? No usa la red.
    pub async fn pull_request_support(&self, tab_id: &str) -> ApiResult<PrSupport> {
        let (remote, _, _) = self.pr_remote(tab_id).await?;
        let Some(remote) = remote else {
            return Ok(PrSupport {
                supported: false,
                remote: None,
                provider: Provider::Other,
                provider_label: Provider::Other.label().to_owned(),
                host: String::new(),
                repo_label: String::new(),
                capabilities: None,
                reason: Some("Este repositorio no tiene ningún remoto.".into()),
            });
        };
        let info = RemoteInfo::parse(&remote.fetch_url);
        let supported = info.provider.supports_in_app_pull_requests() && info.is_complete();
        let capabilities = if supported {
            let git = self.runner()?.git_path().to_path_buf();
            Some(
                self.inner
                    .pr
                    .factory()
                    .provider(&info, &git)?
                    .capabilities(),
            )
        } else {
            None
        };
        Ok(PrSupport {
            supported,
            remote: Some(remote.name.clone()),
            provider: info.provider,
            provider_label: info.provider.label().to_owned(),
            host: info.host.clone(),
            repo_label: repo_label(&info),
            capabilities,
            reason: (!supported).then(|| {
                format!(
                    "Las pull requests de {} se abren en el navegador.",
                    info.provider.label()
                )
            }),
        })
    }

    /// PR-02.
    pub async fn list_pull_requests(
        &self,
        tab_id: &str,
        filter: PrFilter,
        page: u32,
    ) -> ApiResult<PullRequestPage> {
        let c = self.pr_ctx(tab_id).await?;
        Ok(c.provider.list(&ListQuery::new(filter, page)).await?)
    }

    /// PR-03.
    pub async fn get_pull_request(&self, tab_id: &str, id: u64) -> ApiResult<PullRequestDetail> {
        let c = self.pr_ctx(tab_id).await?;
        Ok(c.provider.detail(id).await?)
    }

    /// PR-04. Crear no es destructivo: sin confirmación (la UI ya enseña el formulario).
    pub async fn create_pull_request(
        &self,
        tab_id: &str,
        draft: NewPullRequest,
    ) -> ApiResult<PullRequestDetail> {
        let c = self.pr_ctx(tab_id).await?;
        Ok(c.provider.create(&draft).await?)
    }

    /// PR-04: plantilla de descripción del repositorio, o `None`.
    pub async fn pull_request_template(&self, tab_id: &str) -> ApiResult<Option<String>> {
        let (remote, _, repo_path) = self.pr_remote(tab_id).await?;
        let provider = remote
            .map(|r| RemoteInfo::parse(&r.fetch_url).provider)
            .unwrap_or(Provider::Other);
        tokio::task::spawn_blocking(move || find_template(Path::new(&repo_path), provider))
            .await
            .map_err(|e| ApiError::io(format!("no se pudo leer la plantilla: {e}")))
    }

    /// PR-06: diff de la PR.
    pub async fn pull_request_diff(&self, tab_id: &str, id: u64) -> ApiResult<PrDiff> {
        let c = self.pr_ctx(tab_id).await?;
        let patches = c.provider.diff(id).await?;
        let (diffs, omitted) = to_file_diffs(&patches);
        Ok(PrDiff {
            files: patches.into_iter().map(|p| p.change).collect(),
            diffs,
            omitted,
        })
    }

    /// PR-06: hilos.
    pub async fn pull_request_threads(
        &self,
        tab_id: &str,
        id: u64,
    ) -> ApiResult<Vec<ReviewThread>> {
        let c = self.pr_ctx(tab_id).await?;
        Ok(c.provider.threads(id).await?)
    }

    /// PR-06: crear un hilo o responder a uno. Escribe en el servidor, pero no es destructivo.
    pub async fn add_pull_request_comment(
        &self,
        tab_id: &str,
        id: u64,
        input: CommentInput,
    ) -> ApiResult<ReviewThread> {
        let c = self.pr_ctx(tab_id).await?;
        Ok(match input {
            CommentInput::New { body, anchor } => {
                c.provider
                    .create_thread(id, &NewThread { body, anchor })
                    .await?
            }
            CommentInput::Reply { thread_id, body } => {
                c.provider.reply(id, &thread_id, &body).await?
            }
        })
    }

    /// PR-06: resolver o reabrir un hilo.
    pub async fn resolve_pull_request_thread(
        &self,
        tab_id: &str,
        id: u64,
        thread_id: &str,
        resolved: bool,
    ) -> ApiResult<ReviewThread> {
        let c = self.pr_ctx(tab_id).await?;
        Ok(c.provider
            .set_thread_resolved(id, thread_id, resolved)
            .await?)
    }

    /// Confirmación nativa que nombra repo, PR y rama destino. `false` si se cancela.
    async fn confirm_pr(
        &self,
        c: &PrCtx,
        title: &str,
        action: &str,
        s: &PullRequestSummary,
        extra: &[String],
    ) -> bool {
        let message = format!(
            "En {}: {action} la PR #{} «{}» (de «{}» a «{}»).",
            c.where_(),
            s.id,
            s.title,
            s.source_branch,
            s.target_branch
        );
        let mut detail = format!(
            "Repositorio: {}\nPR: #{} {}\nOrigen: {}\nDestino: {}\nProveedor: {}\nCarpeta: {}",
            repo_label(&c.info),
            s.id,
            s.title,
            s.source_branch,
            s.target_branch,
            c.info.provider.label(),
            c.repo_path
        );
        for line in extra {
            detail.push('\n');
            detail.push_str(line);
        }
        self.inner.prompter.confirm(title, &message, &detail).await
    }

    fn require_active(s: &PullRequestSummary) -> ApiResult<()> {
        if s.state == PrState::Active {
            Ok(())
        } else {
            Err(ApiError::new(
                "POLICY",
                format!("La PR #{} ya no está abierta.", s.id),
                None,
            ))
        }
    }

    /// PR-07. El voto de rechazo (`reject`, `waitForAuthor`) pide confirmación nativa.
    pub async fn vote_pull_request(
        &self,
        tab_id: &str,
        id: u64,
        vote: Vote,
        comment: Option<String>,
    ) -> ApiResult<PrAction> {
        let c = self.pr_ctx(tab_id).await?;
        if !c.provider.capabilities().votes.contains(&vote) {
            return Err(ApiError::new(
                "INVALID_REFERENCE",
                format!("{} no admite ese voto.", c.info.provider.label()),
                None,
            ));
        }
        if vote.is_rejection() {
            let s = c.provider.summary(id).await?;
            Self::require_active(&s)?;
            let what = if vote == Vote::Reject {
                "votarás «Rechazar» o «Solicitar cambios» en"
            } else {
                "votarás «Esperar al autor» en"
            };
            let extra = ["El voto bloquea la PR hasta que lo cambies.".to_owned()];
            if !self
                .confirm_pr(&c, "Votar en contra", what, &s, &extra)
                .await
            {
                return Ok(PrAction {
                    done: false,
                    detail: None,
                });
            }
        }
        let detail = c.provider.vote(id, vote, comment.as_deref()).await?;
        Ok(PrAction {
            done: true,
            detail: Some(detail),
        })
    }

    /// PR-08: completar / fusionar, con confirmación nativa. Las políticas del proveedor mandan:
    /// si no se cumplen, el servidor rechaza con `POLICY`.
    pub async fn complete_pull_request(
        &self,
        tab_id: &str,
        id: u64,
        options: CompleteOptions,
    ) -> ApiResult<PrAction> {
        let c = self.pr_ctx(tab_id).await?;
        let caps = c.provider.capabilities();
        if !caps.strategies.contains(&options.strategy) {
            return Err(ApiError::new(
                "INVALID_REFERENCE",
                format!(
                    "{} no admite la estrategia «{}».",
                    c.info.provider.label(),
                    strategy_label(options.strategy)
                ),
                None,
            ));
        }
        if options.auto_complete && !caps.auto_complete {
            return Err(ApiError::new(
                "INVALID_REFERENCE",
                format!(
                    "{} no admite la finalización automática desde Ariadna.",
                    c.info.provider.label()
                ),
                None,
            ));
        }
        let s = c.provider.summary(id).await?;
        Self::require_active(&s)?;
        let verb = if options.auto_complete {
            "se activará la finalización automática de"
        } else {
            "se completará y fusionará"
        };
        let extra = [
            format!("Estrategia: {}", strategy_label(options.strategy)),
            format!(
                "Borrar la rama de origen: {}",
                yes_no(options.delete_source_branch)
            ),
            format!(
                "Completar work items: {}",
                yes_no(options.complete_work_items)
            ),
            "Las políticas del proveedor siguen mandando: Ariadna no las salta.".to_owned(),
        ];
        if !self
            .confirm_pr(&c, "Completar pull request", verb, &s, &extra)
            .await
        {
            return Ok(PrAction {
                done: false,
                detail: None,
            });
        }
        let detail = c.provider.complete(id, &options).await?;
        Ok(PrAction {
            done: true,
            detail: Some(detail),
        })
    }

    /// PR-08: abandonar (Azure DevOps) o cerrar sin fusionar (GitHub), con confirmación nativa.
    pub async fn abandon_pull_request(&self, tab_id: &str, id: u64) -> ApiResult<PrAction> {
        let c = self.pr_ctx(tab_id).await?;
        let s = c.provider.summary(id).await?;
        Self::require_active(&s)?;
        let (title, verb) = if c.info.provider == Provider::GitHub {
            ("Cerrar pull request", "se cerrará sin fusionar")
        } else {
            ("Abandonar pull request", "se abandonará")
        };
        let extra = ["Podrás reabrirla mientras no se haya completado.".to_owned()];
        if !self.confirm_pr(&c, title, verb, &s, &extra).await {
            return Ok(PrAction {
                done: false,
                detail: None,
            });
        }
        let detail = c.provider.close(id).await?;
        Ok(PrAction {
            done: true,
            detail: Some(detail),
        })
    }

    /// PR-08: reabrir. No pide confirmación (se puede volver a cerrar).
    pub async fn reopen_pull_request(&self, tab_id: &str, id: u64) -> ApiResult<PrAction> {
        let c = self.pr_ctx(tab_id).await?;
        let detail = c.provider.reopen(id).await?;
        Ok(PrAction {
            done: true,
            detail: Some(detail),
        })
    }
}
