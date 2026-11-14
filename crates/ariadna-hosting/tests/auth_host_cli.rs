//! Sesión de `gh`/`glab` como credencial, con ejecutables **falsos** en un `PATH` temporal.
//!
//! Solo Unix: los dobles son scripts `#!/bin/sh`. En Windows el mismo camino se cubre con
//! `CliRunner` inyectado en las pruebas unitarias del módulo.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use ariadna_hosting::{CliSession, CliTool, SystemCli};
use std::sync::Arc;

/// Escribe un `gh`/`glab` falso que ejecuta `body` con `/bin/sh`.
fn fake_cli(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    path
}

fn session(path: &Path, tool: CliTool) -> CliSession {
    CliSession::with_runner(tool, Arc::new(SystemCli::with_path(path)))
}

#[tokio::test]
async fn sesion_valida_devuelve_el_token_sin_espacios_alrededor() {
    let dir = tempfile::tempdir().unwrap();
    // El token llega con espacios y saltos de línea alrededor (como un `printf` descuidado).
    fake_cli(
        dir.path(),
        "gh",
        "case \"$1\" in\n  --version) echo 'gh version 2.103.3' ;;\n  auth) case \"$2\" in status) exit 0 ;; token) printf '  gho_abcdef123456  \\n\\n' ;; esac ;;\nesac",
    );
    let s = session(dir.path(), CliTool::Gh);
    let st = s.status().await;
    assert!(st.installed && st.logged_in, "{st:?}");
    assert_eq!(st.version.as_deref(), Some("gh version 2.103.3"));
    assert!(st.detail.is_none());
    assert_eq!(s.token().await.unwrap().expose(), "gho_abcdef123456");
}

#[tokio::test]
async fn sin_sesion_el_error_dice_que_comando_ejecutar_y_no_filtra_nada() {
    let dir = tempfile::tempdir().unwrap();
    fake_cli(
        dir.path(),
        "gh",
        "case \"$1\" in\n  --version) echo 'gh version 2.103.3' ;;\n  auth) echo 'no oauth token' >&2; exit 1 ;;\nesac",
    );
    let s = session(dir.path(), CliTool::Gh);
    let st = s.status().await;
    assert!(st.installed && !st.logged_in, "{st:?}");
    assert!(
        st.detail
            .as_deref()
            .unwrap_or_default()
            .contains("gh auth login"),
        "{st:?}"
    );

    let err = s.token().await.unwrap_err();
    assert_eq!(err.code(), "AUTH");
    let text = format!("{err} {err:?}");
    assert!(text.contains("gh auth login"), "{text}");
    assert!(!text.contains("gho_"), "{text}");
}

#[tokio::test]
async fn cli_ausente_se_detecta_sin_pedir_el_token() {
    let dir = tempfile::tempdir().unwrap();
    let s = session(dir.path(), CliTool::Gh);
    let st = s.status().await;
    assert!(!st.installed && !st.logged_in);
    assert!(st
        .detail
        .as_deref()
        .unwrap_or_default()
        .contains("No se encontró «gh»"));
    let err = s.token().await.unwrap_err();
    assert_eq!(err.code(), "AUTH");
    assert!(err.to_string().contains("gh auth login"));
}

#[tokio::test]
async fn un_cli_que_no_responde_no_cuelga_el_dialogo() {
    let dir = tempfile::tempdir().unwrap();
    // `sleep` en ruta absoluta: el `PATH` del hijo es solo la carpeta del doble.
    fake_cli(dir.path(), "gh", "/bin/sleep 5");
    let s = CliSession::with_runner(
        CliTool::Gh,
        Arc::new(
            SystemCli::with_path(dir.path()).with_timeout(std::time::Duration::from_millis(150)),
        ),
    );
    let started = std::time::Instant::now();
    let st = s.status().await;
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
    assert!(st.installed && !st.logged_in, "{st:?}");
    assert!(
        st.detail
            .as_deref()
            .unwrap_or_default()
            .contains("no respondió a tiempo"),
        "{st:?}"
    );
    let err = s.token().await.unwrap_err();
    assert_eq!(err.code(), "AUTH");
    assert!(err.to_string().contains("no respondió a tiempo"));
}

#[tokio::test]
async fn un_token_con_espacios_dentro_se_rechaza_sin_repetirlo() {
    let dir = tempfile::tempdir().unwrap();
    fake_cli(
        dir.path(),
        "gh",
        "case \"$1\" in\n  auth) case \"$2\" in status) exit 0 ;; token) printf 'gho_abc 123 456\\n' ;; esac ;;\nesac",
    );
    let s = CliSession::with_runner(CliTool::Gh, Arc::new(SystemCli::with_path(dir.path())));
    let err = s.token().await.unwrap_err();
    assert_eq!(err.code(), "AUTH");
    let text = format!("{err} {err:?}");
    assert!(!text.contains("gho_abc"), "{text}");
    assert!(text.contains("gh auth login"), "{text}");
}

#[tokio::test]
async fn glab_lee_el_token_de_auth_status_show_token() {
    let dir = tempfile::tempdir().unwrap();
    fake_cli(
        dir.path(),
        "glab",
        "case \"$1\" in\n  --version) echo 'glab version 1.64.0' ;;\n  auth) if [ \"$3\" = '--show-token' ]; then printf 'gitlab.com\\n  - Token: glpat-abcdef123456\\n  - Token scopes: api\\n'; else exit 0; fi ;;\nesac",
    );
    let s = session(dir.path(), CliTool::Glab);
    let st = s.status().await;
    assert!(st.installed && st.logged_in, "{st:?}");
    assert_eq!(s.token().await.unwrap().expose(), "glpat-abcdef123456");
}

#[tokio::test]
async fn la_variable_gh_token_del_entorno_no_se_hereda() {
    // Aunque el proceso tenga GH_TOKEN, la fuente es la sesión del CLI (aquí, sin sesión).
    let dir = tempfile::tempdir().unwrap();
    fake_cli(
        dir.path(),
        "gh",
        "if [ -n \"$GH_TOKEN\" ]; then printf '%s' \"$GH_TOKEN\"; else exit 1; fi",
    );
    std::env::set_var("GH_TOKEN", "gho_del_entorno_12345");
    let s = session(dir.path(), CliTool::Gh);
    let err = s.token().await.unwrap_err();
    std::env::remove_var("GH_TOKEN");
    assert_eq!(err.code(), "AUTH");
    assert!(!err.to_string().contains("gho_del_entorno"));
}
