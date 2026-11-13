//! Extremo a extremo: sesión + motor real + git real, todo a través del trait `GitServing`.
#![allow(clippy::unwrap_used)]
mod common;

use std::sync::Arc;
use std::time::Duration;

use ariadna_git::{
    repo_key, GitPath, GitServing, IntegrationState, MergeFastForward, MergeResult,
    RepositorySession, SessionState,
};
use common::TestRepo;

fn gp(s: &str) -> GitPath {
    GitPath::new(s.as_bytes().to_vec())
}

#[tokio::test]
async fn flujo_completo_stage_commit_rama_merge_a_traves_de_la_sesion() {
    let t = TestRepo::with_initial_commit();
    let engine: Arc<dyn GitServing> = Arc::new(t.engine.clone());

    // open_repository sube a la raíz desde una subcarpeta.
    std::fs::create_dir_all(t.path().join("sub")).unwrap();
    let repo = engine.open_repository(&t.path().join("sub")).await.unwrap();
    assert_eq!(repo.path, t.repo.path);

    let session = RepositorySession::new(engine.clone(), repo, "s1".into());
    assert_eq!(session.state(), SessionState::Opening);
    session.open().await.unwrap();
    assert_eq!(session.state(), SessionState::Active);
    assert!(session.snapshot().status.entries.is_empty());

    // Cambio en el árbol de trabajo -> aparece tras refrescar.
    t.write("a.txt", b"uno\n");
    session.refresh().await.unwrap();
    assert_eq!(session.snapshot().status.entries.len(), 1);

    // stage + commit por la cola de mutaciones.
    session
        .mutate("stage", |e, r| {
            Box::pin(async move { e.stage(&r, &[gp("a.txt")]).await })
        })
        .await
        .unwrap();
    let sha = session
        .mutate("commit", |e, r| {
            Box::pin(async move { e.commit(&r, "añade a.txt", false).await })
        })
        .await
        .unwrap();
    assert_eq!(sha, t.git(&["rev-parse", "HEAD"]).trim());
    session.refresh().await.unwrap();
    assert!(session.snapshot().status.entries.is_empty());
    assert!(session.revision() >= 2);

    // Rama + commit en ella + vuelta a main + merge (no-ff) sin conflictos.
    session
        .mutate("crear rama", |e, r| {
            Box::pin(async move { e.create_branch(&r, "feature", None).await })
        })
        .await
        .unwrap();
    session
        .mutate("checkout", |e, r| {
            Box::pin(async move { e.checkout(&r, "feature", false).await })
        })
        .await
        .unwrap();
    t.write("b.txt", b"dos\n");
    t.commit_all("añade b.txt");
    session
        .mutate("checkout main", |e, r| {
            Box::pin(async move { e.checkout(&r, "main", false).await })
        })
        .await
        .unwrap();
    let res = session
        .mutate("merge", |e, r| {
            Box::pin(async move { e.merge(&r, "feature", MergeFastForward::Never).await })
        })
        .await
        .unwrap();
    assert!(matches!(res, MergeResult::Merged { .. }), "{res:?}");
    assert_eq!(
        engine
            .integration_state(&session.repository().clone())
            .await
            .unwrap(),
        IntegrationState::None
    );

    let log = engine.log(session.repository(), 10).await.unwrap();
    assert!(log.len() >= 4, "{log:?}");
    let found = engine
        .search_log(session.repository(), "b.txt", 10)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);

    let outcome = session.close(Duration::from_millis(200)).await;
    assert_eq!(session.state(), SessionState::Closed);
    let _ = outcome;
}

#[tokio::test]
async fn la_clave_de_repo_es_la_misma_desde_el_motor_real() {
    let t = TestRepo::with_initial_commit();
    let runner = t.engine.runner().clone();
    let a = repo_key(&runner, t.path()).await.unwrap();
    std::fs::create_dir_all(t.path().join("x/y")).unwrap();
    let b = repo_key(&runner, &t.path().join("x/y")).await.unwrap();
    assert_eq!(a, b);
}

#[tokio::test]
async fn mutaciones_concurrentes_no_se_pisan_con_git_real() {
    // 15 stage+commit lanzados a la vez desde 15 tareas: la cola los serializa y ninguno falla con index.lock.
    let t = TestRepo::with_initial_commit();
    let engine: Arc<dyn GitServing> = Arc::new(t.engine.clone());
    let session = RepositorySession::new(engine, t.repo.clone(), "s2".into());
    session.open().await.unwrap();
    let mut tasks = Vec::new();
    for i in 0..15 {
        let s = session.clone();
        let name = format!("f{i}.txt");
        t.write(&name, format!("{i}\n").as_bytes());
        tasks.push(tokio::spawn(async move {
            let n = name.clone();
            s.mutate("stage", move |e, r| {
                let n = n.clone();
                Box::pin(async move { e.stage(&r, &[gp(&n)]).await })
            })
            .await
        }));
    }
    for h in tasks {
        h.await.unwrap().unwrap();
    }
    session
        .mutate("commit", |e, r| {
            Box::pin(async move { e.commit(&r, "todos", false).await })
        })
        .await
        .unwrap();
    assert_eq!(t.git(&["ls-files"]).lines().count(), 16);
}
