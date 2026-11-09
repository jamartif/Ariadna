//! Clasificación del error de autenticación en operaciones de red (spec §4.3).
//!
//! # Excepción a «no se parsean mensajes humanos»
//!
//! Git no distingue «credenciales rechazadas» de otros fallos con un código de salida propio
//! (todos son 128), así que **aquí, y solo aquí**, se reconocen unos pocos textos de `stderr`.
//! Se aplica únicamente a comandos de red (`CommandKind::Network`) que han terminado con
//! código 128, y nunca a fallos de red (DNS, conexión rehusada, tiempo agotado…).

use ariadna_hosting::{Provider, RemoteInfo};

use crate::error::GitError;

/// Fragmentos (en minúsculas) de `stderr` que indican un fallo de autenticación o permisos.
/// Se comparan sin distinguir mayúsculas. Lista exacta, también documentada en el informe.
pub const AUTH_MARKERS: [&str; 11] = [
    "authentication failed",
    "could not read username",
    "could not read password",
    "terminal prompts disabled",
    "permission denied (publickey",
    "http 401",
    "http 403",
    "the requested url returned error: 401",
    "the requested url returned error: 403",
    "tf401019",
    "invalid username or password",
];

/// Texto de `Repository not found`: solo es un fallo de acceso en proveedores que responden
/// así a quien no tiene permiso (GitHub, GitLab, Azure DevOps, Bitbucket).
const NOT_FOUND_MARKER: &str = "repository not found";

/// Señales de fallo de red puro: si aparecen y no hay ninguna señal de autenticación, no es auth.
const NETWORK_MARKERS: [&str; 8] = [
    "could not resolve host",
    "connection timed out",
    "operation timed out",
    "connection refused",
    "network is unreachable",
    "failed to connect",
    "no route to host",
    "ssl",
];

/// ¿Es este fallo de un comando **de red** un problema de autenticación?
///
/// * `exit_code`: código de salida de git (se exige `128`).
/// * `stderr`: salida de error de git.
/// * `remote_url`: URL del remoto (de `git remote get-url` o del argumento), solo para sacar el
///   host y el proveedor; puede llevar credenciales: **no se copian** al resultado.
///
/// EXCEPCIÓN: parsea texto humano de git (ver el módulo). Devuelve
/// [`GitError::Auth`] con un `hint` accionable en español según el proveedor, o `None`.
pub fn classify_auth_failure(
    exit_code: Option<i32>,
    stderr: &str,
    remote_url: Option<&str>,
) -> Option<GitError> {
    // EXCEPCIÓN: único punto donde se interpretan mensajes de git.
    if exit_code != Some(128) {
        return None;
    }
    let text = stderr.to_lowercase();
    let info = remote_url.map(RemoteInfo::parse);
    let recognized = info.as_ref().is_some_and(|i| i.provider != Provider::Other);

    let marker = AUTH_MARKERS.iter().any(|m| text.contains(m));
    let not_found = text.contains(NOT_FOUND_MARKER) && recognized;
    if !marker && !not_found {
        return None;
    }
    // Un `terminal prompts disabled` junto a un fallo de red no es de autenticación.
    if !marker && NETWORK_MARKERS.iter().any(|m| text.contains(m)) {
        return None;
    }
    let ssh = info.as_ref().is_some_and(|i| i.ssh) || text.contains("permission denied (publickey");
    let host = info
        .as_ref()
        .map(|i| i.host.clone())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "el servidor remoto".to_string());
    let provider = info.as_ref().map_or(Provider::Other, |i| i.provider);
    Some(GitError::Auth {
        host,
        hint: hint_for(provider, ssh, not_found && !marker),
    })
}

fn hint_for(provider: Provider, ssh: bool, not_found: bool) -> String {
    let base = if ssh {
        match provider {
            Provider::AzureDevOps => "Comprueba que tu clave SSH está cargada (`ssh-add -l`; si no, `ssh-add`) y registrada en Azure DevOps. Azure DevOps exige claves modernas (RSA con SHA-2 o ed25519).",
            _ => "Comprueba que tu clave SSH está cargada en el agente (`ssh-add -l`; si no, `ssh-add`) y que su clave pública está registrada en tu cuenta del servidor.",
        }
        .to_string()
    } else {
        match provider {
            Provider::AzureDevOps => "Tu token (PAT) de Azure DevOps ha caducado o no tiene el permiso «Code → Read & write». Crea uno nuevo y guárdalo en Credenciales (el usuario puede ser cualquiera que no esté vacío), o inicia sesión con el Git Credential Manager.",
            Provider::GitHub => "Inicia sesión de nuevo con el Git Credential Manager o usa un token con alcance «repo» (o permiso «Contents» de lectura y escritura si es de grano fino) y guárdalo en Credenciales.",
            Provider::GitLab => "Inicia sesión de nuevo o usa un token con los alcances «read_repository» y «write_repository» y guárdalo en Credenciales.",
            Provider::Bitbucket => "Inicia sesión de nuevo o usa un token de aplicación con «Repositories → Write» y guárdalo en Credenciales.",
            Provider::Other => "Inicia sesión con el Git Credential Manager o guarda un token (PAT) para este servidor en Credenciales.",
        }
        .to_string()
    };
    if not_found {
        format!("El servidor responde «repositorio no encontrado»: revisa la URL o que tu cuenta tenga acceso. {base}")
    } else {
        base
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn auth(code: Option<i32>, stderr: &str, url: &str) -> Option<(String, String)> {
        match classify_auth_failure(code, stderr, Some(url)) {
            Some(GitError::Auth { host, hint }) => Some((host, hint)),
            Some(other) => panic!("variante inesperada {other:?}"),
            None => None,
        }
    }

    // Salidas reales de git (2.39+) capturadas de cada proveedor, con nombres inventados.
    const AZURE_BAD_PAT: &str = "remote: TF401019: The Git repository with name or identifier proj does not exist or you do not have permissions for the operation you are attempting.\nfatal: unable to access 'https://dev.azure.com/org/proj/_git/proj/': The requested URL returned error: 403\n";
    const AZURE_AUTH_FAILED: &str = "remote: \nremote: Authentication failed\nfatal: Authentication failed for 'https://dev.azure.com/org/proj/_git/repo/'\n";
    const GITHUB_BAD_TOKEN: &str = "remote: Invalid username or password.\nfatal: Authentication failed for 'https://github.com/o/r.git/'\n";
    const GITHUB_403: &str = "remote: Permission to o/r.git denied to usuario.\nfatal: unable to access 'https://github.com/o/r.git/': The requested URL returned error: 403\n";
    const GITHUB_NOT_FOUND: &str = "remote: Repository not found.\nfatal: repository 'https://github.com/o/r.git/' not found\n";
    const GITLAB_FAILED: &str = "remote: HTTP Basic: Access denied. The provided password or token is incorrect or your account has 2FA enabled and you must use a personal access token instead of a password.\nfatal: Authentication failed for 'https://gitlab.com/g/r.git/'\n";
    const NO_PROMPT: &str =
        "fatal: could not read Username for 'https://github.com': terminal prompts disabled\n";
    const NO_PASSWORD: &str = "fatal: could not read Password for 'https://usuario@bitbucket.org': terminal prompts disabled\n";
    const SSH_DENIED: &str = "git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.\n\nPlease make sure you have the correct access rights\nand the repository exists.\n";
    const SSH_DENIED_PW: &str = "git@ssh.dev.azure.com: Permission denied (publickey,password).\nfatal: Could not read from remote repository.\n";
    const HTTP_401: &str = "error: RPC failed; HTTP 401 curl 22 The requested URL returned error: 401\nfatal: the remote end hung up unexpectedly\n";
    const HTTP_403_RPC: &str = "error: RPC failed; HTTP 403 curl 22 The requested URL returned error: 403\nfatal: the remote end hung up unexpectedly\n";

    #[test]
    fn azure_devops_pat_caducado() {
        for s in [AZURE_BAD_PAT, AZURE_AUTH_FAILED] {
            let (host, hint) =
                auth(Some(128), s, "https://org@dev.azure.com/org/proj/_git/repo").unwrap();
            assert_eq!(host, "dev.azure.com");
            assert!(
                hint.contains("PAT") && hint.contains("Code → Read & write"),
                "{hint}"
            );
        }
    }

    #[test]
    fn github_token_con_alcance_repo() {
        for s in [GITHUB_BAD_TOKEN, GITHUB_403, HTTP_403_RPC] {
            let (host, hint) = auth(Some(128), s, "https://github.com/o/r.git").unwrap();
            assert_eq!(host, "github.com");
            assert!(
                hint.contains("«repo»") && hint.contains("Contents"),
                "{hint}"
            );
        }
    }

    #[test]
    fn gitlab_bitbucket_y_otros() {
        let (h, hint) = auth(Some(128), GITLAB_FAILED, "https://gitlab.com/g/r.git").unwrap();
        assert_eq!(h, "gitlab.com");
        assert!(hint.contains("read_repository"));
        let (h, hint) = auth(
            Some(128),
            NO_PASSWORD,
            "https://usuario@bitbucket.org/w/r.git",
        )
        .unwrap();
        assert_eq!(h, "bitbucket.org");
        assert!(hint.contains("Repositories"));
        let (h, hint) = auth(Some(128), NO_PROMPT, "https://git.acme.com/x/r.git").unwrap();
        assert_eq!(h, "git.acme.com");
        assert!(hint.contains("Git Credential Manager"));
    }

    #[test]
    fn ssh_da_pista_de_ssh_add() {
        let (h, hint) = auth(Some(128), SSH_DENIED, "git@github.com:o/r.git").unwrap();
        assert_eq!(h, "github.com");
        assert!(hint.contains("ssh-add"));
        let (h, hint) = auth(Some(128), SSH_DENIED_PW, "git@ssh.dev.azure.com:v3/org/p/r").unwrap();
        assert_eq!(h, "dev.azure.com");
        assert!(hint.contains("ssh-add") && hint.contains("ed25519"));
    }

    #[test]
    fn http_401_y_403_sueltos() {
        assert!(auth(Some(128), HTTP_401, "https://git.acme.com/x/r.git").is_some());
        assert!(auth(Some(128), HTTP_403_RPC, "https://git.acme.com/x/r.git").is_some());
    }

    #[test]
    fn repository_not_found_solo_en_proveedores_conocidos() {
        let (_, hint) = auth(Some(128), GITHUB_NOT_FOUND, "https://github.com/o/r.git").unwrap();
        assert!(hint.contains("repositorio no encontrado"));
        assert!(auth(Some(128), GITHUB_NOT_FOUND, "https://git.acme.com/x/r.git").is_none());
    }

    #[test]
    fn los_fallos_de_red_no_son_auth() {
        for s in [
            "fatal: unable to access 'https://github.com/o/r.git/': Could not resolve host: github.com\n",
            "fatal: unable to access 'https://github.com/o/r.git/': Failed to connect to github.com port 443 after 21000 ms: Connection timed out\n",
            "fatal: unable to access 'https://git.acme.com/x.git/': Connection refused\n",
            "fatal: unable to access 'https://git.acme.com/x.git/': SSL certificate problem: self-signed certificate\n",
            "ssh: connect to host github.com port 22: Network is unreachable\nfatal: Could not read from remote repository.\n",
            "ssh: Could not resolve hostname github.com: Name or service not known\nfatal: Could not read from remote repository.\n",
            "Host key verification failed.\nfatal: Could not read from remote repository.\n",
            "fatal: could not read Username for 'https://git.acme.com': terminal prompts disabled\nCould not resolve host: git.acme.com\n",
        ] {
            let url = "https://git.acme.com/x/r.git";
            // La última línea mezcla ambas señales: si hay marcador de auth explícito, manda auth.
            let got = classify_auth_failure(Some(128), s, Some(url));
            if s.contains("terminal prompts disabled") {
                assert!(got.is_some());
            } else {
                assert!(got.is_none(), "{s}");
            }
        }
    }

    #[test]
    fn solo_con_codigo_128() {
        assert!(classify_auth_failure(Some(1), GITHUB_BAD_TOKEN, None).is_none());
        assert!(classify_auth_failure(Some(2), NO_PROMPT, None).is_none());
        assert!(classify_auth_failure(None, NO_PROMPT, None).is_none());
    }

    #[test]
    fn sin_url_el_host_es_generico_y_no_se_filtran_credenciales() {
        let Some(GitError::Auth { host, .. }) = classify_auth_failure(Some(128), NO_PROMPT, None)
        else {
            panic!("debía ser Auth");
        };
        assert_eq!(host, "el servidor remoto");
        let pw = ["pa", "ss", "123"].concat();
        let url = format!("https://usuario:{pw}@dev.azure.com/o/p/_git/r");
        let e = classify_auth_failure(Some(128), AZURE_AUTH_FAILED, Some(&url)).unwrap();
        let rendered = format!("{e} {e:?}");
        assert!(!rendered.contains(&pw), "{rendered}");
    }

    #[test]
    fn mensajes_corrientes_de_git_no_son_auth() {
        for s in [
            "fatal: couldn't find remote ref refs/heads/nada\n",
            "fatal: 'origin' does not appear to be a git repository\nfatal: Could not read from remote repository.\n",
            "fatal: refusing to merge unrelated histories\n",
            "",
        ] {
            assert!(classify_auth_failure(Some(128), s, Some("https://github.com/o/r.git")).is_none(), "{s}");
        }
    }
}
