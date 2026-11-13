//! Preparar y quitar de preparados **líneas sueltas** de un hunk, contra git de verdad.
#![allow(clippy::unwrap_used)]

mod common;

use ariadna_git::{DiffHunk, DiffLineKind, DiffScope, FileDiff, GitError, GitPath, Repository};
use common::TestRepo;

fn gp(s: &str) -> GitPath {
    GitPath::from(s)
}

async fn open(t: &TestRepo) -> Repository {
    t.engine.open_repository(t.path()).await.unwrap()
}

async fn diff_of(t: &TestRepo, repo: &Repository, scope: DiffScope, path: &str) -> Vec<FileDiff> {
    t.engine.diff(repo, &scope, &[gp(path)]).await.unwrap()
}

/// Índices de las líneas del hunk cuyo texto (sin prefijo) está en `texts` y cambian.
fn pick(hunk: &DiffHunk, kind: DiffLineKind, texts: &[&str]) -> Vec<u32> {
    hunk.lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind == kind && texts.contains(&l.text.as_str()))
        .map(|(i, _)| u32::try_from(i).unwrap())
        .collect()
}

fn numbered(n: usize) -> String {
    (1..=n).map(|i| format!("l{i}\n")).collect()
}

/// Repo con `f.txt` de 12 líneas confirmado y con tres cambios sueltos (`l4`, `l6`, `l8`) que
/// quedan en un solo hunk, cada uno como par `-l`/`+L` intercalado con líneas sin cambiar.
async fn three_changes() -> (TestRepo, Repository) {
    let t = TestRepo::init();
    t.write("f.txt", numbered(12).as_bytes());
    t.commit_all("base");
    t.write("f.txt", changed_all().as_bytes());
    let repo = open(&t).await;
    (t, repo)
}

fn changed_all() -> String {
    numbered(12)
        .replace("l4\n", "L4\n")
        .replace("l6\n", "L6\n")
        .replace("l8\n", "L8\n")
}

#[tokio::test]
async fn stage_only_one_replaced_line_of_several() {
    let (t, repo) = three_changes().await;
    let p = gp("f.txt");
    let files = diff_of(&t, &repo, DiffScope::Unstaged, "f.txt").await;
    assert_eq!(files[0].hunks.len(), 1);
    let hunk = &files[0].hunks[0];
    let mut sel = pick(hunk, DiffLineKind::Deletion, &["l6"]);
    sel.extend(pick(hunk, DiffLineKind::Addition, &["L6"]));
    t.engine.stage_lines(&repo, hunk, &sel, &p).await.unwrap();

    // El índice solo tiene el cambio de la línea 6.
    let staged = t.git(&["show", ":f.txt"]);
    assert_eq!(staged, numbered(12).replace("l6\n", "L6\n"));
    // El árbol de trabajo no se toca y el resto sigue pendiente.
    assert_eq!(t.read("f.txt"), changed_all().as_bytes());
    let rest = diff_of(&t, &repo, DiffScope::Unstaged, "f.txt").await;
    let texts: Vec<_> = rest[0]
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| l.kind != DiffLineKind::Context)
        .map(|l| l.text.clone())
        .collect();
    assert!(texts.contains(&"L4".to_owned()) && texts.contains(&"L8".to_owned()));
    assert!(!texts.contains(&"L6".to_owned()));
}

#[tokio::test]
async fn stage_only_an_addition_keeps_the_old_line_in_the_index() {
    let (t, repo) = three_changes().await;
    let files = diff_of(&t, &repo, DiffScope::Unstaged, "f.txt").await;
    let hunk = &files[0].hunks[0];
    let sel = pick(hunk, DiffLineKind::Addition, &["L4"]);
    t.engine
        .stage_lines(&repo, hunk, &sel, &gp("f.txt"))
        .await
        .unwrap();
    // `l4` sigue (la borrada sin elegir pasa a contexto) y `L4` se añade tras ella.
    let staged = t.git(&["show", ":f.txt"]);
    assert_eq!(staged, numbered(12).replace("l4\n", "l4\nL4\n"));
}

#[tokio::test]
async fn stage_only_a_deletion_removes_just_that_line() {
    let (t, repo) = three_changes().await;
    let files = diff_of(&t, &repo, DiffScope::Unstaged, "f.txt").await;
    let hunk = &files[0].hunks[0];
    let sel = pick(hunk, DiffLineKind::Deletion, &["l6"]);
    t.engine
        .stage_lines(&repo, hunk, &sel, &gp("f.txt"))
        .await
        .unwrap();
    assert_eq!(t.git(&["show", ":f.txt"]), numbered(12).replace("l6\n", ""));
}

#[tokio::test]
async fn unstage_part_of_a_staged_hunk() {
    let (t, repo) = three_changes().await;
    let p = gp("f.txt");
    t.engine
        .stage(&repo, std::slice::from_ref(&p))
        .await
        .unwrap();
    let files = diff_of(&t, &repo, DiffScope::Staged, "f.txt").await;
    let hunk = &files[0].hunks[0];
    // Quita del índice solo el cambio de `l8`.
    let mut sel = pick(hunk, DiffLineKind::Deletion, &["l8"]);
    sel.extend(pick(hunk, DiffLineKind::Addition, &["L8"]));
    t.engine.unstage_lines(&repo, hunk, &sel, &p).await.unwrap();
    let staged = t.git(&["show", ":f.txt"]);
    assert_eq!(
        staged,
        numbered(12).replace("l4\n", "L4\n").replace("l6\n", "L6\n")
    );
    // Quitar solo la parte «añadida» de otro cambio deja la borrada aplicada.
    let files = diff_of(&t, &repo, DiffScope::Staged, "f.txt").await;
    let hunk = &files[0].hunks[0];
    let sel = pick(hunk, DiffLineKind::Addition, &["L4"]);
    t.engine.unstage_lines(&repo, hunk, &sel, &p).await.unwrap();
    let staged = t.git(&["show", ":f.txt"]);
    assert_eq!(
        staged,
        numbered(12).replace("l4\n", "").replace("l6\n", "L6\n")
    );
}

#[tokio::test]
async fn stage_then_unstage_all_selected_lines_round_trips() {
    let (t, repo) = three_changes().await;
    let p = gp("f.txt");
    let head = t.git(&["show", "HEAD:f.txt"]);
    let files = diff_of(&t, &repo, DiffScope::Unstaged, "f.txt").await;
    let hunk = &files[0].hunks[0];
    let all: Vec<u32> = hunk
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| matches!(l.kind, DiffLineKind::Addition | DiffLineKind::Deletion))
        .map(|(i, _)| u32::try_from(i).unwrap())
        .collect();
    t.engine.stage_lines(&repo, hunk, &all, &p).await.unwrap();
    assert_ne!(t.git(&["show", ":f.txt"]), head);
    let staged = diff_of(&t, &repo, DiffScope::Staged, "f.txt").await;
    let hunk = &staged[0].hunks[0];
    t.engine.unstage_lines(&repo, hunk, &all, &p).await.unwrap();
    assert_eq!(t.git(&["show", ":f.txt"]), head);
}

#[tokio::test]
async fn stage_lines_of_a_new_untracked_file_then_unstage_some() {
    let t = TestRepo::with_initial_commit();
    t.write("nuevo.txt", b"uno\ndos\ntres\ncuatro\n");
    let repo = open(&t).await;
    let p = gp("nuevo.txt");
    let files = diff_of(&t, &repo, DiffScope::Unstaged, "nuevo.txt").await;
    assert!(files[0].is_new_file);
    let hunk = &files[0].hunks[0];
    let sel = pick(hunk, DiffLineKind::Addition, &["dos", "cuatro"]);
    t.engine.stage_lines(&repo, hunk, &sel, &p).await.unwrap();
    assert_eq!(t.git(&["show", ":nuevo.txt"]), "dos\ncuatro\n");
    // Ahora el archivo existe en el índice: lo que falta se ve como modificación.
    let rest = diff_of(&t, &repo, DiffScope::Unstaged, "nuevo.txt").await;
    assert!(!rest[0].is_new_file);

    // Quitar de preparados una de las dos.
    let staged = diff_of(&t, &repo, DiffScope::Staged, "nuevo.txt").await;
    assert!(staged[0].is_new_file);
    let hunk = &staged[0].hunks[0];
    let sel = pick(hunk, DiffLineKind::Addition, &["dos"]);
    t.engine.unstage_lines(&repo, hunk, &sel, &p).await.unwrap();
    assert_eq!(t.git(&["show", ":nuevo.txt"]), "cuatro\n");
}

#[tokio::test]
async fn stage_lines_of_an_intent_to_add_file() {
    let t = TestRepo::with_initial_commit();
    t.write("ita.txt", b"a\nb\nc\n");
    t.git(&["add", "-N", "ita.txt"]);
    let repo = open(&t).await;
    let files = diff_of(&t, &repo, DiffScope::Unstaged, "ita.txt").await;
    let hunk = &files[0].hunks[0];
    let sel = pick(hunk, DiffLineKind::Addition, &["b"]);
    t.engine
        .stage_lines(&repo, hunk, &sel, &gp("ita.txt"))
        .await
        .unwrap();
    assert_eq!(t.git(&["show", ":ita.txt"]), "b\n");
}

#[tokio::test]
async fn crlf_lines_keep_their_carriage_returns() {
    let t = TestRepo::init();
    let base: Vec<u8> = (1..=10)
        .flat_map(|i| format!("l{i}\r\n").into_bytes())
        .collect();
    t.write("crlf.txt", &base);
    t.commit_all("base");
    let changed = String::from_utf8(base.clone())
        .unwrap()
        .replace("l4\r\n", "L4\r\n")
        .replace("l5\r\n", "L5\r\n");
    t.write("crlf.txt", changed.as_bytes());
    let repo = open(&t).await;
    let files = diff_of(&t, &repo, DiffScope::Unstaged, "crlf.txt").await;
    let hunk = &files[0].hunks[0];
    let mut sel = pick(hunk, DiffLineKind::Deletion, &["l5\r"]);
    sel.extend(pick(hunk, DiffLineKind::Addition, &["L5\r"]));
    assert_eq!(sel.len(), 2);
    t.engine
        .stage_lines(&repo, hunk, &sel, &gp("crlf.txt"))
        .await
        .unwrap();
    let staged = t.git(&["show", ":crlf.txt"]);
    assert_eq!(
        staged,
        String::from_utf8(base).unwrap().replace("l5\r\n", "L5\r\n")
    );
}

#[tokio::test]
async fn missing_final_newline_is_preserved() {
    let t = TestRepo::init();
    t.write("sin.txt", b"a\nb\nc\nd");
    t.commit_all("base");
    // Cambia `b` y la última línea (que no termina en salto).
    t.write("sin.txt", b"a\nB\nc\nD");
    let repo = open(&t).await;
    let p = gp("sin.txt");
    let files = diff_of(&t, &repo, DiffScope::Unstaged, "sin.txt").await;
    let hunk = &files[0].hunks[0];
    let mut sel = pick(hunk, DiffLineKind::Deletion, &["b"]);
    sel.extend(pick(hunk, DiffLineKind::Addition, &["B"]));
    t.engine.stage_lines(&repo, hunk, &sel, &p).await.unwrap();
    assert_eq!(t.git(&["show", ":sin.txt"]), "a\nB\nc\nd");

    // Ahora la última línea: la marca «sin salto final» viaja con sus líneas.
    let files = diff_of(&t, &repo, DiffScope::Unstaged, "sin.txt").await;
    let hunk = &files[0].hunks[0];
    let mut sel = pick(hunk, DiffLineKind::Deletion, &["d"]);
    sel.extend(pick(hunk, DiffLineKind::Addition, &["D"]));
    t.engine.stage_lines(&repo, hunk, &sel, &p).await.unwrap();
    assert_eq!(t.git(&["show", ":sin.txt"]), "a\nB\nc\nD");
    assert!(diff_of(&t, &repo, DiffScope::Unstaged, "sin.txt")
        .await
        .is_empty());
}

#[tokio::test]
async fn paths_with_spaces_and_unicode() {
    for name in [
        "notas/cañón archivo.txt",
        "日本語 dir/ファイル.txt",
        "-guion.txt",
    ] {
        let t = TestRepo::init();
        t.write(name, numbered(10).as_bytes());
        t.commit_all("base");
        t.write(
            name,
            numbered(10)
                .replace("l3\n", "L3\n")
                .replace("l4\n", "L4\n")
                .as_bytes(),
        );
        let repo = open(&t).await;
        let files = diff_of(&t, &repo, DiffScope::Unstaged, name).await;
        let hunk = &files[0].hunks[0];
        let mut sel = pick(hunk, DiffLineKind::Deletion, &["l4"]);
        sel.extend(pick(hunk, DiffLineKind::Addition, &["L4"]));
        t.engine
            .stage_lines(&repo, hunk, &sel, &gp(name))
            .await
            .unwrap();
        let spec = format!(":{name}");
        assert_eq!(
            t.git(&["show", &spec]),
            numbered(10).replace("l4\n", "L4\n"),
            "{name}"
        );
        // Y se puede deshacer.
        let staged = diff_of(&t, &repo, DiffScope::Staged, name).await;
        let hunk = &staged[0].hunks[0];
        let sel = pick(hunk, DiffLineKind::Addition, &["L4"]);
        t.engine
            .unstage_lines(&repo, hunk, &sel, &gp(name))
            .await
            .unwrap();
        assert_eq!(
            t.git(&["show", &spec]),
            numbered(10).replace("l4\n", ""),
            "{name}"
        );
    }
}

#[tokio::test]
async fn empty_selection_and_stale_hunks_are_rejected_with_clear_errors() {
    let (t, repo) = three_changes().await;
    let p = gp("f.txt");
    let files = diff_of(&t, &repo, DiffScope::Unstaged, "f.txt").await;
    let hunk = files[0].hunks[0].clone();
    let before = t.git(&["show", ":f.txt"]);

    let e = t
        .engine
        .stage_lines(&repo, &hunk, &[], &p)
        .await
        .unwrap_err();
    assert!(matches!(e, GitError::PatchRejected { .. }), "{e:?}");
    // Solo contexto: tampoco hay nada que preparar.
    let e = t
        .engine
        .stage_lines(&repo, &hunk, &[0], &p)
        .await
        .unwrap_err();
    assert!(matches!(e, GitError::PatchRejected { .. }), "{e:?}");
    // Línea fuera del hunk.
    let e = t
        .engine
        .stage_lines(&repo, &hunk, &[999], &p)
        .await
        .unwrap_err();
    assert!(matches!(e, GitError::PatchRejected { .. }), "{e:?}");

    // La UI tenía otro texto en esa línea: el diff cambió debajo.
    let mut stale = hunk.clone();
    let i = stale
        .lines
        .iter()
        .position(|l| l.kind == DiffLineKind::Addition)
        .unwrap();
    stale.lines[i].text = "otra cosa".into();
    let e = t
        .engine
        .stage_lines(&repo, &stale, &[u32::try_from(i).unwrap()], &p)
        .await
        .unwrap_err();
    assert!(matches!(e, GitError::HunkNotFound { .. }), "{e:?}");

    // Rangos que ya no existen.
    let mut gone = hunk.clone();
    gone.old_start = 77;
    let e = t
        .engine
        .stage_lines(&repo, &gone, &[1], &p)
        .await
        .unwrap_err();
    assert!(matches!(e, GitError::HunkNotFound { .. }), "{e:?}");

    // Nada se tocó.
    assert_eq!(t.git(&["show", ":f.txt"]), before);
}

#[tokio::test]
async fn unstage_lines_of_a_deleted_file_is_refused_but_staging_them_works() {
    let t = TestRepo::init();
    t.write("d.txt", b"a\nb\nc\n");
    t.commit_all("base");
    std::fs::remove_file(t.path().join("d.txt")).unwrap();
    let repo = open(&t).await;
    let p = gp("d.txt");
    // Preparar solo la línea `b` de un archivo borrado en el árbol de trabajo.
    let files = diff_of(&t, &repo, DiffScope::Unstaged, "d.txt").await;
    let hunk = &files[0].hunks[0];
    let sel = pick(hunk, DiffLineKind::Deletion, &["b"]);
    t.engine.stage_lines(&repo, hunk, &sel, &p).await.unwrap();
    assert_eq!(t.git(&["show", ":d.txt"]), "a\nc\n");
    // Con el borrado completo ya en el índice, quitar solo una línea no es posible.
    t.engine
        .stage(&repo, std::slice::from_ref(&p))
        .await
        .unwrap();
    let staged = diff_of(&t, &repo, DiffScope::Staged, "d.txt").await;
    let hunk = &staged[0].hunks[0];
    let sel = pick(hunk, DiffLineKind::Deletion, &["a"]);
    let e = t
        .engine
        .unstage_lines(&repo, hunk, &sel, &p)
        .await
        .unwrap_err();
    assert!(matches!(e, GitError::PatchRejected { .. }), "{e:?}");
}

#[tokio::test]
async fn second_hunk_lines_do_not_disturb_the_first() {
    let t = TestRepo::init();
    t.write("g.txt", numbered(40).as_bytes());
    t.commit_all("base");
    let changed = numbered(40)
        .replace("l2\n", "L2\n")
        .replace("l30\n", "L30\n")
        .replace("l31\n", "L31\n");
    t.write("g.txt", changed.as_bytes());
    let repo = open(&t).await;
    let files = diff_of(&t, &repo, DiffScope::Unstaged, "g.txt").await;
    assert_eq!(files[0].hunks.len(), 2);
    let hunk = &files[0].hunks[1];
    let mut sel = pick(hunk, DiffLineKind::Deletion, &["l31"]);
    sel.extend(pick(hunk, DiffLineKind::Addition, &["L31"]));
    t.engine
        .stage_lines(&repo, hunk, &sel, &gp("g.txt"))
        .await
        .unwrap();
    assert_eq!(
        t.git(&["show", ":g.txt"]),
        numbered(40).replace("l31\n", "L31\n")
    );
}
