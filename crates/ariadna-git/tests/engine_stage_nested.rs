//! «Preparar todo» con un repositorio anidado sin seguimiento: git lo lista como `nested/` (barra
//! final) y esa ruta hacía fallar la validación de todo el lote.
#![allow(clippy::unwrap_used)]

mod common;

use common::{plain_engine, TestRepo};

#[tokio::test]
async fn preparar_todo_con_un_repo_anidado_no_falla() {
    let t = TestRepo::with_initial_commit();
    t.write("suelto.txt", b"1\n");
    t.write("nested/f.txt", b"2\n");
    t.git(&["-C", "nested", "init", "-q"]);
    t.git(&["-C", "nested", "add", "f.txt"]);
    t.git(&[
        "-C",
        "nested",
        "-c",
        "user.email=a@b",
        "-c",
        "user.name=a",
        "commit",
        "-qm",
        "i",
    ]);

    let engine = plain_engine();
    let repo = engine.open_repository(t.path()).await.unwrap();
    let st = engine.status(&repo, false).await.unwrap();
    let paths: Vec<_> = st
        .entries
        .iter()
        .filter(|e| e.is_untracked)
        .map(|e| e.path.clone())
        .collect();
    assert!(
        paths.iter().any(|p| p.as_bytes().ends_with(b"/")),
        "git lista el repo anidado con barra final: {paths:?}"
    );

    engine.stage(&repo, &paths).await.unwrap();
    let out = t.git(&["status", "--short"]);
    assert!(out.contains("A  suelto.txt"), "{out}");
    assert!(out.contains("A  nested"), "{out}");
}
