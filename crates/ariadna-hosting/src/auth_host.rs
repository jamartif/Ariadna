//! Sesión de los CLI de alojamiento (`gh`, `glab`) como método de autenticación.
//!
//! Además del PAT, Ariadna puede usar la sesión que el usuario ya tenga iniciada en GitHub CLI
//! o GitLab CLI: el token se **pide al CLI en cada operación** (`gh auth token`) y se usa en
//! memoria. Nunca se guarda, ni se escribe en un registro, ni viaja en `argv`.
//!
//! Reglas:
//!
//! * **Sin secreto en `argv`**: al CLI solo se le pasan subcomandos fijos (`auth token`,
//!   `auth status`). El token vuelve por **stdout**, se recorta y se envuelve en [`Secret`]
//!   (`zeroize`); el búfer intermedio se borra también.
//! * **Entorno saneado**: se borra todo y se conserva una lista blanca (para que el CLI
//!   encuentre su llavero y su configuración). En particular **no** se heredan
//!   `GH_TOKEN`/`GITHUB_TOKEN`/`GITLAB_TOKEN`: la credencial sale siempre de la sesión del CLI,
//!   no de una variable del entorno de la app.
//! * **Tiempo máximo** corto (10 s): si el llavero pide desbloqueo o el CLI se cuelga, se
//!   responde con un error accionable en lugar de colgar el diálogo de clonado.
//! * `stderr` se descarta siempre (puede traer rutas, nombres de usuario o el token enmascarado)
//!   y el texto de error se construye **sin** la salida del CLI.
//! * Si el CLI no está instalado o no tiene sesión, el error es [`HostingError::Auth`] con el
//!   comando exacto que hay que ejecutar (`gh auth login`).

use std::fmt;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use zeroize::{Zeroize, Zeroizing};

use crate::auth::{AuthMethod, CredentialSource, Secret};
use crate::error::{HostingError, HostingResult};

/// Tope de bytes que se leen de la salida de un CLI (un token ocupa mucho menos).
const OUTPUT_CAP: usize = 64 * 1024;
/// Tiempo máximo de una llamada al CLI.
pub const CLI_TIMEOUT: Duration = Duration::from_secs(10);
/// Un token de sesión más corto que esto no es un token.
const MIN_TOKEN_LEN: usize = 8;
/// Tope defensivo: ningún token real llega a 4 KiB.
const MAX_TOKEN_LEN: usize = 4096;

/// Variables que se heredan al lanzar `gh`/`glab` (configuración y llavero). **Nunca** las que
/// llevan un token (`GH_TOKEN`, `GITHUB_TOKEN`, `GITLAB_TOKEN`, `GLAB_TOKEN`).
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
    "LC_ALL",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_RUNTIME_DIR",
    "XDG_CACHE_HOME",
    "DBUS_SESSION_BUS_ADDRESS",
    // Configuración del propio CLI (rutas, no secretos): si el usuario la movió, el CLI tiene
    // que seguir encontrando su sesión.
    "GH_CONFIG_DIR",
    "GLAB_CONFIG_DIR",
    "GLAB_CHECK_UPDATE",
];

/// CLI de alojamiento soportado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CliTool {
    /// GitHub CLI (`gh`).
    Gh,
    /// GitLab CLI (`glab`).
    Glab,
}

impl CliTool {
    /// Identificador estable para la UI y el contrato IPC.
    pub fn id(self) -> &'static str {
        match self {
            CliTool::Gh => "gh",
            CliTool::Glab => "glab",
        }
    }

    /// Programa que se ejecuta (se busca en `PATH`).
    pub fn program(self) -> &'static str {
        self.id()
    }

    /// Nombre legible del CLI.
    pub fn label(self) -> &'static str {
        match self {
            CliTool::Gh => "GitHub CLI",
            CliTool::Glab => "GitLab CLI",
        }
    }

    /// Comando que el usuario tiene que ejecutar para iniciar sesión.
    pub fn login_command(self) -> &'static str {
        match self {
            CliTool::Gh => "gh auth login",
            CliTool::Glab => "glab auth login",
        }
    }

    /// Usuario por defecto del HTTP Basic. GitHub acepta cualquiera con un token; en GitLab el
    /// usuario convencional para tokens es `oauth2`.
    pub fn default_username(self) -> &'static str {
        match self {
            CliTool::Gh => "x-access-token",
            CliTool::Glab => "oauth2",
        }
    }

    /// Argumentos de la comprobación de sesión (no devuelven el token).
    fn status_args(self) -> &'static [&'static str] {
        match self {
            CliTool::Gh => &["auth", "status"],
            CliTool::Glab => &["auth", "status"],
        }
    }

    /// Argumentos que devuelven el token por `stdout`.
    fn token_args(self) -> &'static [&'static str] {
        match self {
            CliTool::Gh => &["auth", "token"],
            // `glab` no tiene `auth token`: `--show-token` imprime la sesión, con el token.
            CliTool::Glab => &["auth", "status", "--show-token"],
        }
    }

    /// Proveedor de alojamiento al que sirve esta sesión (`ariadna_hosting::Provider` como texto).
    pub fn provider_id(self) -> &'static str {
        match self {
            CliTool::Gh => "github",
            CliTool::Glab => "gitlab",
        }
    }
}

/// Resultado de lanzar un CLI.
#[derive(Debug)]
pub enum CliOutcome {
    /// Terminó (con el código que sea).
    Output(CliRun),
    /// El ejecutable no está en `PATH`.
    Missing,
    /// No terminó dentro del tiempo máximo (se ha matado).
    Timeout,
    /// No se pudo lanzar por otro motivo (permisos, ejecutable roto…).
    Failed,
}

/// Salida ya leída de un CLI, con tope. `stdout` puede llevar el token: se borra al soltarla.
#[derive(Debug)]
pub struct CliRun {
    pub code: Option<i32>,
    pub stdout: Zeroizing<String>,
}

impl CliRun {
    /// `true` si el CLI terminó con código 0.
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// Quién ejecuta el CLI. En producción [`SystemCli`]; en las pruebas, un doble que no lanza
/// procesos (para cubrir los caminos de error sin depender del sistema operativo).
#[async_trait]
pub trait CliRunner: Send + Sync + fmt::Debug {
    async fn run(&self, tool: CliTool, args: &[&str], timeout: Duration) -> CliOutcome;
}

/// [`CliRunner`] real: proceso hijo con entorno saneado, `stdin` cerrado y `stderr` descartado.
#[derive(Debug, Clone, Default)]
pub struct SystemCli {
    /// Solo pruebas: `PATH` a usar en lugar del del proceso (para un `gh` falso).
    path_override: Option<PathBuf>,
    /// Solo pruebas: tiempo máximo distinto del de producción.
    timeout: Option<Duration>,
}

impl SystemCli {
    pub fn new() -> Self {
        Self::default()
    }

    /// **Solo pruebas.** Fija el `PATH` del proceso hijo (dónde se busca `gh`/`glab`).
    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        Self {
            path_override: Some(path.into()),
            timeout: None,
        }
    }

    /// **Solo pruebas.** Cambia el tiempo máximo.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    fn command(&self, tool: CliTool, args: &[&str]) -> Command {
        let mut cmd = Command::new(tool.program());
        cmd.args(args)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        for key in ENV_ALLOWLIST {
            if let Some(v) = std::env::var_os(key) {
                cmd.env(key, v);
            }
        }
        if let Some(path) = &self.path_override {
            cmd.env("PATH", path.as_os_str());
        }
        // Nunca confirmar la actualización ni abrir paginador: no hay terminal.
        cmd.env("GH_PAGER", "cat")
            .env("GH_PROMPT_DISABLED", "1")
            .env("NO_COLOR", "1")
            .env("GLAB_PAGER", "cat");
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        cmd
    }
}

#[async_trait]
impl CliRunner for SystemCli {
    async fn run(&self, tool: CliTool, args: &[&str], timeout: Duration) -> CliOutcome {
        let limit = self.timeout.unwrap_or(timeout);
        let mut child = match self.command(tool, args).spawn() {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return CliOutcome::Missing,
            Err(_) => return CliOutcome::Failed,
        };
        let mut stdout = child.stdout.take();
        let io = async {
            let mut buf = Vec::new();
            if let Some(o) = stdout.as_mut() {
                let _ = o.take(OUTPUT_CAP as u64).read_to_end(&mut buf).await;
            }
            let status = child.wait().await;
            (buf, status)
        };
        match tokio::time::timeout(limit, io).await {
            Ok((mut buf, status)) => {
                let text = String::from_utf8_lossy(&buf).into_owned();
                // El búfer crudo (posible token) se borra en cuanto hay copia en `Zeroizing`.
                buf.zeroize();
                CliOutcome::Output(CliRun {
                    code: status.ok().and_then(|s| s.code()),
                    stdout: Zeroizing::new(text),
                })
            }
            Err(_) => {
                let _ = child.start_kill();
                CliOutcome::Timeout
            }
        }
    }
}

/// Estado de la sesión de un CLI, tal como lo ve la app (sin token).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliSessionStatus {
    pub tool: CliTool,
    /// El ejecutable existe y responde.
    pub installed: bool,
    /// Hay sesión iniciada (el token se puede pedir).
    pub logged_in: bool,
    /// Versión del CLI si se pudo leer (`gh version 2.103.3`).
    pub version: Option<String>,
    /// Por qué no sirve, en español y sin secretos.
    pub detail: Option<String>,
}

impl CliSessionStatus {
    fn missing(tool: CliTool) -> Self {
        Self {
            tool,
            installed: false,
            logged_in: false,
            version: None,
            detail: Some(format!(
                "No se encontró «{}». Instálalo y ejecuta «{}».",
                tool.program(),
                tool.login_command()
            )),
        }
    }

    fn logged_out(tool: CliTool, version: Option<String>) -> Self {
        Self {
            tool,
            installed: true,
            logged_in: false,
            version,
            detail: Some(format!(
                "{} no tiene la sesión iniciada. Ejecuta «{}».",
                tool.label(),
                tool.login_command()
            )),
        }
    }
}

/// Fuente de credencial «sesión del CLI»: pide el token al CLI en cada operación.
#[derive(Clone)]
pub struct CliSession {
    tool: CliTool,
    runner: std::sync::Arc<dyn CliRunner>,
    timeout: Duration,
}

impl fmt::Debug for CliSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CliSession")
            .field("tool", &self.tool)
            .finish_non_exhaustive()
    }
}

impl CliSession {
    /// Sesión con el CLI real del sistema.
    pub fn new(tool: CliTool) -> Self {
        Self::with_runner(tool, std::sync::Arc::new(SystemCli::new()))
    }

    /// Sesión con un ejecutor concreto (pruebas, o [`SystemCli::with_path`]).
    pub fn with_runner(tool: CliTool, runner: std::sync::Arc<dyn CliRunner>) -> Self {
        Self {
            tool,
            runner,
            timeout: CLI_TIMEOUT,
        }
    }

    pub fn tool(&self) -> CliTool {
        self.tool
    }

    /// Inspecciona la sesión **sin** llevarse el token (`<cli> auth status`).
    pub async fn status(&self) -> CliSessionStatus {
        let version = self.version().await;
        match self
            .runner
            .run(self.tool, self.tool.status_args(), self.timeout)
            .await
        {
            CliOutcome::Missing => CliSessionStatus::missing(self.tool),
            CliOutcome::Timeout => CliSessionStatus {
                tool: self.tool,
                installed: true,
                logged_in: false,
                version,
                detail: Some(format!(
                    "«{}» no respondió a tiempo. Comprueba que el llavero del sistema no esté bloqueado.",
                    self.tool.program()
                )),
            },
            CliOutcome::Failed => CliSessionStatus {
                tool: self.tool,
                installed: false,
                logged_in: false,
                version,
                detail: Some(format!(
                    "No se pudo ejecutar «{}».",
                    self.tool.program()
                )),
            },
            CliOutcome::Output(run) => {
                if run.success() {
                    CliSessionStatus {
                        tool: self.tool,
                        installed: true,
                        logged_in: true,
                        version,
                        detail: None,
                    }
                } else {
                    CliSessionStatus::logged_out(self.tool, version)
                }
            }
        }
    }

    /// Versión del CLI (`--version`), primera línea recortada. `None` si no se pudo.
    async fn version(&self) -> Option<String> {
        match self
            .runner
            .run(self.tool, &["--version"], self.timeout)
            .await
        {
            CliOutcome::Output(run) if run.success() => {
                let line = run.stdout.lines().next().unwrap_or_default().trim();
                if line.is_empty() {
                    None
                } else {
                    Some(line.chars().take(120).collect())
                }
            }
            _ => None,
        }
    }

    /// Pide el token a la sesión del CLI. Vive en memoria (`zeroize`) y no se registra.
    pub async fn token(&self) -> HostingResult<Secret> {
        self.token_secret().await
    }

    async fn token_secret(&self) -> HostingResult<Secret> {
        let outcome = self
            .runner
            .run(self.tool, self.tool.token_args(), self.timeout)
            .await;
        let run = match outcome {
            CliOutcome::Missing => {
                return Err(HostingError::auth(
                    CliSessionStatus::missing(self.tool)
                        .detail
                        .unwrap_or_default(),
                    None,
                ))
            }
            CliOutcome::Timeout => {
                return Err(HostingError::auth(
                    format!(
                        "«{}» no respondió a tiempo. Comprueba que el llavero del sistema no esté bloqueado y vuelve a intentarlo.",
                        self.tool.program()
                    ),
                    None,
                ))
            }
            CliOutcome::Failed => {
                return Err(HostingError::auth(
                    format!("No se pudo ejecutar «{}».", self.tool.program()),
                    None,
                ))
            }
            CliOutcome::Output(run) => run,
        };
        if !run.success() {
            return Err(HostingError::auth(
                format!(
                    "{} no tiene la sesión iniciada en este equipo. Ejecuta «{}» y vuelve a intentarlo.",
                    self.tool.label(),
                    self.tool.login_command()
                ),
                Some(self.tool.login_command().to_owned()),
            ));
        }
        // El token se copia directamente a un `Zeroizing`: no queda ninguna copia sin borrar.
        let token = match self.tool {
            // `gh auth token` imprime exactamente el token.
            CliTool::Gh => Zeroizing::new(run.stdout.trim().to_owned()),
            // `glab auth status --show-token` imprime la sesión; el token va en su línea.
            CliTool::Glab => {
                parse_shown_token(&run.stdout).unwrap_or_else(|| Zeroizing::new(String::new()))
            }
        };
        // `run` se suelta aquí y su `stdout` (Zeroizing) se borra.
        drop(run);
        validate_token(self.tool, &token)?;
        Ok(Secret::new(token.as_str()))
    }
}

/// Extrae el token de una salida tipo `glab auth status --show-token`
/// (`  - Token: glpat-…`). Devuelve el **último** válido y nunca una máscara.
fn parse_shown_token(text: &str) -> Option<Zeroizing<String>> {
    let mut found: Option<Zeroizing<String>> = None;
    for line in text.lines() {
        let Some(idx) = line.to_ascii_lowercase().find("token:") else {
            continue;
        };
        let value = line[idx + "token:".len()..]
            .trim()
            .trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .trim();
        if value.is_empty() || value.contains('*') || value.chars().any(char::is_whitespace) {
            continue;
        }
        found = Some(Zeroizing::new(value.to_owned()));
    }
    found
}

/// Un token válido es imprimible, sin espacios dentro, de longitud razonable.
fn validate_token(tool: CliTool, token: &str) -> HostingResult<()> {
    let bad = |why: &str| {
        HostingError::auth(
            format!(
                "La sesión de {} no devolvió un token utilizable ({why}). Ejecuta «{}» otra vez.",
                tool.label(),
                tool.login_command()
            ),
            Some(tool.login_command().to_owned()),
        )
    };
    if token.len() < MIN_TOKEN_LEN {
        return Err(bad("vacío o demasiado corto"));
    }
    if token.len() > MAX_TOKEN_LEN {
        return Err(bad("demasiado largo"));
    }
    if token.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad("tiene espacios o caracteres de control"));
    }
    Ok(())
}

#[async_trait]
impl CredentialSource for CliSession {
    async fn credentials(&self, url: &str) -> HostingResult<AuthMethod> {
        url::Url::parse(url.trim())
            .ok()
            .filter(|u| u.scheme() == "https" && u.host_str().is_some())
            .ok_or_else(|| HostingError::invalid("La dirección del remoto no es https."))?;
        let secret = self.token_secret().await?;
        Ok(AuthMethod::Pat {
            username: self.tool.default_username().to_owned(),
            secret,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parsea_el_token_de_glab_y_descarta_la_mascara() {
        let txt = |o: Option<Zeroizing<String>>| o.map(|s| s.to_string()).unwrap_or_default();
        let masked = "gitlab.com\n  ✓ Logged in (keyring)\n  - Token: glpa********\n";
        assert_eq!(txt(parse_shown_token(masked)), "");
        // Los tokens se construyen en tiempo de ejecución: si el literal aparece en el código,
        // gitleaks (que revisa todo el historial) lo marca como secreto.
        let tok = format!("glpat-{}", "abcdef123");
        let real = format!("gitlab.com\n  ✓ Logged in\n  - Token: {tok}\n  - Token scopes: api\n");
        assert_eq!(txt(parse_shown_token(&real)), tok);
        // Varias cuentas: gana la última válida.
        let dos = format!("glpat-{}", "dos");
        let two = format!("  - Token: glpat-uno\n  - Token: {dos}\n");
        assert_eq!(txt(parse_shown_token(&two)), dos);
        assert!(parse_shown_token("sin nada").is_none());
    }

    #[test]
    fn valida_el_token_sin_repetirlo_en_el_error() {
        let ok = validate_token(CliTool::Gh, "gho_0123456789abcdef");
        assert!(ok.is_ok());
        // Tokens «reales» con un marca reconocible: el mensaje nunca repite su valor.
        for bad in ["", "abc", "gho_0123 4567", "gho_0123\t4567"] {
            let err = validate_token(CliTool::Gh, bad).unwrap_err();
            let text = format!("{err:?} {err}");
            assert!(!text.contains("gho_"), "el token se repite: {text}");
            assert!(text.contains("gh auth login"), "{text}");
        }
        let long = "x".repeat(MAX_TOKEN_LEN + 1);
        assert!(validate_token(CliTool::Glab, &long).is_err());
    }

    #[test]
    fn debug_de_la_sesion_no_lleva_secreto() {
        let s = CliSession::new(CliTool::Gh);
        let text = format!("{s:?}");
        assert!(text.contains("Gh") && !text.contains("token"));
    }

    #[test]
    fn las_variables_de_token_no_se_heredan() {
        // La lista blanca no debe incluir ninguna variable con credencial.
        for name in ["GH_TOKEN", "GITHUB_TOKEN", "GITLAB_TOKEN", "GLAB_TOKEN"] {
            assert!(!ENV_ALLOWLIST.contains(&name), "{name} no puede heredarse");
        }
    }
}
