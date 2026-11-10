//! Vigilancia de archivos de la pestaña activa (spec §8).
//!
//! * Worktree recursivo + `.git/{HEAD,index,refs,…}` aparte; el ruido (`.git/objects`,
//!   `*.lock`, `node_modules`, `target`, `dist`…) se descarta en el propio callback de `notify`.
//!   Las carpetas de compilación con nombres comunes (`bin`, `obj`, `packages`) solo se
//!   descartan si git las ignora (`git check-ignore`, una vez por carpeta).
//! * Los eventos se coalescen: 300 ms de silencio, con tope de 2 s desde el primero.
//! * Si el vigilante no arranca, desborda o el sistema de archivos no emite eventos (WSL
//!   `/mnt/c`, redes, UNC) se cae a un sondeo ligero que solo corre con la ventana enfocada.
//!
//! La lógica pura ([`PathFilter`], [`Coalescer`]) no toca el disco y se prueba aparte.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Silencio necesario antes de refrescar.
pub const QUIET: Duration = Duration::from_millis(300);
/// Tope desde el primer evento pendiente (una tormenta continua no aplaza el refresco para siempre).
pub const MAX_WAIT: Duration = Duration::from_secs(2);
/// Periodo del sondeo de respaldo.
pub const POLL_EVERY: Duration = Duration::from_millis(2500);
/// Tope de directorios vigilados uno a uno (Linux/inotify) antes de rendirse y sondear.
const MAX_WATCHED_DIRS: usize = 8192;

/// Carpetas que nunca se vigilan ni disparan refrescos (cualquier nivel del worktree).
const IGNORED_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    ".svelte-kit",
    ".next",
    ".nuxt",
    ".turbo",
    ".cache",
    ".parcel-cache",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".venv",
    "venv",
    ".gradle",
    ".idea",
    ".vs",
];

/// Carpetas de compilación (.NET, NuGet…) cuyo nombre también se usa para código versionado
/// (`bin/` con scripts, p. ej.): solo se descartan si git las ignora.
const BUILD_DIRS_IF_IGNORED: &[&str] = &["bin", "obj", "packages"];

/// Ficheros del directorio git cuyo cambio altera el estado visible.
const GIT_STATE_FILES: &[&str] = &[
    "HEAD",
    "index",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "REBASE_HEAD",
    "packed-refs",
    "shallow",
    "config",
];
/// Carpetas del directorio git que señalan una operación en curso (merge, rebase…).
const GIT_STATE_DIRS: &[&str] = &["rebase-merge", "rebase-apply", "sequencer"];

// --- filtro de rutas (puro) -------------------------------------------------------------

/// Qué hacer con una ruta que ha cambiado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Ignore,
    /// Cambio en el árbol de trabajo.
    Worktree,
    /// Cambio relevante dentro de `.git`.
    GitState,
}

impl Verdict {
    pub fn is_relevant(self) -> bool {
        self != Verdict::Ignore
    }
}

/// Clasifica rutas absolutas respecto de un worktree y sus directorios git.
#[derive(Debug, Clone)]
pub struct PathFilter {
    roots: Vec<PathBuf>,
    git_dirs: Vec<PathBuf>,
    case_insensitive: bool,
    ignore_probe: Option<Arc<IgnoreProbe>>,
}

/// Pregunta a git si una carpeta está ignorada y recuerda la respuesta.
#[derive(Debug)]
pub struct IgnoreProbe {
    git: PathBuf,
    worktree: PathBuf,
    cache: Mutex<HashMap<Vec<String>, bool>>,
}

impl IgnoreProbe {
    pub fn new(git: PathBuf, worktree: PathBuf) -> Self {
        Self {
            git,
            worktree,
            cache: Mutex::default(),
        }
    }

    /// ¿Ignora git la carpeta `rel` (componentes relativos al worktree)? Si git no responde,
    /// se vigila: es mejor un refresco de más que perder un cambio real.
    fn is_ignored_dir(&self, rel: &[String]) -> bool {
        if let Some(known) = self.cache.lock().ok().and_then(|c| c.get(rel).copied()) {
            return known;
        }
        // La barra final hace que los patrones de solo carpeta (`bin/`) también cuenten.
        let path = format!("{}/", rel.join("/"));
        let mut cmd = Command::new(&self.git);
        cmd.arg("-C")
            .arg(&self.worktree)
            .args(["check-ignore", "-q", "--"])
            .arg(&path)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let ignored = cmd.status().map(|s| s.code() == Some(0)).unwrap_or(false);
        if let Ok(mut c) = self.cache.lock() {
            c.insert(rel.to_vec(), ignored);
        }
        ignored
    }
}

fn variants(p: &Path) -> Vec<PathBuf> {
    let mut v = vec![dunce::simplified(p).to_path_buf()];
    if let Ok(c) = dunce::canonicalize(p) {
        if !v.contains(&c) {
            v.push(c);
        }
    }
    v
}

impl PathFilter {
    /// `common_dir` es el directorio git compartido si difiere de `git_dir` (worktrees enlazados).
    pub fn new(worktree: &Path, git_dir: &Path, common_dir: Option<&Path>) -> Self {
        let mut git_dirs = variants(git_dir);
        if let Some(c) = common_dir {
            for v in variants(c) {
                if !git_dirs.contains(&v) {
                    git_dirs.push(v);
                }
            }
        }
        Self {
            roots: variants(worktree),
            git_dirs,
            case_insensitive: cfg!(any(windows, target_os = "macos")),
            ignore_probe: None,
        }
    }

    /// Activa el descarte de `bin`/`obj`/`packages` cuando git los ignora.
    pub fn with_ignore_probe(mut self, probe: IgnoreProbe) -> Self {
        self.ignore_probe = Some(Arc::new(probe));
        self
    }

    /// ¿Hay en `rel` una carpeta de compilación que git ignora?
    fn inside_ignored_build_dir(&self, rel: &[String]) -> bool {
        let Some(probe) = &self.ignore_probe else {
            return false;
        };
        rel.iter().enumerate().any(|(i, c)| {
            BUILD_DIRS_IF_IGNORED
                .iter()
                .any(|d| d.eq_ignore_ascii_case(c))
                && probe.is_ignored_dir(&rel[..=i])
        })
    }

    /// Parte de `path` bajo `base`, comparando componente a componente.
    fn strip(&self, path: &Path, base: &Path) -> Option<Vec<String>> {
        let mut p = path.components();
        for b in base.components() {
            let a = p.next()?;
            let same = a == b
                || (self.case_insensitive
                    && a.as_os_str()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy()));
            if !same {
                return None;
            }
        }
        Some(
            p.filter_map(|c| match c {
                Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect(),
        )
    }

    pub fn classify(&self, path: &Path) -> Verdict {
        let path = dunce::simplified(path);
        for g in &self.git_dirs {
            if let Some(rel) = self.strip(path, g) {
                return classify_git(&rel);
            }
        }
        for r in &self.roots {
            if let Some(rel) = self.strip(path, r) {
                let verdict = classify_worktree(&rel);
                if verdict == Verdict::Worktree && self.inside_ignored_build_dir(&rel) {
                    return Verdict::Ignore;
                }
                return verdict;
            }
        }
        Verdict::Ignore
    }

    /// ¿Se vigila esta carpeta del worktree (para el recorrido de un solo nivel)?
    pub fn watches_dir(&self, path: &Path) -> bool {
        self.classify(path) == Verdict::Worktree
    }
}

fn is_noise_file(name: &str) -> bool {
    name == ".DS_Store"
        || name == "4913" // sonda de Vim
        || name.starts_with(".#")
        || name.ends_with('~')
        || [".swp", ".swx", ".tmp", ".lock"]
            .iter()
            .any(|e| name.ends_with(e))
}

fn classify_worktree(rel: &[String]) -> Verdict {
    let Some(last) = rel.last() else {
        return Verdict::Ignore;
    };
    if rel
        .iter()
        .any(|c| c == ".git" || IGNORED_DIRS.contains(&c.as_str()))
        || is_noise_file(last)
    {
        return Verdict::Ignore;
    }
    Verdict::Worktree
}

fn classify_git(rel: &[String]) -> Verdict {
    let Some(first) = rel.first() else {
        return Verdict::Ignore;
    };
    if rel.iter().any(|c| c.ends_with(".lock")) {
        return Verdict::Ignore;
    }
    let first = first.as_str();
    if first == "refs" {
        return if rel.len() >= 2 {
            Verdict::GitState
        } else {
            Verdict::Ignore
        };
    }
    if rel.len() == 1 && GIT_STATE_FILES.contains(&first) {
        return Verdict::GitState;
    }
    if GIT_STATE_DIRS.contains(&first) {
        return Verdict::GitState;
    }
    Verdict::Ignore
}

// --- coalescedor (puro) -----------------------------------------------------------------

/// Agrupa ráfagas de eventos en un único disparo.
#[derive(Debug, Clone)]
pub struct Coalescer {
    quiet: Duration,
    max_wait: Duration,
    first: Option<Instant>,
    last: Option<Instant>,
}

impl Coalescer {
    pub fn new(quiet: Duration, max_wait: Duration) -> Self {
        Self {
            quiet,
            max_wait,
            first: None,
            last: None,
        }
    }

    pub fn push(&mut self, now: Instant) {
        self.first.get_or_insert(now);
        self.last = Some(now);
    }

    pub fn is_pending(&self) -> bool {
        self.first.is_some()
    }

    /// Cuándo toca disparar (el menor de «silencio» y «tope»).
    pub fn deadline(&self) -> Option<Instant> {
        let (first, last) = (self.first?, self.last?);
        Some((last + self.quiet).min(first + self.max_wait))
    }

    /// Dispara (y limpia) si ya toca.
    pub fn fire_if_due(&mut self, now: Instant) -> bool {
        match self.deadline() {
            Some(d) if d <= now => {
                self.first = None;
                self.last = None;
                true
            }
            _ => false,
        }
    }
}

// --- vigilante --------------------------------------------------------------------------

pub type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
/// Qué hacer al disparar (refrescar la sesión).
pub type Trigger = Arc<dyn Fn() -> BoxFuture + Send + Sync>;
/// Aviso de degradación a sondeo por error (se llama como mucho una vez por vigilante).
pub type OnDegraded = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Debug, Clone, Copy)]
pub struct WatchTimings {
    pub quiet: Duration,
    pub max_wait: Duration,
    pub poll_every: Duration,
}

impl Default for WatchTimings {
    fn default() -> Self {
        Self {
            quiet: QUIET,
            max_wait: MAX_WAIT,
            poll_every: POLL_EVERY,
        }
    }
}

pub struct WatchConfig {
    pub worktree: PathBuf,
    pub git_dir: PathBuf,
    pub timings: WatchTimings,
    /// Ejecutable de git para preguntar qué carpetas de compilación ignora. Sin él, `bin`,
    /// `obj` y `packages` se vigilan como cualquier otra carpeta.
    pub git: Option<PathBuf>,
}

enum Msg {
    Change,
    NewDir(PathBuf),
    Failure(String),
}

/// Vigilante de una pestaña. Al soltarlo se cancela la tarea y se libera el vigilante del SO.
pub struct TabWatcher {
    task: JoinHandle<()>,
    polling: Arc<AtomicBool>,
}

impl TabWatcher {
    /// `true` si está en modo sondeo (por entorno o por fallo).
    pub fn is_polling(&self) -> bool {
        self.polling.load(Ordering::SeqCst)
    }

    /// Arranca el vigilante. Nunca falla: si no puede vigilar de forma nativa, sondea.
    /// Debe llamarse dentro de un runtime de Tokio.
    pub fn start(
        cfg: WatchConfig,
        trigger: Trigger,
        focused: Arc<AtomicBool>,
        on_degraded: OnDegraded,
    ) -> Self {
        let common = read_common_dir(&cfg.git_dir);
        let mut filter = PathFilter::new(&cfg.worktree, &cfg.git_dir, common.as_deref());
        if let Some(git) = &cfg.git {
            filter = filter.with_ignore_probe(IgnoreProbe::new(git.clone(), cfg.worktree.clone()));
        }
        let (tx, rx) = mpsc::unbounded_channel::<Msg>();
        let polling = Arc::new(AtomicBool::new(false));
        // Un solo `Msg::Change` en la cola a la vez: mientras el refresco está en marcha, una
        // tormenta de eventos (npm install, dotnet restore) no la hace crecer sin límite.
        let change_queued = Arc::new(AtomicBool::new(false));

        let mut degraded = false;
        let mut budget = MAX_WATCHED_DIRS;
        let watcher = if fs_never_notifies(&cfg.worktree) {
            // Entorno conocido sin eventos fiables: sondeo directo y sin aviso.
            polling.store(true, Ordering::SeqCst);
            None
        } else {
            match setup_native(
                &cfg,
                common.as_deref(),
                &filter,
                &tx,
                &change_queued,
                &mut budget,
            ) {
                Ok(w) => Some(w),
                Err(reason) => {
                    degraded = true;
                    polling.store(true, Ordering::SeqCst);
                    on_degraded(reason);
                    None
                }
            }
        };
        drop(tx);

        let task = tokio::spawn(run(
            watcher,
            filter,
            rx,
            change_queued,
            budget,
            cfg.timings,
            trigger,
            focused,
            Arc::clone(&polling),
            on_degraded,
            degraded,
        ));
        Self { task, polling }
    }
}

impl Drop for TabWatcher {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[allow(clippy::too_many_arguments)]
async fn run(
    mut watcher: Option<RecommendedWatcher>,
    filter: PathFilter,
    mut rx: mpsc::UnboundedReceiver<Msg>,
    change_queued: Arc<AtomicBool>,
    // Lo que dejó el primer recorrido: el tope de carpetas es uno para toda la vida del vigilante.
    mut budget: usize,
    timings: WatchTimings,
    trigger: Trigger,
    focused: Arc<AtomicBool>,
    polling: Arc<AtomicBool>,
    on_degraded: OnDegraded,
    mut degraded: bool,
) {
    let mut co = Coalescer::new(timings.quiet, timings.max_wait);
    let mut poll = tokio::time::interval_at(
        tokio::time::Instant::now() + timings.poll_every,
        timings.poll_every,
    );
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Sin canal de eventos (modo sondeo puro) el receptor se cierra: se sigue sondeando.
    let mut rx_open = true;
    loop {
        let deadline = co.deadline().map(tokio::time::Instant::from_std);
        tokio::select! {
            msg = rx.recv(), if rx_open => match msg {
                None => rx_open = false,
                Some(Msg::Change) => {
                    // Primero se baja la bandera: un evento posterior volverá a encolar.
                    change_queued.store(false, Ordering::SeqCst);
                    co.push(Instant::now());
                }
                Some(Msg::NewDir(dir)) => {
                    co.push(Instant::now());
                    if let Some(w) = watcher.as_mut() {
                        if let Err(reason) = watch_tree(w, &dir, &filter, &mut budget) {
                            polling.store(true, Ordering::SeqCst);
                            if !degraded {
                                degraded = true;
                                on_degraded(reason);
                            }
                        }
                    }
                }
                Some(Msg::Failure(reason)) => {
                    // Desbordamiento o error: refresco completo y, de ahí en adelante, sondeo.
                    co.push(Instant::now());
                    polling.store(true, Ordering::SeqCst);
                    if !degraded {
                        degraded = true;
                        on_degraded(reason);
                    }
                }
            },
            () = async {
                if let Some(d) = deadline {
                    tokio::time::sleep_until(d).await;
                }
            }, if deadline.is_some() => {
                if co.fire_if_due(Instant::now()) {
                    trigger().await;
                }
            }
            _ = poll.tick(), if polling.load(Ordering::SeqCst) => {
                if focused.load(Ordering::SeqCst) {
                    trigger().await;
                }
            }
        }
    }
}

fn setup_native(
    cfg: &WatchConfig,
    common: Option<&Path>,
    filter: &PathFilter,
    tx: &mpsc::UnboundedSender<Msg>,
    change_queued: &Arc<AtomicBool>,
    budget: &mut usize,
) -> Result<RecommendedWatcher, String> {
    let cb_filter = filter.clone();
    let cb_tx = tx.clone();
    let cb_queued = Arc::clone(change_queued);
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| match res {
        Ok(ev) => {
            if ev.need_rescan() {
                let _ = cb_tx.send(Msg::Failure("el sistema pidió releer todo".to_owned()));
                return;
            }
            if matches!(ev.kind, EventKind::Access(_) | EventKind::Other) {
                return;
            }
            let mut relevant = false;
            // macOS (FSEvents) y Windows avisan de que una carpeta «cambió» cuando se crea o
            // borra algo dentro, incluso si ese algo es ruido; el hijo ya genera su propio evento.
            let dir_modified = matches!(
                ev.kind,
                EventKind::Modify(notify::event::ModifyKind::Data(_))
                    | EventKind::Modify(notify::event::ModifyKind::Metadata(_))
                    | EventKind::Modify(notify::event::ModifyKind::Any)
            );
            for p in &ev.paths {
                if dir_modified && p.is_dir() {
                    continue;
                }
                let v = cb_filter.classify(p);
                if v.is_relevant() {
                    relevant = true;
                    if v == Verdict::Worktree
                        && matches!(ev.kind, EventKind::Create(_))
                        && p.is_dir()
                    {
                        let _ = cb_tx.send(Msg::NewDir(p.clone()));
                    }
                }
            }
            if relevant && !cb_queued.swap(true, Ordering::SeqCst) {
                let _ = cb_tx.send(Msg::Change);
            }
        }
        Err(e) => {
            let _ = cb_tx.send(Msg::Failure(e.to_string()));
        }
    })
    .map_err(describe)?;

    watch_tree(&mut watcher, &cfg.worktree, filter, budget)?;
    // `.git` sin recursión (HEAD, index, MERGE_HEAD…) y `refs/` completo; nunca `objects/`.
    let mut git_dirs = vec![cfg.git_dir.clone()];
    if let Some(c) = common {
        if c != cfg.git_dir {
            git_dirs.push(c.to_path_buf());
        }
    }
    for g in git_dirs {
        watcher
            .watch(&g, RecursiveMode::NonRecursive)
            .map_err(describe)?;
        let refs = g.join("refs");
        if refs.is_dir() {
            watcher
                .watch(&refs, RecursiveMode::Recursive)
                .map_err(describe)?;
        }
    }
    Ok(watcher)
}

fn describe(e: notify::Error) -> String {
    match e.kind {
        notify::ErrorKind::MaxFilesWatch => {
            "se agotó el límite de archivos vigilados del sistema".to_owned()
        }
        _ => e.to_string(),
    }
}

/// Vigila el worktree. En Linux (inotify) no hay recursión nativa barata: se recorre el árbol
/// saltando carpetas ignoradas para no gastar un manejador por cada `node_modules/**`. En
/// Windows y macOS la vigilancia recursiva del SO es barata y el filtro descarta el ruido.
fn watch_tree(
    w: &mut RecommendedWatcher,
    dir: &Path,
    filter: &PathFilter,
    budget: &mut usize,
) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return w.watch(dir, RecursiveMode::Recursive).map_err(describe);
    }
    // El propio worktree no pasa por `watches_dir` (su ruta relativa está vacía).
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        if *budget == 0 {
            return Err("demasiadas carpetas que vigilar".to_owned());
        }
        *budget -= 1;
        w.watch(&d, RecursiveMode::NonRecursive).map_err(describe)?;
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in rd.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() && filter.watches_dir(&entry.path()) {
                stack.push(entry.path());
            }
        }
    }
    Ok(())
}

/// `commondir` de un worktree enlazado (apunta al `.git` principal).
fn read_common_dir(git_dir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(git_dir.join("commondir")).ok()?;
    let rel = text.trim();
    if rel.is_empty() {
        return None;
    }
    let p = git_dir.join(rel);
    dunce::canonicalize(&p).ok().or(Some(p))
}

/// Entornos donde el SO no entrega eventos de archivos fiables: carpetas de red (UNC en
/// Windows; 9p/drvfs/NFS/CIFS en Linux, p. ej. `/mnt/c` bajo WSL).
fn fs_never_notifies(worktree: &Path) -> bool {
    let p = dunce::simplified(worktree);
    if cfg!(windows) {
        return p.to_string_lossy().starts_with("\\\\");
    }
    remote_fs(p)
}

#[cfg(not(target_os = "linux"))]
fn remote_fs(_: &Path) -> bool {
    false
}

#[cfg(target_os = "linux")]
fn remote_fs(p: &Path) -> bool {
    linux_remote_fs(p)
}

#[cfg(target_os = "linux")]
fn linux_remote_fs(p: &Path) -> bool {
    const REMOTE: &[&str] = &[
        "9p",
        "drvfs",
        "cifs",
        "smb3",
        "smbfs",
        "nfs",
        "nfs4",
        "fuse.sshfs",
        "afs",
        "vboxsf",
    ];
    let Ok(real) = dunce::canonicalize(p) else {
        return false;
    };
    let Ok(mounts) = std::fs::read_to_string("/proc/self/mounts") else {
        return false;
    };
    // El punto de montaje más largo que sea prefijo de la ruta decide.
    mounts
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let (_dev, mp, fstype) = (f.next()?, f.next()?, f.next()?);
            let mp = mp.replace("\\040", " ");
            real.starts_with(&mp).then(|| (mp.len(), fstype.to_owned()))
        })
        .max_by_key(|(len, _)| *len)
        .is_some_and(|(_, fs)| REMOTE.contains(&fs.as_str()))
}

#[cfg(test)]
mod tests;
