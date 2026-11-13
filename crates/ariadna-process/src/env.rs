use std::ffi::{OsStr, OsString};

/// Política de entorno de un proceso hijo. Los nombres se comparan **sin
/// distinguir mayúsculas** en todas las plataformas.
///
/// Un patrón que termina en `*` es un prefijo (`GIT_*`); si no, es un nombre exacto.
#[derive(Debug, Clone)]
pub enum EnvPolicy {
    /// Lista blanca de nombres, y de las que pasan se quitan además las que
    /// contengan alguna subcadena de aspecto secreto (`TOKEN`, `PASSWORD`…).
    /// Es el modelo del `ProcessEnvironment` del Swift: solo entra lo conocido,
    /// y un `GIT_*`/`SSH_*` con pinta de token se elimina igualmente.
    AllowlistSecretFiltered {
        allow: Vec<String>,
        deny_substrings: Vec<String>,
    },
    /// Solo pasan las variables de la lista blanca, y de esas se quitan las de
    /// `always_remove` aunque coincidan.
    Allowlist {
        allow: Vec<String>,
        always_remove: Vec<String>,
    },
}

impl EnvPolicy {
    /// Entorno para `git`, equivalente al del Swift (`ProcessEnvironment.sanitized`):
    /// los nombres de siempre, `GIT_*` y `SSH_*`, y las variables que Windows exige
    /// para que `git` y el Git Credential Manager funcionen. Fuera cualquier
    /// variable con `TOKEN`, `SECRET`, `PASSWORD`, `PASSWD` o `API_KEY` en el nombre.
    pub fn git_default() -> Self {
        let allow = [
            // Comunes (heredadas del Swift)
            "PATH",
            "HOME",
            "USER",
            "LOGNAME",
            "LANG",
            "LC_*",
            "TERM",
            "TMPDIR",
            "TMP",
            "TEMP",
            "SHELL",
            "SSH_*",
            "GIT_*",
            "DISPLAY",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            // Windows: sin ellas fallan Winsock/DNS, las rutas de usuario y los scripts de git
            "SystemRoot",
            "SystemDrive",
            "windir",
            "USERPROFILE",
            "USERNAME",
            "HOMEDRIVE",
            "HOMEPATH",
            "APPDATA",
            "LOCALAPPDATA",
            "ProgramData",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ComSpec",
            "PATHEXT",
            "OS",
            "COMPUTERNAME",
            "PROCESSOR_ARCHITECTURE",
            // Git Credential Manager
            "GCM_*",
        ];
        EnvPolicy::AllowlistSecretFiltered {
            allow: allow.map(String::from).to_vec(),
            deny_substrings: ["TOKEN", "SECRET", "PASSWORD", "PASSWD", "API_KEY"]
                .map(String::from)
                .to_vec(),
        }
    }
}

/// `true` si `name` coincide con `pattern` (exacto o prefijo con `*`), sin
/// distinguir mayúsculas.
pub fn name_matches(pattern: &str, name: &str) -> bool {
    let (p, n) = (pattern.to_ascii_uppercase(), name.to_ascii_uppercase());
    match p.strip_suffix('*') {
        Some(prefix) => n.starts_with(prefix),
        None => p == n,
    }
}

fn any_match(patterns: &[String], name: &str) -> bool {
    patterns.iter().any(|p| name_matches(p, name))
}

/// Aplica la política al entorno `parent` y luego fija `extra` (que gana siempre).
/// Los nombres que no son UTF-8 válido se descartan: no se pueden evaluar.
pub fn build_env<I>(
    parent: I,
    policy: &EnvPolicy,
    extra: &[(OsString, OsString)],
) -> Vec<(OsString, OsString)>
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    let mut out: Vec<(OsString, OsString)> = parent
        .into_iter()
        .filter(|(k, _)| {
            let Some(name) = k.to_str() else { return false };
            match policy {
                EnvPolicy::AllowlistSecretFiltered {
                    allow,
                    deny_substrings,
                } => {
                    let upper = name.to_ascii_uppercase();
                    let secret = deny_substrings
                        .iter()
                        .any(|d| upper.contains(&d.to_ascii_uppercase()));
                    any_match(allow, name) && !secret
                }
                EnvPolicy::Allowlist {
                    allow,
                    always_remove,
                } => any_match(allow, name) && !any_match(always_remove, name),
            }
        })
        .collect();

    for (k, v) in extra {
        out.retain(|(ek, _)| !eq_ignore_case(ek, k));
        out.push((k.clone(), v.clone()));
    }
    out
}

fn eq_ignore_case(a: &OsStr, b: &OsStr) -> bool {
    match (a.to_str(), b.to_str()) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => a == b,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn e(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs.iter().map(|(k, v)| (k.into(), v.into())).collect()
    }
    fn names(v: &[(OsString, OsString)]) -> Vec<String> {
        let mut n: Vec<String> = v
            .iter()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        n.sort();
        n
    }

    #[test]
    fn git_default_es_lista_blanca_y_quita_secretos() {
        let parent = e(&[
            ("PATH", "x"),
            ("GITHUB_TOKEN", "t"),
            ("GIT_TOKEN", "t"),
            ("GIT_HTTP_PASSWORD", "p"),
            ("my_Api_Key", "k"),
            ("DB_PASSWORD", "p"),
            ("RANDOM_THING", "r"),
            ("SSH_AUTH_SOCK", "s"),
            ("SSH_ASKPASS_PASSWORD", "s"),
            ("GIT_ASKPASS", "a"),
            ("GCM_CREDENTIAL_STORE", "c"),
            ("HOME", "h"),
            ("SystemRoot", "C:\\Windows"),
            ("Path", "y"),
        ]);
        let out = build_env(parent, &EnvPolicy::git_default(), &[]);
        assert_eq!(
            names(&out),
            [
                "GCM_CREDENTIAL_STORE",
                "GIT_ASKPASS",
                "HOME",
                "PATH",
                "Path",
                "SSH_AUTH_SOCK",
                "SystemRoot"
            ]
        );
    }

    #[test]
    fn allowlist_ignora_mayusculas_y_prefijos() {
        let policy = EnvPolicy::Allowlist {
            allow: vec!["PATH".into(), "LC_*".into(), "ANTHROPIC_API_KEY".into()],
            always_remove: vec!["ANTHROPIC_API_KEY".into()],
        };
        let parent = e(&[
            ("Path", "p"),
            ("lc_all", "C"),
            ("ANTHROPIC_API_KEY", "k"),
            ("OTRA", "o"),
        ]);
        let out = build_env(parent, &policy, &[]);
        assert_eq!(names(&out), ["Path", "lc_all"]);
    }

    #[test]
    fn extra_gana_y_reemplaza_sin_distinguir_mayusculas() {
        let policy = EnvPolicy::Allowlist {
            allow: vec!["PATH".into()],
            always_remove: vec![],
        };
        let out = build_env(e(&[("Path", "viejo")]), &policy, &e(&[("PATH", "nuevo")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1, OsString::from("nuevo"));
    }

    #[test]
    fn name_matches_basico() {
        assert!(name_matches("GIT_*", "git_dir"));
        assert!(!name_matches("GIT_*", "MYGIT_X"));
        assert!(name_matches("path", "PATH"));
    }
}
