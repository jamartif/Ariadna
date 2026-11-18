//! `merge_tree` y `unmerged_paths`: leer el merge que git calcularía **sin tocar nada**.
//!
//! `git merge-tree --write-tree` sale con **1 cuando hay conflictos**; tomarlo por «sin
//! conflictos» dejaba la pantalla de conflictos de una PR siempre vacía (el bug que cubre esto).

#![allow(clippy::unwrap_used)]

mod common;

use common::{plain_engine, TestRepo};

/// Dos ramas que tocan la misma línea del mismo fichero.
fn repo_con_conflicto() -> (TestRepo, String, String) {
    let t = TestRepo::with_initial_commit();
    t.write("compartido.txt", b"linea original\n");
    t.git(&["add", "-A"]);
    t.git(&["commit", "-q", "-m", "base"]);
    let base = t.git(&["rev-parse", "HEAD"]).trim().to_owned();

    // Lado de la rama base (lo que ya está publicado).
    t.write("compartido.txt", b"version de main\n");
    let main = t.commit_all("main cambia la línea");

    // Lado de la PR, desde el mismo punto.
    t.git(&["checkout", "-q", "-b", "lateral", &base]);
    t.write("compartido.txt", b"version de la PR\n");
    let lateral = t.commit_all("la PR cambia la línea");
    t.git(&["checkout", "-q", "main"]);

    (t, main, lateral)
}

#[tokio::test]
async fn un_merge_con_conflictos_no_es_un_error_y_devuelve_las_rutas() {
    let (t, main, lateral) = repo_con_conflicto();
    let engine = plain_engine();
    let repo = engine.open_repository(t.path()).await.unwrap();

    let scan = engine.merge_tree(&repo, &main, &lateral).await.unwrap();
    assert!(
        scan.tree.is_some(),
        "merge-tree --write-tree escribe un árbol aunque haya conflictos"
    );
    assert_eq!(
        scan.conflicts,
        vec!["compartido.txt".to_owned()],
        "la ruta en conflicto sale por el código de salida 1, no por el texto de git"
    );

    // No se ha tocado ni el índice ni el árbol de trabajo.
    assert!(
        t.git(&["status", "--porcelain"]).trim().is_empty(),
        "merge-tree no debe dejar cambios en el árbol de trabajo"
    );
}

#[tokio::test]
async fn un_merge_limpio_no_devuelve_rutas() {
    let t = TestRepo::with_initial_commit();
    t.git(&["checkout", "-q", "-b", "otra"]);
    t.write("otra.txt", b"otra\n");
    t.commit_all("otro fichero distinto");
    let otra = t.git(&["rev-parse", "HEAD"]).trim().to_owned();
    t.git(&["checkout", "-q", "main"]);
    t.write("main.txt", b"main\n");
    let main = t.commit_all("fichero distinto en main");

    let engine = plain_engine();
    let repo = engine.open_repository(t.path()).await.unwrap();
    let scan = engine.merge_tree(&repo, &main, &otra).await.unwrap();
    assert!(scan.tree.is_some());
    assert!(scan.conflicts.is_empty(), "{:?}", scan.conflicts);
}

#[tokio::test]
async fn las_rutas_sin_resolver_salen_del_indice() {
    let (t, main, lateral) = repo_con_conflicto();
    let engine = plain_engine();
    let repo = engine.open_repository(t.path()).await.unwrap();

    assert!(
        engine.unmerged_paths(&repo).await.unwrap().is_empty(),
        "antes del merge no hay nada sin resolver"
    );

    // Un merge de verdad deja el conflicto en el índice.
    let merged = engine
        .merge(&repo, &lateral, ariadna_git::MergeFastForward::Never)
        .await
        .unwrap();
    assert!(
        matches!(merged, ariadna_git::MergeResult::Conflicts { .. }),
        "{merged:?}"
    );
    assert_eq!(
        engine.unmerged_paths(&repo).await.unwrap(),
        vec!["compartido.txt".to_owned()]
    );
    let _ = main;
}
