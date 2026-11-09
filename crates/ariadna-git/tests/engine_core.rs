//! Operaciones núcleo del motor contra `git` de verdad en repos temporales.
//! Port de los tests de `GitCLIEngineTests.swift` de esta área más los de endurecimiento
//! (spec §4.1): rutas raras, CRLF, 5.000 archivos, HEAD detached, amend publicado y repos
//! hostiles.
#![allow(clippy::unwrap_used)]

mod common;

use ariadna_git::{
    Confirmation, DiffHunk, DiffScope, GitError, GitPath, GitStatus, Repository, StatusEntry,
};
use common::TestRepo;

fn gp(s: &str) -> GitPath {
    GitPath::from(s)
}

fn paths(list: &[&str]) -> Vec<GitPath> {
    list.iter().map(|s| gp(s)).collect()
}

fn names(entries: &[&StatusEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|e| e.path.to_string_lossy().into_owned())
        .collect()
}

fn numbered(n: usize) -> String {
    (1..=n).map(|i| format!("line{i}\n")).collect()
}

fn entry<'a>(status: &'a GitStatus, path: &str) -> &'a StatusEntry {
    status
        .entries
        .iter()
        .find(|e| e.path == *path)
        .unwrap_or_else(|| panic!("sin entrada para {path}"))
}

async fn open(t: &TestRepo) -> Repository {
    t.engine.open_repository(t.path()).await.unwrap()
}

fn bogus_hunk() -> DiffHunk {
    DiffHunk {
        id: "no-esta".into(),
        old_start: 40,
        old_count: 1,
        new_start: 40,
        new_count: 1,
        heading: String::new(),
        lines: vec![],
        raw: b"@@ -40 +40 @@\n".to_vec(),
    }
}

// ---------------------------------------------------------------------------------------
// open / status / stage / commit / log
// ---------------------------------------------------------------------------------------

#[tokio::test]
async fn open_status_stage_commit_and_log() {
    let t = TestRepo::init();
    t.write("a.txt", b"uno\n");
    // Se abre desde un archivo: sube hasta la raíz.
    let repo = t
        .engine
        .open_repository(&t.path().join("a.txt"))
        .await
        .unwrap();
    assert_eq!(repo.path, dunce::canonicalize(t.path()).unwrap());
    assert_eq!(
        repo.git_dir,
        dunce::canonicalize(t.path().join(".git")).unwrap()
    );

    let status = t.engine.status(&repo, false).await.unwrap();
    assert_eq!(status.branch_name.as_deref(), Some("main"));
    assert!(status.is_unborn);
    assert_eq!(names(&status.unstaged()), ["a.txt"]);

    t.engine.stage(&repo, &paths(&["a.txt"])).await.unwrap();
    let status = t.engine.status(&repo, false).await.unwrap();
    assert_eq!(names(&status.staged()), ["a.txt"]);
    assert!(status.unstaged().is_empty());

    let sha = t
        .engine
        .commit(&repo, "asunto corto\n\ncuerpo del cambio\n", false)
        .await
        .unwrap();
    assert_eq!(sha.len(), 40);
    let commits = t.engine.log(&repo, 10).await.unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].subject, "asunto corto");
    assert_eq!(commits[0].body, "cuerpo del cambio");
    assert_eq!(commits[0].sha, sha);
    assert_eq!(commits[0].author_name, "Test");
    assert!(commits[0].parents.is_empty());
}

#[tokio::test]
async fn open_repository_from_subfolder_and_outside() {
    let t = TestRepo::with_initial_commit();
    t.write("sub/dir/x.txt", b"x\n");
    let repo = t
        .engine
        .open_repository(&t.path().join("sub/dir"))
        .await
        .unwrap();
    assert_eq!(repo.path, dunce::canonicalize(t.path()).unwrap());

    let outside = tempfile::tempdir().unwrap();
    let err = t.engine.open_repository(outside.path()).await.unwrap_err();
    assert!(matches!(err, GitError::NotARepository { .. }), "{err:?}");
    let err = t
        .engine
        .open_repository(&outside.path().join("no-existe"))
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::NotARepository { .. }), "{err:?}");
}

#[tokio::test]
async fn empty_commit_message_and_nothing_staged() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    let err = t.engine.commit(&repo, "   \n", false).await.unwrap_err();
    assert_eq!(err, GitError::EmptyCommitMessage);

    // Sin commits y sin nada preparado.
    let err = t.engine.commit(&repo, "primero", false).await.unwrap_err();
    assert_eq!(err, GitError::NothingToCommit);

    t.write("a.txt", b"x\n");
    t.engine.stage(&repo, &paths(&["a.txt"])).await.unwrap();
    t.engine.commit(&repo, "primero", false).await.unwrap();
    let err = t.engine.commit(&repo, "segundo", false).await.unwrap_err();
    assert_eq!(err, GitError::NothingToCommit);
    // Cambios sin preparar tampoco cuentan.
    t.write("a.txt", b"y\n");
    let err = t.engine.commit(&repo, "tercero", false).await.unwrap_err();
    assert_eq!(err, GitError::NothingToCommit);
    assert_eq!(t.engine.log(&repo, 10).await.unwrap().len(), 1);
}

#[tokio::test]
async fn commit_message_goes_through_stdin_verbatim() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    t.write("a.txt", b"x\n");
    t.engine.stage(&repo, &paths(&["a.txt"])).await.unwrap();
    let msg = "Añade «ñandú» -rf --amend\n\ncuerpo línea 1\nlínea 2 con $HOME y `cmd`\n";
    t.engine.commit(&repo, msg, false).await.unwrap();
    let c = &t.engine.log(&repo, 1).await.unwrap()[0];
    assert_eq!(c.subject, "Añade «ñandú» -rf --amend");
    assert_eq!(c.body, "cuerpo línea 1\nlínea 2 con $HOME y `cmd`");
}

#[tokio::test]
async fn log_is_empty_without_commits_and_respects_limit() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    assert!(t.engine.log(&repo, 10).await.unwrap().is_empty());
    for i in 0..5 {
        t.write("a.txt", format!("{i}\n").as_bytes());
        t.commit_all(&format!("c{i}"));
    }
    let two = t.engine.log(&repo, 2).await.unwrap();
    assert_eq!(
        two.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(),
        ["c4", "c3"]
    );
    assert_eq!(two[0].parents.len(), 1);
    // Un límite de 0 se trata como 1 (igual que el Swift).
    assert_eq!(t.engine.log(&repo, 0).await.unwrap().len(), 1);
    assert!(t
        .engine
        .status(&repo, false)
        .await
        .unwrap()
        .branch_name
        .is_some());
}

#[tokio::test]
async fn status_reports_ignored_only_when_asked() {
    let t = TestRepo::with_initial_commit();
    t.write(".gitignore", b"*.log\n");
    t.write("debug.log", b"x\n");
    let repo = open(&t).await;
    let plain = t.engine.status(&repo, false).await.unwrap();
    assert!(plain.ignored().is_empty());
    let with = t.engine.status(&repo, true).await.unwrap();
    assert_eq!(names(&with.ignored()), ["debug.log"]);
    assert!(entry(&with, "debug.log").is_ignored);
}

// ---------------------------------------------------------------------------------------
// stage / unstage de archivos
// ---------------------------------------------------------------------------------------

#[tokio::test]
async fn unstage_works_with_and_without_head() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    t.write("a.txt", b"a\n");
    t.write("b.txt", b"b\n");
    t.engine
        .stage(&repo, &paths(&["a.txt", "b.txt"]))
        .await
        .unwrap();
    // Sin commits: `restore --staged` no sirve, el motor usa `reset`.
    t.engine.unstage(&repo, &paths(&["a.txt"])).await.unwrap();
    let st = t.engine.status(&repo, false).await.unwrap();
    assert_eq!(names(&st.staged()), ["b.txt"]);
    assert!(entry(&st, "a.txt").is_untracked);

    t.engine.commit(&repo, "base", false).await.unwrap();
    t.write("b.txt", b"b2\n");
    t.engine
        .stage(&repo, &paths(&["b.txt", "a.txt"]))
        .await
        .unwrap();
    t.engine.unstage(&repo, &paths(&["b.txt"])).await.unwrap();
    let st = t.engine.status(&repo, false).await.unwrap();
    assert_eq!(names(&st.staged()), ["a.txt"]);
    assert_eq!(names(&st.unstaged()), ["b.txt"]);

    // Lista vacía: no hace nada.
    t.engine.stage(&repo, &[]).await.unwrap();
    t.engine.unstage(&repo, &[]).await.unwrap();
}

#[tokio::test]
async fn stage_handles_deletions() {
    let t = TestRepo::with_initial_commit();
    let repo = open(&t).await;
    std::fs::remove_file(t.path().join("README.md")).unwrap();
    t.engine.stage(&repo, &paths(&["README.md"])).await.unwrap();
    let st = t.engine.status(&repo, false).await.unwrap();
    assert_eq!(names(&st.staged()), ["README.md"]);
    assert!(st.unstaged().is_empty());
}

#[tokio::test]
async fn stage_and_unstage_five_thousand_files() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    let mut list = Vec::new();
    for i in 0..5_000 {
        let rel = format!("bulk/d{}/archivo con espacio {i}.txt", i % 50);
        t.write(&rel, format!("{i}\n").as_bytes());
        list.push(GitPath::from(rel));
    }
    t.engine.stage(&repo, &list).await.unwrap();
    let st = t.engine.status(&repo, false).await.unwrap();
    assert_eq!(st.staged().len(), 5_000);
    assert!(st.unstaged().is_empty());
    t.engine.commit(&repo, "masivo", false).await.unwrap();

    for (i, p) in list.iter().enumerate() {
        t.write(&p.to_string_lossy(), format!("cambio {i}\n").as_bytes());
    }
    t.engine.stage(&repo, &list).await.unwrap();
    assert_eq!(
        t.engine.status(&repo, false).await.unwrap().staged().len(),
        5_000
    );
    t.engine.unstage(&repo, &list).await.unwrap();
    let st = t.engine.status(&repo, false).await.unwrap();
    assert!(st.staged().is_empty());
    assert_eq!(st.unstaged().len(), 5_000);
}

#[tokio::test]
async fn paths_with_spaces_unicode_and_dash_are_literal() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    for name in [
        "-rf",
        "--all",
        "notas/cañón archivo.txt",
        "日本語/ファイル.txt",
        "a b",
    ] {
        t.write(name, b"x\n");
    }
    let all = paths(&[
        "-rf",
        "--all",
        "notas/cañón archivo.txt",
        "日本語/ファイル.txt",
        "a b",
    ]);
    t.engine.stage(&repo, &all[..4]).await.unwrap();
    let st = t.engine.status(&repo, false).await.unwrap();
    let mut staged = names(&st.staged());
    staged.sort();
    let mut expected: Vec<String> = all[..4]
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    expected.sort();
    assert_eq!(staged, expected);
    assert!(entry(&st, "a b").is_untracked);
    t.engine.commit(&repo, "raros", false).await.unwrap();

    t.write("-rf", b"y\n");
    t.engine
        .restore_worktree(&repo, &paths(&["-rf"]), &Confirmation::granted("test"))
        .await
        .unwrap();
    assert_eq!(t.read("-rf"), b"x\n");
}

#[cfg(unix)]
#[tokio::test]
async fn glob_characters_in_names_are_not_expanded() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    t.write("a.txt", b"a\n");
    t.write("*.txt", b"star\n");
    t.write("[a].txt", b"br\n");
    t.engine.stage(&repo, &paths(&["*.txt"])).await.unwrap();
    let st = t.engine.status(&repo, false).await.unwrap();
    assert_eq!(names(&st.staged()), ["*.txt"]);
    assert!(entry(&st, "a.txt").is_untracked);
    assert!(entry(&st, "[a].txt").is_untracked);
}

#[tokio::test]
async fn unsafe_paths_are_rejected_before_git_runs() {
    let t = TestRepo::with_initial_commit();
    let repo = open(&t).await;
    t.write("ok.txt", b"ok\n");
    let bad = [
        "../fuera.txt",
        "/etc/passwd",
        ".git/config",
        "a/../../b",
        ":(glob)*",
        ":/",
        "a\\b",
        "",
        "dir//x",
        "C:evil",
    ];
    for b in bad {
        let list = vec![gp("ok.txt"), gp(b)];
        let err = t.engine.stage(&repo, &list).await.unwrap_err();
        assert!(
            matches!(err, GitError::PathEscapesRepository { .. }),
            "{b:?}: {err:?}"
        );
        let err = t.engine.unstage(&repo, &list).await.unwrap_err();
        assert!(
            matches!(err, GitError::PathEscapesRepository { .. }),
            "{b:?}: {err:?}"
        );
        let err = t
            .engine
            .restore_worktree(&repo, &list, &Confirmation::granted("x"))
            .await
            .unwrap_err();
        assert!(
            matches!(err, GitError::PathEscapesRepository { .. }),
            "{b:?}: {err:?}"
        );
        let err = t
            .engine
            .diff(&repo, &DiffScope::Unstaged, std::slice::from_ref(&gp(b)))
            .await
            .unwrap_err();
        assert!(
            matches!(err, GitError::PathEscapesRepository { .. }),
            "{b:?}: {err:?}"
        );
        let err = t
            .engine
            .stage_hunk(&repo, &bogus_hunk(), &gp(b))
            .await
            .unwrap_err();
        assert!(
            matches!(err, GitError::PathEscapesRepository { .. }),
            "{b:?}: {err:?}"
        );
    }
    // Ninguna de las rutas válidas de esas listas llegó a prepararse.
    let st = t.engine.status(&repo, false).await.unwrap();
    assert!(st.staged().is_empty());
    assert!(entry(&st, "ok.txt").is_untracked);
}

// ---------------------------------------------------------------------------------------
// restore_worktree
// ---------------------------------------------------------------------------------------

#[tokio::test]
async fn restore_worktree_discards_changes_but_leaves_untracked() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    t.write("a.txt", b"original\n");
    t.write("b.txt", b"b\n");
    t.engine
        .stage(&repo, &paths(&["a.txt", "b.txt"]))
        .await
        .unwrap();
    t.engine.commit(&repo, "base", false).await.unwrap();
    t.write("a.txt", b"cambiado\n");
    t.write("b.txt", b"cambiado b\n");
    t.write("nuevo.txt", b"sin seguimiento\n");

    let ok = Confirmation::granted("descartar cambios");
    t.engine
        .restore_worktree(&repo, &paths(&["a.txt"]), &ok)
        .await
        .unwrap();
    assert_eq!(t.read("a.txt"), b"original\n");
    assert_eq!(t.read("b.txt"), b"cambiado b\n");

    // Un archivo sin seguimiento no es restaurable: git falla y no borra nada.
    let err = t
        .engine
        .restore_worktree(&repo, &paths(&["b.txt", "nuevo.txt"]), &ok)
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
    assert_eq!(t.read("nuevo.txt"), b"sin seguimiento\n");
    assert_eq!(t.read("b.txt"), b"cambiado b\n");

    // Restaurar solo el árbol de trabajo no toca lo preparado.
    t.write("a.txt", b"preparado\n");
    t.engine.stage(&repo, &paths(&["a.txt"])).await.unwrap();
    t.write("a.txt", b"mas cambios\n");
    t.engine
        .restore_worktree(&repo, &paths(&["a.txt"]), &ok)
        .await
        .unwrap();
    assert_eq!(t.read("a.txt"), b"preparado\n");
    assert_eq!(t.git(&["show", ":a.txt"]), "preparado\n");
}

// ---------------------------------------------------------------------------------------
// hunks
// ---------------------------------------------------------------------------------------

struct HunkCase {
    file: &'static str,
    eol: &'static str,
    trailing_newline: bool,
    early: usize,
    late: usize,
    autocrlf: &'static str,
}

impl HunkCase {
    fn lines(&self, early: bool, late: bool) -> Vec<String> {
        let mut v: Vec<String> = (1..=30).map(|i| format!("line{i}")).collect();
        if early {
            v[self.early] = "CHANGED-EARLY".into();
        }
        if late {
            v[self.late] = "CHANGED-LATE".into();
        }
        v
    }

    fn render(&self, lines: &[String], eol: &str) -> String {
        let mut s = lines.join(eol);
        if self.trailing_newline {
            s.push_str(eol);
        }
        s
    }
}

/// Dos cambios aislados; prepara el hunk `pick` (0 o 1), comprueba el índice byte a byte y lo
/// deshace con `unstage_hunk`.
async fn stage_one_of_two(c: &HunkCase, pick: usize) {
    let t = TestRepo::init();
    t.git(&["config", "core.autocrlf", c.autocrlf]);
    let repo = open(&t).await;
    t.write(c.file, c.render(&c.lines(false, false), c.eol).as_bytes());
    t.commit_all("base");
    let committed = t.git(&["show", &format!("HEAD:{}", c.file)]);

    t.write(c.file, c.render(&c.lines(true, true), c.eol).as_bytes());
    let path = gp(c.file);
    let unstaged = t
        .engine
        .diff(&repo, &DiffScope::Unstaged, std::slice::from_ref(&path))
        .await
        .unwrap();
    assert_eq!(unstaged.len(), 1);
    assert_eq!(unstaged[0].hunks.len(), 2, "{}", c.file);
    t.engine
        .stage_hunk(&repo, &unstaged[0].hunks[pick], &path)
        .await
        .unwrap();

    // El índice tiene exactamente el cambio elegido (y solo ese), con los finales de línea
    // que git guarda según autocrlf.
    let index_eol = if c.autocrlf == "false" { c.eol } else { "\n" };
    let expected = c.render(&c.lines(pick == 0, pick == 1), index_eol);
    assert_eq!(
        t.git(&["show", &format!(":{}", c.file)]),
        expected,
        "{}",
        c.file
    );
    // El árbol de trabajo no se tocó.
    assert_eq!(
        t.read(c.file),
        c.render(&c.lines(true, true), c.eol).as_bytes()
    );

    let staged = t
        .engine
        .diff(&repo, &DiffScope::Staged, std::slice::from_ref(&path))
        .await
        .unwrap();
    assert_eq!(staged[0].hunks.len(), 1);
    let still = t
        .engine
        .diff(&repo, &DiffScope::Unstaged, std::slice::from_ref(&path))
        .await
        .unwrap();
    assert_eq!(still[0].hunks.len(), 1);

    // Deshacer: el índice vuelve a ser el de HEAD.
    t.engine
        .unstage_hunk(&repo, &staged[0].hunks[0], &path)
        .await
        .unwrap();
    assert_eq!(t.git(&["show", &format!(":{}", c.file)]), committed);
    let (code, _, _) = t.git_raw(&["diff", "--cached", "--quiet"]);
    assert_eq!(code, 0);
}

fn case(file: &'static str, eol: &'static str, trailing: bool, autocrlf: &'static str) -> HunkCase {
    HunkCase {
        file,
        eol,
        trailing_newline: trailing,
        early: 1,
        late: 19,
        autocrlf,
    }
}

#[tokio::test]
async fn stage_isolated_and_contiguous_hunks() {
    for pick in [0, 1] {
        stage_one_of_two(&case("aislado.txt", "\n", true, "false"), pick).await;
        // Separados por 7 líneas sin cambio: hunks casi contiguos (contextos a un paso).
        let mut near = case("contiguo.txt", "\n", true, "false");
        near.late = 9;
        stage_one_of_two(&near, pick).await;
    }
}

#[tokio::test]
async fn stage_hunk_with_crlf_under_every_autocrlf() {
    for mode in ["false", "true", "input"] {
        for pick in [0, 1] {
            stage_one_of_two(&case("crlf.txt", "\r\n", true, mode), pick).await;
        }
    }
}

#[tokio::test]
async fn stage_hunk_with_missing_final_newline() {
    for pick in [0, 1] {
        stage_one_of_two(&case("sin-final.txt", "\n", false, "false"), pick).await;
        // El último hunk cambia justo la última línea, que no termina en salto.
        let mut last = case("sin-final-ultima.txt", "\n", false, "false");
        last.late = 29;
        stage_one_of_two(&last, pick).await;
        let mut crlf_last = case("crlf-sin-final.txt", "\r\n", false, "false");
        crlf_last.late = 29;
        stage_one_of_two(&crlf_last, pick).await;
    }
}

#[tokio::test]
async fn stage_hunk_in_unicode_path_with_spaces() {
    for pick in [0, 1] {
        stage_one_of_two(&case("notas/cañón archivo.txt", "\n", true, "false"), pick).await;
        stage_one_of_two(&case("日本語 dir/ファイル.txt", "\n", true, "false"), pick).await;
    }
}

#[tokio::test]
async fn stage_hunk_in_dash_named_file() {
    stage_one_of_two(&case("-rf", "\n", true, "false"), 0).await;
}

#[tokio::test]
async fn stage_single_hunk_and_unstage_it() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    let original = numbered(40);
    t.write("file.txt", original.as_bytes());
    t.engine.stage(&repo, &paths(&["file.txt"])).await.unwrap();
    t.engine.commit(&repo, "base", false).await.unwrap();
    let mut lines: Vec<String> = original.lines().map(str::to_owned).collect();
    lines[1] = "CHANGED-EARLY".into();
    lines[29] = "CHANGED-LATE".into();
    t.write("file.txt", (lines.join("\n") + "\n").as_bytes());

    let p = gp("file.txt");
    let unstaged = t
        .engine
        .diff(&repo, &DiffScope::Unstaged, std::slice::from_ref(&p))
        .await
        .unwrap();
    assert_eq!(unstaged[0].hunks.len(), 2);
    t.engine
        .stage_hunk(&repo, &unstaged[0].hunks[0], &p)
        .await
        .unwrap();

    let staged = t
        .engine
        .diff(&repo, &DiffScope::Staged, std::slice::from_ref(&p))
        .await
        .unwrap();
    let rest = t
        .engine
        .diff(&repo, &DiffScope::Unstaged, std::slice::from_ref(&p))
        .await
        .unwrap();
    let staged_text = String::from_utf8_lossy(&staged[0].raw_bytes()).into_owned();
    let rest_text = String::from_utf8_lossy(&rest[0].raw_bytes()).into_owned();
    assert!(staged_text.contains("CHANGED-EARLY") && !staged_text.contains("CHANGED-LATE"));
    assert!(rest_text.contains("CHANGED-LATE") && !rest_text.contains("CHANGED-EARLY"));

    t.engine
        .unstage_hunk(&repo, &staged[0].hunks[0], &p)
        .await
        .unwrap();
    let after = t
        .engine
        .diff(&repo, &DiffScope::Staged, std::slice::from_ref(&p))
        .await
        .unwrap();
    assert!(after.iter().all(|f| f.hunks.is_empty()));
}

#[tokio::test]
async fn stale_hunk_is_reported_as_not_found() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    t.write("f.txt", numbered(30).as_bytes());
    t.commit_all("base");
    let mut lines: Vec<String> = numbered(30).lines().map(str::to_owned).collect();
    lines[1] = "X".into();
    t.write("f.txt", (lines.join("\n") + "\n").as_bytes());
    let p = gp("f.txt");
    let d = t
        .engine
        .diff(&repo, &DiffScope::Unstaged, std::slice::from_ref(&p))
        .await
        .unwrap();
    let hunk = d[0].hunks[0].clone();
    t.engine.stage_hunk(&repo, &hunk, &p).await.unwrap();
    // El hunk ya está preparado: no queda en el diff sin preparar.
    let err = t.engine.stage_hunk(&repo, &hunk, &p).await.unwrap_err();
    assert!(matches!(err, GitError::HunkNotFound { .. }), "{err:?}");
    // Y un archivo sin cambios tampoco tiene hunks.
    let err = t
        .engine
        .unstage_hunk(&repo, &bogus_hunk(), &p)
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::HunkNotFound { .. }), "{err:?}");
    let err = t
        .engine
        .stage_hunk(&repo, &hunk, &gp("README.md"))
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::HunkNotFound { .. }), "{err:?}");
}

#[tokio::test]
async fn stage_hunk_of_untracked_file() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    t.write("nuevo.txt", b"linea1\nlinea2\nlinea3\n");
    let p = gp("nuevo.txt");
    let preview = t
        .engine
        .diff(&repo, &DiffScope::Unstaged, std::slice::from_ref(&p))
        .await
        .unwrap();
    assert!(preview[0].is_new_file);
    t.engine
        .stage_hunk(&repo, &preview[0].hunks[0], &p)
        .await
        .unwrap();
    let st = t.engine.status(&repo, false).await.unwrap();
    let e = entry(&st, "nuevo.txt");
    assert!(!e.is_untracked && e.is_staged());
    assert_eq!(t.git(&["show", ":nuevo.txt"]), "linea1\nlinea2\nlinea3\n");
    let staged = t
        .engine
        .diff(&repo, &DiffScope::Staged, std::slice::from_ref(&p))
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&staged[0].raw_bytes()).contains("linea2"));
}

#[tokio::test]
async fn failed_untracked_hunk_leaves_no_intent_to_add() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    t.write("nuevo.txt", b"linea1\nlinea2\n");
    let err = t
        .engine
        .stage_hunk(&repo, &bogus_hunk(), &gp("nuevo.txt"))
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::HunkNotFound { .. }), "{err:?}");
    let st = t.engine.status(&repo, false).await.unwrap();
    let e = entry(&st, "nuevo.txt");
    assert!(e.is_untracked && !e.is_staged());
    assert_eq!(t.git(&["ls-files", "--stage"]), "");
}

#[tokio::test]
async fn stage_hunk_of_untracked_dash_file() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    t.write("-rf", b"a\nb\n");
    let p = gp("-rf");
    let d = t
        .engine
        .diff(&repo, &DiffScope::Unstaged, std::slice::from_ref(&p))
        .await
        .unwrap();
    assert_eq!(d[0].path, "-rf");
    t.engine
        .stage_hunk(&repo, &d[0].hunks[0], &p)
        .await
        .unwrap();
    assert_eq!(t.git(&["show", ":./-rf"]), "a\nb\n");
}

// ---------------------------------------------------------------------------------------
// diff
// ---------------------------------------------------------------------------------------

#[tokio::test]
async fn diff_scopes_commit_between_and_root() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    t.write("a.txt", b"uno\n");
    let root = t.commit_all("raiz");
    t.write("a.txt", b"uno\ndos\n");
    t.write("b.txt", b"b\n");
    let second = t.commit_all("segundo");

    // Commit inicial (sin padre).
    let d = t
        .engine
        .diff(&repo, &DiffScope::Commit(root.clone()), &[])
        .await
        .unwrap();
    assert_eq!(d.len(), 1);
    assert!(d[0].is_new_file);
    assert_eq!(d[0].path, "a.txt");

    let d = t
        .engine
        .diff(&repo, &DiffScope::Commit(second.clone()), &[])
        .await
        .unwrap();
    let mut got: Vec<String> = d
        .iter()
        .map(|f| f.path.to_string_lossy().into_owned())
        .collect();
    got.sort();
    assert_eq!(got, ["a.txt", "b.txt"]);
    let d = t
        .engine
        .diff(
            &repo,
            &DiffScope::Commit(second.clone()),
            &paths(&["b.txt"]),
        )
        .await
        .unwrap();
    assert_eq!(d.len(), 1);

    let d = t
        .engine
        .diff(
            &repo,
            &DiffScope::Between {
                base: root.clone(),
                head: second.clone(),
            },
            &[],
        )
        .await
        .unwrap();
    assert_eq!(d.len(), 2);
    let d = t
        .engine
        .diff(
            &repo,
            &DiffScope::Between {
                base: "HEAD~1".into(),
                head: "HEAD".into(),
            },
            &paths(&["a.txt"]),
        )
        .await
        .unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!(
        d[0].hunks[0]
            .lines
            .iter()
            .filter(|l| l.text.contains("dos"))
            .count(),
        1
    );

    for bad in ["--output=/tmp/x", "-p", "", "a b"] {
        let err = t
            .engine
            .diff(&repo, &DiffScope::Commit(bad.into()), &[])
            .await
            .unwrap_err();
        assert!(
            matches!(err, GitError::InvalidReference(_)),
            "{bad:?}: {err:?}"
        );
    }
    let err = t
        .engine
        .diff(&repo, &DiffScope::Commit("noexiste".into()), &[])
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
}

#[tokio::test]
async fn diff_of_merge_commit_uses_first_parent() {
    let t = TestRepo::with_initial_commit();
    let repo = open(&t).await;
    t.git(&["checkout", "-q", "-b", "feature"]);
    t.write("f.txt", b"f\n");
    t.commit_all("feature");
    t.git(&["checkout", "-q", "main"]);
    t.write("m.txt", b"m\n");
    t.commit_all("main");
    t.git(&["merge", "--no-ff", "-q", "-m", "merge", "feature"]);
    let d = t
        .engine
        .diff(&repo, &DiffScope::Commit("HEAD".into()), &[])
        .await
        .unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].path, "f.txt");
}

#[tokio::test]
async fn diff_unstaged_staged_untracked_and_deleted() {
    let t = TestRepo::with_initial_commit();
    let repo = open(&t).await;
    t.write("a.txt", b"a\n");
    t.write("b.txt", b"b\n");
    t.commit_all("dos");
    t.write("a.txt", b"a2\n");
    t.write("nuevo.txt", b"n\n");
    t.write("dir/x.txt", b"x\n");
    t.write("dir/y.txt", b"y\n");
    std::fs::remove_file(t.path().join("b.txt")).unwrap();

    // Sin rutas: solo seguidos (como el Swift).
    let all = t
        .engine
        .diff(&repo, &DiffScope::Unstaged, &[])
        .await
        .unwrap();
    let mut got: Vec<String> = all
        .iter()
        .map(|f| f.path.to_string_lossy().into_owned())
        .collect();
    got.sort();
    assert_eq!(got, ["a.txt", "b.txt"]);
    assert!(
        all.iter()
            .find(|f| f.path == "b.txt")
            .unwrap()
            .is_deleted_file
    );

    // Con ruta explícita, los archivos sin seguimiento aparecen como «nuevos».
    let d = t
        .engine
        .diff(
            &repo,
            &DiffScope::Unstaged,
            &paths(&["nuevo.txt", "dir", "a.txt"]),
        )
        .await
        .unwrap();
    let mut got: Vec<(String, bool)> = d
        .iter()
        .map(|f| (f.path.to_string_lossy().into_owned(), f.is_new_file))
        .collect();
    got.sort();
    assert_eq!(
        got,
        [
            ("a.txt".to_string(), false),
            ("dir/x.txt".to_string(), true),
            ("dir/y.txt".to_string(), true),
            ("nuevo.txt".to_string(), true),
        ]
    );
    // Lo preparado no cuenta como sin seguimiento y no se duplica.
    t.engine.stage(&repo, &paths(&["nuevo.txt"])).await.unwrap();
    let d = t
        .engine
        .diff(&repo, &DiffScope::Unstaged, &paths(&["nuevo.txt"]))
        .await
        .unwrap();
    assert!(d.is_empty());
    let d = t.engine.diff(&repo, &DiffScope::Staged, &[]).await.unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].path, "nuevo.txt");
}

#[tokio::test]
async fn diff_staged_works_in_unborn_repo_and_ignores_user_prefix_config() {
    let t = TestRepo::init();
    t.git(&["config", "diff.noprefix", "true"]);
    t.git(&["config", "diff.mnemonicPrefix", "true"]);
    let repo = open(&t).await;
    t.write("a.txt", b"uno\n");
    t.engine.stage(&repo, &paths(&["a.txt"])).await.unwrap();
    let d = t.engine.diff(&repo, &DiffScope::Staged, &[]).await.unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].path, "a.txt");
    assert!(d[0].is_new_file);
}

#[tokio::test]
async fn diff_keeps_binary_and_crlf_bytes() {
    let t = TestRepo::with_initial_commit();
    let repo = open(&t).await;
    t.write("c.txt", b"uno\r\ndos\r\n");
    t.write("bin.dat", &[0u8, 1, 2, 255, 0, 3]);
    t.commit_all("base");
    t.write("c.txt", b"uno\r\nDOS\r\n");
    t.write("bin.dat", &[0u8, 9, 9, 255, 0, 4]);
    let d = t
        .engine
        .diff(&repo, &DiffScope::Unstaged, &[])
        .await
        .unwrap();
    let c = d.iter().find(|f| f.path == "c.txt").unwrap();
    assert!(c.raw_bytes().windows(5).any(|w| w == b"DOS\r\n"));
    assert!(d.iter().find(|f| f.path == "bin.dat").unwrap().is_binary);
}

// ---------------------------------------------------------------------------------------
// HEAD detached
// ---------------------------------------------------------------------------------------

#[tokio::test]
async fn detached_head_asks_before_commit() {
    let t = TestRepo::with_initial_commit();
    let repo = open(&t).await;
    t.git(&["checkout", "-q", "--detach"]);
    let st = t.engine.status(&repo, false).await.unwrap();
    assert!(st.is_detached);
    assert!(st.branch_name.is_none());

    t.write("extra.txt", b"extra\n");
    t.engine.stage(&repo, &paths(&["extra.txt"])).await.unwrap();
    let err = t.engine.commit(&repo, "suelto", false).await.unwrap_err();
    assert_eq!(err, GitError::DetachedHeadCommit);
    assert_eq!(t.engine.log(&repo, 10).await.unwrap().len(), 1);

    let sha = t.engine.commit(&repo, "suelto", true).await.unwrap();
    assert_eq!(sha.len(), 40);
    assert!(t.engine.status(&repo, false).await.unwrap().is_detached);
    assert_eq!(t.engine.log(&repo, 10).await.unwrap().len(), 2);
}

// ---------------------------------------------------------------------------------------
// amend
// ---------------------------------------------------------------------------------------

#[tokio::test]
async fn amend_rewrites_unpublished_commit_only() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    let err = t.engine.amend_last_commit(&repo, "nada").await.unwrap_err();
    assert!(matches!(err, GitError::CannotAmend { .. }), "{err:?}");

    t.write("a.txt", b"a\n");
    t.engine.stage(&repo, &paths(&["a.txt"])).await.unwrap();
    let first = t
        .engine
        .commit(&repo, "asunto con erata", false)
        .await
        .unwrap();
    let err = t.engine.amend_last_commit(&repo, "  \n").await.unwrap_err();
    assert_eq!(err, GitError::EmptyCommitMessage);

    t.write("b.txt", b"b\n");
    t.engine.stage(&repo, &paths(&["b.txt"])).await.unwrap();
    let amended = t
        .engine
        .amend_last_commit(&repo, "asunto corregido")
        .await
        .unwrap();
    assert_ne!(amended, first);
    let log = t.engine.log(&repo, 10).await.unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].subject, "asunto corregido");
    let files = t.git(&["show", "--name-only", "--format=", "HEAD"]);
    assert!(files.contains("a.txt") && files.contains("b.txt"));

    // Publicado en un remoto: se niega y no toca nada.
    let _origin = t.with_bare_origin();
    t.git(&["push", "-q", "-u", "origin", "main"]);
    let err = t
        .engine
        .amend_last_commit(&repo, "otra vez")
        .await
        .unwrap_err();
    match err {
        GitError::CannotAmend { reason } => assert!(reason.contains("origin/main"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(t.engine.log(&repo, 1).await.unwrap()[0].sha, amended);

    // Un commit nuevo, aún sin publicar, sí se puede corregir.
    t.write("c.txt", b"c\n");
    t.commit_all("local");
    let local = t
        .engine
        .amend_last_commit(&repo, "local corregido")
        .await
        .unwrap();
    assert_eq!(
        t.engine.log(&repo, 1).await.unwrap()[0].subject,
        "local corregido"
    );
    assert_eq!(t.engine.log(&repo, 1).await.unwrap()[0].sha, local);
}

#[tokio::test]
async fn amend_refuses_when_another_branch_of_a_remote_contains_head() {
    let t = TestRepo::with_initial_commit();
    let repo = open(&t).await;
    let _origin = t.with_bare_origin();
    // Se publica en otra rama remota; la local `main` ni siquiera tiene upstream.
    t.git(&["push", "-q", "origin", "main:otra"]);
    t.git(&["fetch", "-q", "origin"]);
    let err = t
        .engine
        .amend_last_commit(&repo, "nuevo")
        .await
        .unwrap_err();
    match err {
        GitError::CannotAmend { reason } => assert!(reason.contains("origin/otra"), "{reason}"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn amend_refuses_detached_head() {
    let t = TestRepo::with_initial_commit();
    let repo = open(&t).await;
    let before = t.engine.log(&repo, 5).await.unwrap();
    t.git(&["checkout", "-q", "--detach"]);
    let err = t
        .engine
        .amend_last_commit(&repo, "corregido")
        .await
        .unwrap_err();
    match err {
        GitError::CannotAmend { reason } => assert!(reason.contains("rama"), "{reason}"),
        other => panic!("{other:?}"),
    }
    let shas = |v: &[ariadna_git::Commit]| v.iter().map(|c| c.sha.clone()).collect::<Vec<_>>();
    assert_eq!(shas(&t.engine.log(&repo, 5).await.unwrap()), shas(&before));
}

#[tokio::test]
async fn amend_refuses_during_merge_and_cherry_pick() {
    let t = TestRepo::with_initial_commit();
    let repo = open(&t).await;
    t.write("c.txt", b"base\n");
    t.commit_all("base");
    t.git(&["checkout", "-q", "-b", "otra"]);
    t.write("c.txt", b"otra\n");
    t.commit_all("otra");
    t.git(&["checkout", "-q", "main"]);
    t.write("c.txt", b"main\n");
    t.commit_all("main");
    let before = t.engine.log(&repo, 5).await.unwrap();

    let (code, _, _) = t.git_raw(&["merge", "otra"]);
    assert_ne!(code, 0, "tenía que haber conflicto");
    let err = t
        .engine
        .amend_last_commit(&repo, "corregido")
        .await
        .unwrap_err();
    match err {
        GitError::CannotAmend { reason } => assert!(reason.contains("merge"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(t.engine.log(&repo, 5).await.unwrap(), before);
    t.git(&["merge", "--abort"]);

    let (code, _, _) = t.git_raw(&["cherry-pick", "otra"]);
    assert_ne!(code, 0);
    let err = t
        .engine
        .amend_last_commit(&repo, "corregido")
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::CannotAmend { .. }), "{err:?}");
    assert_eq!(t.engine.log(&repo, 5).await.unwrap(), before);
}

// ---------------------------------------------------------------------------------------
// búsqueda
// ---------------------------------------------------------------------------------------

#[tokio::test]
async fn search_log_matches_message_author_and_hash() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    assert!(t
        .engine
        .search_log(&repo, "faro", 50)
        .await
        .unwrap()
        .is_empty());

    t.write("a.txt", b"1\n");
    t.git(&["add", "a.txt"]);
    t.git(&["commit", "-q", "-m", "Añade el faro"]);
    t.write("a.txt", b"2\n");
    t.git(&[
        "commit",
        "-q",
        "-am",
        "Corrige la MAREA",
        "-m",
        "detalle del cuerpo",
    ]);
    t.write("a.txt", b"3\n");
    t.git(&[
        "-c",
        "user.name=Otra Persona",
        "commit",
        "-q",
        "-am",
        "Ajusta el horario",
    ]);

    let subjects =
        |v: Vec<ariadna_git::Commit>| v.into_iter().map(|c| c.subject).collect::<Vec<_>>();
    assert_eq!(
        subjects(t.engine.search_log(&repo, "marea", 50).await.unwrap()),
        ["Corrige la MAREA"]
    );
    assert_eq!(
        subjects(t.engine.search_log(&repo, "DETALLE", 50).await.unwrap()),
        ["Corrige la MAREA"]
    );
    assert_eq!(
        subjects(
            t.engine
                .search_log(&repo, "otra persona", 50)
                .await
                .unwrap()
        ),
        ["Ajusta el horario"]
    );

    let all = t.engine.log(&repo, 10).await.unwrap();
    let target = all.last().unwrap();
    let by_hash = t
        .engine
        .search_log(&repo, &target.sha[..8], 50)
        .await
        .unwrap();
    assert_eq!(
        by_hash.iter().map(|c| c.sha.as_str()).collect::<Vec<_>>(),
        [target.sha.as_str()]
    );

    // Texto literal, no regex; y flags de git en la búsqueda no se interpretan.
    assert!(t
        .engine
        .search_log(&repo, "el.*", 50)
        .await
        .unwrap()
        .is_empty());
    assert!(t
        .engine
        .search_log(&repo, "--all", 50)
        .await
        .unwrap()
        .is_empty());
    assert!(t
        .engine
        .search_log(&repo, "-n1", 50)
        .await
        .unwrap()
        .is_empty());

    // El texto se recorta: «el» está en los tres (el tercero, en «detalle del cuerpo»).
    // De más nuevo a más viejo.
    let several = t.engine.search_log(&repo, " EL ", 50).await.unwrap();
    assert_eq!(
        subjects(several),
        ["Ajusta el horario", "Corrige la MAREA", "Añade el faro"]
    );

    // Vacío = log; el límite se respeta; sin duplicados aunque coincidan mensaje y autor.
    assert_eq!(
        t.engine.search_log(&repo, "   ", 50).await.unwrap().len(),
        3
    );
    assert_eq!(t.engine.search_log(&repo, "   ", 2).await.unwrap().len(), 2);
    // «test» está en el nombre del autor de dos commits y en el correo de los tres.
    assert_eq!(
        t.engine.search_log(&repo, "test", 50).await.unwrap().len(),
        3
    );
    assert_eq!(t.engine.search_log(&repo, "e", 1).await.unwrap().len(), 1);

    let err = t.engine.search_log(&repo, "a\nb", 5).await.unwrap_err();
    assert!(matches!(err, GitError::InvalidReference(_)), "{err:?}");
}

#[tokio::test]
async fn search_by_hash_ignores_commits_on_other_branches() {
    let t = TestRepo::with_initial_commit();
    let repo = open(&t).await;
    let main_sha = t.engine.log(&repo, 5).await.unwrap()[0].sha.clone();

    t.git(&["checkout", "-q", "-b", "otra"]);
    t.write("otra.txt", b"solo en otra\n");
    let other_sha = t.commit_all("commit de otra rama");
    t.git(&["checkout", "-q", "main"]);

    assert!(t
        .engine
        .search_log(&repo, &other_sha[..10], 50)
        .await
        .unwrap()
        .is_empty());
    let from_head = t
        .engine
        .search_log(&repo, &main_sha[..10], 50)
        .await
        .unwrap();
    assert_eq!(
        from_head.iter().map(|c| c.sha.clone()).collect::<Vec<_>>(),
        [main_sha]
    );

    t.git(&["checkout", "-q", "otra"]);
    let on_other = t
        .engine
        .search_log(&repo, &other_sha[..10], 50)
        .await
        .unwrap();
    assert_eq!(
        on_other.iter().map(|c| c.sha.clone()).collect::<Vec<_>>(),
        [other_sha]
    );
}

#[tokio::test]
async fn search_in_unborn_repo_is_empty() {
    let t = TestRepo::init();
    let repo = open(&t).await;
    assert!(t
        .engine
        .search_log(&repo, "abcd", 5)
        .await
        .unwrap()
        .is_empty());
}

// ---------------------------------------------------------------------------------------
// repos hostiles y hooks (Unix: los «programas» son scripts de shell)
// ---------------------------------------------------------------------------------------

#[cfg(unix)]
mod hostile {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn marker_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[tokio::test]
    async fn reads_never_run_fsmonitor_external_diff_or_textconv() {
        let t = TestRepo::with_initial_commit();
        let repo = open(&t).await;
        t.write(".gitattributes", b"*.dat diff=evil\n");
        t.write("f.txt", b"uno\n");
        t.write("f.dat", b"uno\n");
        t.commit_all("base");
        t.write("f.txt", b"dos\n");
        t.write("f.dat", b"dos\n");
        t.write("nuevo.txt", b"n\n");

        let m = marker_dir();
        let fsm = m.path().join("fsmonitor");
        let ext = m.path().join("external");
        let tc = m.path().join("textconv");
        // El `#` convierte en comentario los argumentos que git añade al comando.
        t.git(&[
            "config",
            "core.fsmonitor",
            &format!("touch {} #", fsm.display()),
        ]);
        t.git(&[
            "config",
            "diff.external",
            &format!("touch {} #", ext.display()),
        ]);
        t.git(&[
            "config",
            "diff.evil.textconv",
            &format!("touch {} #", tc.display()),
        ]);

        // Control: sin las defensas del motor, git de verdad sí los ejecuta.
        t.git(&["status"]);
        t.git(&["diff"]);
        assert!(fsm.exists(), "el control no ejecutó fsmonitor");
        assert!(ext.exists(), "el control no ejecutó diff.external");
        std::fs::remove_file(&fsm).unwrap();
        std::fs::remove_file(&ext).unwrap();
        // Con diff.external activo git no llega a textconv: se comprueba aparte.
        t.git(&["diff", "--no-ext-diff", "--textconv"]);
        assert!(tc.exists(), "el control no ejecutó textconv");
        std::fs::remove_file(&tc).unwrap();
        // Este último control también despertó a fsmonitor.
        let _ = std::fs::remove_file(&fsm);

        let eng = &t.engine;
        eng.status(&repo, false).await.unwrap();
        eng.status(&repo, true).await.unwrap();
        let all = eng.diff(&repo, &DiffScope::Unstaged, &[]).await.unwrap();
        assert!(all.iter().any(|f| f.path == "f.dat"));
        eng.diff(
            &repo,
            &DiffScope::Unstaged,
            &paths(&["f.txt", "f.dat", "nuevo.txt"]),
        )
        .await
        .unwrap();
        eng.diff(&repo, &DiffScope::Staged, &[]).await.unwrap();
        eng.diff(&repo, &DiffScope::Commit("HEAD".into()), &[])
            .await
            .unwrap();
        eng.diff(
            &repo,
            &DiffScope::Between {
                base: "HEAD~1".into(),
                head: "HEAD".into(),
            },
            &[],
        )
        .await
        .unwrap();
        eng.log(&repo, 10).await.unwrap();
        eng.search_log(&repo, "base", 10).await.unwrap();
        for (name, p) in [
            ("fsmonitor", &fsm),
            ("diff.external", &ext),
            ("textconv", &tc),
        ] {
            assert!(!p.exists(), "el motor ejecutó {name} en una lectura");
        }
    }

    #[tokio::test]
    async fn hostile_pager_editor_and_credential_config_are_harmless() {
        let t = TestRepo::with_initial_commit();
        let repo = open(&t).await;
        let m = marker_dir();
        let pager = m.path().join("pager");
        t.git(&[
            "config",
            "core.pager",
            &format!("touch {} #", pager.display()),
        ]);
        t.git(&[
            "config",
            "pager.log",
            &format!("touch {} #", pager.display()),
        ]);
        t.git(&[
            "config",
            "pager.diff",
            &format!("touch {} #", pager.display()),
        ]);
        t.git(&["config", "log.showSignature", "true"]);
        t.write("a.txt", b"x\n");
        t.engine
            .diff(&repo, &DiffScope::Unstaged, &paths(&["a.txt"]))
            .await
            .unwrap();
        t.engine.log(&repo, 5).await.unwrap();
        assert!(!pager.exists());
    }

    #[tokio::test]
    async fn commit_runs_hooks_and_reports_a_failing_one() {
        let t = TestRepo::init();
        let repo = open(&t).await;
        let m = marker_dir();
        let post = m.path().join("post-commit-ran");
        let hooks = t.path().join(".git/hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let write_hook = |name: &str, body: &str| {
            let p = hooks.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        write_hook("pre-commit", "echo 'rechazado por el hook' >&2; exit 1");
        t.write("a.txt", b"a\n");
        t.engine.stage(&repo, &paths(&["a.txt"])).await.unwrap();
        let err = t.engine.commit(&repo, "no pasa", false).await.unwrap_err();
        match err {
            GitError::CommandFailed { stderr, .. } => {
                assert!(stderr.contains("rechazado por el hook"), "{stderr}");
            }
            other => panic!("{other:?}"),
        }
        assert!(t.engine.log(&repo, 5).await.unwrap().is_empty());

        write_hook("pre-commit", "exit 0");
        write_hook("post-commit", &format!("touch {}", post.display()));
        t.engine.commit(&repo, "pasa", false).await.unwrap();
        assert!(post.exists(), "el post-commit debía ejecutarse");
        assert_eq!(t.engine.log(&repo, 5).await.unwrap().len(), 1);
    }
}
