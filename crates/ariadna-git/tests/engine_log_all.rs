//! Historial del grafo completo: `log_page_all` recorre todas las ramas y etiquetas, no solo HEAD.
#![allow(clippy::unwrap_used)]

mod common;

use common::{os, plain_engine, TestRepo};

/// Con dos ramas divergentes y una etiqueta, el historial de HEAD no ve la otra rama; el del
/// grafo completo (lo que pinta la vista por defecto) sí.
#[tokio::test]
async fn el_historial_completo_incluye_otras_ramas_y_etiquetas() {
    let t = TestRepo::with_initial_commit();
    let base = t.git(&["rev-parse", "HEAD"]).trim().to_owned();
    t.write("main.txt", b"main\n");
    let main_commit = t.commit_all("trabajo en main");
    t.git(&["branch", "lateral"]);

    // Un commit en la rama lateral que main no tiene.
    t.git(&["checkout", "-q", "lateral"]);
    t.write("lateral.txt", b"lateral\n");
    let lateral_commit = t.commit_all("trabajo en lateral");
    t.git(&["checkout", "-q", "main"]);

    // Una etiqueta anotada sobre el commit inicial.
    t.git(&["tag", "-a", "v1", "-m", "primera", &base]);

    let engine = plain_engine();
    let repo = engine.open_repository(t.path()).await.unwrap();

    let head = engine.log_page(&repo, 0, 50).await.unwrap();
    let shas: Vec<&str> = head.iter().map(|c| c.sha.as_str()).collect();
    assert!(shas.contains(&main_commit.as_str()), "main está en HEAD");
    assert!(
        !shas.contains(&lateral_commit.as_str()),
        "la rama lateral no se ve desde HEAD: {shas:?}"
    );

    let all = engine.log_page_all(&repo, 0, 50).await.unwrap();
    let all_shas: Vec<&str> = all.iter().map(|c| c.sha.as_str()).collect();
    assert!(all_shas.contains(&main_commit.as_str()), "main en el grafo");
    assert!(
        all_shas.contains(&lateral_commit.as_str()),
        "la rama lateral aparece en el grafo completo: {all_shas:?}"
    );
    assert!(
        all_shas.contains(&base.as_str()),
        "el commit etiquetado aparece"
    );
    // El orden topológico nunca pone a un padre antes que a su hijo.
    let pos = |sha: &str| all_shas.iter().position(|s| *s == sha);
    assert!(
        pos(&lateral_commit) < pos(&base),
        "el padre va después del hijo"
    );

    // La búsqueda también ve la otra rama cuando el ámbito es todo el repositorio.
    let found = engine.search_log_all(&repo, "lateral", 10).await.unwrap();
    assert!(
        found.iter().any(|c| c.sha == lateral_commit),
        "search_all_log encuentra el commit de la otra rama"
    );
    // Y no lo encuentra buscando solo en HEAD.
    let only_head = engine.search_log(&repo, "lateral", 10).await.unwrap();
    assert!(only_head.iter().all(|c| c.sha != lateral_commit));

    // Comprobación directa de los argumentos que usa el motor para el grafo completo.
    let out = t.git(&["log", "--all", "--topo-order", "--format=%H"]);
    assert!(out.contains(&lateral_commit));
    let _ = os(&["log"]);
}
