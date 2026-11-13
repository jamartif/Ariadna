//! Azure DevOps (`dev.azure.com`, `{org}.visualstudio.com` y SSH → HTTPS).
//!
//! **NO VERIFICADO con servidor real**: los endpoints, cuerpos y formas de respuesta salen de la
//! documentación de Azure DevOps Services REST 7.1 (`specs/rust/spike-pr.md`). Las pruebas
//! usan un servidor simulado con JSON de ejemplo reconstruido de esa documentación.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Method;
use serde::Deserialize;
use serde_json::{json, Value};
use similar::{ChangeTag, TextDiff};
use url::Url;

use crate::auth::{CredentialSource, Scrubber};
use crate::error::{HostingError, HostingResult};
use crate::http::{error_message, retry_after, Http, Op, Reply};
use crate::model::*;
use crate::provider::{
    build_url, check_branch, first_line, safe_path, short_branch, ProviderSettings,
    PullRequestProvider,
};
use crate::remote::{Provider, RemoteInfo};

const API: &str = "7.1";
const API_PREVIEW: &str = "7.1-preview.1";
/// Archivos que se comparan como máximo en `diff` y tamaño máximo de cada uno.
const MAX_DIFF_FILES: usize = 100;
const MAX_FILE_BYTES: usize = 1024 * 1024;

pub struct AzureDevOps {
    http: Http,
    creds: Arc<dyn CredentialSource>,
    /// Página web del repo (para pedir la credencial).
    cred_url: String,
    /// `https://dev.azure.com/{org}`.
    base: Url,
    project: String,
    repo: String,
    web_repo: String,
}

// --- JSON de la API (solo lo que se usa; todo opcional) --------------------------------------

#[derive(Deserialize, Default, Clone)]
#[serde(default, rename_all = "camelCase")]
struct AzIdentity {
    id: Option<String>,
    display_name: Option<String>,
    unique_name: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzReviewer {
    id: Option<String>,
    display_name: Option<String>,
    unique_name: Option<String>,
    vote: Option<i32>,
    is_required: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzProject {
    id: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzRepo {
    project: Option<AzProject>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzCommitRef {
    commit_id: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzPr {
    pull_request_id: u64,
    title: Option<String>,
    description: Option<String>,
    status: Option<String>,
    is_draft: Option<bool>,
    created_by: Option<AzIdentity>,
    creation_date: Option<String>,
    closed_date: Option<String>,
    source_ref_name: Option<String>,
    target_ref_name: Option<String>,
    reviewers: Vec<AzReviewer>,
    merge_status: Option<String>,
    repository: Option<AzRepo>,
    last_merge_source_commit: Option<AzCommitRef>,
    last_merge_target_commit: Option<AzCommitRef>,
    auto_complete_set_by: Option<AzIdentity>,
}

#[derive(Deserialize)]
struct AzList<T> {
    #[serde(default = "Vec::new")]
    value: Vec<T>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzConnection {
    authenticated_user: Option<AzConnUser>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzConnUser {
    id: Option<String>,
    provider_display_name: Option<String>,
    custom_display_name: Option<String>,
    properties: Option<Value>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzIteration {
    id: u64,
    source_ref_commit: Option<AzCommitRef>,
    common_ref_commit: Option<AzCommitRef>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzChanges {
    change_entries: Vec<AzChange>,
    next_skip: u32,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzChange {
    change_type: Option<String>,
    item: Option<AzItem>,
    original_path: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzItem {
    path: Option<String>,
    is_folder: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzCommit {
    commit_id: Option<String>,
    comment: Option<String>,
    author: Option<AzCommitAuthor>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct AzCommitAuthor {
    name: Option<String>,
    date: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzPolicyEval {
    status: Option<String>,
    configuration: Option<AzPolicyConfig>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzPolicyConfig {
    is_blocking: Option<bool>,
    #[serde(rename = "type")]
    kind: Option<AzPolicyType>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzPolicyType {
    display_name: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzWorkItem {
    id: Option<Value>,
    url: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzThread {
    id: u64,
    status: Option<String>,
    thread_context: Option<AzThreadContext>,
    comments: Vec<AzComment>,
    is_deleted: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzThreadContext {
    file_path: Option<String>,
    right_file_start: Option<AzPos>,
    left_file_start: Option<AzPos>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzPos {
    line: u32,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AzComment {
    id: u64,
    author: Option<AzIdentity>,
    content: Option<String>,
    published_date: Option<String>,
    comment_type: Option<String>,
    is_deleted: Option<bool>,
}

// --- conversión ------------------------------------------------------------------------------

fn user(i: &AzIdentity) -> PrUser {
    let login = i.unique_name.clone().unwrap_or_default();
    PrUser {
        id: i.id.clone().unwrap_or_default(),
        display_name: i
            .display_name
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| login.clone()),
        login,
    }
}

fn vote_from(n: i32) -> Vote {
    match n {
        10.. => Vote::Approve,
        5..=9 => Vote::ApproveWithSuggestions,
        -9..=-5 => Vote::WaitForAuthor,
        i32::MIN..=-10 => Vote::Reject,
        _ => Vote::NoVote,
    }
}

fn vote_value(v: Vote) -> HostingResult<i32> {
    Ok(match v {
        Vote::Approve => 10,
        Vote::ApproveWithSuggestions => 5,
        Vote::NoVote => 0,
        Vote::WaitForAuthor => -5,
        Vote::Reject => -10,
        Vote::Comment => {
            return Err(HostingError::invalid(
                "Azure DevOps no tiene un voto «solo comentar»: deja un comentario en la PR.",
            ))
        }
    })
}

fn strategy_name(s: MergeStrategy) -> &'static str {
    match s {
        MergeStrategy::Merge => "noFastForward",
        MergeStrategy::Squash => "squash",
        MergeStrategy::Rebase => "rebase",
        MergeStrategy::RebaseMerge => "rebaseMerge",
    }
}

fn summary(pr: &AzPr, web_repo: &str) -> PullRequestSummary {
    let state = match pr.status.as_deref() {
        Some("completed") => PrState::Completed,
        Some("abandoned") => PrState::Abandoned,
        _ => PrState::Active,
    };
    let merge_status = match pr.merge_status.as_deref() {
        Some("succeeded") => MergeStatus::Clean,
        Some("conflicts") => MergeStatus::Conflicts,
        _ => MergeStatus::Unknown,
    };
    PullRequestSummary {
        id: pr.pull_request_id,
        title: pr.title.clone().unwrap_or_default(),
        author: pr.created_by.as_ref().map(user).unwrap_or_else(|| PrUser {
            id: String::new(),
            display_name: String::new(),
            login: String::new(),
        }),
        source_branch: short_branch(pr.source_ref_name.as_deref().unwrap_or("")),
        target_branch: short_branch(pr.target_ref_name.as_deref().unwrap_or("")),
        state,
        is_draft: pr.is_draft.unwrap_or(false),
        created_at: pr.creation_date.clone().unwrap_or_default(),
        updated_at: pr.closed_date.clone(),
        reviewers: pr
            .reviewers
            .iter()
            .map(|r| Reviewer {
                user: user(&AzIdentity {
                    id: r.id.clone(),
                    display_name: r.display_name.clone(),
                    unique_name: r.unique_name.clone(),
                }),
                vote: vote_from(r.vote.unwrap_or(0)),
                required: r.is_required.unwrap_or(false),
            })
            .collect(),
        checks: None,
        merge_status,
        web_url: format!("{web_repo}/pullrequest/{}", pr.pull_request_id),
    }
}

fn file_status(change_type: &str) -> FileStatus {
    let t = change_type.to_ascii_lowercase();
    if t.contains("rename") {
        FileStatus::Renamed
    } else if t.contains("add") {
        FileStatus::Added
    } else if t.contains("delete") {
        FileStatus::Deleted
    } else if t.contains("edit") {
        FileStatus::Modified
    } else {
        FileStatus::Other
    }
}

fn change_of(c: &AzChange) -> Option<PrFileChange> {
    let item = c.item.as_ref()?;
    if item.is_folder.unwrap_or(false) {
        return None;
    }
    let path = safe_path(item.path.as_deref()?)?;
    let status = file_status(c.change_type.as_deref().unwrap_or(""));
    let old_path = c
        .original_path
        .as_deref()
        .and_then(safe_path)
        .filter(|o| *o != path);
    Some(PrFileChange {
        path,
        old_path,
        status,
        additions: None,
        deletions: None,
        binary: false,
    })
}

fn thread_of(t: &AzThread) -> Option<ReviewThread> {
    if t.is_deleted.unwrap_or(false) {
        return None;
    }
    let comments: Vec<ThreadComment> = t
        .comments
        .iter()
        .filter(|c| {
            !c.is_deleted.unwrap_or(false)
                && !c
                    .comment_type
                    .as_deref()
                    .is_some_and(|k| k.eq_ignore_ascii_case("system"))
        })
        .map(|c| ThreadComment {
            id: c.id.to_string(),
            author: c.author.as_ref().map(user).unwrap_or_else(|| PrUser {
                id: String::new(),
                display_name: String::new(),
                login: String::new(),
            }),
            body: c.content.clone().unwrap_or_default(),
            created_at: c.published_date.clone(),
        })
        .collect();
    if comments.is_empty() {
        return None;
    }
    let status = match t.status.as_deref() {
        Some("fixed") => ThreadStatus::Resolved,
        Some("closed" | "wontFix" | "byDesign") => ThreadStatus::Closed,
        _ => ThreadStatus::Active,
    };
    let anchor = t.thread_context.as_ref().and_then(|c| {
        let path = safe_path(c.file_path.as_deref()?)?;
        let (line, side) = match (&c.right_file_start, &c.left_file_start) {
            (Some(p), _) if p.line > 0 => (p.line, DiffSide::New),
            (_, Some(p)) if p.line > 0 => (p.line, DiffSide::Old),
            _ => return None,
        };
        Some(ThreadAnchor { path, line, side })
    });
    Some(ReviewThread {
        id: t.id.to_string(),
        status,
        anchor,
        outdated: false,
        comments,
        can_resolve: true,
    })
}

fn permission_from(message: &str) -> Option<String> {
    // «TF401027: You need the GenericContribute permission to perform this operation.»
    let start = message.find("You need the ")? + "You need the ".len();
    let rest = message.get(start..)?;
    let end = rest.find(" permission")?;
    let p = rest.get(..end)?.trim();
    (!p.is_empty() && p.len() <= 80).then(|| p.to_owned())
}

fn classify(reply: &Reply, scrub: &Scrubber) -> HostingError {
    let msg = error_message(&reply.body).map(|m| scrub.scrub(&m));
    let type_key: String = serde_json::from_slice::<Value>(&reply.body)
        .ok()
        .and_then(|v| v.get("typeKey").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_default();
    let lowered = msg.as_deref().unwrap_or("").to_lowercase();
    let policyish = type_key.contains("Policy")
        || lowered.contains("policy")
        || lowered.contains("política")
        || type_key.contains("CompletionFailed");
    let status = reply.status.as_u16();
    match status {
        // Azure DevOps responde 203 o redirige a la página de inicio de sesión con un token
        // inválido, caducado o sin acceso a la organización.
        203 | 301 | 302 | 303 | 307 | 308 | 401 => HostingError::auth(
            "Azure DevOps rechazó el token (caducado, revocado o sin acceso a la organización).",
            None,
        ),
        403 => {
            let permission = msg.as_deref().and_then(permission_from);
            if type_key.contains("Policy") {
                return HostingError::Policy {
                    message: msg.unwrap_or_else(|| "Una política de la rama lo impide.".into()),
                };
            }
            HostingError::auth(
                msg.unwrap_or_else(|| {
                    "Azure DevOps denegó el acceso: el token no tiene el permiso necesario.".into()
                }),
                permission,
            )
        }
        404 => HostingError::NotFound {
            message: msg.unwrap_or_else(|| {
                "Azure DevOps no encuentra ese recurso (¿repositorio, proyecto o PR inexistente, o sin acceso?).".into()
            }),
        },
        409 => HostingError::Conflict {
            message: msg.unwrap_or_else(|| "La PR cambió o ya existe una igual.".into()),
        },
        429 => HostingError::RateLimited {
            message: "Azure DevOps limita las peticiones: espera un momento.".into(),
            retry_after_secs: retry_after(&reply.headers),
        },
        400 | 412 | 422 if policyish => HostingError::Policy {
            message: msg.unwrap_or_else(|| "Una política del proveedor lo impide.".into()),
        },
        400 | 412 | 422 => HostingError::Invalid {
            message: msg.unwrap_or_else(|| "Azure DevOps rechazó la petición.".into()),
        },
        500..=599 => HostingError::network(format!(
            "Azure DevOps no está disponible ahora (HTTP {status})."
        )),
        _ => HostingError::bad(format!("Respuesta inesperada de Azure DevOps (HTTP {status}).")),
    }
}

/// Las partes accesorias del detalle (políticas, work items) no deben tumbarlo si el token no
/// llega a ellas.
fn soft<T: Default>(r: HostingResult<T>) -> HostingResult<T> {
    match r {
        Err(
            HostingError::NotFound { .. }
            | HostingError::Auth { .. }
            | HostingError::Policy { .. }
            | HostingError::BadResponse { .. },
        ) => Ok(T::default()),
        other => other,
    }
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|b| *b == 0)
}

impl AzureDevOps {
    pub fn new(
        info: &RemoteInfo,
        creds: Arc<dyn CredentialSource>,
        settings: ProviderSettings,
    ) -> HostingResult<Self> {
        if info.owner.is_empty() || info.repo.is_empty() {
            return Err(HostingError::invalid(
                "Falta la organización o el repositorio de Azure DevOps en el remoto.",
            ));
        }
        let web_repo = info
            .web_url
            .clone()
            .ok_or_else(|| HostingError::invalid("El remoto no tiene página web conocida."))?;
        let base = match settings.api_base {
            Some(b) => b,
            None => Url::parse(&format!("https://dev.azure.com/{}", info.owner)).map_err(|_| {
                HostingError::invalid("La organización de Azure DevOps no es válida.")
            })?,
        };
        let project = info.project.clone().unwrap_or_else(|| info.repo.clone());
        Ok(Self {
            http: Http::new(&base, settings.http)?,
            creds,
            cred_url: web_repo.clone(),
            base,
            project,
            repo: info.repo.clone(),
            web_repo,
        })
    }

    async fn op(&self) -> HostingResult<Op> {
        Ok(Op::new(self.creds.credentials(&self.cred_url).await?))
    }

    /// `…/{project}/_apis/git/repositories/{repo}/{tail}` con `api-version`.
    fn git_url(&self, tail: &[&str], query: &[(&str, &str)]) -> HostingResult<Url> {
        let mut segs = vec![
            self.project.as_str(),
            "_apis",
            "git",
            "repositories",
            self.repo.as_str(),
        ];
        segs.extend_from_slice(tail);
        let mut q: Vec<(&str, &str)> = query.to_vec();
        q.push(("api-version", API));
        build_url(&self.base, &segs, &q)
    }

    fn pr_url(&self, id: u64, tail: &[&str], query: &[(&str, &str)]) -> HostingResult<Url> {
        let id = id.to_string();
        let mut segs = vec!["pullrequests", id.as_str()];
        segs.extend_from_slice(tail);
        self.git_url(&segs, query)
    }

    async fn call(
        &self,
        op: &Op,
        method: Method,
        url: Url,
        body: Option<&Value>,
    ) -> HostingResult<Reply> {
        self.http.send(op, method, url, body, &[], &classify).await
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, op: &Op, url: Url) -> HostingResult<T> {
        let reply = self.call(op, Method::GET, url, None).await?;
        reply.json()
    }

    async fn me(&self, op: &Op) -> HostingResult<PrUser> {
        let url = build_url(
            &self.base,
            &["_apis", "connectionData"],
            &[("api-version", API_PREVIEW)],
        )?;
        let c: AzConnection = self.get(op, url).await?;
        let u = c
            .authenticated_user
            .ok_or_else(|| HostingError::bad("Azure DevOps no devolvió el usuario autenticado."))?;
        let account = u
            .properties
            .as_ref()
            .and_then(|p| p.get("Account"))
            .and_then(|a| a.get("$value"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        Ok(PrUser {
            id: u.id.unwrap_or_default(),
            display_name: u
                .custom_display_name
                .or(u.provider_display_name)
                .unwrap_or_else(|| account.clone()),
            login: account,
        })
    }

    async fn pr(&self, op: &Op, id: u64) -> HostingResult<AzPr> {
        self.get(op, self.pr_url(id, &[], &[])?).await
    }

    async fn iterations(&self, op: &Op, id: u64) -> HostingResult<Vec<AzIteration>> {
        let l: AzList<AzIteration> = self.get(op, self.pr_url(id, &["iterations"], &[])?).await?;
        Ok(l.value)
    }

    /// Cambios de la última iteración (con paginación `$top`/`$skip`).
    async fn changes(&self, op: &Op, id: u64, iteration: u64) -> HostingResult<Vec<AzChange>> {
        let iter = iteration.to_string();
        let mut out = Vec::new();
        let mut skip = 0u32;
        for _ in 0..self.http.config().max_pages {
            let skip_s = skip.to_string();
            let url = self.pr_url(
                id,
                &["iterations", &iter, "changes"],
                &[("$top", "100"), ("$skip", &skip_s)],
            )?;
            let page: AzChanges = self.get(op, url).await?;
            out.extend(page.change_entries);
            if page.next_skip == 0 || page.next_skip <= skip {
                break;
            }
            skip = page.next_skip;
        }
        Ok(out)
    }

    async fn item_text(&self, op: &Op, path: &str, commit: &str) -> HostingResult<Option<Vec<u8>>> {
        let p = format!("/{path}");
        let url = self.git_url(
            &["items"],
            &[
                ("path", &p),
                ("versionDescriptor.version", commit),
                ("versionDescriptor.versionType", "commit"),
                ("$format", "text"),
            ],
        )?;
        let reply = self
            .http
            .send(
                op,
                Method::GET,
                url,
                None,
                &[("Accept", "text/plain")],
                &classify,
            )
            .await;
        match reply {
            Ok(r) => Ok(Some(r.body)),
            Err(HostingError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn build_detail(&self, op: &Op, pr: AzPr) -> HostingResult<PullRequestDetail> {
        let id = pr.pull_request_id;
        let project_id = pr
            .repository
            .as_ref()
            .and_then(|r| r.project.as_ref())
            .and_then(|p| p.id.clone());
        let policies = match project_id {
            Some(pid) => soft(self.policies(op, &pid, id).await)?,
            None => Vec::new(),
        };
        let work_items = soft(self.work_items(op, id).await)?;
        let commits = soft(self.commits(op, id).await)?;
        let files = soft(self.files(op, id).await)?;
        let mut s = summary(&pr, &self.web_repo);
        s.checks = Some(ChecksSummary::of(&policies));
        Ok(PullRequestDetail {
            summary: s,
            description: pr.description.clone().unwrap_or_default(),
            policies,
            work_items,
            commits,
            files,
            source_commit: pr.last_merge_source_commit.and_then(|c| c.commit_id),
            target_commit: pr.last_merge_target_commit.and_then(|c| c.commit_id),
            checkout_ref: pr.source_ref_name.clone().unwrap_or_default(),
            is_fork: false,
            auto_complete: pr.auto_complete_set_by.is_some(),
            merge_detail: pr.merge_status.clone(),
        })
    }

    async fn policies(
        &self,
        op: &Op,
        project_id: &str,
        id: u64,
    ) -> HostingResult<Vec<PolicyCheck>> {
        let artifact = format!("vstfs:///CodeReview/CodeReviewId/{project_id}/{id}");
        let url = build_url(
            &self.base,
            &[&self.project, "_apis", "policy", "evaluations"],
            &[("artifactId", &artifact), ("api-version", API_PREVIEW)],
        )?;
        let l: AzList<AzPolicyEval> = self.get(op, url).await?;
        Ok(l.value
            .into_iter()
            .map(|e| {
                let cfg = e.configuration.unwrap_or_default();
                PolicyCheck {
                    name: cfg
                        .kind
                        .and_then(|k| k.display_name)
                        .unwrap_or_else(|| "Política".into()),
                    status: match e.status.as_deref() {
                        Some("approved") => CheckStatus::Passing,
                        Some("rejected") => CheckStatus::Failing,
                        Some("running" | "queued") => CheckStatus::Pending,
                        Some("broken") => CheckStatus::Broken,
                        _ => CheckStatus::NotApplicable,
                    },
                    blocking: cfg.is_blocking,
                    url: None,
                }
            })
            .collect())
    }

    async fn work_items(&self, op: &Op, id: u64) -> HostingResult<Vec<WorkItemRef>> {
        let l: AzList<AzWorkItem> = self.get(op, self.pr_url(id, &["workitems"], &[])?).await?;
        Ok(l.value
            .into_iter()
            .filter_map(|w| {
                let id = match w.id? {
                    Value::String(s) => s.parse().ok()?,
                    Value::Number(n) => n.as_u64()?,
                    _ => return None,
                };
                Some(WorkItemRef { id, url: w.url })
            })
            .collect())
    }

    async fn commits(&self, op: &Op, id: u64) -> HostingResult<Vec<PrCommit>> {
        let l: AzList<AzCommit> = self
            .get(op, self.pr_url(id, &["commits"], &[("$top", "100")])?)
            .await?;
        Ok(l.value
            .into_iter()
            .filter_map(|c| {
                let a = c.author.unwrap_or_default();
                Some(PrCommit {
                    sha: c.commit_id?,
                    subject: first_line(c.comment.as_deref().unwrap_or("")),
                    author: a.name.unwrap_or_default(),
                    date: a.date,
                })
            })
            .collect())
    }

    async fn files(&self, op: &Op, id: u64) -> HostingResult<Vec<PrFileChange>> {
        let its = self.iterations(op, id).await?;
        let Some(last) = its.iter().max_by_key(|i| i.id) else {
            return Ok(Vec::new());
        };
        let changes = self.changes(op, id, last.id).await?;
        Ok(changes.iter().filter_map(change_of).collect())
    }

    fn check_text(what: &str, value: &str, max: usize) -> HostingResult<()> {
        if value.trim().is_empty() || value.chars().count() > max {
            return Err(HostingError::invalid(format!(
                "{what} no puede estar vacío ni superar {max} caracteres."
            )));
        }
        Ok(())
    }

    async fn patch_status(&self, id: u64, status: &str) -> HostingResult<PullRequestDetail> {
        let op = self.op().await?;
        let pr: AzPr = {
            let reply = self
                .call(
                    &op,
                    Method::PATCH,
                    self.pr_url(id, &[], &[])?,
                    Some(&json!({ "status": status })),
                )
                .await?;
            reply.json()?
        };
        self.build_detail(&op, pr).await
    }

    fn thread_from(reply: &Reply) -> HostingResult<ReviewThread> {
        let t: AzThread = reply.json()?;
        thread_of(&t).ok_or_else(|| HostingError::bad("Azure DevOps devolvió un hilo vacío."))
    }
}

#[async_trait]
impl PullRequestProvider for AzureDevOps {
    fn provider(&self) -> Provider {
        Provider::AzureDevOps
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            provider: Provider::AzureDevOps,
            votes: vec![
                Vote::Approve,
                Vote::ApproveWithSuggestions,
                Vote::NoVote,
                Vote::WaitForAuthor,
                Vote::Reject,
            ],
            strategies: vec![
                MergeStrategy::Merge,
                MergeStrategy::Squash,
                MergeStrategy::Rebase,
                MergeStrategy::RebaseMerge,
            ],
            auto_complete: true,
            resolve_threads: true,
            work_items: true,
            draft: true,
        }
    }

    async fn current_user(&self) -> HostingResult<PrUser> {
        let op = self.op().await?;
        self.me(&op).await
    }

    async fn list(&self, q: &ListQuery) -> HostingResult<PullRequestPage> {
        let op = self.op().await?;
        let per = q.per_page.clamp(1, 100);
        let page = q.page.max(1);
        let top = (per + 1).to_string();
        let skip = ((page - 1) * per).to_string();
        let status = if q.filter == PrFilter::Closed {
            "all"
        } else {
            "active"
        };
        let mut query: Vec<(&str, String)> = vec![
            ("searchCriteria.status", status.to_owned()),
            ("$top", top),
            ("$skip", skip),
        ];
        match q.filter {
            PrFilter::Mine => query.push(("searchCriteria.creatorId", self.me(&op).await?.id)),
            PrFilter::Reviewing => {
                query.push(("searchCriteria.reviewerId", self.me(&op).await?.id));
            }
            PrFilter::Open | PrFilter::Closed => {}
        }
        let refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let list: AzList<AzPr> = self
            .get(&op, self.git_url(&["pullrequests"], &refs)?)
            .await?;
        let has_more = list.value.len() > per as usize;
        let items = list
            .value
            .iter()
            .take(per as usize)
            .filter(|p| q.filter != PrFilter::Closed || p.status.as_deref() != Some("active"))
            .map(|p| summary(p, &self.web_repo))
            .collect();
        Ok(PullRequestPage {
            items,
            page,
            has_more,
        })
    }

    async fn summary(&self, id: u64) -> HostingResult<PullRequestSummary> {
        let op = self.op().await?;
        let pr = self.pr(&op, id).await?;
        Ok(summary(&pr, &self.web_repo))
    }

    async fn detail(&self, id: u64) -> HostingResult<PullRequestDetail> {
        let op = self.op().await?;
        let pr = self.pr(&op, id).await?;
        self.build_detail(&op, pr).await
    }

    async fn create(&self, new: &NewPullRequest) -> HostingResult<PullRequestDetail> {
        Self::check_text("El título", &new.title, 400)?;
        check_branch(&new.source_branch, "de origen")?;
        check_branch(&new.target_branch, "de destino")?;
        if new.description.chars().count() > 4000 {
            return Err(HostingError::invalid(
                "Azure DevOps limita la descripción a 4000 caracteres.",
            ));
        }
        let mut body = json!({
            "sourceRefName": format!("refs/heads/{}", new.source_branch.trim()),
            "targetRefName": format!("refs/heads/{}", new.target_branch.trim()),
            "title": new.title.trim(),
            "description": new.description,
            "isDraft": new.draft,
        });
        if !new.reviewers.is_empty() {
            body["reviewers"] = Value::Array(
                new.reviewers
                    .iter()
                    .filter(|r| !r.trim().is_empty() && !r.chars().any(char::is_control))
                    .map(|r| json!({ "id": r.trim() }))
                    .collect(),
            );
        }
        if !new.work_item_ids.is_empty() {
            body["workItemRefs"] = Value::Array(
                new.work_item_ids
                    .iter()
                    .map(|w| json!({ "id": w.to_string() }))
                    .collect(),
            );
        }
        let op = self.op().await?;
        let reply = self
            .call(
                &op,
                Method::POST,
                self.git_url(&["pullrequests"], &[])?,
                Some(&body),
            )
            .await?;
        let pr: AzPr = reply.json()?;
        self.build_detail(&op, pr).await
    }

    async fn close(&self, id: u64) -> HostingResult<PullRequestDetail> {
        self.patch_status(id, "abandoned").await
    }

    async fn reopen(&self, id: u64) -> HostingResult<PullRequestDetail> {
        self.patch_status(id, "active").await
    }

    async fn diff(&self, id: u64) -> HostingResult<Vec<FilePatch>> {
        let op = self.op().await?;
        let its = self.iterations(&op, id).await?;
        let Some(last) = its.iter().max_by_key(|i| i.id) else {
            return Ok(Vec::new());
        };
        let head = last
            .source_ref_commit
            .as_ref()
            .and_then(|c| c.commit_id.clone())
            .ok_or_else(|| HostingError::bad("Azure DevOps no devolvió la cabeza de la PR."))?;
        let base = last
            .common_ref_commit
            .as_ref()
            .and_then(|c| c.commit_id.clone())
            .ok_or_else(|| HostingError::bad("Azure DevOps no devolvió el ancestro común."))?;
        let changes = self.changes(&op, id, last.id).await?;
        let mut out = Vec::new();
        for (n, c) in changes.iter().filter_map(change_of).enumerate() {
            let mut change = c;
            if n >= MAX_DIFF_FILES {
                out.push(FilePatch {
                    change,
                    patch: None,
                    omitted: Some(format!(
                        "Solo se comparan los primeros {MAX_DIFF_FILES} archivos."
                    )),
                });
                continue;
            }
            let old_path = change
                .old_path
                .clone()
                .unwrap_or_else(|| change.path.clone());
            let old = if change.status == FileStatus::Added {
                Some(Vec::new())
            } else {
                self.item_text(&op, &old_path, &base).await?
            };
            let new = if change.status == FileStatus::Deleted {
                Some(Vec::new())
            } else {
                self.item_text(&op, &change.path, &head).await?
            };
            let (Some(old), Some(new)) = (old, new) else {
                out.push(FilePatch {
                    change,
                    patch: None,
                    omitted: Some("No se pudo leer el archivo en el servidor.".into()),
                });
                continue;
            };
            if is_binary(&old) || is_binary(&new) {
                change.binary = true;
                out.push(FilePatch {
                    change,
                    patch: None,
                    omitted: Some("Archivo binario.".into()),
                });
                continue;
            }
            if old.len() > MAX_FILE_BYTES || new.len() > MAX_FILE_BYTES {
                out.push(FilePatch {
                    change,
                    patch: None,
                    omitted: Some(
                        "Archivo demasiado grande para compararlo (más de 1 MiB).".into(),
                    ),
                });
                continue;
            }
            let (o, nw) = (
                String::from_utf8_lossy(&old).into_owned(),
                String::from_utf8_lossy(&new).into_owned(),
            );
            let diff = TextDiff::from_lines(&o, &nw);
            let (mut add, mut del) = (0u32, 0u32);
            for ch in diff.iter_all_changes() {
                match ch.tag() {
                    ChangeTag::Insert => add += 1,
                    ChangeTag::Delete => del += 1,
                    ChangeTag::Equal => {}
                }
            }
            change.additions = Some(add);
            change.deletions = Some(del);
            let a = if change.status == FileStatus::Added {
                "/dev/null".to_owned()
            } else {
                format!("a/{old_path}")
            };
            let b = if change.status == FileStatus::Deleted {
                "/dev/null".to_owned()
            } else {
                format!("b/{}", change.path)
            };
            let patch = diff
                .unified_diff()
                .context_radius(3)
                .header(&a, &b)
                .to_string();
            out.push(FilePatch {
                change,
                patch: (add + del > 0).then_some(patch),
                omitted: None,
            });
        }
        Ok(out)
    }

    async fn threads(&self, id: u64) -> HostingResult<Vec<ReviewThread>> {
        let op = self.op().await?;
        let l: AzList<AzThread> = self.get(&op, self.pr_url(id, &["threads"], &[])?).await?;
        Ok(l.value.iter().filter_map(thread_of).collect())
    }

    async fn create_thread(&self, id: u64, thread: &NewThread) -> HostingResult<ReviewThread> {
        Self::check_text("El comentario", &thread.body, 150_000)?;
        let mut body = json!({
            "comments": [{ "parentCommentId": 0, "content": thread.body, "commentType": "text" }],
            "status": "active",
        });
        if let Some(a) = &thread.anchor {
            let path = safe_path(&a.path)
                .ok_or_else(|| HostingError::invalid("La ruta del comentario no es válida."))?;
            if a.line == 0 {
                return Err(HostingError::invalid(
                    "La línea del comentario empieza en 1.",
                ));
            }
            let pos = json!({ "line": a.line, "offset": 1 });
            let (start, end) = match a.side {
                DiffSide::New => ("rightFileStart", "rightFileEnd"),
                DiffSide::Old => ("leftFileStart", "leftFileEnd"),
            };
            body["threadContext"] = json!({ "filePath": format!("/{path}"), start: pos, end: pos });
        }
        let op = self.op().await?;
        let reply = self
            .call(
                &op,
                Method::POST,
                self.pr_url(id, &["threads"], &[])?,
                Some(&body),
            )
            .await?;
        Self::thread_from(&reply)
    }

    async fn reply(&self, id: u64, thread_id: &str, body: &str) -> HostingResult<ReviewThread> {
        Self::check_text("El comentario", body, 150_000)?;
        let tid: u64 = thread_id
            .parse()
            .map_err(|_| HostingError::invalid("El hilo no es válido."))?;
        let tid = tid.to_string();
        let op = self.op().await?;
        self.call(
            &op,
            Method::POST,
            self.pr_url(id, &["threads", &tid, "comments"], &[])?,
            Some(&json!({ "content": body, "parentCommentId": 1, "commentType": "text" })),
        )
        .await?;
        let t: AzThread = self
            .get(&op, self.pr_url(id, &["threads", &tid], &[])?)
            .await?;
        thread_of(&t).ok_or_else(|| HostingError::bad("Azure DevOps devolvió un hilo vacío."))
    }

    async fn set_thread_resolved(
        &self,
        id: u64,
        thread_id: &str,
        resolved: bool,
    ) -> HostingResult<ReviewThread> {
        let tid: u64 = thread_id
            .parse()
            .map_err(|_| HostingError::invalid("El hilo no es válido."))?;
        let tid = tid.to_string();
        let op = self.op().await?;
        let reply = self
            .call(
                &op,
                Method::PATCH,
                self.pr_url(id, &["threads", &tid], &[])?,
                Some(&json!({ "status": if resolved { "fixed" } else { "active" } })),
            )
            .await?;
        Self::thread_from(&reply)
    }

    async fn vote(
        &self,
        id: u64,
        vote: Vote,
        comment: Option<&str>,
    ) -> HostingResult<PullRequestDetail> {
        let value = vote_value(vote)?;
        let op = self.op().await?;
        let me = self.me(&op).await?;
        if me.id.is_empty() {
            return Err(HostingError::bad(
                "Azure DevOps no devolvió tu identificador.",
            ));
        }
        self.call(
            &op,
            Method::PUT,
            self.pr_url(id, &["reviewers", &me.id], &[])?,
            Some(&json!({ "vote": value })),
        )
        .await?;
        if let Some(c) = comment.map(str::trim).filter(|c| !c.is_empty()) {
            let body = json!({
                "comments": [{ "parentCommentId": 0, "content": c, "commentType": "text" }],
                "status": "active",
            });
            self.call(
                &op,
                Method::POST,
                self.pr_url(id, &["threads"], &[])?,
                Some(&body),
            )
            .await?;
        }
        let pr = self.pr(&op, id).await?;
        self.build_detail(&op, pr).await
    }

    async fn complete(
        &self,
        id: u64,
        options: &CompleteOptions,
    ) -> HostingResult<PullRequestDetail> {
        let op = self.op().await?;
        let pr = self.pr(&op, id).await?;
        let head = pr
            .last_merge_source_commit
            .as_ref()
            .and_then(|c| c.commit_id.clone())
            .ok_or_else(|| {
                HostingError::bad("Azure DevOps no devolvió la cabeza de la PR; vuelve a abrirla.")
            })?;
        let mut completion = json!({
            "mergeStrategy": strategy_name(options.strategy),
            "deleteSourceBranch": options.delete_source_branch,
            "transitionWorkItems": options.complete_work_items,
        });
        if let Some(m) = options
            .commit_message
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
        {
            if m.chars().count() > 4000 || m.chars().any(|c| c == '\0') {
                return Err(HostingError::invalid("El mensaje de fusión no es válido."));
            }
            completion["mergeCommitMessage"] = Value::String(m.to_owned());
        }
        let body = if options.auto_complete {
            let me = self.me(&op).await?;
            json!({ "autoCompleteSetBy": { "id": me.id }, "completionOptions": completion })
        } else {
            json!({
                "status": "completed",
                "lastMergeSourceCommit": { "commitId": head },
                "completionOptions": completion,
            })
        };
        let reply = self
            .call(&op, Method::PATCH, self.pr_url(id, &[], &[])?, Some(&body))
            .await?;
        let updated: AzPr = reply.json()?;
        self.build_detail(&op, updated).await
    }
}
