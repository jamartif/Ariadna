//! Ajustes en el núcleo: persistencia, validación, arranque con archivo corrupto, ruta de git
//! configurada, pull con rebase y reglas repo → perfil de IA.

use super::Env;
use crate::core::api::AiSettingsPatch;
use crate::core::settings::{ProfileRule, SettingsLoadStatus, Theme};
use ariadna_ai::ProfileKind;

fn settings_file(env: &Env) -> std::path::PathBuf {
    env.data.join("settings.json")
}

#[tokio::test]
async fn por_defecto_y_datos_de_acerca_de() {
    let env = Env::new();
    let v = env.core.get_settings();
    assert_eq!(v.settings, crate::core::Settings::default());
    assert_eq!(v.load.status, SettingsLoadStatus::Missing);
    assert_eq!(v.about.license, "MIT");
    assert_eq!(v.about.version, env!("CARGO_PKG_VERSION"));
    assert!(v.about.repository.starts_with("https://"));
    assert!(v.git.is_some() && v.git_problem.is_none());
}

#[tokio::test]
async fn guardar_persiste_y_sobrevive_a_un_reinicio() {
    let env = Env::new();
    let mut s = env.core.get_settings().settings;
    s.theme = Theme::Dark;
    s.pull_rebase = true;
    s.diff_font_size = 16;
    s.profile_rules = vec![ProfileRule {
        pattern: "GitLab.com/Mi-Org/".into(),
        profile: ProfileKind::Empresa,
    }];
    let saved = env.core.save_settings(s).await.unwrap();
    assert_eq!(saved.settings.profile_rules[0].pattern, "gitlab.com/mi-org");
    assert!(settings_file(&env).is_file());
    assert!(!env.data.join("settings.json.tmp").exists());

    let (core2, _) = env.restart();
    let v = core2.get_settings();
    assert_eq!(v.settings, saved.settings);
    assert_eq!(v.load.status, SettingsLoadStatus::Ok);
}

#[tokio::test]
async fn ajuste_invalido_se_rechaza_sin_tocar_nada() {
    let env = Env::new();
    let before = env.core.get_settings().settings;
    let mut s = before.clone();
    s.diff_font_size = 3;
    let e = env.core.save_settings(s).await.unwrap_err();
    assert_eq!(e.code, "PARSE");
    assert_eq!(e.detail.as_deref(), Some("diffFontSize"));
    let mut s = before.clone();
    s.profile_rules = vec![ProfileRule {
        pattern: "https://github.com".into(),
        profile: ProfileKind::Personal,
    }];
    assert_eq!(env.core.save_settings(s).await.unwrap_err().code, "PARSE");
    assert_eq!(env.core.get_settings().settings, before);
    assert!(!settings_file(&env).exists());
}

#[tokio::test]
async fn archivo_corrupto_no_rompe_el_arranque() {
    let env = Env::new();
    std::fs::create_dir_all(&env.data).unwrap();
    std::fs::write(settings_file(&env), b"{ roto").unwrap();
    let (core2, _) = env.restart();
    let v = core2.get_settings();
    assert_eq!(v.settings, crate::core::Settings::default());
    assert_eq!(v.load.status, SettingsLoadStatus::ResetCorrupt);
    assert!(v.load.warning.unwrap().contains("settings.json.bak"));
    assert_eq!(
        std::fs::read(env.data.join("settings.json.bak")).unwrap(),
        b"{ roto"
    );
    // La app funciona: se abre un repo y se puede guardar de nuevo.
    let repo = env.repo("tras-corrupto");
    let open = core2.open_repository(&Env::s(&repo)).await.unwrap();
    assert!(!open.already_open);
    let mut s = v.settings;
    s.pull_rebase = true;
    core2.save_settings(s).await.unwrap();
    // El aviso es del arranque; en el siguiente arranque el archivo ya es válido.
    let (core3, _) = env.restart();
    assert_eq!(core3.get_settings().load.status, SettingsLoadStatus::Ok);
    assert!(core3.get_settings().settings.pull_rebase);
}

#[tokio::test]
async fn detect_git_valida_y_da_la_version() {
    let env = Env::new();
    let ok = env.core.detect_git(Some(Env::s(&env.git))).await;
    assert!(ok.ok, "{ok:?}");
    assert_eq!(ok.source, "configured");
    assert!(ok.version.unwrap().starts_with('2'));
    let auto = env.core.detect_git(None).await;
    assert!(auto.ok && auto.source == "auto", "{auto:?}");
    let falso = env.root.join("no-hay-git");
    let bad = env.core.detect_git(Some(Env::s(&falso))).await;
    assert!(!bad.ok && bad.problem.is_some() && bad.version.is_none());
    let rel = env.core.detect_git(Some("git".into())).await;
    assert!(!rel.ok, "una ruta relativa no vale");
}

#[tokio::test]
async fn ruta_de_git_invalida_se_rechaza_y_valida_se_aplica() {
    let env = Env::new();
    let mut s = env.core.get_settings().settings;
    s.git_path = Some(Env::s(&env.root.join("no-hay-git")));
    let e = env.core.save_settings(s.clone()).await.unwrap_err();
    assert_eq!(e.code, "GIT_NOT_FOUND");
    assert_eq!(env.core.get_settings().settings.git_path, None);

    s.git_path = Some(Env::s(&env.git));
    let v = env.core.save_settings(s).await.unwrap();
    let usado = v.git.unwrap().path;
    assert!(
        usado.ends_with(env.git.file_name().unwrap().to_str().unwrap()),
        "{usado}"
    );
    // Volver a «detectar solo».
    let mut s = v.settings;
    s.git_path = None;
    assert!(env.core.save_settings(s).await.unwrap().git.is_some());
}

#[tokio::test]
async fn ruta_guardada_que_ya_no_sirve_avisa_y_arranca_con_la_detectada() {
    let env = Env::new();
    std::fs::create_dir_all(&env.data).unwrap();
    std::fs::write(
        settings_file(&env),
        format!(
            "{{\"version\":1,\"gitPath\":{}}}",
            serde_json::to_string(&Env::s(&env.root.join("desaparecido"))).unwrap()
        ),
    )
    .unwrap();
    let (core2, _) = env.restart();
    let v = core2.get_settings();
    assert!(v.git.is_some(), "usa el git detectado");
    assert!(v.load.warning.unwrap().contains("ruta de git guardada"));
}

#[cfg(unix)]
#[tokio::test]
async fn el_motor_usa_la_ruta_de_git_configurada() {
    use std::os::unix::fs::PermissionsExt;
    let env = Env::new();
    let log = env.root.join("git-invocado.log");
    let wrapper = env.root.join("mi-git");
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\nexec '{}' \"$@\"\n",
            log.display(),
            env.git.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut s = env.core.get_settings().settings;
    s.git_path = Some(Env::s(&wrapper));
    env.core.save_settings(s).await.unwrap();
    let before = std::fs::read_to_string(&log).unwrap_or_default();

    let repo = env.repo("con-mi-git");
    env.open(&repo).await;
    let after = std::fs::read_to_string(&log).unwrap();
    assert!(
        after.len() > before.len(),
        "el motor no pasó por el git configurado"
    );
    assert!(after.contains("status"), "{after}");
}

/// Dos clones del mismo remoto con historias divergentes.
async fn divergente(env: &Env) -> (String, std::path::PathBuf) {
    let bare = env.bare("remoto.git");
    let repo = env.repo("local");
    env.git(&repo, &["remote", "add", "origin", &Env::s(&bare)]);
    env.git(&repo, &["push", "-q", "-u", "origin", "main"]);
    let other = env.root.join("otro");
    env.git(&env.root, &["clone", "-q", &Env::s(&bare), &Env::s(&other)]);
    for (k, v) in [
        ("core.autocrlf", "false"),
        ("user.name", "T"),
        ("user.email", "t@x.invalid"),
    ] {
        env.git(&other, &["config", k, v]);
    }
    env.write(&other, "remoto.txt", b"r\n");
    env.git(&other, &["add", "-A"]);
    env.git(&other, &["commit", "-q", "-m", "desde el remoto"]);
    env.git(&other, &["push", "-q", "origin", "main"]);
    env.write(&repo, "local.txt", b"l\n");
    env.git(&repo, &["add", "-A"]);
    env.git(&repo, &["commit", "-q", "-m", "local"]);
    let id = env.open(&repo).await;
    (id, repo)
}

fn merges(env: &Env, repo: &std::path::Path) -> usize {
    env.git(repo, &["rev-list", "--merges", "HEAD"])
        .lines()
        .count()
}

#[tokio::test]
async fn pull_sin_indicacion_usa_el_ajuste_de_rebase() {
    // Rebase activado: historia lineal.
    let env = Env::new();
    let mut s = env.core.get_settings().settings;
    s.pull_rebase = true;
    env.core.save_settings(s).await.unwrap();
    let (id, repo) = divergente(&env).await;
    env.core
        .pull(&id, "origin".into(), Some("main".into()), None)
        .await
        .unwrap();
    assert_eq!(merges(&env, &repo), 0, "con rebase no hay commit de fusión");
    assert!(repo.join("remoto.txt").exists() && repo.join("local.txt").exists());

    // Rebase desactivado (por defecto): commit de fusión.
    let env2 = Env::new();
    let (id2, repo2) = divergente(&env2).await;
    env2.core
        .pull(&id2, "origin".into(), Some("main".into()), None)
        .await
        .unwrap();
    assert_eq!(merges(&env2, &repo2), 1);

    // Una indicación explícita manda sobre el ajuste.
    let env3 = Env::new();
    let mut s = env3.core.get_settings().settings;
    s.pull_rebase = true;
    env3.core.save_settings(s).await.unwrap();
    let (id3, repo3) = divergente(&env3).await;
    env3.core
        .pull(&id3, "origin".into(), Some("main".into()), Some(false))
        .await
        .unwrap();
    assert_eq!(merges(&env3, &repo3), 1);
}

#[tokio::test]
async fn git_identity_lee_la_config_de_git_sin_escribirla() {
    let env = Env::new();
    // El Env aísla HOME en `root/home`, pero el motor hereda el entorno real: solo se
    // comprueba que no falla y que devuelve texto no vacío o `None`.
    let id = env.core.git_identity().await;
    assert!(id.name.as_deref().is_none_or(|n| !n.is_empty()));
    assert!(id.email.as_deref().is_none_or(|n| !n.is_empty()));
}

#[tokio::test]
async fn la_regla_repo_a_perfil_manda_y_se_muestra() {
    let env = Env::new();
    let azure = env.repo("azure");
    env.git(
        &azure,
        &[
            "remote",
            "add",
            "origin",
            "https://dev.azure.com/acme/proj/_git/api",
        ],
    );
    let gh = env.repo("gh");
    env.git(
        &gh,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:javie/ariadna.git",
        ],
    );
    let otro = env.repo("otro-host");
    env.git(
        &otro,
        &[
            "remote",
            "add",
            "origin",
            "https://git.interno.example/x/y.git",
        ],
    );
    let sin = env.repo("sin-remoto");
    let (a, g, o, s) = (
        env.open(&azure).await,
        env.open(&gh).await,
        env.open(&otro).await,
        env.open(&sin).await,
    );

    let st = env.core.ai_state(&a).await.unwrap();
    assert_eq!(st.profile.kind, ProfileKind::Empresa);
    assert_eq!(st.profile.rule.as_deref(), Some("dev.azure.com"));
    let claude = st
        .providers
        .iter()
        .find(|p| p.id.binary_name() == "claude")
        .unwrap();
    assert!(!claude.enabled, "Claude desactivado en Empresa");

    let st = env.core.ai_state(&g).await.unwrap();
    assert_eq!(st.profile.kind, ProfileKind::Personal);
    assert_eq!(st.profile.rule.as_deref(), Some("github.com"));

    // Sin regla aplicable o sin remoto: el perfil elegido en Ajustes, sin `rule`.
    env.core
        .ai_update_settings(AiSettingsPatch {
            profile: Some("empresa".into()),
            ..AiSettingsPatch::default()
        })
        .await
        .unwrap();
    for tab in [&o, &s] {
        let st = env.core.ai_state(tab).await.unwrap();
        assert_eq!(st.profile.kind, ProfileKind::Empresa);
        assert_eq!(st.profile.rule, None);
    }
    // Con la regla, el perfil manual no cuenta: GitHub sigue siendo Personal.
    let st = env.core.ai_state(&g).await.unwrap();
    assert_eq!(st.profile.kind, ProfileKind::Personal);

    // El estado devuelto al guardar ajustes de IA también aplica la regla de la pestaña.
    let st = env
        .core
        .ai_update_settings(AiSettingsPatch {
            tab_id: Some(a.clone()),
            ..AiSettingsPatch::default()
        })
        .await
        .unwrap();
    assert_eq!(st.profile.rule.as_deref(), Some("dev.azure.com"));

    // Editar las reglas cambia el resultado en caliente.
    let mut cfg = env.core.get_settings().settings;
    cfg.profile_rules = vec![ProfileRule {
        pattern: "dev.azure.com/acme".into(),
        profile: ProfileKind::Personal,
    }];
    env.core.save_settings(cfg).await.unwrap();
    let st = env.core.ai_state(&a).await.unwrap();
    assert_eq!(st.profile.kind, ProfileKind::Personal);
    assert_eq!(st.profile.rule.as_deref(), Some("dev.azure.com/acme"));
    assert_eq!(env.core.ai_state(&g).await.unwrap().profile.rule, None);
}

#[tokio::test]
async fn cli_status_no_encontrado_sin_cli_y_con_comando_de_login() {
    let env = Env::new();
    env.core
        .inner
        .ai
        .set_search_env(ariadna_ai::resolve::SearchEnv::default());
    let list = env.core.get_cli_status().await;
    assert_eq!(list.len(), 3);
    for c in &list {
        assert!(
            !c.installed && c.path.is_none() && c.version.is_none(),
            "{c:?}"
        );
        assert_eq!(c.availability, "missing");
        assert!(c.hint.is_some());
    }
    let login: Vec<_> = list.iter().map(|c| c.login_command.as_str()).collect();
    assert_eq!(login, ["claude auth login", "codex login", "copilot login"]);
}

fn claves(v: &serde_json::Value) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    k.sort();
    k
}

/// Las claves JSON son las que valida `ui/src/lib/api/validate.ts` (camelCase, sin extras).
#[tokio::test]
async fn formas_json_del_contrato_con_la_ui() {
    let env = Env::new();
    env.core
        .inner
        .ai
        .set_search_env(ariadna_ai::resolve::SearchEnv::default());
    let v = serde_json::to_value(env.core.get_settings()).unwrap();
    assert_eq!(
        claves(&v),
        ["about", "git", "gitProblem", "load", "settings"]
    );
    assert_eq!(
        claves(&v["settings"]),
        [
            "diffFontSize",
            "gitPath",
            "language",
            "profileRules",
            "pullRebase",
            "theme",
            "version"
        ]
    );
    assert_eq!(claves(&v["load"]), ["status", "warning"]);
    assert_eq!(v["load"]["status"], "missing");
    assert_eq!(
        claves(&v["about"]),
        ["issues", "license", "repository", "version"]
    );
    assert_eq!(claves(&v["git"]), ["path", "version"]);
    assert_eq!(v["settings"]["theme"], "system");
    assert_eq!(
        claves(&v["settings"]["profileRules"][0]),
        ["pattern", "profile"]
    );
    assert_eq!(v["settings"]["profileRules"][0]["profile"], "empresa");

    let d = serde_json::to_value(env.core.detect_git(None).await).unwrap();
    assert_eq!(claves(&d), ["ok", "path", "problem", "source", "version"]);
    let i = serde_json::to_value(env.core.git_identity().await).unwrap();
    assert_eq!(claves(&i), ["email", "name"]);
    let c = serde_json::to_value(env.core.get_cli_status().await).unwrap();
    assert_eq!(
        claves(&c[0]),
        [
            "availability",
            "displayName",
            "hint",
            "id",
            "installed",
            "loginCommand",
            "path",
            "version"
        ]
    );
    assert_eq!(c[0]["id"], "claude");

    // `ai_state` solo lleva `profile.rule` cuando hay regla.
    let repo = env.repo("forma");
    let tab = env.open(&repo).await;
    let st = serde_json::to_value(env.core.ai_state(&tab).await.unwrap()).unwrap();
    assert_eq!(claves(&st["profile"]), ["isolated", "kind", "name"]);
    assert_eq!(
        claves(&st["providers"][0]),
        [
            "availability",
            "consented",
            "displayName",
            "enabled",
            "hint",
            "id",
            "isolated",
            "isolationVerified",
            "loginCommand",
            "sharesLogin",
            "version"
        ]
    );
    env.git(
        &repo,
        &["remote", "add", "origin", "https://github.com/a/b.git"],
    );
    env.core.refresh(&tab).await.unwrap();
    let st = serde_json::to_value(env.core.ai_state(&tab).await.unwrap()).unwrap();
    assert_eq!(claves(&st["profile"]), ["isolated", "kind", "name", "rule"]);

    // Formas de `ai_accounts` y `ai_login`.
    let a = serde_json::to_value(
        env.core
            .ai_accounts(crate::core::api::AiAccountsRequest::default())
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(claves(&a), ["accounts", "profile"]);
    assert_eq!(
        claves(&a["accounts"][0]),
        [
            "authMethod",
            "configDir",
            "email",
            "organization",
            "plan",
            "provider",
            "reason",
            "state"
        ]
    );
    assert_eq!(a["accounts"][0]["state"], "unavailable");
}
