//! Localización del binario de cada CLI y shims de Windows (RF-IA2).
//!
//! Orden: (1) binario nativo; (2) `node <entrypoint.js>` extraído del shim de npm;
//! (3) shim `.cmd` solo con argumentos de una lista blanca constante.
//! Todo es testeable en Linux: el entorno de búsqueda se pasa como dato.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::commands::cmd_shim_allowed_args;
use crate::models::ProviderId;

/// Cómo se lanza realmente un CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedBinary {
    /// Ejecutable nativo (`claude.exe`, `/usr/bin/codex`...).
    Native(PathBuf),
    /// `node <script>`: se salta el shim `.cmd`.
    NodeEntry { node: PathBuf, script: PathBuf },
    /// Shim `.cmd`: solo admite argumentos de la lista blanca y prompt por stdin.
    CmdShim(PathBuf),
}

/// Errores al preparar el lanzamiento.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    #[error("el shim .cmd no admite el argumento {0:?}: no está en la lista blanca")]
    ArgNotAllowed(String),
}

impl ResolvedBinary {
    /// Programa y argumentos finales para lanzar este binario con `args`.
    ///
    /// Con un [`CmdShim`](ResolvedBinary::CmdShim) falla si algún argumento no está
    /// en la lista blanca constante (RF-IA2).
    pub fn command(&self, args: &[OsString]) -> Result<(PathBuf, Vec<OsString>), ResolveError> {
        match self {
            ResolvedBinary::Native(p) => Ok((p.clone(), args.to_vec())),
            ResolvedBinary::NodeEntry { node, script } => {
                let mut v = vec![script.as_os_str().to_os_string()];
                v.extend(args.iter().cloned());
                Ok((node.clone(), v))
            }
            ResolvedBinary::CmdShim(p) => {
                validate_cmd_shim_args(args)?;
                Ok((p.clone(), args.to_vec()))
            }
        }
    }

    /// Ruta que identifica el binario (para mostrar en Ajustes).
    pub fn display_path(&self) -> &Path {
        match self {
            ResolvedBinary::Native(p) | ResolvedBinary::CmdShim(p) => p,
            ResolvedBinary::NodeEntry { script, .. } => script,
        }
    }
}

/// Falla si algún argumento no está en [`cmd_shim_allowed_args`].
pub fn validate_cmd_shim_args(args: &[OsString]) -> Result<(), ResolveError> {
    let allowed = cmd_shim_allowed_args();
    for a in args {
        let s = a.to_string_lossy();
        if !allowed.contains(&s.as_ref()) {
            return Err(ResolveError::ArgNotAllowed(s.into_owned()));
        }
    }
    Ok(())
}

/// Dónde buscar. Se construye del proceso real o, en tests, a mano.
#[derive(Debug, Clone, Default)]
pub struct SearchEnv {
    /// Directorios de `PATH`.
    pub path_dirs: Vec<PathBuf>,
    pub home: Option<PathBuf>,
    pub appdata: Option<PathBuf>,
    pub localappdata: Option<PathBuf>,
    /// Aplica reglas de Windows (`.exe`, `.cmd`, shims) aunque no se compile para Windows.
    pub windows: bool,
    /// Directorios extra adicionales (Homebrew, etc.).
    pub extra_dirs: Vec<PathBuf>,
}

impl SearchEnv {
    /// Entorno real del proceso.
    pub fn from_process() -> Self {
        let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
        let home = var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).or_else(|| var("HOME"));
        Self {
            path_dirs: std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            home,
            appdata: var("APPDATA"),
            localappdata: var("LOCALAPPDATA"),
            windows: cfg!(windows),
            extra_dirs: vec![PathBuf::from("/opt/homebrew/bin")],
        }
    }

    /// Directorios habituales donde se instalan CLI de usuario, tras `PATH`.
    pub fn candidate_dirs(&self) -> Vec<PathBuf> {
        let mut v = self.path_dirs.clone();
        if let Some(h) = &self.home {
            v.push(h.join(".local").join("bin"));
            v.push(h.join(".npm-global").join("bin"));
            v.push(h.join("scoop").join("shims"));
            v.push(h.join(".volta").join("bin"));
        }
        if let Some(a) = &self.appdata {
            v.push(a.join("npm"));
        }
        if let Some(l) = &self.localappdata {
            v.push(l.join("Volta").join("bin"));
        }
        v.extend(self.extra_dirs.iter().cloned());
        v
    }
}

/// Extrae del shim de npm (`.cmd`, y también `.ps1`) el entrypoint `.js`, relativo
/// al directorio del shim. Función pura y tolerante: devuelve `None` si no lo reconoce.
///
/// Reconoce `"%dp0%\node_modules\...\x.js"`, `"%~dp0\..."` y `"$basedir/node_modules/.../x.js"`.
/// Si hay varias coincidencias, usa la última (la línea que lanza node).
pub fn parse_npm_cmd_shim(contents: &str) -> Option<String> {
    let cmd = Regex::new(r#"(?i)%~?dp0%?[\\/]+([^"%\r\n]+?\.[cm]?js)\b"#).ok()?;
    let ps1 = Regex::new(r#"(?i)\$basedir[\\/]+([^"'\r\n]+?\.[cm]?js)\b"#).ok()?;
    let last = |re: &Regex| {
        re.captures_iter(contents)
            .last()
            .map(|c| c[1].replace('\\', "/"))
    };
    last(&cmd).or_else(|| last(&ps1))
}

fn is_file(p: &Path) -> bool {
    p.is_file()
}

/// Busca `node` junto al shim (npm lo instala allí a veces) o en las rutas de búsqueda.
fn find_node(shim_dir: &Path, env: &SearchEnv) -> Option<PathBuf> {
    let names: &[&str] = if env.windows {
        &["node.exe"]
    } else {
        &["node"]
    };
    std::iter::once(shim_dir.to_path_buf())
        .chain(env.candidate_dirs())
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| is_file(p))
}

/// Decide cómo lanzar un fichero concreto (el configurado, o el encontrado).
fn classify(path: &Path, env: &SearchEnv) -> Option<ResolvedBinary> {
    if !is_file(path) {
        return None;
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    match ext.as_deref() {
        Some("cmd") | Some("bat") | Some("ps1") if env.windows => {
            let dir = path.parent().unwrap_or(Path::new("."));
            let contents = std::fs::read_to_string(path).unwrap_or_default();
            if let Some(rel) = parse_npm_cmd_shim(&contents) {
                let script = dir.join(rel);
                if is_file(&script) {
                    if let Some(node) = find_node(dir, env) {
                        return Some(ResolvedBinary::NodeEntry { node, script });
                    }
                }
            }
            // Un .ps1 no se puede lanzar sin PowerShell; solo el .cmd es un shim válido.
            if ext.as_deref() == Some("ps1") {
                return None;
            }
            Some(ResolvedBinary::CmdShim(path.to_path_buf()))
        }
        _ => Some(ResolvedBinary::Native(path.to_path_buf())),
    }
}

/// Ejecutables nativos que publican los paquetes de npm en rutas **conocidas**, fuera del
/// `PATH`. Hoy solo Copilot: `@github/copilot` es un cargador (`npm-loader.js`) que lanza un
/// binario por plataforma. Lanzarlo directamente evita el proceso intermedio de `node` (una
/// capa menos que matar en la cancelación y nada de `node.exe` en `PATH`).
///
/// Verificado el 2026-11-14 con `@github/copilot 1.0.112` en Windows 11 x64:
/// `%APPDATA%\npm\node_modules\@github\copilot\node_modules\@github\copilot-win32-x64\copilot.exe`.
/// En WSL/Linux el `copilot` del `PATH` es un lanzador propio y se resuelve como nativo.
pub fn well_known_natives(provider: ProviderId, env: &SearchEnv) -> Vec<PathBuf> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return Vec::new(),
    };
    match provider {
        ProviderId::Copilot if env.windows => env
            .appdata
            .as_ref()
            .map(|a| {
                vec![a
                    .join("npm")
                    .join("node_modules")
                    .join("@github")
                    .join("copilot")
                    .join("node_modules")
                    .join("@github")
                    .join(format!("copilot-win32-{arch}"))
                    .join("copilot.exe")]
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Localiza el CLI de `provider` (RF-IA2). `configured` es la ruta de Ajustes: si se da
/// y existe, manda; si se da y no existe, devuelve `None` (no se busca otro en silencio).
pub fn resolve_binary(
    provider: ProviderId,
    configured: Option<&Path>,
    env: &SearchEnv,
) -> Option<ResolvedBinary> {
    if let Some(c) = configured {
        return classify(c, env);
    }
    let name = provider.binary_name();
    let dirs = env.candidate_dirs();

    // (1) nativo en cualquier directorio.
    let native_names: Vec<String> = if env.windows {
        vec![format!("{name}.exe")]
    } else {
        vec![name.to_string()]
    };
    for d in &dirs {
        for n in &native_names {
            let p = d.join(n);
            if is_file(&p) {
                return Some(ResolvedBinary::Native(p));
            }
        }
    }
    // (2) nativo publicado por un paquete de npm en una ruta conocida (Copilot).
    for p in well_known_natives(provider, env) {
        if is_file(&p) {
            return Some(ResolvedBinary::Native(p));
        }
    }
    // (3)/(4) shims de Windows.
    if env.windows {
        for d in &dirs {
            for ext in ["cmd", "bat", "ps1"] {
                let p = d.join(format!("{name}.{ext}"));
                if let Some(r) = classify(&p, env) {
                    return Some(r);
                }
            }
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Shim típico de npm (`npm i -g @openai/codex`), generado por cmd-shim.
    const NPM_CMD: &str = r#"@ECHO off
GOTO start
:find_dp0
SET dp0=%~dp0
EXIT /b
:start
SETLOCAL
CALL :find_dp0

IF EXIST "%dp0%\node.exe" (
  SET "_prog=%dp0%\node.exe"
) ELSE (
  SET "_prog=node"
  SET PATHEXT=%PATHEXT:;.JS;=;%
)

endLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & "%_prog%"  "%dp0%\node_modules\@openai\codex\bin\codex.js" %*
"#;

    /// Variante con `%~dp0` y CRLF, de versiones antiguas de npm.
    const NPM_CMD_OLD: &str = "@IF EXIST \"%~dp0\\node.exe\" (\r\n  \"%~dp0\\node.exe\"  \"%~dp0\\node_modules\\@github\\copilot\\index.js\" %*\r\n) ELSE (\r\n  @SETLOCAL\r\n  @SET PATHEXT=%PATHEXT:;.JS;=;%\r\n  node  \"%~dp0\\node_modules\\@github\\copilot\\index.js\" %*\r\n)\r\n";

    /// `copilot.cmd` **real** de `npm i -g @github/copilot` (1.0.112, Windows 11).
    const COPILOT_CMD: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\@github\\copilot\\npm-loader.js\" %*\r\n";

    /// `copilot.ps1` real: el cargador se invoca con `$basedir` y `npm-loader.js`.
    const COPILOT_PS1: &str = "#!/usr/bin/env pwsh\n$basedir=Split-Path $MyInvocation.MyCommand.Definition -Parent\n$ret=0\nif (Test-Path \"$basedir/node$exe\") {\n  & \"$basedir/node$exe\"  \"$basedir/node_modules/@github/copilot/npm-loader.js\" $args\n} else {\n  & \"node$exe\"  \"$basedir/node_modules/@github/copilot/npm-loader.js\" $args\n}\nexit $ret\n";

    const NPM_PS1: &str = r#"#!/usr/bin/env pwsh
$basedir=Split-Path $MyInvocation.MyCommand.Definition -Parent
$exe=""
if ($PSVersionTable.PSVersion -lt "6.0" -or $IsWindows) {
  $exe=".exe"
}
$ret=0
if (Test-Path "$basedir/node$exe") {
  & "$basedir/node$exe"  "$basedir/node_modules/@openai/codex/bin/codex.js" $args
} else {
  & "node$exe"  "$basedir/node_modules/@openai/codex/bin/codex.js" $args
}
exit $ret
"#;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "x").unwrap();
    }

    #[test]
    fn parsea_shim_cmd_de_npm() {
        assert_eq!(
            parse_npm_cmd_shim(NPM_CMD).as_deref(),
            Some("node_modules/@openai/codex/bin/codex.js")
        );
    }

    #[test]
    fn parsea_shim_cmd_antiguo_con_crlf() {
        assert_eq!(
            parse_npm_cmd_shim(NPM_CMD_OLD).as_deref(),
            Some("node_modules/@github/copilot/index.js")
        );
    }

    #[test]
    fn parsea_shim_ps1() {
        assert_eq!(
            parse_npm_cmd_shim(NPM_PS1).as_deref(),
            Some("node_modules/@openai/codex/bin/codex.js")
        );
    }

    #[test]
    fn parsea_los_shims_reales_de_copilot() {
        for shim in [COPILOT_CMD, COPILOT_PS1] {
            assert_eq!(
                parse_npm_cmd_shim(shim).as_deref(),
                Some("node_modules/@github/copilot/npm-loader.js")
            );
        }
        // El binario de plataforma y el `.exe.old-*` de una actualización no se confunden.
        assert_eq!(parse_npm_cmd_shim("copilot.exe.old-2748"), None);
    }

    #[test]
    fn copilot_prefiere_el_exe_nativo_del_paquete_de_plataforma() {
        let t = tempfile::tempdir().unwrap();
        let npm = t.path().join("npm");
        std::fs::create_dir_all(&npm).unwrap();
        std::fs::write(npm.join("copilot.cmd"), COPILOT_CMD).unwrap();
        std::fs::write(npm.join("copilot.ps1"), COPILOT_PS1).unwrap();
        let exe = npm
            .join("node_modules/@github/copilot/node_modules/@github")
            .join(platform_pkg())
            .join("copilot.exe");
        touch(&exe);
        let env = SearchEnv {
            appdata: Some(t.path().to_path_buf()),
            windows: true,
            ..SearchEnv::default()
        };
        // Existe un `.cmd`, pero se lanza el `.exe` real: sin `node` de por medio no hay
        // proceso nieto que quede vivo si se cancela.
        assert_eq!(
            resolve_binary(ProviderId::Copilot, None, &env),
            Some(ResolvedBinary::Native(exe))
        );
    }

    #[test]
    fn copilot_sin_exe_nativo_usa_el_cargador_de_npm() {
        let t = tempfile::tempdir().unwrap();
        let npm = t.path().join("npm");
        std::fs::create_dir_all(&npm).unwrap();
        std::fs::write(npm.join("copilot.cmd"), COPILOT_CMD).unwrap();
        touch(&npm.join("node_modules/@github/copilot/npm-loader.js"));
        touch(&npm.join("node.exe"));
        let env = SearchEnv {
            appdata: Some(t.path().to_path_buf()),
            windows: true,
            ..SearchEnv::default()
        };
        assert_eq!(
            resolve_binary(ProviderId::Copilot, None, &env),
            Some(ResolvedBinary::NodeEntry {
                node: npm.join("node.exe"),
                script: npm.join("node_modules/@github/copilot/npm-loader.js"),
            })
        );
    }

    #[test]
    fn well_known_en_wsl_no_inventa_rutas_de_windows() {
        let env = SearchEnv {
            appdata: Some(PathBuf::from("/no/aplica")),
            windows: false,
            ..SearchEnv::default()
        };
        assert!(well_known_natives(ProviderId::Copilot, &env).is_empty());
        assert!(well_known_natives(ProviderId::Claude, &env).is_empty());
        // En Linux/WSL el `copilot` del PATH es un lanzador nativo: se usa tal cual.
        let t = tempfile::tempdir().unwrap();
        let launcher = t.path().join("bin").join("copilot");
        touch(&launcher);
        let env = SearchEnv {
            path_dirs: vec![t.path().join("bin")],
            ..SearchEnv::default()
        };
        assert_eq!(
            resolve_binary(ProviderId::Copilot, None, &env),
            Some(ResolvedBinary::Native(launcher))
        );
    }

    fn platform_pkg() -> String {
        format!(
            "copilot-win32-{}",
            if cfg!(target_arch = "aarch64") {
                "arm64"
            } else {
                "x64"
            }
        )
    }

    #[test]
    fn shim_irreconocible_devuelve_none() {
        assert_eq!(
            parse_npm_cmd_shim("@echo off\r\ncodex-real.exe %*\r\n"),
            None
        );
        assert_eq!(parse_npm_cmd_shim(""), None);
    }

    #[test]
    fn lista_blanca_de_cmd_rechaza_datos_del_repo() {
        let ok = vec![OsString::from("exec"), OsString::from("--json")];
        assert!(validate_cmd_shim_args(&ok).is_ok());
        for bad in ["feature/mi-rama", "C:\\repo", "fix: algo", "--bare", ""] {
            let r = validate_cmd_shim_args(&[OsString::from(bad)]);
            assert_eq!(
                r,
                Err(ResolveError::ArgNotAllowed(bad.to_string())),
                "{bad}"
            );
        }
    }

    #[test]
    fn cmd_shim_falla_si_recibe_un_argumento_fuera_de_la_lista() {
        let shim = ResolvedBinary::CmdShim(PathBuf::from("codex.cmd"));
        assert!(shim
            .command(&[OsString::from("exec"), OsString::from("-")])
            .is_ok());
        assert!(shim
            .command(&[OsString::from("exec"), OsString::from("mi-rama-secreta")])
            .is_err());
        // Native y NodeEntry no validan: no pasan por cmd.exe.
        let native = ResolvedBinary::Native(PathBuf::from("claude"));
        assert!(native.command(&[OsString::from("lo-que-sea")]).is_ok());
    }

    #[test]
    fn node_entry_antepone_el_script() {
        let r = ResolvedBinary::NodeEntry {
            node: "node.exe".into(),
            script: "codex.js".into(),
        };
        let (prog, args) = r.command(&[OsString::from("exec")]).unwrap();
        assert_eq!(prog, PathBuf::from("node.exe"));
        assert_eq!(
            args,
            vec![OsString::from("codex.js"), OsString::from("exec")]
        );
    }

    #[test]
    fn prefiere_el_nativo_en_windows() {
        let t = tempfile::tempdir().unwrap();
        let bin = t.path().join(".local").join("bin");
        touch(&bin.join("claude.exe"));
        touch(&t.path().join("npm").join("claude.cmd"));
        let env = SearchEnv {
            home: Some(t.path().to_path_buf()),
            appdata: Some(t.path().to_path_buf()),
            windows: true,
            ..SearchEnv::default()
        };
        assert_eq!(
            resolve_binary(ProviderId::Claude, None, &env),
            Some(ResolvedBinary::Native(bin.join("claude.exe")))
        );
    }

    #[test]
    fn shim_cmd_con_entrypoint_se_convierte_en_node_entry() {
        let t = tempfile::tempdir().unwrap();
        let npm = t.path().join("npm");
        std::fs::create_dir_all(&npm).unwrap();
        std::fs::write(npm.join("codex.cmd"), NPM_CMD).unwrap();
        touch(&npm.join("node_modules/@openai/codex/bin/codex.js"));
        touch(&npm.join("node.exe"));
        let env = SearchEnv {
            appdata: Some(t.path().to_path_buf()),
            windows: true,
            ..SearchEnv::default()
        };
        assert_eq!(
            resolve_binary(ProviderId::Codex, None, &env),
            Some(ResolvedBinary::NodeEntry {
                node: npm.join("node.exe"),
                script: npm.join("node_modules/@openai/codex/bin/codex.js"),
            })
        );
    }

    #[test]
    fn shim_cmd_sin_node_o_sin_script_queda_como_cmd_shim() {
        let t = tempfile::tempdir().unwrap();
        let npm = t.path().join("npm");
        std::fs::create_dir_all(&npm).unwrap();
        std::fs::write(npm.join("copilot.cmd"), "@echo off\r\nraro %*\r\n").unwrap();
        let env = SearchEnv {
            appdata: Some(t.path().to_path_buf()),
            windows: true,
            ..SearchEnv::default()
        };
        assert_eq!(
            resolve_binary(ProviderId::Copilot, None, &env),
            Some(ResolvedBinary::CmdShim(npm.join("copilot.cmd")))
        );
    }

    #[test]
    fn ruta_configurada_manda_y_no_hay_busqueda_silenciosa() {
        let t = tempfile::tempdir().unwrap();
        let mine = t.path().join("mi-codex");
        touch(&mine);
        let env = SearchEnv::default();
        assert_eq!(
            resolve_binary(ProviderId::Codex, Some(&mine), &env),
            Some(ResolvedBinary::Native(mine))
        );
        assert_eq!(
            resolve_binary(ProviderId::Codex, Some(&t.path().join("no-existe")), &env),
            None
        );
    }

    #[test]
    fn busca_en_path_y_en_dirs_extra_en_unix() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join(".npm-global").join("bin");
        touch(&dir.join("codex"));
        let env = SearchEnv {
            home: Some(t.path().to_path_buf()),
            ..SearchEnv::default()
        };
        assert_eq!(
            resolve_binary(ProviderId::Codex, None, &env),
            Some(ResolvedBinary::Native(dir.join("codex")))
        );
        assert_eq!(resolve_binary(ProviderId::Copilot, None, &env), None);

        let env2 = SearchEnv {
            path_dirs: vec![dir],
            ..SearchEnv::default()
        };
        assert!(resolve_binary(ProviderId::Codex, None, &env2).is_some());
    }

    #[test]
    fn busca_en_scoop_y_volta() {
        let t = tempfile::tempdir().unwrap();
        touch(&t.path().join("scoop/shims/codex.exe"));
        let env = SearchEnv {
            home: Some(t.path().to_path_buf()),
            windows: true,
            ..SearchEnv::default()
        };
        assert!(matches!(
            resolve_binary(ProviderId::Codex, None, &env),
            Some(ResolvedBinary::Native(_))
        ));
        let t2 = tempfile::tempdir().unwrap();
        touch(&t2.path().join("Volta/bin/copilot.exe"));
        let env = SearchEnv {
            localappdata: Some(t2.path().to_path_buf()),
            windows: true,
            ..SearchEnv::default()
        };
        assert!(resolve_binary(ProviderId::Copilot, None, &env).is_some());
    }
}
