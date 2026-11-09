//! Comandos destructivos (confirmación nativa) y flujos de los comandos de pestaña.

use ariadna_git::{DiffScope, IntegrationState, MergeFastForward, MergeResult};

use super::*;
use crate::core::HunkArg;

#[tokio::test]
async fn discard_sin_confirmacion_no_toca_nada() {
    let env = Env::new();
    let repo = env.repo("d");
    let id = env.open(&repo).await;
    env.write(&repo, "README.md", b"cambiado\n");

    env.prompter.reject_all();
    let r = env
        .core
        .discard(&id, vec!["README.md".into()])
        .await
        .unwrap();
    assert!(!r.done);
    assert_eq!(
        std::fs::read(repo.join("README.md")).unwrap(),
        b"cambiado\n"
    );
    assert_eq!(env.prompter.count(), 1);
    let q = env.prompter.last();
    // El diálogo nombra repo, rama y ruta.
    assert!(
        q.message.contains("d") && q.message.contains("rama main"),
        "{q:?}"
    );
    assert!(q.detail.contains("README.md"), "{q:?}");
    assert_eq!(q.title, "Descartar cambios");
}

#[tokio::test]
async fn discard_con_confirmacion_restaura_el_archivo() {
    let env = Env::new();
    let repo = env.repo("d2");
    let id = env.open(&repo).await;
    env.write(&repo, "README.md", b"cambiado\n");
    env.prompter.accept_all();
    let r = env
        .core
        .discard(&id, vec!["README.md".into()])
        .await
        .unwrap();
    assert!(r.done);
    assert_eq!(std::fs::read(repo.join("README.md")).unwrap(), b"hola\n");
    // Sin rutas no hay nada que confirmar.
    let before = env.prompter.count();
    assert!(env.core.discard(&id, vec![]).await.unwrap().done);
    assert_eq!(env.prompter.count(), before);
}

#[tokio::test]
async fn commit_en_detached_con_y_sin_confirmacion() {
    let env = Env::new();
    let repo = env.repo("det");
    env.write(&repo, "otro.txt", b"2\n");
    env.git(&repo, &["add", "-A"]);
    env.git(&repo, &["commit", "-q", "-m", "segundo"]);
    env.git(&repo, &["checkout", "-q", "--detach", "HEAD~1"]);
    let head = env.git(&repo, &["rev-parse", "HEAD"]);
    let id = env.open(&repo).await;
    assert!(env.core.app_state().tabs[0].detached);
    env.write(&repo, "nuevo.txt", b"n\n");
    env.core.stage(&id, vec!["nuevo.txt".into()]).await.unwrap();

    // Rechazado: no hay commit.
    env.prompter.reject_all();
    let r = env.core.commit(&id, "suelto".into(), false).await.unwrap();
    assert!(!r.done && r.sha.is_none());
    assert_eq!(env.prompter.count(), 1);
    assert!(env.prompter.last().message.contains("HEAD"));
    assert_eq!(env.git(&repo, &["rev-parse", "HEAD"]), head);

    // Aceptado: hace el commit.
    env.prompter.accept_all();
    let r = env.core.commit(&id, "suelto".into(), false).await.unwrap();
    assert!(r.done);
    let sha = r.sha.unwrap();
    assert_eq!(env.git(&repo, &["rev-parse", "HEAD"]).trim(), sha);
    assert_eq!(env.prompter.count(), 2);

    // Con allowDetached=true no se pregunta.
    env.write(&repo, "otro2.txt", b"n\n");
    env.core.stage(&id, vec!["otro2.txt".into()]).await.unwrap();
    let r = env.core.commit(&id, "otro".into(), true).await.unwrap();
    assert!(r.done);
    assert_eq!(env.prompter.count(), 2);
}

#[tokio::test]
async fn commit_normal_y_errores_de_commit() {
    let env = Env::new();
    let repo = env.repo("c");
    let id = env.open(&repo).await;
    let e = env
        .core
        .commit(&id, "vacío".into(), false)
        .await
        .unwrap_err();
    assert_eq!(e.code, "NOTHING_TO_COMMIT");
    env.write(&repo, "x.txt", b"1\n");
    env.core.stage(&id, vec!["x.txt".into()]).await.unwrap();
    let e = env.core.commit(&id, "  ".into(), false).await.unwrap_err();
    assert_eq!(e.code, "EMPTY_COMMIT_MESSAGE");
    let r = env.core.commit(&id, "ok".into(), false).await.unwrap();
    assert!(r.done && r.sha.is_some());
    assert_eq!(env.prompter.count(), 0);
}

/// Repo con `origin` (bare) y una rama local `feature` que NO coincide con `origin/feature`.
fn repo_con_rama_divergente(env: &Env) -> std::path::PathBuf {
    let bare = env.bare("origin.git");
    let repo = env.repo("div");
    env.git(&repo, &["remote", "add", "origin", &Env::s(&bare)]);
    env.git(&repo, &["push", "-q", "-u", "origin", "main"]);
    env.git(&repo, &["branch", "feature"]);
    env.write(&repo, "mas.txt", b"mas\n");
    env.git(&repo, &["add", "-A"]);
    env.git(&repo, &["commit", "-q", "-m", "avanza main"]);
    env.git(&repo, &["push", "-q", "origin", "main:feature"]);
    env.git(&repo, &["fetch", "-q", "origin"]);
    repo
}

#[tokio::test]
async fn checkout_divergente_pregunta_y_respeta_el_no() {
    let env = Env::new();
    let repo = repo_con_rama_divergente(&env);
    let id = env.open(&repo).await;

    env.prompter.reject_all();
    let r = env
        .core
        .checkout(&id, "origin/feature".into())
        .await
        .unwrap();
    assert!(!r.done);
    assert_eq!(env.prompter.count(), 1);
    let q = env.prompter.last();
    assert!(
        q.message.contains("feature") && q.message.contains("origin/feature"),
        "{q:?}"
    );
    assert_eq!(env.git(&repo, &["branch", "--show-current"]).trim(), "main");

    env.prompter.accept_all();
    let r = env
        .core
        .checkout(&id, "origin/feature".into())
        .await
        .unwrap();
    assert!(r.done);
    assert_eq!(
        env.git(&repo, &["branch", "--show-current"]).trim(),
        "feature"
    );
}

#[tokio::test]
async fn checkout_normal_no_pregunta_y_los_errores_se_propagan() {
    let env = Env::new();
    let repo = env.repo("co");
    let id = env.open(&repo).await;
    env.core
        .create_branch(&id, "otra".into(), None)
        .await
        .unwrap();
    assert!(env.core.checkout(&id, "otra".into()).await.unwrap().done);
    assert_eq!(env.prompter.count(), 0);
    let e = env.core.checkout(&id, "-rf".into()).await.unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
    env.core
        .rename_branch(&id, "otra".into(), "nueva".into())
        .await
        .unwrap();
    env.core.checkout(&id, "main".into()).await.unwrap();
    env.core
        .delete_branch(&id, "nueva".into(), false)
        .await
        .unwrap();
    let snap = env.core.get_snapshot(&id).await.unwrap();
    assert_eq!(snap.0.branches.len(), 1);
}

/// Repo en mitad de un merge con conflicto en `c.txt`.
async fn repo_en_conflicto(env: &Env) -> (std::path::PathBuf, String) {
    let repo = env.repo("conf");
    env.write(&repo, "c.txt", b"a\nb\nc\n");
    env.git(&repo, &["add", "-A"]);
    env.git(&repo, &["commit", "-q", "-m", "base"]);
    env.git(&repo, &["checkout", "-q", "-b", "feature"]);
    env.write(&repo, "c.txt", b"a\nB-feature\nc\n");
    env.git(&repo, &["commit", "-q", "-am", "feature"]);
    env.git(&repo, &["checkout", "-q", "main"]);
    env.write(&repo, "c.txt", b"a\nB-main\nc\n");
    env.git(&repo, &["commit", "-q", "-am", "main"]);
    let id = env.open(&repo).await;
    let r = env
        .core
        .merge(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap();
    assert!(matches!(r, MergeResult::Conflicts { .. }), "{r:?}");
    (repo, id)
}

#[tokio::test]
async fn escribir_resolucion_exige_confirmacion() {
    let env = Env::new();
    let (repo, id) = repo_en_conflicto(&env).await;
    let conflicts = env.core.get_conflicts(&id).await.unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].path, "c.txt");
    let marked = std::fs::read(repo.join("c.txt")).unwrap();
    assert!(String::from_utf8_lossy(&marked).contains("<<<<<<<"));

    env.prompter.reject_all();
    let r = env
        .core
        .write_conflict_resolution(&id, "c.txt".into(), b"a\nresuelto\nc\n".to_vec())
        .await
        .unwrap();
    assert!(!r.done);
    assert_eq!(
        std::fs::read(repo.join("c.txt")).unwrap(),
        marked,
        "intacto"
    );
    assert!(env.prompter.last().detail.contains("c.txt"));

    env.prompter.accept_all();
    let r = env
        .core
        .write_conflict_resolution(&id, "c.txt".into(), b"a\nresuelto\r\nc\n".to_vec())
        .await
        .unwrap();
    assert!(r.done);
    // Bytes tal cual (conserva el CRLF) y sin `git add`.
    assert_eq!(
        std::fs::read(repo.join("c.txt")).unwrap(),
        b"a\nresuelto\r\nc\n"
    );
    assert_eq!(
        env.git(&repo, &["diff", "--name-only", "--diff-filter=U"])
            .trim(),
        "c.txt"
    );

    // marcar resuelto + continuar = el merge termina.
    env.core.mark_resolved(&id, "c.txt".into()).await.unwrap();
    env.core.continue_integration(&id).await.unwrap();
    let snap = env.core.get_snapshot(&id).await.unwrap();
    assert_eq!(snap.0.integration, IntegrationState::None);
}

#[tokio::test]
async fn abortar_integracion_con_y_sin_confirmacion() {
    let env = Env::new();
    let (repo, id) = repo_en_conflicto(&env).await;

    env.prompter.reject_all();
    let r = env.core.abort_integration(&id).await.unwrap();
    assert!(!r.done);
    assert!(repo.join(".git/MERGE_HEAD").exists());
    assert_eq!(env.prompter.count(), 1);

    env.prompter.accept_all();
    let r = env.core.abort_integration(&id).await.unwrap();
    assert!(r.done);
    assert!(!repo.join(".git/MERGE_HEAD").exists());

    // Sin integración en curso: error del motor, sin diálogo.
    let before = env.prompter.count();
    let e = env.core.abort_integration(&id).await.unwrap_err();
    assert_eq!(e.code, "NO_INTEGRATION");
    assert_eq!(env.prompter.count(), before);
}

#[tokio::test]
async fn flujo_de_indice_hunks_diff_log_y_amend() {
    let env = Env::new();
    let repo = env.repo("flujo");
    let lines: String = (1..=30).map(|i| format!("linea {i}\n")).collect();
    env.write(&repo, "f.txt", lines.as_bytes());
    env.git(&repo, &["add", "-A"]);
    env.git(&repo, &["commit", "-q", "-m", "base f"]);
    let id = env.open(&repo).await;

    // Dos hunks separados.
    let changed = lines
        .replace("linea 2\n", "LINEA 2\n")
        .replace("linea 28\n", "LINEA 28\n");
    env.write(&repo, "f.txt", changed.as_bytes());
    let diffs = env
        .core
        .diff(&id, DiffScope::Unstaged, vec![])
        .await
        .unwrap();
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].hunks.len(), 2);

    // El hunk viaja como JSON (sin bytes crudos), igual que desde la UI.
    let hunk_json = serde_json::to_value(&diffs[0].hunks[0]).unwrap();
    let hunk: HunkArg = serde_json::from_value(hunk_json).unwrap();
    env.core
        .stage_hunk(&id, "f.txt".into(), hunk.clone())
        .await
        .unwrap();
    let staged = env.core.diff(&id, DiffScope::Staged, vec![]).await.unwrap();
    assert_eq!(staged[0].hunks.len(), 1);
    env.core
        .unstage_hunk(&id, "f.txt".into(), hunk)
        .await
        .unwrap();
    assert!(env
        .core
        .diff(&id, DiffScope::Staged, vec![])
        .await
        .unwrap()
        .is_empty());
    let stale = HunkArg {
        id: String::new(),
        old_start: 99,
        old_count: 1,
        new_start: 99,
        new_count: 1,
        heading: String::new(),
        lines: vec![],
    };
    let e = env
        .core
        .stage_hunk(&id, "f.txt".into(), stale)
        .await
        .unwrap_err();
    assert_eq!(e.code, "HUNK_NOT_FOUND");

    env.core.stage(&id, vec!["f.txt".into()]).await.unwrap();
    let snap = env.core.get_snapshot(&id).await.unwrap();
    assert!(snap.0.status.entries[0].is_staged());
    env.core.unstage(&id, vec!["f.txt".into()]).await.unwrap();
    env.core.stage(&id, vec!["f.txt".into()]).await.unwrap();

    let draft = env.core.amend_draft(&id).await.unwrap();
    assert!(draft.can_amend);
    assert_eq!(draft.message, "base f");
    env.core
        .commit(&id, "cambia f".into(), false)
        .await
        .unwrap();
    let sha = env
        .core
        .amend(&id, "cambia f (corregido)".into())
        .await
        .unwrap();
    assert_eq!(sha, env.git(&repo, &["rev-parse", "HEAD"]).trim());

    let log = env.core.log(&id, 10, None).await.unwrap();
    assert_eq!(log[0].subject, "cambia f (corregido)");
    let found = env.core.log(&id, 10, Some("base f".into())).await.unwrap();
    assert_eq!(found.len(), 1);
    let found = env.core.log(&id, 10, Some("   ".into())).await.unwrap();
    assert_eq!(found.len(), log.len(), "consulta vacía = historial normal");
}

#[tokio::test]
async fn amend_draft_explica_por_que_no_se_puede() {
    let env = Env::new();
    let repo = env.empty_repo("vacio");
    let id = env.open(&repo).await;
    let d = env.core.amend_draft(&id).await.unwrap();
    assert!(!d.can_amend && d.reason.is_some());

    let repo2 = env.repo("detached");
    env.git(&repo2, &["checkout", "-q", "--detach"]);
    let id2 = env.open(&repo2).await;
    let d = env.core.amend_draft(&id2).await.unwrap();
    assert!(!d.can_amend);
    assert!(d.reason.unwrap().contains("rama"));
}

#[tokio::test]
async fn fetch_pull_push_contra_un_remoto_local() {
    let env = Env::new();
    let bare = env.bare("remoto.git");
    let repo = env.repo("cli");
    env.git(&repo, &["remote", "add", "origin", &Env::s(&bare)]);
    let id = env.open(&repo).await;
    env.core
        .push(&id, "origin".into(), Some("main".into()), true)
        .await
        .unwrap();
    env.core.fetch(&id, "origin".into()).await.unwrap();
    let snap = env.core.get_snapshot(&id).await.unwrap();
    assert_eq!(snap.0.remotes.len(), 1);
    // Otro clon sube un commit; pull lo trae.
    let other = env.root.join("otro-clon");
    env.git(&env.root, &["clone", "-q", &Env::s(&bare), &Env::s(&other)]);
    env.git(&other, &["config", "core.autocrlf", "false"]);
    env.git(&other, &["config", "user.name", "T"]);
    env.git(&other, &["config", "user.email", "t@example.invalid"]);
    env.write(&other, "nuevo.txt", b"n\n");
    env.git(&other, &["add", "-A"]);
    env.git(&other, &["commit", "-q", "-m", "desde otro"]);
    env.git(&other, &["push", "-q", "origin", "main"]);
    let r = env
        .core
        .pull(&id, "origin".into(), Some("main".into()), false)
        .await
        .unwrap();
    assert!(matches!(r, MergeResult::FastForward { .. }), "{r:?}");
    assert!(repo.join("nuevo.txt").exists());
    let e = env
        .core
        .push(&id, "no-existe".into(), None, false)
        .await
        .unwrap_err();
    assert_eq!(e.code, "COMMAND_FAILED");
}
