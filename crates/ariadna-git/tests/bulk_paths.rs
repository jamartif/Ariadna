//! `stage`/`unstage` con más de 2 000 rutas usan órdenes de índice lineales
//! (`update-index`) en vez de pathspecs (cuadráticos). Deben dar el mismo resultado que
//! la vía normal, también con nombres raros, borrados y archivos que HEAD no tiene.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use ariadna_git::{GitCliEngine, GitPath, GitRunner};
use common::TestRepo;

fn name(i: usize) -> String {
    // Windows no admite `*`, `?`, `"`, `<`, `>` ni `|` en un nombre de archivo.
    const RAROS: &[char] = &['*', '?'];
    let gl = |c: char| {
        if cfg!(windows) && RAROS.contains(&c) {
            'x'
        } else {
            c
        }
    };
    match i % 5 {
        0 => format!("d{}/archivo con espacio {i}.txt", i % 7),
        1 => format!("-guion {i}.txt"),
        2 => format!("ñandú/[glob]{}{}{i}.txt", gl('*'), gl('?')),
        3 => format!("a/b/c{i}.rs"),
        _ => format!("raiz{i}.md"),
    }
}

#[tokio::test]
async fn bulk_stage_and_unstage_match_the_pathspec_route() {
    // 2 600 rutas el runner de Windows de CI tarda más de los 180 s del producto, y un test
    // lento no debe confundirse con un cuelgue: aquí se prueba la corrección, no el plazo.
    let n: usize = if cfg!(windows) { 2_200 } else { 2_600 };
    let mut t = TestRepo::with_initial_commit();
    t.engine = GitCliEngine::new(
        GitRunner::with_path(common::git_path()).with_local_timeout(Duration::from_secs(900)),
    );
    let repo = t.engine.open_repository(t.path()).await.unwrap();
    let all: Vec<GitPath> = (0..n).map(|i| GitPath::from(name(i))).collect();
    for (i, p) in all.iter().enumerate() {
        t.write(&p.to_string_lossy(), format!("v1 {i}\n").as_bytes());
    }

    // Archivos nuevos: ninguno está en HEAD.
    t.engine.stage(&repo, &all).await.unwrap();
    let st = t.engine.status(&repo, false).await.unwrap();
    assert_eq!(st.staged().len(), n);
    assert!(st.unstaged().is_empty());
    t.engine.commit(&repo, "base", false).await.unwrap();

    // Modificados, borrados y uno nuevo.
    for (i, p) in all.iter().enumerate() {
        let rel = p.to_string_lossy().into_owned();
        if i % 10 == 0 {
            std::fs::remove_file(t.path().join(&rel)).unwrap();
        } else {
            t.write(&rel, format!("v2 {i}\n").as_bytes());
        }
    }
    let mut touched = all.clone();
    t.write("recien nuevo.txt", b"nuevo\n");
    touched.push(GitPath::from("recien nuevo.txt"));

    t.engine.stage(&repo, &touched).await.unwrap();
    let st = t.engine.status(&repo, false).await.unwrap();
    assert_eq!(st.staged().len(), n + 1);
    assert!(st.unstaged().is_empty(), "queda algo sin preparar");
    let deleted = st
        .staged()
        .iter()
        .filter(|e| e.path != "recien nuevo.txt")
        .count();
    assert_eq!(deleted, n);
    assert_eq!(t.git(&["ls-files", "--deleted"]).trim(), "");
    assert_eq!(
        t.git(&["diff", "--cached", "--name-status", "--diff-filter=D"])
            .lines()
            .count(),
        n / 10
    );

    // Quitar todo del índice: HEAD manda; el archivo nuevo desaparece del índice.
    t.engine.unstage(&repo, &touched).await.unwrap();
    let st = t.engine.status(&repo, false).await.unwrap();
    assert!(st.staged().is_empty(), "sigue algo preparado");
    assert_eq!(st.unstaged().len(), n + 1); // n modificados/borrados + el nuevo sin seguimiento
    assert!(st
        .entries
        .iter()
        .any(|e| e.path == "recien nuevo.txt" && e.is_untracked));
    // El contenido del árbol de trabajo no se ha tocado.
    assert_eq!(t.read("recien nuevo.txt"), b"nuevo\n");
    assert_eq!(t.read(&name(1)), b"v2 1\n");
}

#[tokio::test]
async fn bulk_paths_are_still_validated() {
    let t = TestRepo::with_initial_commit();
    let repo = t.engine.open_repository(t.path()).await.unwrap();
    let mut list: Vec<GitPath> = (0..2_100).map(|i| GitPath::from(format!("f{i}"))).collect();
    list.push(GitPath::from("../fuera"));
    assert!(t.engine.stage(&repo, &list).await.is_err());
    assert!(t.engine.unstage(&repo, &list).await.is_err());
}
