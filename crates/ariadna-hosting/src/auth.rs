//! Autenticación de las llamadas a la API del proveedor (D22).
//!
//! **Regla inamovible:** Ariadna no guarda, cachea ni registra el token. Cada operación pide la
//! credencial a una [`CredentialSource`] (en producción [`GitCredentialFill`]: `git credential
//! fill` no interactivo), la usa en memoria y la suelta al terminar (`zeroize`). El secreto
//! nunca va en `argv`, en una URL, en un registro, en un `Debug` ni en un error.

use std::fmt;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use zeroize::{Zeroize, Zeroizing};

use crate::error::{HostingError, HostingResult};

/// Secreto (token/PAT). Se borra de memoria al soltarlo. Sin `Clone` ni `Serialize`; `Debug` y
/// `Display` muestran `***`.
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    /// El secreto en claro. Solo para construir la cabecera `Authorization`.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// Cómo se autentica una llamada. Hoy PAT (HTTP Basic); `Bearer` queda definido para el inicio de
/// sesión con Microsoft (Entra ID) de la v0.2 sin reescribir nada (D22).
#[non_exhaustive]
pub enum AuthMethod {
    /// Usuario y token personal: `Authorization: Basic base64(usuario:token)`.
    Pat { username: String, secret: Secret },
    /// Token de portador (OAuth / Entra ID): `Authorization: Bearer …`.
    Bearer(Secret),
}

impl fmt::Debug for AuthMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthMethod::Pat { username, .. } => f
                .debug_struct("Pat")
                .field("username", username)
                .field("secret", &"***")
                .finish(),
            AuthMethod::Bearer(_) => f.write_str("Bearer(***)"),
        }
    }
}

impl AuthMethod {
    pub fn pat(username: impl Into<String>, secret: impl Into<String>) -> Self {
        AuthMethod::Pat {
            username: username.into(),
            secret: Secret::new(secret),
        }
    }

    /// Aplica la autenticación. `reqwest` marca la cabecera como sensible (no sale en `Debug`).
    pub(crate) fn apply(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self {
            AuthMethod::Pat { username, secret } => rb.basic_auth(username, Some(secret.expose())),
            AuthMethod::Bearer(secret) => rb.bearer_auth(secret.expose()),
        }
    }

    /// Prepara el redactor de esta operación.
    pub(crate) fn scrubber(&self) -> Scrubber {
        let mut needles: Vec<Zeroizing<String>> = Vec::new();
        match self {
            AuthMethod::Pat { username, secret } => {
                needles.push(Zeroizing::new(secret.expose().to_owned()));
                let pair = Zeroizing::new(format!("{username}:{}", secret.expose()));
                needles.push(Zeroizing::new(base64(pair.as_bytes())));
            }
            AuthMethod::Bearer(secret) => needles.push(Zeroizing::new(secret.expose().to_owned())),
        }
        Scrubber { needles }
    }
}

/// Sustituye por `***` el secreto (y su forma en Base64) en cualquier texto que vaya a salir.
pub struct Scrubber {
    needles: Vec<Zeroizing<String>>,
}

impl Scrubber {
    /// Un redactor sin nada que redactar (operaciones sin credencial).
    pub fn none() -> Self {
        Self {
            needles: Vec::new(),
        }
    }

    pub fn scrub(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for n in &self.needles {
            // Con menos de 4 caracteres redactar destrozaría el mensaje sin proteger nada.
            if n.len() >= 4 {
                out = out.replace(n.as_str(), "***");
            }
        }
        out
    }
}

impl fmt::Debug for Scrubber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Scrubber(***)")
    }
}

fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// De dónde sale la credencial de **cada** operación. No se guarda nada entre llamadas.
#[async_trait]
pub trait CredentialSource: Send + Sync {
    /// `url` es la página web del repositorio (`https://host/…`); el helper de git decide qué
    /// parte de la ruta cuenta.
    async fn credentials(&self, url: &str) -> HostingResult<AuthMethod>;
}

/// Credencial fija para pruebas y herramientas. **No** se usa en la aplicación.
pub struct StaticCredentials {
    username: String,
    secret: Zeroizing<String>,
}

impl StaticCredentials {
    pub fn pat(username: impl Into<String>, secret: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            secret: Zeroizing::new(secret.into()),
        }
    }
}

impl fmt::Debug for StaticCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StaticCredentials({}, ***)", self.username)
    }
}

#[async_trait]
impl CredentialSource for StaticCredentials {
    async fn credentials(&self, _url: &str) -> HostingResult<AuthMethod> {
        Ok(AuthMethod::pat(&self.username, self.secret.as_str()))
    }
}

/// Obtiene la credencial con `git credential fill`, sin interacción.
///
/// * El `url=` viaja por **stdin**; el secreto vuelve por stdout. Nada va en `argv`.
/// * Entorno **saneado**: se borra todo y se conserva solo una lista blanca (PATH, HOME,
///   APPDATA…, lo que el helper necesita para encontrar su almacén) más `GIT_TERMINAL_PROMPT=0`
///   y `GCM_INTERACTIVE=never`.
/// * `stderr` se descarta: puede traer rutas o nombres de usuario y nunca se muestra.
/// * Si el helper no tiene la credencial, el error es [`HostingError::Auth`] y la interfaz
///   abre el formulario de credenciales.
#[derive(Debug, Clone)]
pub struct GitCredentialFill {
    git: PathBuf,
    timeout: Duration,
    /// Solo pruebas reales (spike): helper de credenciales fijado por `-c` para este proceso.
    helper: Option<String>,
}

/// Variables de entorno que se heredan al lanzar `git credential fill`.
const ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PROGRAMW6432",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "COMSPEC",
    "PATHEXT",
    "TEMP",
    "TMP",
    "TMPDIR",
    "USER",
    "USERNAME",
    "LOGNAME",
    "LANG",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG_NOSYSTEM",
    "GIT_EXEC_PATH",
];

impl GitCredentialFill {
    pub fn new(git: impl Into<PathBuf>) -> Self {
        Self {
            git: git.into(),
            timeout: Duration::from_secs(30),
            helper: None,
        }
    }

    /// **Solo pruebas/spike.** Fija el helper de credenciales (`credential.helper`) únicamente
    /// para este proceso con `git -c credential.helper= -c credential.helper=<helper>`, p. ej.
    /// `!gh.exe auth git-credential` en WSL, donde no hay configuración de git del equipo.
    /// El entorno saneado descarta `GIT_CONFIG_COUNT/KEY/VALUE`, de modo que la producción
    /// no puede cambiar de helper por variables de entorno; esto es una opción explícita del
    /// constructor que la aplicación no usa. El helper no es un secreto (va en `argv`).
    /// Además deja pasar `WSL_INTEROP`/`WSL_DISTRO_NAME`, que un helper de Windows necesita.
    pub fn with_helper_override(mut self, helper: impl Into<String>) -> Self {
        let h: String = helper.into();
        self.helper = (!h.trim().is_empty() && !h.chars().any(char::is_control)).then_some(h);
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.git);
        if let Some(h) = &self.helper {
            cmd.args(["-c", "credential.helper=", "-c"])
                .arg(format!("credential.helper={h}"));
        }
        cmd.args(["credential", "fill"])
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        for key in ENV_ALLOWLIST {
            if let Some(v) = std::env::var_os(key) {
                cmd.env(key, v);
            }
        }
        if self.helper.is_some() {
            for key in ["WSL_INTEROP", "WSL_DISTRO_NAME"] {
                if let Some(v) = std::env::var_os(key) {
                    cmd.env(key, v);
                }
            }
        }
        cmd.env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never");
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        cmd
    }
}

fn no_credential(host: &str) -> HostingError {
    HostingError::auth(
        format!(
            "El gestor de credenciales de git no tiene un token para {host}. Guárdalo en Ajustes › Cuentas y credenciales."
        ),
        None,
    )
}

#[async_trait]
impl CredentialSource for GitCredentialFill {
    async fn credentials(&self, url: &str) -> HostingResult<AuthMethod> {
        let url = url.trim();
        let parsed = url::Url::parse(url)
            .ok()
            .filter(|u| u.scheme() == "https" && u.host_str().is_some())
            .ok_or_else(|| HostingError::invalid("La dirección del remoto no es https."))?;
        if url.chars().any(char::is_control) {
            return Err(HostingError::invalid(
                "La dirección del remoto lleva caracteres no válidos.",
            ));
        }
        let host = parsed.host_str().unwrap_or_default().to_owned();
        // Nunca se reenvía una contraseña que viniera en la URL.
        let mut clean = parsed.clone();
        let _ = clean.set_password(None);
        let input = Zeroizing::new(format!("url={clean}\n\n"));

        let mut child = self.command().spawn().map_err(|_| HostingError::Io {
            message: "No se pudo lanzar git para pedir la credencial.".into(),
        })?;
        let mut stdin = child.stdin.take();
        let mut stdout = child.stdout.take();
        let io = async {
            if let Some(mut s) = stdin.take() {
                // Si git cierra stdin antes de tiempo se ignora: lo dirá el código de salida.
                let _ = s.write_all(input.as_bytes()).await;
                let _ = s.shutdown().await;
            }
            let mut buf = Vec::new();
            if let Some(o) = stdout.as_mut() {
                // Tope: una salida de más de 64 KiB no es una credencial.
                let _ = o.take(64 * 1024).read_to_end(&mut buf).await;
            }
            let status = child.wait().await;
            (buf, status)
        };
        let (mut buf, status) = match tokio::time::timeout(self.timeout, io).await {
            Ok(r) => r,
            Err(_) => {
                return Err(HostingError::auth(
                    format!("El gestor de credenciales no respondió para {host}."),
                    None,
                ))
            }
        };
        let ok = status.map(|s| s.success()).unwrap_or(false);
        let text = Zeroizing::new(String::from_utf8_lossy(&buf).into_owned());
        buf.zeroize();
        if !ok {
            return Err(no_credential(&host));
        }
        let (mut user, mut pass) = (None::<String>, None::<Secret>);
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("username=") {
                user = Some(v.to_owned());
            } else if let Some(v) = line.strip_prefix("password=") {
                pass = Some(Secret::new(v));
            }
        }
        match pass {
            Some(secret) if !secret.is_empty() => Ok(AuthMethod::Pat {
                username: user.unwrap_or_default(),
                secret,
            }),
            _ => Err(no_credential(&host)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_coincide_con_el_estandar() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
    }

    #[test]
    fn scrubber_quita_el_secreto_y_su_base64() {
        let auth = AuthMethod::pat("ana", "ghp_SECRETO123456");
        let s = auth.scrubber();
        let b64 = base64(b"ana:ghp_SECRETO123456");
        let out = s.scrub(&format!("token ghp_SECRETO123456 y Basic {b64}"));
        assert!(!out.contains("SECRETO"), "{out}");
        assert!(!out.contains(&b64), "{out}");
        assert!(out.contains("***"));
    }

    #[test]
    fn debug_y_display_no_muestran_el_secreto() {
        let auth = AuthMethod::pat("ana", "ghp_SECRETO123456");
        assert!(!format!("{auth:?}").contains("SECRETO"));
        assert!(!format!("{:?}", auth.scrubber()).contains("SECRETO"));
        let b = AuthMethod::Bearer(Secret::new("ey.SECRETO.token"));
        assert!(!format!("{b:?}").contains("SECRETO"));
        assert_eq!(Secret::new("x").to_string(), "***");
        let st = StaticCredentials::pat("u", "ghp_SECRETO123456");
        assert!(!format!("{st:?}").contains("SECRETO"));
    }
}
