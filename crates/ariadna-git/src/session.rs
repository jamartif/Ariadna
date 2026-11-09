//! `RepositorySession`: cola de mutaciones, revisión, refresco coalescido y ciclo de vida
//! (spec §3.1–§3.3 y §8). Equivale a `RepositorySession.swift`, pero con cola explícita.
//!
//! Reglas:
//! * **Mutaciones**: serie estricta por sesión y con prioridad sobre las lecturas. Una
//!   mutación espera a que terminen las lecturas en curso; mientras haya una mutación en
//!   curso o en cola, las lecturas nuevas esperan.
//! * **Lecturas**: pueden solaparse entre sí, nunca con una mutación.
//! * **Revisión**: sube en 1 tras cada mutación con éxito. Los snapshots y los eventos la llevan.
//! * **Refresco**: como mucho uno en curso y uno pendiente. Se descarta si hay una mutación en
//!   cola, y un refresco cancelado u obsoleto (la revisión cambió) no publica nada.
//! * **`index.lock` ajeno**: reintento acotado (`SessionConfig::lock_retry_delays`) y después se
//!   devuelve el error. Nunca se borra el lock.
//! * **Cierre**: cancelación cooperativa, plazo de gracia, cancelación forzosa, cola vacía.
//!   Después de `Closed` todo devuelve [`GitError::Cancelled`] y los eventos tardíos se descartan.
//!
//! La sesión solo depende del trait [`GitServing`]; no conoce `GitCliEngine`.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ariadna_process::CancellationToken;
use tokio::sync::{broadcast, oneshot, watch, Notify};

use crate::error::{GitError, GitResult};
use crate::models::{Branch, ConflictFile, GitStatus, IntegrationState, Remote, Repository};
use crate::runner::{CommandKind, GitRunner};
use crate::serving::GitServing;

// ---------------------------------------------------------------------------
// Clave de repo
// ---------------------------------------------------------------------------

/// Clave de unicidad de un repo (spec §3.2 / §8): ruta canónica del `--git-common-dir`, sin
/// prefijo `\\?\`, comparada sin distinguir mayúsculas en Windows y macOS. Dos worktrees del
/// mismo repo tienen la misma clave.
#[derive(Debug, Clone)]
pub struct RepoKey {
    /// Forma comparable (plegada). Es lo único que cuenta para `Eq`/`Hash`.
    folded: String,
    /// Ruta canónica legible (sin plegar), para mensajes.
    display: PathBuf,
}

impl RepoKey {
    /// ¿El sistema de archivos de esta plataforma distingue mayúsculas? (Windows y macOS no.)
    pub fn platform_is_case_insensitive() -> bool {
        cfg!(any(windows, target_os = "macos"))
    }

    /// Construye la clave con las reglas de la plataforma actual.
    ///
    /// `worktree` es el directorio desde el que se ejecutó git (sirve para resolver un
    /// `common_dir` relativo, como el `.git` que devuelve `git rev-parse --git-common-dir`).
    pub fn from_paths(worktree: &Path, common_dir: &Path) -> Self {
        Self::from_paths_with(worktree, common_dir, Self::platform_is_case_insensitive())
    }

    /// Como [`from_paths`](Self::from_paths) pero eligiendo si se pliegan las mayúsculas
    /// (para poder probar ambos modos en cualquier plataforma).
    pub fn from_paths_with(worktree: &Path, common_dir: &Path, case_insensitive: bool) -> Self {
        let joined = if common_dir.is_absolute() {
            common_dir.to_path_buf()
        } else {
            worktree.join(common_dir)
        };
        let canonical = dunce::canonicalize(&joined).unwrap_or_else(|_| lexical_clean(&joined));
        let mut text = canonical.to_string_lossy().into_owned();
        if cfg!(windows) {
            text = text.replace('\\', "/");
        }
        while text.len() > 1 && text.ends_with('/') {
            text.pop();
        }
        if case_insensitive {
            text = text.to_lowercase();
        }
        Self {
            folded: text,
            display: canonical,
        }
    }

    /// Forma comparable de la clave.
    pub fn as_str(&self) -> &str {
        &self.folded
    }

    /// Ruta canónica sin plegar.
    pub fn path(&self) -> &Path {
        &self.display
    }
}

impl PartialEq for RepoKey {
    fn eq(&self, other: &Self) -> bool {
        self.folded == other.folded
    }
}
impl Eq for RepoKey {}
impl Hash for RepoKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.folded.hash(state);
    }
}
impl fmt::Display for RepoKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.folded)
    }
}

/// Limpieza léxica (`.` y `..`) para rutas que no existen y no se pueden canonicalizar.
fn lexical_clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Calcula la clave de un repo ejecutando `git rev-parse --git-common-dir` en `path`.
pub async fn repo_key(runner: &GitRunner, path: &Path) -> GitResult<RepoKey> {
    let out = runner
        .run_ok_str(path, CommandKind::Read, &["rev-parse", "--git-common-dir"])
        .await?;
    let text = out.stdout_lossy();
    let common = text.trim();
    if common.is_empty() {
        return Err(GitError::NotARepository {
            path: path.display().to_string(),
        });
    }
    Ok(RepoKey::from_paths(path, Path::new(common)))
}

// ---------------------------------------------------------------------------
// Tipos públicos
// ---------------------------------------------------------------------------

/// Ciclo de vida (spec §3.1): `Abriendo → Activa → Cerrando → Cerrada`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionState {
    Opening,
    Active,
    Closing,
    Closed,
}

/// Identificador de operación, único en todo el proceso (sirve para `cancel_operation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationId(u64);

static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);

impl OperationId {
    fn next() -> Self {
        Self(NEXT_OPERATION.fetch_add(1, Ordering::Relaxed))
    }
    /// Valor numérico (para el IPC).
    pub fn get(self) -> u64 {
        self.0
    }
}
impl fmt::Display for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "op-{}", self.0)
    }
}

/// Resultado de cerrar la sesión.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOutcome {
    /// Nada en curso, o terminó (cooperativamente) dentro del plazo de gracia.
    Clean,
    /// Hubo que cancelar a la fuerza operaciones que no terminaron en el plazo de gracia.
    Forced,
    /// La sesión ya estaba cerrada (o se estaba cerrando por otra llamada).
    AlreadyClosed,
}

/// Resultado de una petición de refresco.
#[derive(Debug, Clone)]
pub enum RefreshOutcome {
    /// Se publicó este snapshot.
    Published(Arc<Snapshot>),
    /// Se descartó sin publicar: hay mutaciones en cola/en curso o el resultado quedó obsoleto.
    /// La propia mutación programa su refresco, así que no se pierde información.
    Skipped,
}

/// Estado consistente del repo en una revisión.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub revision: u64,
    pub status: GitStatus,
    pub branches: Vec<Branch>,
    pub remote_branches: Vec<Branch>,
    pub remotes: Vec<Remote>,
    pub integration: IntegrationState,
    pub conflicts: Vec<ConflictFile>,
}

impl Snapshot {
    /// Snapshot vacío de la revisión 0 (antes del primer refresco).
    pub fn empty() -> Self {
        Self {
            revision: 0,
            status: GitStatus::default(),
            branches: Vec::new(),
            remote_branches: Vec::new(),
            remotes: Vec::new(),
            integration: IntegrationState::None,
            conflicts: Vec::new(),
        }
    }
}

/// Ajustes de la sesión.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Esperas entre reintentos ante `index.lock` ajeno (spec §3.2: 200/400/800 ms ⇒ hasta
    /// 3 reintentos tras el primer intento). Vacío = sin reintentos.
    pub lock_retry_delays: Vec<Duration>,
    /// Capacidad del canal de eventos (un receptor lento pierde los más antiguos).
    pub event_capacity: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            lock_retry_delays: vec![
                Duration::from_millis(200),
                Duration::from_millis(400),
                Duration::from_millis(800),
            ],
            event_capacity: 256,
        }
    }
}

/// Evento de sesión. Todos llevan `session_id` y la `revision` vigente al emitirlo (spec §3.3).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionEvent {
    pub session_id: String,
    pub revision: u64,
    pub kind: SessionEventKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionEventKind {
    StateChanged {
        state: SessionState,
    },
    OperationStarted {
        operation_id: OperationId,
        label: String,
    },
    /// Reintento por `index.lock` ajeno; `attempt` empieza en 1.
    OperationRetrying {
        operation_id: OperationId,
        label: String,
        attempt: u32,
        delay: Duration,
    },
    OperationFinished {
        operation_id: OperationId,
        label: String,
        result: Result<(), GitError>,
    },
    SnapshotPublished {
        revision: u64,
    },
    RefreshFailed {
        error: GitError,
    },
}

/// Mutación en curso o en cola (para el diálogo de confirmación al cerrar).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingOperation {
    pub operation_id: OperationId,
    pub label: String,
    /// `true` si ya se está ejecutando; `false` si espera en la cola.
    pub running: bool,
}

/// Futuro boxeado que devuelven las operaciones que se encolan.
pub type OpFuture<T> = Pin<Box<dyn Future<Output = GitResult<T>> + Send>>;

// ---------------------------------------------------------------------------
// Núcleo compartido
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpKind {
    Mutation,
    Read,
    Refresh,
}

struct OpEntry {
    label: String,
    kind: OpKind,
    running: bool,
    token: CancellationToken,
}

type RefreshResult = GitResult<RefreshOutcome>;

struct Waiter {
    id: OperationId,
    tx: oneshot::Sender<RefreshResult>,
}

#[derive(Default)]
struct RefreshState {
    running: bool,
    pending: bool,
    run_waiters: Vec<Waiter>,
    pending_waiters: Vec<Waiter>,
    pass_token: Option<CancellationToken>,
}

struct Sched {
    state: SessionState,
    revision: u64,
    active_reads: usize,
    mutation_active: bool,
    queue: VecDeque<OperationId>,
    ops: HashMap<OperationId, OpEntry>,
    refresh: RefreshState,
}

impl Sched {
    fn open(&self) -> bool {
        matches!(self.state, SessionState::Opening | SessionState::Active)
    }
    fn mutation_pending(&self) -> bool {
        self.mutation_active || !self.queue.is_empty()
    }
    fn idle(&self) -> bool {
        !self.mutation_active && self.active_reads == 0
    }
}

enum Step<R> {
    Ready(R),
    Wait,
    Closed,
}

struct Core {
    engine: Arc<dyn GitServing>,
    repo: Repository,
    session_id: String,
    config: SessionConfig,
    sched: Mutex<Sched>,
    wake: Notify,
    /// Cancelación cooperativa de toda la sesión (la ven los ejecutores git que la usen).
    cancel: CancellationToken,
    /// Cancelación forzosa: suelta los futuros que no cooperaron tras el plazo de gracia.
    force: CancellationToken,
    snapshot_tx: watch::Sender<Arc<Snapshot>>,
    state_tx: watch::Sender<SessionState>,
    events: broadcast::Sender<SessionEvent>,
}

/// Registro de una operación: se da de baja (y sale de la cola) al soltarlo.
struct OpGuard {
    core: Arc<Core>,
    id: OperationId,
    kind: OpKind,
    token: CancellationToken,
}

impl Drop for OpGuard {
    fn drop(&mut self) {
        {
            let mut s = self.core.lock();
            s.ops.remove(&self.id);
            s.queue.retain(|q| *q != self.id);
        }
        if self.kind == OpKind::Refresh {
            self.core.abandon_refresh(self.id);
        }
        self.core.wake.notify_waiters();
    }
}

struct ReadPermit {
    core: Arc<Core>,
}

impl Drop for ReadPermit {
    fn drop(&mut self) {
        {
            let mut s = self.core.lock();
            s.active_reads = s.active_reads.saturating_sub(1);
        }
        self.core.wake.notify_waiters();
    }
}

struct MutationPermit {
    core: Arc<Core>,
    done: bool,
}

impl MutationPermit {
    /// Libera el turno. Si `bump`, sube la revisión **en la misma sección crítica**, de modo que
    /// ningún refresco pueda publicar un estado anterior a la mutación.
    fn finish(mut self, bump: bool) {
        self.release(bump);
    }

    fn release(&mut self, bump: bool) {
        if self.done {
            return;
        }
        self.done = true;
        {
            let mut s = self.core.lock();
            s.mutation_active = false;
            if bump {
                s.revision += 1;
            }
        }
        self.core.wake.notify_waiters();
    }
}

impl Drop for MutationPermit {
    fn drop(&mut self) {
        self.release(false);
    }
}

impl Core {
    fn lock(&self) -> MutexGuard<'_, Sched> {
        self.sched.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn emit(&self, kind: SessionEventKind) {
        let (state, revision) = {
            let s = self.lock();
            (s.state, s.revision)
        };
        // Eventos tardíos: tras Closed solo pasa el propio cierre.
        let is_closed_marker = matches!(
            kind,
            SessionEventKind::StateChanged {
                state: SessionState::Closed
            }
        );
        if state == SessionState::Closed && !is_closed_marker {
            return;
        }
        let _ = self.events.send(SessionEvent {
            session_id: self.session_id.clone(),
            revision,
            kind,
        });
    }

    fn set_state(&self, new: SessionState) {
        let changed = {
            let mut s = self.lock();
            if s.state == new || s.state == SessionState::Closed {
                false
            } else {
                s.state = new;
                true
            }
        };
        if changed {
            self.state_tx.send_replace(new);
            self.emit(SessionEventKind::StateChanged { state: new });
        }
    }

    fn activate_if_opening(&self) {
        let opening = self.lock().state == SessionState::Opening;
        if opening {
            self.set_state(SessionState::Active);
        }
    }

    fn register(self: &Arc<Self>, label: &str, kind: OpKind) -> OpGuard {
        let id = OperationId::next();
        let token = CancellationToken::new();
        {
            let mut s = self.lock();
            s.ops.insert(
                id,
                OpEntry {
                    label: label.to_string(),
                    kind,
                    running: false,
                    token: token.clone(),
                },
            );
            if kind == OpKind::Mutation && s.open() {
                s.queue.push_back(id);
            }
        }
        OpGuard {
            core: Arc::clone(self),
            id,
            kind,
            token,
        }
    }

    /// Espera (sin bloquear el hilo) a que `check` devuelva `Ready`. `Closed` ⇒ `Cancelled`.
    async fn wait_until<R>(
        &self,
        token: &CancellationToken,
        mut check: impl FnMut(&mut Sched) -> Step<R>,
    ) -> GitResult<R> {
        loop {
            let notified = self.wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let step = {
                let mut s = self.lock();
                check(&mut s)
            };
            match step {
                Step::Ready(r) => return Ok(r),
                Step::Closed => return Err(GitError::Cancelled),
                Step::Wait => {}
            }
            tokio::select! {
                () = &mut notified => {}
                () = token.cancelled() => return Err(GitError::Cancelled),
                () = self.force.cancelled() => return Err(GitError::Cancelled),
            }
        }
    }

    async fn acquire_read(self: &Arc<Self>, token: &CancellationToken) -> GitResult<ReadPermit> {
        self.wait_until(token, |s| {
            if !s.open() {
                Step::Closed
            } else if s.mutation_pending() {
                Step::Wait
            } else {
                s.active_reads += 1;
                Step::Ready(())
            }
        })
        .await?;
        Ok(ReadPermit {
            core: Arc::clone(self),
        })
    }

    async fn acquire_mutation(self: &Arc<Self>, guard: &OpGuard) -> GitResult<MutationPermit> {
        let id = guard.id;
        self.wait_until(&guard.token, |s| {
            if !s.open() {
                Step::Closed
            } else if s.queue.front() == Some(&id) && s.idle() {
                s.queue.pop_front();
                s.mutation_active = true;
                if let Some(e) = s.ops.get_mut(&id) {
                    e.running = true;
                }
                Step::Ready(())
            } else {
                Step::Wait
            }
        })
        .await?;
        Ok(MutationPermit {
            core: Arc::clone(self),
            done: false,
        })
    }

    /// Ejecuta `fut` hasta que termine o lo cancelen (la operación o el cierre forzoso).
    async fn drive<T>(
        &self,
        token: &CancellationToken,
        fut: impl Future<Output = GitResult<T>>,
    ) -> GitResult<T> {
        tokio::select! {
            biased;
            r = fut => r,
            () = token.cancelled() => Err(GitError::Cancelled),
            () = self.force.cancelled() => Err(GitError::Cancelled),
        }
    }

    async fn wait_idle(&self) {
        loop {
            let notified = self.wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.lock().idle() {
                return;
            }
            notified.await;
        }
    }

    // --- mutaciones ------------------------------------------------------------------------

    async fn run_mutation<T, M>(
        self: &Arc<Self>,
        guard: OpGuard,
        label: String,
        retry: bool,
        mut make: M,
    ) -> GitResult<T>
    where
        M: FnMut(Arc<dyn GitServing>, Repository) -> OpFuture<T>,
    {
        let operation_id = guard.id;
        let permit = match self.acquire_mutation(&guard).await {
            Ok(p) => p,
            Err(e) => {
                self.emit(SessionEventKind::OperationFinished {
                    operation_id,
                    label,
                    result: Err(e.clone()),
                });
                return Err(e);
            }
        };
        self.emit(SessionEventKind::OperationStarted {
            operation_id,
            label: label.clone(),
        });

        let mut attempt = 0usize;
        let result = loop {
            let fut = make(Arc::clone(&self.engine), self.repo.clone());
            match self.drive(&guard.token, fut).await {
                Err(GitError::Locked { .. })
                    if retry && attempt < self.config.lock_retry_delays.len() =>
                {
                    let delay = self.config.lock_retry_delays[attempt];
                    attempt += 1;
                    self.emit(SessionEventKind::OperationRetrying {
                        operation_id,
                        label: label.clone(),
                        attempt: u32::try_from(attempt).unwrap_or(u32::MAX),
                        delay,
                    });
                    tokio::select! {
                        () = tokio::time::sleep(delay) => {}
                        () = guard.token.cancelled() => break Err(GitError::Cancelled),
                        () = self.cancel.cancelled() => break Err(GitError::Cancelled),
                        () = self.force.cancelled() => break Err(GitError::Cancelled),
                    }
                }
                other => break other,
            }
        };

        let ok = result.is_ok();
        permit.finish(ok);
        if ok {
            self.activate_if_opening();
        }
        self.emit(SessionEventKind::OperationFinished {
            operation_id,
            label,
            result: result.as_ref().map(|_| ()).map_err(Clone::clone),
        });
        if !matches!(result, Err(GitError::Cancelled)) {
            self.schedule_refresh();
        }
        result
    }

    // --- refresco --------------------------------------------------------------------------

    /// Pide un refresco sin esperar el resultado (lo usan las mutaciones).
    fn schedule_refresh(self: &Arc<Self>) {
        let (tx, _rx) = oneshot::channel();
        self.enqueue_refresh(OperationId::next(), tx);
    }

    fn enqueue_refresh(self: &Arc<Self>, id: OperationId, tx: oneshot::Sender<RefreshResult>) {
        let spawn = {
            let mut s = self.lock();
            if !s.open() {
                drop(s);
                let _ = tx.send(Err(GitError::Cancelled));
                return;
            }
            let waiter = Waiter { id, tx };
            if s.refresh.running {
                s.refresh.pending = true;
                s.refresh.pending_waiters.push(waiter);
                false
            } else {
                s.refresh.running = true;
                s.refresh.run_waiters.push(waiter);
                s.refresh.pass_token = Some(CancellationToken::new());
                true
            }
        };
        if spawn {
            tokio::spawn(refresh_worker(Arc::clone(self)));
        }
    }

    /// Una petición de refresco se abandona (cancelada o su futuro soltado). Si nadie más
    /// espera la pasada en curso, se aborta.
    fn abandon_refresh(&self, id: OperationId) {
        let mut s = self.lock();
        s.refresh.run_waiters.retain(|w| w.id != id);
        s.refresh.pending_waiters.retain(|w| w.id != id);
        if s.refresh.pending_waiters.is_empty() {
            s.refresh.pending = false;
        }
        if s.refresh.running && s.refresh.run_waiters.is_empty() {
            if let Some(t) = &s.refresh.pass_token {
                t.cancel();
            }
        }
    }

    /// Cierre: aborta la pasada en curso y rechaza a todos los que esperaban.
    fn abort_refresh(&self) {
        let waiters = {
            let mut s = self.lock();
            s.refresh.pending = false;
            if let Some(t) = &s.refresh.pass_token {
                t.cancel();
            }
            let mut w = std::mem::take(&mut s.refresh.run_waiters);
            w.append(&mut s.refresh.pending_waiters);
            w
        };
        for w in waiters {
            let _ = w.tx.send(Err(GitError::Cancelled));
        }
    }

    /// Una lectura de la pasada de refresco. `None` ⇒ hay una mutación pendiente (se descarta).
    async fn refresh_step<T>(
        self: &Arc<Self>,
        token: &CancellationToken,
        make: impl FnOnce(Arc<dyn GitServing>, Repository) -> OpFuture<T>,
    ) -> GitResult<Option<T>> {
        if self.lock().mutation_pending() {
            return Ok(None);
        }
        let permit = self.acquire_read(token).await?;
        let r = self
            .drive(token, make(Arc::clone(&self.engine), self.repo.clone()))
            .await;
        drop(permit);
        r.map(Some)
    }

    async fn fetch_snapshot(
        self: &Arc<Self>,
        token: &CancellationToken,
        revision: u64,
    ) -> GitResult<Option<Snapshot>> {
        let Some(status) = self
            .refresh_step(token, |e, r| {
                Box::pin(async move { e.status(&r, false).await })
            })
            .await?
        else {
            return Ok(None);
        };
        let Some(branches) = self
            .refresh_step(token, |e, r| Box::pin(async move { e.branches(&r).await }))
            .await?
        else {
            return Ok(None);
        };
        let Some(remote_branches) = self
            .refresh_step(token, |e, r| {
                Box::pin(async move { e.remote_tracking_branches(&r).await })
            })
            .await?
        else {
            return Ok(None);
        };
        let Some(remotes) = self
            .refresh_step(token, |e, r| {
                Box::pin(async move { e.list_remotes(&r).await })
            })
            .await?
        else {
            return Ok(None);
        };
        let Some(integration) = self
            .refresh_step(token, |e, r| {
                Box::pin(async move { e.integration_state(&r).await })
            })
            .await?
        else {
            return Ok(None);
        };
        let Some(conflicts) = self
            .refresh_step(token, |e, r| Box::pin(async move { e.conflicts(&r).await }))
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(Snapshot {
            revision,
            status,
            branches,
            remote_branches,
            remotes,
            integration,
            conflicts,
        }))
    }

    async fn refresh_pass(self: &Arc<Self>, token: &CancellationToken) -> RefreshResult {
        let start = {
            let s = self.lock();
            if !s.open() {
                return Err(GitError::Cancelled);
            }
            if s.mutation_pending() {
                return Ok(RefreshOutcome::Skipped);
            }
            s.revision
        };
        let built = match self.fetch_snapshot(token, start).await {
            Ok(b) => b,
            Err(e) => {
                if e != GitError::Cancelled {
                    self.emit(SessionEventKind::RefreshFailed { error: e.clone() });
                }
                return Err(e);
            }
        };
        let Some(snapshot) = built else {
            return Ok(RefreshOutcome::Skipped);
        };
        let snapshot = Arc::new(snapshot);
        // Comprobar y publicar en la misma sección crítica que sube la revisión.
        let published = {
            let s = self.lock();
            if !s.open() || token.is_cancelled() {
                return Err(GitError::Cancelled);
            }
            if s.revision == start && !s.mutation_pending() {
                self.snapshot_tx.send_replace(Arc::clone(&snapshot));
                true
            } else {
                false
            }
        };
        if published {
            self.activate_if_opening();
            self.emit(SessionEventKind::SnapshotPublished { revision: start });
            Ok(RefreshOutcome::Published(snapshot))
        } else {
            Ok(RefreshOutcome::Skipped)
        }
    }
}

/// Tarea que ejecuta pasadas de refresco: la en curso y, si alguien pidió otra mientras
/// tanto, una única pasada pendiente más.
async fn refresh_worker(core: Arc<Core>) {
    loop {
        let token = core.lock().refresh.pass_token.clone();
        let result = match token {
            Some(t) => core.refresh_pass(&t).await,
            None => Err(GitError::Cancelled),
        };
        let (waiters, again) = {
            let mut s = core.lock();
            let waiters = std::mem::take(&mut s.refresh.run_waiters);
            if s.refresh.pending && s.open() {
                s.refresh.pending = false;
                s.refresh.run_waiters = std::mem::take(&mut s.refresh.pending_waiters);
                s.refresh.pass_token = Some(CancellationToken::new());
                (waiters, true)
            } else {
                s.refresh.running = false;
                s.refresh.pending = false;
                s.refresh.pass_token = None;
                for w in std::mem::take(&mut s.refresh.pending_waiters) {
                    let _ = w.tx.send(Err(GitError::Cancelled));
                }
                (waiters, false)
            }
        };
        for w in waiters {
            let _ = w.tx.send(result.clone());
        }
        if !again {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// API pública
// ---------------------------------------------------------------------------

/// Sesión de un repo abierto. `Clone` barato (`Arc` interno).
#[derive(Clone)]
pub struct RepositorySession {
    core: Arc<Core>,
}

impl fmt::Debug for RepositorySession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RepositorySession")
            .field("session_id", &self.core.session_id)
            .field("repo", &self.core.repo.path)
            .field("state", &self.state())
            .finish()
    }
}

impl RepositorySession {
    /// Crea la sesión en estado `Opening` con los ajustes por defecto.
    pub fn new(engine: Arc<dyn GitServing>, repo: Repository, session_id: String) -> Self {
        Self::with_config(engine, repo, session_id, SessionConfig::default())
    }

    pub fn with_config(
        engine: Arc<dyn GitServing>,
        repo: Repository,
        session_id: String,
        config: SessionConfig,
    ) -> Self {
        let (events, _) = broadcast::channel(config.event_capacity.max(1));
        let (snapshot_tx, _) = watch::channel(Arc::new(Snapshot::empty()));
        let (state_tx, _) = watch::channel(SessionState::Opening);
        let core = Core {
            engine,
            repo,
            session_id,
            config,
            sched: Mutex::new(Sched {
                state: SessionState::Opening,
                revision: 0,
                active_reads: 0,
                mutation_active: false,
                queue: VecDeque::new(),
                ops: HashMap::new(),
                refresh: RefreshState::default(),
            }),
            wake: Notify::new(),
            cancel: CancellationToken::new(),
            force: CancellationToken::new(),
            snapshot_tx,
            state_tx,
            events,
        };
        Self {
            core: Arc::new(core),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.core.session_id
    }

    pub fn repository(&self) -> &Repository {
        &self.core.repo
    }

    pub fn config(&self) -> &SessionConfig {
        &self.core.config
    }

    pub fn state(&self) -> SessionState {
        self.core.lock().state
    }

    /// Revisión actual (sube en 1 tras cada mutación con éxito).
    pub fn revision(&self) -> u64 {
        self.core.lock().revision
    }

    /// Último snapshot publicado (revisión 0 y vacío antes del primer refresco).
    pub fn snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&self.core.snapshot_tx.borrow())
    }

    /// Canal con los snapshots publicados. Nunca recibe un estado a medias.
    pub fn subscribe(&self) -> watch::Receiver<Arc<Snapshot>> {
        self.core.snapshot_tx.subscribe()
    }

    /// Canal con los cambios de estado del ciclo de vida.
    pub fn subscribe_state(&self) -> watch::Receiver<SessionState> {
        self.core.state_tx.subscribe()
    }

    /// Eventos de la sesión (con `session_id` y `revision`). Tras `Closed` no llegan más.
    pub fn events(&self) -> broadcast::Receiver<SessionEvent> {
        self.core.events.subscribe()
    }

    /// Token de cancelación **cooperativa** de la sesión: se cancela al empezar `close`.
    /// Pásalo a `GitRunner::with_cancel` para que git termine su árbol de procesos.
    pub fn cancel_token(&self) -> CancellationToken {
        self.core.cancel.clone()
    }

    /// ¿Hay mutaciones en curso o en cola? El llamador debe pedir confirmación antes de `close`.
    pub fn has_pending_work(&self) -> bool {
        self.core
            .lock()
            .ops
            .values()
            .any(|e| e.kind == OpKind::Mutation)
    }

    /// Mutaciones en curso y en cola, en orden de creación (para nombrarlas en el diálogo).
    pub fn pending_operations(&self) -> Vec<PendingOperation> {
        let s = self.core.lock();
        let mut v: Vec<(u64, PendingOperation)> = s
            .ops
            .iter()
            .filter(|(_, e)| e.kind == OpKind::Mutation)
            .map(|(id, e)| {
                (
                    id.get(),
                    PendingOperation {
                        operation_id: *id,
                        label: e.label.clone(),
                        running: e.running,
                    },
                )
            })
            .collect();
        v.sort_by_key(|(k, _)| *k);
        v.into_iter().map(|(_, p)| p).collect()
    }

    /// Cancela una operación (mutación, lectura o petición de refresco) de esta sesión.
    /// En cola: sale de la cola. En curso: se suelta su futuro (git muere con él).
    /// Devuelve `false` si el id no es de una operación viva de esta sesión.
    pub fn cancel_operation(&self, operation_id: OperationId) -> bool {
        let token = self
            .core
            .lock()
            .ops
            .get(&operation_id)
            .map(|e| e.token.clone());
        match token {
            Some(t) => {
                t.cancel();
                true
            }
            None => false,
        }
    }

    /// Encola una **mutación** que se puede repetir: ante `GitError::Locked` se vuelve a
    /// llamar a `op` (3 reintentos con 200/400/800 ms por defecto) y después se devuelve el error.
    /// La mutación entra en la cola **al llamar** (no al primer `poll`), así que el orden de
    /// llamada es el orden de ejecución.
    ///
    /// **El futuro devuelto debe sondearse** (`await` o `tokio::spawn`): una mutación en cola
    /// cuyo futuro nunca se sondea ocupa su turno y bloquea a las demás hasta que se suelte.
    pub fn mutate_with_id<T, F>(
        &self,
        label: &str,
        op: F,
    ) -> (
        OperationId,
        impl Future<Output = GitResult<T>> + Send + 'static,
    )
    where
        T: Send + 'static,
        F: Fn(Arc<dyn GitServing>, Repository) -> OpFuture<T> + Send + Sync + 'static,
    {
        self.mutate_inner(label, true, op)
    }

    /// Como [`mutate_with_id`](Self::mutate_with_id) sin devolver el id.
    pub fn mutate<T, F>(
        &self,
        label: &str,
        op: F,
    ) -> impl Future<Output = GitResult<T>> + Send + 'static
    where
        T: Send + 'static,
        F: Fn(Arc<dyn GitServing>, Repository) -> OpFuture<T> + Send + Sync + 'static,
    {
        self.mutate_with_id(label, op).1
    }

    /// Mutación de un solo uso (`FnOnce`): igual que `mutate` pero **sin reintento** ante
    /// `Locked` (no se puede volver a llamar a la clausura).
    pub fn mutate_once_with_id<T, F>(
        &self,
        label: &str,
        op: F,
    ) -> (
        OperationId,
        impl Future<Output = GitResult<T>> + Send + 'static,
    )
    where
        T: Send + 'static,
        F: FnOnce(Arc<dyn GitServing>, Repository) -> OpFuture<T> + Send + 'static,
    {
        let mut slot = Some(op);
        self.mutate_inner(label, false, move |e, r| match slot.take() {
            Some(f) => f(e, r),
            None => Box::pin(async { Err(GitError::Cancelled) }),
        })
    }

    pub fn mutate_once<T, F>(
        &self,
        label: &str,
        op: F,
    ) -> impl Future<Output = GitResult<T>> + Send + 'static
    where
        T: Send + 'static,
        F: FnOnce(Arc<dyn GitServing>, Repository) -> OpFuture<T> + Send + 'static,
    {
        self.mutate_once_with_id(label, op).1
    }

    fn mutate_inner<T, M>(
        &self,
        label: &str,
        retry: bool,
        make: M,
    ) -> (
        OperationId,
        impl Future<Output = GitResult<T>> + Send + 'static,
    )
    where
        T: Send + 'static,
        M: FnMut(Arc<dyn GitServing>, Repository) -> OpFuture<T> + Send + 'static,
    {
        let core = Arc::clone(&self.core);
        let guard = core.register(label, OpKind::Mutation);
        let id = guard.id;
        let label = label.to_string();
        (id, async move {
            core.run_mutation(guard, label, retry, make).await
        })
    }

    /// Encola una **lectura**: se solapa con otras lecturas, pero espera a cualquier mutación
    /// en curso o en cola. Devuelve el `operation_id` para poder cancelarla.
    pub fn read_with_id<T, F>(
        &self,
        label: &str,
        op: F,
    ) -> (
        OperationId,
        impl Future<Output = GitResult<T>> + Send + 'static,
    )
    where
        T: Send + 'static,
        F: FnOnce(Arc<dyn GitServing>, Repository) -> OpFuture<T> + Send + 'static,
    {
        let core = Arc::clone(&self.core);
        let guard = core.register(label, OpKind::Read);
        let id = guard.id;
        (id, async move {
            let permit = core.acquire_read(&guard.token).await?;
            let r = core
                .drive(
                    &guard.token,
                    op(Arc::clone(&core.engine), core.repo.clone()),
                )
                .await;
            drop(permit);
            drop(guard);
            r
        })
    }

    pub fn read<T, F>(
        &self,
        label: &str,
        op: F,
    ) -> impl Future<Output = GitResult<T>> + Send + 'static
    where
        T: Send + 'static,
        F: FnOnce(Arc<dyn GitServing>, Repository) -> OpFuture<T> + Send + 'static,
    {
        self.read_with_id(label, op).1
    }

    /// Pide un refresco coalescido (máx. uno en curso y uno pendiente por sesión). La petición
    /// espera a la pasada que la cubre. El `OperationId` permite cancelar solo esta petición:
    /// la pasada en curso se aborta únicamente si nadie más la espera.
    pub fn refresh_with_id(
        &self,
    ) -> (
        OperationId,
        impl Future<Output = GitResult<RefreshOutcome>> + Send + 'static,
    ) {
        let core = Arc::clone(&self.core);
        let guard = core.register("refresh", OpKind::Refresh);
        let id = guard.id;
        let (tx, rx) = oneshot::channel();
        core.enqueue_refresh(id, tx);
        (id, async move {
            tokio::select! {
                biased;
                r = rx => r.unwrap_or(Err(GitError::Cancelled)),
                () = guard.token.cancelled() => Err(GitError::Cancelled),
            }
        })
    }

    pub fn refresh(&self) -> impl Future<Output = GitResult<RefreshOutcome>> + Send + 'static {
        self.refresh_with_id().1
    }

    /// Primer refresco: la sesión pasa de `Opening` a `Active` cuando se publica el primer
    /// snapshot (o al completarse la primera mutación con éxito).
    pub async fn open(&self) -> GitResult<RefreshOutcome> {
        self.refresh().await
    }

    /// Cierra la sesión (idempotente). Quien llama ya ha decidido con
    /// [`has_pending_work`](Self::has_pending_work) si hace falta confirmación.
    ///
    /// 1. Pasa a `Closing`; las llamadas nuevas y las mutaciones/lecturas en cola devuelven
    ///    `Cancelled`; se cancela el token cooperativo y el refresco.
    /// 2. Espera hasta `grace` a que terminen las operaciones en curso.
    /// 3. Si siguen, las cancela a la fuerza (se sueltan sus futuros; `Forced`).
    /// 4. Pasa a `Closed`; los eventos posteriores se descartan.
    pub async fn close(&self, grace: Duration) -> CloseOutcome {
        let core = &self.core;
        let previous = {
            let mut s = core.lock();
            let p = s.state;
            if matches!(p, SessionState::Opening | SessionState::Active) {
                s.state = SessionState::Closing;
            }
            p
        };
        match previous {
            SessionState::Closed => return CloseOutcome::AlreadyClosed,
            SessionState::Closing => {
                let mut rx = core.state_tx.subscribe();
                let _ = rx.wait_for(|s| *s == SessionState::Closed).await;
                return CloseOutcome::AlreadyClosed;
            }
            SessionState::Opening | SessionState::Active => {}
        }
        core.state_tx.send_replace(SessionState::Closing);
        core.emit(SessionEventKind::StateChanged {
            state: SessionState::Closing,
        });
        core.cancel.cancel();
        core.abort_refresh();
        core.wake.notify_waiters();

        let mut forced = false;
        if tokio::time::timeout(grace, core.wait_idle()).await.is_err() {
            forced = true;
            core.force.cancel();
            core.wake.notify_waiters();
            core.wait_idle().await;
        }
        core.set_state(SessionState::Closed);
        if forced {
            CloseOutcome::Forced
        } else {
            CloseOutcome::Clean
        }
    }
}
