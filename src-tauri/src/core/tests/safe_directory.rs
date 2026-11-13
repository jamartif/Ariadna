//! `safe.directory` desde el núcleo: confirmación nativa y escritura de **solo** esa carpeta.
//!
//! El error real de `dubious ownership` no se puede provocar sin cambiar propietarios, así que
//! aquí se prueba la parte que sí es nuestra: el diálogo, la validación y lo que se escribe en
//! la configuración global (con `GIT_CONFIG_GLOBAL` apuntando a un fichero temporal: los tests
//! **nunca** tocan el `.gitconfig` de quien los ejecuta).

use super::*;

fn config(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[tokio::test]
async fn sin_confirmacion_no_se_escribe_nada() {
    let env = Env::new();
    let repo = env.repo("ajeno");
    let cfg = env.root.join("gitconfig-safe");
    env.prompter.reject_all();
    let ok = env
        .core
        .confirm_safe_directory(&Env::s(&repo), Some(&cfg))
        .await;
    assert!(!ok);
    assert_eq!(env.prompter.count(), 1);
    let asked = env.prompter.last();
    assert!(asked.message.contains(&Env::s(&repo)), "{}", asked.message);
    assert!(asked.detail.contains("safe.directory"), "{}", asked.detail);
    assert!(!cfg.exists(), "no se creó la configuración");
}

#[tokio::test]
async fn al_aceptar_se_anade_solo_esa_carpeta_y_nunca_asterisco() {
    let env = Env::new();
    let repo = env.repo("ajeno");
    let cfg = env.root.join("gitconfig-safe");
    let ok = env
        .core
        .confirm_safe_directory(&Env::s(&repo), Some(&cfg))
        .await;
    assert!(ok);
    let text = config(&cfg);
    assert!(text.contains("[safe]"), "{text}");
    assert!(text.contains("directory ="), "{text}");
    assert!(text.contains(&Env::s(&repo)), "{text}");
    assert!(!text.contains('*'), "{text}");
    // Idempotente: aceptar otra vez no rompe nada.
    assert!(
        env.core
            .confirm_safe_directory(&Env::s(&repo), Some(&cfg))
            .await
    );
    assert!(!config(&cfg).contains('*'));
}

#[tokio::test]
async fn una_ruta_que_no_vale_falla_sin_reintentar() {
    let env = Env::new();
    let cfg = env.root.join("gitconfig-safe");
    // `*` y rutas relativas se rechazan en el motor: la confirmación se acepta pero no se escribe.
    for bad in ["*", "relativa/repo"] {
        assert!(
            !env.core.confirm_safe_directory(bad, Some(&cfg)).await,
            "{bad}"
        );
    }
    assert!(!cfg.exists() || !config(&cfg).contains('*'));
}

#[test]
fn el_aviso_del_error_nombra_la_carpeta() {
    let e: crate::core::ApiError = ariadna_git::GitError::DubiousOwnership {
        path: "/mnt/usb/repo".into(),
    }
    .into();
    assert_eq!(e.code, "DUBIOUS_OWNERSHIP");
    assert_eq!(e.detail.as_deref(), Some("/mnt/usb/repo"));
}
