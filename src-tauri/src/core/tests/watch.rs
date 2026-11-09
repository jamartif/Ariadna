//! Vigilancia de archivos de extremo a extremo: git real, `AppCore` y eventos `snapshot`.

use super::*;
use crate::core::watch::WatchTimings;

fn fast(env: &Env) {
    env.core.set_watch_timings(WatchTimings {
        quiet: Duration::from_millis(100),
        max_wait: Duration::from_millis(600),
        poll_every: Duration::from_millis(150),
    });
}

fn snaps(env: &Env, tab: &str) -> usize {
    env.sink
        .all()
        .iter()
        .filter(|e| matches!(e, AppEvent::Snapshot(s) if s.tab_id == tab))
        .count()
}

fn last_snapshot(env: &Env, tab: &str) -> std::sync::Arc<ariadna_git::Snapshot> {
    env.sink
        .all()
        .iter()
        .rev()
        .find_map(|e| match e {
            AppEvent::Snapshot(s) if s.tab_id == tab => Some(std::sync::Arc::clone(&s.snapshot.0)),
            _ => None,
        })
        .expect("ningún snapshot")
}

/// Espera a que haya exactamente `n` snapshots y comprueba que no llegan más.
async fn expect_snapshots(env: &Env, tab: &str, n: usize, what: &str) {
    wait_until(what, || snaps(env, tab) >= n).await;
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(snaps(env, tab), n, "{what}: ni más ni menos");
}

#[tokio::test]
async fn cambios_externos_publican_un_snapshot_nuevo_cada_uno() {
    let env = Env::new();
    fast(&env);
    let repo = env.repo("vivo");
    let tab = env.open(&repo).await;
    assert_eq!(
        env.core.watcher_state(&tab),
        Some(false),
        "vigilancia nativa"
    );
    // Estado inicial estable.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let base = snaps(&env, &tab);
    let rev0 = env.core.get_snapshot(&tab).await.unwrap().0.revision;

    // Crear un archivo.
    env.write(&repo, "nuevo.txt", b"hola\n");
    expect_snapshots(&env, &tab, base + 1, "tras crear").await;
    let s = last_snapshot(&env, &tab);
    assert!(
        s.revision > rev0,
        "la revisión sube (la UI descarta las que no suben)"
    );
    assert!(s
        .status
        .entries
        .iter()
        .any(|e| e.path.to_string_lossy() == "nuevo.txt" && e.is_untracked));

    // `git add` externo: cambia .git/index.
    env.git(&repo, &["add", "nuevo.txt"]);
    expect_snapshots(&env, &tab, base + 2, "tras git add externo").await;
    let s = last_snapshot(&env, &tab);
    assert!(s
        .status
        .entries
        .iter()
        .any(|e| e.path.to_string_lossy() == "nuevo.txt" && !e.is_untracked));
    assert!(s.revision > rev0 + 1);

    // Editar un archivo seguido.
    env.write(&repo, "README.md", b"hola, mundo\n");
    expect_snapshots(&env, &tab, base + 3, "tras editar").await;

    // Borrar.
    std::fs::remove_file(repo.join("README.md")).unwrap();
    expect_snapshots(&env, &tab, base + 4, "tras borrar").await;

    // Un commit externo mueve HEAD/refs.
    env.git(&repo, &["add", "-A"]);
    wait_until("add -A", || snaps(&env, &tab) > base + 4).await;
    env.git(&repo, &["commit", "-q", "-m", "otro"]);
    wait_until("el estado limpio tras el commit", || {
        snaps(&env, &tab) > base + 5
            && last_snapshot(&env, &tab)
                .status
                .entries
                .iter()
                .all(|e| e.is_ignored)
    })
    .await;
}

#[tokio::test]
async fn el_ruido_de_git_y_las_carpetas_ignoradas_no_publican_nada() {
    let env = Env::new();
    fast(&env);
    let repo = env.repo("ruido");
    std::fs::create_dir_all(repo.join("node_modules/pkg")).unwrap();
    let tab = env.open(&repo).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let base = snaps(&env, &tab);

    env.write(&repo, ".git/objects/zz/basura", b"x");
    env.write(&repo, ".git/index.lock", b"");
    env.write(&repo, "node_modules/pkg/index.js", b"x");
    env.write(&repo, ".git/logs/refs/extra", b"x");
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(snaps(&env, &tab), base);
}

#[tokio::test]
async fn nuestras_propias_mutaciones_no_provocan_refrescos_en_cascada() {
    let env = Env::new();
    fast(&env);
    let repo = env.repo("propio");
    env.write(&repo, "a.txt", b"1\n");
    let tab = env.open(&repo).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let base = snaps(&env, &tab);

    env.core.stage(&tab, vec!["a.txt".into()]).await.unwrap();
    // Un snapshot (el de la mutación); el eco del vigilante no publica uno igual.
    expect_snapshots(&env, &tab, base + 1, "stage propio").await;

    env.core.unstage(&tab, vec!["a.txt".into()]).await.unwrap();
    expect_snapshots(&env, &tab, base + 2, "unstage propio").await;
}

#[tokio::test]
async fn solo_la_pestana_activa_vigila_y_al_activar_se_pone_al_dia() {
    let env = Env::new();
    fast(&env);
    let a = env.repo("a");
    let b = env.repo("b");
    let ta = env.open(&a).await;
    let tb = env.open(&b).await;
    // `b` es la activa.
    assert_eq!(env.core.watcher_state(&ta), None);
    assert_eq!(env.core.watcher_state(&tb), Some(false));

    // Cambio en la inactiva: nada se publica...
    tokio::time::sleep(Duration::from_millis(500)).await;
    let base_a = snaps(&env, &ta);
    env.write(&a, "dormida.txt", b"x");
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(snaps(&env, &ta), base_a);

    // ...hasta que se activa.
    env.core.activate_tab(&ta).unwrap();
    wait_until("al activar se refresca", || snaps(&env, &ta) > base_a).await;
    assert!(last_snapshot(&env, &ta)
        .status
        .entries
        .iter()
        .any(|e| e.path.to_string_lossy() == "dormida.txt"));
    assert_eq!(env.core.watcher_state(&ta), Some(false));
    assert_eq!(
        env.core.watcher_state(&tb),
        None,
        "la anterior libera su vigilante"
    );
}

#[tokio::test]
async fn cerrar_la_pestana_libera_el_vigilante_y_el_siguiente_pasa_a_vigilar() {
    let env = Env::new();
    fast(&env);
    let a = env.repo("a");
    let b = env.repo("b");
    let ta = env.open(&a).await;
    let tb = env.open(&b).await;
    assert!(env.core.close_tab(&tb).await.unwrap());
    assert_eq!(env.core.watcher_state(&tb), None);
    assert_eq!(env.core.watcher_state(&ta), Some(false));

    // Cerrada, ya no hay eventos para ella.
    let closed_count = snaps(&env, &tb);
    env.write(&b, "tras-cerrar.txt", b"x");
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(snaps(&env, &tb), closed_count);
    let base_a = snaps(&env, &ta);
    env.write(&a, "x.txt", b"x");
    wait_until("la pestaña que queda vigila", || snaps(&env, &ta) > base_a).await;
}

#[tokio::test]
async fn recuperar_el_foco_refresca_la_pestana_activa() {
    let env = Env::new();
    fast(&env);
    let repo = env.repo("foco");
    let tab = env.open(&repo).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let base = snaps(&env, &tab);

    env.core.set_window_focus(false);
    env.core.set_window_focus(true);
    // Sin cambios reales no se publica nada nuevo...
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(snaps(&env, &tab), base);
}
