//! Stash contra git de verdad: guardar, listar, aplicar, sacar, borrar y ver el diff.
#![allow(clippy::unwrap_used)]

mod common;

use ariadna_git::{Confirmation, DiffScope, GitError, GitPath, Repository, StashPushOptions};
use common::TestRepo;

async fn open(t: &TestRepo) -> Repository {
    t.engine.open_repository(t.path()).await.unwrap()
}

fn opts(message: Option<&str>, untracked: bool, staged: bool) -> StashPushOptions {
    StashPushOptions {
        message: message.map(str::to_owned),
        include_untracked: untracked,
        staged_only: staged,
    }
}

fn confirm() -> Confirmation {
    Confirmation::granted("borrar el stash")
}

fn dirty() -> (TestRepo,) {
    let t = TestRepo::with_initial_commit();
    t.write("a.txt", b"uno\n");
    t.commit_all("a");
    t.write("a.txt", b"uno\ndos\n");
    (t,)
}

#[tokio::test]
async fn push_with_message_lists_and_cleans_the_tree() {
    let (t,) = dirty();
    let repo = open(&t).await;
    assert!(t.engine.stash_list(&repo).await.unwrap().is_empty());
    let entry = t
        .engine
        .stash_push(&repo, &opts(Some("  mi trabajo a medias  "), false, false))
        .await
        .unwrap();
    assert_eq!(entry.message, "mi trabajo a medias");
    assert_eq!(entry.branch.as_deref(), Some("main"));
    assert!(!entry.auto_message);
    assert_eq!(entry.index, 0);
    assert_eq!(entry.oid.len(), 40);
    assert_eq!(t.read("a.txt"), b"uno\n");

    let list = t.engine.stash_list(&repo).await.unwrap();
    assert_eq!(list, vec![entry]);
}

#[tokio::test]
async fn push_without_message_has_an_automatic_one() {
    let (t,) = dirty();
    let repo = open(&t).await;
    let entry = t
        .engine
        .stash_push(&repo, &opts(None, false, false))
        .await
        .unwrap();
    assert!(entry.auto_message, "{entry:?}");
    assert!(!entry.message.is_empty());
}

#[tokio::test]
async fn nothing_to_stash_is_a_clear_error_and_creates_nothing() {
    let t = TestRepo::with_initial_commit();
    let repo = open(&t).await;
    let e = t
        .engine
        .stash_push(&repo, &opts(None, false, false))
        .await
        .unwrap_err();
    assert_eq!(e, GitError::NothingToStash);
    assert!(t.engine.stash_list(&repo).await.unwrap().is_empty());

    // Sin nada preparado, «solo lo preparado» tampoco guarda nada aunque haya cambios.
    t.write("README.md", b"cambiado\n");
    let e = t
        .engine
        .stash_push(&repo, &opts(None, false, true))
        .await
        .unwrap_err();
    assert_eq!(e, GitError::NothingToStash);
    assert!(t.engine.stash_list(&repo).await.unwrap().is_empty());
    // Un archivo sin seguimiento solo cuenta si se pide.
    t.git(&["checkout", "--", "README.md"]);
    t.write("nuevo.txt", b"x\n");
    let e = t
        .engine
        .stash_push(&repo, &opts(None, false, false))
        .await
        .unwrap_err();
    assert_eq!(e, GitError::NothingToStash);
}

#[tokio::test]
async fn include_untracked_saves_and_restores_new_files() {
    let (t,) = dirty();
    t.write("nuevo ñ.txt", "hola\n".as_bytes());
    let repo = open(&t).await;
    let entry = t
        .engine
        .stash_push(&repo, &opts(Some("con nuevos"), true, false))
        .await
        .unwrap();
    assert!(!t.path().join("nuevo ñ.txt").exists());
    assert_eq!(t.read("a.txt"), b"uno\n");

    // El diff del stash incluye el archivo sin seguimiento como nuevo.
    let diff = t.engine.stash_diff(&repo, &entry.oid).await.unwrap();
    let names: Vec<_> = diff
        .iter()
        .map(|f| f.path.to_string_lossy().into_owned())
        .collect();
    assert!(
        names.contains(&"a.txt".to_owned()) && names.contains(&"nuevo ñ.txt".to_owned()),
        "{names:?}"
    );
    assert!(
        diff.iter()
            .find(|f| f.path == *"nuevo ñ.txt")
            .unwrap()
            .is_new_file
    );

    let res = t.engine.stash_pop(&repo, &entry.oid).await.unwrap();
    assert!(res.conflicts.is_empty() && res.dropped);
    assert_eq!(t.read("nuevo ñ.txt"), "hola\n".as_bytes());
    assert_eq!(t.read("a.txt"), b"uno\ndos\n");
    assert!(t.engine.stash_list(&repo).await.unwrap().is_empty());
}

#[tokio::test]
async fn staged_only_leaves_unstaged_changes_in_place() {
    let t = TestRepo::with_initial_commit();
    t.write("p.txt", b"1\n");
    t.write("q.txt", b"1\n");
    t.commit_all("pq");
    t.write("p.txt", b"preparado\n");
    t.write("q.txt", b"sin preparar\n");
    t.git(&["add", "p.txt"]);
    let repo = open(&t).await;
    t.engine
        .stash_push(&repo, &opts(Some("solo lo preparado"), false, true))
        .await
        .unwrap();
    assert_eq!(t.read("p.txt"), b"1\n");
    assert_eq!(t.read("q.txt"), b"sin preparar\n");
}

#[tokio::test]
async fn apply_keeps_the_stash_and_pop_removes_it() {
    let (t,) = dirty();
    let repo = open(&t).await;
    let entry = t
        .engine
        .stash_push(&repo, &opts(Some("uno"), false, false))
        .await
        .unwrap();

    let res = t.engine.stash_apply(&repo, &entry.oid).await.unwrap();
    assert!(res.conflicts.is_empty() && !res.dropped);
    assert_eq!(t.read("a.txt"), b"uno\ndos\n");
    assert_eq!(t.engine.stash_list(&repo).await.unwrap().len(), 1);

    t.git(&["checkout", "--", "a.txt"]);
    let res = t.engine.stash_pop(&repo, &entry.oid).await.unwrap();
    assert!(res.dropped);
    assert!(t.engine.stash_list(&repo).await.unwrap().is_empty());
}

#[tokio::test]
async fn oid_identifies_the_stash_even_when_the_order_changes() {
    let (t,) = dirty();
    let repo = open(&t).await;
    let first = t
        .engine
        .stash_push(&repo, &opts(Some("primero"), false, false))
        .await
        .unwrap();
    t.write("a.txt", b"uno\ntres\n");
    let second = t
        .engine
        .stash_push(&repo, &opts(Some("segundo"), false, false))
        .await
        .unwrap();
    let list = t.engine.stash_list(&repo).await.unwrap();
    assert_eq!(list[0].oid, second.oid);
    assert_eq!(list[1].oid, first.oid);
    assert_eq!(list[1].index, 1);

    // Se borra el segundo (índice 0): el primero pasa a ser stash@{0} y se sigue encontrando.
    t.engine
        .stash_drop(&repo, &second.oid, &confirm())
        .await
        .unwrap();
    let list = t.engine.stash_list(&repo).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].oid, first.oid);
    assert_eq!(list[0].index, 0);
    // Un prefijo único también vale.
    let diff = t.engine.stash_diff(&repo, &first.oid[..10]).await.unwrap();
    assert_eq!(diff.len(), 1);
}

#[tokio::test]
async fn drop_only_the_requested_one_and_unknown_oids_fail_cleanly() {
    let (t,) = dirty();
    let repo = open(&t).await;
    let a = t
        .engine
        .stash_push(&repo, &opts(Some("a"), false, false))
        .await
        .unwrap();
    t.write("a.txt", b"otro\n");
    let b = t
        .engine
        .stash_push(&repo, &opts(Some("b"), false, false))
        .await
        .unwrap();

    t.engine
        .stash_drop(&repo, &a.oid, &confirm())
        .await
        .unwrap();
    let list = t.engine.stash_list(&repo).await.unwrap();
    assert_eq!(
        list.iter().map(|e| e.oid.clone()).collect::<Vec<_>>(),
        vec![b.oid.clone()]
    );

    // Ya no existe → error propio, sin tocar el otro.
    let e = t
        .engine
        .stash_drop(&repo, &a.oid, &confirm())
        .await
        .unwrap_err();
    assert!(matches!(e, GitError::StashNotFound { .. }), "{e:?}");
    let e = t
        .engine
        .stash_pop(&repo, &"0".repeat(40))
        .await
        .unwrap_err();
    assert!(matches!(e, GitError::StashNotFound { .. }), "{e:?}");
    // Basura que no es un SHA nunca llega a git.
    for bad in ["--all", "stash@{0}", "", "xyz", "HEAD"] {
        let e = t.engine.stash_apply(&repo, bad).await.unwrap_err();
        assert!(matches!(e, GitError::InvalidReference(_)), "{bad}: {e:?}");
    }
    assert_eq!(t.engine.stash_list(&repo).await.unwrap().len(), 1);
}

#[tokio::test]
async fn pop_with_conflicts_keeps_the_stash_and_leaves_conflict_state() {
    let (t,) = dirty();
    let repo = open(&t).await;
    let entry = t
        .engine
        .stash_push(&repo, &opts(Some("choca"), false, false))
        .await
        .unwrap();
    // Cambio incompatible en la misma línea, ya confirmado.
    t.write("a.txt", b"uno\nDISTINTO\n");
    t.commit_all("distinto");

    let res = t.engine.stash_pop(&repo, &entry.oid).await.unwrap();
    assert_eq!(res.conflicts, vec![GitPath::from("a.txt")]);
    assert!(!res.dropped);
    // El stash sigue ahí y el panel de conflictos tiene qué mostrar.
    assert_eq!(t.engine.stash_list(&repo).await.unwrap().len(), 1);
    let conflicts = t.engine.conflicts(&repo).await.unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].path, GitPath::from("a.txt"));
    assert!(String::from_utf8_lossy(&t.read("a.txt")).contains("<<<<<<<"));
}

#[tokio::test]
async fn apply_over_local_changes_that_would_be_overwritten_is_an_error() {
    let (t,) = dirty();
    let repo = open(&t).await;
    let entry = t
        .engine
        .stash_push(&repo, &opts(None, false, false))
        .await
        .unwrap();
    t.write("a.txt", b"uno\nlocal\n");
    let e = t.engine.stash_apply(&repo, &entry.oid).await.unwrap_err();
    assert!(matches!(e, GitError::CommandFailed { .. }), "{e:?}");
    // Nada se perdió.
    assert_eq!(t.read("a.txt"), b"uno\nlocal\n");
    assert_eq!(t.engine.stash_list(&repo).await.unwrap().len(), 1);
}

#[tokio::test]
async fn stash_diff_shows_the_saved_changes_with_stable_line_ids() {
    let (t,) = dirty();
    t.write("b.txt", b"nuevo\n");
    let repo = open(&t).await;
    let entry = t
        .engine
        .stash_push(&repo, &opts(Some("d"), true, false))
        .await
        .unwrap();
    let files = t.engine.stash_diff(&repo, &entry.oid).await.unwrap();
    assert_eq!(files.len(), 2);
    let mut ids: Vec<&str> = files
        .iter()
        .flat_map(|f| {
            f.hunks
                .iter()
                .flat_map(|h| h.lines.iter().map(|l| l.id.as_str()))
        })
        .collect();
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(
        ids.len(),
        n,
        "los ids de línea no se repiten entre archivos"
    );
    // El diff normal no cambió por haber stashes.
    assert!(t
        .engine
        .diff(&repo, &DiffScope::Unstaged, &[])
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn unborn_repo_stash_push_fails_cleanly() {
    let t = TestRepo::init();
    t.write("x.txt", b"x\n");
    t.git(&["add", "x.txt"]);
    let repo = open(&t).await;
    let e = t
        .engine
        .stash_push(&repo, &opts(None, false, false))
        .await
        .unwrap_err();
    assert!(matches!(e, GitError::CommandFailed { .. }), "{e:?}");
}
