//! Ramas, remotos, merge, integración y conflictos contra git de verdad.
#![allow(clippy::unwrap_used)]
mod common;

use ariadna_git::{
    resolve_all, Confirmation, ConflictChoice, ConflictKind, GitError, GitPath, IntegrationState,
    MergeFastForward, MergeResult,
};
use common::TestRepo;
use tempfile::TempDir;

fn conf() -> Confirmation {
    Confirmation::granted("test")
}

fn p(s: &str) -> GitPath {
    GitPath::from(s)
}

fn names(branches: &[ariadna_git::Branch]) -> Vec<String> {
    branches.iter().map(|b| b.name.clone()).collect()
}

/// `main` y `feature` tocan la misma línea de `file.txt`; HEAD queda en `main`.
fn conflicted_repo() -> TestRepo {
    let r = TestRepo::init();
    r.write("file.txt", b"line1\nline2\nline3\n");
    r.commit_all("base");
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("file.txt", b"line1\nfrom-feature\nline3\n");
    r.commit_all("feature");
    r.git(&["checkout", "-q", "main"]);
    r.write("file.txt", b"line1\nfrom-main\nline3\n");
    r.commit_all("main");
    r
}

/// Origen con un commit publicado en un remoto bare.
fn origin_with_bare() -> (TestRepo, TempDir) {
    let origin = TestRepo::with_initial_commit();
    let bare = origin.with_bare_origin();
    origin.git(&["push", "-q", "-u", "origin", "main"]);
    (origin, bare)
}

/// Clon (con identidad de prueba) del bare, con `main` siguiendo a `origin/main`.
fn clone_of(bare: &TempDir) -> TestRepo {
    let c = TestRepo::init();
    let url = bare.path().to_string_lossy().into_owned();
    c.git(&["remote", "add", "origin", &url]);
    c.git(&["fetch", "-q", "origin"]);
    c.git(&["checkout", "-q", "-B", "main", "--track", "origin/main"]);
    c.git(&["remote", "set-head", "origin", "main"]);
    c
}

// ---------------------------------------------------------------------------
// Ramas
// ---------------------------------------------------------------------------

#[tokio::test]
async fn branches_checkout_rename_and_delete() {
    let r = TestRepo::with_initial_commit();
    let e = &r.engine;
    let sha = r.git(&["rev-parse", "HEAD"]).trim().to_string();

    e.create_branch(&r.repo, "feature", None).await.unwrap();
    e.checkout(&r.repo, "feature", false).await.unwrap();
    assert_eq!(r.git(&["branch", "--show-current"]).trim(), "feature");

    e.rename_branch(&r.repo, "feature", "feature-2")
        .await
        .unwrap();
    e.checkout(&r.repo, &sha, false).await.unwrap();
    // HEAD detached en el commit.
    assert_eq!(r.git(&["branch", "--show-current"]).trim(), "");
    assert_eq!(r.git(&["rev-parse", "HEAD"]).trim(), sha);

    e.checkout(&r.repo, "main", false).await.unwrap();
    e.delete_branch(&r.repo, "feature-2", false).await.unwrap();
    let list = e.branches(&r.repo).await.unwrap();
    assert_eq!(names(&list), ["main"]);
    assert!(list[0].is_head);
    assert_eq!(list[0].oid, sha);

    // La rama actual no se puede borrar.
    let err = e.delete_branch(&r.repo, "main", true).await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
}

#[tokio::test]
async fn checkout_a_tag_leaves_head_detached() {
    let r = TestRepo::with_initial_commit();
    r.git(&["tag", "v1"]);
    r.write("b.txt", b"b\n");
    r.commit_all("segundo");
    r.engine.checkout(&r.repo, "v1", false).await.unwrap();
    assert_eq!(r.git(&["branch", "--show-current"]).trim(), "");
    assert_eq!(
        r.git(&["rev-parse", "HEAD"]),
        r.git(&["rev-parse", "v1^{commit}"])
    );
}

#[tokio::test]
async fn create_branch_from_start_point_and_delete_unmerged_needs_force() {
    let r = TestRepo::with_initial_commit();
    let first = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    r.write("b.txt", b"b\n");
    r.commit_all("segundo");
    let e = &r.engine;

    e.create_branch(&r.repo, "old", Some(&first)).await.unwrap();
    assert_eq!(r.git(&["rev-parse", "old"]).trim(), first);

    e.checkout(&r.repo, "old", false).await.unwrap();
    r.write("c.txt", b"c\n");
    r.commit_all("solo en old");
    e.checkout(&r.repo, "main", false).await.unwrap();

    let err = e.delete_branch(&r.repo, "old", false).await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
    assert!(names(&e.branches(&r.repo).await.unwrap()).contains(&"old".to_string()));
    e.delete_branch(&r.repo, "old", true).await.unwrap();
    assert_eq!(names(&e.branches(&r.repo).await.unwrap()), ["main"]);
}

#[tokio::test]
async fn invalid_references_are_rejected_before_running_git() {
    let r = TestRepo::with_initial_commit();
    let e = &r.engine;
    for bad in ["-x", "a..b", "", "con espacio", "x~1"] {
        let err = e.create_branch(&r.repo, bad, None).await.unwrap_err();
        assert!(
            matches!(err, GitError::InvalidReference(_)),
            "{bad:?}: {err:?}"
        );
    }
    let err = e.checkout(&r.repo, "--orphan", false).await.unwrap_err();
    assert!(matches!(err, GitError::InvalidReference(_)), "{err:?}");
    let err = e
        .merge(&r.repo, "-s", MergeFastForward::Allow)
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::InvalidReference(_)), "{err:?}");
    let err = e
        .create_branch(&r.repo, "ok", Some("-D"))
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::InvalidReference(_)), "{err:?}");
    let err = e.delete_branch(&r.repo, "--force", true).await.unwrap_err();
    assert!(matches!(err, GitError::InvalidReference(_)), "{err:?}");
}

#[tokio::test]
async fn checkout_dirty_tree_blocks_with_detail_and_keeps_changes() {
    let r = TestRepo::init();
    r.write("a.txt", b"uno\n");
    r.commit_all("base");
    r.git(&["checkout", "-q", "-b", "otra"]);
    r.write("a.txt", b"desde-otra\n");
    r.commit_all("otra");
    r.git(&["checkout", "-q", "main"]);
    r.write("a.txt", b"sucio\n");

    let err = r.engine.checkout(&r.repo, "otra", false).await.unwrap_err();
    match &err {
        GitError::LocalChangesBlockCheckout { detail } => assert!(detail.contains("a.txt")),
        other => panic!("esperaba LocalChangesBlockCheckout, llegó {other:?}"),
    }
    let text = err.to_string();
    assert!(text.contains("cambios locales") && text.contains("cambio de rama"));
    assert_eq!(r.git(&["branch", "--show-current"]).trim(), "main");
    assert_eq!(r.read("a.txt"), b"sucio\n");
}

#[tokio::test]
async fn checkout_allows_unrelated_local_changes() {
    let r = TestRepo::init();
    r.write("a.txt", b"uno\n");
    r.write("b.txt", b"b\n");
    r.commit_all("base");
    r.git(&["checkout", "-q", "-b", "otra"]);
    r.write("a.txt", b"desde-otra\n");
    r.commit_all("otra");
    r.git(&["checkout", "-q", "main"]);
    r.write("b.txt", b"sucio pero sin relacion\n");
    r.engine.checkout(&r.repo, "otra", false).await.unwrap();
    assert_eq!(r.read("b.txt"), b"sucio pero sin relacion\n");
}

#[tokio::test]
async fn checkout_blocks_when_untracked_file_would_be_overwritten() {
    let r = TestRepo::with_initial_commit();
    r.git(&["checkout", "-q", "-b", "otra"]);
    r.write("nuevo.txt", b"de otra\n");
    r.commit_all("añade nuevo");
    r.git(&["checkout", "-q", "main"]);
    r.write("nuevo.txt", b"mio sin seguimiento\n");
    let err = r.engine.checkout(&r.repo, "otra", false).await.unwrap_err();
    assert!(
        matches!(&err, GitError::LocalChangesBlockCheckout { detail } if detail.contains("nuevo.txt")),
        "{err:?}"
    );
    assert_eq!(r.read("nuevo.txt"), "mio sin seguimiento\n".as_bytes());
}

#[tokio::test]
async fn checkout_remote_tracking_creates_local_branch_with_upstream() {
    let (origin, bare) = origin_with_bare();
    origin.git(&["checkout", "-q", "-b", "feature"]);
    origin.write("b.txt", b"feature\n");
    origin.commit_all("en feature");
    origin.git(&["push", "-q", "-u", "origin", "feature"]);

    let clone = clone_of(&bare);
    let e = &clone.engine;
    assert!(!names(&e.branches(&clone.repo).await.unwrap()).contains(&"feature".to_string()));

    e.checkout(&clone.repo, "origin/feature", false)
        .await
        .unwrap();
    assert_eq!(clone.git(&["branch", "--show-current"]).trim(), "feature");
    let feature = e
        .branches(&clone.repo)
        .await
        .unwrap()
        .into_iter()
        .find(|b| b.name == "feature")
        .unwrap();
    assert_eq!(feature.upstream.as_deref(), Some("origin/feature"));
    assert!(feature.is_head);

    // Repetir sobre la local ya sincronizada no es divergencia.
    e.checkout(&clone.repo, "main", false).await.unwrap();
    e.checkout(&clone.repo, "origin/feature", false)
        .await
        .unwrap();
    assert_eq!(clone.git(&["branch", "--show-current"]).trim(), "feature");
}

#[tokio::test]
async fn checkout_remote_does_not_switch_diverged_local() {
    let (r, _bare) = origin_with_bare();
    let base = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    r.git(&["branch", "feature"]);
    r.git(&["push", "-q", "origin", "feature"]);
    r.git(&["checkout", "-q", "feature"]);
    r.write("b.txt", b"local\n");
    r.commit_all("solo-local");
    r.git(&["checkout", "-q", "main"]);

    let err = r
        .engine
        .checkout(&r.repo, "origin/feature", false)
        .await
        .unwrap_err();
    match &err {
        GitError::DivergedLocalBranch { local, remote } => {
            assert_eq!(local, "feature");
            assert_eq!(remote, "origin/feature");
        }
        other => panic!("esperaba DivergedLocalBranch, llegó {other:?}"),
    }
    assert!(err.to_string().contains("no coincide"));
    assert_eq!(r.git(&["branch", "--show-current"]).trim(), "main");

    r.engine
        .checkout(&r.repo, "origin/feature", true)
        .await
        .unwrap();
    assert_eq!(r.git(&["branch", "--show-current"]).trim(), "feature");
    assert_ne!(r.git(&["rev-parse", "HEAD"]).trim(), base);
}

#[tokio::test]
async fn checkout_remote_with_local_following_another_upstream_is_diverged() {
    let (r, _bare) = origin_with_bare();
    r.git(&["branch", "feature"]);
    r.git(&["push", "-q", "origin", "feature"]);
    // Misma posición, pero la local sigue a otra rama.
    r.git(&["branch", "--set-upstream-to=origin/main", "feature"]);
    let err = r
        .engine
        .checkout(&r.repo, "origin/feature", false)
        .await
        .unwrap_err();
    assert!(
        matches!(err, GitError::DivergedLocalBranch { .. }),
        "{err:?}"
    );
}

#[tokio::test]
async fn remote_tracking_branches_skip_origin_head() {
    let (origin, bare) = origin_with_bare();
    origin.git(&["branch", "dev"]);
    origin.git(&["push", "-q", "origin", "dev"]);
    let clone = clone_of(&bare);
    // `set-head` creó refs/remotes/origin/HEAD.
    assert!(clone
        .git(&["for-each-ref", "refs/remotes"])
        .contains("origin/HEAD"));
    clone.git(&["fetch", "-q", "origin"]);
    let list = clone
        .engine
        .remote_tracking_branches(&clone.repo)
        .await
        .unwrap();
    assert_eq!(names(&list), ["origin/dev", "origin/main"]);
    // Las locales no incluyen las remotas.
    assert_eq!(
        names(&clone.engine.branches(&clone.repo).await.unwrap()),
        ["main"]
    );
}

// ---------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------

#[tokio::test]
async fn merge_fast_forward_no_ff_and_up_to_date() {
    let r = TestRepo::with_initial_commit();
    let e = &r.engine;
    e.create_branch(&r.repo, "feature", None).await.unwrap();
    e.checkout(&r.repo, "feature", false).await.unwrap();
    r.write("b.txt", b"feature\n");
    let feature_tip = r.commit_all("en feature");
    e.checkout(&r.repo, "main", false).await.unwrap();

    let ff = e
        .merge(&r.repo, "feature", MergeFastForward::Allow)
        .await
        .unwrap();
    assert_eq!(
        ff,
        MergeResult::FastForward {
            new_head: feature_tip
        }
    );

    e.create_branch(&r.repo, "topic", None).await.unwrap();
    e.checkout(&r.repo, "topic", false).await.unwrap();
    r.write("c.txt", b"topic\n");
    r.commit_all("en topic");
    e.checkout(&r.repo, "main", false).await.unwrap();
    let merged = e
        .merge(&r.repo, "topic", MergeFastForward::Never)
        .await
        .unwrap();
    let head = r.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(merged, MergeResult::Merged { new_head: head });
    assert_eq!(
        r.git(&["rev-list", "--parents", "-n1", "HEAD"])
            .split(' ')
            .count(),
        3
    );

    let again = e
        .merge(&r.repo, "topic", MergeFastForward::Allow)
        .await
        .unwrap();
    assert_eq!(again, MergeResult::UpToDate);
    let again = e
        .merge(&r.repo, "topic", MergeFastForward::Never)
        .await
        .unwrap();
    assert_eq!(again, MergeResult::UpToDate);
}

#[tokio::test]
async fn merge_ff_only_that_cannot_advance_is_an_error_and_changes_nothing() {
    let r = conflicted_repo(); // main y feature divergen
    let before = r.git(&["rev-parse", "HEAD"]);
    let err = r
        .engine
        .merge(&r.repo, "feature", MergeFastForward::Only)
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
    assert_eq!(r.git(&["rev-parse", "HEAD"]), before);
    assert_eq!(
        r.engine.integration_state(&r.repo).await.unwrap(),
        IntegrationState::None
    );
}

#[tokio::test]
async fn merge_ff_only_that_can_advance_works() {
    let r = TestRepo::with_initial_commit();
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("b.txt", b"b\n");
    let tip = r.commit_all("f");
    r.git(&["checkout", "-q", "main"]);
    let res = r
        .engine
        .merge(&r.repo, "feature", MergeFastForward::Only)
        .await
        .unwrap();
    assert_eq!(res, MergeResult::FastForward { new_head: tip });
}

#[tokio::test]
async fn merge_blocked_by_local_changes_is_an_error_not_conflicts() {
    let r = TestRepo::init();
    r.write("a.txt", b"1\n");
    r.write("b.txt", b"1\n");
    r.commit_all("base");
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("a.txt", b"2\n");
    r.commit_all("f");
    r.git(&["checkout", "-q", "main"]);
    r.write("a.txt", b"sucio\n");
    let err = r
        .engine
        .merge(&r.repo, "feature", MergeFastForward::Allow)
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
    assert_eq!(r.read("a.txt"), b"sucio\n");
}

#[tokio::test]
async fn merge_conflict_returns_paths_then_resolve_mark_and_continue() {
    let r = conflicted_repo();
    let e = &r.engine;
    let res = e
        .merge(&r.repo, "feature", MergeFastForward::Allow)
        .await
        .unwrap();
    assert_eq!(
        res,
        MergeResult::Conflicts {
            paths: vec![p("file.txt")]
        }
    );
    assert_eq!(
        e.integration_state(&r.repo).await.unwrap(),
        IntegrationState::Merging
    );

    let files = e.conflicts(&r.repo).await.unwrap();
    assert_eq!(files.len(), 1);
    let f = &files[0];
    assert_eq!(f.path, "file.txt");
    assert_eq!(f.kind, ConflictKind::BothModified);
    assert_eq!(
        f.ours.content.as_deref(),
        Some(&b"line1\nfrom-main\nline3\n"[..])
    );
    assert_eq!(
        f.base.content.as_deref(),
        Some(&b"line1\nline2\nline3\n"[..])
    );
    assert_eq!(
        f.theirs.content.as_deref(),
        Some(&b"line1\nfrom-feature\nline3\n"[..])
    );
    assert_eq!(f.hunks.len(), 1);
    assert_eq!(f.hunks[0].ours, b"from-main\n");
    assert_eq!(f.hunks[0].theirs, b"from-feature\n");
    assert!(f
        .working_tree
        .as_ref()
        .unwrap()
        .starts_with(b"line1\n<<<<<<<"));
    assert_eq!(
        f.taking_whole_side(ConflictChoice::Ours),
        f.ours.content.clone().unwrap()
    );

    // Continuar con conflictos sin resolver falla y no cambia el estado.
    assert!(e.continue_integration(&r.repo).await.is_err());
    assert_eq!(
        e.integration_state(&r.repo).await.unwrap(),
        IntegrationState::Merging
    );

    e.write_working_tree_file(&r.repo, &p("file.txt"), b"resuelto\n", &conf())
        .await
        .unwrap();
    // Escribir no hace `git add`: sigue en conflicto.
    assert_eq!(e.conflicts(&r.repo).await.unwrap().len(), 1);
    assert_eq!(r.read("file.txt"), b"resuelto\n");

    e.mark_resolved(&r.repo, &p("file.txt")).await.unwrap();
    assert!(e.conflicts(&r.repo).await.unwrap().is_empty());
    assert_eq!(r.git(&["ls-files", "-u"]), "");

    e.continue_integration(&r.repo).await.unwrap();
    assert_eq!(
        e.integration_state(&r.repo).await.unwrap(),
        IntegrationState::None
    );
    assert_eq!(
        r.git(&["rev-list", "--parents", "-n1", "HEAD"])
            .split(' ')
            .count(),
        3
    );
    assert_eq!(r.git(&["status", "--porcelain"]), "");
}

#[tokio::test]
async fn abort_merge_restores_clean_tree() {
    let r = conflicted_repo();
    let e = &r.engine;
    let before = r.git(&["rev-parse", "HEAD"]);
    e.merge(&r.repo, "feature", MergeFastForward::Allow)
        .await
        .unwrap();
    e.abort_integration(&r.repo).await.unwrap();
    assert_eq!(r.git(&["status", "--porcelain"]), "");
    assert_eq!(r.git(&["rev-parse", "HEAD"]), before);
    assert_eq!(
        e.integration_state(&r.repo).await.unwrap(),
        IntegrationState::None
    );
    let err = e.abort_integration(&r.repo).await.unwrap_err();
    assert_eq!(err, GitError::NoIntegrationInProgress);
    let err = e.continue_integration(&r.repo).await.unwrap_err();
    assert_eq!(err, GitError::NoIntegrationInProgress);
}

// ---------------------------------------------------------------------------
// Rebase y cherry-pick
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rebase_conflict_abort_and_continue() {
    let r = conflicted_repo();
    let e = &r.engine;
    r.git(&["checkout", "-q", "feature"]);
    let feature_before = r.git(&["rev-parse", "HEAD"]);
    let (code, _, _) = r.git_raw(&["rebase", "main"]);
    assert_ne!(code, 0);
    assert_eq!(
        e.integration_state(&r.repo).await.unwrap(),
        IntegrationState::Rebasing
    );
    let files = e.conflicts(&r.repo).await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].kind, ConflictKind::BothModified);

    e.abort_integration(&r.repo).await.unwrap();
    assert_eq!(
        e.integration_state(&r.repo).await.unwrap(),
        IntegrationState::None
    );
    assert_eq!(r.git(&["rev-parse", "HEAD"]), feature_before);
    assert_eq!(r.git(&["branch", "--show-current"]).trim(), "feature");

    let (code, _, _) = r.git_raw(&["rebase", "main"]);
    assert_ne!(code, 0);
    e.write_working_tree_file(
        &r.repo,
        &p("file.txt"),
        b"line1\nrebase-ok\nline3\n",
        &conf(),
    )
    .await
    .unwrap();
    e.mark_resolved(&r.repo, &p("file.txt")).await.unwrap();
    e.continue_integration(&r.repo).await.unwrap();
    assert_eq!(
        e.integration_state(&r.repo).await.unwrap(),
        IntegrationState::None
    );
    assert_eq!(r.read("file.txt"), b"line1\nrebase-ok\nline3\n");
    assert_eq!(r.git(&["branch", "--show-current"]).trim(), "feature");
    let subjects = r.git(&["log", "--format=%s", "-3"]);
    assert_eq!(
        subjects.lines().collect::<Vec<_>>(),
        ["feature", "main", "base"]
    );
}

#[tokio::test]
async fn cherry_pick_conflict_abort() {
    let r = conflicted_repo();
    let e = &r.engine;
    let before = r.git(&["rev-parse", "HEAD"]);
    let (code, _, _) = r.git_raw(&["cherry-pick", "feature"]);
    assert_ne!(code, 0);
    assert_eq!(
        e.integration_state(&r.repo).await.unwrap(),
        IntegrationState::CherryPicking
    );
    assert_eq!(e.conflicts(&r.repo).await.unwrap().len(), 1);

    e.abort_integration(&r.repo).await.unwrap();
    assert_eq!(
        e.integration_state(&r.repo).await.unwrap(),
        IntegrationState::None
    );
    assert_eq!(r.git(&["rev-parse", "HEAD"]), before);
    assert_eq!(r.git(&["status", "--porcelain"]), "");
}

#[tokio::test]
async fn cherry_pick_conflict_continue() {
    let r = conflicted_repo();
    let e = &r.engine;
    let (code, _, _) = r.git_raw(&["cherry-pick", "feature"]);
    assert_ne!(code, 0);
    e.write_working_tree_file(&r.repo, &p("file.txt"), b"line1\npicked\nline3\n", &conf())
        .await
        .unwrap();
    e.mark_resolved(&r.repo, &p("file.txt")).await.unwrap();
    e.continue_integration(&r.repo).await.unwrap();
    assert_eq!(
        e.integration_state(&r.repo).await.unwrap(),
        IntegrationState::None
    );
    assert_eq!(r.git(&["log", "--format=%s", "-1"]).trim(), "feature");
    assert_eq!(r.read("file.txt"), b"line1\npicked\nline3\n");
}

#[tokio::test]
async fn integration_state_follows_linked_worktree_git_dir() {
    let r = conflicted_repo();
    let wt_parent = tempfile::tempdir().unwrap();
    let wt_path = wt_parent.path().canonicalize().unwrap().join("wt");
    r.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "wt-branch",
        wt_path.to_str().unwrap(),
        "main",
    ]);
    let wt_repo = ariadna_git::Repository {
        path: wt_path.clone(),
        git_dir: wt_path.join(".git"),
    };
    let e = &r.engine;
    let res = e
        .merge(&wt_repo, "feature", MergeFastForward::Allow)
        .await
        .unwrap();
    assert!(matches!(res, MergeResult::Conflicts { .. }), "{res:?}");
    assert_eq!(
        e.integration_state(&wt_repo).await.unwrap(),
        IntegrationState::Merging
    );
    // El worktree principal no tiene nada en curso.
    assert_eq!(
        e.integration_state(&r.repo).await.unwrap(),
        IntegrationState::None
    );
    e.abort_integration(&wt_repo).await.unwrap();
    assert_eq!(
        e.integration_state(&wt_repo).await.unwrap(),
        IntegrationState::None
    );
}

// ---------------------------------------------------------------------------
// Tipos de conflicto
// ---------------------------------------------------------------------------

#[tokio::test]
async fn conflict_delete_modify_both_ways() {
    // feature borra; main modifica -> «borrado por ellos».
    let r = TestRepo::init();
    r.write("a.txt", b"uno\ndos\ntres\n");
    r.commit_all("base");
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.git(&["rm", "-q", "a.txt"]);
    r.commit_all("borra");
    r.git(&["checkout", "-q", "main"]);
    r.write("a.txt", b"uno\nDOS\ntres\n");
    r.commit_all("modifica");

    let e = &r.engine;
    let res = e
        .merge(&r.repo, "feature", MergeFastForward::Allow)
        .await
        .unwrap();
    assert_eq!(
        res,
        MergeResult::Conflicts {
            paths: vec![p("a.txt")]
        }
    );
    let files = e.conflicts(&r.repo).await.unwrap();
    assert_eq!(files[0].kind, ConflictKind::DeletedByThem);
    assert!(files[0].ours.is_present() && files[0].base.is_present());
    assert!(!files[0].theirs.is_present());
    assert!(files[0].hunks.is_empty());
    // Resolver borrando el archivo.
    std::fs::remove_file(r.path().join("a.txt")).unwrap();
    e.mark_resolved(&r.repo, &p("a.txt")).await.unwrap();
    assert!(e.conflicts(&r.repo).await.unwrap().is_empty());
    e.continue_integration(&r.repo).await.unwrap();
    assert!(!r.path().join("a.txt").exists());
    e.abort_integration(&r.repo).await.unwrap_err();

    // Al revés: la rama actual borra y la otra modifica -> «borrado por nosotros».
    let r = TestRepo::init();
    r.write("a.txt", b"uno\ndos\ntres\n");
    r.commit_all("base");
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("a.txt", b"uno\nDOS\ntres\n");
    r.commit_all("modifica");
    r.git(&["checkout", "-q", "main"]);
    r.git(&["rm", "-q", "a.txt"]);
    r.commit_all("borra");
    let res = r
        .engine
        .merge(&r.repo, "feature", MergeFastForward::Allow)
        .await
        .unwrap();
    assert!(matches!(res, MergeResult::Conflicts { .. }));
    let files = r.engine.conflicts(&r.repo).await.unwrap();
    assert_eq!(files[0].kind, ConflictKind::DeletedByUs);
    assert!(!files[0].ours.is_present() && files[0].theirs.is_present());
    // Resolver quedándose con el archivo de ellos.
    r.engine
        .write_working_tree_file(&r.repo, &p("a.txt"), b"uno\nDOS\ntres\n", &conf())
        .await
        .unwrap();
    r.engine.mark_resolved(&r.repo, &p("a.txt")).await.unwrap();
    r.engine.continue_integration(&r.repo).await.unwrap();
    assert_eq!(r.read("a.txt"), b"uno\nDOS\ntres\n");
}

#[tokio::test]
async fn conflict_both_added() {
    let r = TestRepo::with_initial_commit();
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("n.txt", b"de feature\n");
    r.commit_all("f");
    r.git(&["checkout", "-q", "main"]);
    r.write("n.txt", b"de main\n");
    r.commit_all("m");
    r.engine
        .merge(&r.repo, "feature", MergeFastForward::Allow)
        .await
        .unwrap();
    let files = r.engine.conflicts(&r.repo).await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].kind, ConflictKind::BothAdded);
    assert!(!files[0].base.is_present());
    assert_eq!(files[0].hunks.len(), 1);
}

#[tokio::test]
async fn conflict_rename_rename_gives_added_by_us_added_by_them_and_both_deleted() {
    let body: String = (1..=30).map(|i| format!("línea {i}\n")).collect();
    let r = TestRepo::init();
    r.write("a.txt", body.as_bytes());
    r.commit_all("base");
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.git(&["mv", "a.txt", "c.txt"]);
    r.commit_all("a -> c");
    r.git(&["checkout", "-q", "main"]);
    r.git(&["mv", "a.txt", "b.txt"]);
    r.commit_all("a -> b");
    let res = r
        .engine
        .merge(&r.repo, "feature", MergeFastForward::Allow)
        .await
        .unwrap();
    assert!(matches!(res, MergeResult::Conflicts { .. }), "{res:?}");
    let files = r.engine.conflicts(&r.repo).await.unwrap();
    let kinds: Vec<(String, ConflictKind)> = files
        .iter()
        .map(|f| (f.path.to_string_lossy().into_owned(), f.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("a.txt".to_string(), ConflictKind::BothDeleted),
            ("b.txt".to_string(), ConflictKind::AddedByUs),
            ("c.txt".to_string(), ConflictKind::AddedByThem),
        ]
    );
}

#[tokio::test]
async fn conflict_binary_has_sides_but_no_hunks() {
    let r = TestRepo::init();
    r.write("img.bin", &[0, 1, 2, 3, 0, 9]);
    r.commit_all("base");
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write("img.bin", &[0, 1, 2, 3, 0, 7, 7]);
    r.commit_all("f");
    r.git(&["checkout", "-q", "main"]);
    r.write("img.bin", &[0, 1, 2, 3, 0, 8, 8, 8]);
    r.commit_all("m");
    let res = r
        .engine
        .merge(&r.repo, "feature", MergeFastForward::Allow)
        .await
        .unwrap();
    assert_eq!(
        res,
        MergeResult::Conflicts {
            paths: vec![p("img.bin")]
        }
    );
    let f = &r.engine.conflicts(&r.repo).await.unwrap()[0];
    assert_eq!(f.kind, ConflictKind::BothModified);
    assert!(f.ours.is_present() && f.theirs.is_present() && f.base.is_present());
    assert!(f.ours.is_binary && f.theirs.is_binary);
    assert!(f.hunks.is_empty());
    assert_eq!(
        f.theirs.content.as_deref(),
        Some(&[0u8, 1, 2, 3, 0, 7, 7][..])
    );
}

#[tokio::test]
async fn writing_a_resolution_preserves_crlf_and_bom() {
    let bom = b"\xEF\xBB\xBF";
    let r = TestRepo::init();
    r.write(
        "w.txt",
        &[&bom[..], b"line1\r\nline2\r\nline3\r\n"].concat(),
    );
    r.commit_all("base");
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write(
        "w.txt",
        &[&bom[..], b"line1\r\nfrom-feature\r\nline3\r\n"].concat(),
    );
    r.commit_all("f");
    r.git(&["checkout", "-q", "main"]);
    r.write(
        "w.txt",
        &[&bom[..], b"line1\r\nfrom-main\r\nline3\r\n"].concat(),
    );
    r.commit_all("m");
    let e = &r.engine;
    e.merge(&r.repo, "feature", MergeFastForward::Allow)
        .await
        .unwrap();
    let f = &e.conflicts(&r.repo).await.unwrap()[0];
    assert_eq!(f.hunks.len(), 1);
    let wt = f.working_tree.clone().unwrap();
    assert!(wt.starts_with(bom));

    let resolved = resolve_all(&wt, ConflictChoice::Theirs).unwrap();
    e.write_working_tree_file(&r.repo, &f.path, &resolved, &conf())
        .await
        .unwrap();
    let expected = [&bom[..], b"line1\r\nfrom-feature\r\nline3\r\n"].concat();
    assert_eq!(r.read("w.txt"), expected);
    e.mark_resolved(&r.repo, &f.path).await.unwrap();
    e.continue_integration(&r.repo).await.unwrap();
    assert_eq!(r.read("w.txt"), expected);
}

// ---------------------------------------------------------------------------
// write_working_tree_file
// ---------------------------------------------------------------------------

#[tokio::test]
async fn write_creates_nested_files_without_staging_and_leaves_no_temp_files() {
    let r = TestRepo::with_initial_commit();
    r.engine
        .write_working_tree_file(&r.repo, &p("dir/sub/x.bin"), &[0, 255, 1], &conf())
        .await
        .unwrap();
    assert_eq!(r.read("dir/sub/x.bin"), [0, 255, 1]);
    assert_eq!(r.git(&["diff", "--cached", "--name-only"]), "");
    let leftovers: Vec<_> = std::fs::read_dir(r.path().join("dir/sub"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers.len(), 1);
}

#[tokio::test]
async fn write_path_escape_does_not_write() {
    let r = TestRepo::with_initial_commit();
    let outside = r
        .path()
        .parent()
        .unwrap()
        .join(format!("ariadna-escape-{}.txt", std::process::id()));
    let rel = format!("../{}", outside.file_name().unwrap().to_string_lossy());
    for bad in [
        rel.as_str(),
        "a/../../x.txt",
        "/etc/x",
        ".git/hooks/pre-commit",
        "a\\b",
    ] {
        let err = r
            .engine
            .write_working_tree_file(&r.repo, &GitPath::from(bad), b"no", &conf())
            .await
            .unwrap_err();
        assert!(
            matches!(err, GitError::PathEscapesRepository { .. }),
            "{bad}: {err:?}"
        );
    }
    assert!(!outside.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn write_through_symlinked_directory_is_rejected() {
    let r = TestRepo::with_initial_commit();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), r.path().join("link")).unwrap();
    let err = r
        .engine
        .write_working_tree_file(&r.repo, &p("link/x.txt"), b"no", &conf())
        .await
        .unwrap_err();
    assert!(
        matches!(err, GitError::PathEscapesRepository { .. }),
        "{err:?}"
    );
    // Tampoco con directorios nuevos debajo del enlace.
    let err = r
        .engine
        .write_working_tree_file(&r.repo, &p("link/nuevo/x.txt"), b"no", &conf())
        .await
        .unwrap_err();
    assert!(
        matches!(err, GitError::PathEscapesRepository { .. }),
        "{err:?}"
    );
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn write_over_symlinked_file_replaces_the_link_and_never_touches_the_target() {
    let r = TestRepo::with_initial_commit();
    let outside = tempfile::tempdir().unwrap();
    let victim = outside.path().join("victima.txt");
    std::fs::write(&victim, b"intacto").unwrap();
    std::os::unix::fs::symlink(&victim, r.path().join("evil.txt")).unwrap();
    r.engine
        .write_working_tree_file(&r.repo, &p("evil.txt"), b"nuevo", &conf())
        .await
        .unwrap();
    assert_eq!(std::fs::read(&victim).unwrap(), b"intacto");
    assert_eq!(r.read("evil.txt"), b"nuevo");
    assert!(!std::fs::symlink_metadata(r.path().join("evil.txt"))
        .unwrap()
        .file_type()
        .is_symlink());
}

#[cfg(unix)]
#[tokio::test]
async fn write_keeps_the_executable_bit() {
    use std::os::unix::fs::PermissionsExt;
    let r = TestRepo::with_initial_commit();
    r.write("run.sh", b"#!/bin/sh\n");
    std::fs::set_permissions(
        r.path().join("run.sh"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    r.engine
        .write_working_tree_file(&r.repo, &p("run.sh"), b"#!/bin/sh\necho\n", &conf())
        .await
        .unwrap();
    let mode = std::fs::metadata(r.path().join("run.sh"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o111, 0o111);
}

// ---------------------------------------------------------------------------
// Remotos
// ---------------------------------------------------------------------------

#[tokio::test]
async fn add_and_list_remotes_with_validation() {
    let r = TestRepo::with_initial_commit();
    let e = &r.engine;
    e.add_remote(
        &r.repo,
        "origin",
        "https://user:secreto@example.com/x/y.git",
    )
    .await
    .unwrap();
    e.add_remote(&r.repo, "backup", "/tmp/algún sitio/con espacio.git")
        .await
        .unwrap();
    let remotes = e.list_remotes(&r.repo).await.unwrap();
    assert_eq!(
        remotes.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(),
        ["backup", "origin"]
    );
    // Nunca se devuelven credenciales.
    assert_eq!(remotes[1].fetch_url, "https://***@example.com/x/y.git");
    assert!(!remotes[1].push_url.contains("secreto"));
    assert_eq!(remotes[0].fetch_url, "/tmp/algún sitio/con espacio.git");

    for (name, url) in [
        ("-bad", "/x"),
        ("a b", "/x"),
        ("ok", "--upload-pack=touch /tmp/pwn"),
        ("ok", "-x"),
        ("ok", "/x\ny"),
        ("ok", ""),
        ("ok", "/x\u{7}"),
    ] {
        let err = e.add_remote(&r.repo, name, url).await.unwrap_err();
        assert!(
            matches!(err, GitError::InvalidReference(_)),
            "{name} {url:?}: {err:?}"
        );
    }
    assert_eq!(e.list_remotes(&r.repo).await.unwrap().len(), 2);
    // Un remoto repetido es un error de git.
    let err = e.add_remote(&r.repo, "origin", "/otro").await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
}

#[tokio::test]
async fn push_set_upstream_then_branches_report_it() {
    let r = TestRepo::with_initial_commit();
    let bare = r.with_bare_origin();
    let e = &r.engine;
    assert_eq!(e.list_remotes(&r.repo).await.unwrap()[0].name, "origin");
    assert_eq!(e.branches(&r.repo).await.unwrap()[0].upstream, None);

    e.push(&r.repo, "origin", Some("main"), true).await.unwrap();
    let main = &e.branches(&r.repo).await.unwrap()[0];
    assert_eq!(main.upstream.as_deref(), Some("origin/main"));
    assert_eq!((main.ahead, main.behind), (0, 0));
    assert_eq!(
        r.git(&["rev-parse", "HEAD"]),
        std::process::Command::new(common::git_path())
            .args(["-C", bare.path().to_str().unwrap(), "rev-parse", "main"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap()
    );

    // Sin refspec y con set_upstream empuja la rama actual.
    r.git(&["checkout", "-q", "-b", "nueva"]);
    r.write("n.txt", b"n\n");
    r.commit_all("n");
    e.push(&r.repo, "origin", None, true).await.unwrap();
    let nueva = e
        .branches(&r.repo)
        .await
        .unwrap()
        .into_iter()
        .find(|b| b.name == "nueva")
        .unwrap();
    assert_eq!(nueva.upstream.as_deref(), Some("origin/nueva"));

    // Un commit local más: ahead 1.
    r.write("m.txt", b"m\n");
    r.commit_all("m");
    let nueva = e
        .branches(&r.repo)
        .await
        .unwrap()
        .into_iter()
        .find(|b| b.name == "nueva")
        .unwrap();
    assert_eq!((nueva.ahead, nueva.behind), (1, 0));
    e.push(&r.repo, "origin", Some("nueva"), false)
        .await
        .unwrap();
}

#[tokio::test]
async fn push_without_refspec_on_detached_head_with_set_upstream_is_rejected() {
    let r = TestRepo::with_initial_commit();
    let _bare = r.with_bare_origin();
    r.git(&["checkout", "-q", "--detach"]);
    let err = r
        .engine
        .push(&r.repo, "origin", None, true)
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::InvalidReference(_)), "{err:?}");
}

#[tokio::test]
async fn fetch_updates_remote_tracking_branches_without_touching_local() {
    let (origin, bare) = origin_with_bare();
    let clone = clone_of(&bare);
    origin.write("nuevo.txt", b"x\n");
    let new_tip = origin.commit_all("nuevo en origen");
    origin.git(&["push", "-q", "origin", "main"]);

    let local_before = clone.git(&["rev-parse", "main"]);
    clone.engine.fetch(&clone.repo, "origin").await.unwrap();
    assert_eq!(clone.git(&["rev-parse", "main"]), local_before);
    let tracking = clone
        .engine
        .remote_tracking_branches(&clone.repo)
        .await
        .unwrap();
    assert_eq!(names(&tracking), ["origin/main"]);
    assert_eq!(tracking[0].oid, new_tip);
    let main = &clone.engine.branches(&clone.repo).await.unwrap()[0];
    assert_eq!((main.ahead, main.behind), (0, 1));

    let err = clone
        .engine
        .fetch(&clone.repo, "no-existe")
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
    let err = clone.engine.fetch(&clone.repo, "--all").await.unwrap_err();
    assert!(matches!(err, GitError::InvalidReference(_)), "{err:?}");
}

#[tokio::test]
async fn pull_fast_forwards_from_bare_remote() {
    let (origin, bare) = origin_with_bare();
    let clone = clone_of(&bare);
    clone.write("README.md", b"v2\n");
    let v2 = clone.commit_all("v2");
    clone.git(&["push", "-q", "origin", "main"]);

    let res = origin
        .engine
        .pull(&origin.repo, "origin", Some("main"), false)
        .await
        .unwrap();
    assert_eq!(res, MergeResult::FastForward { new_head: v2 });
    assert_eq!(origin.read("README.md"), b"v2\n");
    assert_eq!(origin.git(&["log", "--format=%s", "-1"]).trim(), "v2");

    let again = origin
        .engine
        .pull(&origin.repo, "origin", Some("main"), false)
        .await
        .unwrap();
    assert_eq!(again, MergeResult::UpToDate);
}

#[tokio::test]
async fn pull_with_diverged_histories_creates_a_merge() {
    let (origin, bare) = origin_with_bare();
    let clone = clone_of(&bare);
    clone.write("c.txt", b"clone\n");
    clone.commit_all("clone");
    clone.git(&["push", "-q", "origin", "main"]);
    origin.write("o.txt", b"origen\n");
    origin.commit_all("origen");
    let res = origin
        .engine
        .pull(&origin.repo, "origin", Some("main"), false)
        .await
        .unwrap();
    assert!(matches!(res, MergeResult::Merged { .. }), "{res:?}");
    assert_eq!(
        origin
            .git(&["rev-list", "--parents", "-n1", "HEAD"])
            .split(' ')
            .count(),
        3
    );
}

#[tokio::test]
async fn pull_rebase_replays_local_commit() {
    let (origin, bare) = origin_with_bare();
    let clone = clone_of(&bare);
    clone.write("b.txt", b"local\n");
    clone.commit_all("local");
    origin.write("c.txt", b"remoto\n");
    origin.commit_all("remoto");
    origin.git(&["push", "-q", "origin", "main"]);

    let res = clone
        .engine
        .pull(&clone.repo, "origin", Some("main"), true)
        .await
        .unwrap();
    assert!(matches!(res, MergeResult::Merged { .. }), "{res:?}");
    let subjects = clone.git(&["log", "--format=%s", "-3"]);
    assert_eq!(
        subjects.lines().collect::<Vec<_>>(),
        ["local", "remoto", "inicial"]
    );
    assert_eq!(
        clone.engine.integration_state(&clone.repo).await.unwrap(),
        IntegrationState::None
    );
}

#[tokio::test]
async fn pull_rebase_conflict_abort_and_continue() {
    let origin = TestRepo::init();
    origin.write("file.txt", b"line1\nline2\nline3\n");
    origin.commit_all("base");
    let bare = origin.with_bare_origin();
    origin.git(&["push", "-q", "-u", "origin", "main"]);
    let clone = clone_of(&bare);

    origin.write("file.txt", b"line1\nfrom-origin\nline3\n");
    origin.commit_all("origin");
    origin.git(&["push", "-q", "origin", "main"]);
    clone.write("file.txt", b"line1\nfrom-clone\nline3\n");
    clone.commit_all("clone");
    let e = &clone.engine;

    let res = e
        .pull(&clone.repo, "origin", Some("main"), true)
        .await
        .unwrap();
    assert_eq!(
        res,
        MergeResult::Conflicts {
            paths: vec![p("file.txt")]
        }
    );
    assert_eq!(
        e.integration_state(&clone.repo).await.unwrap(),
        IntegrationState::Rebasing
    );

    e.abort_integration(&clone.repo).await.unwrap();
    assert_eq!(
        e.integration_state(&clone.repo).await.unwrap(),
        IntegrationState::None
    );
    assert_eq!(clone.git(&["status", "--porcelain"]), "");
    assert_eq!(clone.git(&["branch", "--show-current"]).trim(), "main");

    let again = e
        .pull(&clone.repo, "origin", Some("main"), true)
        .await
        .unwrap();
    assert!(matches!(again, MergeResult::Conflicts { .. }), "{again:?}");
    e.write_working_tree_file(
        &clone.repo,
        &p("file.txt"),
        b"line1\nresuelto\nline3\n",
        &conf(),
    )
    .await
    .unwrap();
    e.mark_resolved(&clone.repo, &p("file.txt")).await.unwrap();
    e.continue_integration(&clone.repo).await.unwrap();
    assert_eq!(
        e.integration_state(&clone.repo).await.unwrap(),
        IntegrationState::None
    );
    assert_eq!(clone.read("file.txt"), b"line1\nresuelto\nline3\n");
    let subjects = clone.git(&["log", "--format=%s", "-3"]);
    assert_eq!(
        subjects.lines().collect::<Vec<_>>(),
        ["clone", "origin", "base"]
    );
}

#[tokio::test]
async fn pull_merge_conflict_leaves_merge_in_progress() {
    let origin = TestRepo::init();
    origin.write("file.txt", b"line1\nline2\nline3\n");
    origin.commit_all("base");
    let bare = origin.with_bare_origin();
    origin.git(&["push", "-q", "-u", "origin", "main"]);
    let clone = clone_of(&bare);
    origin.write("file.txt", b"line1\nfrom-origin\nline3\n");
    origin.commit_all("origin");
    origin.git(&["push", "-q", "origin", "main"]);
    clone.write("file.txt", b"line1\nfrom-clone\nline3\n");
    clone.commit_all("clone");
    let res = clone
        .engine
        .pull(&clone.repo, "origin", Some("main"), false)
        .await
        .unwrap();
    assert_eq!(
        res,
        MergeResult::Conflicts {
            paths: vec![p("file.txt")]
        }
    );
    assert_eq!(
        clone.engine.integration_state(&clone.repo).await.unwrap(),
        IntegrationState::Merging
    );
}
