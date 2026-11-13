use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use command_group::AsyncCommandGroup;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroize;

use crate::env::{build_env, EnvPolicy};

/// Qué lanzar y con qué límites.
///
/// `Debug` no muestra el contenido de `stdin` (puede llevar secretos): solo su tamaño.
#[derive(Clone)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub env_policy: EnvPolicy,
    /// Variables que se fijan después de aplicar la política.
    pub extra_env: Vec<(OsString, OsString)>,
    /// Bytes que se escriben por stdin (que luego se cierra). `None` = stdin cerrado.
    pub stdin: Option<Vec<u8>>,
    /// `None` = sin timeout.
    pub timeout: Option<Duration>,
    /// Tope de bytes que se conservan de stdout y de stderr, por separado.
    pub max_output_bytes: usize,
}

impl std::fmt::Debug for ProcessSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessSpec")
            .field("program", &self.program)
            .field("args", &self.args)
            .field("cwd", &self.cwd)
            .field("env_policy", &self.env_policy)
            .field(
                "extra_env",
                &self.extra_env.iter().map(|(k, _)| k).collect::<Vec<_>>(),
            )
            .field(
                "stdin",
                &self.stdin.as_ref().map(|s| format!("<{} bytes>", s.len())),
            )
            .field("timeout", &self.timeout)
            .field("max_output_bytes", &self.max_output_bytes)
            .finish()
    }
}

impl ProcessSpec {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env_policy: EnvPolicy::git_default(),
            extra_env: Vec::new(),
            stdin: None,
            timeout: Some(Duration::from_secs(60)),
            max_output_bytes: 16 * 1024 * 1024,
        }
    }
    pub fn arg(mut self, a: impl Into<OsString>) -> Self {
        self.args.push(a.into());
        self
    }
    pub fn args<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(it.into_iter().map(Into::into));
        self
    }
    pub fn cwd(mut self, p: impl Into<PathBuf>) -> Self {
        self.cwd = Some(p.into());
        self
    }
    pub fn env_policy(mut self, p: EnvPolicy) -> Self {
        self.env_policy = p;
        self
    }
    pub fn env(mut self, k: impl Into<OsString>, v: impl Into<OsString>) -> Self {
        self.extra_env.push((k.into(), v.into()));
        self
    }
    pub fn stdin(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(bytes.into());
        self
    }
    pub fn timeout(mut self, t: Option<Duration>) -> Self {
        self.timeout = t;
        self
    }
    pub fn max_output_bytes(mut self, n: usize) -> Self {
        self.max_output_bytes = n;
        self
    }
}

/// Resultado de un proceso terminado (o interrumpido).
#[derive(Debug, Clone, Default)]
pub struct ProcessOutput {
    /// `None` si lo mató una señal o lo terminamos nosotros.
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
    pub cancelled: bool,
    /// `true` si stdout o stderr superaron el tope y se descartó el resto.
    pub truncated: bool,
}

impl ProcessOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0) && !self.timed_out && !self.cancelled
    }
    pub fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
    pub fn stderr_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("no se pudo lanzar {program:?}: {source}")]
    Spawn {
        program: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("error de E/S con el proceso: {0}")]
    Io(#[from] std::io::Error),
}

/// Lee hasta el final conservando como máximo `cap` bytes. Devuelve (datos, truncado).
async fn drain<R: AsyncRead + Unpin>(mut r: R, cap: usize) -> (Vec<u8>, bool) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    let mut truncated = false;
    loop {
        match r.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let room = cap.saturating_sub(buf.len());
                if room >= n {
                    buf.extend_from_slice(&chunk[..n]);
                } else {
                    buf.extend_from_slice(&chunk[..room]);
                    truncated = true;
                }
            }
        }
    }
    (buf, truncated)
}

/// Receptor opcional de trozos de salida a medida que llegan.
type ChunkSink<'a> = Option<&'a mut (dyn FnMut(&[u8]) + Send)>;

/// Como [`drain`], pero entrega cada trozo leído (aunque supere el tope) a `on_chunk`.
async fn drain_streaming<R: AsyncRead + Unpin>(
    mut r: R,
    cap: usize,
    on_chunk: ChunkSink<'_>,
) -> (Vec<u8>, bool) {
    let mut on_chunk = on_chunk;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    let mut truncated = false;
    loop {
        match r.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if let Some(cb) = on_chunk.as_deref_mut() {
                    cb(&chunk[..n]);
                }
                let room = cap.saturating_sub(buf.len());
                if room >= n {
                    buf.extend_from_slice(&chunk[..n]);
                } else {
                    buf.extend_from_slice(&chunk[..room]);
                    truncated = true;
                }
            }
        }
    }
    (buf, truncated)
}

/// Lanza el proceso y espera a que termine, caduque o se cancele.
///
/// Al caducar o cancelar se termina el árbol completo y se devuelve lo leído hasta entonces.
pub async fn run(
    spec: ProcessSpec,
    cancel: CancellationToken,
) -> Result<ProcessOutput, ProcessError> {
    run_impl(spec, cancel, None).await
}

/// Como [`run`], pero además entrega el **stderr en streaming**: `on_stderr` se llama con cada
/// trozo de bytes según llega (sin cortar por líneas; un trozo puede acabar a mitad de una
/// línea o de un carácter UTF-8, y los avances de progreso de git se separan con `\r`).
///
/// El stderr completo también se devuelve en [`ProcessOutput::stderr`] (con el mismo tope).
/// El callback no necesita ser `'static`: se ejecuta en la propia tarea que espera al proceso.
pub async fn run_streaming<F>(
    spec: ProcessSpec,
    cancel: CancellationToken,
    mut on_stderr: F,
) -> Result<ProcessOutput, ProcessError>
where
    F: FnMut(&[u8]) + Send,
{
    run_impl(spec, cancel, Some(&mut on_stderr)).await
}

async fn run_impl(
    spec: ProcessSpec,
    cancel: CancellationToken,
    on_stderr: ChunkSink<'_>,
) -> Result<ProcessOutput, ProcessError> {
    let env = build_env(std::env::vars_os(), &spec.env_policy, &spec.extra_env);

    let mut cmd = tokio::process::Command::new(&spec.program);
    cmd.args(&spec.args)
        .env_clear()
        .envs(env)
        .stdin(if spec.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }

    let mut builder = cmd.group();
    // Si Ariadna muere de golpe, el árbol de procesos muere con ella (Job Object con
    // KILL_ON_JOB_CLOSE en Windows).
    builder.kill_on_drop(true);
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: sin ventana de consola parpadeando al lanzar git o un CLI.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        builder.creation_flags(CREATE_NO_WINDOW);
    }
    let mut group = builder.spawn().map_err(|source| ProcessError::Spawn {
        program: spec.program.clone(),
        source,
    })?;

    let child = group.inner();
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let stdin_task = tokio::spawn(async move {
        if let (Some(mut s), Some(mut data)) = (stdin, spec.stdin) {
            // Un BrokenPipe es normal si el proceso no lee todo su stdin.
            let _ = s.write_all(&data).await;
            let _ = s.shutdown().await;
            // El stdin puede llevar un secreto (credential helper): se borra al terminar.
            data.zeroize();
        }
    });
    let cap = spec.max_output_bytes;
    let out_task = tokio::spawn(async move {
        match stdout {
            Some(r) => drain(r, cap).await,
            None => (Vec::new(), false),
        }
    });
    // stderr se drena sin `spawn` para que el callback pueda tomar prestado (no `'static`).
    let err_fut = async move {
        match stderr {
            Some(r) => drain_streaming(r, cap, on_stderr).await,
            None => (Vec::new(), false),
        }
    };

    let sleep = async {
        match spec.timeout {
            Some(t) => tokio::time::sleep(t).await,
            None => std::future::pending::<()>().await,
        }
    };

    let main_fut = async {
        let mut result = ProcessOutput::default();
        tokio::select! {
            status = group.wait() => {
                result.code = status?.code();
            }
            _ = sleep => {
                result.timed_out = true;
                let _ = group.kill().await;
                let _ = group.wait().await;
            }
            _ = cancel.cancelled() => {
                result.cancelled = true;
                let _ = group.kill().await;
                let _ = group.wait().await;
            }
        }
        Ok::<ProcessOutput, ProcessError>(result)
    };
    let (main_res, (stderr, t2)) = tokio::join!(main_fut, err_fut);
    let mut result = main_res?;

    let _ = stdin_task.await;
    let (stdout, t1) = out_task.await.unwrap_or_default();
    result.stdout = stdout;
    result.stderr = stderr;
    result.truncated = t1 || t2;
    Ok(result)
}
