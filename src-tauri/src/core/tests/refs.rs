//! `extra_refs`: refs de pull request de los dos proveedores y worktrees, tal cual llegan al riel.
//!
//! El riel usa `Branch::name` como clave de la lista: si el motor devolviera el nombre recortado
//! (`7/head`), un repo espejo con `refs/pull/7/head` y `refs/merge-requests/7/head` daría dos claves
//! iguales y Svelte 5 rompería la lista.

use super::Env;

/// Un repo espejo puede tener a la vez `refs/pull/*` (GitHub) y `refs/merge-requests/*` (GitLab),
/// con el mismo número en las dos. Las dos tienen que llegar con su referencia completa, sin
/// `refs/pull/N/merge`, ordenadas por número de PR, y con el worktree en detached HEAD incluido.
#[tokio::test]
async fn extra_refs_distingue_proveedores_y_trae_los_worktrees() {
    let env = Env::new();
    let repo = env.repo("refs");
    let head = env.git(&repo, &["rev-parse", "HEAD"]).trim().to_owned();
    // 9 y 10 a propósito: el orden tiene que ser numérico (9 antes que 10), no lexicográfico.
    for referencia in [
        "refs/pull/9/head",
        "refs/pull/10/head",
        "refs/pull/10/merge",
        "refs/merge-requests/7/head",
    ] {
        env.git(&repo, &["update-ref", referencia, &head]);
    }
    let detached = env.root.join("refs-detached");
    env.git(
        &repo,
        &["worktree", "add", "--detach", &Env::s(&detached), &head],
    );

    let id = env.open(&repo).await;
    let extra = env.core.extra_refs(&id).await.unwrap();

    let nombres: Vec<&str> = extra.pull_refs.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        nombres,
        [
            "refs/merge-requests/7/head",
            "refs/pull/9/head",
            "refs/pull/10/head"
        ],
        "orden por número de PR, proveedores distintos y sin `refs/pull/N/merge`"
    );
    // Lo que rompía la lista: dos entradas con el mismo nombre.
    let mut unicos = nombres.clone();
    unicos.sort_unstable();
    unicos.dedup();
    assert_eq!(unicos.len(), nombres.len(), "claves repetidas: {nombres:?}");

    let principal = extra
        .worktrees
        .iter()
        .find(|w| w.is_main)
        .expect("el worktree principal va siempre primero");
    assert_eq!(principal.branch.as_deref(), Some("main"));

    let otro = extra
        .worktrees
        .iter()
        .find(|w| !w.is_main)
        .expect("el worktree en detached HEAD tiene que salir");
    assert!(otro.detached, "{otro:?}");
    assert_eq!(otro.branch, None);
    assert_eq!(otro.head, head);
    // Git escribe la ruta con los separadores del sistema (`C:\...`) y aquí se construyó con `/`:
    // se comparan con una sola forma para que valga en Windows y en Unix.
    let normal = |p: &str| {
        p.split(['\\', '/'])
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("/")
    };
    assert_eq!(normal(&otro.path), normal(&Env::s(&detached)));
}
