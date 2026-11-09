//! `session.json`: guardado atómico, respaldo, recuperación y restauración de pestañas.

use super::*;
use crate::core::{SavedTab, SessionData, SessionLoad, SessionStore};

fn data(paths: &[&str], active: Option<usize>) -> SessionData {
    SessionData {
        version: 1,
        tabs: paths
            .iter()
            .map(|p| SavedTab {
                path: (*p).to_owned(),
            })
            .collect(),
        active,
    }
}

#[test]
fn guarda_de_forma_atomica_y_deja_respaldo() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SessionStore::new(tmp.path().join("sub/session.json"));
    assert_eq!(store.load().1, SessionLoad::Missing);

    store.save(&data(&["/a"], Some(0))).unwrap();
    store.save(&data(&["/a", "/b"], Some(1))).unwrap();
    let dir = tmp.path().join("sub");
    assert!(dir.join("session.json.bak").exists());
    assert!(!dir.join("session.json.tmp").exists(), "no queda temporal");
    let (d, how) = store.load();
    assert_eq!(how, SessionLoad::Ok);
    assert_eq!(d, data(&["/a", "/b"], Some(1)));
    // El respaldo es la versión anterior.
    let bak: SessionData =
        serde_json::from_slice(&std::fs::read(dir.join("session.json.bak")).unwrap()).unwrap();
    assert_eq!(bak, data(&["/a"], Some(0)));
}

#[test]
fn corrupto_se_recupera_del_respaldo_y_se_aparta() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SessionStore::new(tmp.path().join("session.json"));
    store.save(&data(&["/a"], None)).unwrap();
    store.save(&data(&["/a", "/b"], Some(0))).unwrap();
    std::fs::write(store.file(), b"{ esto no es json").unwrap();

    let (d, how) = store.load();
    assert_eq!(how, SessionLoad::RecoveredFromBackup);
    assert_eq!(d, data(&["/a"], None));
    assert!(tmp.path().join("session.json.corrupt").exists());
    // Tras recuperar, el principal vuelve a ser válido.
    assert_eq!(store.load().1, SessionLoad::Ok);
}

#[test]
fn corrupto_sin_respaldo_empieza_vacio_sin_borrar_nada() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SessionStore::new(tmp.path().join("session.json"));
    std::fs::write(store.file(), b"\x00\x01basura").unwrap();
    let (d, how) = store.load();
    assert_eq!(how, SessionLoad::ResetCorrupt);
    assert!(d.tabs.is_empty());
    assert_eq!(
        std::fs::read(tmp.path().join("session.json.corrupt")).unwrap(),
        b"\x00\x01basura"
    );
}

#[test]
fn una_version_desconocida_no_se_interpreta_ni_se_pisa() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SessionStore::new(tmp.path().join("session.json"));
    std::fs::write(
        store.file(),
        br#"{"version": 99, "tabs": [{"path": "/x"}], "active": null}"#,
    )
    .unwrap();
    let (d, how) = store.load();
    assert_eq!(how, SessionLoad::ResetCorrupt);
    assert!(d.tabs.is_empty());
    assert!(tmp.path().join("session.json.corrupt").exists());
}

#[tokio::test]
async fn restaura_pestanas_orden_y_activa() {
    let env = Env::new();
    let ids = [
        env.open(&env.repo("a")).await,
        env.open(&env.repo("b")).await,
        env.open(&env.repo("c")).await,
    ];
    env.core
        .reorder_tabs(&[ids[1].clone(), ids[2].clone(), ids[0].clone()])
        .unwrap();
    env.core.activate_tab(&ids[2]).unwrap();

    let (core, sink) = env.restart();
    assert!(
        core.app_state().tabs.is_empty(),
        "antes de restaurar no hay nada"
    );
    core.restore_session().await;
    let st = core.app_state();
    let names: Vec<_> = st.tabs.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["b", "c", "a"]);
    assert!(st
        .tabs
        .iter()
        .all(|t| !t.missing && t.branch.as_deref() == Some("main")));
    let active = st
        .tabs
        .iter()
        .find(|t| Some(&t.tab_id) == st.active_tab_id.as_ref());
    assert_eq!(active.map(|t| t.name.as_str()), Some("c"));
    assert!(sink
        .all()
        .iter()
        .any(|e| matches!(e, AppEvent::TabsChanged(_))));
    // Las pestañas restauradas funcionan.
    let snap = core.get_snapshot(&st.tabs[0].tab_id).await.unwrap();
    assert_eq!(snap.0.status.branch_name.as_deref(), Some("main"));
}

#[tokio::test]
async fn ruta_inexistente_queda_como_pestana_missing_y_se_puede_reintentar() {
    let env = Env::new();
    let a = env.repo("a");
    let b = env.repo("b");
    env.open(&a).await;
    env.open(&b).await;
    std::fs::remove_dir_all(&b).unwrap();

    let (core, _) = env.restart();
    core.restore_session().await;
    let st = core.app_state();
    assert_eq!(st.tabs.len(), 2);
    assert!(!st.tabs[0].missing);
    assert!(st.tabs[1].missing);
    assert_eq!(st.tabs[1].name, "b");
    let missing_id = st.tabs[1].tab_id.clone();
    let e = core.get_snapshot(&missing_id).await.unwrap_err();
    assert_eq!(e.code, "REPO_MISSING");
    let e = core.stage(&missing_id, vec!["x".into()]).await.unwrap_err();
    assert_eq!(e.code, "REPO_MISSING");

    // «Reintentar»: vuelve a existir y open_repository sustituye la pestaña en su sitio.
    env.repo("b");
    let r = core.open_repository(&Env::s(&b)).await.unwrap();
    assert!(!r.already_open && !r.tab.missing);
    let st = core.app_state();
    assert_eq!(st.tabs.len(), 2, "no se duplica");
    assert_eq!(st.tabs[1].tab_id, r.tab.tab_id);
    assert_eq!(st.tabs[1].branch.as_deref(), Some("main"));
}

#[tokio::test]
async fn carpeta_que_ya_no_es_repo_tambien_queda_missing() {
    let env = Env::new();
    let a = env.repo("a");
    env.open(&a).await;
    std::fs::remove_dir_all(a.join(".git")).unwrap();
    let (core, _) = env.restart();
    core.restore_session().await;
    let st = core.app_state();
    assert_eq!(st.tabs.len(), 1);
    assert!(st.tabs[0].missing);
}

#[tokio::test]
async fn restaurar_desde_el_respaldo_si_session_json_esta_corrupto() {
    let env = Env::new();
    env.open(&env.repo("a")).await;
    env.open(&env.repo("b")).await; // segundo guardado => hay .bak con solo «a»
    std::fs::write(env.data.join("session.json"), b"{{{").unwrap();
    let (core, _) = env.restart();
    core.restore_session().await;
    let names: Vec<_> = core.app_state().tabs.into_iter().map(|t| t.name).collect();
    assert_eq!(names, ["a"]);
}

#[tokio::test]
async fn dos_rutas_del_mismo_repo_en_la_sesion_dan_una_pestana() {
    let env = Env::new();
    let repo = env.repo("uno");
    let store = SessionStore::new(env.data.join("session.json"));
    store
        .save(&data(
            &[&Env::s(&repo), &Env::s(&repo.join(".")), "/no/existe"],
            Some(0),
        ))
        .unwrap();
    let (core, _) = env.restart();
    core.restore_session().await;
    let st = core.app_state();
    assert_eq!(st.tabs.len(), 2);
    assert!(!st.tabs[0].missing && st.tabs[1].missing);
}
