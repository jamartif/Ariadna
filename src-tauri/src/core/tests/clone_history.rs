//! Clonar (D20), historial paginado y ramas con confirmación nativa.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};

use ariadna_git::{IntegrationState, MergeFastForward, MergeResult, Secret};

use super::*;
use crate::core::api::CloneProgressEvent;

const PAT: &str = "pat-secreto-4e8d2b7a91";

fn clone_events(env: &Env) -> Vec<CloneProgressEvent> {
    env.sink
        .all()
        .into_iter()
        .filter_map(|e| match e {
            AppEvent::CloneProgress(p) => Some(p),
            _ => None,
        })
        .collect()
}

/// Repo con 3 commits y su copia bare, para clonar por `file://`.
fn origin(env: &Env) -> String {
    let repo = env.repo("origen");
    for i in 0..2 {
        env.write(&repo, &format!("f{i}.txt"), b"x\n");
        env.git(&repo, &["add", "-A"]);
        env.git(&repo, &["commit", "-q", "-m", &format!("c{i}")]);
    }
    let bare = env.root.join("origen.git");
    env.git(
        &env.root,
        &["clone", "-q", "--bare", &Env::s(&repo), &Env::s(&bare)],
    );
    format!("file://{}", Env::s(&bare))
}

#[tokio::test]
async fn clona_abre_una_pestana_y_emite_progreso() {
    let env = Env::new();
    let url = origin(&env);
    let dest = env.root.join("clones").join("copia");
    let r = env
        .core
        .clone_repository(url, Env::s(&dest), None, None, None)
        .await
        .unwrap();
    assert!(!r.already_open);
    assert_eq!(r.tab.name, "copia");
    assert!(dest.join(".git").exists() && dest.join("f1.txt").exists());
    let ev = clone_events(&env);
    assert!(!ev.is_empty(), "debe haber al menos el evento inicial");
    assert_eq!(ev[0].phase, "Preparando el clonado");
    assert!(ev.iter().all(|e| e.operation_id == ev[0].operation_id));
    // Está en la lista de pestañas y en recientes.
    let st = env.core.app_state();
    assert!(st.tabs.iter().any(|t| t.tab_id == r.tab.tab_id));
    assert!(st.recents.iter().any(|x| x.name == "copia"));
}

#[tokio::test]
async fn urls_y_destinos_invalidos_dan_error_claro_sin_rastro_del_pat() {
    let env = Env::new();
    let good = origin(&env);
    let pat = || Some(Secret::new(PAT));
    for bad in [
        "ext::sh -c 'echo hola'",
        "--upload-pack=touch /tmp/x",
        "https://u:clave@host.invalid/x.git",
        "",
    ] {
        let dest = env.root.join("nunca");
        let e = env
            .core
            .clone_repository(bad.into(), Env::s(&dest), None, Some("u".into()), pat())
            .await
            .unwrap_err();
        let shown = format!("{e:?}");
        assert!(
            !shown.contains(PAT) && !shown.contains("clave"),
            "{bad}: {shown}"
        );
        assert!(!dest.exists(), "{bad}");
    }
    // Destino relativo.
    let e = env
        .core
        .clone_repository(good.clone(), "relativa/x".into(), None, None, None)
        .await
        .unwrap_err();
    assert_eq!(e.code, "IO", "{e:?}");
    // Token con salto de línea: se rechaza sin repetirlo.
    let e = env
        .core
        .clone_repository(
            good,
            Env::s(&env.root.join("z")),
            None,
            None,
            Some(Secret::new("a\nb-secreto")),
        )
        .await
        .unwrap_err();
    assert!(!format!("{e:?}").contains("b-secreto"), "{e:?}");
}

/// Servidor que contesta 401 a todo y registra lo que recibe.
fn server_401() -> (
    u16,
    std::sync::Arc<AtomicBool>,
    std::sync::Arc<std::sync::Mutex<String>>,
) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.set_nonblocking(true).unwrap();
    let port = l.local_addr().unwrap().port();
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let (s2, seen2) = (stop.clone(), seen.clone());
    std::thread::spawn(move || {
        while !s2.load(Ordering::Relaxed) {
            if let Ok((mut c, _)) = l.accept() {
                let _ = c.set_nonblocking(false);
                let mut buf = [0u8; 4096];
                let n = c.read(&mut buf).unwrap_or(0);
                seen2
                    .lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&buf[..n]));
                let _ = c.write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"x\"\r\nContent-Length: 2\r\nConnection: close\r\n\r\nno",
                );
            } else {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    });
    (port, stop, seen)
}

#[tokio::test]
async fn un_401_es_auth_y_el_token_no_aparece_en_el_error_ni_en_disco() {
    let env = Env::new();
    let (port, stop, _seen) = server_401();
    let dest = env.root.join("a").join("privado");
    let e = env
        .core
        .clone_repository(
            format!("http://127.0.0.1:{port}/org/repo.git"),
            Env::s(&dest),
            None,
            Some("persona".into()),
            Some(Secret::new(PAT)),
        )
        .await
        .unwrap_err();
    stop.store(true, Ordering::Relaxed);
    assert_eq!(e.code, "AUTH", "{e:?}");
    let shown = format!("{} {:?}", e.message, e);
    assert!(!shown.contains(PAT), "{shown}");
    assert!(!env.root.join("a").exists(), "no debe quedar basura");
    // El token tampoco acabó en los datos de la app (recientes, sesión…).
    for entry in std::fs::read_dir(&env.data).into_iter().flatten().flatten() {
        if let Ok(s) = std::fs::read_to_string(entry.path()) {
            assert!(!s.contains(PAT), "{}", entry.path().display());
        }
    }
}

#[tokio::test]
async fn cancelar_un_clonado_por_su_id_lo_aborta_y_limpia() {
    let env = Env::new();
    // Acepta la conexión y no contesta nunca: el clonado se queda esperando.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let held = std::thread::spawn(move || {
        let conns: Vec<_> = l.incoming().take(1).flatten().collect();
        std::thread::sleep(Duration::from_secs(20));
        drop(conns);
    });
    let dest = env.root.join("lento");
    let core = env.core.clone();
    let (url, d) = (format!("http://127.0.0.1:{port}/x.git"), Env::s(&dest));
    let task = tokio::spawn(async move { core.clone_repository(url, d, None, None, None).await });
    wait_until("evento inicial", || !clone_events(&env).is_empty()).await;
    let id = clone_events(&env)[0].operation_id;
    // Tab vacío: los clonados se cancelan solo por id.
    assert!(env.core.cancel_operation("", id).unwrap());
    let e = task.await.unwrap().unwrap_err();
    assert_eq!(e.code, "CANCELLED", "{e:?}");
    assert!(!dest.exists());
    // Ya terminó: el id deja de existir.
    assert!(env.core.cancel_operation("tab-inexistente", id).is_err());
    drop(held);
}

#[tokio::test]
async fn credenciales_guardar_probar_y_olvidar_con_un_helper_falso() {
    let env = Env::new();
    // `test_credential` contra un servidor que exige credenciales: AUTH, no error.
    let (port, stop, _seen) = server_401();
    let t = env
        .core
        .test_credential(&format!("http://127.0.0.1:{port}/o/r.git"))
        .await
        .unwrap();
    stop.store(true, Ordering::Relaxed);
    assert!(!t.ok);
    assert_eq!(t.code.as_deref(), Some("AUTH"));
    // Rechaza ssh y secretos con saltos de línea sin repetirlos.
    let e = env
        .core
        .store_credential("git@github.com:o/r.git", "u", Secret::new("x"))
        .await
        .unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
    let e = env
        .core
        .store_credential(
            "https://github.com/o/r",
            "u",
            Secret::new("lin\nea-secreta"),
        )
        .await
        .unwrap_err();
    assert!(!format!("{e:?}").contains("ea-secreta"));
}

// --- historial -------------------------------------------------------------------------------

#[tokio::test]
async fn log_page_pagina_sin_solapes_y_filtra_por_texto() {
    let env = Env::new();
    let repo = env.repo("h");
    for i in 1..=9 {
        env.write(&repo, "n.txt", format!("{i}\n").as_bytes());
        env.git(&repo, &["add", "-A"]);
        env.git(&repo, &["commit", "-q", "-m", &format!("paso {i}")]);
    }
    let id = env.open(&repo).await;
    let all = env.core.log_page(&id, 0, 100, None).await.unwrap();
    assert_eq!(all.len(), 10);
    let p1 = env.core.log_page(&id, 0, 4, None).await.unwrap();
    let p2 = env.core.log_page(&id, 4, 4, None).await.unwrap();
    let p3 = env.core.log_page(&id, 8, 4, None).await.unwrap();
    assert_eq!((p1.len(), p2.len(), p3.len()), (4, 4, 2));
    let joined: Vec<_> = p1
        .iter()
        .chain(&p2)
        .chain(&p3)
        .map(|c| c.sha.clone())
        .collect();
    assert_eq!(
        joined,
        all.iter().map(|c| c.sha.clone()).collect::<Vec<_>>()
    );
    assert!(env
        .core
        .log_page(&id, 10, 4, None)
        .await
        .unwrap()
        .is_empty());
    // Búsqueda paginada.
    let hit = env
        .core
        .log_page(&id, 0, 3, Some("paso".into()))
        .await
        .unwrap();
    assert_eq!(hit.len(), 3);
    let rest = env
        .core
        .log_page(&id, 3, 20, Some("paso".into()))
        .await
        .unwrap();
    assert_eq!(rest.len(), 6);
    // Las refs y los padres vienen para pintar el grafo.
    assert!(
        all[0].refs.iter().any(|r| r.contains("main")),
        "{:?}",
        all[0].refs
    );
    assert_eq!(all[0].parents.len(), 1);
    assert!(all[9].parents.is_empty());
    // Repo sin commits: lista vacía, no error.
    let vacio = env.empty_repo("vacio");
    let vid = env.open(&vacio).await;
    assert!(env
        .core
        .log_page(&vid, 0, 10, None)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn el_diff_de_un_commit_da_sus_archivos() {
    use ariadna_git::DiffScope;
    let env = Env::new();
    let repo = env.repo("dc");
    env.write(&repo, "a.txt", b"uno\n");
    env.write(&repo, "b.txt", b"dos\n");
    env.git(&repo, &["add", "-A"]);
    env.git(&repo, &["commit", "-q", "-m", "dos archivos"]);
    let id = env.open(&repo).await;
    let head = env.core.log_page(&id, 0, 1, None).await.unwrap().remove(0);
    let files = env
        .core
        .diff(&id, DiffScope::Commit(head.sha.clone()), vec![])
        .await
        .unwrap();
    assert_eq!(files.len(), 2);
    // Un commit raíz también funciona.
    let root = env.core.log_page(&id, 1, 1, None).await.unwrap().remove(0);
    let files = env
        .core
        .diff(&id, DiffScope::Commit(root.sha), vec![])
        .await
        .unwrap();
    assert_eq!(files.len(), 1);
}

// --- ramas -----------------------------------------------------------------------------------

#[tokio::test]
async fn borrar_rama_pide_confirmacion_nativa_y_respeta_el_no() {
    let env = Env::new();
    let repo = env.repo("br");
    let id = env.open(&repo).await;
    env.core
        .create_branch(&id, "vieja".into(), None)
        .await
        .unwrap();

    env.prompter.reject_all();
    let d = env
        .core
        .delete_branch(&id, "vieja".into(), false)
        .await
        .unwrap();
    assert!(!d.done);
    let q = env.prompter.last();
    assert_eq!(q.title, "Borrar rama");
    assert!(
        q.message.contains("vieja") && q.message.contains("rama main"),
        "{q:?}"
    );
    assert_eq!(
        env.core.get_snapshot(&id).await.unwrap().0.branches.len(),
        2
    );

    env.prompter.accept_all();
    assert!(
        env.core
            .delete_branch(&id, "vieja".into(), false)
            .await
            .unwrap()
            .done
    );
    assert_eq!(
        env.core.get_snapshot(&id).await.unwrap().0.branches.len(),
        1
    );
}

#[tokio::test]
async fn borrar_una_rama_sin_fusionar_pide_una_segunda_confirmacion() {
    let env = Env::new();
    let repo = env.repo("brs");
    env.git(&repo, &["checkout", "-q", "-b", "trabajo"]);
    env.write(&repo, "t.txt", b"t\n");
    env.git(&repo, &["add", "-A"]);
    env.git(&repo, &["commit", "-q", "-m", "solo en trabajo"]);
    env.git(&repo, &["checkout", "-q", "main"]);
    let id = env.open(&repo).await;

    // Primer sí, segundo no: se conserva.
    env.prompter.reject_all();
    env.prompter.answers.lock().unwrap().extend([true, false]);
    let d = env
        .core
        .delete_branch(&id, "trabajo".into(), false)
        .await
        .unwrap();
    assert!(!d.done);
    assert_eq!(env.prompter.count(), 2);
    assert_eq!(
        env.core.get_snapshot(&id).await.unwrap().0.branches.len(),
        2
    );

    // Dos síes: se borra.
    env.prompter.answers.lock().unwrap().extend([true, true]);
    let d = env
        .core
        .delete_branch(&id, "trabajo".into(), false)
        .await
        .unwrap();
    assert!(d.done);
    assert_eq!(
        env.core.get_snapshot(&id).await.unwrap().0.branches.len(),
        1
    );

    // La rama actual no se puede borrar: error, no `done`.
    env.prompter.accept_all();
    let e = env
        .core
        .delete_branch(&id, "main".into(), false)
        .await
        .unwrap_err();
    assert_eq!(e.code, "COMMAND_FAILED", "{e:?}");
}

#[tokio::test]
async fn merge_limpio_funciona_y_con_conflictos_aborta_con_error_tipado() {
    let env = Env::new();
    let repo = env.repo("mg");
    env.write(&repo, "c.txt", b"a\nb\nc\n");
    env.git(&repo, &["add", "-A"]);
    env.git(&repo, &["commit", "-q", "-m", "base"]);
    env.git(&repo, &["checkout", "-q", "-b", "limpia"]);
    env.write(&repo, "otro.txt", b"o\n");
    env.git(&repo, &["add", "-A"]);
    env.git(&repo, &["commit", "-q", "-m", "limpia"]);
    env.git(&repo, &["checkout", "-q", "main"]);
    env.git(&repo, &["checkout", "-q", "-b", "choca"]);
    env.write(&repo, "c.txt", b"a\nB-choca\nc\n");
    env.git(&repo, &["commit", "-q", "-am", "choca"]);
    env.git(&repo, &["checkout", "-q", "main"]);
    env.write(&repo, "c.txt", b"a\nB-main\nc\n");
    env.git(&repo, &["commit", "-q", "-am", "main"]);
    let id = env.open(&repo).await;

    let ok = env
        .core
        .merge_clean(&id, "limpia".into(), MergeFastForward::Allow)
        .await
        .unwrap();
    assert!(matches!(ok, MergeResult::Merged { .. }), "{ok:?}");

    let head_before = env.git(&repo, &["rev-parse", "HEAD"]);
    let e = env
        .core
        .merge_clean(&id, "choca".into(), MergeFastForward::Allow)
        .await
        .unwrap_err();
    assert_eq!(e.code, "MERGE_CONFLICTS", "{e:?}");
    assert!(e.message.contains("choca") && e.detail.as_deref().unwrap_or("").contains("c.txt"));
    // Aborto limpio: sin merge en curso, mismo HEAD y archivo intacto.
    let snap = env.core.refresh(&id).await.unwrap().unwrap();
    assert_eq!(snap.0.integration, IntegrationState::None);
    assert_eq!(env.git(&repo, &["rev-parse", "HEAD"]), head_before);
    assert_eq!(
        std::fs::read(repo.join("c.txt")).unwrap(),
        b"a\nB-main\nc\n"
    );
    assert!(
        snap.0.status.entries.is_empty(),
        "{:?}",
        snap.0.status.entries
    );
    assert_eq!(env.prompter.count(), 0, "el aborto limpio no pregunta");
}
