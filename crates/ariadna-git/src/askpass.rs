//! Credencial de **un solo uso** para operaciones de red (clonar con un PAT, spec §4.3 / D20).
//!
//! # Mecanismo (`GIT_ASKPASS` temporal)
//!
//! El token **no se guarda**: ni en disco, ni en el gestor de credenciales, ni en `argv`, ni en
//! la URL, ni en `.git/config`. Se hace así:
//!
//! 1. Se crea una carpeta temporal privada (`0700` en Unix) con un script `askpass` **sin
//!    secretos**: solo imprime la variable `ARIADNA_ASKPASS_USER` si el aviso de git pide el
//!    usuario, o `ARIADNA_ASKPASS_SECRET` en cualquier otro caso.
//! 2. Git se ejecuta con `GIT_ASKPASS=<script>` y esas dos variables **solo en el entorno del
//!    proceso hijo** (memoria). Con `-c credential.helper=` se vacía la lista de helpers: así ni
//!    el Git Credential Manager ni el llavero reciben `credential approve` tras el éxito.
//! 3. Al terminar (éxito, error o cancelación) se borra la carpeta temporal; el secreto
//!    ([`Secret`]) se pone a cero al soltarse. Cualquier texto de error se limpia con
//!    [`scrub_error`] antes de salir del motor.
//!
//! Limitación honesta: el sistema operativo copia el entorno del hijo (`String` no zeroizable)
//! mientras git corre; solo es legible por el mismo usuario y desaparece al terminar el proceso.

use std::fmt;
use std::path::PathBuf;

use crate::credentials::Secret;
use crate::error::{GitError, GitResult};

/// Variable con el usuario que lee el script `askpass`.
pub(crate) const USER_VAR: &str = "ARIADNA_ASKPASS_USER";
/// Variable con el secreto que lee el script `askpass`.
pub(crate) const SECRET_VAR: &str = "ARIADNA_ASKPASS_SECRET";
/// Usuario por defecto: GitHub, Azure DevOps y GitLab ignoran el nombre cuando la contraseña es un PAT.
const DEFAULT_USER: &str = "git";

/// Usuario y token de un solo uso. No es `Clone` ni `Serialize`; `Debug` va redactado.
pub struct CloneAuth {
    username: String,
    secret: Secret,
}

impl CloneAuth {
    /// `username` vacío = «git». Falla si algún campo lleva caracteres de control.
    pub fn new(username: &str, secret: Secret) -> GitResult<Self> {
        let username = username.trim();
        if username.chars().any(char::is_control) {
            return Err(GitError::InvalidReference(
                "usuario (no puede contener caracteres de control)".into(),
            ));
        }
        if secret.is_empty() {
            return Err(GitError::InvalidReference(
                "el token no puede estar vacío".into(),
            ));
        }
        if secret.expose().chars().any(char::is_control) {
            return Err(GitError::InvalidReference(
                "el token no puede contener caracteres de control".into(),
            ));
        }
        Ok(Self {
            username: if username.is_empty() {
                DEFAULT_USER.to_owned()
            } else {
                username.to_owned()
            },
            secret,
        })
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub(crate) fn secret(&self) -> &str {
        self.secret.expose()
    }
}

impl fmt::Debug for CloneAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloneAuth")
            .field("username", &self.username)
            .field("secret", &"***")
            .finish()
    }
}

/// Script `askpass` en una carpeta temporal privada; se borra al soltarlo.
pub(crate) struct AskpassScript {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

const SCRIPT: &str = "#!/bin/sh\n\
case \"$1\" in\n\
  *sername*) printf '%s\\n' \"$ARIADNA_ASKPASS_USER\" ;;\n\
  *) printf '%s\\n' \"$ARIADNA_ASKPASS_SECRET\" ;;\n\
esac\n";

impl AskpassScript {
    pub(crate) fn create() -> GitResult<Self> {
        let dir = tempfile::Builder::new()
            .prefix("ariadna-askpass-")
            .tempdir()
            .map_err(|e| GitError::Io(format!("No pude crear la carpeta temporal: {e}")))?;
        let path = dir.path().join("askpass.sh");
        std::fs::write(&path, SCRIPT)
            .map_err(|e| GitError::Io(format!("No pude preparar la credencial temporal: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).map_err(
                |e| GitError::Io(format!("No pude proteger la credencial temporal: {e}")),
            )?;
        }
        Ok(Self { _dir: dir, path })
    }

    /// Valor de `GIT_ASKPASS`. Git (≥ 2.39) lanza el programa directamente, sin `sh -c`, con
    /// el aviso como único argumento: **no** se entrecomilla (un `"` formaría parte del nombre).
    /// En Windows git for Windows interpreta el `#!/bin/sh` del script.
    pub(crate) fn git_askpass_value(&self) -> String {
        dunce::simplified(&self.path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    #[cfg(test)]
    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }
}

/// Quita el secreto de cualquier texto del error (y las credenciales incrustadas en URL).
pub fn scrub_error(err: GitError, secret: &str) -> GitError {
    let clean = |s: String| -> String {
        let mut t = crate::runner::redact_credentials(&s);
        if !secret.is_empty() {
            t = t.replace(secret, "***");
        }
        t
    };
    match err {
        GitError::CommandFailed {
            command,
            exit_code,
            stderr,
        } => GitError::CommandFailed {
            command: clean(command),
            exit_code,
            stderr: clean(stderr),
        },
        GitError::Auth { host, hint } => GitError::Auth {
            host: clean(host),
            hint: clean(hint),
        },
        GitError::GitNotFound(s) => GitError::GitNotFound(clean(s)),
        GitError::InvalidReference(s) => GitError::InvalidReference(clean(s)),
        GitError::Parse(s) => GitError::Parse(clean(s)),
        GitError::Io(s) => GitError::Io(clean(s)),
        GitError::LocalChangesBlockCheckout { detail } => GitError::LocalChangesBlockCheckout {
            detail: clean(detail),
        },
        GitError::Timeout { command } => GitError::Timeout {
            command: clean(command),
        },
        other => other,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn el_script_no_contiene_el_secreto_y_se_borra() {
        let tok = ["ghp_", "supersecreto123"].concat();
        let auth = CloneAuth::new("", Secret::new(tok.clone())).unwrap();
        assert_eq!(auth.username(), "git");
        let script = AskpassScript::create().unwrap();
        let path = script.path().to_path_buf();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(!body.contains(&tok));
        assert!(body.contains(SECRET_VAR) && body.contains(USER_VAR));
        assert!(!script.git_askpass_value().contains('"'));
        drop(script);
        assert!(!path.exists(), "la carpeta temporal debe borrarse");
        assert!(!format!("{auth:?}").contains(&tok));
    }

    #[cfg(unix)]
    #[test]
    fn el_script_responde_usuario_o_secreto_segun_el_aviso() {
        let script = AskpassScript::create().unwrap();
        let run = |prompt: &str| {
            let out = std::process::Command::new(script.path())
                .arg(prompt)
                .env(USER_VAR, "alguien")
                .env(SECRET_VAR, "tok-123")
                .output()
                .unwrap();
            String::from_utf8(out.stdout).unwrap()
        };
        assert_eq!(run("Username for 'https://h': "), "alguien\n");
        assert_eq!(run("Password for 'https://alguien@h': "), "tok-123\n");
    }

    #[test]
    fn rechaza_campos_con_control_o_token_vacio() {
        assert!(CloneAuth::new("a\nb", Secret::new("x")).is_err());
        assert!(CloneAuth::new("a", Secret::new("")).is_err());
        let e = CloneAuth::new("a", Secret::new("to\nken")).unwrap_err();
        assert!(!e.to_string().contains("to\nken"));
    }

    #[test]
    fn scrub_error_limpia_todas_las_variantes_con_texto() {
        let tok = "tok-ultra-secreto";
        let errs = [
            GitError::CommandFailed {
                command: format!("git clone https://u:{tok}@h/x"),
                exit_code: 128,
                stderr: format!("fatal: eco {tok}"),
            },
            GitError::Auth {
                host: "h".into(),
                hint: format!("usa {tok}"),
            },
            GitError::Io(format!("fallo {tok}")),
            GitError::InvalidReference(tok.into()),
            GitError::Parse(tok.into()),
            GitError::GitNotFound(tok.into()),
        ];
        for e in errs {
            let s = scrub_error(e, tok);
            assert!(!format!("{s:?} {s}").contains(tok), "{s:?}");
        }
    }
}
