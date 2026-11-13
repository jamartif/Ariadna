//! Entorno de los hijos de IA por **lista blanca** (RF-IA3).
//!
//! Solo pasan las variables listadas; las de `ALWAYS_REMOVE` se eliminan siempre,
//! aunque alguien las añada a la lista. La comparación no distingue mayúsculas
//! (la hace `ariadna_process::build_env`).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use ariadna_process::EnvPolicy;

use crate::models::ProviderId;

/// Lista blanca común a todos los proveedores (spec §5.3, RF-IA3).
pub const COMMON_ALLOW: &[&str] = &[
    "PATH",
    "SystemRoot",
    "windir",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "HOME",
    "APPDATA",
    "LOCALAPPDATA",
    "ProgramData",
    "ComSpec",
    "PATHEXT",
    "LANG",
    "LC_*",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "SSL_CERT_FILE",
    "NODE_EXTRA_CA_CERTS",
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
];

/// Variables que se eliminan siempre: harían que el CLI use una API key o un
/// endpoint ajeno en lugar de la sesión de suscripción del usuario.
pub const ALWAYS_REMOVE: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
];

/// Variable que aísla la configuración (y el login) de cada proveedor, como dato.
///
/// Claude y Codex: verificado en la documentación de los CLI (spec §5.1.1).
/// Copilot: `None` = no verificado; comparte login entre perfiles (hay que avisar).
pub fn config_dir_var(provider: ProviderId) -> Option<&'static str> {
    match provider {
        ProviderId::Claude => Some("CLAUDE_CONFIG_DIR"),
        ProviderId::Codex => Some("CODEX_HOME"),
        // SPIKE (no verificado): Copilot no tiene variable de aislamiento confirmada.
        ProviderId::Copilot => None,
    }
}

/// Candidata para Copilot que **no** se usa: `COPILOT_HOME` (y `--config-dir`) aparecen en
/// referencias del CLI, pero no se han podido comprobar (no hay `copilot` instalado). Mientras
/// no se verifique, Copilot comparte login entre perfiles y la interfaz lo avisa.
pub const COPILOT_HOME_CANDIDATE_UNVERIFIED: &str = "COPILOT_HOME";

/// Cómo se aísla el login de un proveedor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Isolation {
    /// Variable que fija el directorio de configuración; `None` = no se puede aislar.
    pub var: Option<&'static str>,
    /// `true` si se ha comprobado ejecutando el CLI real en esta máquina.
    pub verified: bool,
}

/// Aislamiento disponible. Claude: **verificado** (`CLAUDE_CONFIG_DIR` cambia `configDirectory`
/// y deja `loggedIn: false` en un directorio vacío, claude 2.1.319). Codex: `CODEX_HOME` según
/// la documentación de OpenAI, **no verificado** (CLI no instalado). Copilot: sin mecanismo
/// confirmado.
pub fn isolation(provider: ProviderId) -> Isolation {
    Isolation {
        var: config_dir_var(provider),
        verified: provider == ProviderId::Claude,
    }
}

/// `true` si el proveedor no permite aislar el login por perfil.
pub fn shares_login_between_profiles(provider: ProviderId) -> bool {
    config_dir_var(provider).is_none()
}

/// Opciones de entorno de una llamada.
#[derive(Debug, Clone, Default)]
pub struct EnvOptions {
    /// Directorio de configuración aislado del perfil, si lo hay.
    pub config_dir: Option<PathBuf>,
    /// Nombre de la variable donde el usuario **declara** su token de GitHub para
    /// Copilot (p. ej. `GH_TOKEN`). Solo se usa para Copilot; sin declaración no pasa ninguna.
    pub copilot_token_var: Option<String>,
}

/// Política y variables fijas para lanzar a `provider`.
#[derive(Debug, Clone)]
pub struct ProviderEnv {
    pub policy: EnvPolicy,
    /// Se fijan después de aplicar la política (ganan siempre).
    pub extra: Vec<(OsString, OsString)>,
}

/// Construye la política de entorno del proveedor.
pub fn provider_env(provider: ProviderId, opts: &EnvOptions) -> ProviderEnv {
    let mut allow: Vec<String> = COMMON_ALLOW.iter().map(|s| (*s).to_string()).collect();
    if provider == ProviderId::Copilot {
        if let Some(var) = opts.copilot_token_var.as_deref().filter(|v| !v.is_empty()) {
            allow.push(var.to_string());
        }
    }
    let always_remove: Vec<String> = ALWAYS_REMOVE.iter().map(|s| (*s).to_string()).collect();

    let mut extra = Vec::new();
    if let (Some(var), Some(dir)) = (config_dir_var(provider), opts.config_dir.as_deref()) {
        extra.push((OsString::from(var), dir_os(dir)));
    }
    ProviderEnv {
        policy: EnvPolicy::Allowlist {
            allow,
            always_remove,
        },
        extra,
    }
}

fn dir_os(p: &Path) -> OsString {
    p.as_os_str().to_os_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use ariadna_process::build_env;

    fn parent() -> Vec<(OsString, OsString)> {
        [
            ("Path", "/bin"),
            ("HOME", "/h"),
            ("lc_all", "C"),
            ("ANTHROPIC_API_KEY", "sk-secreto"),
            ("anthropic_auth_token", "t"),
            ("OPENAI_API_KEY", "o"),
            ("OPENAI_BASE_URL", "u"),
            ("CLAUDE_CODE_USE_BEDROCK", "1"),
            ("GITHUB_TOKEN", "ghp_x"),
            ("GH_TOKEN", "ghp_y"),
            ("AWS_SECRET_ACCESS_KEY", "aws"),
            ("MI_SECRETO", "s"),
            ("HTTPS_PROXY", "http://p"),
        ]
        .iter()
        .map(|(k, v)| (OsString::from(k), OsString::from(v)))
        .collect()
    }

    fn names(v: &[(OsString, OsString)]) -> Vec<String> {
        let mut n: Vec<String> = v
            .iter()
            .map(|(k, _)| k.to_string_lossy().to_ascii_uppercase())
            .collect();
        n.sort();
        n
    }

    #[test]
    fn solo_pasa_la_lista_blanca() {
        let pe = provider_env(ProviderId::Claude, &EnvOptions::default());
        let out = build_env(parent(), &pe.policy, &pe.extra);
        assert_eq!(names(&out), ["HOME", "HTTPS_PROXY", "LC_ALL", "PATH"]);
    }

    #[test]
    fn variables_prohibidas_nunca_llegan_ni_siendo_allowlisted() {
        // Aunque alguien las añadiera a la lista blanca, always_remove gana.
        let pe = provider_env(ProviderId::Codex, &EnvOptions::default());
        let EnvPolicy::Allowlist {
            mut allow,
            always_remove,
        } = pe.policy
        else {
            panic!("debe ser Allowlist");
        };
        allow.extend(ALWAYS_REMOVE.iter().map(|s| (*s).to_string()));
        let policy = EnvPolicy::Allowlist {
            allow,
            always_remove,
        };
        let out = build_env(parent(), &policy, &[]);
        for banned in ALWAYS_REMOVE {
            assert!(!names(&out).contains(&(*banned).to_string()), "{banned}");
        }
    }

    #[test]
    fn copilot_solo_recibe_token_si_el_usuario_lo_declara() {
        let sin = provider_env(ProviderId::Copilot, &EnvOptions::default());
        let out = build_env(parent(), &sin.policy, &sin.extra);
        assert!(!names(&out).contains(&"GH_TOKEN".to_string()));

        let con = provider_env(
            ProviderId::Copilot,
            &EnvOptions {
                copilot_token_var: Some("GH_TOKEN".into()),
                ..EnvOptions::default()
            },
        );
        let out = build_env(parent(), &con.policy, &con.extra);
        assert!(names(&out).contains(&"GH_TOKEN".to_string()));
        // GITHUB_TOKEN no se declaró: no pasa.
        assert!(!names(&out).contains(&"GITHUB_TOKEN".to_string()));
    }

    #[test]
    fn token_declarado_no_se_filtra_a_otros_proveedores() {
        let opts = EnvOptions {
            copilot_token_var: Some("GH_TOKEN".into()),
            ..EnvOptions::default()
        };
        let pe = provider_env(ProviderId::Claude, &opts);
        let out = build_env(parent(), &pe.policy, &pe.extra);
        assert!(!names(&out).contains(&"GH_TOKEN".to_string()));
    }

    #[test]
    fn directorio_de_config_aislado_por_proveedor() {
        let opts = EnvOptions {
            config_dir: Some(PathBuf::from("/perfiles/empresa")),
            ..EnvOptions::default()
        };
        let c = provider_env(ProviderId::Claude, &opts);
        assert_eq!(
            c.extra,
            vec![(
                OsString::from("CLAUDE_CONFIG_DIR"),
                OsString::from("/perfiles/empresa")
            )]
        );
        let x = provider_env(ProviderId::Codex, &opts);
        assert_eq!(x.extra[0].0, OsString::from("CODEX_HOME"));
        let p = provider_env(ProviderId::Copilot, &opts);
        assert!(p.extra.is_empty());
        assert!(shares_login_between_profiles(ProviderId::Copilot));
        assert!(!shares_login_between_profiles(ProviderId::Claude));
    }

    #[test]
    fn aislamiento_verificado_solo_en_claude() {
        assert_eq!(
            isolation(ProviderId::Claude),
            Isolation {
                var: Some("CLAUDE_CONFIG_DIR"),
                verified: true
            }
        );
        assert_eq!(isolation(ProviderId::Codex).var, Some("CODEX_HOME"));
        assert!(!isolation(ProviderId::Codex).verified);
        assert_eq!(isolation(ProviderId::Copilot).var, None);
        // La candidata de Copilot no se aplica nunca mientras no se verifique.
        let opts = EnvOptions {
            config_dir: Some(PathBuf::from("/x")),
            ..EnvOptions::default()
        };
        let pe = provider_env(ProviderId::Copilot, &opts);
        assert!(pe
            .extra
            .iter()
            .all(|(k, _)| k != COPILOT_HOME_CANDIDATE_UNVERIFIED));
    }

    #[test]
    fn el_directorio_aislado_pisa_el_del_padre() {
        let mut p = parent();
        p.push(("CLAUDE_CONFIG_DIR".into(), "/personal".into()));
        let pe = provider_env(
            ProviderId::Claude,
            &EnvOptions {
                config_dir: Some("/empresa".into()),
                ..EnvOptions::default()
            },
        );
        let out = build_env(p, &pe.policy, &pe.extra);
        let v: Vec<_> = out
            .iter()
            .filter(|(k, _)| k == "CLAUDE_CONFIG_DIR")
            .collect();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].1, OsString::from("/empresa"));
    }

    #[test]
    fn la_lista_blanca_es_exactamente_la_del_spec() {
        let expected = [
            "PATH",
            "SystemRoot",
            "windir",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "HOME",
            "APPDATA",
            "LOCALAPPDATA",
            "ProgramData",
            "ComSpec",
            "PATHEXT",
            "LANG",
            "LC_*",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "NO_PROXY",
            "SSL_CERT_FILE",
            "NODE_EXTRA_CA_CERTS",
            "CLAUDE_CONFIG_DIR",
            "CODEX_HOME",
        ];
        assert_eq!(COMMON_ALLOW, expected);
        assert_eq!(ALWAYS_REMOVE.len(), 7);
    }
}
