//! Credenciales por PAT a través del `credential helper` de git (spec §4.3, «Credenciales»).
//!
//! Ariadna **no guarda** el secreto: se lo entrega al helper del sistema (Git Credential
//! Manager, Keychain…) con `git credential approve`. Reglas:
//!
//! * El secreto viaja **solo por stdin** de git; nunca en argv ni en variables de entorno.
//! * [`Secret`] se borra de memoria al soltarlo (`zeroize`), su `Debug`/`Display` van
//!   redactados y no es `Clone` ni `Serialize`.
//! * Ningún error contiene el secreto (se elimina de cualquier texto de git antes de devolverlo).
//! * Los campos del protocolo de credenciales no admiten saltos de línea ni NUL (evita
//!   inyectar líneas como `host=`).

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Deserializer};
use zeroize::{Zeroize, Zeroizing};

use crate::engine::GitCliEngine;
use crate::error::{truncate, GitError, GitResult};
use crate::remote_url::{parse_remote_url, ParsedRemote, UrlKind};
use crate::runner::{redact_credentials, CommandKind};

/// Tiempo máximo del sondeo `git ls-remote` de [`CredentialStore::test_credential`].
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

/// Entorno de los comandos de credenciales no interactivos (el GCM no abre ventanas).
const NON_INTERACTIVE_ENV: [(&str, &str); 2] =
    [("GIT_TERMINAL_PROMPT", "0"), ("GCM_INTERACTIVE", "never")];

/// Secreto (token/PAT). Se borra de memoria al soltarlo.
///
/// Sin `Clone` ni `Serialize`; `Debug` y `Display` muestran `***`. Para leerlo hay que llamar
/// de forma explícita a [`expose`](Self::expose).
///
/// ```compile_fail
/// fn necesita_serialize<T: serde::Serialize>() {}
/// necesita_serialize::<ariadna_git::Secret>();
/// ```
///
/// ```compile_fail
/// fn necesita_clone<T: Clone>() {}
/// necesita_clone::<ariadna_git::Secret>();
/// ```
pub struct Secret(Zeroizing<String>);

impl Secret {
    /// Envuelve el texto del secreto.
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    /// El secreto en claro. Úsalo solo para entregarlo a git por stdin.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
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

/// Se puede recibir por IPC (`store_credential`), pero nunca enviar.
impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Secret::new(String::deserialize(d)?))
    }
}

/// Usuario y secreto que devolvió el helper (`git credential fill`).
#[derive(Debug)]
pub struct FilledCredential {
    pub username: String,
    pub secret: Secret,
}

/// Gestión de credenciales por el helper de git. Lo implementa [`GitCliEngine`].
#[async_trait]
pub trait CredentialStore {
    /// `git credential approve`: entrega `protocol/host/(path)/username/password` por stdin.
    /// `path` solo se incluye si git lo conserva (`credential.useHttpPath`). Solo URL `http(s)`.
    async fn store_credential(&self, url: &str, username: &str, secret: Secret) -> GitResult<()>;

    /// `git credential reject`: el helper olvida la credencial de esa URL y usuario.
    async fn forget_credential(&self, url: &str, username: &str) -> GitResult<()>;

    /// `git ls-remote --exit-code -q <url>` sin interacción. `Ok` si el servidor responde
    /// (también con un repo vacío); [`GitError::Auth`] si las credenciales no valen.
    async fn test_credential(&self, url: &str) -> GitResult<()>;

    /// `git credential fill` sin interacción (para las pull requests). Si el helper no tiene la
    /// credencial devuelve [`GitError::Auth`].
    async fn fill_credential(&self, url: &str) -> GitResult<FilledCredential>;

    /// ¿Hay algún `credential.helper` configurado (en cualquier nivel)? Sin ninguno,
    /// `store_credential` no guarda nada y la UI debe ofrecer «usar una vez».
    async fn has_credential_helper(&self) -> GitResult<bool>;
}

fn neutral_cwd() -> PathBuf {
    std::env::temp_dir()
}

fn field_error(name: &str) -> GitError {
    GitError::InvalidReference(format!(
        "{name} (no puede contener saltos de línea ni caracteres de control)"
    ))
}

fn check_field(name: &str, value: &str) -> GitResult<()> {
    if value.chars().any(char::is_control) {
        return Err(field_error(name));
    }
    Ok(())
}

/// Cuerpo para stdin del protocolo de credenciales de git.
fn credential_input(
    t: &ParsedRemote,
    username: Option<&str>,
    password: Option<&str>,
) -> GitResult<Zeroizing<Vec<u8>>> {
    check_field("host", &t.host)?;
    check_field("ruta", &t.path)?;
    let mut s = Zeroizing::new(String::new());
    s.push_str(&format!("protocol={}\nhost={}\n", t.scheme, t.host));
    if !t.path.is_empty() {
        s.push_str(&format!("path={}\n", t.path));
    }
    if let Some(u) = username {
        check_field("usuario", u)?;
        s.push_str(&format!("username={u}\n"));
    }
    if let Some(p) = password {
        // El secreto no se nombra en el error.
        if p.chars().any(char::is_control) {
            return Err(field_error("el secreto"));
        }
        s.push_str(&format!("password={p}\n"));
    }
    s.push('\n');
    Ok(Zeroizing::new(s.as_bytes().to_vec()))
}

fn parse_http(url: &str) -> GitResult<ParsedRemote> {
    let parsed = parse_remote_url(url, false).map_err(|why| why.into_error(url))?;
    if parsed.kind != UrlKind::Http {
        let shown = redact_credentials(url.trim());
        return Err(GitError::InvalidReference(format!(
            "{} (solo las URL https/http usan token; para SSH usa ssh-add)",
            truncate(&shown, 200)
        )));
    }
    Ok(parsed)
}

/// Quita el secreto de cualquier texto y redacta credenciales de URL.
fn scrub(text: &str, secret: Option<&str>) -> String {
    let mut t = redact_credentials(text);
    if let Some(s) = secret.filter(|s| !s.is_empty()) {
        t = t.replace(s, "***");
    }
    t
}

impl GitCliEngine {
    async fn credential_cmd(
        &self,
        op: &str,
        input: Zeroizing<Vec<u8>>,
        secret: Option<&str>,
        remote: Option<&str>,
    ) -> GitResult<ariadna_process::ProcessOutput> {
        let args = [std::ffi::OsString::from("credential"), op.into()];
        let out = self
            .runner
            .run_with_env(
                &neutral_cwd(),
                CommandKind::Mutate,
                &args,
                Some(input.to_vec()),
                &NON_INTERACTIVE_ENV,
            )
            .await?;
        if out.success() {
            return Ok(out);
        }
        // `fill` sin credencial guardada: git intenta preguntar y falla; es un caso de Auth.
        if op == "fill" {
            if let Some(e) =
                crate::auth::classify_auth_failure(out.code, &out.stderr_lossy(), remote)
            {
                return Err(e);
            }
        }
        Err(GitError::CommandFailed {
            command: format!("git credential {op}"),
            exit_code: out.code.unwrap_or(-1),
            stderr: truncate(&scrub(&out.stderr_lossy(), secret), 4_000),
        })
    }

    /// `ls-remote` sin validar la URL (el llamador ya lo hizo).
    async fn probe_remote(&self, url: &str) -> GitResult<()> {
        let args: Vec<std::ffi::OsString> = ["ls-remote", "--exit-code", "-q", "--", url]
            .iter()
            .map(std::ffi::OsString::from)
            .collect();
        let cwd = neutral_cwd();
        let fut = self.runner.run_with_env(
            &cwd,
            CommandKind::Network,
            &args,
            None,
            &NON_INTERACTIVE_ENV,
        );
        let out = tokio::time::timeout(PROBE_TIMEOUT, fut)
            .await
            .map_err(|_| GitError::Timeout {
                command: "git ls-remote".into(),
            })??;
        match out.code {
            // 2 = no hay referencias que mostrar: el servidor respondió y nos dejó entrar.
            Some(0) | Some(2) => Ok(()),
            _ => Err(self
                .runner
                .failure_error(&cwd, CommandKind::Network, &args, &out)
                .await),
        }
    }
}

#[async_trait]
impl CredentialStore for GitCliEngine {
    async fn store_credential(&self, url: &str, username: &str, secret: Secret) -> GitResult<()> {
        let target = parse_http(url)?;
        if username.trim().is_empty() {
            return Err(GitError::InvalidReference(
                "el usuario no puede estar vacío".into(),
            ));
        }
        if secret.is_empty() {
            return Err(GitError::InvalidReference(
                "el token no puede estar vacío".into(),
            ));
        }
        let input = credential_input(&target, Some(username), Some(secret.expose()))?;
        self.credential_cmd("approve", input, Some(secret.expose()), None)
            .await?;
        Ok(())
    }

    async fn forget_credential(&self, url: &str, username: &str) -> GitResult<()> {
        let target = parse_http(url)?;
        if username.trim().is_empty() {
            return Err(GitError::InvalidReference(
                "el usuario no puede estar vacío".into(),
            ));
        }
        let input = credential_input(&target, Some(username), None)?;
        self.credential_cmd("reject", input, None, None).await?;
        Ok(())
    }

    async fn test_credential(&self, url: &str) -> GitResult<()> {
        // Se valida y se pasa la URL tal cual (sin contraseña: ya se rechazó) para respetar
        // el usuario que lleve.
        parse_remote_url(url, false).map_err(|why| why.into_error(url))?;
        self.probe_remote(url.trim()).await
    }

    async fn fill_credential(&self, url: &str) -> GitResult<FilledCredential> {
        let target = parse_http(url)?;
        let input = credential_input(&target, target.username.as_deref(), None)?;
        let mut out = self
            .credential_cmd("fill", input, None, Some(url.trim()))
            .await?;
        let text = Zeroizing::new(String::from_utf8_lossy(&out.stdout).into_owned());
        out.stdout.zeroize();
        let (mut user, mut pass) = (None, None);
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("username=") {
                user = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("password=") {
                pass = Some(Secret::new(v));
            }
        }
        match (user, pass) {
            (Some(username), Some(secret)) if !secret.is_empty() => {
                Ok(FilledCredential { username, secret })
            }
            _ => Err(GitError::Auth {
                host: target.host,
                hint: "El gestor de credenciales no tiene un token para este servidor. Guárdalo en Credenciales.".into(),
            }),
        }
    }

    async fn has_credential_helper(&self) -> GitResult<bool> {
        let args = ["config", "--get-all", "credential.helper"].map(std::ffi::OsString::from);
        let out = self
            .runner
            .run(&neutral_cwd(), CommandKind::Read, &args, None)
            .await?;
        if !out.success() {
            return Ok(false);
        }
        let text = out.stdout_lossy();
        // Un valor vacío reinicia la lista: solo cuenta lo que haya después.
        Ok(text
            .lines()
            .next_back()
            .is_some_and(|last| !last.trim().is_empty()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn target() -> ParsedRemote {
        parse_remote_url("https://dev.azure.com/org/proj/_git/repo", false).unwrap()
    }

    #[test]
    fn secret_no_se_muestra_ni_se_clona() {
        let tok = ["ghp_", "abcdef123456"].concat();
        let s = Secret::new(tok.clone());
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert_eq!(format!("{s}"), "***");
        let f = FilledCredential {
            username: "u".into(),
            secret: s,
        };
        let dbg = format!("{f:?} {f:#?}");
        assert!(!dbg.contains(&tok));
        assert_eq!(f.secret.expose(), tok);
        assert_eq!(f.secret.len(), tok.len());
    }

    #[test]
    fn secret_se_deserializa_pero_no_se_serializa() {
        let s: Secret = serde_json::from_str("\"valor\"").unwrap();
        assert_eq!(s.expose(), "valor");
        // `Secret` no implementa `Serialize` ni `Clone`: lo comprueban los doctests `compile_fail`.
    }

    #[test]
    fn el_cuerpo_de_stdin_lleva_los_campos_en_orden() {
        let t = target();
        let body = credential_input(&t, Some("cualquiera"), Some("tkn")).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&body),
            "protocol=https\nhost=dev.azure.com\npath=org/proj/_git/repo\nusername=cualquiera\npassword=tkn\n\n"
        );
        let body = credential_input(&t, Some("u"), None).unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("password"));
    }

    #[test]
    fn rechaza_inyeccion_de_lineas_en_el_protocolo() {
        let t = target();
        for bad in ["a\nhost=evil.com", "a\rb", "a\0b"] {
            assert!(credential_input(&t, Some(bad), None).is_err(), "{bad:?}");
            let e = credential_input(&t, Some("u"), Some(bad)).unwrap_err();
            assert!(!format!("{e}").contains("evil"), "{e}");
        }
    }

    #[test]
    fn scrub_quita_el_secreto_y_las_credenciales_de_url() {
        let tok = ["tok", "en-secreto"].concat();
        let t = scrub(&format!("eco {tok} y https://u:{tok}@host/x"), Some(&tok));
        assert!(!t.contains(&tok), "{t}");
    }
}
