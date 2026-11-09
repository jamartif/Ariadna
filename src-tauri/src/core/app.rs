//! `AppCore`: pestañas, sesiones, recientes y sesión persistente. Sin dependencia de la
//! ventana de Tauri: los diálogos van por [`Prompter`] y los eventos por [`EventSink`].

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ariadna_git::{
    find_git, recents, repo_key, GitCliEngine, GitInstall, GitRunner, LocatorOptions,
    RecentRepository, RecentStore, RepoKey, RepositorySession, SessionEventKind,
};
use ariadna_process::CancellationToken;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

use super::ai::AiService;
use super::api::{
    ApiSessionEvent, ApiSnapshot, AppEvent, AppState, EventSink, GitInfo, OpenResult,
    SnapshotEvent, TabInfo,
};
use super::error::{ApiError, ApiResult};
use super::prompter::Prompter;
use super::session_file::{SavedTab, SessionData, SessionStore, SESSION_VERSION};

/// Tope blando de pestañas (spec §8): se avisa, no se impide.
pub const SOFT_TAB_LIMIT: usize = 20;
/// Plazo de gracia al cerrar una sesión con operaciones en curso (spec §3.1).
pub const CLOSE_GRACE: Duration = Duration::from_secs(5);

/// Configuración de arranque.
pub struct AppConfig {
    /// Carpeta de datos (`recents.json`, `session.json`).
    pub data_dir: PathBuf,
    /// `git` ya localizado, o el texto del problema (se muestra en `gitProblem`).
    pub git: Result<GitInstall, String>,
    /// `recents.json` de otra instalación (p. ej. el del Swift) que se importa al arrancar.
    pub legacy_recents: Option<PathBuf>,
}

/// Localiza `git` una sola vez (spec §4.1).
pub fn locate_git() -> Result<GitInstall, String> {
    find_git(&LocatorOptions::default()).map_err(|e| e.to_string())
}

/// Carpeta de datos estándar del usuario.
pub fn default_data_dir() -> Result<PathBuf, String> {
    recents::config_dir().map_err(|e| e.to_string())
}

/// Datos estáticos de una pestaña (los comparte con su tarea de reenvío).
#[derive(Debug, Clone)]
pub(crate) struct TabMeta {
    pub id: String,
    pub path: String,
    pub name: String,
}

/// Sesión viva de una pestaña y las tareas que la acompañan.
pub(crate) struct Live {
    pub session: RepositorySession,
    forwarder: JoinHandle<()>,
    bridge: JoinHandle<()>,
}

impl Live {
    pub fn stop_forwarder(&self) {
        self.forwarder.abort();
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.forwarder.abort();
        self.bridge.abort();
    }
}

pub(crate) struct Tab {
    pub meta: TabMeta,
    /// Clave de repo (solo si está vivo).
    pub key: Option<RepoKey>,
    pub live: Option<Live>,
    pub missing: bool,
}

#[derive(Default)]
pub(crate) struct TabsState {
    pub tabs: Vec<Tab>,
    pub active: Option<String>,
}

pub(crate) struct Inner {
    pub git: Option<GitInstall>,
    pub git_problem: Option<String>,
    pub runner: Option<GitRunner>,
    pub recents: RecentStore,
    pub sessions: SessionStore,
    pub prompter: Arc<dyn Prompter>,
    pub sink: Arc<dyn EventSink>,
    pub tabs: Mutex<TabsState>,
    pub open_lock: tokio::sync::Mutex<()>,
    pub(crate) ai: AiService,
    next_tab: AtomicU64,
}

impl Inner {
    pub fn lock(&self) -> MutexGuard<'_, TabsState> {
        self.tabs.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Estado de la aplicación. Barato de clonar (comparte un `Arc`).
#[derive(Clone)]
pub struct AppCore {
    pub(crate) inner: Arc<Inner>,
}

/// ¿Dos rutas son la misma carpeta? (sin separador final; sin mayúsculas en Windows/macOS).
pub(crate) fn same_dir(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        let mut t = s.replace('\\', "/");
        while t.len() > 1 && t.ends_with('/') {
            t.pop();
        }
        if RepoKey::platform_is_case_insensitive() {
            t = t.to_lowercase();
        }
        t
    }
    norm(a) == norm(b)
}

fn display(path: &Path) -> String {
    dunce::simplified(path).to_string_lossy().into_owned()
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| display(path), |n| n.to_string_lossy().into_owned())
}

/// `TabInfo` de una pestaña. `ignore_ops` son operaciones que ya terminaron pero que la
/// sesión aún no ha dado de baja.
pub(crate) fn tab_info(
    meta: &TabMeta,
    session: Option<&RepositorySession>,
    missing: bool,
    ignore_ops: &HashSet<u64>,
) -> TabInfo {
    let mut info = TabInfo {
        tab_id: meta.id.clone(),
        path: meta.path.clone(),
        name: meta.name.clone(),
        branch: None,
        detached: false,
        change_count: 0,
        conflict_count: 0,
        busy: None,
        missing,
    };
    if let Some(s) = session {
        let snap = s.snapshot();
        let st = &snap.status;
        info.branch = st.branch_name.clone();
        info.detached = st.is_detached;
        let count = |f: &dyn Fn(&ariadna_git::StatusEntry) -> bool| {
            u32::try_from(st.entries.iter().filter(|e| f(e)).count()).unwrap_or(u32::MAX)
        };
        info.change_count = count(&|e| !e.is_ignored);
        info.conflict_count = count(&|e| e.is_conflicted);
        let pending: Vec<_> = s
            .pending_operations()
            .into_iter()
            .filter(|p| !ignore_ops.contains(&p.operation_id.get()))
            .collect();
        info.busy = pending
            .iter()
            .find(|p| p.running)
            .or_else(|| pending.first())
            .map(|p| p.label.clone());
    }
    info
}

/// Reenvía snapshots y eventos de una sesión a la UI.
fn spawn_forwarder(
    sink: Arc<dyn EventSink>,
    meta: TabMeta,
    session: RepositorySession,
) -> JoinHandle<()> {
    // Se suscribe aquí, antes de ceder el control: si lo hiciera la tarea al arrancar, una
    // mutación lanzada de inmediato podría emitir su primer evento sin nadie escuchando.
    let mut snaps = session.subscribe();
    let mut events = session.events();
    tokio::spawn(async move {
        let mut finished: HashSet<u64> = HashSet::new();
        loop {
            tokio::select! {
                changed = snaps.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let snapshot = Arc::clone(&snaps.borrow_and_update());
                    sink.emit(AppEvent::Snapshot(SnapshotEvent {
                        tab_id: meta.id.clone(),
                        snapshot: ApiSnapshot(snapshot),
                    }));
                    sink.emit(AppEvent::TabUpdated(tab_info(&meta, Some(&session), false, &finished)));
                }
                ev = events.recv() => match ev {
                    Ok(ev) => {
                        let refresh_tab = match &ev.kind {
                            SessionEventKind::OperationFinished { operation_id, .. } => {
                                finished.insert(operation_id.get());
                                true
                            }
                            SessionEventKind::OperationStarted { .. }
                            | SessionEventKind::StateChanged { .. } => true,
                            _ => false,
                        };
                        sink.emit(AppEvent::Session(ApiSessionEvent::from(&ev)));
                        if refresh_tab {
                            sink.emit(AppEvent::TabUpdated(tab_info(&meta, Some(&session), false, &finished)));
                        }
                        if finished.len() > 256 && session.pending_operations().is_empty() {
                            finished.clear();
                        }
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                },
            }
        }
    })
}

impl AppCore {
    /// Crea el núcleo (sin restaurar la sesión: llama a [`AppCore::restore_session`]).
    pub fn new(config: AppConfig, prompter: Arc<dyn Prompter>, sink: Arc<dyn EventSink>) -> Self {
        let (git, git_problem) = match config.git {
            Ok(g) => (Some(g), None),
            Err(p) => (None, Some(p)),
        };
        let runner = git.as_ref().map(GitRunner::new);
        let recents = RecentStore::with_base_dir(&config.data_dir);
        if let Some(legacy) = &config.legacy_recents {
            if legacy != recents.file() && legacy.is_file() {
                if let Err(e) = recents.import_from(legacy) {
                    eprintln!("no se pudo importar {}: {e}", legacy.display());
                }
            }
        }
        Self {
            inner: Arc::new(Inner {
                git,
                git_problem,
                runner,
                recents,
                sessions: SessionStore::new(config.data_dir.join("session.json")),
                prompter,
                sink,
                tabs: Mutex::new(TabsState::default()),
                open_lock: tokio::sync::Mutex::new(()),
                ai: AiService::new(&config.data_dir),
                next_tab: AtomicU64::new(1),
            }),
        }
    }

    pub(crate) fn runner(&self) -> ApiResult<GitRunner> {
        self.inner.runner.clone().ok_or_else(|| {
            let problem = self.inner.git_problem.clone().unwrap_or_default();
            ApiError::new(
                "GIT_NOT_FOUND",
                format!("No encuentro git. Instálalo o indica su ruta en Ajustes. {problem}")
                    .trim()
                    .to_owned(),
                Some(problem).filter(|p| !p.is_empty()),
            )
        })
    }

    pub(crate) fn emit(&self, e: AppEvent) {
        self.inner.sink.emit(e);
    }

    fn new_tab_id(&self) -> String {
        format!(
            "tab-{}",
            self.inner.next_tab.fetch_add(1, Ordering::Relaxed)
        )
    }

    // --- estado ----------------------------------------------------------------------------

    fn recents_list(&self) -> Vec<RecentRepository> {
        self.inner.recents.load().unwrap_or_default()
    }

    /// Estado completo para la UI.
    pub fn app_state(&self) -> AppState {
        let tabs = {
            let st = self.inner.lock();
            let none = HashSet::new();
            let infos: Vec<TabInfo> = st
                .tabs
                .iter()
                .map(|t| {
                    tab_info(
                        &t.meta,
                        t.live.as_ref().map(|l| &l.session),
                        t.missing,
                        &none,
                    )
                })
                .collect();
            (infos, st.active.clone())
        };
        AppState {
            tabs: tabs.0,
            active_tab_id: tabs.1,
            recents: self.recents_list(),
            git: self.inner.git.as_ref().map(|g| GitInfo {
                path: display(&g.path),
                version: g.version.to_string(),
            }),
            git_problem: self.inner.git_problem.clone(),
        }
    }

    pub(crate) fn emit_tabs_changed(&self) {
        self.emit(AppEvent::TabsChanged(self.app_state()));
    }

    /// Guarda `session.json` con las pestañas actuales. Un fallo se registra, no rompe nada.
    pub(crate) fn persist(&self) {
        let data = {
            let st = self.inner.lock();
            SessionData {
                version: SESSION_VERSION,
                tabs: st
                    .tabs
                    .iter()
                    .map(|t| SavedTab {
                        path: t.meta.path.clone(),
                    })
                    .collect(),
                active: st
                    .active
                    .as_ref()
                    .and_then(|a| st.tabs.iter().position(|t| &t.meta.id == a)),
            }
        };
        if let Err(e) = self.inner.sessions.save(&data) {
            eprintln!("no se pudo guardar session.json: {e}");
        }
    }

    // --- sesiones --------------------------------------------------------------------------

    /// Crea la sesión viva de un repo: runner con la cancelación de la sesión, puente de
    /// cancelación y (tras el primer refresco) reenvío de eventos.
    async fn make_live(
        &self,
        runner: &GitRunner,
        repo: ariadna_git::Repository,
        meta: &TabMeta,
    ) -> ApiResult<Live> {
        // La sesión crea su token al construirse, pero el motor hay que dárselo antes: el
        // puente cancela este token cuando la sesión cancela el suyo (cierre de pestaña).
        let runner_cancel = CancellationToken::new();
        let engine = Arc::new(GitCliEngine::new(
            runner.clone().with_cancel(runner_cancel.clone()),
        ));
        let session = RepositorySession::new(engine, repo, meta.id.clone());
        let session_cancel = session.cancel_token();
        let bridge = tokio::spawn(async move {
            session_cancel.cancelled().await;
            runner_cancel.cancel();
        });
        if let Err(e) = session.open().await {
            bridge.abort();
            return Err(e.into());
        }
        let forwarder =
            spawn_forwarder(Arc::clone(&self.inner.sink), meta.clone(), session.clone());
        Ok(Live {
            session,
            forwarder,
            bridge,
        })
    }

    /// Abre un repo (o su carpeta de trabajo, o una subcarpeta) en una pestaña nueva; si ya
    /// está abierto (misma clave de repo), activa la existente.
    pub async fn open_repository(&self, path: &str) -> ApiResult<OpenResult> {
        let _guard = self.inner.open_lock.lock().await;
        let runner = self.runner()?;
        let requested = PathBuf::from(path);
        let repo = GitCliEngine::new(runner.clone())
            .open_repository(&requested)
            .await
            .map_err(ApiError::from)?;
        let key = repo_key(&runner, &repo.path)
            .await
            .map_err(ApiError::from)?;
        let root = display(&repo.path);

        // ¿Ya abierto? Por clave (cubre worktrees y rutas equivalentes).
        let existing = {
            let st = self.inner.lock();
            st.tabs
                .iter()
                .find(|t| t.key.as_ref() == Some(&key))
                .map(|t| (t.meta.id.clone(), t.meta.path.clone()))
        };
        if let Some((id, existing_path)) = existing {
            self.activate_inner(&id);
            let tab = self
                .tab_info_of(&id)
                .ok_or_else(|| ApiError::unknown_tab(&id))?;
            let warning = (!same_dir(&existing_path, &root)).then(|| {
                format!(
                    "{root} es un worktree de un repositorio que ya está abierto en {existing_path}. Se muestra esa pestaña."
                )
            });
            return Ok(OpenResult {
                tab,
                already_open: true,
                warning,
            });
        }

        let id = self.new_tab_id();
        let meta = TabMeta {
            id: id.clone(),
            path: root.clone(),
            name: repo.name(),
        };
        let live = self.make_live(&runner, repo, &meta).await?;
        let branch = live.session.snapshot().status.branch_name.clone();
        if let Err(e) =
            self.inner
                .recents
                .remember(Path::new(&root), branch.as_deref(), recents::now_unix())
        {
            eprintln!("no se pudo actualizar recientes: {e}");
        }

        let warning = {
            let mut st = self.inner.lock();
            let count = st.tabs.len();
            let new_tab = Tab {
                meta,
                key: Some(key),
                live: Some(live),
                missing: false,
            };
            // Una pestaña `missing` con la misma ruta se sustituye en su sitio (Reintentar).
            if let Some(pos) = st
                .tabs
                .iter()
                .position(|t| t.missing && same_dir(&t.meta.path, &root))
            {
                st.tabs[pos] = new_tab;
            } else {
                st.tabs.push(new_tab);
            }
            st.active = Some(id.clone());
            (count >= SOFT_TAB_LIMIT).then(|| {
                format!(
                    "Tienes {} pestañas abiertas. Más de {SOFT_TAB_LIMIT} puede ir lento: cierra las que no uses.",
                    count + 1
                )
            })
        };
        self.persist();
        self.emit_tabs_changed();
        let tab = self
            .tab_info_of(&id)
            .ok_or_else(|| ApiError::unknown_tab(&id))?;
        Ok(OpenResult {
            tab,
            already_open: false,
            warning,
        })
    }

    pub(crate) fn tab_info_of(&self, id: &str) -> Option<TabInfo> {
        let st = self.inner.lock();
        let none = HashSet::new();
        st.tabs.iter().find(|t| t.meta.id == id).map(|t| {
            tab_info(
                &t.meta,
                t.live.as_ref().map(|l| &l.session),
                t.missing,
                &none,
            )
        })
    }

    /// Activa la pestaña (si existe) y devuelve su sesión para refrescarla.
    fn activate_inner(&self, id: &str) -> Option<RepositorySession> {
        let mut st = self.inner.lock();
        let tab = st.tabs.iter().find(|t| t.meta.id == id)?;
        let session = tab.live.as_ref().map(|l| l.session.clone());
        st.active = Some(id.to_owned());
        session
    }

    pub async fn pick_folder(&self) -> Option<String> {
        self.inner.prompter.pick_folder().await.map(|p| display(&p))
    }

    pub fn activate_tab(&self, tab_id: &str) -> ApiResult<()> {
        let session = {
            let exists = self.inner.lock().tabs.iter().any(|t| t.meta.id == tab_id);
            if !exists {
                return Err(ApiError::unknown_tab(tab_id));
            }
            self.activate_inner(tab_id)
        };
        self.persist();
        self.emit_tabs_changed();
        // Solo la pestaña visible vigila a fondo: al activarla se pone al día (spec §8).
        if let Some(s) = session {
            tokio::spawn(async move {
                let _ = s.refresh().await;
            });
        }
        Ok(())
    }

    /// Reordena. Los ids desconocidos se ignoran y los que no vengan se quedan al final.
    pub fn reorder_tabs(&self, tab_ids: &[String]) -> ApiResult<()> {
        {
            let mut st = self.inner.lock();
            let mut old = std::mem::take(&mut st.tabs);
            let mut ordered = Vec::with_capacity(old.len());
            for id in tab_ids {
                if let Some(pos) = old.iter().position(|t| &t.meta.id == id) {
                    ordered.push(old.remove(pos));
                }
            }
            ordered.append(&mut old);
            st.tabs = ordered;
        }
        self.persist();
        self.emit_tabs_changed();
        Ok(())
    }

    /// Cierra la pestaña. Con trabajo pendiente pregunta; si el usuario cancela no cierra.
    pub async fn close_tab(&self, tab_id: &str) -> ApiResult<bool> {
        let pending = {
            let st = self.inner.lock();
            let tab = st
                .tabs
                .iter()
                .find(|t| t.meta.id == tab_id)
                .ok_or_else(|| ApiError::unknown_tab(tab_id))?;
            tab.live.as_ref().and_then(|l| {
                l.session.has_pending_work().then(|| {
                    let ops = l
                        .session
                        .pending_operations()
                        .into_iter()
                        .map(|p| p.label)
                        .collect::<Vec<_>>()
                        .join(", ");
                    (tab.meta.name.clone(), tab.meta.path.clone(), ops)
                })
            })
        };
        if let Some((name, path, ops)) = pending {
            let ok = self
                .inner
                .prompter
                .confirm(
                    "Cerrar pestaña con trabajo pendiente",
                    &format!("«{name}» tiene operaciones en curso o en cola: {ops}. ¿Cerrar la pestaña y cancelarlas?"),
                    &path,
                )
                .await;
            if !ok {
                return Ok(false);
            }
        }
        let removed = {
            let mut st = self.inner.lock();
            let Some(pos) = st.tabs.iter().position(|t| t.meta.id == tab_id) else {
                return Ok(true);
            };
            let tab = st.tabs.remove(pos);
            if st.active.as_deref() == Some(tab_id) {
                st.active = st
                    .tabs
                    .get(pos)
                    .or_else(|| pos.checked_sub(1).and_then(|p| st.tabs.get(p)))
                    .map(|t| t.meta.id.clone());
            }
            tab
        };
        // Cerrar la pestaña cancela su generación de IA en curso (RF-IA8).
        self.ai_cancel_tab_silently(tab_id);
        self.close_live(removed).await;
        self.persist();
        self.emit_tabs_changed();
        Ok(true)
    }

    async fn close_live(&self, tab: Tab) {
        if let Some(live) = &tab.live {
            live.stop_forwarder();
            live.session.close(CLOSE_GRACE).await;
        }
        drop(tab);
    }

    /// Salir de la app (o cerrar la ventana): mismo camino que cerrar pestañas. Devuelve
    /// `false` si el usuario cancela por haber trabajo pendiente.
    pub async fn shutdown(&self) -> bool {
        let busy: Vec<String> = {
            let st = self.inner.lock();
            st.tabs
                .iter()
                .filter_map(|t| {
                    let l = t.live.as_ref()?;
                    l.session.has_pending_work().then(|| {
                        let ops = l
                            .session
                            .pending_operations()
                            .into_iter()
                            .map(|p| p.label)
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{}: {ops}", t.meta.name)
                    })
                })
                .collect()
        };
        if !busy.is_empty() {
            let ok = self
                .inner
                .prompter
                .confirm(
                    "Salir con trabajo pendiente",
                    "Hay operaciones en curso o en cola. ¿Salir y cancelarlas?",
                    &busy.join("\n"),
                )
                .await;
            if !ok {
                return false;
            }
        }
        // La sesión guardada conserva todas las pestañas: se guarda antes de cerrar.
        self.persist();
        self.ai_cancel_all();
        let tabs: Vec<Tab> = std::mem::take(&mut self.inner.lock().tabs);
        for t in tabs {
            self.close_live(t).await;
        }
        true
    }

    // --- recientes -------------------------------------------------------------------------

    pub fn toggle_favorite(&self, path: &str) -> ApiResult<Vec<RecentRepository>> {
        let store = &self.inner.recents;
        let items = store.load().map_err(|e| ApiError::io(e.to_string()))?;
        let current = items
            .iter()
            .find(|i| same_dir(&i.path, path))
            .map(|i| i.is_favorite);
        let result = match current {
            Some(fav) => store.set_favorite(path, !fav),
            None => store
                .remember(Path::new(path), None, recents::now_unix())
                .and_then(|()| store.set_favorite(path, true)),
        };
        result.map_err(|e| ApiError::io(e.to_string()))?;
        Ok(self.recents_list())
    }

    pub fn forget_recent(&self, path: &str) -> ApiResult<Vec<RecentRepository>> {
        self.inner
            .recents
            .remove(path)
            .map_err(|e| ApiError::io(e.to_string()))?;
        Ok(self.recents_list())
    }

    // --- restauración ----------------------------------------------------------------------

    /// Restaura `session.json`: abre cada ruta; las que ya no existen (o ya no son repo)
    /// quedan como pestañas `missing: true`. Dos rutas del mismo repo dan una sola pestaña.
    pub async fn restore_session(&self) {
        let (data, _outcome) = self.inner.sessions.load();
        let _guard = self.inner.open_lock.lock().await;
        let mut active_id: Option<String> = None;
        for (i, saved) in data.tabs.iter().enumerate() {
            let Some(tab) = self.restore_tab(&saved.path).await else {
                continue;
            };
            if data.active == Some(i) {
                active_id = Some(tab.meta.id.clone());
            }
            self.inner.lock().tabs.push(tab);
        }
        {
            let mut st = self.inner.lock();
            st.active = active_id.or_else(|| st.tabs.first().map(|t| t.meta.id.clone()));
        }
        self.emit_tabs_changed();
    }

    async fn restore_tab(&self, path: &str) -> Option<Tab> {
        let missing_tab = |path: &str| Tab {
            meta: TabMeta {
                id: self.new_tab_id(),
                path: path.to_owned(),
                name: name_of(Path::new(path)),
            },
            key: None,
            live: None,
            missing: true,
        };
        if !Path::new(path).is_dir() {
            return Some(missing_tab(path));
        }
        let Ok(runner) = self.runner() else {
            // Sin git no se puede abrir nada, pero la pestaña no se pierde de la sesión.
            let mut t = missing_tab(path);
            t.missing = false;
            return Some(t);
        };
        let Ok(repo) = GitCliEngine::new(runner.clone())
            .open_repository(Path::new(path))
            .await
        else {
            return Some(missing_tab(path));
        };
        let Ok(key) = repo_key(&runner, &repo.path).await else {
            return Some(missing_tab(path));
        };
        {
            let st = self.inner.lock();
            if st.tabs.iter().any(|t| t.key.as_ref() == Some(&key)) {
                return None;
            }
        }
        let meta = TabMeta {
            id: self.new_tab_id(),
            path: display(&repo.path),
            name: repo.name(),
        };
        // La primera lectura ya la hace `make_live`; si falla, la pestaña queda como ausente.
        match self.make_live(&runner, repo, &meta).await {
            Ok(live) => Some(Tab {
                meta,
                key: Some(key),
                live: Some(live),
                missing: false,
            }),
            Err(_) => Some(missing_tab(path)),
        }
    }
}
