//! Etiquetas contra git de verdad: listar, crear (ligera/anotada), borrar, publicar y borrar en
//! el remoto, validación del nombre y errores tipados.
#![allow(clippy::unwrap_used)]

mod common;

use std::path::Path;
use std::process::Command;

use ariadna_git::{Confirmation, GitError};
use common::{git_path, TestRepo};

fn confirm() -> Confirmation {
    Confirmation::granted("borrar la etiqueta")
}

/// Etiquetas que tiene un repositorio bare (según el propio git).
fn remote_tags(bare: &Path) -> Vec<String> {
    let out = Command::new(git_path())
        .args(["-C", bare.to_str().unwrap(), "tag", "--list"])
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

#[tokio::test]
async fn lists_lightweight_and_annotated_with_message_and_commit() {
    let t = TestRepo::with_initial_commit();
    let first = t.git(&["rev-parse", "HEAD"]).trim().to_owned();
    t.write("a.txt", b"a\n");
    let second = t.commit_all("segundo");
    assert!(t.engine.tags(&t.repo).await.unwrap().is_empty());

    t.git(&["tag", "ligera", &first]);
    t.git(&[
        "tag",
        "-a",
        "-m",
        "Versión 1\n\nCuerpo largo\ncon dos líneas",
        "anotada",
    ]);
    t.git(&["tag", "-a", "-m", "release", "release/2.0", &first]);

    let tags = t.engine.tags(&t.repo).await.unwrap();
    assert_eq!(tags.len(), 3);
    let by = |n: &str| tags.iter().find(|x| x.name == n).unwrap();

    let l = by("ligera");
    assert!(!l.annotated);
    assert_eq!(l.commit, first);
    assert_eq!((l.message.as_deref(), l.tagger.as_deref()), (None, None));
    assert!(!l.date_iso.is_empty());

    let a = by("anotada");
    assert!(a.annotated);
    assert_eq!(
        a.commit, second,
        "el commit se muestra pelado, no el objeto tag"
    );
    assert_eq!(
        a.message.as_deref(),
        Some("Versión 1\n\nCuerpo largo\ncon dos líneas")
    );
    assert_eq!(a.tagger.as_deref(), Some("Test"));

    let r = by("release/2.0");
    assert!(r.annotated);
    assert_eq!(r.commit, first);
}

#[tokio::test]
async fn creates_on_head_and_on_an_older_commit() {
    let t = TestRepo::with_initial_commit();
    let first = t.git(&["rev-parse", "HEAD"]).trim().to_owned();
    t.write("a.txt", b"a\n");
    let second = t.commit_all("segundo");

    // Ligera en HEAD.
    let v1 = t
        .engine
        .create_tag(&t.repo, "v1", None, None)
        .await
        .unwrap();
    assert!(!v1.annotated);
    assert_eq!(v1.commit, second);
    // Mensaje en blanco = ligera.
    let v1b = t
        .engine
        .create_tag(&t.repo, "v1b", None, Some("  \n "))
        .await
        .unwrap();
    assert!(!v1b.annotated);

    // Anotada en un commit del historial (SHA completo y abreviado).
    let old = t
        .engine
        .create_tag(&t.repo, "base", Some(&first), Some("  El inicio  "))
        .await
        .unwrap();
    assert!(old.annotated);
    assert_eq!(old.commit, first);
    assert_eq!(old.message.as_deref(), Some("El inicio"));
    let short = t
        .engine
        .create_tag(&t.repo, "base2", Some(&first[..8]), Some("m"))
        .await
        .unwrap();
    assert_eq!(short.commit, first);

    // Lo comprueba git de verdad.
    assert_eq!(t.git(&["cat-file", "-t", "refs/tags/base"]).trim(), "tag");
    assert_eq!(t.git(&["cat-file", "-t", "refs/tags/v1"]).trim(), "commit");
    assert_eq!(
        t.git(&["rev-parse", "refs/tags/base^{commit}"]).trim(),
        first
    );
    assert_eq!(t.engine.tags(&t.repo).await.unwrap().len(), 4);
}

#[tokio::test]
async fn annotated_message_survives_odd_characters() {
    let t = TestRepo::with_initial_commit();
    let msg = "ñandú «raro» — línea 1\n\n- punto\n- \"comillas\" y 'simples'\n$HOME `uname` %s";
    let tag = t
        .engine
        .create_tag(&t.repo, "raro", None, Some(msg))
        .await
        .unwrap();
    assert_eq!(tag.message.as_deref(), Some(msg));
}

#[tokio::test]
async fn name_validation_follows_check_ref_format() {
    let t = TestRepo::with_initial_commit();
    for bad in [
        "", "-x", "a b", "a..b", "a~1", "a^", "a:b", "a?", "a*", "a[", "a\\b", "/a", "a/", "a.",
        "a//b", "a@{1}", "@", ".a", "a/.b", "a.lock", "HEAD",
    ] {
        let err = t
            .engine
            .create_tag(&t.repo, bad, None, None)
            .await
            .unwrap_err();
        assert!(
            matches!(&err, GitError::InvalidTagName { name, .. } if name == bad),
            "{bad:?}: {err:?}"
        );
        let err = t
            .engine
            .delete_tag(&t.repo, bad, &confirm())
            .await
            .unwrap_err();
        assert!(matches!(err, GitError::InvalidTagName { .. }), "{bad:?}");
    }
    assert!(t.engine.tags(&t.repo).await.unwrap().is_empty());
    // Los válidos con barra y Unicode funcionan.
    for ok in ["v1.0.0", "release/1.0", "ñandú", "1.2.3-rc.1"] {
        t.engine.create_tag(&t.repo, ok, None, None).await.unwrap();
    }
    assert_eq!(t.engine.tags(&t.repo).await.unwrap().len(), 4);
}

#[tokio::test]
async fn existing_name_and_bad_targets_are_typed_errors() {
    let t = TestRepo::with_initial_commit();
    let first = t.git(&["rev-parse", "HEAD"]).trim().to_owned();
    t.engine
        .create_tag(&t.repo, "v1", None, Some("original"))
        .await
        .unwrap();
    t.write("a.txt", b"a\n");
    t.commit_all("segundo");

    let err = t
        .engine
        .create_tag(&t.repo, "v1", None, None)
        .await
        .unwrap_err();
    assert_eq!(err, GitError::TagExists { name: "v1".into() });
    // No se ha pisado la etiqueta original.
    let tags = t.engine.tags(&t.repo).await.unwrap();
    assert_eq!(tags[0].commit, first);
    assert_eq!(tags[0].message.as_deref(), Some("original"));

    for bad in ["-rf", "no-existe", "HEAD~99", "a b"] {
        let err = t
            .engine
            .create_tag(&t.repo, "x", Some(bad), None)
            .await
            .unwrap_err();
        assert!(
            matches!(err, GitError::InvalidReference(_)),
            "{bad}: {err:?}"
        );
    }
    // Un árbol o un blob no son commits.
    let tree = t.git(&["rev-parse", "HEAD^{tree}"]).trim().to_owned();
    let err = t
        .engine
        .create_tag(&t.repo, "x", Some(&tree), None)
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::InvalidReference(_)), "{err:?}");
}

#[tokio::test]
async fn cannot_tag_an_unborn_head() {
    let t = TestRepo::init();
    let err = t
        .engine
        .create_tag(&t.repo, "v0", None, None)
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::InvalidReference(_)), "{err:?}");
    assert!(t.engine.tags(&t.repo).await.unwrap().is_empty());
}

#[tokio::test]
async fn delete_local_removes_only_the_tag() {
    let t = TestRepo::with_initial_commit();
    let head = t.git(&["rev-parse", "HEAD"]).trim().to_owned();
    t.engine
        .create_tag(&t.repo, "a", None, Some("m"))
        .await
        .unwrap();
    t.engine.create_tag(&t.repo, "b", None, None).await.unwrap();
    t.engine.delete_tag(&t.repo, "a", &confirm()).await.unwrap();
    let names: Vec<String> = t
        .engine
        .tags(&t.repo)
        .await
        .unwrap()
        .into_iter()
        .map(|x| x.name)
        .collect();
    assert_eq!(names, vec!["b"]);
    assert_eq!(t.git(&["rev-parse", "HEAD"]).trim(), head);
    // Borrar otra vez: ya no existe.
    let err = t
        .engine
        .delete_tag(&t.repo, "a", &confirm())
        .await
        .unwrap_err();
    assert_eq!(err, GitError::TagNotFound { name: "a".into() });
}

#[tokio::test]
async fn publish_and_delete_on_the_remote() {
    let t = TestRepo::with_initial_commit();
    let bare = t.with_bare_origin();
    t.engine
        .create_tag(&t.repo, "v1", None, Some("uno"))
        .await
        .unwrap();
    t.engine
        .create_tag(&t.repo, "v2", None, None)
        .await
        .unwrap();
    assert!(remote_tags(bare.path()).is_empty());

    t.engine.push_tag(&t.repo, "origin", "v1").await.unwrap();
    assert_eq!(remote_tags(bare.path()), vec!["v1"], "solo la pedida");
    t.engine.push_tag(&t.repo, "origin", "v2").await.unwrap();
    assert_eq!(remote_tags(bare.path()), vec!["v1", "v2"]);
    // Publicar otra vez la misma es inocuo.
    t.engine.push_tag(&t.repo, "origin", "v1").await.unwrap();

    t.engine
        .delete_remote_tag(&t.repo, "origin", "v1", &confirm())
        .await
        .unwrap();
    assert_eq!(remote_tags(bare.path()), vec!["v2"]);
    // La local sigue.
    assert_eq!(t.engine.tags(&t.repo).await.unwrap().len(), 2);
    // Borrar otra vez lo que ya no está: un servidor local solo avisa (otros, como GitHub,
    // lo rechazan con un error de git); en ningún caso toca el resto.
    let again = t
        .engine
        .delete_remote_tag(&t.repo, "origin", "v1", &confirm())
        .await;
    assert!(
        again.is_ok() || matches!(again, Err(GitError::CommandFailed { .. })),
        "{again:?}"
    );
    assert_eq!(remote_tags(bare.path()), vec!["v2"]);
}

#[tokio::test]
async fn publishing_checks_names_remotes_and_existence() {
    let t = TestRepo::with_initial_commit();
    let _bare = t.with_bare_origin();
    let err = t
        .engine
        .push_tag(&t.repo, "origin", "nada")
        .await
        .unwrap_err();
    assert_eq!(
        err,
        GitError::TagNotFound {
            name: "nada".into()
        }
    );
    t.engine
        .create_tag(&t.repo, "v1", None, None)
        .await
        .unwrap();
    for bad in ["-x", "a b", ""] {
        let err = t.engine.push_tag(&t.repo, bad, "v1").await.unwrap_err();
        assert!(matches!(err, GitError::InvalidReference(_)), "{bad:?}");
        let err = t
            .engine
            .delete_remote_tag(&t.repo, bad, "v1", &confirm())
            .await
            .unwrap_err();
        assert!(matches!(err, GitError::InvalidReference(_)), "{bad:?}");
    }
    let err = t
        .engine
        .push_tag(&t.repo, "origin", "con espacio")
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::InvalidTagName { .. }));
    // Un remoto que no existe da un error de git.
    let err = t.engine.push_tag(&t.repo, "otro", "v1").await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
}

#[tokio::test]
async fn a_remote_tag_pointing_elsewhere_is_not_overwritten() {
    let t = TestRepo::with_initial_commit();
    let bare = t.with_bare_origin();
    t.engine
        .create_tag(&t.repo, "v1", None, None)
        .await
        .unwrap();
    t.engine.push_tag(&t.repo, "origin", "v1").await.unwrap();
    // La local pasa a apuntar a otro commit (a mano); publicar no debe pisar la remota.
    t.write("a.txt", b"a\n");
    t.commit_all("segundo");
    t.git(&["tag", "-f", "v1"]);
    let err = t
        .engine
        .push_tag(&t.repo, "origin", "v1")
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
    let remote_sha = Command::new(git_path())
        .args(["-C", bare.path().to_str().unwrap(), "rev-parse", "v1"])
        .output()
        .unwrap();
    let first = t.git(&["rev-parse", "HEAD~1"]);
    assert_eq!(
        String::from_utf8_lossy(&remote_sha.stdout).trim(),
        first.trim()
    );
}
