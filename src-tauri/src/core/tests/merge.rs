//! Merge en curso y resolución de conflictos con git real (`merge_ops`).

use std::path::Path;

use ariadna_git::{ConflictChoice, ConflictKind, IntegrationState, MergeFastForward, MergeResult};

use super::Env;
use crate::core::{ConflictResolution, HunkChoice, HunkSide};

enum Change<'a> {
    Write(&'a str, &'a [u8]),
    Delete(&'a str),
}

fn apply(env: &Env, repo: &Path, changes: &[Change<'_>], msg: &str) {
    for c in changes {
        match c {
            Change::Write(p, b) => env.write(repo, p, b),
            Change::Delete(p) => {
                std::fs::remove_file(repo.join(p)).unwrap();
            }
        }
    }
    env.git(repo, &["add", "-A"]);
    env.git(repo, &["commit", "-q", "-m", msg]);
}

/// `main` y `feature` parten de `base`; el repo queda en `main`, listo para `merge feature`.
async fn setup(
    env: &Env,
    name: &str,
    base: &[Change<'_>],
    feature: &[Change<'_>],
    main: &[Change<'_>],
) -> (std::path::PathBuf, String) {
    let repo = env.repo(name);
    apply(env, &repo, base, "base");
    env.git(&repo, &["checkout", "-q", "-b", "feature"]);
    apply(env, &repo, feature, "feature");
    env.git(&repo, &["checkout", "-q", "main"]);
    apply(env, &repo, main, "main");
    let id = env.open(&repo).await;
    (repo, id)
}

fn status(env: &Env, repo: &Path) -> String {
    env.git(repo, &["status", "--porcelain=v1", "-z"])
}

const PRE: &[u8] = b"uno\ndos\ntres\n";

async fn uu(env: &Env) -> (std::path::PathBuf, String) {
    setup(
        env,
        "uu",
        &[Change::Write("c.txt", PRE)],
        &[Change::Write("c.txt", b"uno\nDOS-feature\ntres\n")],
        &[Change::Write("c.txt", b"uno\nDOS-main\ntres\n")],
    )
    .await
}

#[tokio::test]
async fn merge_start_deja_el_merge_en_curso_con_el_estado_completo() {
    let env = Env::new();
    let (repo, id) = uu(&env).await;
    let r = env
        .core
        .merge_start(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap();
    assert!(matches!(r.result, MergeResult::Conflicts { .. }), "{r:?}");
    let st = r.state.expect("estado con conflictos");
    assert_eq!(st.integration, IntegrationState::Merging);
    assert_eq!(st.source_label.as_deref(), Some("feature"));
    assert_eq!(
        st.other_head.as_deref(),
        Some(env.git(&repo, &["rev-parse", "feature"]).trim())
    );
    assert!(st.message.starts_with("Merge branch 'feature'"), "{st:?}");
    assert!(!st.message.contains('#'), "sin comentarios de git");
    assert!(!st.can_continue);
    assert_eq!(st.conflicts.len(), 1);
    let c = &st.conflicts[0];
    assert_eq!(
        (c.path.to_string_lossy().as_ref(), c.kind),
        ("c.txt", ConflictKind::BothModified)
    );
    assert!(c.ours_present && c.base_present && c.theirs_present);
    assert_eq!(c.hunk_count, 1);
    assert!(c.text_safe && !c.binary);

    // Sin conflictos: no hay estado.
    let env2 = Env::new();
    let repo2 = env2.repo("limpio");
    env2.git(&repo2, &["checkout", "-q", "-b", "otra"]);
    env2.write(&repo2, "o.txt", b"o\n");
    env2.git(&repo2, &["add", "-A"]);
    env2.git(&repo2, &["commit", "-q", "-m", "otra"]);
    env2.git(&repo2, &["checkout", "-q", "main"]);
    env2.write(&repo2, "m.txt", b"m\n");
    env2.git(&repo2, &["add", "-A"]);
    env2.git(&repo2, &["commit", "-q", "-m", "main"]);
    let id2 = env2.open(&repo2).await;
    let r = env2
        .core
        .merge_start(&id2, "otra".into(), MergeFastForward::Allow)
        .await
        .unwrap();
    assert!(matches!(r.result, MergeResult::Merged { .. }), "{r:?}");
    assert!(r.state.is_none());
    let st = env2.core.merge_state(&id2).await.unwrap();
    assert_eq!(st.integration, IntegrationState::None);
    assert!(!st.can_continue);
}

#[tokio::test]
async fn resolver_por_bloque_y_continuar_crea_el_commit_de_merge() {
    let env = Env::new();
    let (repo, id) = uu(&env).await;
    env.core
        .merge_start(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap();

    // Continuar con conflictos pendientes: error tipado, nada cambia.
    let e = env.core.merge_continue(&id, None).await.unwrap_err();
    assert_eq!(e.code, "CONFLICTS_PENDING");
    assert_eq!(e.detail.as_deref(), Some("c.txt"));

    // «Ambos» por bloque.
    let r = env
        .core
        .resolve_conflict(
            &id,
            "c.txt".into(),
            ConflictResolution::Hunks(vec![HunkChoice {
                id: 0,
                choice: HunkSide::Both,
            }]),
        )
        .await
        .unwrap();
    assert!(r.done);
    assert_eq!(
        std::fs::read(repo.join("c.txt")).unwrap(),
        b"uno\nDOS-main\nDOS-feature\ntres\n"
    );
    // Marcado como resuelto (git add): ya no hay rutas sin fusionar.
    assert_eq!(env.git(&repo, &["ls-files", "-u"]).trim(), "");
    let st = env.core.merge_state(&id).await.unwrap();
    assert!(st.can_continue && st.conflicts.is_empty());

    let done = env
        .core
        .merge_continue(&id, Some("Fusiona feature (revisado)\n\nDetalle.".into()))
        .await
        .unwrap();
    assert!(done.done && done.integration == IntegrationState::None);
    let sha = done.sha.unwrap();
    assert_eq!(sha, env.git(&repo, &["rev-parse", "HEAD"]).trim());
    assert_eq!(
        env.git(&repo, &["log", "-1", "--format=%P"])
            .split_whitespace()
            .count(),
        2
    );
    assert_eq!(
        env.git(&repo, &["log", "-1", "--format=%s"]).trim(),
        "Fusiona feature (revisado)"
    );
    assert_eq!(status(&env, &repo), "");
}

#[tokio::test]
async fn continuar_sin_mensaje_usa_el_de_git_y_resolver_exige_confirmacion() {
    let env = Env::new();
    let (repo, id) = uu(&env).await;
    env.core
        .merge_start(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap();
    let marked = std::fs::read(repo.join("c.txt")).unwrap();

    env.prompter.reject_all();
    let r = env
        .core
        .resolve_conflict(
            &id,
            "c.txt".into(),
            ConflictResolution::Side(ConflictChoice::Theirs),
        )
        .await
        .unwrap();
    assert!(!r.done);
    assert_eq!(
        std::fs::read(repo.join("c.txt")).unwrap(),
        marked,
        "intacto"
    );
    assert!(env.prompter.last().detail.contains("c.txt"));
    assert!(env.git(&repo, &["ls-files", "-u"]).contains("c.txt"));

    env.prompter.accept_all();
    let r = env
        .core
        .resolve_conflict(
            &id,
            "c.txt".into(),
            ConflictResolution::Side(ConflictChoice::Theirs),
        )
        .await
        .unwrap();
    assert!(r.done);
    assert_eq!(
        std::fs::read(repo.join("c.txt")).unwrap(),
        b"uno\nDOS-feature\ntres\n"
    );
    let done = env.core.merge_continue(&id, None).await.unwrap();
    assert!(done.sha.is_none() && done.integration == IntegrationState::None);
    assert!(env
        .git(&repo, &["log", "-1", "--format=%s"])
        .starts_with("Merge branch 'feature'"));
}

#[tokio::test]
async fn texto_manual_con_marcadores_se_rechaza_y_sin_marcadores_se_acepta() {
    let env = Env::new();
    let (repo, id) = uu(&env).await;
    env.core
        .merge_start(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap();
    let marked = std::fs::read(repo.join("c.txt")).unwrap();
    let before = env.prompter.count();
    let e = env
        .core
        .resolve_conflict(
            &id,
            "c.txt".into(),
            ConflictResolution::Manual(marked.clone()),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code, "CONFLICT_MARKERS_REMAIN");
    assert_eq!(env.prompter.count(), before, "sin diálogo ni escritura");
    assert_eq!(std::fs::read(repo.join("c.txt")).unwrap(), marked);

    // Bloques sin cubrir: también se rechaza.
    let e = env
        .core
        .resolve_conflict(&id, "c.txt".into(), ConflictResolution::Hunks(vec![]))
        .await
        .unwrap_err();
    assert_eq!(e.code, "CONFLICT_MARKERS_REMAIN");

    // Archivo que no está en conflicto.
    let e = env
        .core
        .resolve_conflict(
            &id,
            "README.md".into(),
            ConflictResolution::Side(ConflictChoice::Ours),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code, "CONFLICT_NOT_FOUND");

    let r = env
        .core
        .resolve_conflict(
            &id,
            "c.txt".into(),
            ConflictResolution::Manual("uno\ndos combinado ✓\ntres\n".as_bytes().to_vec()),
        )
        .await
        .unwrap();
    assert!(r.done);
    assert_eq!(
        std::fs::read_to_string(repo.join("c.txt")).unwrap(),
        "uno\ndos combinado ✓\ntres\n"
    );
    env.core.merge_continue(&id, None).await.unwrap();
}

#[tokio::test]
async fn abortar_deja_el_arbol_como_antes_del_merge() {
    let env = Env::new();
    let (repo, id) = uu(&env).await;
    let head = env.git(&repo, &["rev-parse", "HEAD"]);
    let before_file = std::fs::read(repo.join("c.txt")).unwrap();
    env.core
        .merge_start(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap();
    // Resuelve algo antes de abortar: se pierde (por eso la confirmación).
    env.core
        .resolve_conflict(
            &id,
            "c.txt".into(),
            ConflictResolution::Side(ConflictChoice::Theirs),
        )
        .await
        .unwrap();

    env.prompter.reject_all();
    let r = env.core.merge_abort(&id).await.unwrap();
    assert!(!r.done);
    assert!(repo.join(".git/MERGE_HEAD").exists());

    env.prompter.accept_all();
    let r = env.core.merge_abort(&id).await.unwrap();
    assert!(r.done);
    assert!(!repo.join(".git/MERGE_HEAD").exists());
    assert_eq!(env.git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(std::fs::read(repo.join("c.txt")).unwrap(), before_file);
    assert_eq!(status(&env, &repo), "");
    let st = env.core.merge_state(&id).await.unwrap();
    assert_eq!(st.integration, IntegrationState::None);
    // Sin merge: error tipado.
    let e = env.core.merge_abort(&id).await.unwrap_err();
    assert_eq!(e.code, "NO_INTEGRATION");
}

#[tokio::test]
async fn ambos_anadieron_el_mismo_archivo_aa() {
    let env = Env::new();
    let (repo, id) = setup(
        &env,
        "aa",
        &[Change::Write(
            "otro.txt", b"x
",
        )],
        &[Change::Write("nuevo.txt", b"desde feature\n")],
        &[Change::Write("nuevo.txt", b"desde main\n")],
    )
    .await;
    let r = env
        .core
        .merge_start(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap();
    let st = r.state.unwrap();
    assert_eq!(st.conflicts[0].kind, ConflictKind::BothAdded);
    assert!(!st.conflicts[0].base_present);
    // Sin base: elegir la base es un error tipado, no un borrado.
    let e = env
        .core
        .resolve_conflict(
            &id,
            "nuevo.txt".into(),
            ConflictResolution::Side(ConflictChoice::Base),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code, "CONFLICT_NOT_FOUND");
    env.core
        .resolve_conflict(
            &id,
            "nuevo.txt".into(),
            ConflictResolution::Side(ConflictChoice::Ours),
        )
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(repo.join("nuevo.txt")).unwrap(),
        b"desde main\n"
    );
    env.core.merge_continue(&id, None).await.unwrap();
    assert_eq!(status(&env, &repo), "");
}

#[tokio::test]
async fn borrado_contra_modificado_se_resuelve_borrando_o_conservando() {
    // main modifica, feature borra → «borrado por ellos».
    let env = Env::new();
    let (repo, id) = setup(
        &env,
        "ud",
        &[Change::Write("f.txt", PRE), Change::Write("k.txt", b"k\n")],
        &[Change::Delete("f.txt")],
        &[Change::Write("f.txt", b"uno\nmodificado\ntres\n")],
    )
    .await;
    let st = env
        .core
        .merge_start(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap()
        .state
        .unwrap();
    assert_eq!(st.conflicts[0].kind, ConflictKind::DeletedByThem);
    assert!(!st.conflicts[0].theirs_present && st.conflicts[0].ours_present);
    // Quedarse con «lo suyo» (borrado) → el archivo desaparece del árbol y del índice.
    let r = env
        .core
        .resolve_conflict(
            &id,
            "f.txt".into(),
            ConflictResolution::Side(ConflictChoice::Theirs),
        )
        .await
        .unwrap();
    assert!(r.done);
    assert!(!repo.join("f.txt").exists());
    assert_eq!(env.git(&repo, &["ls-files", "f.txt"]).trim(), "");
    assert!(env.prompter.last().message.contains("se borrará"));
    env.core.merge_continue(&id, None).await.unwrap();
    assert!(!env
        .git(&repo, &["ls-tree", "-r", "--name-only", "HEAD"])
        .contains("f.txt"));

    // main borra, feature modifica → «borrado por nosotros»; conservar lo suyo.
    let env = Env::new();
    let (repo, id) = setup(
        &env,
        "du",
        &[Change::Write("f.txt", PRE)],
        &[Change::Write("f.txt", b"uno\nde feature\ntres\n")],
        &[Change::Delete("f.txt")],
    )
    .await;
    let st = env
        .core
        .merge_start(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap()
        .state
        .unwrap();
    assert_eq!(st.conflicts[0].kind, ConflictKind::DeletedByUs);
    env.core
        .resolve_conflict(
            &id,
            "f.txt".into(),
            ConflictResolution::Side(ConflictChoice::Theirs),
        )
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(repo.join("f.txt")).unwrap(),
        b"uno\nde feature\ntres\n"
    );
    env.core.merge_continue(&id, None).await.unwrap();
    assert_eq!(status(&env, &repo), "");
}

#[tokio::test]
async fn borrado_en_ambos_lados_dd_se_resuelve_borrando() {
    let env = Env::new();
    let (repo, id) = setup(
        &env,
        "dd",
        &[Change::Write("a.txt", PRE)],
        &[Change::Delete("a.txt")],
        &[Change::Delete("a.txt")],
    )
    .await;
    // Dos borrados idénticos se fusionan solos: sin conflicto.
    let r = env
        .core
        .merge_start(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap();
    assert!(r.state.is_none(), "{r:?}");
    assert!(!repo.join("a.txt").exists());
}

#[tokio::test]
async fn binarios_solo_admiten_versiones_enteras() {
    let env = Env::new();
    let bin = |n: u8| vec![0u8, 1, 2, n, 0, 255];
    let (repo, id) = setup(
        &env,
        "bin",
        &[Change::Write("img.bin", &bin(0))],
        &[Change::Write("img.bin", &bin(3))],
        &[Change::Write("img.bin", &bin(2))],
    )
    .await;
    let st = env
        .core
        .merge_start(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap()
        .state
        .unwrap();
    assert!(st.conflicts[0].binary && !st.conflicts[0].text_safe);
    assert_eq!(st.conflicts[0].hunk_count, 0);
    let e = env
        .core
        .resolve_conflict(
            &id,
            "img.bin".into(),
            ConflictResolution::Manual(vec![1, 2]),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code, "CONFLICT_NOT_TEXT");
    let e = env
        .core
        .resolve_conflict(&id, "img.bin".into(), ConflictResolution::Hunks(vec![]))
        .await
        .unwrap_err();
    assert_eq!(e.code, "CONFLICT_NOT_TEXT");
    env.core
        .resolve_conflict(
            &id,
            "img.bin".into(),
            ConflictResolution::Side(ConflictChoice::Theirs),
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(repo.join("img.bin")).unwrap(), bin(3));
    env.core.merge_continue(&id, None).await.unwrap();
}

#[tokio::test]
async fn crlf_se_conserva_byte_a_byte() {
    let env = Env::new();
    let (repo, id) = setup(
        &env,
        "crlf",
        &[Change::Write("w.txt", b"a\r\nb\r\nc\r\n")],
        &[Change::Write("w.txt", b"a\r\nB-feature\r\nc\r\n")],
        &[Change::Write("w.txt", b"a\r\nB-main\r\nc\r\n")],
    )
    .await;
    env.core
        .merge_start(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap();
    let files = env.core.get_conflicts(&id).await.unwrap();
    assert_eq!(files[0].hunks.len(), 1);
    assert_eq!(files[0].hunks[0].ours, b"B-main\r\n");
    env.core
        .resolve_conflict(
            &id,
            "w.txt".into(),
            ConflictResolution::Hunks(vec![HunkChoice {
                id: 0,
                choice: HunkSide::Theirs,
            }]),
        )
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(repo.join("w.txt")).unwrap(),
        b"a\r\nB-feature\r\nc\r\n"
    );
    env.core.merge_continue(&id, None).await.unwrap();
    assert_eq!(
        env.git(&repo, &["show", "HEAD:w.txt"]),
        "a\r\nB-feature\r\nc\r\n"
    );
}

#[tokio::test]
async fn ruta_con_espacios_y_unicode() {
    let env = Env::new();
    let path = "carpeta ñandú/archivo con espacios ✓.txt";
    let (repo, id) = setup(
        &env,
        "uni",
        &[Change::Write(path, PRE)],
        &[Change::Write(path, "uno\nDOS ✓ feature\ntres\n".as_bytes())],
        &[Change::Write(path, "uno\nDOS ñ main\ntres\n".as_bytes())],
    )
    .await;
    let st = env
        .core
        .merge_start(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap()
        .state
        .unwrap();
    assert_eq!(st.conflicts[0].path.to_string_lossy(), path);
    assert!(st.conflicts[0].text_safe);
    env.core
        .resolve_conflict(
            &id,
            path.into(),
            ConflictResolution::Manual("uno\nDOS ñ + ✓\ntres\n".as_bytes().to_vec()),
        )
        .await
        .unwrap();
    env.core.merge_continue(&id, None).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.join(path)).unwrap(),
        "uno\nDOS ñ + ✓\ntres\n"
    );
    assert_eq!(status(&env, &repo), "");
}

#[tokio::test]
async fn varios_archivos_y_continuar_parcial() {
    let env = Env::new();
    let (repo, id) = setup(
        &env,
        "multi",
        &[Change::Write("a.txt", PRE), Change::Write("b.txt", PRE)],
        &[
            Change::Write("a.txt", b"uno\nA-f\ntres\n"),
            Change::Write("b.txt", b"uno\nB-f\ntres\n"),
        ],
        &[
            Change::Write("a.txt", b"uno\nA-m\ntres\n"),
            Change::Write("b.txt", b"uno\nB-m\ntres\n"),
        ],
    )
    .await;
    let st = env
        .core
        .merge_start(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap()
        .state
        .unwrap();
    assert_eq!(st.conflicts.len(), 2);
    env.core
        .resolve_conflict(
            &id,
            "a.txt".into(),
            ConflictResolution::Side(ConflictChoice::Ours),
        )
        .await
        .unwrap();
    let st = env.core.merge_state(&id).await.unwrap();
    assert_eq!(st.conflicts.len(), 1);
    assert!(!st.can_continue);
    let e = env.core.merge_continue(&id, None).await.unwrap_err();
    assert_eq!(e.code, "CONFLICTS_PENDING");
    assert_eq!(e.detail.as_deref(), Some("b.txt"));
    assert!(repo.join(".git/MERGE_HEAD").exists());
}

#[tokio::test]
async fn cherry_pick_en_curso_se_detecta_continua_y_no_se_aborta_como_merge() {
    let env = Env::new();
    let (repo, id) = uu(&env).await;
    let out = env
        .command(&repo)
        .args(["cherry-pick", "feature"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "debe chocar");
    let st = env.core.merge_state(&id).await.unwrap();
    assert_eq!(st.integration, IntegrationState::CherryPicking);
    assert_eq!(st.conflicts.len(), 1);
    assert_eq!(
        st.other_head.as_deref(),
        Some(env.git(&repo, &["rev-parse", "feature"]).trim())
    );
    assert_eq!(st.message.trim(), "feature");

    // `merge_abort` es solo para merges: no toca el cherry-pick.
    let before = env.prompter.count();
    let e = env.core.merge_abort(&id).await.unwrap_err();
    assert_eq!(e.code, "NO_INTEGRATION");
    assert_eq!(env.prompter.count(), before);
    assert!(repo.join(".git/CHERRY_PICK_HEAD").exists());

    env.core
        .resolve_conflict(
            &id,
            "c.txt".into(),
            ConflictResolution::Side(ConflictChoice::Theirs),
        )
        .await
        .unwrap();
    let done = env.core.merge_continue(&id, None).await.unwrap();
    assert_eq!(done.integration, IntegrationState::None);
    assert_eq!(
        env.git(&repo, &["log", "-1", "--format=%s"]).trim(),
        "feature"
    );
    assert_eq!(status(&env, &repo), "");
}

#[tokio::test]
async fn cherry_pick_se_puede_abortar_con_abort_integration() {
    let env = Env::new();
    let (repo, id) = uu(&env).await;
    let head = env.git(&repo, &["rev-parse", "HEAD"]);
    let out = env
        .command(&repo)
        .args(["cherry-pick", "feature"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(env.core.abort_integration(&id).await.unwrap().done);
    assert_eq!(env.git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(status(&env, &repo), "");
}
