//! Métodos de autenticación del clonado (`auth_ops` + `clone_ops`): PAT, sesión de CLI y el
//! flujo OAuth preparado.
//!
//! Lo que se comprueba aquí es lo que se puede comprobar **sin red** y sin depender de que la
//! máquina tenga `gh`/`glab` instalados: resolución del método, rechazos tempranos (antes de
//! tocar el disco) y que la opción OAuth solo aparece con `client_id`. El camino real del CLI se
//! cubre con ejecutables falsos en `crates/ariadna-hosting/tests/auth_host_cli.rs`.

use ariadna_git::Secret;

use super::*;
use crate::core::{CloneAuthArg, CloneMethod};

/// ¿Hay un `gh`/`glab` de verdad en esta máquina? (Si lo hay, las pruebas que dependen de su
/// ausencia no se ejecutan: no se quiere salir a la red desde los tests.)
fn cli_present(tool: &str) -> bool {
    std::process::Command::new(tool)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

#[tokio::test]
async fn sin_sesion_de_gh_el_clonado_falla_antes_de_crear_nada() {
    if cli_present("gh") {
        return; // máquina con GitHub CLI: la prueba de la ausencia no aplica
    }
    let env = Env::new();
    let dest = env.root.join("no-debe-existir");
    let e = env
        .core
        .clone_repository(
            "https://github.com/o/r.git".into(),
            Env::s(&dest),
            None,
            Some(CloneAuthArg::cli(CloneMethod::Gh)),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code, "AUTH", "{e:?}");
    assert!(e.message.contains("gh auth login"), "{e:?}");
    assert!(!dest.exists(), "no se toca el disco si falta la credencial");
}

#[tokio::test]
async fn el_metodo_tiene_que_encajar_con_el_proveedor() {
    let env = Env::new();
    let dest = env.root.join("x");
    // Un token de GitLab nunca se manda a GitHub (ni al revés).
    let e = env
        .core
        .clone_repository(
            "https://github.com/o/r.git".into(),
            Env::s(&dest),
            None,
            Some(CloneAuthArg::cli(CloneMethod::Glab)),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code, "POLICY", "{e:?}");
    assert!(e.message.contains("GitLab CLI"), "{e:?}");
    assert!(!dest.exists());
}

#[tokio::test]
async fn una_url_no_https_no_admite_sesiones_de_cli() {
    let env = Env::new();
    let dest = env.root.join("y");
    let e = env
        .core
        .clone_repository(
            "git@github.com:o/r.git".into(),
            Env::s(&dest),
            None,
            Some(CloneAuthArg::cli(CloneMethod::Gh)),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE", "{e:?}");
    assert!(!dest.exists());
}

#[tokio::test]
async fn el_pat_sigue_funcionando_y_se_valida_con_ls_remote() {
    let env = Env::new();
    // Repo bare vacío por `file://`: no hay red ni PAT que valga, pero el método se usa igual.
    let url = format!("file://{}", Env::s(&env.bare("origen-pat")));
    let dest = env.root.join("con-pat");
    let r = env
        .core
        .clone_repository(
            url,
            Env::s(&dest),
            None,
            Some(CloneAuthArg::pat(None, Secret::new("pat-de-prueba-1234"))),
        )
        .await
        .unwrap();
    assert!(dest.join(".git").exists());
    assert_eq!(r.tab.name, "con-pat");
}

#[tokio::test]
async fn auth_options_describe_el_estado_real_sin_tokens() {
    let env = Env::new();
    let o = env
        .core
        .auth_options("https://github.com/o/r.git")
        .await
        .unwrap();
    assert_eq!(o.provider, "github");
    assert!(o.https);
    assert_eq!(o.cli.len(), 1);
    let gh = &o.cli[0];
    assert_eq!(gh.id, "gh");
    assert_eq!(gh.login_command, "gh auth login");
    assert_eq!(gh.username, "x-access-token");
    // Sin `client_id` en los ajustes, OAuth está oculto y se explica cómo activarlo.
    let oauth = o.oauth.as_ref().expect("GitHub tiene flujo OAuth");
    assert!(!oauth.available);
    assert!(oauth
        .reason
        .as_deref()
        .unwrap_or_default()
        .contains("client_id"));
    assert_eq!(oauth.scopes, vec!["repo".to_string()]);
    // Nada del estado es un token.
    let json = serde_json::to_string(&o).unwrap();
    assert!(!json.contains("gho_") && !json.contains("ghp_"));

    // Azure DevOps no tiene CLI ni OAuth en la app.
    let az = env
        .core
        .auth_options("https://dev.azure.com/o/p/_git/r")
        .await
        .unwrap();
    assert_eq!(az.provider, "azureDevOps");
    assert!(az.cli.is_empty());
    assert!(az.oauth.is_none());

    // GitLab ofrece `glab` y pide los dos permisos de repositorio.
    let gl = env
        .core
        .auth_options("https://gitlab.com/o/r.git")
        .await
        .unwrap();
    assert_eq!(gl.cli[0].id, "glab");
    assert_eq!(gl.cli[0].username, "oauth2");
    assert_eq!(
        gl.oauth.unwrap().scopes,
        vec![
            "read_repository".to_string(),
            "write_repository".to_string()
        ]
    );
}

#[tokio::test]
async fn con_client_id_configurado_oauth_queda_disponible_y_su_flujo_no_arranca_sin_red() {
    let env = Env::new();
    let mut s = env.core.settings_snapshot();
    s.oauth_github_client_id = Some("Iv1.cliente-de-prueba".into());
    env.core.save_settings(s).await.unwrap();
    let o = env
        .core
        .auth_options("https://github.com/o/r.git")
        .await
        .unwrap();
    assert!(o.oauth.unwrap().available);

    // GitLab sigue sin client_id: su opción sigue oculta.
    let gl = env
        .core
        .auth_options("https://gitlab.com/o/r.git")
        .await
        .unwrap();
    assert!(!gl.oauth.unwrap().available);

    // Un flujo que no existe no inventa un token.
    let err = env
        .core
        .device_login_poll("oauth-inexistente")
        .await
        .unwrap_err();
    assert_eq!(err.code, "POLICY");
    // Cancelar algo que no existe no es un error para la UI.
    assert!(env
        .core
        .device_login_cancel("oauth-inexistente")
        .await
        .is_ok());
}

#[tokio::test]
async fn el_device_login_de_azure_se_rechaza_con_policy() {
    let env = Env::new();
    let err = env
        .core
        .device_login_start("https://dev.azure.com/o/p/_git/r")
        .await
        .unwrap_err();
    assert_eq!(err.code, "POLICY");
    assert!(err.message.contains("GitHub y GitLab"));
}

#[tokio::test]
async fn un_client_id_con_basura_se_rechaza_en_los_ajustes() {
    let env = Env::new();
    let mut s = env.core.settings_snapshot();
    s.oauth_github_client_id = Some("no vale con espacios".into());
    let err = env.core.save_settings(s).await.unwrap_err();
    assert_eq!(err.code, "PARSE");
    assert_eq!(err.detail.as_deref(), Some("oauthGitHubClientId"));

    // Vacío = sin OAuth (se guarda como ausente).
    let mut s = env.core.settings_snapshot();
    s.oauth_github_client_id = Some("   ".into());
    env.core.save_settings(s).await.unwrap();
    assert!(env
        .core
        .settings_snapshot()
        .oauth_github_client_id
        .is_none());
}

#[test]
fn los_metodos_se_parsean_y_lo_desconocido_es_un_error() {
    assert_eq!(CloneMethod::parse("").unwrap(), CloneMethod::Pat);
    assert_eq!(CloneMethod::parse("PAT").unwrap(), CloneMethod::Pat);
    assert_eq!(CloneMethod::parse(" gh ").unwrap(), CloneMethod::Gh);
    assert_eq!(CloneMethod::parse("glab").unwrap(), CloneMethod::Glab);
    assert_eq!(CloneMethod::parse("oauth").unwrap(), CloneMethod::OAuth);
    let e = CloneMethod::parse("token").unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
    assert!(e.message.contains("token"));
}
