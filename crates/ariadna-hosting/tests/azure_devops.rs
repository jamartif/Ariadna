//! Azure DevOps contra un servidor simulado (`wiremock`) con JSON de ejemplo reconstruido de la
//! documentación de la API REST 7.1. **NO VERIFICADO con servidor real** (`specs/rust/spike-pr.md`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use ariadna_hosting::*;
use common::*;
use serde_json::{json, Value};
use wiremock::matchers::{body_partial_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PRS: &str = "/org/proj/_apis/git/repositories/repo/pullrequests";

async fn provider(server: &MockServer, cfg: HttpConfig) -> Box<dyn PullRequestProvider> {
    make(base_url(server, "/org"), cfg)
}

fn make(base: url::Url, cfg: HttpConfig) -> Box<dyn PullRequestProvider> {
    let info = RemoteInfo::parse("https://dev.azure.com/org/proj/_git/repo");
    assert_eq!(info.provider, Provider::AzureDevOps);
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

fn pr_json(id: u64, status: &str) -> Value {
    json!({
        "pullRequestId": id,
        "title": format!("PR {id}"),
        "description": "Descripción **markdown**",
        "status": status,
        "isDraft": false,
        "createdBy": {"id": "u-1", "displayName": "Ana", "uniqueName": "ana@acme.com"},
        "creationDate": "2026-11-01T10:00:00Z",
        "sourceRefName": "refs/heads/feature/x",
        "targetRefName": "refs/heads/main",
        "mergeStatus": "succeeded",
        "reviewers": [
            {"id": "u-2", "displayName": "Beto", "uniqueName": "beto@acme.com", "vote": 10, "isRequired": true},
            {"id": "u-3", "displayName": "Cris", "uniqueName": "cris@acme.com", "vote": -10},
            {"id": "u-4", "displayName": "Dani", "uniqueName": "dani@acme.com", "vote": -5},
            {"id": "u-5", "displayName": "Eli", "uniqueName": "eli@acme.com", "vote": 5},
            {"id": "u-6", "displayName": "Fer", "uniqueName": "fer@acme.com", "vote": 0}
        ],
        "repository": {"id": "r-1", "name": "repo", "project": {"id": "p-1", "name": "proj"}},
        "lastMergeSourceCommit": {"commitId": "aaa111"},
        "lastMergeTargetCommit": {"commitId": "bbb222"}
    })
}

async fn mount_detail_extras(server: &MockServer, id: u64) {
    ok(
        server,
        "GET",
        "/org/proj/_apis/policy/evaluations",
        json!({"value": [
            {"status": "approved", "configuration": {"isBlocking": true, "type": {"displayName": "Minimum number of reviewers"}}},
            {"status": "rejected", "configuration": {"isBlocking": true, "type": {"displayName": "Build"}}},
            {"status": "running", "configuration": {"isBlocking": false, "type": {"displayName": "Status"}}}
        ]}),
    )
    .await;
    ok(
        server,
        "GET",
        &format!("{PRS}/{id}/workitems"),
        json!({"value": [{"id": "123", "url": "https://dev.azure.com/org/_apis/wit/workItems/123"}]}),
    )
    .await;
    ok(
        server,
        "GET",
        &format!("{PRS}/{id}/commits"),
        json!({"value": [{"commitId": "c1", "comment": "Primera línea\n\ncuerpo", "author": {"name": "Ana", "date": "2026-11-01T09:00:00Z"}}]}),
    )
    .await;
    ok(
        server,
        "GET",
        &format!("{PRS}/{id}/iterations"),
        json!({"value": [
            {"id": 1, "sourceRefCommit": {"commitId": "old"}, "commonRefCommit": {"commitId": "base"}},
            {"id": 2, "sourceRefCommit": {"commitId": "aaa111"}, "commonRefCommit": {"commitId": "base"}}
        ]}),
    )
    .await;
    ok(
        server,
        "GET",
        &format!("{PRS}/{id}/iterations/2/changes"),
        json!({"changeEntries": [
            {"changeType": "edit", "item": {"path": "/src/a.rs"}},
            {"changeType": "add", "item": {"path": "/src/new.rs"}},
            {"changeType": "delete", "item": {"path": "/src/old.rs"}},
            {"changeType": "rename", "item": {"path": "/src/b.rs"}, "originalPath": "/src/b_old.rs"},
            {"changeType": "add", "item": {"path": "/src", "isFolder": true}}
        ], "nextSkip": 0, "nextTop": 0}),
    )
    .await;
}

#[tokio::test]
async fn lista_abiertas_envia_basic_y_api_version_y_pagina() {
    let server = MockServer::start().await;
    let basic = format!("Basic {BASIC}");
    // per_page = 2 → se piden 3 para saber si hay más.
    Mock::given(method("GET"))
        .and(path(PRS))
        .and(query_param("api-version", "7.1"))
        .and(query_param("searchCriteria.status", "active"))
        .and(query_param("$top", "3"))
        .and(query_param("$skip", "2"))
        .and(header("authorization", basic.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 3,
            "value": [pr_json(3, "active"), pr_json(4, "active"), pr_json(5, "active")]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let p = provider(&server, fast()).await;
    let page = p
        .list(&ListQuery {
            filter: PrFilter::Open,
            page: 2,
            per_page: 2,
        })
        .await
        .unwrap();
    assert_eq!(page.page, 2);
    assert!(page.has_more);
    assert_eq!(page.items.len(), 2);
    let first = &page.items[0];
    assert_eq!(first.id, 3);
    assert_eq!(first.source_branch, "feature/x");
    assert_eq!(first.target_branch, "main");
    assert_eq!(first.state, PrState::Active);
    assert_eq!(first.merge_status, MergeStatus::Clean);
    assert_eq!(
        first.web_url,
        "https://dev.azure.com/org/proj/_git/repo/pullrequest/3"
    );
    let votes: Vec<Vote> = first.reviewers.iter().map(|r| r.vote).collect();
    assert_eq!(
        votes,
        [
            Vote::Approve,
            Vote::Reject,
            Vote::WaitForAuthor,
            Vote::ApproveWithSuggestions,
            Vote::NoVote
        ]
    );
    assert!(first.reviewers[0].required);
}

#[tokio::test]
async fn ultima_pagina_no_tiene_mas() {
    let server = MockServer::start().await;
    ok(
        &server,
        "GET",
        PRS,
        json!({"value": [pr_json(1, "active")]}),
    )
    .await;
    let p = provider(&server, fast()).await;
    let page = p.list(&ListQuery::new(PrFilter::Open, 1)).await.unwrap();
    assert!(!page.has_more);
    assert_eq!(page.items.len(), 1);
}

#[tokio::test]
async fn mias_y_revisor_usan_el_id_del_usuario_autenticado() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/org/_apis/connectionData"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "authenticatedUser": {"id": "me-guid", "providerDisplayName": "Yo", "properties": {"Account": {"$type": "System.String", "$value": "yo@acme.com"}}}
        })))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(PRS))
        .and(query_param("searchCriteria.creatorId", "me-guid"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"value": [pr_json(7, "active")]})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(PRS))
        .and(query_param("searchCriteria.reviewerId", "me-guid"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"value": []})))
        .expect(1)
        .mount(&server)
        .await;
    let p = provider(&server, fast()).await;
    assert_eq!(
        p.list(&ListQuery::new(PrFilter::Mine, 1))
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(p
        .list(&ListQuery::new(PrFilter::Reviewing, 1))
        .await
        .unwrap()
        .items
        .is_empty());
}

#[tokio::test]
async fn cerradas_pide_todo_y_descarta_las_activas() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PRS))
        .and(query_param("searchCriteria.status", "all"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"value": [
            pr_json(1, "active"), pr_json(2, "completed"), pr_json(3, "abandoned")
        ]})))
        .mount(&server)
        .await;
    let p = provider(&server, fast()).await;
    let page = p.list(&ListQuery::new(PrFilter::Closed, 1)).await.unwrap();
    let states: Vec<PrState> = page.items.iter().map(|i| i.state).collect();
    assert_eq!(states, [PrState::Completed, PrState::Abandoned]);
}

#[tokio::test]
async fn detalle_junta_politicas_work_items_commits_y_archivos() {
    let server = MockServer::start().await;
    ok(&server, "GET", &format!("{PRS}/9"), pr_json(9, "active")).await;
    mount_detail_extras(&server, 9).await;
    let p = provider(&server, fast()).await;
    let d = p.detail(9).await.unwrap();
    assert_eq!(d.description, "Descripción **markdown**");
    assert_eq!(d.summary.id, 9);
    assert_eq!(d.policies.len(), 3);
    assert_eq!(d.policies[1].status, CheckStatus::Failing);
    assert_eq!(d.policies[1].blocking, Some(true));
    assert_eq!(
        d.summary.checks,
        Some(ChecksSummary {
            passing: 1,
            failing: 1,
            pending: 1
        })
    );
    assert_eq!(d.work_items[0].id, 123);
    assert_eq!(d.commits[0].subject, "Primera línea");
    assert_eq!(d.source_commit.as_deref(), Some("aaa111"));
    assert_eq!(d.checkout_ref, "refs/heads/feature/x");
    let files: Vec<(&str, FileStatus)> = d
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.status))
        .collect();
    assert_eq!(
        files,
        [
            ("src/a.rs", FileStatus::Modified),
            ("src/new.rs", FileStatus::Added),
            ("src/old.rs", FileStatus::Deleted),
            ("src/b.rs", FileStatus::Renamed)
        ]
    );
    assert_eq!(d.files[3].old_path.as_deref(), Some("src/b_old.rs"));
    // Sin eco de HTML: la descripción viaja tal cual (Markdown), la interfaz la renderiza.
    assert!(!d.is_fork);
}

#[tokio::test]
async fn resumen_es_una_sola_peticion() {
    let server = MockServer::start().await;
    ok(&server, "GET", &format!("{PRS}/9"), pr_json(9, "active")).await;
    let s = provider(&server, fast()).await.summary(9).await.unwrap();
    assert_eq!((s.id, s.target_branch.as_str()), (9, "main"));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn el_detalle_sobrevive_si_el_token_no_lee_politicas() {
    let server = MockServer::start().await;
    ok(&server, "GET", &format!("{PRS}/9"), pr_json(9, "active")).await;
    err(
        &server,
        "GET",
        "/org/proj/_apis/policy/evaluations",
        403,
        json!({"message": "VS403: sin permiso"}),
    )
    .await;
    err(
        &server,
        "GET",
        &format!("{PRS}/9/workitems"),
        404,
        json!({"message": "nada"}),
    )
    .await;
    ok(
        &server,
        "GET",
        &format!("{PRS}/9/commits"),
        json!({"value": []}),
    )
    .await;
    ok(
        &server,
        "GET",
        &format!("{PRS}/9/iterations"),
        json!({"value": []}),
    )
    .await;
    let p = provider(&server, fast()).await;
    let d = p.detail(9).await.unwrap();
    assert!(d.policies.is_empty() && d.work_items.is_empty() && d.files.is_empty());
}

#[tokio::test]
async fn crear_envia_refs_borrador_revisores_y_work_items() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(PRS))
        .and(body_partial_json(json!({
            "sourceRefName": "refs/heads/feature/x",
            "targetRefName": "refs/heads/main",
            "title": "Mi PR",
            "isDraft": true,
            "reviewers": [{"id": "u-2"}],
            "workItemRefs": [{"id": "123"}]
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(pr_json(11, "active")))
        .expect(1)
        .mount(&server)
        .await;
    ok(
        &server,
        "GET",
        "/org/proj/_apis/policy/evaluations",
        json!({"value": []}),
    )
    .await;
    ok(
        &server,
        "GET",
        &format!("{PRS}/11/workitems"),
        json!({"value": []}),
    )
    .await;
    ok(
        &server,
        "GET",
        &format!("{PRS}/11/commits"),
        json!({"value": []}),
    )
    .await;
    ok(
        &server,
        "GET",
        &format!("{PRS}/11/iterations"),
        json!({"value": []}),
    )
    .await;
    let p = provider(&server, fast()).await;
    let d = p
        .create(&NewPullRequest {
            title: "Mi PR".into(),
            description: "desc".into(),
            source_branch: "feature/x".into(),
            target_branch: "main".into(),
            draft: true,
            reviewers: vec!["u-2".into()],
            work_item_ids: vec![123],
        })
        .await
        .unwrap();
    assert_eq!(d.summary.id, 11);
}

#[tokio::test]
async fn crear_valida_antes_de_llamar() {
    let server = MockServer::start().await; // sin mocks: cualquier llamada falla con 404
    let p = provider(&server, fast()).await;
    let mut n = NewPullRequest {
        title: " ".into(),
        description: String::new(),
        source_branch: "a".into(),
        target_branch: "main".into(),
        draft: false,
        reviewers: vec![],
        work_item_ids: vec![],
    };
    assert_eq!(p.create(&n).await.unwrap_err().code(), "INVALID_REFERENCE");
    n.title = "ok".into();
    n.source_branch = "-malo".into();
    assert_eq!(p.create(&n).await.unwrap_err().code(), "INVALID_REFERENCE");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn crear_duplicada_es_conflicto() {
    let server = MockServer::start().await;
    err(&server, "POST", PRS, 409, json!({"message": "TF401179: An active pull request for the source and target branch already exists.", "typeKey": "GitPullRequestExistsException"})).await;
    let p = provider(&server, fast()).await;
    let e = p
        .create(&NewPullRequest {
            title: "t".into(),
            description: String::new(),
            source_branch: "a".into(),
            target_branch: "main".into(),
            draft: false,
            reviewers: vec![],
            work_item_ids: vec![],
        })
        .await
        .unwrap_err();
    assert_eq!(e.code(), "CONFLICT");
    assert!(e.to_string().contains("TF401179"));
}

#[tokio::test]
async fn abandonar_y_reabrir_cambian_el_estado() {
    let server = MockServer::start().await;
    for (status, label) in [("abandoned", "abandoned"), ("active", "active")] {
        Mock::given(method("PATCH"))
            .and(path(format!("{PRS}/5")))
            .and(body_partial_json(json!({"status": status})))
            .respond_with(ResponseTemplate::new(200).set_body_json(pr_json(5, label)))
            .expect(1)
            .mount(&server)
            .await;
    }
    ok(
        &server,
        "GET",
        "/org/proj/_apis/policy/evaluations",
        json!({"value": []}),
    )
    .await;
    ok(
        &server,
        "GET",
        &format!("{PRS}/5/workitems"),
        json!({"value": []}),
    )
    .await;
    ok(
        &server,
        "GET",
        &format!("{PRS}/5/commits"),
        json!({"value": []}),
    )
    .await;
    ok(
        &server,
        "GET",
        &format!("{PRS}/5/iterations"),
        json!({"value": []}),
    )
    .await;
    let p = provider(&server, fast()).await;
    assert_eq!(p.close(5).await.unwrap().summary.state, PrState::Abandoned);
    assert_eq!(p.reopen(5).await.unwrap().summary.state, PrState::Active);
}

#[tokio::test]
async fn votar_pone_el_voto_del_usuario_autenticado() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/org/_apis/connectionData"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"authenticatedUser": {"id": "me-guid"}})),
        )
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("{PRS}/5/reviewers/me-guid")))
        .and(body_partial_json(json!({"vote": -10})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"vote": -10})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{PRS}/5/threads")))
        .and(body_partial_json(
            json!({"comments": [{"content": "Falta un test"}]}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    ok(&server, "GET", &format!("{PRS}/5"), pr_json(5, "active")).await;
    mount_detail_extras(&server, 5).await;
    let p = provider(&server, fast()).await;
    p.vote(5, Vote::Reject, Some("Falta un test"))
        .await
        .unwrap();
}

#[tokio::test]
async fn voto_solo_comentar_no_existe_en_azure_devops() {
    let server = MockServer::start().await;
    let p = provider(&server, fast()).await;
    let e = p.vote(5, Vote::Comment, None).await.unwrap_err();
    assert_eq!(e.code(), "INVALID_REFERENCE");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn completar_envia_estrategia_commit_de_origen_y_opciones() {
    let server = MockServer::start().await;
    ok(&server, "GET", &format!("{PRS}/5"), pr_json(5, "active")).await;
    Mock::given(method("PATCH"))
        .and(path(format!("{PRS}/5")))
        .and(body_partial_json(json!({
            "status": "completed",
            "lastMergeSourceCommit": {"commitId": "aaa111"},
            "completionOptions": {"mergeStrategy": "squash", "deleteSourceBranch": true, "transitionWorkItems": true, "mergeCommitMessage": "Mensaje"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(pr_json(5, "completed")))
        .expect(1)
        .mount(&server)
        .await;
    mount_detail_extras(&server, 5).await;
    let p = provider(&server, fast()).await;
    let d = p
        .complete(
            5,
            &CompleteOptions {
                strategy: MergeStrategy::Squash,
                delete_source_branch: true,
                complete_work_items: true,
                auto_complete: false,
                commit_message: Some("Mensaje".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(d.summary.state, PrState::Completed);
}

#[tokio::test]
async fn auto_complete_no_manda_status_completed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/org/_apis/connectionData"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"authenticatedUser": {"id": "me-guid"}})),
        )
        .mount(&server)
        .await;
    ok(&server, "GET", &format!("{PRS}/5"), pr_json(5, "active")).await;
    let mut with_auto = pr_json(5, "active");
    with_auto["autoCompleteSetBy"] = json!({"id": "me-guid"});
    Mock::given(method("PATCH"))
        .and(path(format!("{PRS}/5")))
        .and(body_partial_json(json!({"autoCompleteSetBy": {"id": "me-guid"}, "completionOptions": {"mergeStrategy": "rebaseMerge"}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(with_auto))
        .expect(1)
        .mount(&server)
        .await;
    mount_detail_extras(&server, 5).await;
    let p = provider(&server, fast()).await;
    let d = p
        .complete(
            5,
            &CompleteOptions {
                strategy: MergeStrategy::RebaseMerge,
                delete_source_branch: false,
                complete_work_items: false,
                auto_complete: true,
                commit_message: None,
            },
        )
        .await
        .unwrap();
    assert!(d.auto_complete);
    let reqs = server.received_requests().await.unwrap();
    let patch = reqs.iter().find(|r| r.method.as_str() == "PATCH").unwrap();
    let body: Value = serde_json::from_slice(&patch.body).unwrap();
    assert!(body.get("status").is_none(), "{body}");
}

#[tokio::test]
async fn politica_incumplida_al_completar_es_policy() {
    let server = MockServer::start().await;
    ok(&server, "GET", &format!("{PRS}/5"), pr_json(5, "active")).await;
    err(&server, "PATCH", &format!("{PRS}/5"), 400, json!({"message": "TF402455: Pushes to this branch are not permitted; you must use a pull request. Required policy not satisfied.", "typeKey": "GitPullRequestPolicyException"})).await;
    let p = provider(&server, fast()).await;
    let e = p
        .complete(
            5,
            &CompleteOptions {
                strategy: MergeStrategy::Merge,
                delete_source_branch: false,
                complete_work_items: false,
                auto_complete: false,
                commit_message: None,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), "POLICY");
    assert!(e.to_string().contains("TF402455"));
}

#[tokio::test]
async fn hilos_filtran_sistema_borrados_y_mapean_estado_y_ancla() {
    let server = MockServer::start().await;
    ok(&server, "GET", &format!("{PRS}/5/threads"), json!({"value": [
        {"id": 1, "status": "active", "threadContext": {"filePath": "/src/a.rs", "rightFileStart": {"line": 10, "offset": 1}},
         "comments": [{"id": 1, "author": {"id": "u-1", "displayName": "Ana", "uniqueName": "ana@acme.com"}, "content": "Revisa esto", "publishedDate": "2026-11-02T10:00:00Z", "commentType": "text"},
                      {"id": 2, "author": {"displayName": "Beto"}, "content": "Hecho", "commentType": "text"}]},
        {"id": 2, "status": "fixed", "threadContext": {"filePath": "/src/b.rs", "leftFileStart": {"line": 4, "offset": 1}},
         "comments": [{"id": 1, "author": {"displayName": "Ana"}, "content": "Viejo", "commentType": "text"}]},
        {"id": 3, "status": "active", "comments": [{"id": 1, "author": {"displayName": "Sistema"}, "content": "Ana votó", "commentType": "system"}]},
        {"id": 4, "status": "active", "isDeleted": true, "comments": [{"id": 1, "content": "borrado", "commentType": "text"}]},
        {"id": 5, "status": "wontFix", "comments": [{"id": 1, "author": {"displayName": "Ana"}, "content": "General", "commentType": "text"}]}
    ]})).await;
    let p = provider(&server, fast()).await;
    let t = p.threads(5).await.unwrap();
    assert_eq!(t.len(), 3);
    assert_eq!(
        t[0].anchor
            .as_ref()
            .map(|a| (a.path.as_str(), a.line, a.side)),
        Some(("src/a.rs", 10, DiffSide::New))
    );
    assert_eq!(t[0].comments.len(), 2);
    assert_eq!(t[1].status, ThreadStatus::Resolved);
    assert_eq!(t[1].anchor.as_ref().map(|a| a.side), Some(DiffSide::Old));
    assert_eq!(t[2].status, ThreadStatus::Closed);
    assert!(t[2].anchor.is_none());
}

#[tokio::test]
async fn crear_responder_y_resolver_hilos() {
    let server = MockServer::start().await;
    let thread = |status: &str| {
        json!({"id": 8, "status": status, "threadContext": {"filePath": "/src/a.rs", "rightFileStart": {"line": 3, "offset": 1}},
               "comments": [{"id": 1, "author": {"displayName": "Yo"}, "content": "Hola", "commentType": "text"}]})
    };
    Mock::given(method("POST"))
        .and(path(format!("{PRS}/5/threads")))
        .and(body_partial_json(json!({"threadContext": {"filePath": "/src/a.rs", "rightFileStart": {"line": 3}, "rightFileEnd": {"line": 3}}, "comments": [{"content": "Hola"}]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(thread("active")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{PRS}/5/threads/8/comments")))
        .and(body_partial_json(
            json!({"content": "Respuesta", "parentCommentId": 1}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 2})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{PRS}/5/threads/8")))
        .respond_with(ResponseTemplate::new(200).set_body_json(thread("active")))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!("{PRS}/5/threads/8")))
        .and(body_partial_json(json!({"status": "fixed"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(thread("fixed")))
        .expect(1)
        .mount(&server)
        .await;
    let p = provider(&server, fast()).await;
    let created = p
        .create_thread(
            5,
            &NewThread {
                body: "Hola".into(),
                anchor: Some(ThreadAnchor {
                    path: "src/a.rs".into(),
                    line: 3,
                    side: DiffSide::New,
                }),
            },
        )
        .await
        .unwrap();
    assert_eq!(created.id, "8");
    assert_eq!(p.reply(5, "8", "Respuesta").await.unwrap().id, "8");
    assert_eq!(
        p.set_thread_resolved(5, "8", true).await.unwrap().status,
        ThreadStatus::Resolved
    );
    // Un id de hilo que no es numérico no llega al servidor.
    assert_eq!(
        p.reply(5, "8/../x", "x").await.unwrap_err().code(),
        "INVALID_REFERENCE"
    );
}

#[tokio::test]
async fn diff_genera_parches_unificados_desde_las_versiones() {
    let server = MockServer::start().await;
    ok(&server, "GET", &format!("{PRS}/5/iterations"), json!({"value": [
        {"id": 1, "sourceRefCommit": {"commitId": "head1"}, "commonRefCommit": {"commitId": "base1"}}
    ]})).await;
    ok(
        &server,
        "GET",
        &format!("{PRS}/5/iterations/1/changes"),
        json!({"changeEntries": [
            {"changeType": "edit", "item": {"path": "/a.txt"}},
            {"changeType": "add", "item": {"path": "/n.txt"}},
            {"changeType": "edit", "item": {"path": "/bin.dat"}},
            {"changeType": "edit", "item": {"path": "/enorme.txt"}}
        ]}),
    )
    .await;
    let item = |p: &'static str, v: &'static str, body: Vec<u8>| {
        Mock::given(method("GET"))
            .and(path("/org/proj/_apis/git/repositories/repo/items"))
            .and(query_param("path", p))
            .and(query_param("versionDescriptor.version", v))
            .and(query_param("versionDescriptor.versionType", "commit"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
    };
    item("/a.txt", "base1", b"uno\ndos\ntres\n".to_vec())
        .mount(&server)
        .await;
    item("/a.txt", "head1", b"uno\nDOS\ntres\ncuatro\n".to_vec())
        .mount(&server)
        .await;
    item("/n.txt", "head1", b"hola\n".to_vec())
        .mount(&server)
        .await;
    item("/bin.dat", "base1", vec![0, 1, 2])
        .mount(&server)
        .await;
    item("/bin.dat", "head1", vec![0, 1, 3])
        .mount(&server)
        .await;
    item("/enorme.txt", "base1", vec![b'a'; 1024 * 1024 + 10])
        .mount(&server)
        .await;
    item("/enorme.txt", "head1", b"x\n".to_vec())
        .mount(&server)
        .await;
    let p = provider(&server, fast()).await;
    let d = p.diff(5).await.unwrap();
    assert_eq!(d.len(), 4);
    let a = d[0].patch.as_deref().unwrap();
    assert!(a.starts_with("--- a/a.txt\n+++ b/a.txt\n@@ "), "{a}");
    assert!(a.contains("-dos\n+DOS\n") && a.contains("+cuatro"), "{a}");
    assert_eq!(
        (d[0].change.additions, d[0].change.deletions),
        (Some(2), Some(1))
    );
    let n = d[1].patch.as_deref().unwrap();
    assert!(n.starts_with("--- /dev/null\n+++ b/n.txt\n"), "{n}");
    assert!(d[2].change.binary && d[2].patch.is_none());
    assert!(d[3]
        .omitted
        .as_deref()
        .unwrap()
        .contains("demasiado grande"));
}

// --- errores ---------------------------------------------------------------------------------

#[tokio::test]
async fn errores_http_se_tipan() {
    let cases: Vec<(u16, Value, &str)> = vec![
        (401, json!({"message": "TF400813"}), "AUTH"),
        (
            403,
            json!({"message": "TF401027: You need the GenericContribute permission to perform this operation. Your current permissions are Read."}),
            "AUTH",
        ),
        (
            404,
            json!({"message": "TF401019: The Git repository with name or identifier repo does not exist", "typeKey": "GitRepositoryNotFoundException"}),
            "NOT_FOUND",
        ),
        (409, json!({"message": "cambió"}), "CONFLICT"),
        (500, json!({"message": "boom"}), "NETWORK"),
    ];
    for (status, body, code) in cases {
        let server = MockServer::start().await;
        err(&server, "GET", &format!("{PRS}/1"), status, body).await;
        let p = provider(&server, fast()).await;
        let e = p.detail(1).await.unwrap_err();
        assert_eq!(e.code(), code, "{status}: {e}");
        if status == 403 {
            assert_eq!(e.detail().as_deref(), Some("GenericContribute"));
        }
    }
}

#[tokio::test]
async fn token_invalido_redirige_o_responde_203_y_es_auth() {
    // 203 con la página de inicio de sesión.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{PRS}/1")))
        .respond_with(ResponseTemplate::new(203).set_body_string("<html>Sign in</html>"))
        .mount(&server)
        .await;
    let e = provider(&server, fast()).await.detail(1).await.unwrap_err();
    assert_eq!(e.code(), "AUTH");
    // Redirección a otro origen: no se sigue (el token no viaja) y se trata como AUTH.
    let server = MockServer::start().await;
    let other = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{PRS}/1")))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/_signin", other.uri())),
        )
        .mount(&server)
        .await;
    let e = provider(&server, fast()).await.detail(1).await.unwrap_err();
    assert_eq!(e.code(), "AUTH");
    assert!(other.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn limite_de_uso_reintenta_con_retry_after_y_luego_funciona() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PRS))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    ok(&server, "GET", PRS, json!({"value": []})).await;
    let p = provider(&server, fast()).await;
    p.list(&ListQuery::new(PrFilter::Open, 1)).await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn limite_de_uso_agotado_devuelve_rate_limited_con_la_espera() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PRS))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
        .mount(&server)
        .await;
    let cfg = HttpConfig {
        max_retries: 1,
        ..fast()
    };
    let e = provider(&server, cfg)
        .await
        .list(&ListQuery::new(PrFilter::Open, 1))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "RATE_LIMITED");
    assert_eq!(e.detail().as_deref(), Some("1"));
    // 1 intento + 1 reintento.
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn retry_after_demasiado_largo_no_se_espera() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PRS))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "600"))
        .mount(&server)
        .await;
    let e = provider(&server, fast())
        .await
        .list(&ListQuery::new(PrFilter::Open, 1))
        .await
        .unwrap_err();
    assert_eq!(e.detail().as_deref(), Some("600"));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn un_503_se_reintenta_en_lecturas_pero_no_en_escrituras() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PRS))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    ok(&server, "GET", PRS, json!({"value": []})).await;
    err(&server, "PATCH", &format!("{PRS}/5"), 503, json!({})).await;
    ok(&server, "GET", &format!("{PRS}/5"), pr_json(5, "active")).await;
    let p = provider(&server, fast()).await;
    p.list(&ListQuery::new(PrFilter::Open, 1)).await.unwrap();
    let e = p.close(5).await.unwrap_err();
    assert_eq!(e.code(), "NETWORK");
    let patches = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str() == "PATCH")
        .count();
    assert_eq!(patches, 1, "una escritura no se reintenta");
}

#[tokio::test]
async fn respuesta_mal_formada_o_gigante_es_parse() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{PRS}/1")))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>no soy JSON</html>"))
        .mount(&server)
        .await;
    let e = provider(&server, fast()).await.detail(1).await.unwrap_err();
    assert_eq!(e.code(), "PARSE");

    let server = MockServer::start().await;
    let big = "x".repeat(8 * 1024);
    Mock::given(method("GET"))
        .and(path(format!("{PRS}/1")))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(format!("{{\"title\":\"{big}\"}}")),
        )
        .mount(&server)
        .await;
    let cfg = HttpConfig {
        max_response_bytes: 1024,
        ..fast()
    };
    let e = provider(&server, cfg).await.detail(1).await.unwrap_err();
    assert_eq!(e.code(), "PARSE");
    assert!(e.to_string().contains("límite"), "{e}");
}

#[tokio::test]
async fn sin_servidor_es_network() {
    // Puerto cerrado: se reserva uno libre y se suelta.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let cfg = HttpConfig {
        max_retries: 1,
        ..fast()
    };
    let p = make(
        url::Url::parse(&format!("http://127.0.0.1:{port}/org")).unwrap(),
        cfg,
    );
    let e = p.detail(1).await.unwrap_err();
    assert_eq!(e.code(), "NETWORK");
}

// --- el token jamás sale -----------------------------------------------------------------------

#[tokio::test]
async fn el_token_no_aparece_en_ningun_error_ni_debug() {
    // Un servidor hostil (o un proxy) que repite la cabecera Authorization y el token en el cuerpo.
    let server = MockServer::start().await;
    let echo = format!("Authorization: Basic {BASIC} token {TOKEN}");
    for (status, m) in [(400u16, "PATCH"), (403, "GET"), (404, "PUT"), (409, "POST")] {
        err(
            &server,
            m,
            &format!("{PRS}/5"),
            status,
            json!({"message": echo, "typeKey": "Policy"}),
        )
        .await;
    }
    err(&server, "GET", PRS, 403, json!({"message": echo})).await;
    err(&server, "POST", PRS, 409, json!({"message": echo})).await;
    let p = provider(&server, fast()).await;
    let mut errors = vec![
        p.detail(5).await.unwrap_err(),
        p.close(5).await.unwrap_err(),
        p.list(&ListQuery::new(PrFilter::Open, 1))
            .await
            .unwrap_err(),
    ];
    errors.push(
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
    );
    for e in &errors {
        assert_no_secret(&e.to_string());
        assert_no_secret(&format!("{e:?}"));
        assert_no_secret(&e.detail().unwrap_or_default());
    }
    // Y la credencial viajó solo en la cabecera: ni en la URL ni en el cuerpo de lo enviado.
    for r in server.received_requests().await.unwrap() {
        assert_no_secret(r.url.as_str());
        assert_no_secret(&String::from_utf8_lossy(&r.body));
    }
}
