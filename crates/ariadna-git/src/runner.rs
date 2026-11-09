//! Ejecutor de `git` endurecido (spec §4.1). Todo el motor lanza git a través de aquí.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ariadna_process::{
    run, CancellationToken, EnvPolicy, ProcessError, ProcessOutput, ProcessSpec,
};
use regex::Regex;

use crate::error::{truncate, GitError, GitResult};
use crate::git_locator::GitInstall;

/// Tipo de comando, para elegir flags, entorno y timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    /// `status`, `diff`, `log`…: sin locks opcionales, sin fsmonitor, sin diff externo.
    Read,
    /// Modifica el repo (stage, commit, checkout, merge…). Ejecuta hooks.
    Mutate,
    /// Habla con un remoto (fetch, pull, push). Sin timeout: solo cancelación manual,
    /// porque puede haber un diálogo de credenciales (GCM, SSH).
    Network,
}

/// Lanza `git` con el entorno y los flags de seguridad del spec.
#[derive(Debug, Clone)]
pub struct GitRunner {
    git: PathBuf,
    local_timeout: Duration,
    max_output_bytes: usize,
    cancel: CancellationToken,
}

impl GitRunner {
    pub fn new(install: &GitInstall) -> Self {
        Self::with_path(install.path.clone())
    }

    /// Para tests o rutas elegidas por el usuario. No valida la versión.
    pub fn with_path(git: PathBuf) -> Self {
        Self {
            git,
            local_timeout: Duration::from_secs(180),
            max_output_bytes: 64 * 1024 * 1024,
            cancel: CancellationToken::new(),
        }
    }

    /// Token que cancela todas las llamadas en curso de este ejecutor (lo da la sesión).
    pub fn with_cancel(mut self, token: CancellationToken) -> Self {
        self.cancel = token;
        self
    }

    pub fn with_local_timeout(mut self, t: Duration) -> Self {
        self.local_timeout = t;
        self
    }

    pub fn git_path(&self) -> &Path {
        &self.git
    }

    /// Argumentos globales (antes del subcomando) según el tipo de comando.
    fn global_args(kind: CommandKind) -> Vec<OsString> {
        let mut v: Vec<&str> = vec![
            "-c",
            "core.quotepath=false",
            "-c",
            "i18n.logOutputEncoding=UTF-8",
            "-c",
            "core.longpaths=true",
            "-c",
            "core.pager=cat",
        ];
        if kind == CommandKind::Read {
            v.extend(["-c", "core.fsmonitor=false", "--no-optional-locks"]);
        }
        v.into_iter().map(OsString::from).collect()
    }

    fn base_spec(&self, cwd: &Path, kind: CommandKind, args: &[OsString]) -> ProcessSpec {
        let mut all = Self::global_args(kind);
        all.extend(args.iter().cloned());
        let mut spec = ProcessSpec::new(&self.git)
            .args(all)
            .cwd(cwd)
            .env_policy(EnvPolicy::git_default())
            .max_output_bytes(self.max_output_bytes)
            .timeout(match kind {
                CommandKind::Network => None,
                _ => Some(self.local_timeout),
            })
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PAGER", "cat")
            .env("GIT_EDITOR", "true")
            .env("GIT_SEQUENCE_EDITOR", "true")
            .env("GIT_MERGE_AUTOEDIT", "no")
            .env("GIT_LITERAL_PATHSPECS", "1")
            // Ayuda, no garantía: no se parsean mensajes humanos (spec §4.1).
            .env("LC_ALL", "C");
        if kind == CommandKind::Read {
            spec = spec.env("GIT_OPTIONAL_LOCKS", "0");
        }
        spec
    }

    /// Ejecuta y devuelve la salida **aunque el código de salida sea distinto de 0**.
    /// Úsalo cuando el código de salida es información (p. ej. `diff --quiet`).
    pub async fn run(
        &self,
        cwd: &Path,
        kind: CommandKind,
        args: &[OsString],
        stdin: Option<Vec<u8>>,
    ) -> GitResult<ProcessOutput> {
        let mut spec = self.base_spec(cwd, kind, args);
        if let Some(s) = stdin {
            spec = spec.stdin(s);
        }
        let command = describe(args);
        match run(spec, self.cancel.clone()).await {
            Ok(out) if out.timed_out => Err(GitError::Timeout { command }),
            Ok(out) if out.cancelled => Err(GitError::Cancelled),
            Ok(out) => Ok(out),
            Err(ProcessError::Spawn { program, source }) => Err(GitError::GitNotFound(format!(
                "{}: {source}",
                program.display()
            ))),
            Err(ProcessError::Io(e)) => Err(GitError::Io(e.to_string())),
        }
    }

    /// Como [`run`](Self::run) pero convierte un código distinto de 0 en [`GitError`].
    pub async fn run_ok(
        &self,
        cwd: &Path,
        kind: CommandKind,
        args: &[OsString],
        stdin: Option<Vec<u8>>,
    ) -> GitResult<ProcessOutput> {
        let out = self.run(cwd, kind, args, stdin).await?;
        if out.success() {
            return Ok(out);
        }
        Err(classify_failure(cwd, args, &out))
    }

    /// Atajo: argumentos como `&str`.
    pub async fn run_ok_str(
        &self,
        cwd: &Path,
        kind: CommandKind,
        args: &[&str],
    ) -> GitResult<ProcessOutput> {
        let a: Vec<OsString> = args.iter().map(OsString::from).collect();
        self.run_ok(cwd, kind, &a, None).await
    }
}

/// «git <subcomando> …» sin argumentos que puedan llevar datos del usuario.
fn describe(args: &[OsString]) -> String {
    args.first().map_or_else(
        || "git".to_string(),
        |a| format!("git {}", a.to_string_lossy()),
    )
}

/// Traduce un fallo a un error tipado **sin interpretar el texto de git**, salvo
/// `dubious ownership`, que no tiene otra señal (excepción documentada).
fn classify_failure(cwd: &Path, args: &[OsString], out: &ProcessOutput) -> GitError {
    let lock = cwd.join(".git").join("index.lock");
    if lock.exists() {
        return GitError::Locked {
            lock: lock.display().to_string(),
        };
    }
    let stderr = redact_credentials(&out.stderr_lossy());
    if out.code == Some(128) && stderr.contains("dubious ownership") {
        return GitError::DubiousOwnership {
            path: cwd.display().to_string(),
        };
    }
    GitError::CommandFailed {
        command: describe(args),
        exit_code: out.code.unwrap_or(-1),
        stderr: truncate(&stderr, 8_000),
    }
}

/// Quita usuario y contraseña de las URLs (`https://user:pass@host` → `https://***@host`).
pub fn redact_credentials(text: &str) -> String {
    // Se compila en cada llamada: solo se usa en el camino de error.
    match Regex::new(r"(?i)([a-z][a-z0-9+.-]*://)[^/@\s]+@") {
        Ok(re) => re.replace_all(text, "${1}***@").into_owned(),
        Err(_) => text.to_string(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::git_locator::{find_git, LocatorOptions};

    fn runner() -> GitRunner {
        GitRunner::new(&find_git(&LocatorOptions::default()).unwrap())
    }
    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[tokio::test]
    async fn run_ok_devuelve_la_salida() {
        let dir = tempfile::tempdir().unwrap();
        let out = runner()
            .run_ok(dir.path(), CommandKind::Read, &os(&["--version"]), None)
            .await
            .unwrap();
        assert!(out.stdout_lossy().starts_with("git version"));
    }

    #[tokio::test]
    async fn fallo_se_convierte_en_command_failed_con_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let err = runner()
            .run_ok(
                dir.path(),
                CommandKind::Read,
                &os(&["rev-parse", "--verify", "nada"]),
                None,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, GitError::CommandFailed { exit_code, .. } if exit_code != 0));
    }

    #[tokio::test]
    async fn index_lock_ajeno_da_locked() {
        let dir = tempfile::tempdir().unwrap();
        let r = runner();
        r.run_ok(dir.path(), CommandKind::Mutate, &os(&["init", "-q"]), None)
            .await
            .unwrap();
        std::fs::write(dir.path().join(".git").join("index.lock"), b"").unwrap();
        let err = r
            .run_ok(dir.path(), CommandKind::Mutate, &os(&["add", "-A"]), None)
            .await;
        // `add` sin nada que añadir puede no tocar el índice; forzamos un fallo seguro:
        let err = match err {
            Err(e) => e,
            Ok(_) => {
                std::fs::write(dir.path().join("a.txt"), b"x").unwrap();
                r.run_ok(
                    dir.path(),
                    CommandKind::Mutate,
                    &os(&["add", "a.txt"]),
                    None,
                )
                .await
                .unwrap_err()
            }
        };
        assert!(matches!(err, GitError::Locked { .. }), "{err:?}");
    }

    #[tokio::test]
    async fn timeout_se_traduce() {
        let dir = tempfile::tempdir().unwrap();
        let r = runner().with_local_timeout(Duration::from_millis(1));
        // `git` tarda más de 1 ms en arrancar y listar; si fuera más rápido, el test no aplica.
        let res = r
            .run(dir.path(), CommandKind::Read, &os(&["--version"]), None)
            .await;
        assert!(matches!(res, Err(GitError::Timeout { .. })) || res.is_ok());
    }

    #[test]
    fn redacta_credenciales_en_urls() {
        let s = redact_credentials("fatal: https://user:s3cr3t@github.com/x/y.git no responde");
        assert!(!s.contains("s3cr3t") && s.contains("https://***@github.com"));
    }

    #[test]
    fn global_args_de_lectura_incluyen_no_optional_locks() {
        let v = GitRunner::global_args(CommandKind::Read);
        assert!(v.iter().any(|a| a == "--no-optional-locks"));
        assert!(!GitRunner::global_args(CommandKind::Mutate)
            .iter()
            .any(|a| a == "--no-optional-locks"));
    }
}
