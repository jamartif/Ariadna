//! Historial del grafo completo: `log_page_all` recorre todas las ramas y etiquetas, no solo HEAD.
#![allow(clippy::unwrap_used)]

mod common;

use common::{os, plain_engine, TestRepo};

#[tokio::test]
async fn el_grafo_completo_ve_otra_rama_cuando_head_va_por_detras() {
    // El caso real: HEAD está en una rama que se quedó atrás y el merge más reciente vive en
    // otra rama. `git log --oneline HEAD` no lo muestra; el grafo completo sí.
    let t = TestRepo::with_initial_commit();
    let base = t.git(&["rev-parse", "HEAD"]).trim().to_owned();
    t.write("vieja.txt", b"vieja\n");
    t.git(&["add", "-A"]);
    t.git(&["commit", "-q", "-m", "trabajo en vieja"]);
    t.git(&["branch", "-M", "vieja"]);

    // `main` avanza y fusiona `vieja`: su punta no es ancestro de HEAD.
    t.git(&["checkout", "-q", "-b", "main", &base]);
    t.write("main.txt", b"main\n");
    t.git(&["add", "-A"]);
    t.git(&["commit", "-q", "-m", "trabajo en main"]);
    t.git(&["merge", "-q", "--no-ff", "vieja", "-m", "merge de vieja"]);
    let merge = t.git(&["rev-parse", "HEAD"]).trim().to_owned();

    // HEAD vuelve a la rama vieja, que va por detrás.
    t.git(&["checkout", "-q", "vieja"]);

    let engine = plain_engine();
    let repo = engine.open_repository(t.path()).await.unwrap();
    let head = engine.log_page(&repo, 0, 50).await.unwrap();
    assert!(
        !head.iter().any(|c| c.sha == merge),
        "el merge de main no está en el historial de HEAD"
    );
    let todo = engine.log_page_all(&repo, 0, 50).await.unwrap();
    assert!(
        todo.iter().any(|c| c.sha == merge),
        "el grafo completo tiene que ver el merge de la otra rama: {:?}",
        todo.iter().map(|c| c.sha.clone()).collect::<Vec<_>>()
    );
}

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

/// Caso medido en el repo real: ramas que **solo** existen en el remoto, nombres con varias barras y
/// paginación. El grafo completo tiene que verlas y ninguna página puede perder commits.
#[tokio::test]
async fn el_grafo_completo_ve_ramas_solo_remotas_y_pagina_sin_perder_nada() {
    let t = TestRepo::with_initial_commit();
    let base = t.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let _bare = t.with_bare_origin();
    t.git(&["push", "-q", "origin", "main"]);

    // HEAD se queda en `main`; la rama nace en el remoto y aquí no existe como local.
    t.git(&["checkout", "-q", "-b", "ccr-466506c4-f6y78p", &base]);
    t.write("remota.txt", b"remota\n");
    let solo_remota = t.commit_all("trabajo solo en el remoto");
    t.git(&["push", "-q", "origin", "ccr-466506c4-f6y78p"]);
    t.git(&["checkout", "-q", "main"]);
    t.git(&["branch", "-D", "ccr-466506c4-f6y78p"]);
    t.git(&["fetch", "-q", "origin"]);
    // Se borra la referencia local de la rama: solo queda `remotes/origin/...`.
    assert!(t
        .git(&["branch", "--list", "ccr-466506c4-f6y78p"])
        .trim()
        .is_empty());

    // Nombre con varias barras, también solo en el remoto.
    t.git(&[
        "checkout",
        "-q",
        "-b",
        "claude/ariadna-amend-epoch-fix-bgz9lc",
        &base,
    ]);
    t.write("barras.txt", b"barras\n");
    let varias_barras = t.commit_all("trabajo con nombre de varias barras");
    t.git(&[
        "push",
        "-q",
        "origin",
        "claude/ariadna-amend-epoch-fix-bgz9lc",
    ]);
    t.git(&["checkout", "-q", "main"]);
    t.git(&["branch", "-D", "claude/ariadna-amend-epoch-fix-bgz9lc"]);
    t.git(&["fetch", "-q", "origin"]);

    let engine = plain_engine();
    let repo = engine.open_repository(t.path()).await.unwrap();

    // 1. La enumeración de referencias coincide con `for-each-ref` de ramas y remotas.
    let locales = engine.branches(&repo).await.unwrap();
    let remotas = engine.remote_tracking_branches(&repo).await.unwrap();
    let esperadas: Vec<String> = t
        .git(&[
            "for-each-ref",
            "--format=%(refname:lstrip=2)",
            "refs/heads",
            "refs/remotes",
        ])
        .lines()
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty() && !l.ends_with("/HEAD"))
        .collect();
    let mut vistas: Vec<String> = locales
        .iter()
        .chain(remotas.iter())
        .map(|b| b.name.clone())
        .collect();
    vistas.sort();
    let mut esperadas_ordenadas = esperadas.clone();
    esperadas_ordenadas.sort();
    assert_eq!(
        vistas, esperadas_ordenadas,
        "el panel lista las mismas refs que git"
    );

    // 2. Sus commits salen en el grafo completo y no en el de HEAD.
    let head = engine.log_page(&repo, 0, 50).await.unwrap();
    assert!(!head
        .iter()
        .any(|c| c.sha == solo_remota || c.sha == varias_barras));
    let todo = engine.log_page_all(&repo, 0, 50).await.unwrap();
    let shas: Vec<&str> = todo.iter().map(|c| c.sha.as_str()).collect();
    assert!(
        shas.contains(&solo_remota.as_str()),
        "rama solo remota: {shas:?}"
    );
    assert!(
        shas.contains(&varias_barras.as_str()),
        "nombre con varias barras: {shas:?}"
    );

    // 3. Paginando de uno en uno no se pierde ni se repite ningún commit.
    let total = engine.log_page_all(&repo, 0, 500).await.unwrap();
    let mut paginado: Vec<String> = Vec::new();
    for skip in 0..total.len() {
        let pagina = engine.log_page_all(&repo, skip, 1).await.unwrap();
        assert_eq!(pagina.len(), 1, "skip {skip} devuelve un commit");
        paginado.push(pagina[0].sha.clone());
    }
    let esperado: Vec<String> = total.iter().map(|c| c.sha.clone()).collect();
    assert_eq!(
        paginado, esperado,
        "las páginas coinciden con la lista completa"
    );
}
