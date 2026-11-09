//! Localizador de `git` (spec §4.1): se resuelve **una vez** a ruta absoluta y nunca usa el
//! directorio actual, para que un `git.exe` plantado dentro de un repo no se ejecute.
//!
//! * Windows: rutas conocidas (`Program Files\Git\cmd\git.exe`, `%LOCALAPPDATA%\Programs\Git`,
//!   Scoop, Chocolatey) y después PATH. No se consulta el registro (no hay crate para ello).
//! * macOS/Linux: PATH del usuario y después `/opt/homebrew/bin`, `/usr/local/bin`,
//!   Command Line Tools y `/usr/bin`. `/usr/bin/git` va el último en macOS porque, sin las
//!   herramientas de línea de comandos, es un *shim* que abre un diálogo de instalación.
//!
//! Los candidatos y el entorno son inyectables para poder probarlo en cualquier sistema.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

/// Versión mínima soportada de git.
pub const MIN_GIT_VERSION: GitVersion = GitVersion {
    major: 2,
    minor: 39,
    patch: 0,
};

/// Versión de git (`major.minor.patch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl PartialOrd for GitVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for GitVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}
impl fmt::Display for GitVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Interpreta la salida de `git --version`:
/// `git version 2.53.0.windows.3`, `git version 2.39.5 (Apple Git-154)`, `git version 2.45.0-rc1`.
pub fn parse_git_version(output: &str) -> Option<GitVersion> {
    let token = output
        .trim()
        .strip_prefix("git version ")?
        .split_whitespace()
        .next()?;
    let mut numbers = [0u32; 3];
    let mut count = 0;
    for segment in token.split('.').take(3) {
        let digits: String = segment.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            break;
        }
        numbers[count] = digits.parse().ok()?;
        count += 1;
        // `2.45.0-rc1`: lo que sigue a los dígitos de este segmento corta la versión.
        if digits.len() != segment.len() {
            break;
        }
    }
    (count >= 2).then_some(GitVersion {
        major: numbers[0],
        minor: numbers[1],
        patch: numbers[2],
    })
}

/// Sistema para el que se calculan candidatos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOs {
    Windows,
    MacOs,
    Linux,
}

impl TargetOs {
    /// Sistema en el que se compiló.
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
    fn exe_name(self) -> &'static str {
        if self == Self::Windows {
            "git.exe"
        } else {
            "git"
        }
    }
}

/// Opciones de búsqueda.
#[derive(Debug, Clone)]
pub struct LocatorOptions {
    /// Sistema objetivo (para los candidatos por defecto).
    pub os: TargetOs,
    /// Candidatos que se prueban antes que todo lo demás (p. ej. una ruta elegida por el usuario).
    pub extra_candidates: Vec<PathBuf>,
    /// Incluir las ubicaciones conocidas del sistema.
    pub include_known_locations: bool,
    /// Incluir las carpetas del PATH.
    pub include_path: bool,
    /// Valor de PATH. `None` = el del proceso.
    pub path_var: Option<OsString>,
    /// Variables de entorno para expandir `%ProgramFiles%`, etc. `None` = las del proceso.
    pub env: Option<HashMap<String, OsString>>,
    /// Versión mínima exigida.
    pub min_version: GitVersion,
    /// Carpeta neutra donde se ejecuta `git --version`. `None` = la temporal del sistema.
    pub work_dir: Option<PathBuf>,
    /// Plazo para que `git --version` responda.
    pub timeout: Duration,
}

impl Default for LocatorOptions {
    fn default() -> Self {
        Self {
            os: TargetOs::current(),
            extra_candidates: Vec::new(),
            include_known_locations: true,
            include_path: true,
            path_var: None,
            env: None,
            min_version: MIN_GIT_VERSION,
            work_dir: None,
            timeout: Duration::from_secs(15),
        }
    }
}

impl LocatorOptions {
    fn env_var(&self, name: &str) -> Option<OsString> {
        match &self.env {
            Some(map) => map.get(name).cloned(),
            None => std::env::var_os(name),
        }
    }
}

/// `git` localizado y validado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitInstall {
    /// Ruta absoluta al ejecutable.
    pub path: PathBuf,
    pub version: GitVersion,
    /// Salida literal de `git --version`.
    pub raw_version: String,
}

/// Fallos de localización, con texto accionable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GitLocateError {
    #[error("no se encontró git. Instala Git (https://git-scm.com/downloads) o indica su ruta en Ajustes. Se buscó en: {}", join_paths(.searched))]
    NotFound { searched: Vec<PathBuf> },
    #[error("git {found} en {path:?} es demasiado antiguo: Ariadna necesita {required} o posterior. Actualiza Git (https://git-scm.com/downloads).")]
    TooOld {
        path: PathBuf,
        found: GitVersion,
        required: GitVersion,
    },
    #[error("no se pudo interpretar la versión de git de {path:?}: {output:?}")]
    UnparseableVersion { path: PathBuf, output: String },
}

fn join_paths(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        return "(ninguna ubicación)".to_owned();
    }
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Ubicaciones conocidas de git para `os`, en orden de preferencia.
pub fn known_locations(os: TargetOs, opts: &LocatorOptions) -> Vec<PathBuf> {
    let mut out = Vec::new();
    match os {
        TargetOs::Windows => {
            let pf = opts
                .env_var("ProgramFiles")
                .unwrap_or_else(|| "C:\\Program Files".into());
            let mut roots: Vec<OsString> = vec![pf];
            if let Some(x86) = opts.env_var("ProgramFiles(x86)") {
                roots.push(x86);
            }
            for root in roots {
                out.push(win_join(&root, "Git\\cmd\\git.exe"));
                out.push(win_join(&root, "Git\\bin\\git.exe"));
            }
            if let Some(local) = opts.env_var("LOCALAPPDATA") {
                out.push(win_join(&local, "Programs\\Git\\cmd\\git.exe"));
            }
            if let Some(profile) = opts.env_var("USERPROFILE") {
                out.push(win_join(
                    &profile,
                    "scoop\\apps\\git\\current\\cmd\\git.exe",
                ));
                out.push(win_join(&profile, "scoop\\shims\\git.exe"));
            }
            if let Some(scoop) = opts.env_var("SCOOP") {
                out.push(win_join(&scoop, "apps\\git\\current\\cmd\\git.exe"));
            }
            let choco = opts
                .env_var("ChocolateyInstall")
                .or_else(|| {
                    opts.env_var("ProgramData")
                        .map(|pd| win_join(&pd, "chocolatey").into_os_string())
                })
                .unwrap_or_else(|| "C:\\ProgramData\\chocolatey".into());
            out.push(win_join(&choco, "bin\\git.exe"));
        }
        TargetOs::MacOs => {
            for dir in [
                "/opt/homebrew/bin",
                "/usr/local/bin",
                "/Library/Developer/CommandLineTools/usr/bin",
                "/usr/bin",
            ] {
                out.push(Path::new(dir).join("git"));
            }
        }
        TargetOs::Linux => {
            for dir in ["/usr/bin", "/usr/local/bin", "/opt/homebrew/bin"] {
                out.push(Path::new(dir).join("git"));
            }
        }
    }
    out
}

fn win_join(root: &OsString, tail: &str) -> PathBuf {
    let mut text = root
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_owned();
    text.push('\\');
    text.push_str(tail);
    PathBuf::from(text)
}

/// Candidatos en orden de prueba: explícitos, ubicaciones conocidas (Windows) o PATH,
/// y el resto. Sin duplicados. **No filtra** por existencia ni por ruta absoluta
/// (eso lo hace [`find_git`]).
pub fn candidate_paths(opts: &LocatorOptions) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = opts.extra_candidates.clone();
    let known = if opts.include_known_locations {
        known_locations(opts.os, opts)
    } else {
        vec![]
    };
    let mut path_entries = Vec::new();
    if opts.include_path {
        let var = opts
            .path_var
            .clone()
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default();
        for dir in std::env::split_paths(&var) {
            path_entries.push(dir.join(opts.os.exe_name()));
        }
    }
    if opts.os == TargetOs::Windows {
        out.extend(known);
        out.extend(path_entries);
    } else {
        out.extend(path_entries);
        out.extend(known);
    }
    let mut seen = Vec::new();
    out.retain(|p| {
        if seen.contains(p) {
            false
        } else {
            seen.push(p.clone());
            true
        }
    });
    out
}

/// Ejecuta `<git> --version` con plazo y sin heredar el directorio actual.
fn run_version(git: &Path, opts: &LocatorOptions) -> Option<String> {
    let work_dir = opts.work_dir.clone().unwrap_or_else(std::env::temp_dir);
    let mut cmd = Command::new(git);
    cmd.arg("--version")
        .current_dir(work_dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(cmd.output());
    });
    match rx.recv_timeout(opts.timeout) {
        Ok(Ok(output)) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        _ => None,
    }
}

/// Resuelve `git` a una ruta absoluta validada (`git --version >= opts.min_version`).
///
/// Prueba los candidatos en orden y devuelve el primero que funciona y cumple la versión.
/// Las rutas relativas y las entradas vacías del PATH (que significarían «directorio
/// actual») se descartan siempre.
pub fn find_git(opts: &LocatorOptions) -> Result<GitInstall, GitLocateError> {
    let candidates = candidate_paths(opts);
    let mut searched = Vec::new();
    let mut too_old: Option<GitLocateError> = None;
    let mut unparseable: Option<GitLocateError> = None;
    for candidate in candidates {
        if !candidate.is_absolute() || !candidate.is_file() {
            searched.push(candidate);
            continue;
        }
        searched.push(candidate.clone());
        let Some(raw) = run_version(&candidate, opts) else {
            continue;
        };
        let Some(version) = parse_git_version(&raw) else {
            unparseable.get_or_insert(GitLocateError::UnparseableVersion {
                path: candidate,
                output: raw.trim().to_owned(),
            });
            continue;
        };
        if version < opts.min_version {
            too_old.get_or_insert(GitLocateError::TooOld {
                path: candidate,
                found: version,
                required: opts.min_version,
            });
            continue;
        }
        return Ok(GitInstall {
            path: candidate,
            version,
            raw_version: raw.trim().to_owned(),
        });
    }
    Err(too_old
        .or(unparseable)
        .unwrap_or(GitLocateError::NotFound { searched }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn v(major: u32, minor: u32, patch: u32) -> GitVersion {
        GitVersion {
            major,
            minor,
            patch,
        }
    }

    #[test]
    fn parses_known_version_formats() {
        assert_eq!(
            parse_git_version("git version 2.53.0.windows.3\n"),
            Some(v(2, 53, 0))
        );
        assert_eq!(
            parse_git_version("git version 2.39.5 (Apple Git-154)"),
            Some(v(2, 39, 5))
        );
        assert_eq!(
            parse_git_version("git version 2.45.0-rc1"),
            Some(v(2, 45, 0))
        );
        assert_eq!(parse_git_version("git version 2.40"), Some(v(2, 40, 0)));
        assert_eq!(
            parse_git_version("git version 2.43.0.rc0.12.gabc"),
            Some(v(2, 43, 0))
        );
        assert_eq!(parse_git_version("git version 2.7.4"), Some(v(2, 7, 4)));
        for bad in [
            "",
            "git",
            "git version",
            "git version x.y",
            "git version 2",
            "version 2.40.0",
            "2.40.0",
        ] {
            assert_eq!(parse_git_version(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn version_ordering_and_minimum() {
        assert!(v(2, 53, 0) > v(2, 39, 0));
        assert!(v(2, 9, 0) < MIN_GIT_VERSION);
        assert!(v(2, 38, 9) < MIN_GIT_VERSION);
        assert!(v(2, 39, 0) >= MIN_GIT_VERSION);
        assert!(v(3, 0, 0) > MIN_GIT_VERSION);
        assert_eq!(v(2, 39, 5).to_string(), "2.39.5");
    }

    fn opts_for(os: TargetOs, env: &[(&str, &str)]) -> LocatorOptions {
        LocatorOptions {
            os,
            include_path: false,
            env: Some(
                env.iter()
                    .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
                    .collect(),
            ),
            ..LocatorOptions::default()
        }
    }

    #[test]
    fn windows_known_locations_come_from_environment() {
        let opts = opts_for(
            TargetOs::Windows,
            &[
                ("ProgramFiles", "C:\\Program Files"),
                ("LOCALAPPDATA", "C:\\Users\\Ana\\AppData\\Local"),
                ("USERPROFILE", "C:\\Users\\Ana"),
                ("ProgramData", "C:\\ProgramData"),
            ],
        );
        let list: Vec<String> = known_locations(TargetOs::Windows, &opts)
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        assert_eq!(list[0], "C:\\Program Files\\Git\\cmd\\git.exe");
        assert!(list
            .contains(&"C:\\Users\\Ana\\AppData\\Local\\Programs\\Git\\cmd\\git.exe".to_owned()));
        assert!(list.contains(&"C:\\Users\\Ana\\scoop\\shims\\git.exe".to_owned()));
        assert!(list.contains(&"C:\\ProgramData\\chocolatey\\bin\\git.exe".to_owned()));
    }

    #[test]
    fn macos_known_locations_put_apple_shim_last() {
        let list = known_locations(TargetOs::MacOs, &opts_for(TargetOs::MacOs, &[]));
        assert_eq!(list.first().unwrap(), Path::new("/opt/homebrew/bin/git"));
        assert_eq!(list.last().unwrap(), Path::new("/usr/bin/git"));
        assert!(list.contains(&PathBuf::from(
            "/Library/Developer/CommandLineTools/usr/bin/git"
        )));
    }

    #[test]
    fn candidate_order_dedup_and_path_split() {
        let mut opts = opts_for(TargetOs::Linux, &[]);
        opts.include_known_locations = false;
        opts.include_path = true;
        let sep = if cfg!(windows) { ";" } else { ":" };
        opts.path_var = Some(format!("/a{sep}/b{sep}/a").into());
        opts.extra_candidates = vec![PathBuf::from("/custom/git")];
        let list = candidate_paths(&opts);
        assert_eq!(
            list,
            [
                PathBuf::from("/custom/git"),
                PathBuf::from("/a/git"),
                PathBuf::from("/b/git")
            ]
        );
    }

    #[test]
    fn relative_and_empty_path_entries_are_never_executed() {
        let dir = tempfile::tempdir().unwrap();
        // Un «git» plantado en el directorio actual no debe ejecutarse.
        let planted = dir.path().join("git");
        std::fs::write(&planted, "#!/bin/sh\necho 'git version 9.9.9'\n").unwrap();
        let sep = if cfg!(windows) { ";" } else { ":" };
        let opts = LocatorOptions {
            include_known_locations: false,
            path_var: Some(format!("{sep}.{sep}relativo").into()),
            ..LocatorOptions::default()
        };
        let err = find_git(&opts).unwrap_err();
        assert!(matches!(err, GitLocateError::NotFound { .. }), "{err:?}");
    }

    #[test]
    fn not_found_error_is_actionable() {
        let opts = LocatorOptions {
            include_known_locations: false,
            include_path: false,
            extra_candidates: vec![PathBuf::from("/no/existe/git")],
            ..LocatorOptions::default()
        };
        let err = find_git(&opts).unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("Instala Git") && text.contains("/no/existe/git"),
            "{text}"
        );
    }

    #[cfg(unix)]
    fn fake_git(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    fn injected(candidates: Vec<PathBuf>) -> LocatorOptions {
        LocatorOptions {
            include_known_locations: false,
            include_path: false,
            extra_candidates: candidates,
            timeout: Duration::from_secs(10),
            ..LocatorOptions::default()
        }
    }

    #[cfg(unix)]
    #[test]
    fn skips_too_old_and_broken_candidates_then_accepts_valid_one() {
        let dir = tempfile::tempdir().unwrap();
        let old = fake_git(dir.path(), "old", "echo 'git version 2.30.2'");
        let broken = fake_git(dir.path(), "broken", "exit 1");
        let junk = fake_git(dir.path(), "junk", "echo 'hola'");
        let good = fake_git(
            dir.path(),
            "good",
            "echo 'git version 2.39.5 (Apple Git-154)'",
        );
        let found = find_git(&injected(vec![old, broken, junk, good.clone()])).unwrap();
        assert_eq!(found.path, good);
        assert_eq!(found.version, v(2, 39, 5));
        assert_eq!(found.raw_version, "git version 2.39.5 (Apple Git-154)");
    }

    #[cfg(unix)]
    #[test]
    fn too_old_git_reports_actionable_error() {
        let dir = tempfile::tempdir().unwrap();
        let old = fake_git(dir.path(), "old", "echo 'git version 2.38.9'");
        let err = find_git(&injected(vec![old.clone()])).unwrap_err();
        assert_eq!(
            err,
            GitLocateError::TooOld {
                path: old,
                found: v(2, 38, 9),
                required: v(2, 39, 0)
            }
        );
        let text = err.to_string();
        assert!(
            text.contains("2.38.9") && text.contains("2.39.0") && text.contains("Actualiza"),
            "{text}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn hanging_git_times_out_and_next_candidate_wins() {
        let dir = tempfile::tempdir().unwrap();
        let slow = fake_git(dir.path(), "slow", "sleep 5; echo 'git version 2.50.0'");
        let good = fake_git(dir.path(), "good", "echo 'git version 2.50.0'");
        let mut opts = injected(vec![slow, good.clone()]);
        opts.timeout = Duration::from_millis(300);
        assert_eq!(find_git(&opts).unwrap().path, good);
    }

    #[test]
    fn finds_real_git_on_path_as_absolute_path() {
        if !crate::testutil::git_available() {
            return;
        }
        let found = find_git(&LocatorOptions::default()).unwrap();
        assert!(found.path.is_absolute());
        assert!(found.version >= MIN_GIT_VERSION);
    }
}
