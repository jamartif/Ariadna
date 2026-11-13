//! Clonar un repositorio desde una URL con progreso y cancelación (spec §4.3, «Clonar»).
//!
//! Reglas de seguridad:
//! * URL validada (`https`, `http`, `ssh`, `git@host:ruta`, `file://` o ruta local absoluta);
//!   se rechazan `ext::` y cualquier transporte con ayudante, lo que empieza por `-`, los
//!   caracteres de control y las contraseñas incrustadas. Siempre se pasa `--` antes de la URL
//!   y el ejecutor lleva `-c protocol.ext.allow=never` en todas las llamadas.
//! * El destino no existe o está vacío, es absoluto y no contiene `..` ni `.git`.
//! * Operación de red (`CommandKind::Network`): sin timeout, cancelable, con progreso.
//! * Si falla o se cancela se borra **solo** lo que Ariadna creó: la carpeta destino (y los
//!   padres que no existían) o, si el destino ya existía vacío, su contenido (la carpeta se
//!   conserva).
//! * Sin submódulos (`--no-recurse-submodules`).

use std::ffi::OsString;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use ariadna_process::CancellationToken;
use async_trait::async_trait;
use regex::Regex;
use serde::Serialize;

use crate::askpass::{scrub_error, AskpassScript, CloneAuth, SECRET_VAR, USER_VAR};
use crate::engine::GitCliEngine;
use crate::error::{GitError, GitResult};
use crate::models::Repository;
use crate::path_safety::validate_ref;
use crate::remote_url::{parse_remote_url, UrlKind};
use crate::runner::{redact_credentials, CommandKind};

/// Qué clonar y dónde.
#[derive(Clone, PartialEq, Eq)]
pub struct CloneRequest {
    /// URL del remoto o ruta local absoluta.
    pub url: String,
    /// Carpeta destino (absoluta): no existe o está vacía.
    pub destination: PathBuf,
    /// Rama a extraer (`--branch`); `None` = la rama por defecto del remoto.
    pub branch: Option<String>,
}

/// `Debug` redacta usuario y contraseña de la URL.
impl fmt::Debug for CloneRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloneRequest")
            .field("url", &redact_credentials(&self.url))
            .field("destination", &self.destination)
            .field("branch", &self.branch)
            .finish()
    }
}

/// Fase del clonado, en el orden en que git las emite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClonePhase {
    /// Preparando el clonado (antes de la primera línea de git).
    Starting,
    /// `Enumerating objects` / `Counting objects` (en el servidor).
    Counting,
    /// `Compressing objects` (en el servidor).
    Compressing,
    /// `Receiving objects`.
    Receiving,
    /// `Resolving deltas`.
    Resolving,
    /// `Updating files` / `Checking out files`.
    CheckingOut,
}

impl ClonePhase {
    /// Nombre estable (el mismo que se serializa).
    pub fn as_str(self) -> &'static str {
        match self {
            ClonePhase::Starting => "starting",
            ClonePhase::Counting => "counting",
            ClonePhase::Compressing => "compressing",
            ClonePhase::Receiving => "receiving",
            ClonePhase::Resolving => "resolving",
            ClonePhase::CheckingOut => "checkingOut",
        }
    }
}

/// Avance del clonado (evento `clone-progress`). `percent` es `None` si git no da porcentaje.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneProgress {
    pub phase: ClonePhase,
    pub percent: Option<u8>,
}

fn progress_regex() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?:remote:\s*)?(Enumerating objects|Counting objects|Compressing objects|Receiving objects|Resolving deltas|Updating files|Checking out files)\s*:\s*(?:(\d{1,3})%)?",
        )
        .ok()
    })
    .as_ref()
}

/// Interpreta **una** línea de progreso del `stderr` de `git clone --progress`
/// (`Receiving objects:  45% (9/20), 1.2 MiB | 3 MiB/s`, `remote: Counting objects: 100% …`,
/// `Resolving deltas: …`). Tolerante: devuelve `None` si no la entiende.
pub fn parse_progress_line(line: &str) -> Option<CloneProgress> {
    let line = line.trim();
    let caps = progress_regex()?.captures(line)?;
    let phase = match caps.get(1)?.as_str() {
        "Enumerating objects" | "Counting objects" => ClonePhase::Counting,
        "Compressing objects" => ClonePhase::Compressing,
        "Receiving objects" => ClonePhase::Receiving,
        "Resolving deltas" => ClonePhase::Resolving,
        _ => ClonePhase::CheckingOut,
    };
    let percent = caps
        .get(2)
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .map(|p| u8::try_from(p.min(100)).unwrap_or(100));
    Some(CloneProgress { phase, percent })
}

/// Parte el `stderr` en streaming en «líneas»: git separa las actualizaciones de progreso con
/// `\r`, y los mensajes finales con `\n`.
#[derive(Default)]
struct LineSplitter {
    buf: Vec<u8>,
}

impl LineSplitter {
    const MAX_LINE: usize = 4096;

    fn push(&mut self, chunk: &[u8], mut emit: impl FnMut(&str)) {
        for &b in chunk {
            if b == b'\r' || b == b'\n' {
                if !self.buf.is_empty() {
                    emit(&String::from_utf8_lossy(&self.buf));
                    self.buf.clear();
                }
            } else if self.buf.len() < Self::MAX_LINE {
                self.buf.push(b);
            }
        }
    }
}

/// Clonado desde URL. Lo implementa [`GitCliEngine`].
#[async_trait]
pub trait CloneService {
    /// Clona `req.url` en `req.destination` y devuelve el repositorio abierto.
    ///
    /// `progress` recibe los avances en orden (sin repetir el mismo valor seguido). Con `cancel`
    /// se mata el árbol de procesos de git y se limpia el disco: [`GitError::Cancelled`]. Un fallo
    /// de credenciales es [`GitError::Auth`]. Nunca deja basura creada por esta llamada.
    async fn clone_repository<P>(
        &self,
        req: CloneRequest,
        progress: P,
        cancel: CancellationToken,
    ) -> GitResult<Repository>
    where
        P: Fn(CloneProgress) + Send;

    /// Como [`clone_repository`](Self::clone_repository) con un token **de un solo uso**
    /// (`GIT_ASKPASS` temporal; ver [`crate::askpass`]). El token no se guarda en ningún sitio y
    /// ningún error lo contiene.
    async fn clone_repository_with_auth<P>(
        &self,
        req: CloneRequest,
        auth: CloneAuth,
        progress: P,
        cancel: CancellationToken,
    ) -> GitResult<Repository>
    where
        P: Fn(CloneProgress) + Send;
}

/// Qué crea (y por tanto puede borrar) esta llamada.
#[derive(Debug)]
struct DestPlan {
    dest: PathBuf,
    /// Carpeta más alta que no existía (puede ser el propio destino). `None` si el destino ya
    /// existía vacío.
    created_root: Option<PathBuf>,
}

fn io_err(msg: impl Into<String>) -> GitError {
    GitError::Io(msg.into())
}

fn plan_destination(dest: &Path) -> GitResult<DestPlan> {
    let shown = dest.display().to_string();
    if dest.as_os_str().is_empty() {
        return Err(io_err("Indica la carpeta de destino."));
    }
    if shown.chars().any(char::is_control) {
        return Err(io_err(
            "La carpeta de destino contiene caracteres de control.",
        ));
    }
    if !dest.is_absolute() {
        return Err(io_err(format!(
            "La carpeta de destino debe ser una ruta absoluta: {shown}."
        )));
    }
    for c in dest.components() {
        match c {
            Component::ParentDir | Component::CurDir => {
                return Err(io_err(format!(
                    "La carpeta de destino no puede contener «.» ni «..»: {shown}."
                )))
            }
            Component::Normal(n) if n.to_string_lossy().eq_ignore_ascii_case(".git") => {
                return Err(io_err(format!(
                    "La carpeta de destino no puede estar dentro de «.git»: {shown}."
                )))
            }
            _ => {}
        }
    }
    if dest.exists() {
        if !dest.is_dir() {
            return Err(io_err(format!(
                "El destino existe y no es una carpeta: {shown}."
            )));
        }
        let mut entries = std::fs::read_dir(dest)
            .map_err(|e| io_err(format!("No puedo leer la carpeta de destino {shown}: {e}")))?;
        if entries.next().is_some() {
            return Err(io_err(format!(
                "La carpeta de destino no está vacía: {shown}. Elige una vacía o una que no exista."
            )));
        }
        return Ok(DestPlan {
            dest: dest.to_path_buf(),
            created_root: None,
        });
    }
    // Carpeta más alta que no existe; su padre sí existe y debe ser una carpeta.
    let mut root = dest;
    for anc in dest.ancestors().skip(1) {
        if anc.exists() {
            if !anc.is_dir() {
                return Err(io_err(format!(
                    "{} existe y no es una carpeta.",
                    anc.display()
                )));
            }
            break;
        }
        root = anc;
    }
    if root.parent().is_none() {
        return Err(io_err(format!("Destino no válido: {shown}.")));
    }
    Ok(DestPlan {
        dest: dest.to_path_buf(),
        created_root: Some(root.to_path_buf()),
    })
}

/// Borra lo que creó esta llamada (reintenta: en Windows el árbol puede tardar en soltarse).
async fn cleanup(plan: &DestPlan) {
    for attempt in 0..6 {
        let res = match &plan.created_root {
            Some(root) if plan.dest.starts_with(root) => match std::fs::remove_dir_all(root) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            },
            Some(_) => return, // nunca borrar algo que no contiene al destino
            None => empty_dir(&plan.dest),
        };
        if res.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100 * (attempt + 1))).await;
    }
}

/// Vacía una carpeta sin borrarla.
fn empty_dir(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            // Los ficheros de solo lectura de `.git/objects` bloquean el borrado en Windows.
            let mut perm = meta.permissions();
            if perm.readonly() {
                #[allow(clippy::permissions_set_readonly_false)]
                perm.set_readonly(false);
                let _ = std::fs::set_permissions(&path, perm);
            }
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

#[async_trait]
impl CloneService for GitCliEngine {
    async fn clone_repository<P>(
        &self,
        req: CloneRequest,
        progress: P,
        cancel: CancellationToken,
    ) -> GitResult<Repository>
    where
        P: Fn(CloneProgress) + Send,
    {
        self.clone_inner(req, None, progress, cancel).await
    }

    async fn clone_repository_with_auth<P>(
        &self,
        req: CloneRequest,
        auth: CloneAuth,
        progress: P,
        cancel: CancellationToken,
    ) -> GitResult<Repository>
    where
        P: Fn(CloneProgress) + Send,
    {
        let secret = auth.secret().to_owned();
        let secret = zeroize::Zeroizing::new(secret);
        self.clone_inner(req, Some(&auth), progress, cancel)
            .await
            .map_err(|e| scrub_error(e, &secret))
    }
}

impl GitCliEngine {
    async fn clone_inner<P>(
        &self,
        req: CloneRequest,
        auth: Option<&CloneAuth>,
        progress: P,
        cancel: CancellationToken,
    ) -> GitResult<Repository>
    where
        P: Fn(CloneProgress) + Send,
    {
        if cancel.is_cancelled() {
            return Err(GitError::Cancelled);
        }
        let parsed = parse_remote_url(&req.url, true).map_err(|why| why.into_error(&req.url))?;
        let url = req.url.trim().to_string();
        if parsed.kind == UrlKind::Local
            && !url.starts_with("file:")
            && !Path::new(&url).is_absolute()
        {
            return Err(GitError::InvalidReference(format!(
                "{} (la ruta local debe ser absoluta)",
                redact_credentials(&url)
            )));
        }
        if let Some(branch) = req.branch.as_deref() {
            validate_ref(branch).map_err(|_| GitError::InvalidReference(branch.to_owned()))?;
        }
        let plan = plan_destination(&req.destination)?;

        let mut args: Vec<OsString> = ["clone", "--progress", "--no-recurse-submodules"]
            .iter()
            .map(OsString::from)
            .collect();
        if let Some(branch) = &req.branch {
            args.push("--branch".into());
            args.push(branch.into());
        }
        args.push("--".into());
        args.push(url.into());
        args.push(dunce::simplified(&plan.dest).as_os_str().to_owned());

        progress(CloneProgress {
            phase: ClonePhase::Starting,
            percent: None,
        });

        let cwd = std::env::temp_dir();
        let mut splitter = LineSplitter::default();
        let mut last: Option<CloneProgress> = None;

        // Token de un solo uso: script `askpass` temporal (sin secretos) + variables solo para
        // el proceso hijo. Con `credential.helper=` vacío nadie lo guarda tras el éxito. La
        // carpeta temporal se borra al salir de esta función (`_askpass`).
        let askpass = match auth {
            Some(_) => Some(AskpassScript::create()?),
            None => None,
        };
        let askpass_value = askpass.as_ref().map(AskpassScript::git_askpass_value);
        let mut env: Vec<(&str, &str)> = Vec::new();
        let runner = match (auth, askpass_value.as_deref()) {
            (Some(a), Some(script)) => {
                env.push(("GIT_ASKPASS", script));
                env.push(("GCM_INTERACTIVE", "never"));
                env.push((USER_VAR, a.username()));
                env.push((SECRET_VAR, a.secret()));
                self.runner.clone().with_config("credential.helper", "")
            }
            _ => self.runner.clone(),
        };
        let res = runner
            .run_streaming(
                &cwd,
                CommandKind::Network,
                &args,
                &env,
                &cancel,
                move |chunk| {
                    splitter.push(chunk, |line| {
                        if let Some(p) = parse_progress_line(line) {
                            if last != Some(p) {
                                last = Some(p);
                                progress(p);
                            }
                        }
                    });
                },
            )
            .await;

        let out = match res {
            Ok(out) => out,
            Err(e) => {
                cleanup(&plan).await;
                return Err(e);
            }
        };
        if !out.success() {
            cleanup(&plan).await;
            return Err(runner
                .failure_error(&cwd, CommandKind::Network, &args, &out)
                .await);
        }
        match self.open_repository(&plan.dest).await {
            Ok(repo) => Ok(repo),
            Err(e) => {
                cleanup(&plan).await;
                Err(e)
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn p(phase: ClonePhase, percent: Option<u8>) -> Option<CloneProgress> {
        Some(CloneProgress { phase, percent })
    }

    #[test]
    fn interpreta_las_lineas_de_git() {
        use ClonePhase::*;
        let cases: &[(&str, Option<CloneProgress>)] = &[
            ("remote: Enumerating objects: 28, done.", p(Counting, None)),
            (
                "remote: Counting objects:  50% (14/28)",
                p(Counting, Some(50)),
            ),
            (
                "remote: Counting objects: 100% (28/28), done.\u{1b}[K",
                p(Counting, Some(100)),
            ),
            (
                "remote: Compressing objects:  33% (6/18)",
                p(Compressing, Some(33)),
            ),
            (
                "Receiving objects:  45% (13/28), 1.20 MiB | 2.50 MiB/s",
                p(Receiving, Some(45)),
            ),
            (
                "Receiving objects: 100% (28/28), 5.10 KiB | 5.10 MiB/s, done.",
                p(Receiving, Some(100)),
            ),
            ("Resolving deltas:   0% (0/7)", p(Resolving, Some(0))),
            (
                "Resolving deltas: 100% (7/7), done.",
                p(Resolving, Some(100)),
            ),
            (
                "Updating files: 100% (12/12), done.",
                p(CheckingOut, Some(100)),
            ),
            ("Checking out files:  10% (1/10)", p(CheckingOut, Some(10))),
            ("Receiving objects: 250% raro", p(Receiving, Some(100))),
            ("Cloning into 'repo'...", None),
            (
                "warning: You appear to have cloned an empty repository.",
                None,
            ),
            ("fatal: repository 'x' not found", None),
            ("", None),
            ("Receiving objects: sin porcentaje", p(Receiving, None)),
            ("Descargando objetos: 10%", None),
        ];
        for (line, want) in cases {
            assert_eq!(parse_progress_line(line), *want, "{line:?}");
        }
    }

    #[test]
    fn el_divisor_separa_por_cr_y_lf_y_tolera_trozos_partidos() {
        let mut s = LineSplitter::default();
        let mut lines: Vec<String> = Vec::new();
        for chunk in [
            &b"Receiving objects:  1"[..],
            b"0% (1/10)\rReceiving objects:  20% (2/10)\r",
            b"\nfatal: x\n",
        ] {
            s.push(chunk, |l| lines.push(l.to_string()));
        }
        assert_eq!(
            lines,
            [
                "Receiving objects:  10% (1/10)",
                "Receiving objects:  20% (2/10)",
                "fatal: x"
            ]
        );
        // Una línea gigante no crece sin límite.
        let mut s = LineSplitter::default();
        s.push(&vec![b'a'; 100_000], |_| {});
        assert!(s.buf.len() <= LineSplitter::MAX_LINE);
    }

    #[test]
    fn fases_en_orden_y_nombres_estables() {
        use ClonePhase::*;
        let order = [
            Starting,
            Counting,
            Compressing,
            Receiving,
            Resolving,
            CheckingOut,
        ];
        assert!(order.windows(2).all(|w| w[0] < w[1]));
        let json = serde_json::to_string(&CloneProgress {
            phase: CheckingOut,
            percent: Some(3),
        })
        .unwrap();
        assert_eq!(json, r#"{"phase":"checkingOut","percent":3}"#);
        for ph in order {
            assert_eq!(
                serde_json::to_string(&ph).unwrap(),
                format!("\"{}\"", ph.as_str())
            );
        }
    }

    #[test]
    fn el_debug_de_la_peticion_oculta_credenciales() {
        let pw = ["cla", "ve-x"].concat();
        let r = CloneRequest {
            url: format!("https://usuario:{pw}@host.example/x.git"),
            destination: PathBuf::from("/tmp/x"),
            branch: None,
        };
        assert!(!format!("{r:?}").contains(&pw));
    }

    #[test]
    fn destino_nuevo_vacio_ocupado_y_raro() {
        let t = tempfile::tempdir().unwrap();
        let base = dunce::canonicalize(t.path()).unwrap();
        // No existe, con padres que tampoco existen: se borrará desde el primero que falte.
        let nuevo = base.join("a").join("b").join("repo");
        let plan = plan_destination(&nuevo).unwrap();
        assert_eq!(plan.created_root, Some(base.join("a")));
        // Existe y está vacío: no se borra la carpeta.
        let vacio = base.join("vacio");
        std::fs::create_dir(&vacio).unwrap();
        assert_eq!(plan_destination(&vacio).unwrap().created_root, None);
        // Existe con contenido.
        std::fs::write(vacio.join("x.txt"), b"x").unwrap();
        assert!(
            matches!(plan_destination(&vacio), Err(GitError::Io(m)) if m.contains("no está vacía"))
        );
        // Es un fichero.
        assert!(plan_destination(&vacio.join("x.txt")).is_err());
        assert!(plan_destination(&vacio.join("x.txt").join("hijo")).is_err());
        // Relativa, con `..`, dentro de .git, vacía.
        assert!(plan_destination(Path::new("relativa")).is_err());
        assert!(plan_destination(&base.join("a").join("..").join("b")).is_err());
        assert!(plan_destination(&base.join(".GIT").join("x")).is_err());
        assert!(plan_destination(Path::new("")).is_err());
        // Raíz del sistema.
        let root = base.ancestors().last().unwrap().to_path_buf();
        assert!(plan_destination(&root).is_err());
    }

    #[tokio::test]
    async fn cleanup_borra_solo_lo_creado() {
        let t = tempfile::tempdir().unwrap();
        let base = dunce::canonicalize(t.path()).unwrap();
        let hermano = base.join("hermano");
        std::fs::create_dir(&hermano).unwrap();
        let dest = base.join("a").join("repo");
        let plan = plan_destination(&dest).unwrap();
        std::fs::create_dir_all(dest.join(".git")).unwrap();
        cleanup(&plan).await;
        assert!(!base.join("a").exists() && hermano.exists());
        // Destino preexistente vacío: se vacía pero se conserva.
        let vacio = base.join("vacio");
        std::fs::create_dir(&vacio).unwrap();
        let plan = plan_destination(&vacio).unwrap();
        std::fs::create_dir_all(vacio.join(".git").join("objects")).unwrap();
        std::fs::write(vacio.join("f"), b"x").unwrap();
        cleanup(&plan).await;
        assert!(vacio.is_dir() && std::fs::read_dir(&vacio).unwrap().next().is_none());
    }
}
