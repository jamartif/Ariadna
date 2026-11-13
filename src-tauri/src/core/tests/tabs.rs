//! Abrir repos, dedupe, cerrar, reordenar, tope de 20, eventos y recientes.

use ariadna_git::RepoKey;

use super::*;
use crate::core::{ApiSessionEventKind, SOFT_TAB_LIMIT};

#[tokio::test]
async fn abre_un_repo_y_tambien_desde_una_subcarpeta() {
    let env = Env::new();
    let repo = env.repo("proyecto");
    std::fs::create_dir_all(repo.join("src/hondo")).unwrap();

    let r = env.core.open_repository(&Env::s(&repo)).await.unwrap();
    assert!(!r.already_open);
    assert_eq!(r.tab.name, "proyecto");
    assert_eq!(r.tab.branch.as_deref(), Some("main"));
    assert!(!r.tab.missing && !r.tab.detached);
    assert_eq!(r.tab.path, Env::s(&repo));

    // Desde una subcarpeta sube a la raíz y devuelve la pestaña existente.
    let sub = env
        .core
        .open_repository(&Env::s(&repo.join("src/hondo")))
        .await
        .unwrap();
    assert!(sub.already_open);
    assert_eq!(sub.tab.tab_id, r.tab.tab_id);

    let st = env.core.app_state();
    assert_eq!(st.tabs.len(), 1);
    assert_eq!(st.active_tab_id.as_deref(), Some(r.tab.tab_id.as_str()));
    assert_eq!(st.recents.len(), 1);
    assert_eq!(st.recents[0].path, Env::s(&repo));
    assert_eq!(st.recents[0].last_branch.as_deref(), Some("main"));
    assert!(st.git.is_some() && st.git_problem.is_none());
}

#[tokio::test]
async fn una_carpeta_que_no_es_repo_da_not_a_repository() {
    let env = Env::new();
    let dir = env.root.join("suelta");
    std::fs::create_dir_all(&dir).unwrap();
    let e = env.core.open_repository(&Env::s(&dir)).await.unwrap_err();
    assert_eq!(e.code, "NOT_A_REPOSITORY");
    let e = env
        .core
        .open_repository(&Env::s(&env.root.join("no-existe")))
        .await
        .unwrap_err();
    assert_eq!(e.code, "NOT_A_REPOSITORY");
    assert!(env.core.app_state().tabs.is_empty());
}

#[tokio::test]
async fn sin_git_los_comandos_devuelven_git_not_found() {
    let env = Env::new();
    let core = crate::core::AppCore::new(
        AppConfig {
            data_dir: env.data.clone(),
            git: Err("no hay git".to_owned()),
            legacy_recents: None,
        },
        env.prompter.clone(),
        env.sink.clone(),
    );
    let st = core.app_state();
    assert!(st.git.is_none());
    assert_eq!(st.git_problem.as_deref(), Some("no hay git"));
    let e = core.open_repository(&Env::s(&env.root)).await.unwrap_err();
    assert_eq!(e.code, "GIT_NOT_FOUND");
}

#[cfg(unix)]
#[tokio::test]
async fn dedupe_por_ruta_equivalente_con_enlace_simbolico() {
    let env = Env::new();
    let repo = env.repo("real");
    let alias = env.root.join("alias");
    std::os::unix::fs::symlink(&repo, &alias).unwrap();

    let a = env.core.open_repository(&Env::s(&repo)).await.unwrap();
    let b = env.core.open_repository(&Env::s(&alias)).await.unwrap();
    assert!(b.already_open);
    assert_eq!(a.tab.tab_id, b.tab.tab_id);
    assert_eq!(env.core.app_state().tabs.len(), 1);
}

/// La clave de repo no distingue mayúsculas donde el sistema de archivos tampoco.
#[test]
fn la_clave_de_repo_pliega_mayusculas_segun_la_plataforma() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("MiRepo");
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    let upper = tmp.path().join("MIREPO/.git");
    let lower = tmp.path().join("mirepo/.git");
    // Forzado: con plegado, mayúsculas distintas son la misma clave.
    let a = RepoKey::from_paths_with(&dir, &upper, true);
    let b = RepoKey::from_paths_with(&dir, &lower, true);
    assert_eq!(a, b);
    // Sin plegado forzado: en un sistema de archivos que distingue mayúsculas (Linux) son claves
    // distintas; en uno que no las distingue (macOS, Windows) `canonicalize` devuelve la
    // capitalización real del disco para ambas rutas y la clave coincide igualmente.
    let a = RepoKey::from_paths_with(&dir, &upper, false);
    let b = RepoKey::from_paths_with(&dir, &lower, false);
    if cfg!(any(windows, target_os = "macos")) {
        assert_eq!(a, b);
    } else {
        assert_ne!(a, b);
    }
}

#[cfg(any(windows, target_os = "macos"))]
#[tokio::test]
async fn dedupe_con_distinta_capitalizacion_en_sistemas_insensibles() {
    let env = Env::new();
    let repo = env.repo("MiProyecto");
    let upper = env.root.join("MIPROYECTO");
    let a = env.core.open_repository(&Env::s(&repo)).await.unwrap();
    let b = env.core.open_repository(&Env::s(&upper)).await.unwrap();
    assert!(b.already_open);
    assert_eq!(a.tab.tab_id, b.tab.tab_id);
}

#[tokio::test]
async fn un_worktree_comparte_pestana_con_su_repo_y_avisa() {
    let env = Env::new();
    let repo = env.repo("principal");
    let wt = env.root.join("principal-wt");
    env.git(
        &repo,
        &["worktree", "add", "-q", &Env::s(&wt), "-b", "otra"],
    );

    let a = env.core.open_repository(&Env::s(&repo)).await.unwrap();
    let b = env.core.open_repository(&Env::s(&wt)).await.unwrap();
    assert!(b.already_open);
    assert_eq!(a.tab.tab_id, b.tab.tab_id);
    assert!(b.warning.as_deref().unwrap_or("").contains("worktree"));
    assert_eq!(env.core.app_state().tabs.len(), 1);
}

#[tokio::test]
async fn reabrir_activa_la_pestana_existente() {
    let env = Env::new();
    let a = env.repo("a");
    let b = env.repo("b");
    let ida = env.open(&a).await;
    let idb = env.open(&b).await;
    assert_eq!(env.core.app_state().active_tab_id, Some(idb.clone()));
    let again = env.core.open_repository(&Env::s(&a)).await.unwrap();
    assert!(again.already_open);
    assert_eq!(again.tab.tab_id, ida);
    assert_eq!(env.core.app_state().active_tab_id, Some(ida));
}

#[tokio::test]
async fn reordena_ignorando_ids_desconocidos_y_conservando_los_que_faltan() {
    let env = Env::new();
    let ids = [
        env.open(&env.repo("a")).await,
        env.open(&env.repo("b")).await,
        env.open(&env.repo("c")).await,
    ];
    let order = |env: &Env| -> Vec<String> {
        env.core
            .app_state()
            .tabs
            .into_iter()
            .map(|t| t.tab_id)
            .collect()
    };
    env.core
        .reorder_tabs(&[ids[2].clone(), ids[0].clone(), ids[1].clone()])
        .unwrap();
    assert_eq!(
        order(&env),
        vec![ids[2].clone(), ids[0].clone(), ids[1].clone()]
    );

    env.core
        .reorder_tabs(&["fantasma".to_owned(), ids[1].clone()])
        .unwrap();
    assert_eq!(
        order(&env),
        vec![ids[1].clone(), ids[2].clone(), ids[0].clone()]
    );
    // La activa no cambia al reordenar.
    assert_eq!(env.core.app_state().active_tab_id, Some(ids[2].clone()));
}

#[tokio::test]
async fn cerrar_la_activa_pasa_a_la_vecina_y_actualiza_la_sesion() {
    let env = Env::new();
    let ids = [
        env.open(&env.repo("a")).await,
        env.open(&env.repo("b")).await,
        env.open(&env.repo("c")).await,
    ];
    env.core.activate_tab(&ids[1]).unwrap();
    assert!(env.core.close_tab(&ids[1]).await.unwrap());
    let st = env.core.app_state();
    assert_eq!(st.tabs.len(), 2);
    assert_eq!(st.active_tab_id, Some(ids[2].clone()));
    assert_eq!(
        env.prompter.count(),
        0,
        "sin trabajo pendiente no se pregunta"
    );

    // session.json refleja solo las abiertas.
    let (data, _) = crate::core::SessionStore::new(env.data.join("session.json")).load();
    assert_eq!(data.tabs.len(), 2);

    let e = env.core.close_tab("tab-999").await.unwrap_err();
    assert_eq!(e.code, "REPO_MISSING");
    let e = env.core.get_snapshot(&ids[1]).await.unwrap_err();
    assert_eq!(e.code, "REPO_MISSING");
}

#[tokio::test]
async fn cerrar_con_trabajo_pendiente_pregunta_y_respeta_el_no() {
    let env = Env::new();
    let repo = env.repo("lento");
    slow_pre_commit_hook(&repo, 30);
    env.write(&repo, "nuevo.txt", b"x\n");
    let id = env.open(&repo).await;
    env.core.stage(&id, vec!["nuevo.txt".into()]).await.unwrap();

    // Un commit que se queda en el hook = mutación en curso.
    let core = env.core.clone();
    let tid = id.clone();
    let task = tokio::spawn(async move { core.commit(&tid, "lento".to_owned(), false).await });
    wait_until("la pestaña aparece ocupada", || {
        env.core
            .app_state()
            .tabs
            .first()
            .is_some_and(|t| t.busy.as_deref() == Some("commit"))
    })
    .await;

    // El usuario dice que no: la pestaña sigue.
    env.prompter.reject_all();
    assert!(!env.core.close_tab(&id).await.unwrap());
    assert_eq!(env.core.app_state().tabs.len(), 1);
    assert_eq!(env.prompter.count(), 1);
    let q = env.prompter.last();
    assert!(
        q.message.contains("lento") && q.message.contains("commit"),
        "{q:?}"
    );

    // El usuario dice que sí: se cancela y se cierra.
    env.prompter.accept_all();
    assert!(env.core.close_tab(&id).await.unwrap());
    assert!(env.core.app_state().tabs.is_empty());
    let r = task.await.unwrap();
    assert_eq!(r.unwrap_err().code, "CANCELLED");
}

#[tokio::test]
async fn salir_con_trabajo_pendiente_sigue_el_mismo_camino() {
    let env = Env::new();
    let repo = env.repo("salida");
    slow_pre_commit_hook(&repo, 30);
    env.write(&repo, "n.txt", b"x\n");
    let id = env.open(&repo).await;
    env.core.stage(&id, vec!["n.txt".into()]).await.unwrap();
    let core = env.core.clone();
    let tid = id.clone();
    let task = tokio::spawn(async move { core.commit(&tid, "m".to_owned(), false).await });
    wait_until("ocupada", || {
        env.core
            .app_state()
            .tabs
            .first()
            .is_some_and(|t| t.busy.is_some())
    })
    .await;

    env.prompter.reject_all();
    assert!(!env.core.shutdown().await);
    assert_eq!(env.core.app_state().tabs.len(), 1);
    env.prompter.accept_all();
    assert!(env.core.shutdown().await);
    assert!(env.core.app_state().tabs.is_empty());
    assert!(task.await.unwrap().is_err());
    // La sesión guardada conserva la pestaña para el próximo arranque.
    let (data, _) = crate::core::SessionStore::new(env.data.join("session.json")).load();
    assert_eq!(data.tabs.len(), 1);
}

#[tokio::test]
async fn tope_blando_de_20_pestanas_avisa_sin_fallar() {
    let env = Env::new();
    for i in 0..SOFT_TAB_LIMIT {
        let r = env
            .core
            .open_repository(&Env::s(&env.empty_repo(&format!("r{i}"))))
            .await
            .unwrap();
        assert!(r.warning.is_none(), "r{i}: {:?}", r.warning);
    }
    let r = env
        .core
        .open_repository(&Env::s(&env.empty_repo("extra")))
        .await
        .unwrap();
    assert!(!r.already_open);
    assert!(
        r.warning.as_deref().unwrap_or("").contains("21"),
        "{:?}",
        r.warning
    );
    assert_eq!(env.core.app_state().tabs.len(), SOFT_TAB_LIMIT + 1);
}

#[tokio::test]
async fn los_eventos_llegan_con_snapshot_tab_updated_y_session_event() {
    let env = Env::new();
    let repo = env.repo("eventos");
    let id = env.open(&repo).await;
    env.write(&repo, "a.txt", b"uno\n");
    env.core.refresh(&id).await.unwrap();
    let before = env.core.get_snapshot(&id).await.unwrap().0.revision;

    env.core.stage(&id, vec!["a.txt".into()]).await.unwrap();

    wait_until("snapshot nuevo con a.txt preparado", || {
        env.sink.all().iter().any(|e| match e {
            AppEvent::Snapshot(s) => {
                s.tab_id == id
                    && s.snapshot.0.revision > before
                    && s.snapshot.0.status.entries.iter().any(|x| x.is_staged())
            }
            _ => false,
        })
    })
    .await;
    wait_until("tab-updated con 1 cambio y sin ocupación", || {
        env.sink.all().iter().any(|e| match e {
            AppEvent::TabUpdated(t) => t.change_count == 1 && t.busy.is_none(),
            _ => false,
        })
    })
    .await;
    let events = env.sink.all();
    let has = |f: &dyn Fn(&ApiSessionEventKind) -> bool| {
        events
            .iter()
            .any(|e| matches!(e, AppEvent::Session(s) if f(&s.kind)))
    };
    assert!(has(
        &|k| matches!(k, ApiSessionEventKind::OperationStarted { label, .. } if label == "preparar")
    ));
    assert!(has(&|k| matches!(
        k,
        ApiSessionEventKind::OperationFinished { ok: true, .. }
    )));
    assert!(events.iter().any(|e| matches!(e, AppEvent::TabsChanged(_))));
}

#[tokio::test]
async fn cancelar_una_operacion_por_su_id() {
    let env = Env::new();
    let repo = env.repo("cancelable");
    slow_pre_commit_hook(&repo, 30);
    env.write(&repo, "n.txt", b"x\n");
    let id = env.open(&repo).await;
    env.core.stage(&id, vec!["n.txt".into()]).await.unwrap();
    let core = env.core.clone();
    let tid = id.clone();
    let task = tokio::spawn(async move { core.commit(&tid, "c".to_owned(), false).await });

    let mut op = None;
    wait_until("OperationStarted del commit", || {
        op = env.sink.all().iter().find_map(|e| match e {
            AppEvent::Session(s) => match &s.kind {
                ApiSessionEventKind::OperationStarted {
                    operation_id,
                    label,
                } if label == "commit" => Some(*operation_id),
                _ => None,
            },
            _ => None,
        });
        op.is_some()
    })
    .await;
    assert!(!env.core.cancel_operation(&id, 999_999).unwrap());
    assert!(env.core.cancel_operation(&id, op.unwrap()).unwrap());
    assert_eq!(task.await.unwrap().unwrap_err().code, "CANCELLED");
    // La pestaña sigue viva y utilizable.
    assert!(env.core.refresh(&id).await.is_ok());
}

#[tokio::test]
async fn favoritos_y_olvidar_recientes() {
    let env = Env::new();
    let repo = env.repo("fav");
    env.open(&repo).await;
    let p = Env::s(&repo);
    let list = env.core.toggle_favorite(&p).unwrap();
    assert!(list[0].is_favorite);
    let list = env.core.toggle_favorite(&p).unwrap();
    assert!(!list[0].is_favorite);
    assert!(env.core.forget_recent(&p).unwrap().is_empty());
    // Un repo que no está en recientes se añade al marcarlo.
    let list = env.core.toggle_favorite("/otra/ruta/x").unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].is_favorite);
}

#[tokio::test]
async fn importa_el_recents_json_del_swift() {
    let env = Env::new();
    let legacy = env.root.join("swift-recents.json");
    std::fs::write(
        &legacy,
        r#"[
  {
    "isFavorite" : true,
    "lastBranch" : "main",
    "lastOpened" : "2023-11-14T22:13:20Z",
    "name" : "Viejo",
    "path" : "/Users/x/Viejo"
  }
]"#,
    )
    .unwrap();
    let core = crate::core::AppCore::new(
        AppConfig {
            data_dir: env.data.clone(),
            git: Ok(git_install()),
            legacy_recents: Some(legacy),
        },
        env.prompter.clone(),
        env.sink.clone(),
    );
    let st = core.app_state();
    assert_eq!(st.recents.len(), 1);
    assert_eq!(st.recents[0].name, "Viejo");
    assert!(st.recents[0].is_favorite);
}

#[tokio::test]
async fn pick_folder_pasa_por_el_prompter() {
    let env = Env::new();
    assert_eq!(env.core.pick_folder().await, None);
    *env.prompter.folder.lock().unwrap() = Some(env.root.clone());
    assert_eq!(env.core.pick_folder().await, Some(Env::s(&env.root)));
}
