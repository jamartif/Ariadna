//! Inicio de sesión oficial de cada CLI, en el entorno aislado del perfil (spec §5.1.1).
//!
//! Principios:
//! - **Ariadna nunca ve ni pide credenciales**: solo abre una terminal (o muestra el comando)
//!   donde el propio CLI hace el login (SSO incluido).
//! - Nada derivado del repo entra aquí: el programa, los argumentos y las variables son
//!   constantes más la ruta del directorio de configuración del perfil, que genera Ariadna.
//! - Abrir una terminal es la parte menos portable. Si un valor no se puede entrecomillar con
//!   seguridad en el intérprete de destino, o el sistema no está soportado, no se lanza nada y
//!   la interfaz muestra el comando para que el usuario lo copie.
//!
//! Verificado: en Windows el CLI de Claude arranca con `claude auth login`
//! (`claude 2.1.319`). Codex y Copilot (`codex login`, `copilot login`) **no están verificados**.

use std::path::{Path, PathBuf};

use crate::commands::login_args;
use crate::env::{config_dir_var, ALWAYS_REMOVE};
use crate::models::{AiError, ProviderId};

/// Intérprete de la terminal de destino.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    /// `sh`/`zsh`/`bash` (macOS, Linux).
    Posix,
    /// `cmd.exe` (Windows).
    Cmd,
}

impl Shell {
    /// El intérprete del sistema en el que corre Ariadna.
    pub fn native() -> Self {
        if cfg!(windows) {
            Shell::Cmd
        } else {
            Shell::Posix
        }
    }

    fn script_extension(self) -> &'static str {
        match self {
            Shell::Posix => "command",
            Shell::Cmd => "cmd",
        }
    }
}

/// Qué ejecutar para iniciar sesión.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginPlan {
    pub provider: ProviderId,
    /// Ejecutable del CLI (ruta absoluta si se resolvió, si no el nombre).
    pub program: String,
    /// Argumentos constantes (`auth login`, `login`…).
    pub args: Vec<String>,
    /// Variables que se fijan antes de lanzar el CLI (el directorio de configuración aislado).
    pub set_env: Vec<(String, String)>,
    /// Variables que se eliminan (las prohibidas de RF-IA3: una API key ajena cambiaría de cuenta).
    pub unset_env: Vec<String>,
}

fn unsafe_value(shell: Shell, what: &str) -> AiError {
    AiError::Io(format!(
        "{what} contiene caracteres que no se pueden entrecomillar con seguridad en {}",
        match shell {
            Shell::Posix => "una terminal POSIX",
            Shell::Cmd => "cmd.exe",
        }
    ))
}

/// Entrecomilla un valor para el intérprete, o falla si no se puede hacer con seguridad.
fn quote(shell: Shell, value: &str, what: &str) -> Result<String, AiError> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(unsafe_value(shell, what));
    }
    match shell {
        Shell::Posix => Ok(format!("'{}'", value.replace('\'', r"'\''"))),
        Shell::Cmd => {
            // Dentro de comillas dobles de cmd.exe, `%` se expande siempre y `"` cierra la
            // comilla: no hay escape fiable. `^` y `!` se rechazan por prudencia.
            if value.chars().any(|c| matches!(c, '"' | '%' | '^' | '!')) {
                return Err(unsafe_value(shell, what));
            }
            Ok(format!("\"{value}\""))
        }
    }
}

fn is_cmd_shim(program: &str) -> bool {
    let p = program.to_ascii_lowercase();
    p.ends_with(".cmd") || p.ends_with(".bat")
}

impl LoginPlan {
    /// `binary`: ruta resuelta del CLI (si no, se usa el nombre del ejecutable).
    /// `config_dir`: directorio aislado del perfil; solo se aplica si el proveedor lo admite.
    pub fn new(provider: ProviderId, binary: Option<&Path>, config_dir: Option<&Path>) -> Self {
        let set_env = match (config_dir_var(provider), config_dir) {
            (Some(var), Some(dir)) => vec![(var.to_string(), dir.to_string_lossy().into_owned())],
            _ => Vec::new(),
        };
        Self {
            provider,
            program: binary.map_or_else(
                || provider.binary_name().to_string(),
                |b| b.to_string_lossy().into_owned(),
            ),
            args: login_args(provider)
                .into_iter()
                .map(str::to_string)
                .collect(),
            set_env,
            unset_env: ALWAYS_REMOVE.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    /// Una línea para copiar y pegar en la terminal del usuario. Usa el nombre del CLI (no la
    /// ruta absoluta) para que sea legible y portable; incluye la variable de aislamiento.
    pub fn command_line(&self, shell: Shell) -> Result<String, AiError> {
        let mut parts: Vec<String> = Vec::new();
        match shell {
            Shell::Posix => {
                for (k, v) in &self.set_env {
                    parts.push(format!(
                        "{k}={}",
                        quote(shell, v, "el directorio de configuración")?
                    ));
                }
            }
            Shell::Cmd => {}
        }
        let mut run = vec![self.provider.binary_name().to_string()];
        run.extend(self.args.iter().cloned());
        let run = run.join(" ");
        match shell {
            Shell::Posix => {
                parts.push(run);
                Ok(parts.join(" "))
            }
            Shell::Cmd => {
                let mut sets: Vec<String> = Vec::new();
                for (k, v) in &self.set_env {
                    let q = quote(shell, v, "el directorio de configuración")?;
                    // `set "VAR=valor"`: se reutiliza el entrecomillado sin repetir las comillas.
                    sets.push(format!("set \"{k}={}\"", q.trim_matches('"')));
                }
                sets.push(run);
                Ok(sets.join(" && "))
            }
        }
    }

    /// Contenido del script que abre la terminal. Solo ASCII en el título (el `.cmd` no
    /// declara codificación).
    pub fn script(&self, shell: Shell, title: &str) -> Result<String, AiError> {
        let title: String = title
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '(' | ')'))
            .collect();
        let mut out = String::new();
        match shell {
            Shell::Posix => {
                out.push_str("#!/bin/sh\n");
                out.push_str(&format!(
                    "printf '\\033]0;%s\\007' {}\n",
                    quote(shell, &title, "el título")?
                ));
                for k in &self.unset_env {
                    out.push_str(&format!("unset {k}\n"));
                }
                for (k, v) in &self.set_env {
                    out.push_str(&format!(
                        "export {k}={}\n",
                        quote(shell, v, "el directorio de configuración")?
                    ));
                }
                let mut line = quote(shell, &self.program, "la ruta del CLI")?;
                for a in &self.args {
                    line.push(' ');
                    line.push_str(a);
                }
                out.push_str(&format!("{line}\n"));
                out.push_str("echo\n");
                out.push_str(
                    "echo 'Puedes cerrar esta ventana. Vuelve a Ariadna y pulsa Actualizar.'\n",
                );
                out.push_str("printf 'Pulsa Intro para cerrar... '; read _\n");
            }
            Shell::Cmd => {
                out.push_str("@echo off\r\n");
                out.push_str(&format!("title {title}\r\n"));
                for k in &self.unset_env {
                    out.push_str(&format!("set \"{k}=\"\r\n"));
                }
                for (k, v) in &self.set_env {
                    let q = quote(shell, v, "el directorio de configuración")?;
                    out.push_str(&format!("set \"{k}={}\"\r\n", q.trim_matches('"')));
                }
                let prog = quote(shell, &self.program, "la ruta del CLI")?;
                let call = if is_cmd_shim(&self.program) {
                    "call "
                } else {
                    ""
                };
                out.push_str(&format!("{call}{prog} {}\r\n", self.args.join(" ")));
                out.push_str("echo.\r\n");
                out.push_str(
                    "echo Puedes cerrar esta ventana. Vuelve a Ariadna y pulsa Actualizar.\r\n",
                );
                out.push_str("pause\r\n");
            }
        }
        Ok(out)
    }
}

/// Nombre del script de un proveedor (sin datos del repo).
pub fn script_file_name(provider: ProviderId, shell: Shell) -> String {
    format!(
        "login-{}.{}",
        provider.binary_name(),
        shell.script_extension()
    )
}

/// Escribe el script en `dir` (se sobrescribe en cada intento). En Unix queda ejecutable.
pub fn write_script(
    dir: &Path,
    plan: &LoginPlan,
    shell: Shell,
    title: &str,
) -> Result<PathBuf, AiError> {
    let content = plan.script(shell, title)?;
    std::fs::create_dir_all(dir).map_err(|e| AiError::Io(e.to_string()))?;
    let path = dir.join(script_file_name(plan.provider, shell));
    std::fs::write(&path, content).map_err(|e| AiError::Io(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| AiError::Io(e.to_string()))?;
    }
    Ok(path)
}

/// Abre una terminal que ejecuta un script. Detrás de un trait para que los tests no abran
/// ninguna ventana.
pub trait TerminalLauncher: Send + Sync {
    /// `Ok(())` si la terminal se ha lanzado. Un `Err` significa «muestra el comando».
    fn open(&self, script: &Path) -> Result<(), AiError>;
}

/// Terminal del sistema: `cmd /c start` en Windows, `open` (Terminal.app) en macOS. En otros
/// sistemas no hay una forma portable de elegir terminal y no se abre ninguna.
#[derive(Debug, Clone, Copy, Default)]
pub struct OsTerminal;

impl TerminalLauncher for OsTerminal {
    #[cfg(windows)]
    fn open(&self, script: &Path) -> Result<(), AiError> {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let path = script.to_string_lossy();
        // El título vacío evita que `start` tome la ruta entrecomillada como título.
        let quoted = quote(Shell::Cmd, &path, "la ruta del script")?;
        std::process::Command::new("cmd.exe")
            .raw_arg(format!("/d /c start \"\" {quoted}"))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|e| AiError::Io(format!("no se pudo abrir la terminal: {e}")))
    }

    #[cfg(target_os = "macos")]
    fn open(&self, script: &Path) -> Result<(), AiError> {
        // Los `.command` se abren en Terminal.app. La ruta va como argumento propio.
        std::process::Command::new("open")
            .arg(script)
            .spawn()
            .map(|_| ())
            .map_err(|e| AiError::Io(format!("no se pudo abrir la terminal: {e}")))
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    fn open(&self, _script: &Path) -> Result<(), AiError> {
        Err(AiError::Io(
            "abrir una terminal no está soportado en este sistema".into(),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn plan(p: ProviderId, dir: Option<&str>) -> LoginPlan {
        LoginPlan::new(p, None, dir.map(Path::new))
    }

    #[test]
    fn claude_aislado_en_posix_y_cmd() {
        let p = plan(ProviderId::Claude, Some("/datos/perfiles/empresa/claude"));
        assert_eq!(
            p.command_line(Shell::Posix).unwrap(),
            "CLAUDE_CONFIG_DIR='/datos/perfiles/empresa/claude' claude auth login"
        );
        let p = plan(
            ProviderId::Claude,
            Some(r"C:\Users\ana\AppData\ariadna\empresa"),
        );
        assert_eq!(
            p.command_line(Shell::Cmd).unwrap(),
            r#"set "CLAUDE_CONFIG_DIR=C:\Users\ana\AppData\ariadna\empresa" && claude auth login"#
        );
    }

    #[test]
    fn codex_usa_codex_home_y_copilot_nunca_lleva_variable() {
        let c = plan(ProviderId::Codex, Some("/x"));
        assert_eq!(
            c.command_line(Shell::Posix).unwrap(),
            "CODEX_HOME='/x' codex login"
        );
        // Copilot no tiene aislamiento verificado: ni aunque se le pase un directorio.
        let k = plan(ProviderId::Copilot, Some("/x"));
        assert!(k.set_env.is_empty());
        assert_eq!(k.command_line(Shell::Posix).unwrap(), "copilot login");
    }

    #[test]
    fn sin_directorio_no_hay_variable() {
        let p = plan(ProviderId::Claude, None);
        assert_eq!(p.command_line(Shell::Posix).unwrap(), "claude auth login");
        assert_eq!(p.command_line(Shell::Cmd).unwrap(), "claude auth login");
    }

    #[test]
    fn posix_escapa_comillas_simples() {
        let p = plan(ProviderId::Claude, Some("/o'brien/x"));
        assert_eq!(
            p.command_line(Shell::Posix).unwrap(),
            r"CLAUDE_CONFIG_DIR='/o'\''brien/x' claude auth login"
        );
    }

    #[test]
    fn cmd_rechaza_valores_que_no_se_pueden_entrecomillar() {
        for bad in [
            r"C:\a%PATH%\b",
            r#"C:\a"b"#,
            r"C:\a^b",
            "C:\\a!b",
            "C:\\a\nb",
        ] {
            let p = plan(ProviderId::Claude, Some(bad));
            assert!(p.command_line(Shell::Cmd).is_err(), "{bad:?}");
            assert!(p.script(Shell::Cmd, "t").is_err(), "{bad:?}");
        }
        // Un espacio sí se puede entrecomillar.
        let ok = plan(ProviderId::Claude, Some(r"C:\Mis datos\x"));
        assert!(ok
            .command_line(Shell::Cmd)
            .unwrap()
            .contains(r#"C:\Mis datos\x"#));
    }

    #[test]
    fn el_script_limpia_variables_prohibidas_y_fija_el_aislamiento() {
        let p = LoginPlan::new(
            ProviderId::Claude,
            Some(Path::new("/usr/bin/claude")),
            Some(Path::new("/d/empresa")),
        );
        let s = p
            .script(Shell::Posix, "Ariadna - Claude (Empresa)")
            .unwrap();
        assert!(s.starts_with("#!/bin/sh\n"));
        for banned in ALWAYS_REMOVE {
            assert!(s.contains(&format!("unset {banned}\n")), "{banned}");
        }
        assert!(s.contains("export CLAUDE_CONFIG_DIR='/d/empresa'\n"));
        assert!(s.contains("'/usr/bin/claude' auth login\n"));
        // Ningún secreto ni dato del repo: solo constantes y la ruta del perfil.
        assert!(!s.contains("sk-"));

        let w = LoginPlan::new(
            ProviderId::Codex,
            Some(Path::new(r"C:\npm\codex.cmd")),
            Some(Path::new(r"C:\d\empresa")),
        )
        .script(Shell::Cmd, "Ariadna - Codex (Empresa) é")
        .unwrap();
        assert!(w.contains("\r\n") && w.starts_with("@echo off\r\n"));
        assert!(w.contains("title Ariadna - Codex (Empresa) \r\n"), "{w}");
        assert!(w.contains("set \"OPENAI_API_KEY=\"\r\n"));
        assert!(w.contains("set \"CODEX_HOME=C:\\d\\empresa\"\r\n"));
        assert!(w.contains("call \"C:\\npm\\codex.cmd\" login\r\n"));
        assert!(w.is_ascii());
    }

    #[test]
    fn write_script_crea_el_archivo_ejecutable() {
        let tmp = tempfile::tempdir().unwrap();
        let p = plan(ProviderId::Claude, Some("/x"));
        let path = write_script(tmp.path(), &p, Shell::Posix, "t").unwrap();
        assert_eq!(path.file_name().unwrap(), "login-claude.command");
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("CLAUDE_CONFIG_DIR"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111);
        }
        assert_eq!(
            script_file_name(ProviderId::Codex, Shell::Cmd),
            "login-codex.cmd"
        );
    }

    #[test]
    fn argumentos_constantes() {
        for p in ProviderId::AUTO_ORDER {
            let plan = plan(p, Some("/x"));
            assert!(!plan.args.iter().any(|a| a == "--bare"));
            assert!(plan.args.len() <= 2);
        }
    }
}
