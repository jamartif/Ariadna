//! GitHub contra un servidor simulado (`wiremock`) con JSON de ejemplo reconstruido de la
//! documentación de la API REST/GraphQL. **NO VERIFICADO con servidor real**
//! (`specs/rust/spike-pr.md`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use ariadna_hosting::*;
use common::*;
use serde_json::{json, Value};
use wiremock::matchers::{
    body_partial_json, body_string_contains, header, method, path, query_param,
};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PULLS: &str = "/repos/o/r/pulls";

fn make(base: url::Url, cfg: HttpConfig) -> Box<dyn PullRequestProvider> {
    let info = RemoteInfo::parse("https://github.com/o/r.git");
    assert_eq!(info.provider, Provider::GitHub);
    provider_for(
        &info,
        Arc::new(StaticCredentials::pat(USER, TOKEN)),
        ProviderSettings {
            http: cfg,
            api_base: Some(base),
        },
    )
    .expect("proveedor")
}

async fn provider(server: &MockServer, cfg: HttpConfig) -> Box<dyn PullRequestProvider> {
    make(base_url(server, ""), cfg)
}

fn gh_pr(n: u64, state: &str, merged: bool) -> Value {
    json!({
        "number": n,
        "title": format!("PR {n}"),
        "body": "Cuerpo **md**",
        "state": state,
        "draft": false,
        "merged": merged,
        "merged_at": if merged { json!("2026-11-03T10:00:00Z") } else { Value::Null },
        "user": {"login": "ana", "id": 1},
        "head": {"ref": "feature/x", "sha": "head-sha", "repo": {"full_name": "o/r"}},
        "base": {"ref": "main", "sha": "base-sha", "repo": {"full_name": "o/r"}},
        "created_at": "2026-11-01T10:00:00Z",
        "updated_at": "2026-11-02T10:00:00Z",
        "requested_reviewers": [{"login": "beto", "id": 2}],
        "html_url": format!("https://github.com/o/r/pull/{n}"),
        "mergeable": true,
        "mergeable_state": "clean"
    })
}

async fn mount_detail_extras(server: &MockServer, n: u64) {
    ok(
        server,
        "GET",
        &format!("{PULLS}/{n}/reviews"),
        json!([
            {"user": {"login": "beto", "id": 2}, "state": "CHANGES_REQUESTED"},
            {"user": {"login": "beto", "id": 2}, "state": "APPROVED"},
            {"user": {"login": "cris", "id": 3}, "state": "COMMENTED"},
            {"user": {"login": "dani", "id": 4}, "state": "CHANGES_REQUESTED"},
            {"user": {"login": "dani", "id": 4}, "state": "COMMENTED"},
            {"user": {"login": "eli", "id": 5}, "state": "PENDING"}
        ]),
    )
    .await;
    ok(server, "GET", "/repos/o/r/commits/head-sha/check-runs", json!({"total_count": 3, "check_runs": [
        {"name": "build", "status": "completed", "conclusion": "success", "html_url": "https://github.com/o/r/runs/1"},
        {"name": "lint", "status": "completed", "conclusion": "failure"},
        {"name": "e2e", "status": "in_progress", "conclusion": null}
    ]})).await;
    ok(
        server,
        "GET",
        "/repos/o/r/commits/head-sha/status",
        json!({"statuses": [
            {"context": "ci/legacy", "state": "success", "target_url": "https://ci.example/1"}
        ]}),
    )
    .await;
    ok(server, "GET", &format!("{PULLS}/{n}/commits"), json!([
        {"sha": "c1", "commit": {"message": "feat: x\n\ncuerpo", "author": {"name": "Ana", "date": "2026-11-01T09:00:00Z"}}}
    ])).await;
    ok(server, "GET", &format!("{PULLS}/{n}/files"), json!([
        {"filename": "src/a.rs", "status": "modified", "additions": 2, "deletions": 1, "changes": 3,
         "patch": "@@ -1,3 +1,4 @@\n uno\n-dos\n+DOS\n tres\n+cuatro"},
        {"filename": "src/new.rs", "status": "added", "additions": 1, "deletions": 0, "changes": 1, "patch": "@@ -0,0 +1 @@\n+hola\n"},
        {"filename": "src/old.rs", "status": "removed", "additions": 0, "deletions": 1, "changes": 1, "patch": "@@ -1 +0,0 @@\n-adios\n"},
        {"filename": "src/b.rs", "previous_filename": "src/b_old.rs", "status": "renamed", "additions": 0, "deletions": 0, "changes": 0},
        {"filename": "img.png", "status": "modified", "additions": 0, "deletions": 0, "changes": 9}
    ])).await;
}

#[tokio::test]
async fn lista_envia_basic_cabeceras_de_github_y_sigue_link() {
    let server = MockServer::start().await;
    let basic = format!("Basic {BASIC}");
    Mock::given(method("GET"))
        .and(path(PULLS))
        .and(query_param("state", "open"))
        .and(query_param("per_page", "2"))
        .and(query_param("page", "1"))
        .and(header("authorization", basic.as_str()))
        .and(header("accept", "application/vnd.github+json"))
        .and(header("x-github-api-version", "2022-11-28"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    format!("<{}{PULLS}?page=2>; rel=\"next\"", server.uri()).as_str(),
                )
                .set_body_json(json!([gh_pr(1, "open", false), gh_pr(2, "open", false)])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let p = provider(&server, fast()).await;
    let page = p
        .list(&ListQuery {
            filter: PrFilter::Open,
            page: 1,
            per_page: 2,
        })
        .await
        .unwrap();
    assert!(page.has_more);
    assert_eq!(page.items.len(), 2);
    let f = &page.items[0];
    assert_eq!(
        (f.id, f.source_branch.as_str(), f.target_branch.as_str()),
        (1, "feature/x", "main")
    );
    assert_eq!(f.state, PrState::Active);
    assert_eq!(f.merge_status, MergeStatus::Clean);
    assert_eq!(f.reviewers[0].user.login, "beto");
    assert_eq!(f.web_url, "https://github.com/o/r/pull/1");
}

#[tokio::test]
async fn mias_y_revisor_filtran_por_el_usuario() {
    let server = MockServer::start().await;
    ok(&server, "GET", "/user", json!({"login": "Beto", "id": 2})).await;
    let mut mine = gh_pr(3, "open", false);
    mine["user"] = json!({"login": "beto", "id": 2});
    mine["requested_reviewers"] = json!([]);
    ok(
        &server,
        "GET",
        PULLS,
        json!([gh_pr(1, "open", false), mine]),
    )
    .await;
    let p = provider(&server, fast()).await;
    let m = p.list(&ListQuery::new(PrFilter::Mine, 1)).await.unwrap();
    assert_eq!(m.items.iter().map(|i| i.id).collect::<Vec<_>>(), [3]);
    let r = p
        .list(&ListQuery::new(PrFilter::Reviewing, 1))
        .await
        .unwrap();
    assert_eq!(r.items.iter().map(|i| i.id).collect::<Vec<_>>(), [1]);
}

#[tokio::test]
async fn cerradas_distingue_fusionadas_de_cerradas() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PULLS))
        .and(query_param("state", "closed"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([gh_pr(1, "closed", true), gh_pr(2, "closed", false)])),
        )
        .mount(&server)
        .await;
    let p = provider(&server, fast()).await;
    let page = p.list(&ListQuery::new(PrFilter::Closed, 1)).await.unwrap();
    let st: Vec<PrState> = page.items.iter().map(|i| i.state).collect();
    assert_eq!(st, [PrState::Completed, PrState::Abandoned]);
    assert!(!page.has_more);
}

#[tokio::test]
async fn detalle_calcula_votos_comprobaciones_commits_y_archivos() {
    let server = MockServer::start().await;
    ok(
        &server,
        "GET",
        &format!("{PULLS}/4"),
        gh_pr(4, "open", false),
    )
    .await;
    mount_detail_extras(&server, 4).await;
    let p = provider(&server, fast()).await;
    let d = p.detail(4).await.unwrap();
    let vote = |login: &str| {
        d.summary
            .reviewers
            .iter()
            .find(|r| r.user.login == login)
            .map(|r| r.vote)
    };
    // beto: pidió cambios, luego aprobó, y GitHub lo lista aún como solicitado → nueva revisión pendiente.
    assert_eq!(vote("beto"), Some(Vote::NoVote));
    assert_eq!(vote("cris"), Some(Vote::Comment));
    // Un comentario posterior no anula la petición de cambios.
    assert_eq!(vote("dani"), Some(Vote::Reject));
    assert_eq!(vote("eli"), None, "PENDING no cuenta");
    assert_eq!(d.policies.len(), 4);
    assert_eq!(
        d.summary.checks,
        Some(ChecksSummary {
            passing: 2,
            failing: 1,
            pending: 1
        })
    );
    assert_eq!(d.commits[0].subject, "feat: x");
    assert_eq!(d.checkout_ref, "refs/pull/4/head");
    assert!(!d.is_fork);
    assert_eq!(d.files.len(), 5);
    assert_eq!(d.files[3].old_path.as_deref(), Some("src/b_old.rs"));
    assert_eq!(d.files[3].status, FileStatus::Renamed);
    assert_eq!(d.description, "Cuerpo **md**");
}

#[tokio::test]
async fn resumen_es_una_sola_peticion() {
    let server = MockServer::start().await;
    ok(
        &server,
        "GET",
        &format!("{PULLS}/4"),
        gh_pr(4, "open", false),
    )
    .await;
    let s = provider(&server, fast()).await.summary(4).await.unwrap();
    assert_eq!((s.id, s.target_branch.as_str()), (4, "main"));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn detalle_de_pr_de_fork_lo_marca() {
    let server = MockServer::start().await;
    let mut fork = gh_pr(4, "open", false);
    fork["head"]["repo"] = json!({"full_name": "otro/r"});
    ok(&server, "GET", &format!("{PULLS}/4"), fork).await;
    mount_detail_extras(&server, 4).await;
    let d = provider(&server, fast()).await.detail(4).await.unwrap();
    assert!(d.is_fork);
    assert_eq!(d.checkout_ref, "refs/pull/4/head");
}

#[tokio::test]
async fn archivos_paginan_con_link_y_rechazan_un_next_de_otro_host() {
    let server = MockServer::start().await;
    ok(
        &server,
        "GET",
        &format!("{PULLS}/4"),
        gh_pr(4, "open", false),
    )
    .await;
    mount_detail_extras(&server, 4).await;
    // Segunda página de archivos: se sigue mientras el host sea el mismo.
    Mock::given(method("GET"))
        .and(path(format!("{PULLS}/6/files")))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"filename": "dos.rs", "status": "added", "additions": 1, "deletions": 0, "changes": 1, "patch": "@@ -0,0 +1 @@\n+x"}])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{PULLS}/6/files")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", format!("<{}{PULLS}/6/files?page=2>; rel=\"next\"", server.uri()).as_str())
                .set_body_json(json!([{"filename": "uno.rs", "status": "added", "additions": 1, "deletions": 0, "changes": 1, "patch": "@@ -0,0 +1 @@\n+x"}])),
        )
        .mount(&server)
        .await;
    let p = provider(&server, fast()).await;
    let d = p.diff(6).await.unwrap();
    assert_eq!(
        d.iter().map(|f| f.change.path.as_str()).collect::<Vec<_>>(),
        ["uno.rs", "dos.rs"]
    );

    // Un `next` hacia otro host jamás se visita (allí iría el token).
    let evil = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{PULLS}/7/files")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    format!("<{}/robo>; rel=\"next\"", evil.uri()).as_str(),
                )
                .set_body_json(json!([])),
        )
        .mount(&server)
        .await;
    let e = p.diff(7).await.unwrap_err();
    assert_eq!(e.code(), "PARSE");
    assert!(evil.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn diff_compone_cabeceras_y_explica_los_parches_ausentes() {
    let server = MockServer::start().await;
    mount_detail_extras(&server, 4).await;
    let d = provider(&server, fast()).await.diff(4).await.unwrap();
    let a = d[0].patch.as_deref().unwrap();
    assert!(
        a.starts_with("--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,3 +1,4 @@\n"),
        "{a}"
    );
    assert!(a.ends_with("+cuatro\n"), "{a:?}");
    assert!(d[1]
        .patch
        .as_deref()
        .unwrap()
        .starts_with("--- /dev/null\n+++ b/src/new.rs\n"));
    assert!(d[2]
        .patch
        .as_deref()
        .unwrap()
        .starts_with("--- a/src/old.rs\n+++ /dev/null\n"));
    assert!(d[3].patch.is_none() && d[3].omitted.as_deref().unwrap().contains("renombrado"));
    assert!(d[4].patch.is_none() && d[4].omitted.as_deref().unwrap().contains("binario"));
}

#[tokio::test]
async fn crear_manda_head_base_borrador_y_pide_revisores() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(PULLS))
        .and(body_partial_json(json!({"title": "Mi PR", "head": "feature/x", "base": "main", "body": "desc", "draft": true})))
        .respond_with(ResponseTemplate::new(201).set_body_json(gh_pr(12, "open", false)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{PULLS}/12/requested_reviewers")))
        .and(body_partial_json(json!({"reviewers": ["beto"]})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    ok(
        &server,
        "GET",
        &format!("{PULLS}/12"),
        gh_pr(12, "open", false),
    )
    .await;
    mount_detail_extras(&server, 12).await;
    let d = provider(&server, fast())
        .await
        .create(&NewPullRequest {
            title: "Mi PR".into(),
            description: "desc".into(),
            source_branch: "feature/x".into(),
            target_branch: "main".into(),
            draft: true,
            reviewers: vec!["beto".into()],
            work_item_ids: vec![],
        })
        .await
        .unwrap();
    assert_eq!(d.summary.id, 12);
}

#[tokio::test]
async fn crear_rechaza_revisores_raros_y_duplicadas_son_conflicto() {
    let server = MockServer::start().await;
    err(
        &server,
        "POST",
        PULLS,
        422,
        json!({"message": "Validation Failed: A pull request already exists for o:feature/x."}),
    )
    .await;
    let p = provider(&server, fast()).await;
    let mut n = NewPullRequest {
        title: "t".into(),
        description: String::new(),
        source_branch: "feature/x".into(),
        target_branch: "main".into(),
        draft: false,
        reviewers: vec!["bad name!".into()],
        work_item_ids: vec![],
    };
    assert_eq!(p.create(&n).await.unwrap_err().code(), "INVALID_REFERENCE");
    n.reviewers.clear();
    assert_eq!(p.create(&n).await.unwrap_err().code(), "CONFLICT");
}

#[tokio::test]
async fn cerrar_y_reabrir() {
    let server = MockServer::start().await;
    for (state, label, merged) in [("closed", "closed", false), ("open", "open", false)] {
        Mock::given(method("PATCH"))
            .and(path(format!("{PULLS}/5")))
            .and(body_partial_json(json!({"state": state})))
            .respond_with(ResponseTemplate::new(200).set_body_json(gh_pr(5, label, merged)))
            .expect(1)
            .mount(&server)
            .await;
    }
    mount_detail_extras(&server, 5).await;
    let p = provider(&server, fast()).await;
    assert_eq!(p.close(5).await.unwrap().summary.state, PrState::Abandoned);
    assert_eq!(p.reopen(5).await.unwrap().summary.state, PrState::Active);
}

#[tokio::test]
async fn votar_mapea_a_eventos_de_review_y_valida() {
    let server = MockServer::start().await;
    ok(
        &server,
        "GET",
        &format!("{PULLS}/5"),
        gh_pr(5, "open", false),
    )
    .await;
    mount_detail_extras(&server, 5).await;
    for (event, extra) in [
        ("APPROVE", json!({})),
        ("REQUEST_CHANGES", json!({"body": "Arregla X"})),
        ("COMMENT", json!({"body": "Ojo"})),
    ] {
        let mut expected = json!({"event": event, "commit_id": "head-sha"});
        for (k, v) in extra.as_object().unwrap() {
            expected[k] = v.clone();
        }
        Mock::given(method("POST"))
            .and(path(format!("{PULLS}/5/reviews")))
            .and(body_partial_json(expected))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 1})))
            .expect(1)
            .mount(&server)
            .await;
    }
    let p = provider(&server, fast()).await;
    p.vote(5, Vote::Approve, None).await.unwrap();
    p.vote(5, Vote::Reject, Some("Arregla X")).await.unwrap();
    p.vote(5, Vote::Comment, Some("Ojo")).await.unwrap();
    for bad in [
        Vote::ApproveWithSuggestions,
        Vote::WaitForAuthor,
        Vote::NoVote,
    ] {
        assert_eq!(
            p.vote(5, bad, Some("x")).await.unwrap_err().code(),
            "INVALID_REFERENCE"
        );
    }
    assert_eq!(
        p.vote(5, Vote::Reject, None).await.unwrap_err().code(),
        "INVALID_REFERENCE"
    );
}

#[tokio::test]
async fn aprobar_la_propia_pr_es_policy() {
    let server = MockServer::start().await;
    ok(
        &server,
        "GET",
        &format!("{PULLS}/5"),
        gh_pr(5, "open", false),
    )
    .await;
    err(
        &server,
        "POST",
        &format!("{PULLS}/5/reviews"),
        422,
        json!({"message": "Unprocessable Entity: Can not approve your own pull request"}),
    )
    .await;
    assert_eq!(
        provider(&server, fast())
            .await
            .vote(5, Vote::Approve, None)
            .await
            .unwrap_err()
            .code(),
        "POLICY"
    );
}

#[tokio::test]
async fn completar_hace_squash_con_el_sha_y_borra_la_rama() {
    let server = MockServer::start().await;
    ok(
        &server,
        "GET",
        &format!("{PULLS}/5"),
        gh_pr(5, "open", false),
    )
    .await;
    Mock::given(method("PUT"))
        .and(path(format!("{PULLS}/5/merge")))
        .and(body_partial_json(json!({"merge_method": "squash", "sha": "head-sha", "commit_title": "Título", "commit_message": "Detalle"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"merged": true, "sha": "m1"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/repos/o/r/git/refs/heads/feature/x"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    mount_detail_extras(&server, 5).await;
    let d = provider(&server, fast())
        .await
        .complete(
            5,
            &CompleteOptions {
                strategy: MergeStrategy::Squash,
                delete_source_branch: true,
                complete_work_items: false,
                auto_complete: false,
                commit_message: Some("Título\n\nDetalle".into()),
            },
        )
        .await
        .unwrap();
    assert!(d.merge_detail.as_deref() != Some("fusionada; la rama de origen no se pudo borrar"));
}

#[tokio::test]
async fn completar_respeta_405_409_y_estrategias_no_soportadas() {
    let server = MockServer::start().await;
    ok(
        &server,
        "GET",
        &format!("{PULLS}/5"),
        gh_pr(5, "open", false),
    )
    .await;
    let opts = |strategy| CompleteOptions {
        strategy,
        delete_source_branch: false,
        complete_work_items: false,
        auto_complete: false,
        commit_message: None,
    };
    Mock::given(method("PUT"))
        .and(path(format!("{PULLS}/5/merge")))
        .respond_with(
            ResponseTemplate::new(405)
                .set_body_json(json!({"message": "Required status check \"build\" is expected."})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    err(
        &server,
        "PUT",
        &format!("{PULLS}/5/merge"),
        409,
        json!({"message": "Head branch was modified. Review and try the merge again."}),
    )
    .await;
    let p = provider(&server, fast()).await;
    let e = p
        .complete(5, &opts(MergeStrategy::Merge))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "POLICY");
    assert!(e.to_string().contains("Required status check"));
    assert_eq!(
        p.complete(5, &opts(MergeStrategy::Merge))
            .await
            .unwrap_err()
            .code(),
        "CONFLICT"
    );
    assert_eq!(
        p.complete(5, &opts(MergeStrategy::RebaseMerge))
            .await
            .unwrap_err()
            .code(),
        "INVALID_REFERENCE"
    );
    let mut auto = opts(MergeStrategy::Merge);
    auto.auto_complete = true;
    assert_eq!(
        p.complete(5, &auto).await.unwrap_err().code(),
        "INVALID_REFERENCE"
    );
}

fn gql_threads(resolved: bool) -> Value {
    json!({"data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": [
        {"id": "PRRT_abc", "isResolved": resolved, "isOutdated": false, "path": "src/a.rs", "line": 10, "originalLine": 10, "diffSide": "RIGHT",
         "viewerCanResolve": true, "viewerCanUnresolve": true,
         "comments": {"nodes": [
            {"databaseId": 501, "body": "Revisa esto", "createdAt": "2026-11-02T10:00:00Z", "author": {"login": "ana"}},
            {"databaseId": 502, "body": "Hecho", "createdAt": "2026-11-02T11:00:00Z", "author": {"login": "beto"}}
         ]}},
        {"id": "PRRT_old", "isResolved": false, "isOutdated": true, "path": "src/b.rs", "line": null, "originalLine": 4, "diffSide": "LEFT",
         "viewerCanResolve": false, "viewerCanUnresolve": false,
         "comments": {"nodes": [{"databaseId": 600, "body": "Viejo", "author": null}]}}
    ]}}}}})
}

async fn mount_graphql_threads(server: &MockServer, resolved: bool) {
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("reviewThreads"))
        .respond_with(ResponseTemplate::new(200).set_body_json(gql_threads(resolved)))
        .mount(server)
        .await;
}

#[tokio::test]
async fn hilos_juntan_graphql_y_comentarios_generales() {
    let server = MockServer::start().await;
    mount_graphql_threads(&server, false).await;
    ok(&server, "GET", "/repos/o/r/issues/5/comments", json!([
        {"id": 900, "body": "Comentario general", "user": {"login": "cris", "id": 3}, "created_at": "2026-11-02T12:00:00Z"}
    ])).await;
    let t = provider(&server, fast()).await.threads(5).await.unwrap();
    assert_eq!(t.len(), 3);
    assert_eq!(t[0].id, "PRRT_abc");
    assert_eq!(
        t[0].anchor
            .as_ref()
            .map(|a| (a.path.as_str(), a.line, a.side)),
        Some(("src/a.rs", 10, DiffSide::New))
    );
    assert_eq!(t[0].comments.len(), 2);
    assert!(t[0].can_resolve);
    assert!(t[1].outdated);
    assert_eq!(
        t[1].anchor.as_ref().map(|a| (a.line, a.side)),
        Some((4, DiffSide::Old))
    );
    assert_eq!(t[2].id, "issue:900");
    assert!(t[2].anchor.is_none() && !t[2].can_resolve);
}

#[tokio::test]
async fn crear_hilo_en_linea_general_responder_y_resolver() {
    let server = MockServer::start().await;
    ok(
        &server,
        "GET",
        &format!("{PULLS}/5"),
        gh_pr(5, "open", false),
    )
    .await;
    Mock::given(method("POST"))
        .and(path(format!("{PULLS}/5/comments")))
        .and(body_partial_json(json!({"body": "Mira", "commit_id": "head-sha", "path": "src/a.rs", "line": 10, "side": "RIGHT"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": 501})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/o/r/issues/5/comments"))
        .and(body_partial_json(json!({"body": "General"})))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(
                json!({"id": 901, "body": "General", "user": {"login": "yo", "id": 9}}),
            ),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("addPullRequestReviewThreadReply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"data": {"addPullRequestReviewThreadReply": {"comment": {"id": "c"}}}}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("resolveReviewThread"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"resolveReviewThread": {"thread": {"id": "PRRT_abc", "isResolved": true}}}})))
        .expect(1)
        .mount(&server)
        .await;
    mount_graphql_threads(&server, true).await;
    let p = provider(&server, fast()).await;
    let t = p
        .create_thread(
            5,
            &NewThread {
                body: "Mira".into(),
                anchor: Some(ThreadAnchor {
                    path: "src/a.rs".into(),
                    line: 10,
                    side: DiffSide::New,
                }),
            },
        )
        .await
        .unwrap();
    assert_eq!(t.id, "PRRT_abc");
    let g = p
        .create_thread(
            5,
            &NewThread {
                body: "General".into(),
                anchor: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(g.id, "issue:901");
    assert_eq!(
        p.reply(5, "PRRT_abc", "Gracias").await.unwrap().id,
        "PRRT_abc"
    );
    assert_eq!(
        p.set_thread_resolved(5, "PRRT_abc", true)
            .await
            .unwrap()
            .status,
        ThreadStatus::Resolved
    );
    // Lo que GitHub no soporta se rechaza sin llamar.
    assert_eq!(
        p.reply(5, "issue:901", "x").await.unwrap_err().code(),
        "INVALID_REFERENCE"
    );
    assert_eq!(
        p.set_thread_resolved(5, "issue:901", true)
            .await
            .unwrap_err()
            .code(),
        "INVALID_REFERENCE"
    );
    assert_eq!(
        p.reply(5, "a\"; drop", "x").await.unwrap_err().code(),
        "INVALID_REFERENCE"
    );
}

#[tokio::test]
async fn errores_graphql_se_tipan() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"errors": [{"type": "FORBIDDEN", "message": "Resource not accessible by personal access token"}]})))
        .mount(&server)
        .await;
    ok(&server, "GET", "/repos/o/r/issues/5/comments", json!([])).await;
    let e = provider(&server, fast())
        .await
        .threads(5)
        .await
        .unwrap_err();
    assert_eq!(e.code(), "AUTH");
}

// --- errores ---------------------------------------------------------------------------------

#[tokio::test]
async fn errores_http_se_tipan() {
    let ratelimit = ResponseTemplate::new(403)
        .insert_header("x-ratelimit-remaining", "0")
        .insert_header("retry-after", "1")
        .set_body_json(json!({"message": "API rate limit exceeded for user ID 1."}));
    let cases: Vec<(ResponseTemplate, &str)> = vec![
        (
            ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})),
            "AUTH",
        ),
        (
            ResponseTemplate::new(403)
                .insert_header("x-accepted-github-permissions", "pull_requests=write")
                .set_body_json(
                    json!({"message": "Resource not accessible by personal access token"}),
                ),
            "AUTH",
        ),
        (
            ResponseTemplate::new(403)
                .insert_header(
                    "x-github-sso",
                    "required; url=https://github.com/orgs/o/sso",
                )
                .set_body_json(
                    json!({"message": "Resource protected by organization SAML enforcement."}),
                ),
            "AUTH",
        ),
        (
            ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})),
            "NOT_FOUND",
        ),
        (
            ResponseTemplate::new(409).set_body_json(json!({"message": "x"})),
            "CONFLICT",
        ),
        (
            ResponseTemplate::new(429).insert_header("retry-after", "1"),
            "RATE_LIMITED",
        ),
        (ratelimit, "RATE_LIMITED"),
        (ResponseTemplate::new(502), "NETWORK"),
    ];
    for (tpl, code) in cases {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("{PULLS}/1")))
            .respond_with(tpl)
            .mount(&server)
            .await;
        let cfg = HttpConfig {
            max_retries: 0,
            ..fast()
        };
        let e = provider(&server, cfg).await.detail(1).await.unwrap_err();
        assert_eq!(e.code(), code, "{e}");
        if code == "RATE_LIMITED" {
            assert_eq!(e.detail().as_deref(), Some("1"));
        }
    }
}

#[tokio::test]
async fn el_permiso_que_falta_viaja_en_el_detalle() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{PULLS}/1")))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-accepted-github-permissions", "pull_requests=read")
                .set_body_json(
                    json!({"message": "Resource not accessible by personal access token"}),
                ),
        )
        .mount(&server)
        .await;
    let e = provider(&server, fast()).await.detail(1).await.unwrap_err();
    assert_eq!(e.code(), "AUTH");
    assert_eq!(e.detail().as_deref(), Some("pull_requests=read"));
}

#[tokio::test]
async fn limite_secundario_reintenta_y_luego_funciona() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PULLS))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("retry-after", "0")
                .set_body_json(json!({"message": "You have exceeded a secondary rate limit."})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    ok(&server, "GET", PULLS, json!([])).await;
    provider(&server, fast())
        .await
        .list(&ListQuery::new(PrFilter::Open, 1))
        .await
        .unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn respuesta_mal_formada_o_gigante_es_parse() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PULLS))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"no\": \"una lista\"}"))
        .mount(&server)
        .await;
    let e = provider(&server, fast())
        .await
        .list(&ListQuery::new(PrFilter::Open, 1))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "PARSE");

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PULLS))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(format!("[\"{}\"]", "x".repeat(4096))),
        )
        .mount(&server)
        .await;
    let cfg = HttpConfig {
        max_response_bytes: 512,
        ..fast()
    };
    let e = provider(&server, cfg)
        .await
        .list(&ListQuery::new(PrFilter::Open, 1))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "PARSE");
}

#[tokio::test]
async fn el_token_no_aparece_en_ningun_error_ni_debug() {
    let server = MockServer::start().await;
    let echo = format!("Authorization: Basic {BASIC} y el token {TOKEN}");
    err(
        &server,
        "GET",
        &format!("{PULLS}/5"),
        403,
        json!({"message": echo}),
    )
    .await;
    err(
        &server,
        "PATCH",
        &format!("{PULLS}/5"),
        422,
        json!({"message": echo}),
    )
    .await;
    err(&server, "POST", PULLS, 409, json!({"message": echo})).await;
    err(&server, "GET", PULLS, 404, json!({"message": echo})).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"errors": [{"type": "OTHER", "message": echo}]})),
        )
        .mount(&server)
        .await;
    ok(&server, "GET", "/repos/o/r/issues/5/comments", json!([])).await;
    let p = provider(&server, fast()).await;
    let errors = vec![
        p.detail(5).await.unwrap_err(),
        p.close(5).await.unwrap_err(),
        p.list(&ListQuery::new(PrFilter::Open, 1))
            .await
            .unwrap_err(),
        p.threads(5).await.unwrap_err(),
        p.create(&NewPullRequest {
            title: "t".into(),
            description: String::new(),
            source_branch: "a".into(),
            target_branch: "b".into(),
            draft: false,
            reviewers: vec![],
            work_item_ids: vec![],
        })
        .await
        .unwrap_err(),
    ];
    for e in &errors {
        assert_no_secret(&e.to_string());
        assert_no_secret(&format!("{e:?}"));
        assert_no_secret(&e.detail().unwrap_or_default());
    }
    for r in server.received_requests().await.unwrap() {
        assert_no_secret(r.url.as_str());
        assert_no_secret(&String::from_utf8_lossy(&r.body));
    }
}

#[test]
fn la_fabrica_solo_acepta_azure_devops_y_github() {
    let creds = || -> Arc<dyn CredentialSource> { Arc::new(StaticCredentials::pat("u", "t")) };
    for url in [
        "https://gitlab.com/g/r.git",
        "https://bitbucket.org/w/r.git",
        "https://git.example.org/r.git",
        "/ruta/local",
        "",
    ] {
        let e = provider_for(
            &RemoteInfo::parse(url),
            creds(),
            ProviderSettings::default(),
        );
        assert_eq!(
            e.err().map(|e| e.code()),
            Some("INVALID_REFERENCE"),
            "{url}"
        );
    }
    for (url, p) in [
        ("git@github.com:o/r.git", Provider::GitHub),
        (
            "https://org@dev.azure.com/org/proj/_git/repo",
            Provider::AzureDevOps,
        ),
        (
            "https://org.visualstudio.com/proj/_git/repo",
            Provider::AzureDevOps,
        ),
        (
            "git@ssh.dev.azure.com:v3/org/proj/repo",
            Provider::AzureDevOps,
        ),
        ("https://github.acme.com/o/r.git", Provider::GitHub),
    ] {
        let prov = provider_for(
            &RemoteInfo::parse(url),
            creds(),
            ProviderSettings::default(),
        )
        .unwrap_or_else(|e| panic!("{url}: {e}"));
        assert_eq!(prov.provider(), p, "{url}");
    }
}

#[test]
fn las_capacidades_difieren_por_proveedor() {
    let creds = || -> Arc<dyn CredentialSource> { Arc::new(StaticCredentials::pat("u", "t")) };
    let az = provider_for(
        &RemoteInfo::parse("https://dev.azure.com/o/p/_git/r"),
        creds(),
        ProviderSettings::default(),
    )
    .unwrap()
    .capabilities();
    let gh = provider_for(
        &RemoteInfo::parse("https://github.com/o/r"),
        creds(),
        ProviderSettings::default(),
    )
    .unwrap()
    .capabilities();
    assert!(
        az.auto_complete && az.work_items && az.strategies.contains(&MergeStrategy::RebaseMerge)
    );
    assert!(
        !gh.auto_complete && !gh.work_items && !gh.strategies.contains(&MergeStrategy::RebaseMerge)
    );
    assert!(az.votes.contains(&Vote::WaitForAuthor) && gh.votes.contains(&Vote::Comment));
    // Serializa en camelCase para el IPC.
    let v = serde_json::to_value(&gh).unwrap();
    assert_eq!(v["provider"], "github");
    assert_eq!(v["strategies"][0], "merge");
}

// --- correcciones tras el spike REAL (2026-11-11) -----------------------------------------------

/// GitHub real: en los 422 `message` es solo «Validation Failed»/«Unprocessable Entity»; el
/// motivo va en `errors` (cadenas u objetos con `message`, o `{resource, field, code}`).
#[tokio::test]
async fn los_422_reales_se_clasifican_por_el_detalle_de_errors() {
    let server = MockServer::start().await;
    err(
        &server,
        "POST",
        &format!("{PULLS}/5/reviews"),
        422,
        json!({"message": "Unprocessable Entity", "errors": ["Review Can not approve your own pull request"]}),
    )
    .await;
    ok(
        &server,
        "GET",
        &format!("{PULLS}/5"),
        gh_pr(5, "open", false),
    )
    .await;
    err(
        &server,
        "POST",
        PULLS,
        422,
        json!({"message": "Validation Failed", "errors": [{"resource": "Issue", "code": "custom",
            "message": "A pull request already exists for jamartif:spike/feature-a."}]}),
    )
    .await;
    let p = provider(&server, fast()).await;
    let e = p.vote(5, Vote::Approve, None).await.unwrap_err();
    assert_eq!(e.code(), "POLICY", "{e}");
    assert!(e.to_string().contains("approve your own"), "{e}");
    let mut n = NewPullRequest {
        title: "t".into(),
        description: String::new(),
        source_branch: "feature/x".into(),
        target_branch: "main".into(),
        draft: false,
        reviewers: vec![],
        work_item_ids: vec![],
    };
    assert_eq!(p.create(&n).await.unwrap_err().code(), "CONFLICT");

    // Rama inexistente: `{resource, field, code}` sin `message`.
    let server = MockServer::start().await;
    err(
        &server,
        "POST",
        PULLS,
        422,
        json!({"message": "Validation Failed", "errors": [{"resource": "PullRequest", "field": "head", "code": "invalid"}]}),
    )
    .await;
    n.source_branch = "no-existe".into();
    let e = provider(&server, fast())
        .await
        .create(&n)
        .await
        .unwrap_err();
    assert_eq!(e.code(), "INVALID_REFERENCE");
    assert!(e.to_string().contains("head: invalid"), "{e}");
}

/// GitHub real: pedir como revisora a la propia autora da 422 *después* de crear la PR. No debe
/// devolverse un error (la PR existe): se anota en `merge_detail`.
#[tokio::test]
async fn crear_con_revisor_rechazado_devuelve_la_pr_con_una_nota() {
    let server = MockServer::start().await;
    ok(&server, "POST", PULLS, gh_pr(12, "open", false)).await;
    err(
        &server,
        "POST",
        &format!("{PULLS}/12/requested_reviewers"),
        422,
        json!({"message": "Review cannot be requested from pull request author."}),
    )
    .await;
    ok(
        &server,
        "GET",
        &format!("{PULLS}/12"),
        gh_pr(12, "open", false),
    )
    .await;
    mount_detail_extras(&server, 12).await;
    let d = provider(&server, fast())
        .await
        .create(&NewPullRequest {
            title: "t".into(),
            description: String::new(),
            source_branch: "feature/x".into(),
            target_branch: "main".into(),
            draft: false,
            reviewers: vec!["ana".into()],
            work_item_ids: vec![],
        })
        .await
        .unwrap();
    assert_eq!(d.summary.id, 12);
    let note = d.merge_detail.unwrap();
    assert!(
        note.starts_with("PR creada;") && note.contains("author"),
        "{note}"
    );
}

/// GitHub real: `mergeable` es `null` mientras GitHub lo calcula; el detalle reintenta, y un
/// `mergeable_state` «unknown» no se muestra como detalle de fusión.
#[tokio::test]
async fn el_detalle_reintenta_mientras_mergeable_es_null() {
    let server = MockServer::start().await;
    let mut calculando = gh_pr(7, "open", false);
    calculando["mergeable"] = Value::Null;
    calculando["mergeable_state"] = json!("unknown");
    Mock::given(method("GET"))
        .and(path(format!("{PULLS}/7")))
        .respond_with(ResponseTemplate::new(200).set_body_json(calculando))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    let mut dirty = gh_pr(7, "open", false);
    dirty["mergeable"] = json!(false);
    dirty["mergeable_state"] = json!("dirty");
    ok(&server, "GET", &format!("{PULLS}/7"), dirty).await;
    mount_detail_extras(&server, 7).await;
    let d = provider(&server, fast()).await.detail(7).await.unwrap();
    assert_eq!(d.summary.merge_status, MergeStatus::Conflicts);
    assert_eq!(d.merge_detail.as_deref(), Some("dirty"));

    // Una PR fusionada llega con `mergeable_state: unknown`: sin detalle.
    let server = MockServer::start().await;
    let mut merged = gh_pr(8, "closed", true);
    merged["mergeable"] = Value::Null;
    merged["mergeable_state"] = json!("unknown");
    ok(&server, "GET", &format!("{PULLS}/8"), merged).await;
    mount_detail_extras(&server, 8).await;
    let d = provider(&server, fast()).await.detail(8).await.unwrap();
    assert_eq!(d.summary.state, PrState::Completed);
    assert_eq!(d.merge_detail, None);
}
