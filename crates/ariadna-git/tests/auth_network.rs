//! `GitError::Auth` en fetch, pull y push contra un remoto HTTP que rechaza las credenciales.
#![allow(clippy::unwrap_used)]
mod common;

use ariadna_git::{GitCliEngine, GitError, GitRunner};
use common::http::{closed_port, HttpServer, Mode};
use common::{git_path, TestRepo};

fn engine() -> GitCliEngine {
    GitCliEngine::new(GitRunner::with_path(git_path()).with_config("credential.helper", ""))
}

fn repo_with_origin(url: &str) -> TestRepo {
    let r = TestRepo::with_initial_commit();
    r.git(&["remote", "add", "origin", url]);
    r
}

#[tokio::test]
async fn fetch_pull_y_push_devuelven_auth_con_host_del_remoto() {
    for mode in [Mode::Status(401), Mode::Status(403)] {
        let srv = HttpServer::start(mode);
        let r = repo_with_origin(&srv.url("o/r.git"));
        let e = engine();

        let err = e.fetch(&r.repo, "origin").await.unwrap_err();
        assert!(
            matches!(&err, GitError::Auth { host, hint } if host == "127.0.0.1" && hint.contains("Git Credential Manager")),
            "fetch: {err:?}"
        );
        let err = e
            .pull(&r.repo, "origin", Some("main"), false)
            .await
            .unwrap_err();
        assert!(matches!(err, GitError::Auth { .. }), "pull: {err:?}");
        let err = e
            .push(&r.repo, "origin", Some("main"), false)
            .await
            .unwrap_err();
        assert!(matches!(err, GitError::Auth { .. }), "push: {err:?}");
    }
}

#[tokio::test]
async fn un_fallo_de_red_no_se_confunde_con_auth() {
    let r = repo_with_origin(&format!("http://127.0.0.1:{}/o/r.git", closed_port()));
    let e = engine();
    for res in [
        e.fetch(&r.repo, "origin").await.unwrap_err(),
        e.pull(&r.repo, "origin", Some("main"), false)
            .await
            .unwrap_err(),
        e.push(&r.repo, "origin", Some("main"), false)
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(res, GitError::CommandFailed { .. }), "{res:?}");
    }
    // Un remoto local inexistente tampoco.
    let r = repo_with_origin("/no/existe/x.git");
    let err = engine().fetch(&r.repo, "origin").await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
}

#[tokio::test]
async fn la_url_del_remoto_con_usuario_no_aparece_en_el_error() {
    let srv = HttpServer::start(Mode::Status(401));
    let url = format!("http://alguien@127.0.0.1:{}/o/r.git", srv.port);
    let r = repo_with_origin(&url);
    let err = engine().fetch(&r.repo, "origin").await.unwrap_err();
    let shown = format!("{err} {err:?}");
    assert!(matches!(err, GitError::Auth { .. }), "{shown}");
    assert!(!shown.contains("alguien"), "{shown}");
}
