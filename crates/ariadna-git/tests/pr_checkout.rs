//! Bajar la cabeza de una pull request a una rama local (`checkout_remote_ref`).
//!
//! El caso «fork» se simula con un remoto bare de verdad que publica `refs/pull/1/head`, igual
//! que hace GitHub en el repositorio destino: el ref vive en el remoto de origen aunque el
//! commit venga de otro repositorio. El test contra GitHub real está `#[ignore]`.
#![allow(clippy::unwrap_used)]

mod common;

use std::path::Path;
use std::process::Command;

use ariadna_git::{GitError, RemoteCheckout};
use common::{git_path, TestRepo};
use tempfile::TempDir;

fn git_in(dir: &Path, args: &[&str]) -> String {
    let out = Command::new(git_path())
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} en {} falló: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn rev(dir: &Path, what: &str) -> String {
    git_in(dir, &["rev-parse", what]).trim().to_owned()
}

/// Repo local con `origin` bare y un `refs/pull/1/head` que apunta a un commit «de un fork».
///
/// Devuelve el repo, la carpeta del bare y el sha de la cabeza de la PR.
fn fork_pr() -> (TestRepo, TempDir, String) {
    let work = TestRepo::with_initial_commit();
    let bare = work.with_bare_origin();
    work.git(&["push", "-q", "origin", "main"]);

    // «Fork»: otro clon del mismo bare que publica su rama; el commit acaba en el bare.
    let fork_dir = tempfile::tempdir().unwrap();
    git_in(
        fork_dir.path(),
        &["clone", "-q", &bare.path().to_string_lossy(), "."],
    );
    std::fs::write(fork_dir.path().join("README.md"), b"hola desde el fork\n").unwrap();
    std::fs::write(fork_dir.path().join("nuevo.txt"), b"archivo del fork\n").unwrap();
    git_in(fork_dir.path(), &["add", "-A"]);
    git_in(fork_dir.path(), &["commit", "-q", "-m", "cambio del fork"]);
    git_in(
        fork_dir.path(),
        &["push", "-q", "origin", "HEAD:refs/heads/fork/x"],
    );
    let head = rev(fork_dir.path(), "HEAD");
    // GitHub publica `refs/pull/N/head` en el repositorio destino, también para forks.
    git_in(bare.path(), &["update-ref", "refs/pull/1/head", &head]);
    (work, bare, head)
}

#[tokio::test]
async fn baja_la_rama_de_una_pr_de_fork_y_se_cambia_a_ella() {
    let (work, bare, head) = fork_pr();
    let out: RemoteCheckout = work
        .engine
        .checkout_remote_ref(
            &work.repo,
            "origin",
            "refs/pull/1/head",
            "pr-1-ana-x",
            false,
        )
        .await
        .unwrap();
    assert_eq!(out.branch, "pr-1-ana-x");
    assert_eq!(out.head, head);
    assert!(out.created && !out.updated);
    // HEAD en la rama nueva y el árbol con el contenido del fork.
    assert_eq!(
        git_in(work.path(), &["branch", "--show-current"]).trim(),
        "pr-1-ana-x"
    );
    assert_eq!(rev(work.path(), "HEAD"), head);
    assert_eq!(work.read("nuevo.txt"), b"archivo del fork\n");
    // El bare no se toca: no se crean ramas de seguimiento.
    let tracking = git_in(bare.path(), &["for-each-ref", "refs/heads"]);
    assert!(!tracking.contains("pr-1-ana-x"), "{tracking}");
}

#[tokio::test]
async fn la_segunda_vez_no_hace_nada_nuevo() {
    let (work, _bare, head) = fork_pr();
    work.engine
        .checkout_remote_ref(
            &work.repo,
            "origin",
            "refs/pull/1/head",
            "pr-1-ana-x",
            false,
        )
        .await
        .unwrap();
    let again = work
        .engine
        .checkout_remote_ref(&work.repo, "origin", "refs/pull/1/head", "pr-1-ana-x", true)
        .await
        .unwrap();
    assert_eq!((again.created, again.updated), (false, false));
    assert_eq!(again.head, head);
}

#[tokio::test]
async fn si_la_rama_local_existe_con_otra_base_no_se_toca_sin_permiso() {
    let (work, _bare, head) = fork_pr();
    // Rama local con el mismo nombre pero en el commit de `main`.
    let main = rev(work.path(), "main");
    work.git(&["branch", "pr-1-ana-x", &main]);

    let err = work
        .engine
        .checkout_remote_ref(
            &work.repo,
            "origin",
            "refs/pull/1/head",
            "pr-1-ana-x",
            false,
        )
        .await
        .unwrap_err();
    match err {
        GitError::DivergedLocalBranch { local, remote } => {
            assert_eq!(local, "pr-1-ana-x");
            assert_eq!(remote, "refs/pull/1/head");
        }
        other => panic!("variante inesperada {other:?}"),
    }
    // Sin permiso no se movió nada.
    assert_eq!(rev(work.path(), "refs/heads/pr-1-ana-x"), main);

    let out = work
        .engine
        .checkout_remote_ref(&work.repo, "origin", "refs/pull/1/head", "pr-1-ana-x", true)
        .await
        .unwrap();
    assert!(out.updated && !out.created);
    assert_eq!(rev(work.path(), "HEAD"), head);
    assert_eq!(
        git_in(work.path(), &["branch", "--show-current"]).trim(),
        "pr-1-ana-x"
    );
}

#[tokio::test]
async fn con_cambios_locales_que_chocan_avisa_antes_de_tocar_el_arbol() {
    let (work, _bare, _head) = fork_pr();
    // README.md cambia en la PR y también está modificado aquí sin confirmar.
    work.write("README.md", b"cambio local sin confirmar\n");
    let err = work
        .engine
        .checkout_remote_ref(
            &work.repo,
            "origin",
            "refs/pull/1/head",
            "pr-1-ana-x",
            false,
        )
        .await
        .unwrap_err();
    match err {
        GitError::LocalChangesBlockCheckout { detail } => {
            assert!(detail.contains("README.md"), "{detail}");
        }
        other => panic!("variante inesperada {other:?}"),
    }
    // Sigue en `main` y el cambio local no se ha perdido.
    assert_eq!(
        git_in(work.path(), &["branch", "--show-current"]).trim(),
        "main"
    );
    assert_eq!(work.read("README.md"), b"cambio local sin confirmar\n");
}

#[tokio::test]
async fn una_referencia_que_el_remoto_no_tiene_es_error_tipado() {
    let (work, _bare, _head) = fork_pr();
    let err = work
        .engine
        .checkout_remote_ref(
            &work.repo,
            "origin",
            "refs/pull/9/head",
            "pr-9-ana-x",
            false,
        )
        .await
        .unwrap_err();
    match err {
        GitError::MissingRemoteRef { remote, reference } => {
            assert_eq!(
                (remote.as_str(), reference.as_str()),
                ("origin", "refs/pull/9/head")
            );
        }
        other => panic!("variante inesperada {other:?}"),
    }
    assert!(git_in(work.path(), &["branch", "--list", "pr-9-ana-x"])
        .trim()
        .is_empty());
}

#[tokio::test]
async fn una_referencia_que_no_es_de_refs_se_rechaza_sin_red() {
    let (work, _bare, _head) = fork_pr();
    for bad in ["main", "-x", "refs/../x", "refs/pull/1/head.."] {
        let err = work
            .engine
            .checkout_remote_ref(&work.repo, "origin", bad, "pr-1-ana-x", false)
            .await
            .unwrap_err();
        assert!(
            matches!(err, GitError::InvalidReference(_)),
            "{bad}: {err:?}"
        );
    }
}

/// Contra GitHub real: baja `refs/pull/N/head` del repo del spike.
///
/// `ARIADNA_SPIKE_REPO=propietario/repo` (obligatoria), `ARIADNA_SPIKE_PR` (por defecto 23) y,
/// si hace falta, `ARIADNA_SPIKE_HELPER` con un `credential.helper` de git (p. ej.
/// `!gh.exe auth git-credential` en WSL). Así el token lo entrega el helper por el protocolo
/// estándar: **no** pasa por argv ni se imprime.
#[tokio::test]
#[ignore = "necesita red y credenciales reales (ARIADNA_SPIKE_REPO)"]
async fn github_real_baja_refs_pull_head() {
    let Ok(repo_slug) = std::env::var("ARIADNA_SPIKE_REPO") else {
        eprintln!("ARIADNA_SPIKE_REPO no está definido: test omitido");
        return;
    };
    let pr = std::env::var("ARIADNA_SPIKE_PR")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(23);
    let helper = std::env::var("ARIADNA_SPIKE_HELPER").ok();
    let dir = tempfile::tempdir().unwrap();
    let url = format!("https://github.com/{repo_slug}.git");

    let mut args: Vec<String> = Vec::new();
    if let Some(h) = &helper {
        args.push("-c".into());
        args.push(format!("credential.https://github.com.helper={h}"));
    }
    args.extend([
        "clone".to_string(),
        "--no-single-branch".into(),
        url.clone(),
        ".".into(),
    ]);
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    git_in(dir.path(), &borrowed);

    let mut runner = ariadna_git::GitRunner::with_path(git_path());
    if let Some(h) = &helper {
        runner = runner.with_config("credential.https://github.com.helper", h);
    }
    let engine = ariadna_git::GitCliEngine::new(runner);
    let repo = engine
        .open_repository(dir.path())
        .await
        .expect("abrir el clon");
    let branch = format!("pr-{pr}-spike");
    let reference = format!("refs/pull/{pr}/head");
    let out = engine
        .checkout_remote_ref(&repo, "origin", &reference, &branch, false)
        .await
        .expect("bajar refs/pull/N/head");
    assert_eq!(out.branch, branch);
    assert_eq!(rev(dir.path(), "HEAD"), out.head);
    assert_eq!(
        git_in(dir.path(), &["branch", "--show-current"]).trim(),
        branch
    );
    eprintln!(
        "GitHub real: {url} {reference} → {branch} @ {} ({} archivos frente a origin/HEAD)",
        &out.head[..8.min(out.head.len())],
        git_in(dir.path(), &["diff", "--name-only", "origin/HEAD..HEAD"])
            .lines()
            .count()
    );
}
