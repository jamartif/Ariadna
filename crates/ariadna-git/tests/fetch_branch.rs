//! `fetch_into_branch`: actualizar una rama local que no está activa, sin cambiar de rama y
//! sin forzar nunca (solo avance rápido).

#![allow(clippy::unwrap_used)]

mod common;

use common::TestRepo;

/// Remoto con `main` y un clon que trabaja desde otra rama: `main` queda local pero no activa.
fn remoto_y_clon() -> (TestRepo, TestRepo) {
    let origen = TestRepo::init();
    origen.write("a.txt", b"uno\n");
    origen.git(&["add", "-A"]);
    origen.git(&["commit", "-q", "-m", "inicial"]);
    origen.git(&["branch", "-M", "main"]);

    let url = origen.path().to_string_lossy().into_owned();
    let otro = TestRepo::init();
    otro.git(&["remote", "add", "origin", &url]);
    otro.git(&["fetch", "-q", "origin"]);
    otro.git(&["checkout", "-q", "-B", "main", "origin/main"]);
    otro.git(&["checkout", "-q", "-b", "trabajo"]);
    (origen, otro)
}

#[tokio::test]
async fn actualiza_la_rama_sin_cambiarla_de_rama() {
    let (origen, otro) = remoto_y_clon();
    let repo = otro.engine.open_repository(otro.path()).await.unwrap();

    // El origen avanza `main`.
    origen.write("a.txt", b"dos\n");
    origen.git(&["add", "-A"]);
    origen.git(&["commit", "-q", "-m", "segundo"]);

    let antes = otro.git(&["rev-parse", "main"]).trim().to_owned();
    otro.engine
        .fetch_into_branch(&repo, "origin", "main")
        .await
        .unwrap();
    let despues = otro.git(&["rev-parse", "main"]).trim().to_owned();
    assert_ne!(antes, despues, "la rama local debería haber avanzado");
    // No se cambió de rama: se sigue en `trabajo`.
    assert_eq!(
        otro.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "trabajo"
    );
}

#[tokio::test]
async fn no_fuerza_si_la_rama_divergio() {
    let (origen, otro) = remoto_y_clon();
    let repo = otro.engine.open_repository(otro.path()).await.unwrap();

    // Divergen: un commit en cada lado de `main`.
    origen.write("a.txt", b"origen\n");
    origen.git(&["add", "-A"]);
    origen.git(&["commit", "-q", "-m", "en el origen"]);

    otro.git(&["checkout", "-q", "main"]);
    otro.write("b.txt", b"local\n");
    otro.git(&["add", "-A"]);
    otro.git(&["commit", "-q", "-m", "en local"]);
    otro.git(&["checkout", "-q", "trabajo"]);

    let antes = otro.git(&["rev-parse", "main"]).trim().to_owned();
    let err = otro
        .engine
        .fetch_into_branch(&repo, "origin", "main")
        .await
        .expect_err("una divergencia no debe forzarse");
    assert!(
        format!("{err}").to_lowercase().contains("avance rápido"),
        "el error debería explicar el rechazo: {err}"
    );
    assert_eq!(
        otro.git(&["rev-parse", "main"]).trim(),
        antes,
        "nada se movió"
    );
}

#[tokio::test]
async fn avisa_si_la_rama_es_la_activa() {
    let (_origen, otro) = remoto_y_clon();
    let repo = otro.engine.open_repository(otro.path()).await.unwrap();
    let err = otro
        .engine
        .fetch_into_branch(&repo, "origin", "trabajo")
        .await
        .expect_err("la rama activa no se trae así");
    assert!(
        format!("{err}").contains("activa"),
        "el error debería decir que es la rama activa: {err}"
    );
}
