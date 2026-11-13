#![allow(clippy::unwrap_used)]
//! Tests de `RepositorySession` con un doble de `GitServing` (sin git real salvo `repo_key`).
//!
//! Mapeo con `Tests/GitCoreTests/OperationQueueTests.swift` (8 tests):
//! 1. `testMutationCommandsAreClassified` → `solo_las_mutaciones_figuran_como_trabajo_pendiente`
//!    (la clasificación de comandos git es del motor; aquí se prueba la de la cola).
//! 2. `testMutationsAreSerialAndReadsOverlap` → `mutaciones_nunca_se_solapan` +
//!    `lecturas_concurrentes_se_solapan_entre_si`.
//! 3. `testGitRunsWithEnglishMessages` → no aplica a la sesión (entorno del `GitRunner`).
//! 4. `testHashDetection` → no aplica a la sesión (utilidad del motor).
//! 5. `testCancelledRefreshDoesNotPublishStatus` → `refresco_cancelado_no_publica_estado`.
//! 6. `testLoginShellPathIsCaptured` → no aplica (PATH del shell de login: `git_locator`).
//! 7. `testInstallScriptConfiguresHooksPath` → `el_script_de_hooks_configura_hooks_path`.
//! 8. `testBundleIdentifier` → no aplica (identidad de la app, no del núcleo git).

mod common;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ariadna_git::session::{
    repo_key, CloseOutcome, OpFuture, OperationId, RefreshOutcome, RepoKey, RepositorySession,
    SessionConfig, SessionEvent, SessionEventKind, SessionState,
};
use ariadna_git::{
    Branch, Commit, Confirmation, ConflictFile, DiffHunk, DiffScope, FileDiff, GitError, GitPath,
    GitRunner, GitServing, GitStatus, IntegrationState, MergeFastForward, MergeResult, Remote,
    Repository,
};
use async_trait::async_trait;
use tokio::sync::{broadcast, Semaphore};

// ---------------------------------------------------------------------------
// Doble
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Probe {
    log: Mutex<Vec<String>>,
    active_mut: AtomicUsize,
    max_mut: AtomicUsize,
    active_read: AtomicUsize,
    max_read: AtomicUsize,
    overlap: AtomicBool,
    status_calls: AtomicUsize,
    status_dropped: AtomicUsize,
}

impl Probe {
    fn names(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
    fn count(&self, entry: &str) -> usize {
        self.names().iter().filter(|e| *e == entry).count()
    }
    fn pos(&self, entry: &str) -> usize {
        self.names()
            .iter()
            .position(|e| e == entry)
            .unwrap_or_else(|| panic!("falta {entry} en {:?}", self.names()))
    }
}

/// Marca una llamada activa mientras viva (también si se cancela soltando el futuro).
struct Busy {
    probe: Arc<Probe>,
    name: &'static str,
    write: bool,
}

impl Busy {
    fn enter(probe: &Arc<Probe>, name: &'static str, write: bool) -> Self {
        probe.log.lock().unwrap().push(format!("start:{name}"));
        if write {
            let now = probe.active_mut.fetch_add(1, Ordering::SeqCst) + 1;
            probe.max_mut.fetch_max(now, Ordering::SeqCst);
            if probe.active_read.load(Ordering::SeqCst) > 0 {
                probe.overlap.store(true, Ordering::SeqCst);
            }
        } else {
            let now = probe.active_read.fetch_add(1, Ordering::SeqCst) + 1;
            probe.max_read.fetch_max(now, Ordering::SeqCst);
            if probe.active_mut.load(Ordering::SeqCst) > 0 {
                probe.overlap.store(true, Ordering::SeqCst);
            }
        }
        Self {
            probe: Arc::clone(probe),
            name,
            write,
        }
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        if self.write {
            self.probe.active_mut.fetch_sub(1, Ordering::SeqCst);
        } else {
            self.probe.active_read.fetch_sub(1, Ordering::SeqCst);
        }
        self.probe
            .log
            .lock()
            .unwrap()
            .push(format!("end:{}", self.name));
    }
}

struct FakeEngine {
    probe: Arc<Probe>,
    read_delay_ms: AtomicUsize,
    mut_delay_ms: AtomicUsize,
    /// Si es `true`, `status` espera un permiso de `gate` (para congelar un refresco).
    gate_status: AtomicBool,
    gate: Semaphore,
    /// Fallos `Locked` pendientes en `stage`.
    locked_failures: AtomicUsize,
    lock_path: Mutex<String>,
    fail_stage: AtomicBool,
}

impl FakeEngine {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            probe: Arc::new(Probe::default()),
            read_delay_ms: AtomicUsize::new(10),
            mut_delay_ms: AtomicUsize::new(10),
            gate_status: AtomicBool::new(false),
            gate: Semaphore::new(0),
            locked_failures: AtomicUsize::new(0),
            lock_path: Mutex::new(String::from("/x/.git/index.lock")),
            fail_stage: AtomicBool::new(false),
        })
    }
    fn with_delays(read_ms: usize, mut_ms: usize) -> Arc<Self> {
        let f = Self::new();
        f.read_delay_ms.store(read_ms, Ordering::SeqCst);
        f.mut_delay_ms.store(mut_ms, Ordering::SeqCst);
        f
    }
    async fn read_pause(&self) {
        let ms = self.read_delay_ms.load(Ordering::SeqCst) as u64;
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
    async fn mut_pause(&self) {
        let ms = self.mut_delay_ms.load(Ordering::SeqCst) as u64;
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
    async fn simple_read(&self, name: &'static str) {
        let _b = Busy::enter(&self.probe, name, false);
        self.read_pause().await;
    }
    async fn simple_write(&self, name: &'static str) -> Result<(), GitError> {
        let _b = Busy::enter(&self.probe, name, true);
        self.mut_pause().await;
        Ok(())
    }
}

/// Cuenta un `status` soltado antes de terminar.
struct DropCount<'a> {
    counter: &'a AtomicUsize,
    armed: bool,
}
impl Drop for DropCount<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.counter.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn unimpl<T>() -> Result<T, GitError> {
    Err(GitError::Io("no implementado en el doble".into()))
}

#[async_trait]
impl GitServing for FakeEngine {
    async fn open_repository(&self, _path: &Path) -> Result<Repository, GitError> {
        unimpl()
    }
    async fn status(&self, _repo: &Repository, _ignored: bool) -> Result<GitStatus, GitError> {
        let _b = Busy::enter(&self.probe, "status", false);
        self.probe.status_calls.fetch_add(1, Ordering::SeqCst);
        let mut dropped = DropCount {
            counter: &self.probe.status_dropped,
            armed: true,
        };
        if self.gate_status.load(Ordering::SeqCst) {
            self.gate.acquire().await.unwrap().forget();
        } else {
            self.read_pause().await;
        }
        dropped.armed = false;
        Ok(GitStatus {
            branch_name: Some("main".into()),
            head_oid: Some("abc".into()),
            ..GitStatus::default()
        })
    }
    async fn stage(&self, _repo: &Repository, _paths: &[GitPath]) -> Result<(), GitError> {
        let _b = Busy::enter(&self.probe, "stage", true);
        self.mut_pause().await;
        if self.fail_stage.load(Ordering::SeqCst) {
            return Err(GitError::CommandFailed {
                command: "git add".into(),
                exit_code: 1,
                stderr: "boom".into(),
            });
        }
        let left = self.locked_failures.load(Ordering::SeqCst);
        if left > 0 {
            self.locked_failures.store(left - 1, Ordering::SeqCst);
            return Err(GitError::Locked {
                lock: self.lock_path.lock().unwrap().clone(),
            });
        }
        Ok(())
    }
    async fn unstage(&self, _repo: &Repository, _paths: &[GitPath]) -> Result<(), GitError> {
        self.simple_write("unstage").await
    }
    async fn stage_hunk(
        &self,
        _repo: &Repository,
        _hunk: &DiffHunk,
        _path: &GitPath,
    ) -> Result<(), GitError> {
        unimpl()
    }
    async fn unstage_hunk(
        &self,
        _repo: &Repository,
        _hunk: &DiffHunk,
        _path: &GitPath,
    ) -> Result<(), GitError> {
        unimpl()
    }
    async fn commit(
        &self,
        _repo: &Repository,
        _message: &str,
        _allow_detached: bool,
    ) -> Result<String, GitError> {
        let _b = Busy::enter(&self.probe, "commit", true);
        self.mut_pause().await;
        Ok("deadbeef".into())
    }
    async fn amend_last_commit(&self, _repo: &Repository, _m: &str) -> Result<String, GitError> {
        unimpl()
    }
    async fn log(&self, _repo: &Repository, _limit: usize) -> Result<Vec<Commit>, GitError> {
        self.simple_read("log").await;
        Ok(Vec::new())
    }
    async fn search_log(
        &self,
        _repo: &Repository,
        _query: &str,
        _limit: usize,
    ) -> Result<Vec<Commit>, GitError> {
        self.simple_read("search_log").await;
        Ok(Vec::new())
    }
    async fn diff(
        &self,
        _repo: &Repository,
        _scope: &DiffScope,
        _paths: &[GitPath],
    ) -> Result<Vec<FileDiff>, GitError> {
        unimpl()
    }
    async fn branches(&self, _repo: &Repository) -> Result<Vec<Branch>, GitError> {
        self.simple_read("branches").await;
        Ok(Vec::new())
    }
    async fn remote_tracking_branches(&self, _repo: &Repository) -> Result<Vec<Branch>, GitError> {
        self.simple_read("remote_tracking_branches").await;
        Ok(Vec::new())
    }
    async fn create_branch(
        &self,
        _repo: &Repository,
        _name: &str,
        _start_point: Option<&str>,
    ) -> Result<(), GitError> {
        self.simple_write("create_branch").await
    }
    async fn rename_branch(&self, _repo: &Repository, _o: &str, _n: &str) -> Result<(), GitError> {
        unimpl()
    }
    async fn checkout(&self, _repo: &Repository, _r: &str, _a: bool) -> Result<(), GitError> {
        unimpl()
    }
    async fn delete_branch(&self, _repo: &Repository, _n: &str, _f: bool) -> Result<(), GitError> {
        unimpl()
    }
    async fn merge(
        &self,
        _repo: &Repository,
        _branch: &str,
        _ff: MergeFastForward,
    ) -> Result<MergeResult, GitError> {
        unimpl()
    }
    async fn integration_state(&self, _repo: &Repository) -> Result<IntegrationState, GitError> {
        self.simple_read("integration_state").await;
        Ok(IntegrationState::None)
    }
    async fn abort_integration(&self, _repo: &Repository) -> Result<(), GitError> {
        unimpl()
    }
    async fn continue_integration(&self, _repo: &Repository) -> Result<(), GitError> {
        unimpl()
    }
    async fn conflicts(&self, _repo: &Repository) -> Result<Vec<ConflictFile>, GitError> {
        self.simple_read("conflicts").await;
        Ok(Vec::new())
    }
    async fn write_working_tree_file(
        &self,
        _repo: &Repository,
        _relative_path: &GitPath,
        _contents: &[u8],
        _confirmation: &Confirmation,
    ) -> Result<(), GitError> {
        unimpl()
    }
    async fn mark_resolved(&self, _repo: &Repository, _path: &GitPath) -> Result<(), GitError> {
        unimpl()
    }
    async fn restore_worktree(
        &self,
        _repo: &Repository,
        _paths: &[GitPath],
        _confirmation: &Confirmation,
    ) -> Result<(), GitError> {
        unimpl()
    }
    async fn list_remotes(&self, _repo: &Repository) -> Result<Vec<Remote>, GitError> {
        self.simple_read("list_remotes").await;
        Ok(Vec::new())
    }
    async fn add_remote(&self, _repo: &Repository, _n: &str, _u: &str) -> Result<(), GitError> {
        unimpl()
    }
    async fn fetch(&self, _repo: &Repository, _remote: &str) -> Result<(), GitError> {
        unimpl()
    }
    async fn pull(
        &self,
        _repo: &Repository,
        _remote: &str,
        _refspec: Option<&str>,
        _rebase: bool,
    ) -> Result<MergeResult, GitError> {
        unimpl()
    }
    async fn push(
        &self,
        _repo: &Repository,
        _remote: &str,
        _refspec: Option<&str>,
        _set_upstream: bool,
    ) -> Result<(), GitError> {
        unimpl()
    }
}

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

fn repo() -> Repository {
    Repository {
        path: PathBuf::from("/tmp/ariadna-session"),
        git_dir: PathBuf::from("/tmp/ariadna-session/.git"),
    }
}

fn fast_config() -> SessionConfig {
    SessionConfig {
        lock_retry_delays: vec![
            Duration::from_millis(1),
            Duration::from_millis(2),
            Duration::from_millis(4),
        ],
        ..SessionConfig::default()
    }
}

fn session_of(fake: &Arc<FakeEngine>) -> RepositorySession {
    RepositorySession::with_config(
        Arc::clone(fake) as Arc<dyn GitServing>,
        repo(),
        "s-1".into(),
        fast_config(),
    )
}

fn stage_op(e: Arc<dyn GitServing>, r: Repository) -> OpFuture<()> {
    Box::pin(async move { e.stage(&r, &[]).await })
}
fn branch_op(e: Arc<dyn GitServing>, r: Repository) -> OpFuture<()> {
    Box::pin(async move { e.create_branch(&r, "x", None).await })
}
fn log_op(e: Arc<dyn GitServing>, r: Repository) -> OpFuture<Vec<Commit>> {
    Box::pin(async move { e.log(&r, 10).await })
}
fn search_op(e: Arc<dyn GitServing>, r: Repository) -> OpFuture<Vec<Commit>> {
    Box::pin(async move { e.search_log(&r, "q", 10).await })
}

async fn eventually(what: &str, mut cond: impl FnMut() -> bool) {
    let r = tokio::time::timeout(Duration::from_secs(10), async {
        while !cond() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await;
    assert!(r.is_ok(), "no se cumplió a tiempo: {what}");
}

fn drain(rx: &mut broadcast::Receiver<SessionEvent>) -> Vec<SessionEvent> {
    let mut v = Vec::new();
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
}

fn spawn<T: Send + 'static>(
    f: impl Future<Output = T> + Send + 'static,
) -> tokio::task::JoinHandle<T> {
    tokio::spawn(f)
}

// ---------------------------------------------------------------------------
// RepoKey
// ---------------------------------------------------------------------------

#[test]
fn repo_key_resuelve_punto_y_punto_punto() {
    let t = tempfile::tempdir().unwrap();
    let base = t.path().canonicalize().unwrap();
    std::fs::create_dir_all(base.join("a").join(".git")).unwrap();
    std::fs::create_dir_all(base.join("b")).unwrap();
    let plain = RepoKey::from_paths(&base, &base.join("a").join(".git"));
    let dotted = RepoKey::from_paths(&base, &base.join("b").join("..").join("a").join(".git"));
    let relative = RepoKey::from_paths(&base.join("a"), Path::new(".git"));
    assert_eq!(plain, dotted);
    assert_eq!(plain, relative);
    assert!(!plain.path().to_string_lossy().starts_with(r"\\?\"));
}

#[test]
fn repo_key_sin_distinguir_mayusculas_cuando_el_fs_no_lo_hace() {
    let t = tempfile::tempdir().unwrap();
    let base = t.path();
    let lower = base.join("proyecto").join(".git");
    let upper = base.join("PROYECTO").join(".GIT");
    let a = RepoKey::from_paths_with(base, &lower, true);
    let b = RepoKey::from_paths_with(base, &upper, true);
    assert_eq!(a, b);
    assert_eq!(a.as_str(), b.as_str());
    // El FS sensible a mayúsculas (Linux) mantiene claves distintas.
    let c = RepoKey::from_paths_with(base, &lower, false);
    let d = RepoKey::from_paths_with(base, &upper, false);
    assert_ne!(c, d);
}

#[test]
fn repo_key_plataforma_actual() {
    assert_eq!(
        RepoKey::platform_is_case_insensitive(),
        cfg!(any(windows, target_os = "macos"))
    );
}

#[cfg(unix)]
#[test]
fn repo_key_sigue_enlaces_simbolicos() {
    let t = tempfile::tempdir().unwrap();
    let base = t.path().canonicalize().unwrap();
    std::fs::create_dir_all(base.join("real").join(".git")).unwrap();
    std::os::unix::fs::symlink(base.join("real"), base.join("enlace")).unwrap();
    let a = RepoKey::from_paths(&base, &base.join("real").join(".git"));
    let b = RepoKey::from_paths(&base, &base.join("enlace").join(".git"));
    assert_eq!(a, b);
}

#[test]
fn repo_key_worktrees_comparten_clave_con_el_mismo_common_dir() {
    let t = tempfile::tempdir().unwrap();
    let base = t.path().canonicalize().unwrap();
    let main = base.join("main");
    let wt = base.join("wt");
    std::fs::create_dir_all(main.join(".git")).unwrap();
    std::fs::create_dir_all(&wt).unwrap();
    let from_main = RepoKey::from_paths(&main, Path::new(".git"));
    let from_wt = RepoKey::from_paths(&wt, &main.join(".git"));
    assert_eq!(from_main, from_wt);
    let mut set = std::collections::HashSet::new();
    set.insert(from_main);
    assert!(set.contains(&from_wt));
}

#[tokio::test]
async fn repo_key_con_git_real_unifica_worktrees() {
    let r = common::TestRepo::with_initial_commit();
    let runner = GitRunner::with_path(common::git_path());
    let wt = r.dir.path().join("wt-extra");
    r.git(&["worktree", "add", "-q", wt.to_str().unwrap(), "-b", "otra"]);
    let k_main = repo_key(&runner, r.path()).await.unwrap();
    let k_wt = repo_key(&runner, &wt).await.unwrap();
    assert_eq!(k_main, k_wt);
    let sub = r.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    assert_eq!(repo_key(&runner, &sub).await.unwrap(), k_main);
}

#[tokio::test]
async fn repo_key_falla_fuera_de_un_repo() {
    let t = tempfile::tempdir().unwrap();
    let runner = GitRunner::with_path(common::git_path());
    assert!(repo_key(&runner, t.path()).await.is_err());
}

// ---------------------------------------------------------------------------
// Cola de mutaciones y lecturas
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mutaciones_nunca_se_solapan() {
    let fake = FakeEngine::with_delays(5, 15);
    let s = session_of(&fake);
    let futs: Vec<_> = (0..5).map(|_| s.mutate("stage", stage_op)).collect();
    for r in futures_join(futs).await {
        r.unwrap();
    }
    assert_eq!(fake.probe.max_mut.load(Ordering::SeqCst), 1);
    assert_eq!(fake.probe.count("end:stage"), 5);
    assert!(!fake.probe.overlap.load(Ordering::SeqCst));
    assert_eq!(s.revision(), 5);
}

/// `join_all` mínimo con tareas de tokio (sin la dependencia `futures`).
async fn futures_join<T: Send + 'static>(
    futs: Vec<impl Future<Output = T> + Send + 'static>,
) -> Vec<T> {
    let handles: Vec<_> = futs.into_iter().map(tokio::spawn).collect();
    let mut out = Vec::new();
    for h in handles {
        out.push(h.await.unwrap());
    }
    out
}

#[tokio::test]
async fn lecturas_concurrentes_se_solapan_entre_si() {
    let fake = FakeEngine::with_delays(80, 5);
    let s = session_of(&fake);
    let a = s.read("a", log_op);
    let b = s.read("b", log_op);
    let (ra, rb) = tokio::join!(a, b);
    ra.unwrap();
    rb.unwrap();
    assert_eq!(fake.probe.max_read.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn lectura_posterior_a_una_mutacion_en_cola_espera() {
    let fake = FakeEngine::with_delays(5, 40);
    let s = session_of(&fake);
    let m1 = s.mutate("m1", stage_op);
    let m2 = s.mutate("m2", stage_op);
    let rd = s.read("lectura", log_op);
    let (a, b, c) = tokio::join!(m1, m2, rd);
    a.unwrap();
    b.unwrap();
    c.unwrap();
    let relevant: Vec<String> = fake
        .probe
        .names()
        .into_iter()
        .filter(|n| n.ends_with(":stage") || n.ends_with(":log"))
        .collect();
    assert_eq!(
        relevant,
        [
            "start:stage",
            "end:stage",
            "start:stage",
            "end:stage",
            "start:log",
            "end:log"
        ]
    );
    assert!(!fake.probe.overlap.load(Ordering::SeqCst));
}

#[tokio::test]
async fn una_mutacion_encolada_espera_a_la_lectura_en_curso_y_pasa_antes_que_otra_lectura() {
    let fake = FakeEngine::with_delays(60, 10);
    let s = session_of(&fake);
    let r1 = spawn(s.read("r1", log_op));
    eventually("r1 en curso", || fake.probe.count("start:log") == 1).await;
    let m = s.mutate("m", stage_op);
    let r2 = s.read("r2", search_op);
    let (m, r2) = tokio::join!(m, r2);
    m.unwrap();
    r2.unwrap();
    r1.await.unwrap().unwrap();
    let p = &fake.probe;
    assert!(p.pos("end:log") < p.pos("start:stage"), "{:?}", p.names());
    assert!(p.pos("end:stage") < p.pos("start:search_log"));
    assert!(!p.overlap.load(Ordering::SeqCst));
}

#[tokio::test]
async fn prioridad_de_mutacion_sobre_lecturas_que_ya_esperaban() {
    let fake = FakeEngine::with_delays(5, 60);
    let s = session_of(&fake);
    let m1 = spawn(s.mutate("m1", stage_op));
    eventually("m1 en curso", || fake.probe.count("start:stage") == 1).await;
    // La lectura llega ANTES que la segunda mutación y aun así esta pasa primero.
    let rd = s.read("lectura", search_op);
    let m2 = s.mutate("m2", branch_op);
    let (rd, m2) = tokio::join!(rd, m2);
    rd.unwrap();
    m2.unwrap();
    m1.await.unwrap().unwrap();
    let p = &fake.probe;
    assert!(
        p.pos("end:create_branch") < p.pos("start:search_log"),
        "{:?}",
        p.names()
    );
    assert!(!p.overlap.load(Ordering::SeqCst));
}

#[tokio::test]
async fn solo_las_mutaciones_figuran_como_trabajo_pendiente() {
    let fake = FakeEngine::with_delays(30, 30);
    let s = session_of(&fake);
    assert!(!s.has_pending_work());
    let m1 = s.mutate("commit", stage_op);
    let m2 = s.mutate("crear rama", branch_op);
    let rd = s.read("log", log_op);
    assert!(s.has_pending_work());
    let pending = s.pending_operations();
    assert_eq!(pending.len(), 2, "las lecturas no cuentan: {pending:?}");
    assert_eq!(pending[0].label, "commit");
    assert_eq!(pending[1].label, "crear rama");
    assert!(pending.iter().all(|p| !p.running));
    let (a, b, c) = tokio::join!(m1, m2, rd);
    a.unwrap();
    b.unwrap();
    c.unwrap();
    assert!(!s.has_pending_work());
    assert!(s.pending_operations().is_empty());
}

// ---------------------------------------------------------------------------
// Revisión, eventos y snapshots
// ---------------------------------------------------------------------------

#[tokio::test]
async fn la_revision_es_monotona_y_los_eventos_llevan_session_id() {
    let fake = FakeEngine::new();
    let s = session_of(&fake);
    let mut rx = s.events();
    assert_eq!(s.revision(), 0);
    for expected in 1..=4u64 {
        s.mutate("stage", stage_op).await.unwrap();
        assert_eq!(s.revision(), expected);
    }
    let mut last = s.subscribe();
    tokio::time::timeout(
        Duration::from_secs(10),
        last.wait_for(|snap| snap.revision == 4),
    )
    .await
    .unwrap()
    .unwrap();
    let events = drain(&mut rx);
    assert!(events.iter().all(|e| e.session_id == "s-1"));
    let revisions: Vec<u64> = events.iter().map(|e| e.revision).collect();
    assert!(revisions.windows(2).all(|w| w[0] <= w[1]), "{revisions:?}");
    let finished: Vec<u64> = events
        .iter()
        .filter(|e| matches!(e.kind, SessionEventKind::OperationFinished { .. }))
        .map(|e| e.revision)
        .collect();
    assert_eq!(finished, [1, 2, 3, 4]);
    assert!(events
        .iter()
        .any(|e| matches!(e.kind, SessionEventKind::SnapshotPublished { revision: 4 })));
}

#[tokio::test]
async fn una_mutacion_fallida_no_sube_la_revision_ni_bloquea_la_cola() {
    let fake = FakeEngine::new();
    let s = session_of(&fake);
    fake.fail_stage.store(true, Ordering::SeqCst);
    let err = s.mutate("stage", stage_op).await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }));
    assert_eq!(s.revision(), 0);
    fake.fail_stage.store(false, Ordering::SeqCst);
    s.mutate("stage", stage_op).await.unwrap();
    assert_eq!(s.revision(), 1);
    assert!(!s.has_pending_work());
}

#[tokio::test]
async fn open_publica_el_primer_snapshot_y_activa_la_sesion() {
    let fake = FakeEngine::new();
    let s = session_of(&fake);
    assert_eq!(s.state(), SessionState::Opening);
    let mut rx = s.events();
    let mut watch = s.subscribe();
    let out = s.open().await.unwrap();
    let RefreshOutcome::Published(snap) = out else {
        panic!("se esperaba Published");
    };
    assert_eq!(snap.revision, 1);
    assert_eq!(snap.status.branch_name.as_deref(), Some("main"));
    assert_eq!(s.state(), SessionState::Active);
    watch.changed().await.unwrap();
    assert_eq!(watch.borrow().status.head_oid.as_deref(), Some("abc"));
    let kinds: Vec<_> = drain(&mut rx).into_iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&SessionEventKind::StateChanged {
        state: SessionState::Active
    }));
    assert!(kinds.contains(&SessionEventKind::SnapshotPublished { revision: 1 }));
}

// ---------------------------------------------------------------------------
// Refresco
// ---------------------------------------------------------------------------

#[tokio::test]
async fn diez_refrescos_seguidos_se_coalescen_en_dos_ejecuciones_como_maximo() {
    let fake = FakeEngine::with_delays(20, 5);
    let s = session_of(&fake);
    let futs: Vec<_> = (0..10).map(|_| s.refresh()).collect();
    let results = futures_join(futs).await;
    assert!(results.iter().all(Result::is_ok));
    let calls = fake.probe.status_calls.load(Ordering::SeqCst);
    assert!((1..=2).contains(&calls), "status se ejecutó {calls} veces");
    assert!(matches!(
        s.snapshot().status.branch_name.as_deref(),
        Some("main")
    ));
}

#[tokio::test]
async fn un_refresco_se_descarta_si_hay_una_mutacion_en_cola() {
    let fake = FakeEngine::new();
    let s = session_of(&fake);
    let m = s.mutate("stage", stage_op); // en cola, aún sin ejecutar
    let out = s.refresh().await.unwrap();
    assert!(matches!(out, RefreshOutcome::Skipped));
    assert_eq!(fake.probe.status_calls.load(Ordering::SeqCst), 0);
    m.await.unwrap();
    // La mutación programó su propio refresco.
    let mut watch = s.subscribe();
    tokio::time::timeout(Duration::from_secs(10), watch.wait_for(|x| x.revision == 1))
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn refresco_cancelado_no_publica_estado() {
    // Port de testCancelledRefreshDoesNotPublishStatus.
    let fake = FakeEngine::new();
    fake.gate_status.store(true, Ordering::SeqCst);
    let s = session_of(&fake);
    let mut events = s.events();
    let (id, fut) = s.refresh_with_id();
    let task = spawn(fut);
    eventually("status en curso", || {
        fake.probe.status_calls.load(Ordering::SeqCst) == 1
    })
    .await;
    assert!(s.cancel_operation(id));
    let err = task.await.unwrap().unwrap_err();
    assert_eq!(err, GitError::Cancelled);
    let snap = s.snapshot();
    assert_eq!(snap.revision, 0);
    assert!(snap.status.branch_name.is_none());
    assert!(snap.status.entries.is_empty());
    assert!(snap.status.head_oid.is_none());
    eventually("status soltado", || {
        fake.probe.status_dropped.load(Ordering::SeqCst) == 1
    })
    .await;
    // Aunque el git "siga vivo", nada se publica después.
    fake.gate.add_permits(4);
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(s.snapshot().status.head_oid.is_none());
    assert!(!drain(&mut events)
        .iter()
        .any(|e| matches!(e.kind, SessionEventKind::SnapshotPublished { .. })));
}

#[tokio::test]
async fn un_refresco_obsoleto_no_publica_un_estado_a_medias() {
    let fake = FakeEngine::new();
    fake.gate_status.store(true, Ordering::SeqCst);
    let s = session_of(&fake);
    let refresh = spawn(s.refresh());
    eventually("status en curso", || {
        fake.probe.status_calls.load(Ordering::SeqCst) == 1
    })
    .await;
    // Una mutación entra mientras el refresco está a medias.
    let m = s.mutate("stage", stage_op);
    fake.gate.add_permits(1);
    let out = refresh.await.unwrap().unwrap();
    assert!(matches!(out, RefreshOutcome::Skipped), "{out:?}");
    assert_eq!(s.snapshot().revision, 0);
    assert!(s.snapshot().status.branch_name.is_none());
    // La mutación termina y su refresco (ya sin congelar) sí publica la revisión nueva.
    fake.gate_status.store(false, Ordering::SeqCst);
    m.await.unwrap();
    let mut watch = s.subscribe();
    let snap = tokio::time::timeout(Duration::from_secs(10), watch.wait_for(|x| x.revision == 1))
        .await
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(snap.status.branch_name.as_deref(), Some("main"));
}

// ---------------------------------------------------------------------------
// Reintento ante index.lock
// ---------------------------------------------------------------------------

#[test]
fn los_reintentos_por_defecto_son_200_400_800_ms() {
    assert_eq!(
        SessionConfig::default().lock_retry_delays,
        [
            Duration::from_millis(200),
            Duration::from_millis(400),
            Duration::from_millis(800)
        ]
    );
}

#[tokio::test]
async fn locked_agota_los_reintentos_devuelve_el_error_y_no_borra_el_lock() {
    let dir = tempfile::tempdir().unwrap();
    let lock = dir.path().join("index.lock");
    std::fs::write(&lock, b"ajeno").unwrap();
    let fake = FakeEngine::new();
    *fake.lock_path.lock().unwrap() = lock.display().to_string();
    fake.locked_failures.store(1_000, Ordering::SeqCst);
    let s = session_of(&fake);
    let mut rx = s.events();
    let err = s.mutate("stage", stage_op).await.unwrap_err();
    assert!(matches!(err, GitError::Locked { .. }), "{err:?}");
    assert_eq!(
        fake.probe.count("start:stage"),
        4,
        "1 intento + 3 reintentos"
    );
    assert!(lock.exists(), "la sesión nunca borra el lock");
    assert_eq!(std::fs::read(&lock).unwrap(), b"ajeno");
    assert_eq!(s.revision(), 0);
    let delays: Vec<Duration> = drain(&mut rx)
        .into_iter()
        .filter_map(|e| match e.kind {
            SessionEventKind::OperationRetrying { delay, .. } => Some(delay),
            _ => None,
        })
        .collect();
    assert_eq!(
        delays,
        [
            Duration::from_millis(1),
            Duration::from_millis(2),
            Duration::from_millis(4)
        ]
    );
}

#[tokio::test]
async fn locked_transitorio_se_recupera_sin_intervencion() {
    let fake = FakeEngine::new();
    fake.locked_failures.store(2, Ordering::SeqCst);
    let s = session_of(&fake);
    s.mutate("stage", stage_op).await.unwrap();
    assert_eq!(fake.probe.count("start:stage"), 3);
    assert_eq!(s.revision(), 1);
}

#[tokio::test]
async fn mutate_once_y_otros_errores_no_se_reintentan() {
    let fake = FakeEngine::new();
    fake.locked_failures.store(5, Ordering::SeqCst);
    let s = session_of(&fake);
    let err = s.mutate_once("stage", stage_op).await.unwrap_err();
    assert!(matches!(err, GitError::Locked { .. }));
    assert_eq!(fake.probe.count("start:stage"), 1);

    fake.locked_failures.store(0, Ordering::SeqCst);
    fake.fail_stage.store(true, Ordering::SeqCst);
    let err = s.mutate("stage", stage_op).await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }));
    assert_eq!(fake.probe.count("start:stage"), 2);
}

// ---------------------------------------------------------------------------
// Cierre
// ---------------------------------------------------------------------------

/// Mutación larga que coopera con la cancelación de la sesión.
fn cooperative_op(
    token: ariadna_process::CancellationToken,
    started: Arc<AtomicUsize>,
) -> impl Fn(Arc<dyn GitServing>, Repository) -> OpFuture<()> + Send + Sync + 'static {
    move |_e, _r| {
        let token = token.clone();
        let started = Arc::clone(&started);
        Box::pin(async move {
            started.fetch_add(1, Ordering::SeqCst);
            tokio::select! {
                () = tokio::time::sleep(Duration::from_secs(30)) => Ok(()),
                () = token.cancelled() => Err(GitError::Cancelled),
            }
        })
    }
}

#[tokio::test]
async fn close_durante_una_mutacion_larga_cancela_y_vacia_la_cola() {
    let fake = FakeEngine::new();
    let s = session_of(&fake);
    let started = Arc::new(AtomicUsize::new(0));
    let long = spawn(s.mutate(
        "larga",
        cooperative_op(s.cancel_token(), Arc::clone(&started)),
    ));
    let q1 = spawn(s.mutate("cola 1", stage_op));
    let q2 = spawn(s.mutate("cola 2", branch_op));
    eventually("mutación larga en curso", || {
        started.load(Ordering::SeqCst) == 1
    })
    .await;
    assert!(s.has_pending_work());
    let pending = s.pending_operations();
    assert_eq!(pending.len(), 3);
    assert!(pending[0].running && pending[0].label == "larga");

    let mut events = s.events();
    let outcome = s.close(Duration::from_secs(5)).await;
    assert_eq!(outcome, CloseOutcome::Clean);
    assert_eq!(long.await.unwrap().unwrap_err(), GitError::Cancelled);
    assert_eq!(q1.await.unwrap().unwrap_err(), GitError::Cancelled);
    assert_eq!(q2.await.unwrap().unwrap_err(), GitError::Cancelled);
    assert_eq!(
        fake.probe.count("start:stage"),
        0,
        "la cola no llegó a ejecutarse"
    );
    assert_eq!(fake.probe.count("start:create_branch"), 0);
    assert_eq!(s.state(), SessionState::Closed);
    assert!(!s.has_pending_work());
    assert_eq!(s.revision(), 0);
    let states: Vec<SessionState> = drain(&mut events)
        .into_iter()
        .filter_map(|e| match e.kind {
            SessionEventKind::StateChanged { state } => Some(state),
            _ => None,
        })
        .collect();
    assert_eq!(states, [SessionState::Closing, SessionState::Closed]);
}

#[tokio::test]
async fn close_fuerza_la_cancelacion_si_la_operacion_no_coopera() {
    let fake = FakeEngine::new();
    let s = session_of(&fake);
    let dropped = Arc::new(AtomicUsize::new(0));
    let started = Arc::new(AtomicUsize::new(0));
    let (d, st) = (Arc::clone(&dropped), Arc::clone(&started));
    let stubborn = spawn(s.mutate("terca", move |_e, _r| {
        let (d, st) = (Arc::clone(&d), Arc::clone(&st));
        Box::pin(async move {
            struct OnDrop(Arc<AtomicUsize>);
            impl Drop for OnDrop {
                fn drop(&mut self) {
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
            }
            let _g = OnDrop(d);
            st.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok(())
        })
    }));
    eventually("operación terca en curso", || {
        started.load(Ordering::SeqCst) == 1
    })
    .await;
    let outcome = s.close(Duration::from_millis(30)).await;
    assert_eq!(outcome, CloseOutcome::Forced);
    assert_eq!(stubborn.await.unwrap().unwrap_err(), GitError::Cancelled);
    assert_eq!(dropped.load(Ordering::SeqCst), 1, "el futuro se soltó");
    assert_eq!(s.state(), SessionState::Closed);
}

#[tokio::test]
async fn tras_closed_toda_llamada_devuelve_cancelled_y_no_llega_al_motor() {
    let fake = FakeEngine::new();
    let s = session_of(&fake);
    assert_eq!(
        s.close(Duration::from_millis(10)).await,
        CloseOutcome::Clean
    );
    assert_eq!(s.state(), SessionState::Closed);
    assert_eq!(
        s.mutate("m", stage_op).await.unwrap_err(),
        GitError::Cancelled
    );
    assert_eq!(
        s.mutate_once("m", stage_op).await.unwrap_err(),
        GitError::Cancelled
    );
    assert_eq!(s.read("r", log_op).await.unwrap_err(), GitError::Cancelled);
    assert_eq!(s.refresh().await.unwrap_err(), GitError::Cancelled);
    assert_eq!(s.open().await.unwrap_err(), GitError::Cancelled);
    assert!(fake.probe.names().is_empty(), "{:?}", fake.probe.names());
    assert!(!s.has_pending_work());
    assert_eq!(s.revision(), 0);
}

#[tokio::test]
async fn los_eventos_tardios_se_descartan_tras_closed() {
    let fake = FakeEngine::new();
    let s = session_of(&fake);
    let mut rx = s.events();
    s.close(Duration::from_millis(10)).await;
    let before = drain(&mut rx);
    assert!(matches!(
        before.last().map(|e| &e.kind),
        Some(SessionEventKind::StateChanged {
            state: SessionState::Closed
        })
    ));
    let _ = s.mutate("m", stage_op).await;
    let _ = s.read("r", log_op).await;
    let _ = s.refresh().await;
    assert!(
        drain(&mut rx).is_empty(),
        "no deben emitirse eventos tras Closed"
    );

    // Una operación forzada: su evento de fin llega como mucho antes del cierre, nunca después.
    let s2 = session_of(&fake);
    let mut rx2 = s2.events();
    let started = Arc::new(AtomicUsize::new(0));
    let t = spawn(s2.mutate("terca", {
        let st = Arc::clone(&started);
        move |_e, _r| {
            let st = Arc::clone(&st);
            Box::pin(async move {
                st.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_secs(30)).await;
                Ok(())
            })
        }
    }));
    eventually("en curso", || started.load(Ordering::SeqCst) == 1).await;
    assert_eq!(
        s2.close(Duration::from_millis(10)).await,
        CloseOutcome::Forced
    );
    let _ = t.await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    let events = drain(&mut rx2);
    let closed_at = events
        .iter()
        .position(|e| {
            e.kind
                == SessionEventKind::StateChanged {
                    state: SessionState::Closed,
                }
        })
        .unwrap();
    assert_eq!(
        closed_at,
        events.len() - 1,
        "nada después de Closed: {events:?}"
    );
}

#[tokio::test]
async fn close_es_idempotente_tambien_en_concurrencia() {
    let fake = FakeEngine::new();
    let s = session_of(&fake);
    let (a, b) = tokio::join!(
        s.close(Duration::from_millis(50)),
        s.close(Duration::from_millis(50))
    );
    let mut got = [a, b];
    got.sort_by_key(|o| matches!(o, CloseOutcome::AlreadyClosed));
    assert_eq!(got, [CloseOutcome::Clean, CloseOutcome::AlreadyClosed]);
    assert_eq!(
        s.close(Duration::from_millis(50)).await,
        CloseOutcome::AlreadyClosed
    );
    assert_eq!(s.state(), SessionState::Closed);
    let mut state_rx = s.subscribe_state();
    assert_eq!(*state_rx.borrow_and_update(), SessionState::Closed);
}

// ---------------------------------------------------------------------------
// Concurrencia entre sesiones y cancelación por operación
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn veinte_sesiones_a_la_vez_sin_interbloqueos() {
    let work = async {
        let mut handles = Vec::new();
        for i in 0..20 {
            handles.push(tokio::spawn(async move {
                let fake = FakeEngine::with_delays(3, 3);
                let s = RepositorySession::with_config(
                    Arc::clone(&fake) as Arc<dyn GitServing>,
                    repo(),
                    format!("s-{i}"),
                    fast_config(),
                );
                let mut handles = Vec::new();
                for _ in 0..3 {
                    handles.push(tokio::spawn(s.mutate("m", stage_op)));
                    let r = s.read("r", log_op);
                    handles.push(tokio::spawn(async move { r.await.map(|_| ()) }));
                    let f = s.refresh();
                    handles.push(tokio::spawn(async move { f.await.map(|_| ()) }));
                }
                for h in handles {
                    h.await.unwrap().unwrap();
                }
                assert_eq!(s.revision(), 3);
                assert_eq!(fake.probe.max_mut.load(Ordering::SeqCst), 1);
                assert!(!fake.probe.overlap.load(Ordering::SeqCst));
                assert_eq!(s.close(Duration::from_secs(5)).await, CloseOutcome::Clean);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
    };
    tokio::time::timeout(Duration::from_secs(30), work)
        .await
        .expect("interbloqueo entre sesiones");
}

#[tokio::test]
async fn cancel_operation_saca_una_mutacion_de_la_cola_sin_ejecutarla() {
    let fake = FakeEngine::with_delays(5, 60);
    let s = session_of(&fake);
    let m1 = spawn(s.mutate("m1", stage_op));
    eventually("m1 en curso", || fake.probe.count("start:stage") == 1).await;
    let (id2, m2) = s.mutate_with_id("m2", branch_op);
    let m2 = spawn(m2);
    assert!(s
        .pending_operations()
        .iter()
        .any(|p| p.operation_id == id2 && !p.running));
    assert!(s.cancel_operation(id2));
    assert_eq!(m2.await.unwrap().unwrap_err(), GitError::Cancelled);
    m1.await.unwrap().unwrap();
    assert_eq!(fake.probe.count("start:create_branch"), 0);
    assert_eq!(s.revision(), 1, "solo cuenta la mutación que terminó bien");
    assert!(!s.has_pending_work());
    // Ya terminada o ajena: no hay nada que cancelar.
    assert!(!s.cancel_operation(id2));
    let other = session_of(&FakeEngine::new());
    let (foreign, f) = other.mutate_with_id("x", stage_op);
    assert!(!s.cancel_operation(foreign));
    f.await.unwrap();
}

#[tokio::test]
async fn cancel_operation_suelta_una_mutacion_en_curso_y_la_cola_sigue() {
    let fake = FakeEngine::new();
    let s = session_of(&fake);
    let dropped = Arc::new(AtomicUsize::new(0));
    let started = Arc::new(AtomicUsize::new(0));
    let (d, st) = (Arc::clone(&dropped), Arc::clone(&started));
    let (id, long) = s.mutate_with_id("larga", move |_e, _r| {
        let (d, st) = (Arc::clone(&d), Arc::clone(&st));
        Box::pin(async move {
            struct OnDrop(Arc<AtomicUsize>);
            impl Drop for OnDrop {
                fn drop(&mut self) {
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
            }
            let _g = OnDrop(d);
            st.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok(())
        })
    });
    let long = spawn(long);
    let next = spawn(s.mutate("siguiente", stage_op));
    eventually("larga en curso", || started.load(Ordering::SeqCst) == 1).await;
    assert!(s.cancel_operation(id));
    assert_eq!(long.await.unwrap().unwrap_err(), GitError::Cancelled);
    next.await.unwrap().unwrap();
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(s.revision(), 1);
    assert_eq!(s.state(), SessionState::Active);
}

#[tokio::test]
async fn cancel_operation_cancela_una_lectura_en_espera() {
    let fake = FakeEngine::with_delays(5, 60);
    let s = session_of(&fake);
    let m = spawn(s.mutate("m", stage_op));
    eventually("m en curso", || fake.probe.count("start:stage") == 1).await;
    let (id, rd) = s.read_with_id("lectura", log_op);
    let rd = spawn(rd);
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert!(s.cancel_operation(id));
    assert_eq!(rd.await.unwrap().unwrap_err(), GitError::Cancelled);
    m.await.unwrap().unwrap();
    assert_eq!(fake.probe.count("start:log"), 0);
}

#[test]
fn los_ids_de_operacion_son_unicos_y_se_pueden_mostrar() {
    let a = fresh_id();
    let b = fresh_id();
    assert_ne!(a, b);
    assert!(a.to_string().starts_with("op-"));
    assert!(b.get() > a.get());
}

fn fresh_id() -> OperationId {
    let s = RepositorySession::new(
        FakeEngine::new() as Arc<dyn GitServing>,
        repo(),
        "ids".into(),
    );
    // El futuro se suelta sin ejecutarse y la operación se da de baja sola.
    s.read_with_id("id", log_op).0
}

// ---------------------------------------------------------------------------
// Port de testInstallScriptConfiguresHooksPath
// ---------------------------------------------------------------------------

#[test]
fn el_script_de_hooks_configura_hooks_path() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("scripts")
        .join("install-git-hooks.sh");
    let text = std::fs::read_to_string(script).unwrap();
    assert!(text.contains("git -C \"$root\" config --local core.hooksPath .githooks"));
    assert!(text.contains("config --local --get core.hooksPath"));
}
