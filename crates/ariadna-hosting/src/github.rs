//! GitHub (`github.com`, GitHub Enterprise Server por host y `*.ghe.com`).
//!
//! Verificado contra GitHub real el 2026-11-11 (`tests/github_real.rs`, ignorado; resultados en
//! `specs/rust/spike-pr.md`). Las pruebas normales usan un servidor simulado.
//!
//! La API REST no puede listar ni resolver los hilos de revisión, así que los hilos en línea
//! van por GraphQL (`reviewThreads`, `resolveReviewThread`) y los comentarios generales por
//! `issues/{n}/comments`.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use reqwest::Method;
use serde::Deserialize;
use serde_json::{json, Value};
use url::Url;

use crate::auth::{CredentialSource, Scrubber};
use crate::error::{HostingError, HostingResult};
use crate::http::{error_message, next_link, retry_after, Http, Op, Reply};
use crate::model::*;
use crate::provider::{
    build_url, check_branch, first_line, safe_path, ProviderSettings, PullRequestProvider,
};
use crate::remote::{Provider, RemoteInfo};

const API_VERSION: &str = "2022-11-28";
const HEADERS: &[(&str, &str)] = &[
    ("Accept", "application/vnd.github+json"),
    ("X-GitHub-Api-Version", API_VERSION),
];

pub struct GitHub {
    http: Http,
    creds: Arc<dyn CredentialSource>,
    cred_url: String,
    /// Base REST (`https://api.github.com`, `https://{host}/api/v3`…).
    base: Url,
    graphql: Url,
    owner: String,
    repo: String,
    web_repo: String,
}

// --- JSON ------------------------------------------------------------------------------------

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct GhUser {
    login: Option<String>,
    id: Option<u64>,
    name: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GhRepoRef {
    full_name: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GhRef {
    #[serde(rename = "ref")]
    name: Option<String>,
    sha: Option<String>,
    repo: Option<GhRepoRef>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GhPr {
    number: u64,
    title: Option<String>,
    body: Option<String>,
    state: Option<String>,
    draft: Option<bool>,
    merged: Option<bool>,
    merged_at: Option<String>,
    user: Option<GhUser>,
    head: GhRef,
    base: GhRef,
    created_at: Option<String>,
    updated_at: Option<String>,
    requested_reviewers: Vec<GhUser>,
    html_url: Option<String>,
    mergeable: Option<bool>,
    mergeable_state: Option<String>,
    auto_merge: Option<Value>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GhReview {
    user: Option<GhUser>,
    state: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GhCheckRuns {
    check_runs: Vec<GhCheckRun>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GhCheckRun {
    name: Option<String>,
    status: Option<String>,
    conclusion: Option<String>,
    html_url: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GhCombinedStatus {
    statuses: Vec<GhStatus>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GhStatus {
    context: Option<String>,
    state: Option<String>,
    target_url: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GhCommit {
    sha: Option<String>,
    commit: GhCommitInner,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GhCommitInner {
    message: Option<String>,
    author: Option<GhCommitAuthor>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GhCommitAuthor {
    name: Option<String>,
    date: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GhFile {
    filename: Option<String>,
    previous_filename: Option<String>,
    status: Option<String>,
    additions: u32,
    deletions: u32,
    changes: u32,
    patch: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GhIssueComment {
    id: u64,
    body: Option<String>,
    user: Option<GhUser>,
    created_at: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GhReviewComment {
    id: u64,
}

// --- conversión ------------------------------------------------------------------------------

fn user(u: &GhUser) -> PrUser {
    let login = u.login.clone().unwrap_or_default();
    PrUser {
        id: u.id.map(|i| i.to_string()).unwrap_or_default(),
        display_name: u
            .name
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| login.clone()),
        login,
    }
}

fn state_of(pr: &GhPr) -> PrState {
    if pr.merged.unwrap_or(false) || pr.merged_at.is_some() {
        PrState::Completed
    } else if pr.state.as_deref() == Some("closed") {
        PrState::Abandoned
    } else {
        PrState::Active
    }
}

fn summary(pr: &GhPr, web_repo: &str) -> PullRequestSummary {
    let merge_status = match (pr.mergeable, pr.mergeable_state.as_deref()) {
        (_, Some("dirty")) | (Some(false), _) => MergeStatus::Conflicts,
        (Some(true), _) => MergeStatus::Clean,
        _ => MergeStatus::Unknown,
    };
    PullRequestSummary {
        id: pr.number,
        title: pr.title.clone().unwrap_or_default(),
        author: pr.user.as_ref().map(user).unwrap_or_else(|| PrUser {
            id: String::new(),
            display_name: String::new(),
            login: String::new(),
        }),
        source_branch: pr.head.name.clone().unwrap_or_default(),
        target_branch: pr.base.name.clone().unwrap_or_default(),
        state: state_of(pr),
        is_draft: pr.draft.unwrap_or(false),
        created_at: pr.created_at.clone().unwrap_or_default(),
        updated_at: pr.updated_at.clone(),
        reviewers: pr
            .requested_reviewers
            .iter()
            .map(|u| Reviewer {
                user: user(u),
                vote: Vote::NoVote,
                required: false,
            })
            .collect(),
        checks: None,
        merge_status,
        web_url: pr
            .html_url
            .clone()
            .unwrap_or_else(|| format!("{web_repo}/pull/{}", pr.number)),
    }
}

fn file_change(f: &GhFile) -> Option<PrFileChange> {
    let path = safe_path(f.filename.as_deref()?)?;
    let status = match f.status.as_deref() {
        Some("added") => FileStatus::Added,
        Some("removed") => FileStatus::Deleted,
        Some("renamed") => FileStatus::Renamed,
        Some("copied") => FileStatus::Copied,
        Some("modified" | "changed") => FileStatus::Modified,
        _ => FileStatus::Other,
    };
    Some(PrFileChange {
        old_path: f
            .previous_filename
            .as_deref()
            .and_then(safe_path)
            .filter(|o| *o != path),
        path,
        status,
        additions: Some(f.additions),
        deletions: Some(f.deletions),
        binary: false,
    })
}

fn check_of_run(c: &GhCheckRun) -> PolicyCheck {
    let status = match (c.status.as_deref(), c.conclusion.as_deref()) {
        (Some("completed"), Some("success" | "neutral" | "skipped")) => CheckStatus::Passing,
        (
            Some("completed"),
            Some("failure" | "timed_out" | "cancelled" | "action_required" | "startup_failure"),
        ) => CheckStatus::Failing,
        (Some("completed"), Some("stale")) => CheckStatus::NotApplicable,
        (Some("completed"), _) => CheckStatus::Broken,
        _ => CheckStatus::Pending,
    };
    PolicyCheck {
        name: c.name.clone().unwrap_or_else(|| "Comprobación".into()),
        status,
        blocking: None,
        url: c.html_url.clone(),
    }
}

fn check_of_status(s: &GhStatus) -> PolicyCheck {
    PolicyCheck {
        name: s.context.clone().unwrap_or_else(|| "Estado".into()),
        status: match s.state.as_deref() {
            Some("success") => CheckStatus::Passing,
            Some("failure" | "error") => CheckStatus::Failing,
            _ => CheckStatus::Pending,
        },
        blocking: None,
        url: s.target_url.clone(),
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Mensaje de un error de GitHub. En los 422 el detalle real va en `errors`, que son cadenas
/// («Review Can not approve your own pull request»), objetos con `message` («A pull request
/// already exists for …») u objetos `{resource, field, code}`; `message` solo dice
/// «Validation Failed» o «Unprocessable Entity» (comprobado contra GitHub real).
fn gh_message(body: &[u8]) -> Option<String> {
    let head = error_message(body);
    let v: Value = serde_json::from_slice(body).ok()?;
    let details: Vec<String> = v
        .get("errors")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| match e {
                    Value::String(s) => Some(s.clone()),
                    Value::Object(o) => o
                        .get("message")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| {
                            let field = o.get("field").and_then(Value::as_str)?;
                            let code = o.get("code").and_then(Value::as_str).unwrap_or("invalid");
                            Some(format!("{field}: {code}"))
                        }),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let joined = match (head, details.is_empty()) {
        (Some(h), true) => h,
        (Some(h), false) => format!("{h}: {}", details.join("; ")),
        (None, false) => details.join("; "),
        (None, true) => return None,
    };
    let one_line: String = joined.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(one_line.chars().take(300).collect())
}

fn classify(reply: &Reply, scrub: &Scrubber) -> HostingError {
    let msg = gh_message(&reply.body).map(|m| scrub.scrub(&m));
    let lowered = msg.as_deref().unwrap_or("").to_lowercase();
    let status = reply.status.as_u16();
    let limited = lowered.contains("rate limit")
        || lowered.contains("abuse")
        || lowered.contains("secondary");
    let exhausted = reply.header("x-ratelimit-remaining") == Some("0");
    if status == 429 || (status == 403 && (limited || exhausted)) {
        let wait = retry_after(&reply.headers).or_else(|| {
            let reset: u64 = reply.header("x-ratelimit-reset")?.trim().parse().ok()?;
            Some(reset.saturating_sub(now_secs()).max(1))
        });
        return HostingError::RateLimited {
            message: "GitHub limita las peticiones: espera un momento.".into(),
            retry_after_secs: wait,
        };
    }
    match status {
        401 => HostingError::auth(
            "GitHub rechazó el token (caducado, revocado o mal escrito).",
            None,
        ),
        403 => {
            if reply.header("x-github-sso").is_some() {
                return HostingError::auth(
                    "La organización exige autorizar el token con SSO en GitHub.",
                    None,
                );
            }
            let permission = reply
                .header("x-accepted-github-permissions")
                .or_else(|| reply.header("x-accepted-oauth-scopes"))
                .map(str::trim)
                .filter(|p| !p.is_empty() && p.len() <= 120)
                .map(str::to_owned);
            HostingError::auth(
                msg.unwrap_or_else(|| {
                    "GitHub denegó el acceso: el token no tiene el permiso necesario.".into()
                }),
                permission,
            )
        }
        404 => HostingError::NotFound {
            message: "GitHub no encuentra ese recurso (¿repositorio o PR inexistente, o el token no tiene acceso?)."
                .into(),
        },
        405 => HostingError::Policy {
            message: msg.unwrap_or_else(|| "GitHub no permite esa operación ahora.".into()),
        },
        409 => HostingError::Conflict {
            message: msg.unwrap_or_else(|| "La PR cambió mientras la editabas.".into()),
        },
        422 if lowered.contains("already exists") => HostingError::Conflict {
            message: msg.unwrap_or_default(),
        },
        422 if lowered.contains("protected")
            || lowered.contains("review")
            || lowered.contains("required")
            || lowered.contains("own pull request") =>
        {
            HostingError::Policy {
                message: msg.unwrap_or_default(),
            }
        }
        400 | 422 => HostingError::Invalid {
            message: msg.unwrap_or_else(|| "GitHub rechazó la petición.".into()),
        },
        500..=599 => {
            HostingError::network(format!("GitHub no está disponible ahora (HTTP {status})."))
        }
        _ => HostingError::bad(format!("Respuesta inesperada de GitHub (HTTP {status}).")),
    }
}

fn graphql_error(errors: &Value, scrub: &Scrubber) -> HostingError {
    let first = errors.as_array().and_then(|a| a.first());
    let kind = first
        .and_then(|e| e.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let msg = first
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .map(|m| scrub.scrub(&m.chars().take(300).collect::<String>()))
        .unwrap_or_else(|| "GitHub devolvió un error.".into());
    match kind {
        "NOT_FOUND" => HostingError::NotFound { message: msg },
        "FORBIDDEN" | "INSUFFICIENT_SCOPES" => HostingError::auth(msg, None),
        "RATE_LIMITED" => HostingError::RateLimited {
            message: msg,
            retry_after_secs: None,
        },
        _ => HostingError::bad(msg),
    }
}

const THREADS_QUERY: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100){nodes{id isResolved isOutdated path line originalLine diffSide viewerCanResolve viewerCanUnresolve comments(first:100){nodes{databaseId body createdAt author{login}}}}}}}}";

fn threads_from_graphql(data: &Value) -> Vec<ReviewThread> {
    let nodes = data
        .pointer("/data/repository/pullRequest/reviewThreads/nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    nodes
        .iter()
        .filter_map(|n| {
            let id = n.get("id")?.as_str()?.to_owned();
            let comments: Vec<ThreadComment> = n
                .pointer("/comments/nodes")?
                .as_array()?
                .iter()
                .map(|c| {
                    let login = c
                        .pointer("/author/login")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    ThreadComment {
                        id: c
                            .get("databaseId")
                            .and_then(Value::as_u64)
                            .map(|i| i.to_string())
                            .unwrap_or_default(),
                        author: PrUser {
                            id: login.clone(),
                            display_name: login.clone(),
                            login,
                        },
                        body: c
                            .get("body")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        created_at: c
                            .get("createdAt")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    }
                })
                .collect();
            if comments.is_empty() {
                return None;
            }
            let line = n
                .get("line")
                .and_then(Value::as_u64)
                .or_else(|| n.get("originalLine").and_then(Value::as_u64))
                .and_then(|l| u32::try_from(l).ok());
            let anchor = match (
                n.get("path").and_then(Value::as_str).and_then(safe_path),
                line,
            ) {
                (Some(path), Some(line)) if line > 0 => Some(ThreadAnchor {
                    path,
                    line,
                    side: if n.get("diffSide").and_then(Value::as_str) == Some("LEFT") {
                        DiffSide::Old
                    } else {
                        DiffSide::New
                    },
                }),
                _ => None,
            };
            let resolved = n
                .get("isResolved")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let can = n
                .get("viewerCanResolve")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || n.get("viewerCanUnresolve")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            Some(ReviewThread {
                id,
                status: if resolved {
                    ThreadStatus::Resolved
                } else {
                    ThreadStatus::Active
                },
                anchor,
                outdated: n
                    .get("isOutdated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                comments,
                can_resolve: can,
            })
        })
        .collect()
}

fn issue_thread(c: &GhIssueComment) -> ReviewThread {
    ReviewThread {
        id: format!("issue:{}", c.id),
        status: ThreadStatus::Active,
        anchor: None,
        outdated: false,
        comments: vec![ThreadComment {
            id: c.id.to_string(),
            author: c.user.as_ref().map(user).unwrap_or_else(|| PrUser {
                id: String::new(),
                display_name: String::new(),
                login: String::new(),
            }),
            body: c.body.clone().unwrap_or_default(),
            created_at: c.created_at.clone(),
        }],
        can_resolve: false,
    }
}

/// Base REST según el host: `github.com`, `*.ghe.com` (nube con residencia de datos) o
/// GitHub Enterprise Server (`https://{host}/api/v3`).
fn default_api_base(host: &str) -> String {
    if host == "github.com" {
        "https://api.github.com".to_owned()
    } else if let Some(sub) = host.strip_suffix(".ghe.com") {
        format!("https://api.{sub}.ghe.com")
    } else {
        format!("https://{host}/api/v3")
    }
}

fn valid_node_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl GitHub {
    pub fn new(
        info: &RemoteInfo,
        creds: Arc<dyn CredentialSource>,
        settings: ProviderSettings,
    ) -> HostingResult<Self> {
        if info.owner.is_empty() || info.repo.is_empty() {
            return Err(HostingError::invalid(
                "Falta el propietario o el repositorio de GitHub en el remoto.",
            ));
        }
        let web_repo = info
            .web_url
            .clone()
            .ok_or_else(|| HostingError::invalid("El remoto no tiene página web conocida."))?;
        let base = match settings.api_base {
            Some(b) => b,
            None => Url::parse(&default_api_base(&info.host))
                .map_err(|_| HostingError::invalid("El host de GitHub no es válido."))?,
        };
        let mut graphql = base.clone();
        let path = base.path().trim_end_matches('/').to_owned();
        match path.strip_suffix("/api/v3") {
            Some(prefix) => graphql.set_path(&format!("{prefix}/api/graphql")),
            None => graphql.set_path(&format!("{path}/graphql")),
        }
        Ok(Self {
            http: Http::new(&base, settings.http)?,
            creds,
            cred_url: web_repo.clone(),
            base,
            graphql,
            owner: info.owner.clone(),
            repo: info.repo.clone(),
            web_repo,
        })
    }

    async fn op(&self) -> HostingResult<Op> {
        Ok(Op::new(self.creds.credentials(&self.cred_url).await?))
    }

    fn repo_url(&self, tail: &[&str], query: &[(&str, &str)]) -> HostingResult<Url> {
        let mut segs = vec!["repos", self.owner.as_str(), self.repo.as_str()];
        segs.extend_from_slice(tail);
        build_url(&self.base, &segs, query)
    }

    fn pull_url(&self, id: u64, tail: &[&str], query: &[(&str, &str)]) -> HostingResult<Url> {
        let id = id.to_string();
        let mut segs = vec!["pulls", id.as_str()];
        segs.extend_from_slice(tail);
        self.repo_url(&segs, query)
    }

    async fn call(
        &self,
        op: &Op,
        method: Method,
        url: Url,
        body: Option<&Value>,
    ) -> HostingResult<Reply> {
        self.http
            .send(op, method, url, body, HEADERS, &classify)
            .await
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, op: &Op, url: Url) -> HostingResult<T> {
        self.call(op, Method::GET, url, None).await?.json()
    }

    /// Recorre `Link: rel="next"` (con tope de páginas) juntando los elementos.
    async fn get_all<T: serde::de::DeserializeOwned>(
        &self,
        op: &Op,
        first: Url,
    ) -> HostingResult<Vec<T>> {
        let mut out = Vec::new();
        let mut next = Some(first);
        for _ in 0..self.http.config().max_pages {
            let Some(url) = next.take() else { break };
            let reply = self.call(op, Method::GET, url, None).await?;
            let page: Vec<T> = reply.json()?;
            out.extend(page);
            next = next_link(&reply.headers);
        }
        Ok(out)
    }

    async fn pr(&self, op: &Op, id: u64) -> HostingResult<GhPr> {
        self.get(op, self.pull_url(id, &[], &[])?).await
    }

    async fn graphql(&self, op: &Op, query: &str, vars: Value) -> HostingResult<Value> {
        let body = json!({ "query": query, "variables": vars });
        let reply = self
            .call(op, Method::POST, self.graphql.clone(), Some(&body))
            .await?;
        let v: Value = reply.json()?;
        if let Some(errors) = v.get("errors").filter(|e| !e.is_null()) {
            return Err(graphql_error(errors, &op.scrub));
        }
        Ok(v)
    }

    async fn inline_threads(&self, op: &Op, id: u64) -> HostingResult<Vec<ReviewThread>> {
        let number =
            i64::try_from(id).map_err(|_| HostingError::invalid("Número de PR no válido."))?;
        let v = self
            .graphql(
                op,
                THREADS_QUERY,
                json!({ "owner": self.owner, "name": self.repo, "number": number }),
            )
            .await?;
        Ok(threads_from_graphql(&v))
    }

    async fn thread_by_id(&self, op: &Op, id: u64, thread_id: &str) -> HostingResult<ReviewThread> {
        self.inline_threads(op, id)
            .await?
            .into_iter()
            .find(|t| t.id == thread_id)
            .ok_or_else(|| HostingError::NotFound {
                message: "GitHub ya no encuentra ese hilo.".into(),
            })
    }

    async fn reviewers(&self, op: &Op, pr: &GhPr) -> HostingResult<Vec<Reviewer>> {
        let url = self.pull_url(pr.number, &["reviews"], &[("per_page", "100")])?;
        let reviews: Vec<GhReview> = self.get_all(op, url).await?;
        let mut out: Vec<Reviewer> = Vec::new();
        for r in &reviews {
            let Some(u) = r.user.as_ref().filter(|u| u.login.is_some()) else {
                continue;
            };
            let vote = match r.state.as_deref() {
                Some("APPROVED") => Vote::Approve,
                Some("CHANGES_REQUESTED") => Vote::Reject,
                Some("DISMISSED") => Vote::NoVote,
                Some("COMMENTED") => Vote::Comment,
                _ => continue,
            };
            let who = user(u);
            match out.iter_mut().find(|x| x.user.login == who.login) {
                // Un comentario suelto no anula una aprobación o un rechazo previos.
                Some(_) if vote == Vote::Comment => {}
                Some(prev) => prev.vote = vote,
                None => out.push(Reviewer {
                    user: who,
                    vote,
                    required: false,
                }),
            }
        }
        for u in &pr.requested_reviewers {
            let who = user(u);
            match out.iter_mut().find(|x| x.user.login == who.login) {
                // Pidieron una revisión nueva: el voto anterior ya no vale.
                Some(prev) => prev.vote = Vote::NoVote,
                None => out.push(Reviewer {
                    user: who,
                    vote: Vote::NoVote,
                    required: false,
                }),
            }
        }
        Ok(out)
    }

    async fn checks(&self, op: &Op, sha: &str) -> HostingResult<Vec<PolicyCheck>> {
        let runs: GhCheckRuns = self
            .get(
                op,
                self.repo_url(&["commits", sha, "check-runs"], &[("per_page", "100")])?,
            )
            .await?;
        let mut out: Vec<PolicyCheck> = runs.check_runs.iter().map(check_of_run).collect();
        let statuses: GhCombinedStatus = self
            .get(op, self.repo_url(&["commits", sha, "status"], &[])?)
            .await?;
        out.extend(statuses.statuses.iter().map(check_of_status));
        Ok(out)
    }

    async fn files(&self, op: &Op, id: u64) -> HostingResult<Vec<GhFile>> {
        self.get_all(op, self.pull_url(id, &["files"], &[("per_page", "100")])?)
            .await
    }

    async fn build_detail(&self, op: &Op, pr: GhPr) -> HostingResult<PullRequestDetail> {
        let reviewers = self.reviewers(op, &pr).await?;
        let sha = pr.head.sha.clone().unwrap_or_default();
        let policies = if sha.is_empty() {
            Vec::new()
        } else {
            match self.checks(op, &sha).await {
                Ok(c) => c,
                // Un token sin «checks: read» no debe tumbar el detalle.
                Err(HostingError::Auth { .. } | HostingError::NotFound { .. }) => Vec::new(),
                Err(e) => return Err(e),
            }
        };
        let commits: Vec<GhCommit> = self
            .get_all(
                op,
                self.pull_url(pr.number, &["commits"], &[("per_page", "100")])?,
            )
            .await?;
        let files = self.files(op, pr.number).await?;
        let mut s = summary(&pr, &self.web_repo);
        s.reviewers = reviewers;
        s.checks = Some(ChecksSummary::of(&policies));
        let is_fork = pr.head.repo.as_ref().and_then(|r| r.full_name.as_ref())
            != pr.base.repo.as_ref().and_then(|r| r.full_name.as_ref());
        Ok(PullRequestDetail {
            summary: s,
            description: pr.body.clone().unwrap_or_default(),
            policies,
            work_items: Vec::new(),
            commits: commits
                .into_iter()
                .filter_map(|c| {
                    let a = c.commit.author.unwrap_or_default();
                    Some(PrCommit {
                        sha: c.sha?,
                        subject: first_line(c.commit.message.as_deref().unwrap_or("")),
                        author: a.name.unwrap_or_default(),
                        date: a.date,
                    })
                })
                .collect(),
            files: files.iter().filter_map(file_change).collect(),
            source_commit: pr.head.sha.clone(),
            target_commit: pr.base.sha.clone(),
            checkout_ref: format!("refs/pull/{}/head", pr.number),
            is_fork,
            auto_complete: pr.auto_merge.as_ref().is_some_and(|v| !v.is_null()),
            // `unknown` = GitHub aún no lo ha calculado (o la PR ya está cerrada): no informa.
            merge_detail: pr
                .mergeable_state
                .clone()
                .filter(|m| !m.is_empty() && m != "unknown"),
        })
    }

    async fn set_state(&self, id: u64, state: &str) -> HostingResult<PullRequestDetail> {
        let op = self.op().await?;
        let pr: GhPr = self
            .call(
                &op,
                Method::PATCH,
                self.pull_url(id, &[], &[])?,
                Some(&json!({ "state": state })),
            )
            .await?
            .json()?;
        self.build_detail(&op, pr).await
    }

    fn check_text(what: &str, value: &str, max: usize) -> HostingResult<()> {
        if value.trim().is_empty() || value.chars().count() > max {
            return Err(HostingError::invalid(format!(
                "{what} no puede estar vacío ni superar {max} caracteres."
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl PullRequestProvider for GitHub {
    fn provider(&self) -> Provider {
        Provider::GitHub
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            provider: Provider::GitHub,
            votes: vec![Vote::Approve, Vote::Reject, Vote::Comment],
            strategies: vec![
                MergeStrategy::Merge,
                MergeStrategy::Squash,
                MergeStrategy::Rebase,
            ],
            auto_complete: false,
            resolve_threads: true,
            work_items: false,
            draft: true,
        }
    }

    async fn current_user(&self) -> HostingResult<PrUser> {
        let op = self.op().await?;
        let u: GhUser = self
            .get(&op, build_url(&self.base, &["user"], &[])?)
            .await?;
        Ok(user(&u))
    }

    async fn list(&self, q: &ListQuery) -> HostingResult<PullRequestPage> {
        let op = self.op().await?;
        let per = q.per_page.clamp(1, 100).to_string();
        let page = q.page.max(1);
        let page_s = page.to_string();
        let state = if q.filter == PrFilter::Closed {
            "closed"
        } else {
            "open"
        };
        let me = match q.filter {
            PrFilter::Mine | PrFilter::Reviewing => {
                let u: GhUser = self
                    .get(&op, build_url(&self.base, &["user"], &[])?)
                    .await?;
                u.login
            }
            _ => None,
        };
        let url = self.repo_url(
            &["pulls"],
            &[
                ("state", state),
                ("sort", "created"),
                ("direction", "desc"),
                ("per_page", &per),
                ("page", &page_s),
            ],
        )?;
        let reply = self.call(&op, Method::GET, url, None).await?;
        let has_more = next_link(&reply.headers).is_some();
        let prs: Vec<GhPr> = reply.json()?;
        let items = prs
            .iter()
            .filter(|p| match (q.filter, me.as_deref()) {
                (PrFilter::Mine, Some(me)) => p
                    .user
                    .as_ref()
                    .and_then(|u| u.login.as_deref())
                    .is_some_and(|l| l.eq_ignore_ascii_case(me)),
                (PrFilter::Reviewing, Some(me)) => p.requested_reviewers.iter().any(|u| {
                    u.login
                        .as_deref()
                        .is_some_and(|l| l.eq_ignore_ascii_case(me))
                }),
                _ => true,
            })
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
        let mut pr = self.pr(&op, id).await?;
        // `mergeable` llega `null` mientras GitHub lo calcula (comprobado: la primera lectura
        // tras un cambio en la rama de destino). Se reintenta un par de veces.
        for attempt in 0..2u32 {
            if pr.mergeable.is_some() || state_of(&pr) != PrState::Active {
                break;
            }
            tokio::time::sleep(self.http.config().backoff_base * (attempt + 2)).await;
            pr = self.pr(&op, id).await?;
        }
        self.build_detail(&op, pr).await
    }

    async fn create(&self, new: &NewPullRequest) -> HostingResult<PullRequestDetail> {
        Self::check_text("El título", &new.title, 256)?;
        check_branch(&new.source_branch, "de origen")?;
        check_branch(&new.target_branch, "de destino")?;
        if new.description.chars().count() > 65_536 {
            return Err(HostingError::invalid(
                "GitHub limita la descripción a 65 536 caracteres.",
            ));
        }
        let reviewers: Vec<&str> = new
            .reviewers
            .iter()
            .map(|r| r.trim())
            .filter(|r| !r.is_empty())
            .collect();
        if reviewers.iter().any(|r| {
            r.len() > 60
                || !r
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '[' | ']'))
        }) {
            return Err(HostingError::invalid("Un revisor no es un usuario válido."));
        }
        let op = self.op().await?;
        let pr: GhPr = self
            .call(
                &op,
                Method::POST,
                self.repo_url(&["pulls"], &[])?,
                Some(&json!({
                    "title": new.title.trim(),
                    "head": new.source_branch.trim(),
                    "base": new.target_branch.trim(),
                    "body": new.description,
                    "draft": new.draft,
                })),
            )
            .await?
            .json()?;
        // La PR ya existe: si GitHub rechaza a un revisor (p. ej. la propia autora, 422) no se
        // devuelve un error —el usuario reintentaría y obtendría «ya existe»—, se anota.
        let mut note = None;
        if !reviewers.is_empty() {
            let asked = self
                .call(
                    &op,
                    Method::POST,
                    self.pull_url(pr.number, &["requested_reviewers"], &[])?,
                    Some(&json!({ "reviewers": reviewers })),
                )
                .await;
            if let Err(e) = asked {
                note = Some(format!("PR creada; no se pudo pedir la revisión: {e}"));
            }
        }
        let fresh = self.pr(&op, pr.number).await?;
        let mut detail = self.build_detail(&op, fresh).await?;
        if note.is_some() {
            detail.merge_detail = note;
        }
        Ok(detail)
    }

    async fn close(&self, id: u64) -> HostingResult<PullRequestDetail> {
        self.set_state(id, "closed").await
    }

    async fn reopen(&self, id: u64) -> HostingResult<PullRequestDetail> {
        self.set_state(id, "open").await
    }

    async fn diff(&self, id: u64) -> HostingResult<Vec<FilePatch>> {
        let op = self.op().await?;
        let files = self.files(&op, id).await?;
        Ok(files
            .iter()
            .filter_map(|f| {
                let change = file_change(f)?;
                let patch = f.patch.as_deref().filter(|p| !p.is_empty());
                Some(match patch {
                    Some(p) => {
                        let a = if change.status == FileStatus::Added {
                            "/dev/null".to_owned()
                        } else {
                            format!("a/{}", change.old_path.as_deref().unwrap_or(&change.path))
                        };
                        let b = if change.status == FileStatus::Deleted {
                            "/dev/null".to_owned()
                        } else {
                            format!("b/{}", change.path)
                        };
                        let nl = if p.ends_with('\n') { "" } else { "\n" };
                        FilePatch {
                            change,
                            patch: Some(format!("--- {a}\n+++ {b}\n{p}{nl}")),
                            omitted: None,
                        }
                    }
                    None => FilePatch {
                        omitted: Some(if f.changes == 0 {
                            "Sin cambios de contenido (renombrado o cambio de permisos).".into()
                        } else {
                            "GitHub no incluye el parche (binario o demasiado grande).".into()
                        }),
                        change,
                        patch: None,
                    },
                })
            })
            .collect())
    }

    async fn threads(&self, id: u64) -> HostingResult<Vec<ReviewThread>> {
        let op = self.op().await?;
        let mut threads = self.inline_threads(&op, id).await?;
        let general: Vec<GhIssueComment> = self
            .get_all(
                &op,
                self.repo_url(
                    &["issues", &id.to_string(), "comments"],
                    &[("per_page", "100")],
                )?,
            )
            .await?;
        threads.extend(general.iter().map(issue_thread));
        Ok(threads)
    }

    async fn create_thread(&self, id: u64, thread: &NewThread) -> HostingResult<ReviewThread> {
        Self::check_text("El comentario", &thread.body, 65_536)?;
        let op = self.op().await?;
        let Some(a) = &thread.anchor else {
            let c: GhIssueComment = self
                .call(
                    &op,
                    Method::POST,
                    self.repo_url(&["issues", &id.to_string(), "comments"], &[])?,
                    Some(&json!({ "body": thread.body })),
                )
                .await?
                .json()?;
            return Ok(issue_thread(&c));
        };
        let path = safe_path(&a.path)
            .ok_or_else(|| HostingError::invalid("La ruta del comentario no es válida."))?;
        if a.line == 0 {
            return Err(HostingError::invalid(
                "La línea del comentario empieza en 1.",
            ));
        }
        let head = self
            .pr(&op, id)
            .await?
            .head
            .sha
            .ok_or_else(|| HostingError::bad("GitHub no devolvió la cabeza de la PR."))?;
        let created: GhReviewComment = self
            .call(
                &op,
                Method::POST,
                self.pull_url(id, &["comments"], &[])?,
                Some(&json!({
                    "body": thread.body,
                    "commit_id": head,
                    "path": path,
                    "line": a.line,
                    "side": if a.side == DiffSide::Old { "LEFT" } else { "RIGHT" },
                })),
            )
            .await?
            .json()?;
        let wanted = created.id.to_string();
        self.inline_threads(&op, id)
            .await?
            .into_iter()
            .find(|t| t.comments.first().is_some_and(|c| c.id == wanted))
            .ok_or_else(|| HostingError::bad("GitHub no devolvió el hilo recién creado."))
    }

    async fn reply(&self, id: u64, thread_id: &str, body: &str) -> HostingResult<ReviewThread> {
        Self::check_text("El comentario", body, 65_536)?;
        if thread_id.starts_with("issue:") {
            return Err(HostingError::invalid(
                "GitHub no anida los comentarios generales: crea un comentario nuevo.",
            ));
        }
        if !valid_node_id(thread_id) {
            return Err(HostingError::invalid("El hilo no es válido."));
        }
        let op = self.op().await?;
        self.graphql(
            &op,
            "mutation($id:ID!,$body:String!){addPullRequestReviewThreadReply(input:{pullRequestReviewThreadId:$id,body:$body}){comment{id}}}",
            json!({ "id": thread_id, "body": body }),
        )
        .await?;
        self.thread_by_id(&op, id, thread_id).await
    }

    async fn set_thread_resolved(
        &self,
        id: u64,
        thread_id: &str,
        resolved: bool,
    ) -> HostingResult<ReviewThread> {
        if thread_id.starts_with("issue:") {
            return Err(HostingError::invalid(
                "Los comentarios generales de GitHub no se pueden resolver.",
            ));
        }
        if !valid_node_id(thread_id) {
            return Err(HostingError::invalid("El hilo no es válido."));
        }
        let op = self.op().await?;
        let q = if resolved {
            "mutation($id:ID!){resolveReviewThread(input:{threadId:$id}){thread{id isResolved}}}"
        } else {
            "mutation($id:ID!){unresolveReviewThread(input:{threadId:$id}){thread{id isResolved}}}"
        };
        self.graphql(&op, q, json!({ "id": thread_id })).await?;
        self.thread_by_id(&op, id, thread_id).await
    }

    async fn vote(
        &self,
        id: u64,
        vote: Vote,
        comment: Option<&str>,
    ) -> HostingResult<PullRequestDetail> {
        let event = match vote {
            Vote::Approve => "APPROVE",
            Vote::Reject => "REQUEST_CHANGES",
            Vote::Comment => "COMMENT",
            Vote::ApproveWithSuggestions | Vote::WaitForAuthor | Vote::NoVote => {
                return Err(HostingError::invalid(
                    "GitHub solo admite aprobar, solicitar cambios o comentar.",
                ))
            }
        };
        let body = comment.map(str::trim).filter(|c| !c.is_empty());
        if body.is_none() && vote != Vote::Approve {
            return Err(HostingError::invalid(
                "GitHub exige un comentario al solicitar cambios o comentar.",
            ));
        }
        if body.is_some_and(|b| b.chars().count() > 65_536) {
            return Err(HostingError::invalid("El comentario es demasiado largo."));
        }
        let op = self.op().await?;
        let pr = self.pr(&op, id).await?;
        let mut payload = json!({ "event": event });
        if let Some(b) = body {
            payload["body"] = Value::String(b.to_owned());
        }
        if let Some(sha) = &pr.head.sha {
            payload["commit_id"] = Value::String(sha.clone());
        }
        self.call(
            &op,
            Method::POST,
            self.pull_url(id, &["reviews"], &[])?,
            Some(&payload),
        )
        .await?;
        let fresh = self.pr(&op, id).await?;
        self.build_detail(&op, fresh).await
    }

    async fn complete(
        &self,
        id: u64,
        options: &CompleteOptions,
    ) -> HostingResult<PullRequestDetail> {
        let method = match options.strategy {
            MergeStrategy::Merge => "merge",
            MergeStrategy::Squash => "squash",
            MergeStrategy::Rebase => "rebase",
            MergeStrategy::RebaseMerge => {
                return Err(HostingError::invalid(
                    "GitHub no tiene «rebase + merge»: usa merge, squash o rebase.",
                ))
            }
        };
        if options.auto_complete {
            return Err(HostingError::invalid(
                "Ariadna no activa la fusión automática de GitHub.",
            ));
        }
        let op = self.op().await?;
        let pr = self.pr(&op, id).await?;
        let head = pr
            .head
            .sha
            .clone()
            .ok_or_else(|| HostingError::bad("GitHub no devolvió la cabeza de la PR."))?;
        let mut payload = json!({ "merge_method": method, "sha": head });
        if let Some(m) = options
            .commit_message
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
        {
            if m.chars().count() > 65_536 {
                return Err(HostingError::invalid(
                    "El mensaje de fusión es demasiado largo.",
                ));
            }
            payload["commit_title"] = Value::String(first_line(m));
            let rest = m.split_once('\n').map(|x| x.1.trim()).unwrap_or("");
            if !rest.is_empty() {
                payload["commit_message"] = Value::String(rest.to_owned());
            }
        }
        self.call(
            &op,
            Method::PUT,
            self.pull_url(id, &["merge"], &[])?,
            Some(&payload),
        )
        .await?;
        let mut note = None;
        let is_fork = pr.head.repo.as_ref().and_then(|r| r.full_name.as_ref())
            != pr.base.repo.as_ref().and_then(|r| r.full_name.as_ref());
        if options.delete_source_branch && !is_fork {
            if let Some(branch) = pr
                .head
                .name
                .as_deref()
                .filter(|b| check_branch(b, "").is_ok())
            {
                let mut segs = vec!["git", "refs", "heads"];
                segs.extend(branch.split('/'));
                let url = self.repo_url(&segs, &[])?;
                if self.call(&op, Method::DELETE, url, None).await.is_err() {
                    note = Some("fusionada; la rama de origen no se pudo borrar".to_owned());
                }
            }
        }
        let fresh = self.pr(&op, id).await?;
        let mut detail = self.build_detail(&op, fresh).await?;
        if note.is_some() {
            detail.merge_detail = note;
        }
        Ok(detail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_base_de_la_api_sale_del_host() {
        assert_eq!(default_api_base("github.com"), "https://api.github.com");
        assert_eq!(
            default_api_base("git.acme.com"),
            "https://git.acme.com/api/v3"
        );
        assert_eq!(default_api_base("acme.ghe.com"), "https://api.acme.ghe.com");
    }

    fn graphql_for(base: &str) -> String {
        let info = RemoteInfo::parse("https://github.com/o/r");
        let g = GitHub::new(
            &info,
            Arc::new(crate::auth::StaticCredentials::pat("u", "t")),
            ProviderSettings {
                api_base: Some(Url::parse(base).unwrap_or_else(|_| unreachable!())),
                http: crate::http::HttpConfig::default(),
            },
        );
        g.map(|g| g.graphql.to_string()).unwrap_or_default()
    }

    #[test]
    fn graphql_cuelga_de_la_base() {
        assert_eq!(
            graphql_for("https://api.github.com"),
            "https://api.github.com/graphql"
        );
        assert_eq!(
            graphql_for("https://git.acme.com/api/v3"),
            "https://git.acme.com/api/graphql"
        );
    }
}
