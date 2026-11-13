//! Líneas sueltas y stash a través de `AppCore`: serialización del hunk, confirmación nativa
//! y códigos de error.

use ariadna_git::DiffScope;

use super::*;
use crate::core::HunkArg;

fn numbered(n: usize) -> String {
    (1..=n).map(|i| format!("l{i}\n")).collect()
}

fn hunk_arg(h: &ariadna_git::DiffHunk) -> HunkArg {
    serde_json::from_value(serde_json::to_value(h).unwrap()).unwrap()
}

#[tokio::test]
async fn preparar_y_quitar_lineas_sueltas_como_lo_hace_la_ui() {
    let env = Env::new();
    let repo = env.repo("lineas");
    env.write(&repo, "f.txt", numbered(12).as_bytes());
    env.git(&repo, &["add", "-A"]);
    env.git(&repo, &["commit", "-q", "-m", "base f"]);
    let id = env.open(&repo).await;
    let changed = numbered(12).replace("l4\n", "L4\n").replace("l8\n", "L8\n");
    env.write(&repo, "f.txt", changed.as_bytes());

    let diffs = env
        .core
        .diff(&id, DiffScope::Unstaged, vec![])
        .await
        .unwrap();
    let hunk = &diffs[0].hunks[0];
    let idx = |kind, text: &str| -> u32 {
        hunk.lines
            .iter()
            .position(|l| l.kind == kind && l.text == text)
            .unwrap() as u32
    };
    let lines = vec![
        idx(ariadna_git::DiffLineKind::Deletion, "l8"),
        idx(ariadna_git::DiffLineKind::Addition, "L8"),
    ];
    env.core
        .stage_lines(&id, "f.txt".into(), hunk_arg(hunk), lines)
        .await
        .unwrap();
    assert_eq!(
        env.git(&repo, &["show", ":f.txt"]),
        numbered(12).replace("l8\n", "L8\n")
    );

    let staged = env.core.diff(&id, DiffScope::Staged, vec![]).await.unwrap();
    let shunk = &staged[0].hunks[0];
    let lines: Vec<u32> = shunk
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.text == "L8")
        .map(|(i, _)| i as u32)
        .collect();
    env.core
        .unstage_lines(&id, "f.txt".into(), hunk_arg(shunk), lines)
        .await
        .unwrap();
    // Se quitó solo `L8`: `l8` sigue borrada en el índice.
    assert_eq!(
        env.git(&repo, &["show", ":f.txt"]),
        numbered(12).replace("l8\n", "")
    );

    // Sin líneas: rechazo claro y sin tocar el índice.
    let diffs = env
        .core
        .diff(&id, DiffScope::Unstaged, vec![])
        .await
        .unwrap();
    let e = env
        .core
        .stage_lines(&id, "f.txt".into(), hunk_arg(&diffs[0].hunks[0]), vec![])
        .await
        .unwrap_err();
    assert_eq!(e.code, "PATCH_REJECTED");
    assert!(e.message.contains("No se pueden preparar"), "{e:?}");
}

#[tokio::test]
async fn stash_completo_con_confirmacion_al_borrar() {
    let env = Env::new();
    let repo = env.repo("stash");
    let id = env.open(&repo).await;
    assert!(env.core.stash_list(&id).await.unwrap().is_empty());

    env.write(&repo, "README.md", b"cambiado\n");
    env.write(&repo, "nuevo.txt", b"n\n");
    let e = env.core.stash_push(&id, None, false, false).await.unwrap();
    assert!(e.auto_message);
    // El archivo sin seguimiento no se guardó: sigue ahí.
    assert!(repo.join("nuevo.txt").exists());
    env.write(&repo, "otro.txt", b"o\n");
    let second = env
        .core
        .stash_push(&id, Some("con nuevos".into()), true, false)
        .await
        .unwrap();
    assert_eq!(second.message, "con nuevos");
    assert!(!repo.join("nuevo.txt").exists());
    let list = env.core.stash_list(&id).await.unwrap();
    assert_eq!(list.len(), 2);

    // Ver el diff no pregunta nada.
    let before = env.prompter.count();
    let diff = env.core.stash_diff(&id, second.oid.clone()).await.unwrap();
    assert!(diff.iter().any(|f| f.path == *"nuevo.txt"));
    assert_eq!(env.prompter.count(), before);

    // Borrar: cancelar no toca nada; el diálogo nombra repo, rama y mensaje.
    env.prompter.reject_all();
    let r = env.core.stash_drop(&id, second.oid.clone()).await.unwrap();
    assert!(!r.done);
    assert_eq!(env.core.stash_list(&id).await.unwrap().len(), 2);
    let q = env.prompter.last();
    assert_eq!(q.title, "Borrar el stash");
    assert!(q.message.contains("rama main"), "{q:?}");
    assert!(
        q.detail.contains("con nuevos") && q.detail.contains("stash@{0}"),
        "{q:?}"
    );

    env.prompter.accept_all();
    let r = env.core.stash_drop(&id, second.oid.clone()).await.unwrap();
    assert!(r.done);
    let list = env.core.stash_list(&id).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].oid, e.oid);

    // Un stash que ya no existe, o un id que no es un SHA, se rechaza sin preguntar.
    let asked = env.prompter.count();
    let err = env.core.stash_drop(&id, second.oid).await.unwrap_err();
    assert_eq!(err.code, "STASH_NOT_FOUND");
    let err = env.core.stash_drop(&id, "--all".into()).await.unwrap_err();
    assert_eq!(err.code, "INVALID_REFERENCE");
    assert_eq!(env.prompter.count(), asked);

    // Sacar el que queda deja el archivo modificado y la lista vacía.
    let res = env.core.stash_pop(&id, e.oid).await.unwrap();
    assert!(res.dropped && res.conflicts.is_empty());
    assert_eq!(
        std::fs::read(repo.join("README.md")).unwrap(),
        b"cambiado\n"
    );
    assert!(env.core.stash_list(&id).await.unwrap().is_empty());

    // Sin cambios que guardar: código propio.
    env.git(&repo, &["checkout", "--", "README.md"]);
    std::fs::remove_file(repo.join("otro.txt")).ok();
    let err = env
        .core
        .stash_push(&id, None, false, false)
        .await
        .unwrap_err();
    assert_eq!(err.code, "NOTHING_TO_STASH");
}

#[tokio::test]
async fn pop_con_conflictos_conserva_el_stash_y_deja_los_conflictos_en_el_snapshot() {
    let env = Env::new();
    let repo = env.repo("stash-conf");
    let id = env.open(&repo).await;
    env.write(&repo, "README.md", b"mi version\n");
    let entry = env.core.stash_push(&id, None, false, false).await.unwrap();
    env.write(&repo, "README.md", b"otra version\n");
    env.git(&repo, &["commit", "-qam", "otra"]);

    let res = env.core.stash_pop(&id, entry.oid).await.unwrap();
    assert!(!res.dropped);
    assert_eq!(res.conflicts.len(), 1);
    assert_eq!(env.core.stash_list(&id).await.unwrap().len(), 1);
    let snap = env.core.get_snapshot(&id).await.unwrap();
    assert_eq!(snap.0.conflicts.len(), 1);
    assert_eq!(snap.0.conflicts[0].path, "README.md");
}

#[tokio::test]
async fn la_forma_serializada_del_stash_es_la_del_contrato() {
    let env = Env::new();
    let repo = env.repo("stash-json");
    let id = env.open(&repo).await;
    env.write(&repo, "README.md", b"x\n");
    let entry = env
        .core
        .stash_push(&id, Some("m".into()), false, false)
        .await
        .unwrap();
    let v = serde_json::to_value(&entry).unwrap();
    let mut keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "autoMessage",
            "branch",
            "createdAtIso",
            "index",
            "message",
            "oid"
        ]
    );
    assert_eq!(v["branch"], "main");
    let res = env.core.stash_apply(&id, entry.oid).await.unwrap();
    let v = serde_json::to_value(&res).unwrap();
    assert_eq!(v, serde_json::json!({ "conflicts": [], "dropped": false }));
}
