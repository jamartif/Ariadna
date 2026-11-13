//! Validación de URL de remoto para clonar y para credenciales (spec §4.3, «Clonar»).
//!
//! Esquemas permitidos: `https`, `http`, `ssh`, `scp-like` (`git@host:ruta`) y, solo para
//! clonar, rutas locales y `file://`. Se rechazan los transportes con ayudante (`ext::`,
//! `fd::`…), todo lo que empiece por `-`, los caracteres de control y las contraseñas
//! incrustadas (`https://user:pass@host`): el secreto no debe acabar en el `.git/config`.

use crate::error::GitError;
use crate::runner::redact_credentials;

/// Tipo de URL aceptada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UrlKind {
    Http,
    Ssh,
    ScpLike,
    Local,
}

/// URL ya validada y descompuesta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedRemote {
    pub kind: UrlKind,
    /// `http` o `https` (solo `UrlKind::Http`).
    pub scheme: String,
    /// `host` o `host:puerto` si el puerto no es el de por defecto.
    pub host: String,
    /// Ruta decodificada, sin la `/` inicial.
    pub path: String,
    /// Usuario de la URL, si lo hay (nunca la contraseña).
    pub username: Option<String>,
}

/// Por qué se rechaza una URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rejection {
    Empty,
    LeadingDash,
    Control,
    Transport,
    Scheme,
    NoHost,
    EmbeddedPassword,
    LocalNotAllowed,
}

impl Rejection {
    /// Error de motor con la URL **redactada** (sin usuario ni contraseña).
    pub(crate) fn into_error(self, raw: &str) -> GitError {
        let shown = crate::error::truncate(&redact_credentials(raw.trim()), 200);
        let shown = shown
            .chars()
            .filter(|c| !c.is_control())
            .collect::<String>();
        match self {
            Rejection::EmbeddedPassword => GitError::InvalidReference(format!(
                "{shown} (no pegues el token en la URL; guárdalo en Credenciales)"
            )),
            _ => GitError::InvalidReference(shown),
        }
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
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
    String::from_utf8_lossy(&out).into_owned()
}

fn is_drive_path(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b.len() == 2 || matches!(b[2], b'/' | b'\\'))
}

/// `transporte::dirección` (`ext::`, `fd::`, `ssh::`…): los `::` van justo tras el nombre.
fn is_transport_helper(s: &str) -> bool {
    let mut chars = s.char_indices();
    if !chars.next().is_some_and(|(_, c)| c.is_ascii_alphabetic()) {
        return false;
    }
    for (i, c) in chars {
        if c == ':' {
            return s[i..].starts_with("::");
        }
        if !(c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-')) {
            return false;
        }
    }
    false
}

fn valid_host(h: &str) -> bool {
    !h.is_empty()
        && !h.starts_with('-')
        && h.len() <= 253
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '[' | ']' | ':'))
}

/// Valida y descompone una URL de remoto. `allow_local`: admite rutas locales y `file://`.
pub(crate) fn parse_remote_url(raw: &str, allow_local: bool) -> Result<ParsedRemote, Rejection> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(Rejection::Empty);
    }
    if s.starts_with('-') {
        return Err(Rejection::LeadingDash);
    }
    if s.chars().any(char::is_control) {
        return Err(Rejection::Control);
    }
    if is_transport_helper(s) {
        return Err(Rejection::Transport);
    }
    let local = |path: &str| {
        if allow_local {
            Ok(ParsedRemote {
                kind: UrlKind::Local,
                scheme: "file".into(),
                host: String::new(),
                path: path.to_string(),
                username: None,
            })
        } else {
            Err(Rejection::LocalNotAllowed)
        }
    };
    if s.contains("://") {
        let url = url::Url::parse(s).map_err(|_| Rejection::Scheme)?;
        let scheme = url.scheme().to_ascii_lowercase();
        if scheme == "file" {
            return local(url.path());
        }
        let kind = match scheme.as_str() {
            "https" | "http" => UrlKind::Http,
            "ssh" => UrlKind::Ssh,
            _ => return Err(Rejection::Scheme),
        };
        let host = url.host_str().unwrap_or("");
        if !valid_host(host) {
            return Err(if host.is_empty() {
                Rejection::NoHost
            } else {
                Rejection::LeadingDash
            });
        }
        if url.password().is_some() {
            return Err(Rejection::EmbeddedPassword);
        }
        let hostport = match url.port() {
            Some(p) => format!("{host}:{p}"),
            None => host.to_string(),
        };
        let username = Some(percent_decode(url.username())).filter(|u| !u.is_empty());
        return Ok(ParsedRemote {
            kind,
            scheme,
            host: hostport,
            path: percent_decode(url.path().trim_start_matches('/')),
            username,
        });
    }
    if s.starts_with(['/', '\\', '~', '.']) || is_drive_path(s) {
        return local(s);
    }
    // scp-like: [usuario@]host:ruta, sin `/` antes de los dos puntos.
    if let Some((head, path)) = s.split_once(':') {
        if !head.contains(['/', '\\']) {
            let (user, host) = match head.rsplit_once('@') {
                Some((u, h)) => (Some(u), h),
                None => (None, head),
            };
            if !valid_host(host) || host.contains(':') {
                return Err(Rejection::NoHost);
            }
            if path.is_empty() {
                return Err(Rejection::NoHost);
            }
            return Ok(ParsedRemote {
                kind: UrlKind::ScpLike,
                scheme: "ssh".into(),
                host: host.to_ascii_lowercase(),
                path: path.trim_start_matches('/').to_string(),
                username: user.filter(|u| !u.is_empty()).map(str::to_string),
            });
        }
    }
    local(s)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn acepta_las_formas_validas() {
        let p = parse_remote_url(
            "https://org@dev.azure.com/org/Mi%20Proyecto/_git/repo",
            false,
        )
        .unwrap();
        assert_eq!(p.kind, UrlKind::Http);
        assert_eq!(p.host, "dev.azure.com");
        assert_eq!(p.path, "org/Mi Proyecto/_git/repo");
        assert_eq!(p.username.as_deref(), Some("org"));
        let p = parse_remote_url("http://git.acme.com:8080/x/r.git", false).unwrap();
        assert_eq!(
            (p.scheme.as_str(), p.host.as_str()),
            ("http", "git.acme.com:8080")
        );
        let p = parse_remote_url("https://GitHub.com:443/o/r.git", false).unwrap();
        assert_eq!(p.host, "github.com");
        assert_eq!(
            parse_remote_url("ssh://git@github.com/o/r.git", false)
                .unwrap()
                .kind,
            UrlKind::Ssh
        );
        let p = parse_remote_url("git@github.com:o/r.git", false).unwrap();
        assert_eq!(
            (p.kind, p.host.as_str(), p.path.as_str()),
            (UrlKind::ScpLike, "github.com", "o/r.git")
        );
        assert_eq!(
            parse_remote_url("/srv/git/r.git", true).unwrap().kind,
            UrlKind::Local
        );
        assert_eq!(
            parse_remote_url("C:\\repos\\r", true).unwrap().kind,
            UrlKind::Local
        );
        assert_eq!(
            parse_remote_url("file:///srv/r.git", true).unwrap().kind,
            UrlKind::Local
        );
        assert_eq!(
            parse_remote_url("relativa/r.git", true).unwrap().kind,
            UrlKind::Local
        );
        assert_eq!(
            parse_remote_url("https://[::1]:8443/x.git", false)
                .unwrap()
                .host,
            "[::1]:8443"
        );
    }

    #[test]
    fn rechaza_lo_peligroso() {
        let cases: &[(&str, Rejection)] = &[
            ("", Rejection::Empty),
            ("   ", Rejection::Empty),
            ("-x", Rejection::LeadingDash),
            ("--upload-pack=touch /tmp/x", Rejection::LeadingDash),
            ("ext::sh -c touch /tmp/x", Rejection::Transport),
            ("fd::17/foo", Rejection::Transport),
            ("ssh::https://x/y", Rejection::Transport),
            ("https://host/x\nfoo", Rejection::Control),
            ("https://host/x\u{0}", Rejection::Control),
            ("ftp://host/x", Rejection::Scheme),
            ("git://host/x.git", Rejection::Scheme),
            ("javascript://x", Rejection::Scheme),
            ("https://", Rejection::Scheme),
            ("ssh://-oProxyCommand=touch/x", Rejection::LeadingDash),
            ("-oProxyCommand=x:path", Rejection::LeadingDash),
            ("user@-host:path", Rejection::NoHost),
            ("host:", Rejection::NoHost),
            ("https://u:clave@host/x.git", Rejection::EmbeddedPassword),
        ];
        for (url, why) in cases {
            assert_eq!(parse_remote_url(url, true), Err(*why), "{url:?}");
        }
        assert_eq!(
            parse_remote_url("/srv/x", false),
            Err(Rejection::LocalNotAllowed)
        );
        assert_eq!(
            parse_remote_url("file:///x", false),
            Err(Rejection::LocalNotAllowed)
        );
    }

    #[test]
    fn el_error_no_contiene_la_contrasena() {
        let pw = ["cla", "ve-inventada"].concat();
        let url = format!("https://usuario:{pw}@host.example/x.git");
        let why = parse_remote_url(&url, false).unwrap_err();
        let e = why.into_error(&url);
        assert!(!format!("{e} {e:?}").contains(&pw));
    }
}
