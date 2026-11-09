//! `open_external_url`: qué enlaces puede abrir la aplicación en el navegador del sistema.
//!
//! Solo `https`, solo hosts de una lista cerrada (ayudas de «cómo crear un PAT» y similares),
//! sin credenciales en la URL y sin puertos raros. Cualquier otra cosa es un error `POLICY`.
//! La apertura en sí (plugin opener) vive en la capa Tauri; aquí solo se decide.

use url::{Host, Url};

use super::error::{ApiError, ApiResult};

/// Hosts exactos permitidos.
pub const ALLOWED_HOSTS: [&str; 8] = [
    "learn.microsoft.com",
    "dev.azure.com",
    "github.com",
    "docs.github.com",
    "gitlab.com",
    "docs.gitlab.com",
    "bitbucket.org",
    "support.atlassian.com",
];
/// Sufijos permitidos: `*.visualstudio.com` (subdominios, no el dominio raíz).
const ALLOWED_SUFFIXES: [&str; 1] = [".visualstudio.com"];

fn denied(url: &str, why: &str) -> ApiError {
    let shown: String = url.chars().take(200).collect();
    ApiError::new(
        "POLICY",
        format!("No se abre ese enlace: {why}."),
        Some(shown),
    )
}

fn host_allowed(host: &str) -> bool {
    if ALLOWED_HOSTS.contains(&host) {
        return true;
    }
    ALLOWED_SUFFIXES.iter().any(|suffix| {
        host.strip_suffix(suffix).is_some_and(|label| {
            !label.is_empty()
                && label.split('.').all(|l| {
                    !l.is_empty()
                        && !l.starts_with('-')
                        && !l.ends_with('-')
                        && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
        })
    })
}

/// Valida una URL para abrirla fuera de la aplicación y la devuelve normalizada.
pub fn validate_external_url(raw: &str) -> ApiResult<Url> {
    let raw = raw.trim();
    let url = Url::parse(raw).map_err(|_| denied(raw, "no es una dirección válida"))?;
    if url.scheme() != "https" {
        return Err(denied(raw, "solo se abren enlaces https"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(denied(raw, "el enlace lleva credenciales"));
    }
    if url.port().is_some_and(|p| p != 443) {
        return Err(denied(raw, "el enlace usa un puerto no estándar"));
    }
    let Some(Host::Domain(host)) = url.host() else {
        return Err(denied(raw, "el enlace no apunta a un nombre de dominio"));
    };
    if !host_allowed(host) {
        return Err(denied(raw, "ese sitio no está en la lista permitida"));
    }
    Ok(url)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn acepta_los_hosts_de_la_lista() {
        for u in [
            "https://learn.microsoft.com/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate",
            "https://dev.azure.com/org/_usersSettings/tokens",
            "https://miorg.visualstudio.com/_usersSettings/tokens",
            "https://a.b.visualstudio.com/x",
            "https://github.com/settings/tokens",
            "https://docs.github.com/es",
            "https://gitlab.com/-/user_settings/personal_access_tokens",
            "https://docs.gitlab.com/ee/user/profile/personal_access_tokens.html",
            "https://bitbucket.org/account/settings/app-passwords/",
            "https://support.atlassian.com/bitbucket-cloud/",
            "https://GitHub.com/x",
            "https://github.com:443/x",
            "  https://github.com/x  ",
        ] {
            assert!(validate_external_url(u).is_ok(), "{u}");
        }
    }

    #[test]
    fn rechaza_http_y_esquemas_raros() {
        for u in [
            "http://github.com/x",
            "ftp://github.com/x",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<script>1</script>",
            "ssh://git@github.com/x",
            "mailto:a@b.c",
            "vscode://file/x",
            "ms-msdt:/id",
            "\\\\servidor\\recurso",
            "github.com/x",
            "",
            "https://",
        ] {
            let e = validate_external_url(u).unwrap_err();
            assert_eq!(e.code, "POLICY", "{u}");
        }
    }

    #[test]
    fn rechaza_otros_hosts_y_trucos() {
        for u in [
            "https://evil.com/",
            "https://github.com.evil.com/",
            "https://evilgithub.com/",
            "https://notgithub.com/",
            "https://github.com@evil.com/",
            "https://user:pw@github.com/",
            "https://visualstudio.com/",
            "https://evilvisualstudio.com/",
            "https://x.visualstudio.com.evil.com/",
            "https://.visualstudio.com/",
            "https://a..visualstudio.com/",
            "https://github.com./x",
            "https://github.com:8443/x",
            "https://127.0.0.1/",
            "https://[::1]/",
            "https://localhost/",
            "https://gіthub.com/",
            "https://github.com%2eevil.com/",
        ] {
            let e = validate_external_url(u).unwrap_err();
            assert_eq!(e.code, "POLICY", "{u}");
        }
    }

    #[test]
    fn el_error_no_se_pasa_de_largo_con_urls_enormes() {
        let u = format!("http://x/{}", "a".repeat(5000));
        let e = validate_external_url(&u).unwrap_err();
        assert!(e.detail.unwrap().chars().count() <= 200);
    }
}
