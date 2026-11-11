//! Pull requests (M8) en el núcleo: remoto elegido, confirmaciones nativas, errores tipados,
//! conversión del diff y plantillas. El proveedor es un doble: **nada toca la red** ni pide
//! credenciales reales.

use std::path::Path;
use std::sync::Mutex;

use ariadna_hosting::{
    Capabilities, CheckStatus, ChecksSummary, CompleteOptions, DiffSide, FilePatch, FileStatus,
    HostingError, HostingResult, ListQuery, MergeStatus, MergeStrategy, NewPullRequest, NewThread,
    PolicyCheck, PrCommit, PrFileChange, PrFilter, PrState, PrUser, Provider, PullRequestDetail,
    PullRequestPage, PullRequestProvider, PullRequestSummary, RemoteInfo, ReviewThread, Reviewer,
    ThreadAnchor, ThreadComment, ThreadStatus, Vote, WorkItemRef,
};
use async_trait::async_trait;
use serde_json::Value;

use super::*;
use crate::core::pr_ops::to_file_diffs;
use crate::core::{ApiError, CommentInput, PrProviderFactory};

const TOKEN: &str = "ghp_NUNCA_DEBE_SALIR_0123456789";

fn user(login: &str) -> PrUser {
    PrUser {
        id: format!("id-{login}"),
        display_name: login.to_uppercase(),
        login: login.to_owned(),
    }
}

fn summary(id: u64, state: PrState) -> PullRequestSummary {
    PullRequestSummary {
        id,
        title: format!("Título {id}"),
        author: user("ana"),
        source_branch: "feature/x".into(),
        target_branch: "main".into(),
        state,
        is_draft: false,
        created_at: "2026-11-01T10:00:00Z".into(),
        updated_at: Some("2026-11-02T10:00:00Z".into()),
        reviewers: vec![Reviewer {
            user: user("beto"),
            vote: Vote::Approve,
            required: true,
        }],
        checks: Some(ChecksSummary {
            passing: 1,
            failing: 0,
            pending: 1,
        }),
        merge_status: MergeStatus::Clean,
        web_url: format!("https://example.test/pr/{id}"),
    }
}

fn change(path: &str, status: FileStatus) -> PrFileChange {
    PrFileChange {
        path: path.into(),
        old_path: None,
        status,
        additions: Some(1),
        deletions: Some(1),
        binary: false,
    }
}

fn detail(id: u64, state: PrState) -> PullRequestDetail {
    PullRequestDetail {
        summary: summary(id, state),
        description: "Descripción **md** <script>alert(1)</script>".into(),
        policies: vec![PolicyCheck {
            name: "Build".into(),
            status: CheckStatus::Pending,
            blocking: Some(true),
            url: None,
        }],
        work_items: vec![WorkItemRef {
            id: 7,
            url: Some("https://example.test/wi/7".into()),
        }],
        commits: vec![PrCommit {
            sha: "abc".into(),
            subject: "feat".into(),
            author: "Ana".into(),
            date: None,
        }],
        files: vec![change("src/a.rs", FileStatus::Modified)],
        source_commit: Some("abc".into()),
        target_commit: None,
        checkout_ref: "refs/pull/1/head".into(),
        is_fork: false,
        auto_complete: false,
        merge_detail: Some("clean".into()),
    }
}

fn thread(id: &str) -> ReviewThread {
    ReviewThread {
        id: id.into(),
        status: ThreadStatus::Active,
        anchor: Some(ThreadAnchor {
            path: "src/a.rs".into(),
            line: 3,
            side: DiffSide::New,
        }),
        outdated: false,
        comments: vec![ThreadComment {
            id: "1".into(),
            author: user("ana"),
            body: "Mira".into(),
            created_at: Some("2026-11-02T10:00:00Z".into()),
        }],
        can_resolve: true,
    }
}

/// Proveedor de mentira: registra las llamadas y devuelve datos fijos.
struct Fake {
    provider: Provider,
    calls: Mutex<Vec<String>>,
    state: Mutex<PrState>,
    fail: Mutex<Option<HostingError>>,
}

impl Fake {
    fn new(provider: Provider) -> Arc<Self> {
        Arc::new(Self {
            provider,
            calls: Mutex::new(Vec::new()),
            state: Mutex::new(PrState::Active),
            fail: Mutex::new(None),
        })
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
    fn rec(&self, what: &str) -> HostingResult<()> {
        self.calls.lock().unwrap().push(what.to_owned());
        match self.fail.lock().unwrap().clone() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
    fn state(&self) -> PrState {
        *self.state.lock().unwrap()
    }
}

#[async_trait]
impl PullRequestProvider for Fake {
    fn provider(&self) -> Provider {
        self.provider
    }
    fn capabilities(&self) -> Capabilities {
        let azure = self.provider == Provider::AzureDevOps;
        Capabilities {
            provider: self.provider,
            votes: if azure {
                vec![
                    Vote::Approve,
                    Vote::ApproveWithSuggestions,
                    Vote::NoVote,
                    Vote::WaitForAuthor,
                    Vote::Reject,
                ]
            } else {
                vec![Vote::Approve, Vote::Reject, Vote::Comment]
            },
            strategies: if azure {
                vec![
                    MergeStrategy::Merge,
                    MergeStrategy::Squash,
                    MergeStrategy::Rebase,
                    MergeStrategy::RebaseMerge,
                ]
            } else {
                vec![
                    MergeStrategy::Merge,
                    MergeStrategy::Squash,
                    MergeStrategy::Rebase,
                ]
            },
            auto_complete: azure,
            resolve_threads: true,
            work_items: azure,
            draft: true,
        }
    }
    async fn current_user(&self) -> HostingResult<PrUser> {
        self.rec("me")?;
        Ok(user("yo"))
    }
    async fn list(&self, q: &ListQuery) -> HostingResult<PullRequestPage> {
        self.rec(&format!("list {:?} p{}", q.filter, q.page))?;
        Ok(PullRequestPage {
            items: vec![summary(1, PrState::Active), summary(2, PrState::Active)],
            page: q.page,
            has_more: true,
        })
    }
    async fn summary(&self, id: u64) -> HostingResult<PullRequestSummary> {
        self.rec(&format!("summary {id}"))?;
        Ok(summary(id, self.state()))
    }
    async fn detail(&self, id: u64) -> HostingResult<PullRequestDetail> {
        self.rec(&format!("detail {id}"))?;
        Ok(detail(id, self.state()))
    }
    async fn create(&self, new: &NewPullRequest) -> HostingResult<PullRequestDetail> {
        self.rec(&format!(
            "create {}→{}",
            new.source_branch, new.target_branch
        ))?;
        Ok(detail(10, PrState::Active))
    }
    async fn close(&self, id: u64) -> HostingResult<PullRequestDetail> {
        self.rec(&format!("close {id}"))?;
        *self.state.lock().unwrap() = PrState::Abandoned;
        Ok(detail(id, PrState::Abandoned))
    }
    async fn reopen(&self, id: u64) -> HostingResult<PullRequestDetail> {
        self.rec(&format!("reopen {id}"))?;
        *self.state.lock().unwrap() = PrState::Active;
        Ok(detail(id, PrState::Active))
    }
    async fn diff(&self, id: u64) -> HostingResult<Vec<FilePatch>> {
        self.rec(&format!("diff {id}"))?;
        Ok(vec![FilePatch {
            change: change("src/a.rs", FileStatus::Modified),
            patch: Some("--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-viejo\n+nuevo\n".into()),
            omitted: None,
        }])
    }
    async fn threads(&self, id: u64) -> HostingResult<Vec<ReviewThread>> {
        self.rec(&format!("threads {id}"))?;
        Ok(vec![thread("t1")])
    }
    async fn create_thread(&self, id: u64, t: &NewThread) -> HostingResult<ReviewThread> {
        self.rec(&format!(
            "create_thread {id} {} {}",
            t.body,
            t.anchor.is_some()
        ))?;
        Ok(thread("t2"))
    }
    async fn reply(&self, id: u64, thread_id: &str, body: &str) -> HostingResult<ReviewThread> {
        self.rec(&format!("reply {id} {thread_id} {body}"))?;
        Ok(thread(thread_id))
    }
    async fn set_thread_resolved(
        &self,
        id: u64,
        thread_id: &str,
        resolved: bool,
    ) -> HostingResult<ReviewThread> {
        self.rec(&format!("resolve {id} {thread_id} {resolved}"))?;
        let mut t = thread(thread_id);
        if resolved {
            t.status = ThreadStatus::Resolved;
        }
        Ok(t)
    }
    async fn vote(
        &self,
        id: u64,
        vote: Vote,
        comment: Option<&str>,
    ) -> HostingResult<PullRequestDetail> {
        self.rec(&format!("vote {id} {vote:?} {}", comment.unwrap_or("-")))?;
        Ok(detail(id, PrState::Active))
    }
    async fn complete(&self, id: u64, o: &CompleteOptions) -> HostingResult<PullRequestDetail> {
        self.rec(&format!(
            "complete {id} {:?} del={}",
            o.strategy, o.delete_source_branch
        ))?;
        *self.state.lock().unwrap() = PrState::Completed;
        Ok(detail(id, PrState::Completed))
    }
}

struct FakeFactory {
    fake: Arc<Fake>,
    seen: Mutex<Vec<(RemoteInfo, std::path::PathBuf)>>,
}

impl PrProviderFactory for FakeFactory {
    fn provider(
        &self,
        info: &RemoteInfo,
        git: &Path,
    ) -> HostingResult<Arc<dyn PullRequestProvider>> {
        self.seen
            .lock()
            .unwrap()
            .push((info.clone(), git.to_path_buf()));
        Ok(self.fake.clone())
    }
}

struct Setup {
    env: Env,
    tab: String,
    fake: Arc<Fake>,
    factory: Arc<FakeFactory>,
    repo: std::path::PathBuf,
}

async fn setup(provider: Provider, remote_url: &str) -> Setup {
    let env = Env::new();
    let repo = env.repo("r");
    env.git(&repo, &["remote", "add", "origin", remote_url]);
    let tab = env.open(&repo).await;
    let fake = Fake::new(provider);
    let factory = Arc::new(FakeFactory {
        fake: fake.clone(),
        seen: Mutex::new(Vec::new()),
    });
    env.core.set_pr_factory(factory.clone());
    Setup {
        env,
        tab,
        fake,
        factory,
        repo,
    }
}

async fn github() -> Setup {
    setup(Provider::GitHub, "git@github.com:acme/app.git").await
}

async fn azure() -> Setup {
    setup(
        Provider::AzureDevOps,
        "https://org@dev.azure.com/org/proj/_git/app",
    )
    .await
}

fn opts(strategy: MergeStrategy) -> CompleteOptions {
    CompleteOptions {
        strategy,
        delete_source_branch: true,
        complete_work_items: false,
        auto_complete: false,
        commit_message: None,
    }
}

// --- soporte y elección del remoto ---------------------------------------------------------

#[tokio::test]
async fn el_soporte_sale_del_remoto_y_no_usa_la_red() {
    let s = github().await;
    let sup = s.env.core.pull_request_support(&s.tab).await.unwrap();
    assert!(sup.supported && sup.reason.is_none());
    assert_eq!(sup.remote.as_deref(), Some("origin"));
    assert_eq!(sup.provider, Provider::GitHub);
    assert_eq!(sup.repo_label, "github.com/acme/app");
    assert!(sup.capabilities.is_some());
    assert!(
        s.fake.calls().is_empty(),
        "preguntar el soporte no llama al proveedor"
    );
    // El `git` que recibe la fábrica es el configurado.
    let seen = s.factory.seen.lock().unwrap();
    assert_eq!(seen[0].0.owner, "acme");
    assert!(seen[0].1.to_string_lossy().contains("git"));
}

#[tokio::test]
async fn gitlab_o_sin_remoto_no_tiene_prs_dentro_de_la_app() {
    let s = setup(Provider::GitHub, "https://gitlab.com/g/r.git").await;
    let sup = s.env.core.pull_request_support(&s.tab).await.unwrap();
    assert!(!sup.supported);
    assert_eq!(sup.provider, Provider::GitLab);
    assert!(sup.reason.as_deref().unwrap().contains("navegador"));
    assert!(sup.capabilities.is_none());
    let e = s
        .env
        .core
        .list_pull_requests(&s.tab, PrFilter::Open, 1)
        .await
        .unwrap_err();
    assert_eq!(e.code, "POLICY");
    assert!(s.fake.calls().is_empty());

    // Sin remotos.
    let env = Env::new();
    let repo = env.repo("sin");
    let tab = env.open(&repo).await;
    assert!(!env.core.pull_request_support(&tab).await.unwrap().supported);
    assert_eq!(
        env.core
            .list_pull_requests(&tab, PrFilter::Open, 1)
            .await
            .unwrap_err()
            .code,
        "POLICY"
    );
    // Pestaña inexistente.
    assert_eq!(
        env.core
            .pull_request_support("nope")
            .await
            .unwrap_err()
            .code,
        "REPO_MISSING"
    );
}

#[tokio::test]
async fn se_prefiere_origin_y_si_no_el_primer_remoto_con_soporte() {
    let env = Env::new();
    let repo = env.repo("r");
    env.git(
        &repo,
        &["remote", "add", "origin", "https://gitlab.com/g/r.git"],
    );
    env.git(
        &repo,
        &[
            "remote",
            "add",
            "azure",
            "https://dev.azure.com/org/proj/_git/app",
        ],
    );
    let tab = env.open(&repo).await;
    let factory = Arc::new(FakeFactory {
        fake: Fake::new(Provider::AzureDevOps),
        seen: Mutex::new(Vec::new()),
    });
    env.core.set_pr_factory(factory);
    let sup = env.core.pull_request_support(&tab).await.unwrap();
    assert!(sup.supported);
    assert_eq!(sup.remote.as_deref(), Some("azure"));
    assert_eq!(sup.provider, Provider::AzureDevOps);
}

// --- lectura y creación --------------------------------------------------------------------

#[tokio::test]
async fn listar_detalle_crear_hilos_y_diff_pasan_por_el_proveedor() {
    let s = github().await;
    let c = &s.env.core;
    let page = c
        .list_pull_requests(&s.tab, PrFilter::Mine, 3)
        .await
        .unwrap();
    assert_eq!((page.page, page.has_more, page.items.len()), (3, true, 2));
    assert_eq!(c.get_pull_request(&s.tab, 5).await.unwrap().summary.id, 5);
    let created = c
        .create_pull_request(
            &s.tab,
            NewPullRequest {
                title: "t".into(),
                description: String::new(),
                source_branch: "feature/x".into(),
                target_branch: "main".into(),
                draft: false,
                reviewers: vec![],
                work_item_ids: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(created.summary.id, 10);
    assert_eq!(c.pull_request_threads(&s.tab, 5).await.unwrap()[0].id, "t1");
    c.add_pull_request_comment(
        &s.tab,
        5,
        CommentInput::New {
            body: "hola".into(),
            anchor: None,
        },
    )
    .await
    .unwrap();
    c.add_pull_request_comment(
        &s.tab,
        5,
        CommentInput::Reply {
            thread_id: "t1".into(),
            body: "ok".into(),
        },
    )
    .await
    .unwrap();
    let t = c
        .resolve_pull_request_thread(&s.tab, 5, "t1", true)
        .await
        .unwrap();
    assert_eq!(t.status, ThreadStatus::Resolved);
    let d = c.pull_request_diff(&s.tab, 5).await.unwrap();
    assert_eq!(d.files.len(), 1);
    assert_eq!(d.diffs.len(), 1);
    assert!(d.omitted.is_empty());
    assert_eq!(
        s.fake.calls(),
        [
            "list Mine p3",
            "detail 5",
            "create feature/x→main",
            "threads 5",
            "create_thread 5 hola false",
            "reply 5 t1 ok",
            "resolve 5 t1 true",
            "diff 5",
        ]
    );
    // Ninguna lectura/creación pregunta.
    assert_eq!(s.env.prompter.count(), 0);
}

#[tokio::test]
async fn comment_input_se_lee_como_lo_manda_la_ui() {
    let new: CommentInput = serde_json::from_value(serde_json::json!({
        "kind": "new", "body": "x",
        "anchor": {"path": "a.rs", "line": 2, "side": "old"}
    }))
    .unwrap();
    assert_eq!(
        new,
        CommentInput::New {
            body: "x".into(),
            anchor: Some(ThreadAnchor {
                path: "a.rs".into(),
                line: 2,
                side: DiffSide::Old
            })
        }
    );
    let general: CommentInput =
        serde_json::from_value(serde_json::json!({"kind": "new", "body": "x"})).unwrap();
    assert!(matches!(general, CommentInput::New { anchor: None, .. }));
    let reply: CommentInput =
        serde_json::from_value(serde_json::json!({"kind": "reply", "threadId": "9", "body": "y"}))
            .unwrap();
    assert_eq!(
        reply,
        CommentInput::Reply {
            thread_id: "9".into(),
            body: "y".into()
        }
    );
    assert!(serde_json::from_value::<CommentInput>(serde_json::json!({"kind": "x"})).is_err());
}

// --- acciones destructivas con confirmación nativa -----------------------------------------

#[tokio::test]
async fn abandonar_pide_confirmacion_que_nombra_repo_pr_y_destino() {
    let s = azure().await;
    s.env.prompter.reject_all();
    let r = s.env.core.abandon_pull_request(&s.tab, 5).await.unwrap();
    assert!(!r.done && r.detail.is_none());
    assert_eq!(s.fake.calls(), ["summary 5"], "cancelar no cambia nada");
    let a = s.env.prompter.last();
    assert!(a.title.contains("Abandonar"), "{a:?}");
    for needle in ["#5", "Título 5", "main", "feature/x"] {
        assert!(a.message.contains(needle), "{needle}: {}", a.message);
    }
    assert!(
        a.detail.contains("dev.azure.com/org/proj/app"),
        "{}",
        a.detail
    );
    assert!(a.detail.contains(&Env::s(&s.repo)));

    s.env.prompter.accept_all();
    let r = s.env.core.abandon_pull_request(&s.tab, 5).await.unwrap();
    assert!(r.done);
    assert_eq!(r.detail.unwrap().summary.state, PrState::Abandoned);
    assert_eq!(s.fake.calls(), ["summary 5", "summary 5", "close 5"]);

    // Ya no está abierta: se explica sin preguntar.
    let asked = s.env.prompter.count();
    let e = s
        .env
        .core
        .abandon_pull_request(&s.tab, 5)
        .await
        .unwrap_err();
    assert_eq!(e.code, "POLICY");
    assert_eq!(s.env.prompter.count(), asked);
}

#[tokio::test]
async fn en_github_abandonar_se_llama_cerrar() {
    let s = github().await;
    s.env.core.abandon_pull_request(&s.tab, 5).await.unwrap();
    assert!(s.env.prompter.last().title.contains("Cerrar"));
}

#[tokio::test]
async fn reabrir_no_pregunta() {
    let s = azure().await;
    let r = s.env.core.reopen_pull_request(&s.tab, 5).await.unwrap();
    assert!(r.done);
    assert_eq!(s.env.prompter.count(), 0);
    assert_eq!(s.fake.calls(), ["reopen 5"]);
}

#[tokio::test]
async fn completar_pide_confirmacion_con_estrategia_y_respeta_el_rechazo() {
    let s = azure().await;
    s.env.prompter.reject_all();
    let r = s
        .env
        .core
        .complete_pull_request(&s.tab, 5, opts(MergeStrategy::Squash))
        .await
        .unwrap();
    assert!(!r.done);
    assert_eq!(s.fake.calls(), ["summary 5"]);
    let a = s.env.prompter.last();
    assert!(a.title.contains("Completar"));
    assert!(
        a.message.contains("«main»") && a.message.contains("#5"),
        "{}",
        a.message
    );
    assert!(a.detail.contains("Estrategia: squash"), "{}", a.detail);
    assert!(a.detail.contains("Borrar la rama de origen: sí"));
    assert!(a.detail.contains("políticas del proveedor"));

    s.env.prompter.accept_all();
    let r = s
        .env
        .core
        .complete_pull_request(&s.tab, 5, opts(MergeStrategy::Squash))
        .await
        .unwrap();
    assert!(r.done);
    assert_eq!(r.detail.unwrap().summary.state, PrState::Completed);
    assert_eq!(s.fake.calls().last().unwrap(), "complete 5 Squash del=true");
}

#[tokio::test]
async fn completar_valida_antes_de_preguntar() {
    let s = github().await;
    let c = &s.env.core;
    // GitHub no tiene «rebase + merge» ni finalización automática.
    let e = c
        .complete_pull_request(&s.tab, 5, opts(MergeStrategy::RebaseMerge))
        .await
        .unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
    let mut auto = opts(MergeStrategy::Merge);
    auto.auto_complete = true;
    let e = c.complete_pull_request(&s.tab, 5, auto).await.unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
    assert_eq!(s.env.prompter.count(), 0);
    assert!(s.fake.calls().is_empty());
    // Una PR ya completada no se vuelve a completar.
    *s.fake.state.lock().unwrap() = PrState::Completed;
    let e = c
        .complete_pull_request(&s.tab, 5, opts(MergeStrategy::Merge))
        .await
        .unwrap_err();
    assert_eq!(e.code, "POLICY");
    assert_eq!(s.env.prompter.count(), 0);
}

#[tokio::test]
async fn el_voto_de_rechazo_pide_confirmacion_y_el_resto_no() {
    let s = azure().await;
    let c = &s.env.core;
    for v in [Vote::Approve, Vote::ApproveWithSuggestions, Vote::NoVote] {
        assert!(c.vote_pull_request(&s.tab, 5, v, None).await.unwrap().done);
    }
    assert_eq!(s.env.prompter.count(), 0);

    s.env.prompter.reject_all();
    for v in [Vote::Reject, Vote::WaitForAuthor] {
        assert!(!c.vote_pull_request(&s.tab, 5, v, None).await.unwrap().done);
    }
    assert_eq!(s.env.prompter.count(), 2);
    assert!(s.env.prompter.last().message.contains("Esperar al autor"));
    assert!(!s
        .fake
        .calls()
        .iter()
        .any(|c| c.contains("Reject") || c.contains("WaitForAuthor")));

    s.env.prompter.accept_all();
    let r = c
        .vote_pull_request(&s.tab, 5, Vote::Reject, Some("falta test".into()))
        .await
        .unwrap();
    assert!(r.done);
    assert_eq!(s.fake.calls().last().unwrap(), "vote 5 Reject falta test");
}

#[tokio::test]
async fn un_voto_que_el_proveedor_no_tiene_se_rechaza_sin_preguntar() {
    let s = github().await;
    let e = s
        .env
        .core
        .vote_pull_request(&s.tab, 5, Vote::WaitForAuthor, None)
        .await
        .unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
    assert_eq!(s.env.prompter.count(), 0);
    assert!(s.fake.calls().is_empty());
}

// --- errores ---------------------------------------------------------------------------------

#[tokio::test]
async fn los_errores_del_proveedor_llegan_tipados_y_sin_el_token() {
    let s = github().await;
    let cases: Vec<(HostingError, &str, Option<&str>)> = vec![
        (
            HostingError::Auth {
                message: "Sin permiso".into(),
                missing_permission: Some("pull_requests=write".into()),
            },
            "AUTH",
            Some("pull_requests=write"),
        ),
        (
            HostingError::NotFound {
                message: "x".into(),
            },
            "NOT_FOUND",
            None,
        ),
        (
            HostingError::Policy {
                message: "x".into(),
            },
            "POLICY",
            None,
        ),
        (
            HostingError::Conflict {
                message: "x".into(),
            },
            "CONFLICT",
            None,
        ),
        (
            HostingError::RateLimited {
                message: "x".into(),
                retry_after_secs: Some(30),
            },
            "RATE_LIMITED",
            Some("30"),
        ),
        (
            HostingError::Network {
                message: "x".into(),
            },
            "NETWORK",
            None,
        ),
        (
            HostingError::Invalid {
                message: "x".into(),
            },
            "INVALID_REFERENCE",
            None,
        ),
        (
            HostingError::BadResponse {
                message: "x".into(),
            },
            "PARSE",
            None,
        ),
        (
            HostingError::Io {
                message: "x".into(),
            },
            "IO",
            None,
        ),
    ];
    for (err, code, detail) in cases {
        *s.fake.fail.lock().unwrap() = Some(err);
        let e = s.env.core.get_pull_request(&s.tab, 1).await.unwrap_err();
        assert_eq!(e.code, code);
        assert_eq!(e.detail.as_deref(), detail);
        assert!(!format!("{e:?}").contains(TOKEN));
        assert!(
            crate::core::error::ALL_CODES.contains(&e.code.as_str()),
            "{}",
            e.code
        );
    }
    // Forma que ve la UI.
    let e = ApiError::from(HostingError::Auth {
        message: "m".into(),
        missing_permission: Some("p".into()),
    });
    assert_eq!(
        serde_json::to_value(&e).unwrap(),
        serde_json::json!({"code": "AUTH", "message": "m", "detail": "p"})
    );
}

// --- diff ----------------------------------------------------------------------------------

fn fp(path: &str, old: Option<&str>, status: FileStatus, patch: Option<&str>) -> FilePatch {
    let mut c = change(path, status);
    c.old_path = old.map(str::to_owned);
    FilePatch {
        change: c,
        patch: patch.map(str::to_owned),
        omitted: patch.is_none().then(|| "Archivo binario.".to_owned()),
    }
}

#[test]
fn el_diff_del_proveedor_se_convierte_al_formato_del_visor() {
    let patches = vec![
        fp(
            "src/a.rs",
            None,
            FileStatus::Modified,
            Some("--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,2 +1,2 @@\n uno\n-dos\n+DOS\n"),
        ),
        fp(
            "nuevo con espacio.txt",
            None,
            FileStatus::Added,
            Some("--- /dev/null\n+++ b/nuevo con espacio.txt\n@@ -0,0 +1 @@\n+hola\n"),
        ),
        fp(
            "viejo.txt",
            None,
            FileStatus::Deleted,
            Some("--- a/viejo.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-adios\n"),
        ),
        fp(
            "dir/b.rs",
            Some("dir/b_antes.rs"),
            FileStatus::Renamed,
            Some("--- a/dir/b_antes.rs\n+++ b/dir/b.rs\n@@ -1 +1 @@\n-x\n+y"),
        ),
        fp("img.png", None, FileStatus::Modified, None),
    ];
    let (diffs, omitted) = to_file_diffs(&patches);
    assert_eq!(diffs.len(), 4, "{diffs:#?}");
    let json = serde_json::to_value(&diffs).unwrap();
    assert_eq!(json[0]["path"], "src/a.rs");
    assert_eq!(json[0]["hunks"][0]["lines"].as_array().unwrap().len(), 3);
    assert_eq!(json[1]["path"], "nuevo con espacio.txt");
    assert_eq!(json[1]["isNewFile"], true);
    assert_eq!(json[2]["isDeletedFile"], true);
    assert_eq!(json[3]["path"], "dir/b.rs");
    assert_eq!(json[3]["oldPath"], "dir/b_antes.rs");
    assert_eq!(omitted.len(), 1);
    assert_eq!(omitted[0].path, "img.png");
    assert_eq!(omitted[0].reason, "Archivo binario.");
}

// --- plantillas ------------------------------------------------------------------------------

#[tokio::test]
async fn la_plantilla_se_busca_segun_el_proveedor() {
    let s = github().await;
    let c = &s.env.core;
    assert_eq!(c.pull_request_template(&s.tab).await.unwrap(), None);
    s.env.write(
        &s.repo,
        ".github/PULL_REQUEST_TEMPLATE.md",
        b"## Qu\xc3\xa9 cambia\n",
    );
    assert_eq!(
        c.pull_request_template(&s.tab).await.unwrap().as_deref(),
        Some("## Qué cambia\n")
    );
    // Una plantilla gigante se ignora.
    s.env.write(
        &s.repo,
        ".github/PULL_REQUEST_TEMPLATE.md",
        &vec![b'x'; 70 * 1024],
    );
    assert_eq!(c.pull_request_template(&s.tab).await.unwrap(), None);
    s.env.write(&s.repo, "pull_request_template.md", b"raiz");
    assert_eq!(
        c.pull_request_template(&s.tab).await.unwrap().as_deref(),
        Some("raiz")
    );

    // Azure DevOps mira `.azuredevops/`, no `.github/`.
    let a = azure().await;
    a.env
        .write(&a.repo, ".github/pull_request_template.md", b"github");
    assert_eq!(
        a.env.core.pull_request_template(&a.tab).await.unwrap(),
        None
    );
    a.env
        .write(&a.repo, ".azuredevops/pull_request_template.md", b"azure");
    assert_eq!(
        a.env
            .core
            .pull_request_template(&a.tab)
            .await
            .unwrap()
            .as_deref(),
        Some("azure")
    );
}

// --- contrato con la UI ------------------------------------------------------------------------

#[tokio::test]
async fn las_formas_serializadas_coinciden_con_los_fixtures_de_la_ui() {
    let s = azure().await;
    let c = &s.env.core;
    let mut fx = PrFixtures::new();
    let mut d = detail(5, PrState::Active);
    d.summary.updated_at = None;
    d.summary.checks = None;
    d.target_commit = Some("def".into());
    d.files.push(PrFileChange {
        path: "src/b.rs".into(),
        old_path: Some("src/b_old.rs".into()),
        status: FileStatus::Renamed,
        additions: None,
        deletions: None,
        binary: true,
    });
    fx.check("pr-detail.json", &d);
    fx.check(
        "pr-page.json",
        &c.list_pull_requests(&s.tab, PrFilter::Open, 1)
            .await
            .unwrap(),
    );
    fx.check(
        "pr-support.json",
        &c.pull_request_support(&s.tab).await.unwrap(),
    );
    let mut threads = c.pull_request_threads(&s.tab, 5).await.unwrap();
    let mut general = thread("issue:9");
    general.anchor = None;
    general.status = ThreadStatus::Closed;
    general.outdated = true;
    general.comments[0].created_at = None;
    threads.push(general);
    fx.check("pr-threads.json", &threads);
    let diff = c.pull_request_diff(&s.tab, 5).await.unwrap();
    fx.check("pr-diff.json", &diff);
    let a = c.reopen_pull_request(&s.tab, 5).await.unwrap();
    fx.check("pr-action.json", &a);
    s.env.prompter.reject_all();
    let declined = c.abandon_pull_request(&s.tab, 5).await.unwrap();
    fx.check("pr-action-declined.json", &declined);
    let g = github().await;
    fx.check(
        "pr-support-github.json",
        &g.env.core.pull_request_support(&g.tab).await.unwrap(),
    );
    fx.finish();
}

/// Como el resto de fixtures del contrato: falla si cambian sin regenerar
/// (`ARIADNA_UPDATE_FIXTURES=1 cargo test -p ariadna-app pull_requests`).
struct PrFixtures {
    update: bool,
    problems: Vec<String>,
}

impl PrFixtures {
    fn new() -> Self {
        Self {
            update: std::env::var("ARIADNA_UPDATE_FIXTURES").is_ok_and(|v| v == "1"),
            problems: Vec::new(),
        }
    }

    fn check<T: serde::Serialize>(&mut self, name: &str, value: &T) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src/lib/api/fixtures-pr");
        let path = dir.join(name);
        let v: Value = serde_json::to_value(value).unwrap();
        let mut text = serde_json::to_string_pretty(&v).unwrap();
        text.push('\n');
        // Los fixtures no deben contener nada que parezca un secreto.
        assert!(!text.contains(TOKEN));
        if self.update {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&path, &text).unwrap();
            return;
        }
        match std::fs::read_to_string(&path) {
            Ok(old) if old.replace("\r\n", "\n") == text => {}
            Ok(_) => self.problems.push(format!("{name}: difiere")),
            Err(_) => self.problems.push(format!("{name}: no existe")),
        }
    }

    fn finish(self) {
        assert!(
            self.problems.is_empty(),
            "Los fixtures de PR no coinciden: {:?}. Regenera con \
             `ARIADNA_UPDATE_FIXTURES=1 cargo test -p ariadna-app pull_requests` y revisa \
             `ui/src/lib/api/fixtures-pr/`.",
            self.problems
        );
    }
}
