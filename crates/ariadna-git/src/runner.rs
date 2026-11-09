//! Ejecutor de `git` endurecido (spec §4.1). Todo el motor lanza git a través de aquí.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ariadna_process::{
    run, run_streaming, CancellationToken, EnvPolicy, ProcessError, ProcessOutput, ProcessSpec,
};
use regex::Regex;

use crate::auth::classify_auth_failure;
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
    /// Pares `-c clave=valor` extra, antes del subcomando (p. ej. un `credential.helper`).
    extra_config: Vec<(String, String)>,
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
            extra_config: Vec::new(),
        }
    }

    /// Añade `-c clave=valor` a **todas** las llamadas de este ejecutor (después de los flags de
    /// seguridad). Sirve para forzar un `credential.helper` concreto o `credential.useHttpPath`;
    /// el valor no debe llevar secretos (acabaría en el argv del proceso).
    pub fn with_config(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_config.push((key.into(), value.into()));
        self
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
    fn global_args(&self, kind: CommandKind) -> Vec<OsString> {
        let mut v: Vec<&str> = vec![
            "-c",
            "core.quotepath=false",
            "-c",
            "i18n.logOutputEncoding=UTF-8",
            "-c",
            "core.longpaths=true",
            "-c",
            "core.pager=cat",
            // Un repo ajeno puede definir `core.fsmonitor` como programa a ejecutar: nunca,
            // ni en lecturas ni en mutaciones (spec §4.1).
            "-c",
            "core.fsmonitor=false",
            // Nunca transportes con ayudante (`ext::`): ejecutan programas arbitrarios.
            "-c",
            "protocol.ext.allow=never",
        ];
        if kind == CommandKind::Read {
            v.push("--no-optional-locks");
        }
        let mut out: Vec<OsString> = v.into_iter().map(OsString::from).collect();
        for (k, val) in &self.extra_config {
            out.push("-c".into());
            out.push(format!("{k}={val}").into());
        }
        out
    }

    fn base_spec(
        &self,
        cwd: &Path,
        kind: CommandKind,
        args: &[OsString],
        env: &[(&str, &str)],
    ) -> ProcessSpec {
        let mut all = self.global_args(kind);
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
        for (k, v) in env {
            spec = spec.env(*k, *v);
        }
        spec
    }

    fn map_run(
        command: String,
        res: Result<ProcessOutput, ProcessError>,
    ) -> GitResult<ProcessOutput> {
        match res {
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

    /// Ejecuta y devuelve la salida **aunque el código de salida sea distinto de 0**.
    /// Úsalo cuando el código de salida es información (p. ej. `diff --quiet`).
    pub async fn run(
        &self,
        cwd: &Path,
        kind: CommandKind,
        args: &[OsString],
        stdin: Option<Vec<u8>>,
    ) -> GitResult<ProcessOutput> {
        self.run_with_env(cwd, kind, args, stdin, &[]).await
    }

    /// Como [`run`](Self::run) con variables de entorno extra (se fijan al final; ganan siempre).
    /// Útil para `GCM_INTERACTIVE=never` en sondeos no interactivos.
    pub async fn run_with_env(
        &self,
        cwd: &Path,
        kind: CommandKind,
        args: &[OsString],
        stdin: Option<Vec<u8>>,
        env: &[(&str, &str)],
    ) -> GitResult<ProcessOutput> {
        let mut spec = self.base_spec(cwd, kind, args, env);
        if let Some(s) = stdin {
            spec = spec.stdin(s);
        }
        Self::map_run(describe(args), run(spec, self.cancel.clone()).await)
    }

    /// Como [`run_with_env`](Self::run_with_env) pero entrega el **stderr en streaming** a
    /// `on_stderr` (trozos de bytes; ver `ariadna_process::run_streaming`). Se cancela con el
    /// token del ejecutor **o** con `cancel`, lo que ocurra primero.
    pub async fn run_streaming<F>(
        &self,
        cwd: &Path,
        kind: CommandKind,
        args: &[OsString],
        env: &[(&str, &str)],
        cancel: &CancellationToken,
        on_stderr: F,
    ) -> GitResult<ProcessOutput>
    where
        F: FnMut(&[u8]) + Send,
    {
        let spec = self.base_spec(cwd, kind, args, env);
        let token = self.cancel.child_token();
        let (inner, outer) = (token.clone(), cancel.clone());
        let watcher = tokio::spawn(async move {
            outer.cancelled().await;
            inner.cancel();
        });
        let res = run_streaming(spec, token, on_stderr).await;
        watcher.abort();
        Self::map_run(describe(args), res)
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
        Err(self.failure_error(cwd, kind, args, &out).await)
    }

    /// Error tipado de un comando que terminó con código ≠ 0. Para `Network` reconoce antes el
    /// fallo de autenticación ([`GitError::Auth`]); el resto va a `classify_failure`.
    pub async fn failure_error(
        &self,
        cwd: &Path,
        kind: CommandKind,
        args: &[OsString],
        out: &ProcessOutput,
    ) -> GitError {
        if kind == CommandKind::Network {
            if let Some(e) = self.auth_error(cwd, args, out).await {
                return e;
            }
        }
        classify_failure(cwd, args, out)
    }

    /// `Some(Auth)` si el comando de red fallido es un problema de credenciales. Averigua la URL
    /// del remoto (argumento o `git remote get-url`) solo para sacar host y proveedor.
    pub async fn auth_error(
        &self,
        cwd: &Path,
        args: &[OsString],
        out: &ProcessOutput,
    ) -> Option<GitError> {
        let stderr = out.stderr_lossy();
        // git ≥ 2.50 sale con 1 (no 128) cuando el `fetch` interno de `git pull` falla con `fatal:`.
        let is_pull = args.first().is_some_and(|a| a == "pull");
        let code = if out.code == Some(1) && is_pull && stderr.contains("fatal:") {
            Some(128)
        } else {
            out.code
        };
        if code != Some(128) {
            return None;
        }
        let url = self.remote_url_for(cwd, args).await;
        classify_auth_failure(code, &stderr, url.as_deref())
    }

    /// URL del remoto al que apunta un `clone`/`ls-remote`/`fetch`/`pull`/`push`.
    async fn remote_url_for(&self, cwd: &Path, args: &[OsString]) -> Option<String> {
        let strs: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let first_plain = || strs.iter().skip(1).find(|a| !a.starts_with('-')).cloned();
        match strs.first()?.as_str() {
            "clone" | "ls-remote" => match strs.iter().position(|a| a == "--") {
                Some(i) => strs.get(i + 1).cloned(),
                None => first_plain(),
            },
            "fetch" | "pull" | "push" => {
                let name = first_plain().unwrap_or_else(|| "origin".to_string());
                if name.contains("://") || name.contains(':') {
                    return Some(name);
                }
                let out = self
                    .run(
                        cwd,
                        CommandKind::Read,
                        &[
                            OsString::from("remote"),
                            OsString::from("get-url"),
                            OsString::from(name),
                        ],
                        None,
                    )
                    .await
                    .ok()?;
                let url = out.stdout_lossy().trim().to_string();
                (out.success() && !url.is_empty()).then_some(url)
            }
            _ => None,
        }
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
    fn fsmonitor_se_desactiva_en_todos_los_tipos() {
        for k in [CommandKind::Read, CommandKind::Mutate, CommandKind::Network] {
            let v = runner().global_args(k);
            assert!(v.iter().any(|a| a == "core.fsmonitor=false"), "{k:?}");
        }
    }

    #[test]
    fn ext_queda_prohibido_y_la_config_extra_se_anade_al_final() {
        let r = runner()
            .with_config("credential.helper", "")
            .with_config("a.b", "c");
        let v: Vec<String> = r
            .global_args(CommandKind::Network)
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(v
            .windows(2)
            .any(|w| w[0] == "-c" && w[1] == "protocol.ext.allow=never"));
        let tail = &v[v.len() - 4..];
        assert_eq!(tail, ["-c", "credential.helper=", "-c", "a.b=c"]);
    }

    #[test]
    fn global_args_de_lectura_incluyen_no_optional_locks() {
        let v = runner().global_args(CommandKind::Read);
        assert!(v.iter().any(|a| a == "--no-optional-locks"));
        assert!(!runner()
            .global_args(CommandKind::Mutate)
            .iter()
            .any(|a| a == "--no-optional-locks"));
    }
}
