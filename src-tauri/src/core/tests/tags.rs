//! Etiquetas en el núcleo: crear/listar con git real, borrados con confirmación nativa (y el
//! «no» del usuario no toca nada), publicar y borrar en el remoto, errores tipados.

use super::Env;
use crate::core::ApiError;

fn tag_names(env: &Env, repo: &std::path::Path) -> Vec<String> {
    env.git(repo, &["tag", "--list"])
        .lines()
        .map(str::to_owned)
        .collect()
}

#[tokio::test]
async fn crear_y_listar_etiquetas_ligeras_y_anotadas() {
    let env = Env::new();
    let repo = env.repo("tags");
    let id = env.open(&repo).await;
    let first = env.git(&repo, &["rev-parse", "HEAD"]).trim().to_owned();
    env.write(&repo, "a.txt", b"a\n");
    env.git(&repo, &["add", "-A"]);
    env.git(&repo, &["commit", "-q", "-m", "segundo"]);

    let light = env
        .core
        .create_tag(&id, "v1".into(), None, None)
        .await
        .unwrap();
    assert!(!light.annotated);
    let note = env
        .core
        .create_tag(
            &id,
            " v0 ".into(),
            Some(first.clone()),
            Some("el inicio".into()),
        )
        .await
        .unwrap();
    assert!(note.annotated);
    assert_eq!(note.name, "v0", "el nombre se recorta");
    assert_eq!(note.commit, first);
    assert_eq!(note.message.as_deref(), Some("el inicio"));
    // Crear no pide confirmación (no destruye nada).
    assert_eq!(env.prompter.count(), 0);

    let list = env.core.list_tags(&id).await.unwrap();
    assert_eq!(list.len(), 2);
    assert!(list.iter().any(|t| t.name == "v1" && !t.annotated));
    // El commit del historial lleva la etiqueta en sus refs.
    let page = env.core.log_page(&id, 0, 10, None).await.unwrap();
    assert!(
        page[0].refs.iter().any(|r| r == "tag: v1"),
        "{:?}",
        page[0].refs
    );
    assert!(
        page[1].refs.iter().any(|r| r == "tag: v0"),
        "{:?}",
        page[1].refs
    );
}

#[tokio::test]
async fn errores_tipados_al_crear() {
    let env = Env::new();
    let repo = env.repo("tags-err");
    let id = env.open(&repo).await;
    env.core
        .create_tag(&id, "v1".into(), None, None)
        .await
        .unwrap();
    let code = |e: ApiError| e.code;
    assert_eq!(
        code(
            env.core
                .create_tag(&id, "v1".into(), None, None)
                .await
                .unwrap_err()
        ),
        "TAG_EXISTS"
    );
    for bad in ["con espacio", "a..b", "-x", "a~", ""] {
        let e = env
            .core
            .create_tag(&id, bad.into(), None, None)
            .await
            .unwrap_err();
        assert_eq!(e.code, "INVALID_TAG_NAME", "{bad:?}");
        assert!(e.message.contains("no es válido"));
    }
    let e = env
        .core
        .create_tag(&id, "otra".into(), Some("no-existe".into()), None)
        .await
        .unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
    assert_eq!(tag_names(&env, &repo), vec!["v1"]);
}

#[tokio::test]
async fn borrar_etiqueta_local_pide_confirmacion_nativa_y_respeta_el_no() {
    let env = Env::new();
    let repo = env.repo("tags-del");
    let id = env.open(&repo).await;
    env.core
        .create_tag(&id, "v1".into(), None, Some("uno".into()))
        .await
        .unwrap();

    env.prompter.reject_all();
    let r = env.core.delete_tag(&id, "v1".into()).await.unwrap();
    assert!(!r.done);
    assert_eq!(tag_names(&env, &repo), vec!["v1"], "con «no» no se borra");
    let asked = env.prompter.last();
    assert_eq!(asked.title, "Borrar etiqueta");
    assert!(asked.message.contains("«v1»") && asked.message.contains("anotada"));
    assert!(asked.detail.contains("Etiqueta: v1"));

    env.prompter.accept_all();
    let r = env.core.delete_tag(&id, "v1".into()).await.unwrap();
    assert!(r.done);
    assert!(tag_names(&env, &repo).is_empty());

    // Una que ya no existe: error tipado y sin diálogo.
    let before = env.prompter.count();
    let e = env.core.delete_tag(&id, "v1".into()).await.unwrap_err();
    assert_eq!(e.code, "TAG_NOT_FOUND");
    assert_eq!(env.prompter.count(), before);
}

#[tokio::test]
async fn publicar_y_borrar_en_el_remoto() {
    let env = Env::new();
    let bare = env.bare("origen.git");
    let repo = env.repo("tags-remoto");
    env.git(&repo, &["remote", "add", "origin", &Env::s(&bare)]);
    let id = env.open(&repo).await;
    env.core
        .create_tag(&id, "v1".into(), None, None)
        .await
        .unwrap();
    assert!(env.git(&bare, &["tag", "--list"]).trim().is_empty());

    env.core
        .push_tag(&id, "origin".into(), "v1".into())
        .await
        .unwrap();
    assert_eq!(env.git(&bare, &["tag", "--list"]).trim(), "v1");
    assert_eq!(env.prompter.count(), 0, "publicar no pide confirmación");

    // «No» en el diálogo: sigue en el remoto.
    env.prompter.reject_all();
    let r = env
        .core
        .delete_remote_tag(&id, "origin".into(), "v1".into())
        .await
        .unwrap();
    assert!(!r.done);
    assert_eq!(env.git(&bare, &["tag", "--list"]).trim(), "v1");
    let asked = env.prompter.last();
    assert_eq!(asked.title, "Borrar etiqueta del remoto");
    assert!(asked.message.contains("«origin»") && asked.message.contains("«v1»"));

    env.prompter.accept_all();
    let r = env
        .core
        .delete_remote_tag(&id, "origin".into(), "v1".into())
        .await
        .unwrap();
    assert!(r.done);
    assert!(env.git(&bare, &["tag", "--list"]).trim().is_empty());
    assert_eq!(tag_names(&env, &repo), vec!["v1"], "la local no se toca");

    // Publicar algo que no existe o con un remoto inválido: errores tipados.
    let e = env
        .core
        .push_tag(&id, "origin".into(), "nada".into())
        .await
        .unwrap_err();
    assert_eq!(e.code, "TAG_NOT_FOUND");
    let e = env
        .core
        .push_tag(&id, "-x".into(), "v1".into())
        .await
        .unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
}

#[tokio::test]
async fn log_after_por_el_nucleo_continua_donde_acabo_la_pagina() {
    let env = Env::new();
    let repo = env.repo("cursor");
    for i in 0..12 {
        env.write(&repo, "f.txt", format!("{i}\n").as_bytes());
        env.git(&repo, &["add", "-A"]);
        env.git(&repo, &["commit", "-q", "-m", &format!("c{i}")]);
    }
    let id = env.open(&repo).await;
    let p1 = env.core.log_page(&id, 0, 5, None).await.unwrap();
    let cursor = ariadna_git::history_cursor(&p1);
    let p2 = env.core.log_after(&id, cursor, 5).await.unwrap();
    assert_eq!(p2, env.core.log_page(&id, 5, 5, None).await.unwrap());
    // Entradas no válidas no llegan a git.
    let e = env
        .core
        .log_after(&id, vec!["--all".into()], 5)
        .await
        .unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
}
