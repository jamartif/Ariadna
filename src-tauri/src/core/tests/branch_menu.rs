//! Menú contextual del riel de ramas: publicar una rama cualquiera (`push_branch`), rebasar la
//! actual sobre otra (`rebase_onto`), borrar una rama en el remoto (`delete_remote_branch`) y el
//! diff entre ramas (`branch_diff_against`, `git diff A...B`). Todo con repos y remotos locales
//! (carpetas bare); nada toca la red.

use ariadna_git::{IntegrationState, MergeResult};

use super::*;

fn head(env: &Env, repo: &Path) -> String {
    env.git(repo, &["rev-parse", "HEAD"]).trim().to_owned()
}

/// Archivo + `git add -A` + commit, con el entorno de prueba.
fn commit(env: &Env, repo: &Path, file: &str, bytes: &[u8], message: &str) {
    env.write(repo, file, bytes);
    env.git(repo, &["add", "-A"]);
    env.git(repo, &["commit", "-q", "-m", message]);
}

fn ref_exists(env: &Env, dir: &Path, name: &str) -> bool {
    env.command(dir)
        .args(["rev-parse", "--verify", "--quiet", name])
        .output()
        .unwrap()
        .status
        .success()
}

#[tokio::test]
async fn push_branch_publica_cualquier_rama_y_fija_el_upstream() {
    let env = Env::new();
    let repo = env.repo("r");
    let bare = env.bare("bare.git");
    env.git(&repo, &["remote", "add", "origin", &Env::s(&bare)]);
    env.git(&repo, &["checkout", "-q", "-b", "feature"]);
    commit(&env, &repo, "f.txt", b"uno\n", "feature 1");
    let tip = head(&env, &repo);
    env.git(&repo, &["checkout", "-q", "main"]);
    let tab = env.open(&repo).await;

    // El remoto lo decide Rust (no hay upstream todavía: cae en `origin`).
    let r = env
        .core
        .push_branch(&tab, "feature".into(), true)
        .await
        .unwrap();
    assert_eq!(r.remote, "origin");
    assert_eq!(env.git(&bare, &["rev-parse", "feature"]).trim(), tip);
    assert_eq!(
        env.git(&repo, &["config", "branch.feature.remote"]).trim(),
        "origin"
    );
    assert_eq!(
        env.git(&repo, &["config", "branch.feature.merge"]).trim(),
        "refs/heads/feature"
    );
    // La rama actual no se mueve: solo se publicó «feature».
    assert_eq!(
        env.git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"])
            .trim(),
        "main"
    );
    // Y aparece en el snapshot como rama remota (sin que la UI pida nada).
    let snap = env.core.get_snapshot(&tab).await.unwrap();
    assert!(snap
        .0
        .remote_branches
        .iter()
        .any(|b| b.name == "origin/feature"));
}

#[tokio::test]
async fn push_branch_sin_origen_no_fija_upstream_si_no_se_pide() {
    let env = Env::new();
    let repo = env.repo("r");
    let bare = env.bare("bare.git");
    env.git(&repo, &["remote", "add", "origin", &Env::s(&bare)]);
    env.git(&repo, &["checkout", "-q", "-b", "suelta"]);
    commit(&env, &repo, "s.txt", b"a\n", "suelta");
    env.git(&repo, &["checkout", "-q", "main"]);
    let tab = env.open(&repo).await;

    env.core
        .push_branch(&tab, "suelta".into(), false)
        .await
        .unwrap();
    assert!(ref_exists(&env, &bare, "suelta"));
    // Sin `setUpstream` la rama local sigue sin origen.
    assert!(env
        .command(&repo)
        .args(["config", "branch.suelta.merge"])
        .output()
        .unwrap()
        .stdout
        .is_empty());
}

#[tokio::test]
async fn push_branch_rechaza_lo_que_no_existe_sin_tocar_el_remoto() {
    let env = Env::new();
    let repo = env.repo("r");
    let bare = env.bare("bare.git");
    env.git(&repo, &["remote", "add", "origin", &Env::s(&bare)]);
    let tab = env.open(&repo).await;

    for (branch, code) in [
        ("fantasma", "INVALID_REFERENCE"),
        ("main..x", "INVALID_REFERENCE"),
        ("-x", "INVALID_REFERENCE"),
    ] {
        let e = env
            .core
            .push_branch(&tab, branch.into(), false)
            .await
            .unwrap_err();
        assert_eq!(e.code, code, "{branch:?}");
    }
    assert!(!ref_exists(&env, &bare, "main"));

    // Sin remotos no hay a dónde subir, y se dice antes de llamar a git.
    env.git(&repo, &["remote", "remove", "origin"]);
    let e = env
        .core
        .push_branch(&tab, "main".into(), false)
        .await
        .unwrap_err();
    assert_eq!(e.code, "POLICY");
}

#[tokio::test]
async fn rebase_reescribe_la_actual_sobre_la_otra_con_confirmacion() {
    let env = Env::new();
    let repo = env.repo("r");
    env.git(&repo, &["checkout", "-q", "-b", "base-nueva"]);
    commit(&env, &repo, "otra.txt", b"a\n", "de base-nueva");
    env.git(&repo, &["checkout", "-q", "main"]);
    commit(&env, &repo, "main.txt", b"b\n", "trabajo en main");
    let tab = env.open(&repo).await;
    let before = head(&env, &repo);

    let r = env
        .core
        .rebase_onto(&tab, "base-nueva".into())
        .await
        .unwrap();
    assert!(r.done);
    assert!(r.state.is_none());
    assert!(
        matches!(r.result, MergeResult::Merged { .. }),
        "{:?}",
        r.result
    );
    // La confirmación nombra las dos ramas.
    let asked = env.prompter.last();
    assert!(asked.title.contains("Rebasar"), "{asked:?}");
    assert!(asked.message.contains("«main»") && asked.message.contains("«base-nueva»"));
    // La rama actual lleva ahora lo de las dos, y el commit de main se reescribió.
    assert!(repo.join("otra.txt").exists() && repo.join("main.txt").exists());
    assert_ne!(head(&env, &repo), before);
    assert!(env
        .git(&repo, &["log", "--format=%s"])
        .contains("trabajo en main"));
    // Sigue en la misma rama y sin integración en curso.
    assert_eq!(
        env.git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"])
            .trim(),
        "main"
    );
    assert_eq!(
        env.core.get_snapshot(&tab).await.unwrap().0.integration,
        IntegrationState::None
    );
}

#[tokio::test]
async fn rebase_cancelado_no_toca_nada() {
    let env = Env::new();
    let repo = env.repo("r");
    env.git(&repo, &["checkout", "-q", "-b", "otra"]);
    commit(&env, &repo, "otra.txt", b"a\n", "otra");
    env.git(&repo, &["checkout", "-q", "main"]);
    commit(&env, &repo, "main.txt", b"b\n", "main 2");
    let tab = env.open(&repo).await;
    let before = head(&env, &repo);

    env.prompter.reject_all();
    let r = env.core.rebase_onto(&tab, "otra".into()).await.unwrap();
    assert!(!r.done);
    assert_eq!(head(&env, &repo), before);
    assert!(!repo.join("otra.txt").exists());
    assert_eq!(env.prompter.count(), 1);
}

#[tokio::test]
async fn rebase_con_conflictos_queda_en_curso_y_se_aborta() {
    let env = Env::new();
    let repo = env.repo("r");
    env.git(&repo, &["checkout", "-q", "-b", "otra"]);
    commit(&env, &repo, "README.md", b"otra\n", "otra");
    env.git(&repo, &["checkout", "-q", "main"]);
    commit(&env, &repo, "README.md", b"main\n", "main");
    let tab = env.open(&repo).await;

    let r = env.core.rebase_onto(&tab, "otra".into()).await.unwrap();
    assert!(r.done);
    assert!(
        matches!(r.result, MergeResult::Conflicts { .. }),
        "{:?}",
        r.result
    );
    let state = r.state.expect("estado del rebase en curso");
    assert_eq!(state.integration, IntegrationState::Rebasing);
    assert!(!state.conflicts.is_empty());
    assert!(repo.join(".git/rebase-merge").is_dir());

    // Se aborta como cualquier otra integración: el repo vuelve a como estaba.
    env.core.abort_integration(&tab).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.join("README.md")).unwrap(),
        "main\n"
    );
    assert!(!repo.join(".git/rebase-merge").exists());
}

#[tokio::test]
async fn rebase_rechaza_integracion_en_curso_y_la_propia_rama() {
    let env = Env::new();
    let repo = env.repo("r");
    env.git(&repo, &["checkout", "-q", "-b", "otra"]);
    commit(&env, &repo, "otra.txt", b"a\n", "otra");
    env.git(&repo, &["checkout", "-q", "main"]);
    let tab = env.open(&repo).await;
    let before = env.prompter.count();

    let e = env.core.rebase_onto(&tab, "main".into()).await.unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
    let e = env
        .core
        .rebase_onto(&tab, "fantasma".into())
        .await
        .unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
    // Ninguno de los dos casos llega a preguntar.
    assert_eq!(env.prompter.count(), before);
}

#[tokio::test]
async fn borrar_rama_remota_borra_en_el_servidor_y_respeta_cancelar() {
    let env = Env::new();
    let repo = env.repo("r");
    let bare = env.bare("bare.git");
    env.git(&repo, &["remote", "add", "origin", &Env::s(&bare)]);
    env.git(&repo, &["checkout", "-q", "-b", "feature"]);
    commit(&env, &repo, "f.txt", b"x\n", "feature");
    env.git(&repo, &["push", "-q", "origin", "feature"]);
    env.git(&repo, &["checkout", "-q", "main"]);
    env.git(&repo, &["fetch", "-q", "origin"]);
    let tab = env.open(&repo).await;

    env.prompter.reject_all();
    let r = env
        .core
        .delete_remote_branch(&tab, "origin".into(), "feature".into())
        .await
        .unwrap();
    assert!(!r.done);
    let asked = env.prompter.last();
    assert!(asked.message.contains("«feature»") && asked.message.contains("«origin»"));
    assert!(ref_exists(&env, &bare, "feature"));

    env.prompter.accept_all();
    let r = env
        .core
        .delete_remote_branch(&tab, "origin".into(), "feature".into())
        .await
        .unwrap();
    assert!(r.done);
    assert!(!ref_exists(&env, &bare, "feature"));
    // La rama local sigue donde estaba: solo se tocó el servidor.
    assert_eq!(
        env.git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"])
            .trim(),
        "main"
    );
    assert!(ref_exists(&env, &repo, "feature"));

    // Ya no está en el remoto (el snapshot lo refleja tras el refresco de la mutación).
    let e = env
        .core
        .delete_remote_branch(&tab, "origin".into(), "feature".into())
        .await
        .unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
}

#[tokio::test]
async fn diff_entre_ramas_incluye_binarios_y_renombrados() {
    let env = Env::new();
    let repo = env.repo("r");
    env.write(&repo, "viejo.txt", b"contenido para renombrar\nlinea 2\n");
    env.write(&repo, "bin.dat", &[0, 1, 2, 3, 0]);
    env.git(&repo, &["add", "-A"]);
    env.git(&repo, &["commit", "-q", "-m", "base"]);
    env.git(&repo, &["checkout", "-q", "-b", "tema"]);
    env.git(&repo, &["mv", "viejo.txt", "nuevo.txt"]);
    env.write(&repo, "bin.dat", &[0, 9, 9, 9, 0]);
    env.write(&repo, "extra.txt", b"nuevo\n");
    commit(&env, &repo, "extra.txt", b"nuevo\n", "tema");
    env.git(&repo, &["checkout", "-q", "main"]);
    let tab = env.open(&repo).await;

    let files = env
        .core
        .branch_diff_against(&tab, "main".into(), "tema".into())
        .await
        .unwrap();
    let by = |p: &str| {
        files
            .iter()
            .find(|f| f.path.to_string_lossy() == p)
            .unwrap_or_else(|| panic!("falta {p}"))
    };
    assert!(by("extra.txt").is_new_file);
    assert!(by("bin.dat").is_binary);
    assert_eq!(
        by("nuevo.txt")
            .old_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
        Some("viejo.txt".to_owned())
    );
    assert!(!by("nuevo.txt").is_binary);
    // Contra sí misma no hay nada que comparar.
    assert!(env
        .core
        .branch_diff_against(&tab, "tema".into(), "tema".into())
        .await
        .unwrap()
        .is_empty());
    // Una rama que no existe se rechaza sin llegar a git.
    let e = env
        .core
        .branch_diff_against(&tab, "main".into(), "fantasma".into())
        .await
        .unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
}
