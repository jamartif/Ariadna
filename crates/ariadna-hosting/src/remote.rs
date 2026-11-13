//! Reconocimiento del proveedor a partir de la URL de un remoto (spec §4.3).
//!
//! Todo es **puro**: sin E/S, sin red y sin pánicos con entrada arbitraria. Las credenciales
//! embebidas (`user:pass@`) se descartan siempre; nunca llegan a ningún campo del resultado.

use serde::{Deserialize, Serialize};

/// Hosts a los que puede apuntar [`PatHelp::url`] (coincide con la lista de `open_external_url`
/// del contrato IPC; `*.visualstudio.com` no hace falta aquí porque la ayuda va a `learn.microsoft.com`).
pub const ALLOWED_HELP_HOSTS: [&str; 4] = [
    "learn.microsoft.com",
    "docs.github.com",
    "docs.gitlab.com",
    "support.atlassian.com",
];

/// Proveedor de alojamiento reconocido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    #[serde(rename = "azureDevOps")]
    AzureDevOps,
    #[serde(rename = "github")]
    GitHub,
    #[serde(rename = "gitlab")]
    GitLab,
    #[serde(rename = "bitbucket")]
    Bitbucket,
    #[serde(rename = "other")]
    Other,
}

impl Provider {
    /// Etiqueta para la UI.
    pub fn label(self) -> &'static str {
        match self {
            Provider::AzureDevOps => "Azure DevOps",
            Provider::GitHub => "GitHub",
            Provider::GitLab => "GitLab",
            Provider::Bitbucket => "Bitbucket",
            Provider::Other => "Otro",
        }
    }

    /// `true` si las pull requests se gestionan dentro de la app (§4.4: Azure DevOps y GitHub).
    pub fn supports_in_app_pull_requests(self) -> bool {
        matches!(self, Provider::AzureDevOps | Provider::GitHub)
    }
}

/// Pistas configurables para reconocer servidores propios (GitHub Enterprise, GitLab
/// autoalojado, Bitbucket Server). Los nombres se comparan sin distinguir mayúsculas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostHints {
    /// Hosts que son GitHub Enterprise (p. ej. `git.acme.com`).
    pub github_hosts: Vec<String>,
    /// Hosts que son GitLab autoalojado.
    pub gitlab_hosts: Vec<String>,
    /// Hosts que son Bitbucket (Server/Data Center).
    pub bitbucket_hosts: Vec<String>,
    /// Si es `true`, un host cuyo primer rótulo empieza por `github`, `gitlab` o `bitbucket`
    /// (`github.acme.com`) se reconoce solo; `*.ghe.com` es siempre GitHub.
    pub heuristics: bool,
}

impl Default for HostHints {
    fn default() -> Self {
        Self {
            github_hosts: Vec::new(),
            gitlab_hosts: Vec::new(),
            bitbucket_hosts: Vec::new(),
            heuristics: true,
        }
    }
}

fn host_in(list: &[String], host: &str) -> bool {
    list.iter().any(|h| h.trim().eq_ignore_ascii_case(host))
}

/// Ayuda para crear un token de acceso personal (PAT) en un proveedor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatHelp {
    /// Página `https` de creación de tokens (solo hosts de [`ALLOWED_HELP_HOSTS`]). `None` si el
    /// proveedor no se reconoce.
    pub url: Option<String>,
    /// Permisos mínimos, en español.
    pub permissions: String,
    /// Nota sobre el nombre de usuario, si procede.
    pub username_note: Option<String>,
}

/// Qué remoto es: proveedor, host, propietario y repositorio. Se obtiene con
/// [`RemoteInfo::parse`], que **nunca falla**: lo irreconocible sale como [`Provider::Other`]
/// con campos vacíos.
///
/// * `host`: sin puerto, en minúsculas. En Azure DevOps se normaliza a `dev.azure.com`
///   (también para `ssh.dev.azure.com`, `*.visualstudio.com` y `vs-ssh.visualstudio.com`), de
///   modo que las reglas de perfil `dev.azure.com/mi-org` valgan para todas las formas.
/// * `owner`: usuario/organización/espacio de trabajo; en GitLab incluye los subgrupos
///   (`g/sub`); en Azure DevOps es la organización.
/// * `project`: solo Azure DevOps. Si la URL no lo trae (`.../org/_git/repo`), es el repo.
/// * `web_url`: página del repo en el navegador (`https`); `None` para rutas locales o si no se
///   pudo determinar.
/// * `ssh`: la URL usa SSH (`ssh://` o `git@host:ruta`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfo {
    pub provider: Provider,
    pub host: String,
    pub owner: String,
    pub project: Option<String>,
    pub repo: String,
    pub web_url: Option<String>,
    pub ssh: bool,
}

// ---------------------------------------------------------------------------------------
// Análisis sintáctico bruto
// ---------------------------------------------------------------------------------------

/// URL descompuesta, antes de decidir el proveedor.
#[derive(Debug, Default)]
struct Raw {
    host: String,
    segs: Vec<String>,
    ssh: bool,
    local: bool,
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out)
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

fn percent_encode(s: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~');
        if unreserved || (keep_slash && b == b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn split_segments(path: &str) -> Vec<String> {
    let path = path.split(['?', '#']).next().unwrap_or("");
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(percent_decode)
        .filter(|s| !s.is_empty())
        .collect()
}

fn valid_host(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 253
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// `host[:puerto]` → host en minúsculas, sin puerto ni punto final. `None` si no es válido.
fn parse_host(hostport: &str) -> Option<String> {
    let h = if let Some(rest) = hostport.strip_prefix('[') {
        // IPv6: se acepta el literal, pero no se reconoce ningún proveedor con él.
        let end = rest.find(']')?;
        let inner = &rest[..end];
        if inner.is_empty() || !inner.chars().all(|c| c.is_ascii_hexdigit() || c == ':') {
            return None;
        }
        return Some(format!("[{}]", inner.to_ascii_lowercase()));
    } else {
        match hostport.rsplit_once(':') {
            Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h,
            Some(_) => return None,
            None => hostport,
        }
    };
    let h = h.trim_end_matches('.').to_ascii_lowercase();
    valid_host(&h).then_some(h)
}

fn is_drive_path(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b.len() == 2 || matches!(b[2], b'/' | b'\\'))
}

fn local_raw(s: &str) -> Raw {
    let segs = s
        .split(['/', '\\'])
        .filter(|p| !p.is_empty())
        .map(|p| p.chars().filter(|c| !c.is_control()).collect::<String>())
        .filter(|p| !p.is_empty() && p != "." && p != ".." && !is_drive_path(p))
        .collect();
    Raw {
        local: true,
        segs,
        ..Raw::default()
    }
}

fn parse_raw(input: &str) -> Raw {
    let s = input.trim();
    if s.is_empty() || s.chars().any(char::is_control) {
        return Raw::default();
    }
    // scheme://…
    if let Some((scheme, rest)) = s.split_once("://") {
        let valid_scheme = scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
        if !valid_scheme {
            return Raw::default();
        }
        let scheme = scheme.to_ascii_lowercase();
        if scheme == "file" {
            return local_raw(&percent_decode(rest));
        }
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        let authority = authority.split(['?', '#']).next().unwrap_or("");
        // Un `\` en la autoridad es un truco clásico para confundir al host real.
        if authority.contains('\\') || authority.chars().any(char::is_whitespace) {
            return Raw::default();
        }
        let hostport = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        let Some(host) = parse_host(hostport) else {
            return Raw::default();
        };
        return Raw {
            host,
            segs: split_segments(path),
            ssh: scheme == "ssh" || scheme == "git+ssh" || scheme == "ssh+git",
            local: false,
        };
    }
    // Rutas locales evidentes.
    let local_like = s.starts_with(['/', '\\', '~', '.']) || is_drive_path(s);
    if !local_like {
        // scp-like: [user@]host:ruta (sin `/` antes de los dos puntos).
        let colon = if s.starts_with('[') {
            s.find(']').and_then(|i| s[i..].find(':').map(|j| i + j))
        } else {
            s.find(':')
        };
        if let Some(c) = colon {
            let (head, path) = (&s[..c], &s[c + 1..]);
            if !head.contains(['/', '\\']) && !head.is_empty() {
                // `ext::cmd` y similares (`host::ruta`) no son scp-like: basura.
                if path.starts_with(':') {
                    return Raw::default();
                }
                let hostpart = head.rsplit_once('@').map_or(head, |(_, h)| h);
                if let Some(host) = parse_host(hostpart) {
                    return Raw {
                        host,
                        segs: split_segments(path),
                        ssh: true,
                        local: false,
                    };
                }
                return Raw::default();
            }
        }
    }
    local_raw(s)
}

fn strip_git(name: &str) -> String {
    let n = name.trim();
    let lower = n.to_ascii_lowercase();
    if lower.ends_with(".git") && n.len() > 4 {
        n[..n.len() - 4].to_string()
    } else {
        n.to_string()
    }
}

fn empty_info(ssh: bool) -> RemoteInfo {
    RemoteInfo {
        provider: Provider::Other,
        host: String::new(),
        owner: String::new(),
        project: None,
        repo: String::new(),
        web_url: None,
        ssh,
    }
}

fn first_label(host: &str) -> &str {
    host.split('.').next().unwrap_or("")
}

// ---------------------------------------------------------------------------------------
// Clasificación
// ---------------------------------------------------------------------------------------

fn azure(raw: &Raw) -> Option<RemoteInfo> {
    let h = raw.host.as_str();
    let (org, project, repo);
    if h == "ssh.dev.azure.com" || h.ends_with("vs-ssh.visualstudio.com") {
        // git@ssh.dev.azure.com:v3/{org}/{project}/{repo}
        let s = &raw.segs;
        if s.len() < 4 || !s[0].eq_ignore_ascii_case("v3") {
            return None;
        }
        org = s[1].clone();
        project = s[2].clone();
        repo = strip_git(&s[3]);
    } else if h == "dev.azure.com" || h.ends_with(".visualstudio.com") {
        let idx = raw
            .segs
            .iter()
            .position(|s| s.eq_ignore_ascii_case("_git"))?;
        let repo_seg = raw.segs.get(idx + 1)?;
        repo = strip_git(repo_seg);
        let before: Vec<&String> = raw.segs[..idx]
            .iter()
            .filter(|s| !s.eq_ignore_ascii_case("DefaultCollection"))
            .collect();
        if h == "dev.azure.com" {
            org = (*before.first()?).clone();
            project = before.get(1).map_or_else(|| repo.clone(), |p| (*p).clone());
        } else {
            let label = h.strip_suffix(".visualstudio.com")?;
            if label.is_empty() || label.contains('.') {
                return None;
            }
            org = label.to_string();
            project = before
                .first()
                .map_or_else(|| repo.clone(), |p| (*p).clone());
        }
    } else {
        return None;
    }
    if org.is_empty() || project.is_empty() || repo.is_empty() {
        return None;
    }
    let web = format!(
        "https://dev.azure.com/{}/{}/_git/{}",
        percent_encode(&org, false),
        percent_encode(&project, false),
        percent_encode(&repo, false)
    );
    Some(RemoteInfo {
        provider: Provider::AzureDevOps,
        host: "dev.azure.com".into(),
        owner: org,
        project: Some(project),
        repo,
        web_url: Some(web),
        ssh: raw.ssh,
    })
}

fn web_from(host: &str, owner: &str, repo: &str) -> Option<String> {
    if host.is_empty() || host.contains(['[', ':']) || owner.is_empty() || repo.is_empty() {
        return None;
    }
    let owner_enc: Vec<String> = owner.split('/').map(|p| percent_encode(p, false)).collect();
    Some(format!(
        "https://{host}/{}/{}",
        owner_enc.join("/"),
        percent_encode(repo, false)
    ))
}

fn classify(raw: Raw, hints: &HostHints) -> RemoteInfo {
    if raw.host.is_empty() && !raw.local {
        return empty_info(raw.ssh);
    }
    if raw.local {
        let mut it = raw.segs.iter().rev();
        let repo = it.next().map(|s| strip_git(s)).unwrap_or_default();
        let owner = it.next().cloned().unwrap_or_default();
        return RemoteInfo {
            repo,
            owner,
            ..empty_info(false)
        };
    }
    if let Some(info) = azure(&raw) {
        return info;
    }
    let mut host = raw.host.clone();
    let label = first_label(&host).to_string();
    // La heurística nunca reconoce hosts que contienen el dominio de un proveedor conocido sin
    // serlo (`github.com.evil.com`); las pistas explícitas del usuario sí mandan.
    let heur = hints.heuristics
        && !["github.com", "gitlab.com", "bitbucket.org", "dev.azure.com"]
            .iter()
            .any(|d| host.contains(d));
    let is_github = host == "github.com"
        || host == "www.github.com"
        || host == "ssh.github.com"
        || host.ends_with(".ghe.com")
        || host_in(&hints.github_hosts, &host)
        || (heur && label.starts_with("github") && host.contains('.'));
    let is_gitlab = !is_github
        && (host == "gitlab.com"
            || host_in(&hints.gitlab_hosts, &host)
            || (heur && label.starts_with("gitlab") && host.contains('.')));
    let is_bitbucket = !is_github
        && !is_gitlab
        && (host == "bitbucket.org"
            || host_in(&hints.bitbucket_hosts, &host)
            || (heur && label.starts_with("bitbucket") && host.contains('.')));

    let segs = &raw.segs;
    let (provider, owner, repo) = if is_github && segs.len() >= 2 {
        if host == "ssh.github.com" || host == "www.github.com" {
            host = "github.com".into();
        }
        (Provider::GitHub, segs[0].clone(), strip_git(&segs[1]))
    } else if is_gitlab && segs.len() >= 2 {
        // Las URLs web llevan `/-/…` tras el repo; se corta ahí.
        let end = segs.iter().position(|s| s == "-").unwrap_or(segs.len());
        let s = &segs[..end];
        if s.len() >= 2 {
            let repo = strip_git(&s[s.len() - 1]);
            (Provider::GitLab, s[..s.len() - 1].join("/"), repo)
        } else {
            return generic(raw, host);
        }
    } else if is_bitbucket {
        let s: Vec<&String> = if segs.first().is_some_and(|s| s == "scm") {
            segs.iter().skip(1).collect()
        } else {
            segs.iter().collect()
        };
        if s.len() >= 2 {
            (Provider::Bitbucket, s[0].clone(), strip_git(s[1]))
        } else {
            return generic(raw, host);
        }
    } else {
        return generic(raw, host);
    };
    if owner.is_empty() || repo.is_empty() {
        return generic(raw, host);
    }
    // Bitbucket Server tiene otra estructura web: no se adivina.
    let web_url = if provider == Provider::Bitbucket && host != "bitbucket.org" {
        None
    } else {
        web_from(&host, &owner, &repo)
    };
    RemoteInfo {
        provider,
        host,
        owner,
        project: None,
        repo,
        web_url,
        ssh: raw.ssh,
    }
}

fn generic(raw: Raw, host: String) -> RemoteInfo {
    let mut it = raw.segs.iter().rev();
    let repo = it.next().map(|s| strip_git(s)).unwrap_or_default();
    let mut owner: Vec<&String> = it.collect();
    owner.reverse();
    let owner = owner
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join("/");
    let web_url = web_from(&host, &owner, &repo);
    RemoteInfo {
        provider: Provider::Other,
        host,
        owner,
        project: None,
        repo,
        web_url,
        ssh: raw.ssh,
    }
}

impl RemoteInfo {
    /// Reconoce el proveedor con las pistas por defecto ([`HostHints::default`]).
    ///
    /// Función pura que **nunca falla ni entra en pánico**: con basura devuelve
    /// [`Provider::Other`] con campos vacíos. Quita siempre las credenciales embebidas.
    pub fn parse(url: &str) -> RemoteInfo {
        Self::parse_with(url, &HostHints::default())
    }

    /// Como [`parse`](Self::parse) con pistas para servidores propios.
    pub fn parse_with(url: &str, hints: &HostHints) -> RemoteInfo {
        classify(parse_raw(url), hints)
    }

    /// `true` si hay host y repositorio (una URL de repo completa y no local).
    pub fn is_complete(&self) -> bool {
        !self.host.is_empty() && !self.repo.is_empty()
    }

    /// Página del repositorio en el navegador (`https`).
    pub fn repo_web_url(&self) -> Option<String> {
        self.web_url.clone()
    }

    /// Página de «nueva pull request» desde `branch`. `None` si el proveedor es `other`, la rama
    /// está vacía o lleva caracteres de control, o no hay URL web.
    pub fn new_pull_request_url(&self, branch: &str) -> Option<String> {
        let branch = branch.trim();
        if branch.is_empty() || branch.chars().any(char::is_control) {
            return None;
        }
        let base = self.web_url.as_deref()?;
        Some(match self.provider {
            Provider::GitHub => format!("{base}/pull/new/{}", percent_encode(branch, true)),
            Provider::AzureDevOps => format!(
                "{base}/pullrequestcreate?sourceRef={}",
                percent_encode(branch, false)
            ),
            Provider::GitLab => format!(
                "{base}/-/merge_requests/new?merge_request[source_branch]={}",
                percent_encode(branch, false)
            ),
            Provider::Bitbucket => format!(
                "{base}/pull-requests/new?source={}",
                percent_encode(branch, false)
            ),
            Provider::Other => return None,
        })
    }

    /// Página de la rama `branch` en el navegador (`https`). `None` si el proveedor es `other`,
    /// la rama está vacía o lleva caracteres de control, o no hay URL web.
    pub fn branch_web_url(&self, branch: &str) -> Option<String> {
        let branch = branch.trim();
        if branch.is_empty() || branch.chars().any(char::is_control) {
            return None;
        }
        let base = self.web_url.as_deref()?;
        Some(match self.provider {
            Provider::GitHub => format!("{base}/tree/{}", percent_encode(branch, true)),
            Provider::AzureDevOps => {
                format!("{base}?version=GB{}", percent_encode(branch, false))
            }
            Provider::GitLab => format!("{base}/-/tree/{}", percent_encode(branch, true)),
            Provider::Bitbucket => format!("{base}/branch/{}", percent_encode(branch, true)),
            Provider::Other => return None,
        })
    }

    /// Cómo crear un token de acceso personal en este proveedor y qué permisos pedirle.
    pub fn pat_help(&self) -> PatHelp {
        match self.provider {
            Provider::AzureDevOps => PatHelp {
                url: Some("https://learn.microsoft.com/es-es/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate".into()),
                permissions: "Code → Read & write (lectura y escritura de código).".into(),
                username_note: Some(
                    "El nombre de usuario puede ser cualquiera que no esté vacío; lo que cuenta es el token.".into(),
                ),
            },
            Provider::GitHub => PatHelp {
                url: Some("https://docs.github.com/es/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens".into()),
                permissions: "Alcance «repo» (token clásico) o, con un token de grano fino, permiso «Contents» de lectura y escritura sobre el repositorio.".into(),
                username_note: Some("Usa tu nombre de usuario de GitHub.".into()),
            },
            Provider::GitLab => PatHelp {
                url: Some("https://docs.gitlab.com/user/profile/personal_access_tokens/".into()),
                permissions: "Alcances «read_repository» y «write_repository».".into(),
                username_note: Some("Usa tu nombre de usuario de GitLab.".into()),
            },
            Provider::Bitbucket => PatHelp {
                url: Some("https://support.atlassian.com/bitbucket-cloud/docs/using-app-passwords/".into()),
                permissions: "Token de aplicación con «Repositories → Write» (lectura y escritura de repositorios).".into(),
                username_note: Some("Usa tu nombre de usuario de Bitbucket (no el correo).".into()),
            },
            Provider::Other => PatHelp {
                url: None,
                permissions: "Un token con permiso de lectura y escritura sobre el repositorio; consulta la ayuda de tu servidor.".into(),
                username_note: None,
            },
        }
    }

    /// Nombre de carpeta sugerido para clonar: el repo, saneado para Windows y macOS
    /// (sin separadores ni caracteres reservados, sin nombres reservados, nunca vacío).
    pub fn suggested_dir_name(&self) -> String {
        sanitize_dir_name(&self.repo)
    }
}

fn sanitize_dir_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim().to_string();
    let stem = cleaned.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    if cleaned.is_empty() || reserved || cleaned.chars().all(|c| c == '-') {
        "repo".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    type Row = (
        &'static str,
        Provider,
        &'static str,
        &'static str,
        Option<&'static str>,
        &'static str,
        bool,
    );

    // (url, proveedor, host, owner, project, repo, ssh)
    const TABLE: &[Row] = &[
        // GitHub
        (
            "https://github.com/o/r",
            Provider::GitHub,
            "github.com",
            "o",
            None,
            "r",
            false,
        ),
        (
            "https://github.com/o/r.git",
            Provider::GitHub,
            "github.com",
            "o",
            None,
            "r",
            false,
        ),
        (
            "https://github.com/o/r/",
            Provider::GitHub,
            "github.com",
            "o",
            None,
            "r",
            false,
        ),
        (
            "HTTPS://GitHub.COM/O/R.GIT",
            Provider::GitHub,
            "github.com",
            "O",
            None,
            "R",
            false,
        ),
        (
            "git@github.com:o/r.git",
            Provider::GitHub,
            "github.com",
            "o",
            None,
            "r",
            true,
        ),
        (
            "git@github.com:o/r",
            Provider::GitHub,
            "github.com",
            "o",
            None,
            "r",
            true,
        ),
        (
            "ssh://git@github.com/o/r.git",
            Provider::GitHub,
            "github.com",
            "o",
            None,
            "r",
            true,
        ),
        (
            "ssh://git@ssh.github.com:443/o/r.git",
            Provider::GitHub,
            "github.com",
            "o",
            None,
            "r",
            true,
        ),
        (
            "https://github.com/o/r/tree/main/src",
            Provider::GitHub,
            "github.com",
            "o",
            None,
            "r",
            false,
        ),
        (
            "https://github.com:443/o/mi.repo.git",
            Provider::GitHub,
            "github.com",
            "o",
            None,
            "mi.repo",
            false,
        ),
        (
            "https://usuario:inventada@github.com/o/r.git",
            Provider::GitHub,
            "github.com",
            "o",
            None,
            "r",
            false,
        ),
        (
            "https://usuario@github.com/o/r.git",
            Provider::GitHub,
            "github.com",
            "o",
            None,
            "r",
            false,
        ),
        // GitHub Enterprise
        (
            "https://github.acme.com/eq/app.git",
            Provider::GitHub,
            "github.acme.com",
            "eq",
            None,
            "app",
            false,
        ),
        (
            "git@github.acme.com:eq/app.git",
            Provider::GitHub,
            "github.acme.com",
            "eq",
            None,
            "app",
            true,
        ),
        (
            "https://acme.ghe.com/eq/app",
            Provider::GitHub,
            "acme.ghe.com",
            "eq",
            None,
            "app",
            false,
        ),
        // Azure DevOps
        (
            "https://dev.azure.com/org/proj/_git/repo",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("proj"),
            "repo",
            false,
        ),
        (
            "https://org@dev.azure.com/org/proj/_git/repo",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("proj"),
            "repo",
            false,
        ),
        (
            "https://org:inventada@dev.azure.com/org/proj/_git/repo",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("proj"),
            "repo",
            false,
        ),
        (
            "https://dev.azure.com/org/Mi%20Proyecto/_git/mi%20repo",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("Mi Proyecto"),
            "mi repo",
            false,
        ),
        (
            "https://dev.azure.com/org/Mi Proyecto/_git/repo",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("Mi Proyecto"),
            "repo",
            false,
        ),
        (
            "https://dev.azure.com/org/_git/repo",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("repo"),
            "repo",
            false,
        ),
        (
            "https://dev.azure.com/org/proj/_git/repo?version=GBmain",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("proj"),
            "repo",
            false,
        ),
        (
            "https://dev.azure.com/org/proj/_GIT/repo/",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("proj"),
            "repo",
            false,
        ),
        (
            "git@ssh.dev.azure.com:v3/org/proj/repo",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("proj"),
            "repo",
            true,
        ),
        (
            "git@ssh.dev.azure.com:v3/org/Mi%20Proyecto/repo",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("Mi Proyecto"),
            "repo",
            true,
        ),
        (
            "ssh://git@ssh.dev.azure.com/v3/org/proj/repo",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("proj"),
            "repo",
            true,
        ),
        (
            "https://org.visualstudio.com/proj/_git/repo",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("proj"),
            "repo",
            false,
        ),
        (
            "https://org.visualstudio.com/DefaultCollection/proj/_git/repo",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("proj"),
            "repo",
            false,
        ),
        (
            "https://org.visualstudio.com/_git/repo",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("repo"),
            "repo",
            false,
        ),
        (
            "ssh://org@vs-ssh.visualstudio.com:22/org/proj/repo",
            Provider::Other,
            "vs-ssh.visualstudio.com",
            "org/proj",
            None,
            "repo",
            true,
        ),
        (
            "ssh://org@vs-ssh.visualstudio.com:22/v3/org/proj/repo",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("proj"),
            "repo",
            true,
        ),
        (
            "org@vs-ssh.visualstudio.com:v3/org/proj/repo",
            Provider::AzureDevOps,
            "dev.azure.com",
            "org",
            Some("proj"),
            "repo",
            true,
        ),
        // GitLab
        (
            "https://gitlab.com/g/r.git",
            Provider::GitLab,
            "gitlab.com",
            "g",
            None,
            "r",
            false,
        ),
        (
            "https://gitlab.com/g/sub/r.git",
            Provider::GitLab,
            "gitlab.com",
            "g/sub",
            None,
            "r",
            false,
        ),
        (
            "git@gitlab.com:g/sub/sub2/r.git",
            Provider::GitLab,
            "gitlab.com",
            "g/sub/sub2",
            None,
            "r",
            true,
        ),
        (
            "https://gitlab.com/g/r/-/merge_requests/3",
            Provider::GitLab,
            "gitlab.com",
            "g",
            None,
            "r",
            false,
        ),
        (
            "https://gitlab.acme.com/eq/sub/r.git",
            Provider::GitLab,
            "gitlab.acme.com",
            "eq/sub",
            None,
            "r",
            false,
        ),
        (
            "ssh://git@gitlab.acme.com:2222/eq/r.git",
            Provider::GitLab,
            "gitlab.acme.com",
            "eq",
            None,
            "r",
            true,
        ),
        // Bitbucket
        (
            "https://bitbucket.org/w/r.git",
            Provider::Bitbucket,
            "bitbucket.org",
            "w",
            None,
            "r",
            false,
        ),
        (
            "https://usuario@bitbucket.org/w/r.git",
            Provider::Bitbucket,
            "bitbucket.org",
            "w",
            None,
            "r",
            false,
        ),
        (
            "git@bitbucket.org:w/r.git",
            Provider::Bitbucket,
            "bitbucket.org",
            "w",
            None,
            "r",
            true,
        ),
        (
            "https://bitbucket.acme.com/scm/PROJ/r.git",
            Provider::Bitbucket,
            "bitbucket.acme.com",
            "PROJ",
            None,
            "r",
            false,
        ),
        // Otros y locales
        (
            "https://git.acme.com/eq/sub/r.git",
            Provider::Other,
            "git.acme.com",
            "eq/sub",
            None,
            "r",
            false,
        ),
        (
            "git@servidor.interno:datos/r.git",
            Provider::Other,
            "servidor.interno",
            "datos",
            None,
            "r",
            true,
        ),
        (
            "ssh://git@10.0.0.5:2222/srv/git/r.git",
            Provider::Other,
            "10.0.0.5",
            "srv/git",
            None,
            "r",
            true,
        ),
        (
            "git://example.org/r.git",
            Provider::Other,
            "example.org",
            "",
            None,
            "r",
            false,
        ),
        (
            "/srv/git/repo.git",
            Provider::Other,
            "",
            "git",
            None,
            "repo",
            false,
        ),
        (
            "file:///srv/git/repo.git",
            Provider::Other,
            "",
            "git",
            None,
            "repo",
            false,
        ),
        (
            "C:\\Users\\yo\\repo",
            Provider::Other,
            "",
            "yo",
            None,
            "repo",
            false,
        ),
        (
            "C:/Users/yo/repo.git",
            Provider::Other,
            "",
            "yo",
            None,
            "repo",
            false,
        ),
        ("../repo", Provider::Other, "", "", None, "repo", false),
        (
            "~/dev/repo",
            Provider::Other,
            "",
            "dev",
            None,
            "repo",
            false,
        ),
        ("repo", Provider::Other, "", "", None, "repo", false),
        // Hostiles y raras
        (
            "https://github.com.evil.com/o/r",
            Provider::Other,
            "github.com.evil.com",
            "o",
            None,
            "r",
            false,
        ),
        (
            "https://github.com@evil.com/o/r",
            Provider::Other,
            "evil.com",
            "o",
            None,
            "r",
            false,
        ),
        (
            "https://evil.com\\@github.com/o/r",
            Provider::Other,
            "",
            "",
            None,
            "",
            false,
        ),
        (
            "https://evil.com/github.com/o/r",
            Provider::Other,
            "evil.com",
            "github.com/o",
            None,
            "r",
            false,
        ),
        (
            "https://dev.azure.com/org",
            Provider::Other,
            "dev.azure.com",
            "",
            None,
            "org",
            false,
        ),
        (
            "https://dev.azure.com/org/proj/_git/",
            Provider::Other,
            "dev.azure.com",
            "org/proj",
            None,
            "_git",
            false,
        ),
        (
            "https://github.com/solo",
            Provider::Other,
            "github.com",
            "",
            None,
            "solo",
            false,
        ),
        ("", Provider::Other, "", "", None, "", false),
        ("   ", Provider::Other, "", "", None, "", false),
        (
            "ext::sh -c touch% /tmp/x",
            Provider::Other,
            "",
            "",
            None,
            "",
            false,
        ),
        (
            "-oProxyCommand=algo",
            Provider::Other,
            "",
            "",
            None,
            "-oProxyCommand=algo",
            false,
        ),
        ("https://", Provider::Other, "", "", None, "", false),
        ("https://@/", Provider::Other, "", "", None, "", false),
        ("://x", Provider::Other, "", "", None, "", false),
        (
            "https://github.com/o/r\n",
            Provider::GitHub,
            "github.com",
            "o",
            None,
            "r",
            false,
        ),
        (
            "https://git\u{0}hub.com/o/r",
            Provider::Other,
            "",
            "",
            None,
            "",
            false,
        ),
        (
            "https://github.com/o/%zz%/r",
            Provider::GitHub,
            "github.com",
            "o",
            None,
            "%zz%",
            false,
        ),
        (
            "https://github.com/o/r%",
            Provider::GitHub,
            "github.com",
            "o",
            None,
            "r%",
            false,
        ),
        (
            "https://[::1]:8080/o/r.git",
            Provider::Other,
            "[::1]",
            "o",
            None,
            "r",
            false,
        ),
    ];

    #[test]
    fn tabla_de_urls() {
        assert!(TABLE.len() >= 40, "{} filas", TABLE.len());
        for (url, provider, host, owner, project, repo, ssh) in TABLE {
            let i = RemoteInfo::parse(url);
            let ctx = format!("{url:?} → {i:?}");
            assert_eq!(i.provider, *provider, "{ctx}");
            assert_eq!(i.host, *host, "{ctx}");
            assert_eq!(i.owner, *owner, "{ctx}");
            assert_eq!(i.project.as_deref(), *project, "{ctx}");
            assert_eq!(i.repo, *repo, "{ctx}");
            assert_eq!(i.ssh, *ssh, "{ctx}");
        }
    }

    #[test]
    fn nunca_deja_credenciales_en_el_resultado() {
        let secreto = ["tok", "en-in", "ventado"].concat();
        for url in [
            format!("https://usuario:{secreto}@github.com/o/r.git"),
            format!("https://{secreto}@dev.azure.com/org/p/_git/r"),
            format!("https://u:{secreto}@gitlab.acme.com/g/s/r.git"),
            format!("ssh://u:{secreto}@host.example/x/r.git"),
            format!("{secreto}@host.example:x/r.git"),
            format!("https://u:{secreto}@evil.com\\@github.com/o/r"),
        ] {
            let i = RemoteInfo::parse(&url);
            let json = serde_json::to_string(&i).unwrap();
            assert!(!json.contains(&secreto), "{url} → {json}");
            assert!(!format!("{i:?}").contains(&secreto));
        }
    }

    #[test]
    fn web_urls_de_azure_codifican_los_espacios() {
        let i = RemoteInfo::parse("https://org@dev.azure.com/org/Mi%20Proyecto/_git/repo");
        assert_eq!(
            i.repo_web_url().as_deref(),
            Some("https://dev.azure.com/org/Mi%20Proyecto/_git/repo")
        );
        let v = RemoteInfo::parse("https://org.visualstudio.com/DefaultCollection/p/_git/r");
        assert_eq!(
            v.repo_web_url().as_deref(),
            Some("https://dev.azure.com/org/p/_git/r")
        );
    }

    #[test]
    fn web_urls_de_los_demas() {
        let g = RemoteInfo::parse("git@github.com:o/r.git");
        assert_eq!(g.repo_web_url().as_deref(), Some("https://github.com/o/r"));
        let l = RemoteInfo::parse("git@gitlab.com:g/s/r.git");
        assert_eq!(
            l.repo_web_url().as_deref(),
            Some("https://gitlab.com/g/s/r")
        );
        assert_eq!(RemoteInfo::parse("/srv/x.git").repo_web_url(), None);
        assert_eq!(RemoteInfo::parse("basura").repo_web_url(), None);
    }

    #[test]
    fn urls_de_pull_request() {
        let gh = RemoteInfo::parse("https://github.com/o/r.git");
        assert_eq!(
            gh.new_pull_request_url("feature/x y").as_deref(),
            Some("https://github.com/o/r/pull/new/feature/x%20y")
        );
        let az = RemoteInfo::parse("https://dev.azure.com/org/p/_git/r");
        assert_eq!(
            az.new_pull_request_url("feature/x y&z=1").as_deref(),
            Some("https://dev.azure.com/org/p/_git/r/pullrequestcreate?sourceRef=feature%2Fx%20y%26z%3D1")
        );
        let gl = RemoteInfo::parse("https://gitlab.com/g/s/r.git");
        assert_eq!(
            gl.new_pull_request_url("a/b").as_deref(),
            Some(
                "https://gitlab.com/g/s/r/-/merge_requests/new?merge_request[source_branch]=a%2Fb"
            )
        );
        let bb = RemoteInfo::parse("git@bitbucket.org:w/r.git");
        assert_eq!(
            bb.new_pull_request_url("a/b").as_deref(),
            Some("https://bitbucket.org/w/r/pull-requests/new?source=a%2Fb")
        );
        let other = RemoteInfo::parse("https://git.acme.com/x/r.git");
        assert_eq!(other.new_pull_request_url("a"), None);
        assert_eq!(gh.new_pull_request_url(""), None);
        assert_eq!(gh.new_pull_request_url("a\nb"), None);
    }

    #[test]
    fn urls_de_rama() {
        let gh = RemoteInfo::parse("https://github.com/o/r.git");
        assert_eq!(
            gh.branch_web_url("feature/x y").as_deref(),
            Some("https://github.com/o/r/tree/feature/x%20y")
        );
        let az = RemoteInfo::parse("https://dev.azure.com/org/p/_git/r");
        assert_eq!(
            az.branch_web_url("feature/x&y").as_deref(),
            Some("https://dev.azure.com/org/p/_git/r?version=GBfeature%2Fx%26y")
        );
        let gl = RemoteInfo::parse("https://gitlab.com/g/s/r.git");
        assert_eq!(
            gl.branch_web_url("a/b").as_deref(),
            Some("https://gitlab.com/g/s/r/-/tree/a/b")
        );
        let bb = RemoteInfo::parse("git@bitbucket.org:w/r.git");
        assert_eq!(
            bb.branch_web_url("a/b").as_deref(),
            Some("https://bitbucket.org/w/r/branch/a/b")
        );
        let other = RemoteInfo::parse("https://git.acme.com/x/r.git");
        assert_eq!(other.branch_web_url("a"), None);
        assert_eq!(gh.branch_web_url(" "), None);
        assert_eq!(gh.branch_web_url("a\nb"), None);
        // Una rama con `?`, `#` o `..` no puede salirse de la ruta: se codifica.
        assert_eq!(
            gh.branch_web_url("x?y#z").as_deref(),
            Some("https://github.com/o/r/tree/x%3Fy%23z")
        );
    }

    #[test]
    fn pistas_configurables_para_servidores_propios() {
        let url = "https://git.acme.com/eq/app.git";
        assert_eq!(RemoteInfo::parse(url).provider, Provider::Other);
        let hints = HostHints {
            github_hosts: vec!["GIT.acme.com".into()],
            ..HostHints::default()
        };
        assert_eq!(
            RemoteInfo::parse_with(url, &hints).provider,
            Provider::GitHub
        );
        let gl = HostHints {
            gitlab_hosts: vec!["git.acme.com".into()],
            ..HostHints::default()
        };
        assert_eq!(RemoteInfo::parse_with(url, &gl).provider, Provider::GitLab);
        let sin = HostHints {
            heuristics: false,
            ..HostHints::default()
        };
        assert_eq!(
            RemoteInfo::parse_with("https://github.acme.com/a/b", &sin).provider,
            Provider::Other
        );
    }

    #[test]
    fn ayuda_de_pat_usa_solo_https_y_hosts_permitidos() {
        for url in [
            "https://dev.azure.com/o/p/_git/r",
            "https://github.com/o/r",
            "https://gitlab.com/g/r",
            "https://bitbucket.org/w/r",
        ] {
            let h = RemoteInfo::parse(url).pat_help();
            let u = h.url.unwrap();
            let rest = u.strip_prefix("https://").unwrap();
            let host = rest.split('/').next().unwrap();
            assert!(ALLOWED_HELP_HOSTS.contains(&host), "{u}");
            assert!(!h.permissions.is_empty());
        }
        let az = RemoteInfo::parse("https://dev.azure.com/o/p/_git/r").pat_help();
        assert!(az.permissions.contains("Code") && az.permissions.contains("Read & write"));
        assert!(az.username_note.unwrap().contains("cualquiera"));
        let gh = RemoteInfo::parse("https://github.com/o/r").pat_help();
        assert!(gh.permissions.contains("repo") && gh.permissions.contains("Contents"));
        let gl = RemoteInfo::parse("https://gitlab.com/g/r").pat_help();
        assert!(
            gl.permissions.contains("read_repository")
                && gl.permissions.contains("write_repository")
        );
        assert!(RemoteInfo::parse("/x/y").pat_help().url.is_none());
    }

    #[test]
    fn nombre_de_carpeta_sugerido() {
        let s = |u: &str| RemoteInfo::parse(u).suggested_dir_name();
        assert_eq!(s("https://github.com/o/mi-repo.git"), "mi-repo");
        assert_eq!(s("https://dev.azure.com/o/p/_git/mi%20repo"), "mi repo");
        assert_eq!(s("https://github.com/o/%2e%2e"), "repo");
        assert_eq!(s("https://github.com/o/a%2Fb"), "a-b");
        assert_eq!(s("https://github.com/o/CON"), "repo");
        assert_eq!(s("https://github.com/o/nul.txt"), "repo");
        assert_eq!(s("basura://"), "repo");
        assert_eq!(s(""), "repo");
    }

    #[test]
    fn serializa_en_camel_case() {
        let i = RemoteInfo::parse("https://dev.azure.com/org/p/_git/r");
        let v = serde_json::to_value(&i).unwrap();
        assert_eq!(v["provider"], "azureDevOps");
        assert_eq!(v["webUrl"], "https://dev.azure.com/org/p/_git/r");
        assert_eq!(v["project"], "p");
        assert_eq!(v["ssh"], false);
        let back: RemoteInfo = serde_json::from_value(v).unwrap();
        assert_eq!(back, i);
    }

    /// Generador pseudoaleatorio determinista (xorshift) para no añadir dependencias.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    #[test]
    fn propiedad_nunca_entra_en_panico_con_entrada_aleatoria() {
        const PIEZAS: &[&str] = &[
            "https://",
            "ssh://",
            "git@",
            "file://",
            "://",
            "@",
            ":",
            "/",
            "\\",
            "%",
            "%2",
            "%20",
            "%zz",
            "%00",
            "..",
            ".",
            ".git",
            "_git",
            "v3",
            "github.com",
            "dev.azure.com",
            "gitlab.com",
            "bitbucket.org",
            "vs-ssh.visualstudio.com",
            ".visualstudio.com",
            "DefaultCollection",
            "[",
            "]",
            "[::1]",
            "?",
            "#",
            " ",
            "\t",
            "\n",
            "\u{0}",
            "é",
            "ñ",
            "日本",
            "\u{202e}",
            "😀",
            "-",
            "--",
            "a",
            "org",
            "proj",
            "repo",
            ":22",
            ":99999999999",
        ];
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let hints = HostHints::default();
        for _ in 0..30_000 {
            let n = 1 + rng.below(12);
            let mut s = String::new();
            for _ in 0..n {
                s.push_str(PIEZAS[rng.below(PIEZAS.len())]);
            }
            let i = RemoteInfo::parse_with(&s, &hints);
            // Invariantes: nada de control en los campos, y los métodos tampoco paniquean.
            for f in [&i.host, &i.owner, &i.repo] {
                assert!(!f.chars().any(char::is_control), "{s:?} → {i:?}");
            }
            if let Some(w) = &i.web_url {
                assert!(
                    w.starts_with("https://") && !w.contains(char::is_whitespace),
                    "{s:?} → {w}"
                );
            }
            let _ = i.new_pull_request_url("rama/x");
            let _ = i.pat_help();
            let d = i.suggested_dir_name();
            assert!(!d.is_empty() && !d.contains(['/', '\\']));
        }
    }
}
