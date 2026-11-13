//! IA en el núcleo (spec §5, `specs/rust/ipc.md` «Comandos de IA»): ajustes persistidos,
//! contexto a partir del diff PREPARADO, consentimiento por perfil y proveedor, selección
//! de proveedor y una sola generación por pestaña, cancelable de verdad.
//!
//! La IA solo propone: nada de este módulo escribe en el repo. **Nunca se registra el prompt
//! ni el diff**: los errores y los eventos solo llevan códigos, tamaños y nombres.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ariadna_ai::env::{isolation, shares_login_between_profiles};
use ariadna_ai::login::{write_script, LoginPlan, OsTerminal, Shell, TerminalLauncher};
use ariadna_ai::orchestrator::{Limiter, Orchestrator, Profile, ProfileKind};
use ariadna_ai::prompt::{
    is_sensitive_path, redact_secrets, validate_conflict_proposal, ConflictInput, PromptBuilder,
    PromptPreview, PullRequestInput,
};
use ariadna_ai::provider::DEFAULT_TIMEOUT;
use ariadna_ai::resolve::{resolve_binary, SearchEnv};
use ariadna_ai::{
    AiContextPolicy, AiError, AiProvider, AiSettings, Availability, CallContext, CliProvider,
    CliProviderConfig, CommitLanguage, CommitOptions, ProviderChoice, ProviderId,
};
use ariadna_git::{
    CommandKind, ConflictFile, ConflictKind, ConflictSide, DiffScope, FileDiff, GitPath,
};
use ariadna_process::CancellationToken;
use serde::{Deserialize, Serialize};

use super::api::{
    AiAccounts, AiAccountsRequest, AiCommitOptions, AiCommitSuggestion, AiConflictSuggestion,
    AiContextPreview, AiExcludeReason, AiExcludedFile, AiLoginInfo, AiLoginRequest, AiPreviewFile,
    AiProfileInfo, AiProviderState, AiPullRequestOptions, AiPullRequestSuggestion, AiSettingsPatch,
    AiState, ApiSessionEvent, ApiSessionEventKind, AppEvent,
};
use super::app::AppCore;
use super::error::{ApiError, ApiResult};
use super::settings::RuleMatch;

/// Versión actual de `ai-settings.json`.
pub const AI_SETTINGS_VERSION: u32 = 1;
/// Asuntos recientes que se envían como pista de idioma y estilo (`language = auto`).
pub const RECENT_SUBJECTS: usize = 20;
/// Los ids de operación de la IA van muy por encima de los de la sesión para no chocar.
const AI_OP_BASE: u64 = 1 << 40;
/// Archivos que se nombran en el diálogo de consentimiento.
const SHOWN_FILES: usize = 12;

// ------------------------------------------------------------------ ajustes persistidos

/// Contenido de `ai-settings.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StoredAi {
    pub version: u32,
    /// Interruptor global, elección de proveedor, tope de contexto y rutas de los CLI.
    pub settings: AiSettings,
    pub active_profile: ProfileKind,
    pub personal: Profile,
    pub empresa: Profile,
}

impl Default for StoredAi {
    fn default() -> Self {
        Self {
            version: AI_SETTINGS_VERSION,
            settings: AiSettings::default(),
            active_profile: ProfileKind::Personal,
            personal: Profile::new("Personal", ProfileKind::Personal),
            empresa: Profile::new("Empresa", ProfileKind::Empresa),
        }
    }
}

impl StoredAi {
    pub fn profile(&self) -> &Profile {
        match self.active_profile {
            ProfileKind::Personal => &self.personal,
            ProfileKind::Empresa => &self.empresa,
        }
    }

    fn profile_of(&self, kind: ProfileKind) -> &Profile {
        match kind {
            ProfileKind::Personal => &self.personal,
            ProfileKind::Empresa => &self.empresa,
        }
    }

    fn profile_mut(&mut self) -> &mut Profile {
        match self.active_profile {
            ProfileKind::Personal => &mut self.personal,
            ProfileKind::Empresa => &mut self.empresa,
        }
    }

    /// Perfil activo con el tope de contexto global aplicado.
    fn call_profile(&self) -> Profile {
        let mut p = self.profile().clone();
        p.max_context_bytes = self.settings.max_context_bytes;
        p
    }

    fn binary(&self, id: ProviderId) -> Option<PathBuf> {
        self.settings.binary_for(id).map(Path::to_path_buf)
    }
}

/// Cómo se obtuvieron los ajustes al cargar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiSettingsLoad {
    Missing,
    Ok,
    RecoveredFromBackup,
    ResetCorrupt,
}

enum Read {
    Missing,
    Valid(Box<StoredAi>),
    Corrupt,
}

fn sibling(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    file.with_file_name(name)
}

fn read(file: &Path) -> Read {
    let bytes = match fs::read(file) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Read::Missing,
        Err(_) => return Read::Corrupt,
    };
    match serde_json::from_slice::<StoredAi>(&bytes) {
        Ok(d) if d.version == AI_SETTINGS_VERSION => Read::Valid(Box::new(d)),
        _ => Read::Corrupt,
    }
}

/// `ai-settings.json`: escritura atómica (tmp + rename), `.bak` del último contenido válido
/// y recuperación si está corrupto (el corrupto se aparta como `.corrupt`, no se borra).
#[derive(Debug, Clone)]
pub struct AiStore {
    file: PathBuf,
}

impl AiStore {
    pub fn new(file: impl Into<PathBuf>) -> Self {
        Self { file: file.into() }
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    pub fn load(&self) -> (StoredAi, AiSettingsLoad) {
        let backup = sibling(&self.file, ".bak");
        match read(&self.file) {
            Read::Valid(d) => (*d, AiSettingsLoad::Ok),
            Read::Missing => match read(&backup) {
                Read::Valid(d) => (*d, AiSettingsLoad::RecoveredFromBackup),
                _ => (StoredAi::default(), AiSettingsLoad::Missing),
            },
            Read::Corrupt => {
                let _ = fs::rename(&self.file, sibling(&self.file, ".corrupt"));
                match read(&backup) {
                    Read::Valid(d) => {
                        let _ = self.save(&d);
                        (*d, AiSettingsLoad::RecoveredFromBackup)
                    }
                    _ => (StoredAi::default(), AiSettingsLoad::ResetCorrupt),
                }
            }
        }
    }

    pub fn save(&self, data: &StoredAi) -> std::io::Result<()> {
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut json = serde_json::to_vec_pretty(data)?;
        json.push(b'\n');
        let tmp = sibling(&self.file, ".tmp");
        {
            let mut h = fs::File::create(&tmp)?;
            h.write_all(&json)?;
            h.sync_all()?;
        }
        // Respaldo solo de lo válido: no se pisa una copia buena con una mala.
        if matches!(read(&self.file), Read::Valid(_)) {
            fs::copy(&self.file, sibling(&self.file, ".bak"))?;
        }
        fs::rename(&tmp, &self.file).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    }
}

// ------------------------------------------------------------------ servicio

/// Generación en curso de una pestaña.
struct Running {
    gen: u64,
    op_id: u64,
    token: CancellationToken,
}

/// Una generación registrada. Al soltarla se da de baja (también si la tarea se aborta).
pub(crate) struct Run {
    svc: Arc<AiShared>,
    tab_id: String,
    gen: u64,
    pub op_id: u64,
    pub token: CancellationToken,
}

impl Drop for Run {
    fn drop(&mut self) {
        self.svc.finish(&self.tab_id, self.gen);
    }
}

pub(crate) struct AiShared {
    store: AiStore,
    state: Mutex<StoredAi>,
    limiter: Arc<Limiter>,
    sandbox: PathBuf,
    /// `…/ai-profiles/<perfil>/<proveedor>`: un directorio de configuración por perfil y CLI.
    profiles_dir: PathBuf,
    launcher: Mutex<Arc<dyn TerminalLauncher>>,
    search: Mutex<SearchEnv>,
    timeout: Mutex<Duration>,
    running: Mutex<HashMap<String, Running>>,
    next_run: AtomicU64,
    consent_lock: tokio::sync::Mutex<()>,
}

/// Estado de IA de la app (cuelga de `Inner`).
#[derive(Clone)]
pub(crate) struct AiService {
    shared: Arc<AiShared>,
    /// Cómo se cargaron los ajustes (para diagnóstico y tests).
    #[allow(dead_code)]
    pub(crate) load: AiSettingsLoad,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl AiService {
    pub fn new(data_dir: &Path) -> Self {
        let store = AiStore::new(data_dir.join("ai-settings.json"));
        let (state, load) = store.load();
        Self {
            shared: Arc::new(AiShared {
                store,
                state: Mutex::new(state),
                limiter: Arc::new(Limiter::default()),
                sandbox: data_dir.join("ai-sandbox"),
                profiles_dir: data_dir.join("ai-profiles"),
                launcher: Mutex::new(Arc::new(OsTerminal)),
                search: Mutex::new(SearchEnv::from_process()),
                timeout: Mutex::new(DEFAULT_TIMEOUT),
                running: Mutex::new(HashMap::new()),
                next_run: AtomicU64::new(1),
                consent_lock: tokio::sync::Mutex::new(()),
            }),
            load,
        }
    }

    /// Copia de los ajustes actuales.
    pub fn snapshot(&self) -> StoredAi {
        lock(&self.shared.state).clone()
    }

    fn search(&self) -> SearchEnv {
        lock(&self.shared.search).clone()
    }

    fn timeout(&self) -> Duration {
        *lock(&self.shared.timeout)
    }

    /// Dónde se buscan los CLI (los tests lo vacían para no encontrar un `claude` real).
    #[cfg(test)]
    pub fn set_search_env(&self, env: SearchEnv) {
        *lock(&self.shared.search) = env;
    }

    /// Quién abre la terminal del login (los tests ponen uno que no abre nada).
    #[cfg(test)]
    pub fn set_launcher(&self, l: Arc<dyn TerminalLauncher>) {
        *lock(&self.shared.launcher) = l;
    }

    #[cfg(test)]
    pub fn profiles_dir(&self) -> PathBuf {
        self.shared.profiles_dir.clone()
    }

    #[cfg(test)]
    pub fn set_timeout(&self, t: Duration) {
        *lock(&self.shared.timeout) = t;
    }

    #[cfg(test)]
    pub fn settings_file(&self) -> PathBuf {
        self.shared.store.file().to_path_buf()
    }

    #[cfg(test)]
    pub fn is_running(&self, tab_id: &str) -> bool {
        lock(&self.shared.running).contains_key(tab_id)
    }

    /// Aplica un cambio y lo guarda; si no se puede guardar, no cambia nada.
    pub fn update(&self, patch: &AiSettingsPatch) -> ApiResult<()> {
        let mut guard = lock(&self.shared.state);
        let mut next = guard.clone();
        if let Some(e) = patch.enabled {
            next.settings.enabled = e;
        }
        if let Some(c) = &patch.choice {
            next.settings.provider = parse_choice(c)?;
        }
        if let Some(p) = &patch.profile {
            next.active_profile = parse_profile(p)?;
        }
        if let Some(pe) = &patch.provider_enabled {
            let id = parse_provider(&pe.id)?;
            next.profile_mut().set_enabled(id, pe.enabled);
        }
        if let Some(iso) = patch.isolated {
            next.profile_mut().isolated = Some(iso);
        }
        if let Some(b) = &patch.binary_path {
            let id = parse_provider(&b.id)?;
            let path = b
                .path
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(PathBuf::from);
            match id {
                ProviderId::Claude => next.settings.claude_binary = path,
                ProviderId::Codex => next.settings.codex_binary = path,
                ProviderId::Copilot => next.settings.copilot_binary = path,
            }
        }
        if next == *guard {
            return Ok(());
        }
        self.shared
            .store
            .save(&next)
            .map_err(|e| ApiError::io(format!("no se pudo guardar ai-settings.json: {e}")))?;
        *guard = next;
        Ok(())
    }

    /// Copia de los ajustes con el perfil de la regla repo → perfil (si la hay) como activo.
    fn snapshot_with(&self, rule: Option<&RuleMatch>) -> StoredAi {
        let mut st = self.snapshot();
        if let Some(r) = rule {
            st.active_profile = r.kind;
        }
        st
    }

    /// El consentimiento se guarda en el perfil con el que se pidió (no en el «activo»).
    fn grant_consent(&self, kind: ProfileKind, id: ProviderId) {
        let mut guard = lock(&self.shared.state);
        let mut next = guard.clone();
        match kind {
            ProfileKind::Personal => next.personal.grant_consent(id),
            ProfileKind::Empresa => next.empresa.grant_consent(id),
        }
        if let Err(e) = self.shared.store.save(&next) {
            // La decisión vale para esta sesión; se vuelve a preguntar en la próxima.
            eprintln!("no se pudo guardar el consentimiento de IA: {e}");
        }
        *guard = next;
    }

    /// Registra una generación de la pestaña. Si ya había una, se cancela: solo corre una
    /// por pestaña y la nueva toma el relevo (la anterior termina con `CANCELLED`).
    pub fn begin(&self, tab_id: &str) -> Run {
        let gen = self.shared.next_run.fetch_add(1, Ordering::Relaxed);
        let token = CancellationToken::new();
        let mut running = lock(&self.shared.running);
        if let Some(old) = running.insert(
            tab_id.to_owned(),
            Running {
                gen,
                op_id: AI_OP_BASE + gen,
                token: token.clone(),
            },
        ) {
            old.token.cancel();
        }
        Run {
            svc: Arc::clone(&self.shared),
            tab_id: tab_id.to_owned(),
            gen,
            op_id: AI_OP_BASE + gen,
            token,
        }
    }

    /// Cancela la generación de la pestaña. `true` si había una.
    pub fn cancel_tab(&self, tab_id: &str) -> bool {
        match lock(&self.shared.running).get(tab_id) {
            Some(r) => {
                r.token.cancel();
                true
            }
            None => false,
        }
    }

    /// Cancela por id de operación (el de `session-event`). `true` si era de la IA.
    pub fn cancel_op(&self, tab_id: &str, op_id: u64) -> bool {
        match lock(&self.shared.running).get(tab_id) {
            Some(r) if r.op_id == op_id => {
                r.token.cancel();
                true
            }
            _ => false,
        }
    }

    /// Cancela todas (salir de la aplicación).
    pub fn cancel_all(&self) {
        for r in lock(&self.shared.running).values() {
            r.token.cancel();
        }
    }

    /// Directorio de configuración aislado de `kind` para `id`, si el perfil aísla y el CLI lo
    /// permite. No lo crea. Copilot nunca tiene (no hay mecanismo verificado): comparte login.
    pub(crate) fn config_dir(
        &self,
        st: &StoredAi,
        kind: ProfileKind,
        id: ProviderId,
    ) -> Option<PathBuf> {
        if !st.profile_of(kind).is_isolated() || isolation(id).var.is_none() {
            return None;
        }
        Some(
            self.shared
                .profiles_dir
                .join(kind.dir_name())
                .join(id.binary_name()),
        )
    }

    /// Como [`config_dir`](Self::config_dir) pero crea el directorio.
    fn ensure_config_dir(
        &self,
        st: &StoredAi,
        kind: ProfileKind,
        id: ProviderId,
    ) -> Option<PathBuf> {
        let dir = self.config_dir(st, kind, id)?;
        match fs::create_dir_all(&dir) {
            Ok(()) => Some(dir),
            Err(e) => {
                eprintln!("no se pudo crear el directorio de IA del perfil: {e}");
                None
            }
        }
    }

    /// Perfil de las llamadas: tope de contexto global y directorios aislados por proveedor.
    fn call_profile(&self, st: &StoredAi) -> Profile {
        let mut p = st.call_profile();
        p.config_dirs.clear();
        for id in ProviderId::AUTO_ORDER {
            if let Some(dir) = self.ensure_config_dir(st, p.kind, id) {
                p.config_dirs.insert(id, dir);
            }
        }
        p
    }

    fn provider(&self, id: ProviderId, st: &StoredAi) -> CliProvider {
        let mut cfg = CliProviderConfig::new(id, &self.shared.sandbox);
        cfg.configured_binary = st.binary(id);
        cfg.search = self.search();
        cfg.timeout = self.timeout();
        cfg.policy = AiContextPolicy::new(st.settings.max_context_bytes);
        CliProvider::new(cfg)
    }

    fn installed(&self, id: ProviderId, st: &StoredAi) -> bool {
        resolve_binary(id, st.binary(id).as_deref(), &self.search()).is_some()
    }

    /// Proveedor que se usaría ahora, sin preguntar nada (vista previa).
    fn likely_provider(&self, st: &StoredAi) -> Option<ProviderId> {
        let profile = st.profile();
        let candidates: Vec<ProviderId> = match st.settings.provider.provider() {
            Some(p) => vec![p],
            None => ProviderId::AUTO_ORDER.to_vec(),
        };
        let usable = |id: &ProviderId| profile.is_enabled(*id) && self.installed(*id, st);
        candidates
            .iter()
            .copied()
            .find(|id| usable(id) && profile.has_consent(*id))
            .or_else(|| candidates.iter().copied().find(usable))
    }

    /// Decide con qué proveedor ir y si falta el consentimiento (RF-IA9).
    fn plan(&self, st: &StoredAi) -> Result<Plan, ApiError> {
        let profile = st.profile();
        match st.settings.provider.provider() {
            Some(p) => {
                profile.check_usable(p).or_else(|e| match e {
                    AiError::ConsentRequired { .. } => Ok(()),
                    other => Err(other),
                })?;
                if !self.installed(p, st) {
                    return Err(not_installed(p).into());
                }
                Ok(if profile.has_consent(p) {
                    Plan::Run
                } else {
                    Plan::NeedConsent(p)
                })
            }
            None => {
                let enabled: Vec<ProviderId> = ProviderId::AUTO_ORDER
                    .into_iter()
                    .filter(|p| profile.is_enabled(*p))
                    .collect();
                if enabled.is_empty() {
                    return Err(ApiError::new(
                        "AI_DISABLED",
                        format!(
                            "Ningún proveedor de IA está habilitado en el perfil {}.",
                            profile.name
                        ),
                        Some(profile.name.clone()),
                    ));
                }
                if enabled
                    .iter()
                    .any(|p| profile.has_consent(*p) && self.installed(*p, st))
                {
                    return Ok(Plan::Run);
                }
                match enabled.iter().find(|p| self.installed(**p, st)) {
                    Some(p) => Ok(Plan::NeedConsent(*p)),
                    None => Err(AiError::NoProviderAvailable {
                        hints: enabled
                            .iter()
                            .map(|p| format!("{}: no está instalado.", p.display_name()))
                            .collect(),
                    }
                    .into()),
                }
            }
        }
    }
}

impl AiShared {
    fn finish(&self, tab_id: &str, gen: u64) {
        let mut running = lock(&self.running);
        if running.get(tab_id).is_some_and(|r| r.gen == gen) {
            running.remove(tab_id);
        }
    }
}

enum Plan {
    Run,
    NeedConsent(ProviderId),
}

/// Para qué se pide el consentimiento (cambia el texto del diálogo nativo).
#[derive(Clone, Copy)]
enum Purpose {
    Commit,
    PullRequest,
    Conflict,
}

impl Purpose {
    /// (qué se envía, para qué, qué garantía se da). Sin nombres de archivo: eso va aparte.
    fn describe(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Commit => (
                "el diff PREPARADO de este repositorio",
                "proponer un mensaje de commit",
                "La IA solo propone: nunca hace commit ni toca el repositorio.",
            ),
            Self::PullRequest => (
                "los commits y el diff de la rama actual contra la rama base",
                "proponer el título y la descripción de una pull request",
                "La IA solo propone: nunca hace commit, no crea la pull request ni toca el repositorio.",
            ),
            Self::Conflict => (
                "las tres versiones (base, nuestra y suya) del archivo en conflicto",
                "proponer el archivo completo ya resuelto",
                "La IA solo propone: no escribe el archivo ni lo marca como resuelto.",
            ),
        }
    }
}

/// Lo que necesita el diálogo de consentimiento: qué archivos, cuánto contexto y para qué.
struct Consent<'a> {
    files: &'a [AiPreviewFile],
    /// Archivos que se quedan fuera (sensibles, binarios, demasiado grandes).
    excluded: usize,
    context_bytes: usize,
    purpose: Purpose,
}

/// Datos de una rama frente a su base, tal como los leyó git.
enum PrRaw {
    /// No hay rama base utilizable (no existe, es la propia rama o no hay ancestro común).
    NoBase,
    Found {
        base: String,
        head: String,
        commits: Vec<String>,
        commit_count: usize,
        diffs: Vec<FileDiff>,
    },
}

/// Commits que se envían como máximo (los más recientes de la rama).
const PR_MAX_COMMITS: usize = 50;

/// Nombre de rama o referencia aceptable como base. Lo que no pase no llega a git.
fn valid_base_ref(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 200
        && !s.starts_with('-')
        && !s.starts_with('/')
        && !s.ends_with('/')
        && !s.ends_with('.')
        && !s.contains("..")
        && s.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '@' | '+'))
}

async fn git_text(runner: &ariadna_git::GitRunner, cwd: &Path, args: &[&str]) -> Option<String> {
    let args: Vec<std::ffi::OsString> = args.iter().map(std::ffi::OsString::from).collect();
    let out = runner
        .run(cwd, CommandKind::Read, &args, None)
        .await
        .ok()
        .filter(|o| o.success())?;
    let text = out.stdout_lossy().trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// Rama base: la pedida o, si no, la que sugiere `origin/HEAD` y las habituales. Devuelve
/// el nombre y su SHA (los comandos siguientes usan solo SHAs).
async fn resolve_base(
    runner: &ariadna_git::GitRunner,
    cwd: &Path,
    explicit: Option<&str>,
    head_branch: Option<&str>,
) -> Option<(String, String)> {
    let mut candidates: Vec<String> = Vec::new();
    match explicit {
        Some(b) => candidates.push(b.to_owned()),
        None => {
            if let Some(o) = git_text(
                runner,
                cwd,
                &["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"],
            )
            .await
            {
                candidates.push(o);
            }
            candidates.extend(["origin/main", "origin/master", "main", "master"].map(String::from));
        }
    }
    for c in candidates {
        if Some(c.as_str()) == head_branch || !valid_base_ref(&c) {
            continue;
        }
        let spec = format!("{c}^{{commit}}");
        if let Some(sha) = git_text(
            runner,
            cwd,
            &["rev-parse", "--verify", "-q", "--end-of-options", &spec],
        )
        .await
        {
            return Some((c, sha));
        }
    }
    None
}

/// Separa la salida de `git log --format=%s%x1f%b%x1e` en «asunto\n\ncuerpo».
fn parse_pr_commits(raw: &str) -> Vec<String> {
    raw.split('\u{1e}')
        .filter_map(|rec| {
            let (subject, body) = rec.split_once('\u{1f}').unwrap_or((rec, ""));
            let (subject, body) = (subject.trim(), body.trim());
            match (subject.is_empty(), body.is_empty()) {
                (true, _) => None,
                (false, true) => Some(subject.to_owned()),
                (false, false) => Some(format!("{subject}\n\n{body}")),
            }
        })
        .collect()
}

/// Propuesta de conflicto inservible (marcadores, texto de más o nada). Nunca lleva el
/// contenido: el detalle solo dice qué pasó.
fn invalid_proposal(detail: &str) -> ApiError {
    ApiError::new(
        "AI_CONFLICT_INVALID_PROPOSAL",
        "La propuesta no sirve como resolución del conflicto. Regenera o resuelve el archivo a mano.",
        Some(detail.to_owned()),
    )
}

/// Nombre del tipo de conflicto tal como lo ve la UI (`bothModified`, `deletedByUs`…).
fn conflict_kind_name(kind: ConflictKind) -> &'static str {
    match kind {
        ConflictKind::BothModified => "bothModified",
        ConflictKind::BothAdded => "bothAdded",
        ConflictKind::BothDeleted => "bothDeleted",
        ConflictKind::AddedByUs => "addedByUs",
        ConflictKind::AddedByThem => "addedByThem",
        ConflictKind::DeletedByUs => "deletedByUs",
        ConflictKind::DeletedByThem => "deletedByThem",
        ConflictKind::Unknown => "unknown",
    }
}

/// Las tres versiones del conflicto y el archivo de trabajo, todo en UTF-8. `None` si algún lado
/// es binario, si el archivo de trabajo no es texto (o no existe) o si no es UTF-8 válido: es
/// exactamente lo que no se puede mandar a la IA.
#[allow(clippy::type_complexity)]
fn conflict_texts(
    file: &ConflictFile,
) -> Option<(Option<String>, Option<String>, Option<String>, String)> {
    if file.ours.is_binary || file.base.is_binary || file.theirs.is_binary {
        return None;
    }
    let side = |s: &ConflictSide| -> Option<Option<String>> {
        match s.content.as_deref() {
            None => Some(None),
            Some(bytes) => std::str::from_utf8(bytes).ok().map(|t| Some(t.to_owned())),
        }
    };
    let working = file.working_tree.as_deref()?;
    if working.contains(&0) {
        return None;
    }
    let working = std::str::from_utf8(working).ok()?.to_owned();
    Some((
        side(&file.ours)?,
        side(&file.base)?,
        side(&file.theirs)?,
        working,
    ))
}

fn not_installed(p: ProviderId) -> AiError {
    AiError::NotInstalled {
        provider: p,
        hint: format!(
            "No encuentro `{}`. Instálalo y ejecuta `{}`.",
            p.binary_name(),
            p.login_invocation()
        ),
    }
}

fn parse_choice(s: &str) -> ApiResult<ProviderChoice> {
    match s {
        "auto" => Ok(ProviderChoice::Auto),
        "claude" => Ok(ProviderChoice::Claude),
        "codex" => Ok(ProviderChoice::Codex),
        "copilot" => Ok(ProviderChoice::Copilot),
        other => Err(invalid("choice", other)),
    }
}

fn parse_profile(s: &str) -> ApiResult<ProfileKind> {
    match s {
        "personal" => Ok(ProfileKind::Personal),
        "empresa" => Ok(ProfileKind::Empresa),
        other => Err(invalid("profile", other)),
    }
}

fn parse_provider(s: &str) -> ApiResult<ProviderId> {
    match s {
        "claude" => Ok(ProviderId::Claude),
        "codex" => Ok(ProviderId::Codex),
        "copilot" => Ok(ProviderId::Copilot),
        other => Err(invalid("id", other)),
    }
}

fn invalid(field: &str, value: &str) -> ApiError {
    let shown: String = value.chars().take(40).collect();
    ApiError::new(
        "PARSE",
        format!("Valor no válido para «{field}» en los ajustes de IA."),
        Some(shown),
    )
}

// ------------------------------------------------------------------ estado

/// Estado de IA a partir de los ajustes y de la disponibilidad ya comprobada.
pub(crate) fn compose_state(st: &StoredAi, avail: &[(ProviderId, Availability)]) -> AiState {
    let profile = st.profile();
    let providers = avail
        .iter()
        .map(|(id, a)| {
            let (availability, version, hint) = match a {
                Availability::Available { version, .. } => (
                    "available",
                    Some(version.clone()).filter(|v| !v.is_empty()),
                    None,
                ),
                Availability::Missing { hint } => ("missing", None, Some(hint.clone())),
                Availability::Failed { detail } => ("failed", None, Some(detail.clone())),
            };
            AiProviderState {
                id: *id,
                display_name: id.display_name().to_owned(),
                availability: availability.to_owned(),
                version,
                hint,
                enabled: profile.is_enabled(*id),
                consented: profile.has_consent(*id),
                login_command: id.login_invocation().to_owned(),
                shares_login: shares_login_between_profiles(*id),
                isolated: profile.is_isolated() && !shares_login_between_profiles(*id),
                isolation_verified: isolation(*id).verified,
            }
        })
        .collect();
    AiState {
        enabled: st.settings.enabled,
        profile: AiProfileInfo {
            name: profile.name.clone(),
            kind: profile.kind,
            isolated: profile.is_isolated(),
            rule: None,
        },
        choice: st.settings.provider,
        providers,
    }
}

// ------------------------------------------------------------------ contexto

/// Archivos preparados repartidos entre los que se envían y los excluidos.
pub(crate) struct Classified {
    /// (índice en la entrada, texto del diff del archivo).
    pub included: Vec<(usize, String)>,
    pub excluded: Vec<AiExcludedFile>,
    /// Tamaño del mayor archivo excluido por superar el tope por sí solo.
    pub largest_too_large: usize,
}

/// Aplica los criterios de exclusión (spec §5.1): sensibles, binarios y archivos que por sí
/// solos superan el tope. Los criterios de «sensible» son los del `PromptBuilder`.
pub(crate) fn classify(diffs: &[FileDiff], max_bytes: usize) -> Classified {
    let mut out = Classified {
        included: Vec::new(),
        excluded: Vec::new(),
        largest_too_large: 0,
    };
    for (i, fd) in diffs.iter().enumerate() {
        let sensitive = is_sensitive_path(&fd.path.to_string_lossy())
            || fd
                .old_path
                .as_ref()
                .is_some_and(|p| is_sensitive_path(&p.to_string_lossy()));
        let reason = if sensitive {
            Some(AiExcludeReason::Sensitive)
        } else if fd.is_binary {
            Some(AiExcludeReason::Binary)
        } else {
            None
        };
        if let Some(reason) = reason {
            out.excluded.push(AiExcludedFile {
                path: fd.path.clone(),
                reason,
            });
            continue;
        }
        let text = String::from_utf8_lossy(&fd.raw_bytes()).into_owned();
        if text.len() > max_bytes {
            out.largest_too_large = out.largest_too_large.max(text.len());
            out.excluded.push(AiExcludedFile {
                path: fd.path.clone(),
                reason: AiExcludeReason::TooLarge,
            });
        } else {
            out.included.push((i, text));
        }
    }
    out
}

/// Lo que se enviaría, ya calculado.
pub(crate) struct Gathered {
    pub diff: String,
    pub files: Vec<AiPreviewFile>,
    pub excluded: Vec<AiExcludedFile>,
    pub opts: CommitOptions,
    pub prompt: PromptPreview,
    pub revision: u64,
    pub max_bytes: usize,
    pub largest_too_large: usize,
    /// No había nada preparado (ni siquiera archivos que excluir).
    pub nothing_staged: bool,
}

impl Gathered {
    fn preview(&self, provider: Option<ProviderId>) -> AiContextPreview {
        AiContextPreview {
            provider: provider.map(|p| p.binary_name().to_owned()),
            bytes: self.prompt.context_bytes,
            max_bytes: self.max_bytes,
            files: self.files.clone(),
            excluded: self.excluded.clone(),
            redactions: self.prompt.redactions,
            recent_subjects: if self.opts.language == CommitLanguage::Auto {
                self.opts.recent_subjects.len()
            } else {
                0
            },
        }
    }

    /// El error que corresponde cuando no queda nada que enviar.
    fn empty_error(&self) -> Option<ApiError> {
        if !self.files.is_empty() {
            return None;
        }
        if self.nothing_staged {
            return Some(ApiError::new(
                "AI_NO_STAGED_CHANGES",
                "No hay cambios preparados. Prepara los archivos que quieras describir y vuelve a intentarlo.",
                None,
            ));
        }
        self.excluded_error()
    }

    /// Todo lo que había se excluyó (sensible, binario o demasiado grande).
    fn excluded_error(&self) -> Option<ApiError> {
        let has = |r: AiExcludeReason| self.excluded.iter().any(|e| e.reason == r);
        Some(if has(AiExcludeReason::Sensitive) {
            ApiError::new(
                "AI_SENSITIVE_FILE",
                "Los cambios son archivos sensibles (.env, claves, certificados…) o binarios, y no se envían a la IA.",
                self.excluded.first().map(|e| e.path.to_string_lossy().into_owned()),
            )
        } else if has(AiExcludeReason::TooLarge) {
            ApiError::from(AiError::ContextTooLarge {
                bytes: self.largest_too_large,
                max_bytes: self.max_bytes,
            })
        } else {
            ApiError::new(
                "AI_NO_STAGED_CHANGES",
                "Lo preparado son solo archivos binarios, que no se envían a la IA.",
                Some("binary".to_owned()),
            )
        })
    }
}

fn human_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} bytes")
    } else {
        // Redondeo a una décima; los valores son pequeños, la precisión no importa.
        let tenths = bytes * 10 / 1024;
        format!("{}.{} KB", tenths / 10, tenths % 10)
    }
}

impl AppCore {
    fn ai(&self) -> &AiService {
        &self.inner.ai
    }

    /// Lee el diff preparado y los asuntos recientes de la pestaña y construye lo que se
    /// enviaría: es la **única** fuente de verdad para la vista previa y para el envío.
    async fn ai_gather(
        &self,
        tab_id: &str,
        options: &AiCommitOptions,
        st: &StoredAi,
    ) -> ApiResult<Gathered> {
        let (session, _) = self.live(tab_id)?;
        let snap = Self::fresh_snapshot(&session).await;
        let max_bytes = st.settings.max_context_bytes;

        let staged: Vec<FileDiff> = self
            .read_op(tab_id, "diff preparado para IA", |e, r| {
                Box::pin(async move {
                    let none: Vec<GitPath> = Vec::new();
                    e.diff(&r, &DiffScope::Staged, &none).await
                })
            })
            .await?;

        let recent_subjects = if options.language == CommitLanguage::Auto {
            self.read_op(tab_id, "historial para IA", |e, r| {
                Box::pin(async move { e.log(&r, RECENT_SUBJECTS).await })
            })
            .await
            // Un repo sin commits no tiene historial: no es un error.
            .unwrap_or_default()
            .into_iter()
            .map(|c| c.subject)
            .filter(|s| !s.trim().is_empty())
            .collect()
        } else {
            Vec::new()
        };

        let nothing_staged = staged.is_empty();
        let classified = classify(&staged, max_bytes);
        let diff: String = classified
            .included
            .iter()
            .map(|(_, t)| t.as_str())
            .collect();
        let files = classified
            .included
            .iter()
            .map(|(i, text)| AiPreviewFile {
                path: staged[*i].path.clone(),
                bytes: redact_secrets(text).0.len(),
            })
            .collect();
        let opts = CommitOptions {
            template: options.template.clone(),
            language: options.language,
            style: options.style,
            recent_subjects,
        };
        let prompt = PromptBuilder::commit_message_with(&diff, &opts);
        Ok(Gathered {
            diff,
            files,
            excluded: classified.excluded,
            opts,
            prompt,
            revision: snap.revision,
            max_bytes,
            largest_too_large: classified.largest_too_large,
            nothing_staged,
        })
    }

    // --- comandos --------------------------------------------------------------------------

    pub(crate) async fn ai_availability(&self, st: &StoredAi) -> Vec<(ProviderId, Availability)> {
        let svc = self.ai();
        let (claude, codex, copilot) = (
            svc.provider(ProviderId::Claude, st),
            svc.provider(ProviderId::Codex, st),
            svc.provider(ProviderId::Copilot, st),
        );
        let (a, b, c) = tokio::join!(
            claude.availability(),
            codex.availability(),
            copilot.availability()
        );
        vec![
            (ProviderId::Claude, a),
            (ProviderId::Codex, b),
            (ProviderId::Copilot, c),
        ]
    }

    /// Estado de IA; `rule` es la regla repo → perfil de la pestaña (manda sobre el perfil
    /// elegido en Ajustes y se muestra en `profile.rule`).
    async fn ai_state_now(&self, rule: Option<&RuleMatch>) -> AiState {
        let st = self.ai().snapshot_with(rule);
        let avail = self.ai_availability(&st).await;
        let mut state = compose_state(&st, &avail);
        state.profile.rule = rule.map(|r| r.pattern.clone());
        state
    }

    /// `ai_state`: ajustes, perfil y disponibilidad real de cada CLI (ejecutando `--version`).
    pub async fn ai_state(&self, tab_id: &str) -> ApiResult<AiState> {
        self.live(tab_id)?;
        let rule = self.rule_for_tab(tab_id);
        Ok(self.ai_state_now(rule.as_ref()).await)
    }

    /// `ai_update_settings`: ajuste global, guardado en `ai-settings.json`.
    pub async fn ai_update_settings(&self, patch: AiSettingsPatch) -> ApiResult<AiState> {
        self.ai().update(&patch)?;
        let rule = patch.tab_id.as_deref().and_then(|t| self.rule_for_tab(t));
        Ok(self.ai_state_now(rule.as_ref()).await)
    }

    /// `ai_preview_context`: lo que se enviaría, sin enviarlo ni preguntar nada.
    pub async fn ai_preview_context(
        &self,
        tab_id: &str,
        options: Option<AiCommitOptions>,
    ) -> ApiResult<AiContextPreview> {
        let rule = self.rule_for_tab(tab_id);
        let st = self.ai().snapshot_with(rule.as_ref());
        let g = self
            .ai_gather(tab_id, &options.unwrap_or_default(), &st)
            .await?;
        Ok(g.preview(self.ai().likely_provider(&st)))
    }

    /// `ai_cancel`: cancela de verdad (mata el proceso hijo). `true` si había una en curso.
    pub fn ai_cancel(&self, tab_id: &str) -> ApiResult<bool> {
        self.live(tab_id)?;
        Ok(self.ai().cancel_tab(tab_id))
    }

    /// Cierre de pestaña o de la aplicación.
    pub(crate) fn ai_cancel_tab_silently(&self, tab_id: &str) {
        self.ai().cancel_tab(tab_id);
    }

    pub(crate) fn ai_cancel_all(&self) {
        self.ai().cancel_all();
    }

    pub(crate) fn ai_cancel_operation(&self, tab_id: &str, op_id: u64) -> bool {
        self.ai().cancel_op(tab_id, op_id)
    }

    fn ai_event(&self, tab_id: &str, kind: ApiSessionEventKind) {
        let Ok((session, _)) = self.live(tab_id) else {
            return;
        };
        self.emit(AppEvent::Session(ApiSessionEvent {
            session_id: tab_id.to_owned(),
            revision: session.revision(),
            kind,
        }));
    }

    /// `ai_generate_commit_message`: propone un mensaje a partir del diff preparado.
    pub async fn ai_generate_commit_message(
        &self,
        tab_id: &str,
        options: AiCommitOptions,
    ) -> ApiResult<AiCommitSuggestion> {
        let st = self.ai().snapshot();
        if !st.settings.enabled {
            return Err(ApiError::from(AiError::AiDisabled));
        }
        self.live(tab_id)?;

        // Una sola generación por pestaña: esta toma el relevo de la anterior (que acaba en
        // CANCELLED). Se registra antes de leer nada para que `ai_cancel` valga desde ya.
        let run = self.ai().begin(tab_id);
        let label = "IA: mensaje de commit".to_owned();
        self.ai_event(
            tab_id,
            ApiSessionEventKind::OperationStarted {
                operation_id: run.op_id,
                label: label.clone(),
            },
        );
        let result = self.ai_generate_inner(tab_id, &options, &run).await;
        self.ai_event(
            tab_id,
            ApiSessionEventKind::OperationFinished {
                operation_id: run.op_id,
                label,
                ok: result.is_ok(),
                error: result.as_ref().err().cloned(),
            },
        );
        drop(run);
        result
    }

    async fn ai_generate_inner(
        &self,
        tab_id: &str,
        options: &AiCommitOptions,
        run: &Run,
    ) -> ApiResult<AiCommitSuggestion> {
        let svc = self.ai();
        // La regla repo → perfil manda sobre el perfil elegido en Ajustes.
        let rule = self.rule_for_tab(tab_id);
        let mut st = svc.snapshot_with(rule.as_ref());

        let gathered = self.ai_gather(tab_id, options, &st).await?;
        if run.token.is_cancelled() {
            return Err(AiError::Cancelled.into());
        }
        if let Some(e) = gathered.empty_error() {
            return Err(e);
        }
        // Tope de contexto: si se supera, no se llama al CLI (ni se pregunta nada).
        AiContextPolicy::new(gathered.max_bytes)
            .validate_len(gathered.prompt.context_bytes)
            .map_err(ApiError::from)?;

        if let Plan::NeedConsent(provider) = svc.plan(&st)? {
            self.ai_ask_consent(
                provider,
                st.active_profile,
                Consent {
                    files: &gathered.files,
                    excluded: gathered.excluded.len(),
                    context_bytes: gathered.prompt.context_bytes,
                    purpose: Purpose::Commit,
                },
                run,
            )
            .await?;
            st = svc.snapshot_with(rule.as_ref());
        }
        if run.token.is_cancelled() {
            return Err(AiError::Cancelled.into());
        }

        let providers: Vec<Arc<dyn AiProvider>> = ProviderId::AUTO_ORDER
            .into_iter()
            .map(|id| Arc::new(svc.provider(id, &st)) as Arc<dyn AiProvider>)
            .collect();
        let orchestrator = Orchestrator::new(providers, Arc::clone(&svc.shared.limiter));
        let call = CallContext::new(gathered.revision.to_string()).with_cancel(run.token.clone());
        let suggestion = orchestrator
            .suggest_commit_message_with(
                &svc.call_profile(&st),
                st.settings.provider,
                &call,
                &gathered.diff,
                &gathered.opts,
            )
            .await
            .map_err(ApiError::from)?;

        Ok(AiCommitSuggestion {
            subject: suggestion.subject.clone().unwrap_or_default(),
            body: suggestion.body.clone(),
            provider: suggestion.provider.binary_name().to_owned(),
            command_summary: suggestion.command_summary.clone(),
            repo_revision: gathered.revision,
        })
    }

    // --- resolución de conflictos -------------------------------------------------------------

    /// `ai_resolve_conflict`: propone el archivo **completo** ya resuelto a partir de las tres
    /// versiones del conflicto (base, nuestra y suya).
    ///
    /// Mismos filtros y garantías que el resto de la IA: sensibles, binarios, texto que no es
    /// UTF-8 y tope de contexto antes de llamar al CLI; consentimiento por perfil y proveedor;
    /// una sola generación por pestaña, cancelable de verdad. La propuesta se valida (sin
    /// marcadores de conflicto y sin tocar el texto fuera de los bloques) y **no se escribe
    /// nada**: el texto vuelve a la UI para que el usuario lo revise y decida.
    pub async fn ai_resolve_conflict(
        &self,
        tab_id: &str,
        path: GitPath,
    ) -> ApiResult<AiConflictSuggestion> {
        let st = self.ai().snapshot();
        if !st.settings.enabled {
            return Err(ApiError::from(AiError::AiDisabled));
        }
        self.live(tab_id)?;

        let run = self.ai().begin(tab_id);
        let label = "IA: resolver conflicto".to_owned();
        self.ai_event(
            tab_id,
            ApiSessionEventKind::OperationStarted {
                operation_id: run.op_id,
                label: label.clone(),
            },
        );
        let result = self.ai_conflict_inner(tab_id, &path, &run).await;
        self.ai_event(
            tab_id,
            ApiSessionEventKind::OperationFinished {
                operation_id: run.op_id,
                label,
                ok: result.is_ok(),
                error: result.as_ref().err().cloned(),
            },
        );
        drop(run);
        result
    }

    async fn ai_conflict_inner(
        &self,
        tab_id: &str,
        path: &GitPath,
        run: &Run,
    ) -> ApiResult<AiConflictSuggestion> {
        let svc = self.ai();
        let rule = self.rule_for_tab(tab_id);
        let mut st = svc.snapshot_with(rule.as_ref());
        let max_bytes = st.settings.max_context_bytes;

        let (session, _) = self.live(tab_id)?;
        let revision = Self::fresh_snapshot(&session).await.revision;
        let shown = path.to_string_lossy().into_owned();

        // Estado del conflicto: solo lectura (índice con etapas 1/2/3 y archivo de trabajo).
        let files = self.get_conflicts(tab_id).await?;
        let file = files.iter().find(|f| &f.path == path).ok_or_else(|| {
            ApiError::new(
                "CONFLICT_NOT_FOUND",
                "Ese archivo ya no está en conflicto.",
                Some(shown.clone()),
            )
        })?;

        // Mismos filtros que el resto de la IA: lo sensible no se envía nunca.
        if is_sensitive_path(&shown) {
            return Err(ApiError::from(AiError::SensitiveFile(shown)));
        }
        let Some((ours, base, theirs, working)) = conflict_texts(file) else {
            return Err(ApiError::new(
                "AI_CONFLICT_NOT_TEXT",
                "Este conflicto es binario o no es texto UTF-8, y la IA no puede resolverlo: elige una versión entera.",
                Some(shown),
            ));
        };

        let input = ConflictInput {
            path: shown.clone(),
            kind: conflict_kind_name(file.kind).to_owned(),
            ours,
            base,
            theirs,
            working_tree: Some(working.clone()),
        };
        let prompt = PromptBuilder::resolve_conflict(&input).map_err(ApiError::from)?;
        AiContextPolicy::new(max_bytes)
            .validate_len(prompt.context_bytes)
            .map_err(ApiError::from)?;
        if run.token.is_cancelled() {
            return Err(AiError::Cancelled.into());
        }

        if let Plan::NeedConsent(provider) = svc.plan(&st)? {
            let files = [AiPreviewFile {
                path: path.clone(),
                bytes: prompt.context_bytes,
            }];
            self.ai_ask_consent(
                provider,
                st.active_profile,
                Consent {
                    files: &files,
                    excluded: 0,
                    context_bytes: prompt.context_bytes,
                    purpose: Purpose::Conflict,
                },
                run,
            )
            .await?;
            st = svc.snapshot_with(rule.as_ref());
        }
        if run.token.is_cancelled() {
            return Err(AiError::Cancelled.into());
        }

        let providers: Vec<Arc<dyn AiProvider>> = ProviderId::AUTO_ORDER
            .into_iter()
            .map(|id| Arc::new(svc.provider(id, &st)) as Arc<dyn AiProvider>)
            .collect();
        let orchestrator = Orchestrator::new(providers, Arc::clone(&svc.shared.limiter));
        let call = CallContext::new(revision.to_string()).with_cancel(run.token.clone());
        let suggestion = orchestrator
            .propose_conflict_resolution(
                &svc.call_profile(&st),
                st.settings.provider,
                &call,
                &input,
            )
            .await
            .map_err(ApiError::from)?;

        let content = suggestion
            .proposed_content
            .clone()
            .filter(|c| !c.trim().is_empty())
            .ok_or_else(|| invalid_proposal("el proveedor no devolvió el archivo completo"))?;
        // Se vuelve a validar aquí (además de en el proveedor): sin marcadores, sin tocar el
        // texto fuera de los bloques y sin `[REDACTED]` que se comería un secreto del archivo.
        // El texto llega como `String`, así que es UTF-8 válido por construcción.
        validate_conflict_proposal(&working, &content).map_err(ApiError::from)?;

        Ok(AiConflictSuggestion {
            path: path.clone(),
            content,
            rationale: suggestion
                .rationale
                .clone()
                .filter(|r| !r.trim().is_empty()),
            provider: suggestion.provider.binary_name().to_owned(),
            command_summary: suggestion.command_summary.clone(),
            repo_revision: revision,
        })
    }

    // --- cuenta e inicio de sesión (RF-IA12, D16) ---------------------------------------------

    /// Perfil para cuenta/login: el pedido, si no el de la regla de la pestaña, si no el activo.
    fn ai_profile_kind(
        &self,
        st: &StoredAi,
        tab_id: Option<&str>,
        profile: Option<&str>,
    ) -> ApiResult<(ProfileKind, Option<String>)> {
        if let Some(p) = profile {
            return Ok((parse_profile(p)?, None));
        }
        if let Some(r) = tab_id.and_then(|t| self.rule_for_tab(t)) {
            return Ok((r.kind, Some(r.pattern)));
        }
        Ok((st.active_profile, None))
    }

    /// `ai_accounts`: la cuenta de cada proveedor **según el propio CLI** y con el directorio
    /// del perfil (sin leer archivos de credenciales). Lo que el CLI no expone es «no
    /// disponible».
    pub async fn ai_accounts(&self, req: AiAccountsRequest) -> ApiResult<AiAccounts> {
        let svc = self.ai();
        let st = svc.snapshot();
        if let Some(t) = req.tab_id.as_deref() {
            self.live(t)?;
        }
        let (kind, rule) =
            self.ai_profile_kind(&st, req.tab_id.as_deref(), req.profile.as_deref())?;
        let profile = st.profile_of(kind);
        let providers: Vec<CliProvider> = ProviderId::AUTO_ORDER
            .into_iter()
            .map(|id| svc.provider(id, &st))
            .collect();
        let dirs: Vec<Option<PathBuf>> = ProviderId::AUTO_ORDER
            .into_iter()
            .map(|id| svc.ensure_config_dir(&st, kind, id))
            .collect();
        let (a, b, c) = tokio::join!(
            providers[0].account(dirs[0].as_deref()),
            providers[1].account(dirs[1].as_deref()),
            providers[2].account(dirs[2].as_deref())
        );
        Ok(AiAccounts {
            profile: AiProfileInfo {
                name: profile.name.clone(),
                kind,
                isolated: profile.is_isolated(),
                rule,
            },
            accounts: vec![a, b, c],
        })
    }

    /// `ai_login`: prepara (y, si `launch`, abre) el inicio de sesión oficial del CLI con el
    /// directorio del perfil. Ariadna no ve credenciales; solo lanza una terminal o muestra el
    /// comando.
    pub async fn ai_login(&self, req: AiLoginRequest) -> ApiResult<AiLoginInfo> {
        let svc = self.ai();
        let st = svc.snapshot();
        if let Some(t) = req.tab_id.as_deref() {
            self.live(t)?;
        }
        let id = parse_provider(&req.provider)?;
        let (kind, _) = self.ai_profile_kind(&st, req.tab_id.as_deref(), req.profile.as_deref())?;
        let profile = st.profile_of(kind);
        if !profile.is_enabled(id) {
            return Err(AiError::ProviderDisabled { provider: id }.into());
        }
        let Some(resolved) = resolve_binary(id, st.binary(id).as_deref(), &svc.search()) else {
            return Err(not_installed(id).into());
        };
        let dir = svc.ensure_config_dir(&st, kind, id);
        let plan = LoginPlan::new(id, Some(resolved.display_path()), dir.as_deref());
        let shell = Shell::native();
        let mut notes: Vec<String> = Vec::new();
        let shares = shares_login_between_profiles(id);
        if shares {
            notes.push(format!(
                "{} no permite aislar la sesión por perfil: Personal y Empresa comparten el mismo login.",
                id.display_name()
            ));
        } else if !profile.is_isolated() {
            notes.push(format!(
                "El perfil {} usa la sesión normal de {} (aislamiento desactivado).",
                profile.name,
                id.display_name()
            ));
        } else if !isolation(id).verified {
            notes.push(format!(
                "El aislamiento de {} sigue la documentación del CLI y no se ha verificado.",
                id.display_name()
            ));
        }
        let command = match plan.command_line(shell) {
            Ok(c) => Some(c),
            Err(e) => {
                notes.push(e.to_string());
                None
            }
        };
        let mut launched = false;
        if req.launch {
            let title = format!("Ariadna - {} ({})", id.display_name(), profile.name);
            let script_dir = svc.shared.profiles_dir.join(kind.dir_name());
            match write_script(&script_dir, &plan, shell, &title) {
                Ok(script) => {
                    let launcher = Arc::clone(&lock(&svc.shared.launcher));
                    // Abrir una terminal puede bloquear un instante: fuera del hilo asíncrono.
                    let res = tokio::task::spawn_blocking(move || launcher.open(&script)).await;
                    match res {
                        Ok(Ok(())) => launched = true,
                        Ok(Err(e)) => notes.push(format!("No se pudo abrir la terminal: {e}")),
                        Err(_) => notes.push("No se pudo abrir la terminal.".to_owned()),
                    }
                }
                Err(e) => notes.push(format!("No se pudo preparar el inicio de sesión: {e}")),
            }
        }
        Ok(AiLoginInfo {
            provider: id,
            profile: kind,
            command,
            shell: match shell {
                Shell::Posix => "posix",
                Shell::Cmd => "cmd",
            }
            .to_owned(),
            config_dir: dir.map(|d| d.to_string_lossy().into_owned()),
            isolated: profile.is_isolated() && !shares,
            shares_login: shares,
            launched,
            verified: ariadna_ai::commands::login_verified(id),
            note: (!notes.is_empty()).then(|| notes.join(" ")),
        })
    }

    // --- título y descripción de pull request -------------------------------------------------

    /// Lee commits y diff de la rama actual contra su base (ancestro común), de solo lectura.
    async fn ai_read_pr(&self, tab_id: &str, base: Option<String>) -> ApiResult<PrRaw> {
        let Some(runner) = self.git_runner() else {
            return Err(ApiError::new(
                "GIT_NOT_FOUND",
                "No hay un git utilizable para leer la rama.",
                None,
            ));
        };
        self.read_op(tab_id, "rama frente a la base para IA", move |e, r| {
            Box::pin(async move {
                let cwd = r.path.clone();
                let head_branch =
                    git_text(&runner, &cwd, &["symbolic-ref", "--short", "-q", "HEAD"]).await;
                let Some((base_name, base_sha)) =
                    resolve_base(&runner, &cwd, base.as_deref(), head_branch.as_deref()).await
                else {
                    return Ok(PrRaw::NoBase);
                };
                let Some(mb) = git_text(&runner, &cwd, &["merge-base", &base_sha, "HEAD"]).await
                else {
                    return Ok(PrRaw::NoBase);
                };
                let range = format!("{mb}..HEAD");
                let max = format!("--max-count={PR_MAX_COMMITS}");
                let raw = git_text(
                    &runner,
                    &cwd,
                    &[
                        "log",
                        "--no-color",
                        "--no-show-signature",
                        "--reverse",
                        &max,
                        "--format=%s%x1f%b%x1e",
                        &range,
                    ],
                )
                .await
                .unwrap_or_default();
                let commits = parse_pr_commits(&raw);
                let commit_count = git_text(&runner, &cwd, &["rev-list", "--count", &range])
                    .await
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(commits.len());
                let none: Vec<GitPath> = Vec::new();
                let diffs = e
                    .diff(
                        &r,
                        &DiffScope::Between {
                            base: mb,
                            head: "HEAD".into(),
                        },
                        &none,
                    )
                    .await?;
                Ok(PrRaw::Found {
                    base: base_name,
                    head: head_branch.unwrap_or_else(|| "HEAD".into()),
                    commits,
                    commit_count,
                    diffs,
                })
            })
        })
        .await
    }

    /// `ai_generate_pull_request`: propone título y descripción de PR a partir de la rama
    /// actual, sus commits y el diff contra la base. **Solo propone**: no crea la PR ni toca el
    /// repo. Mismas garantías que el mensaje de commit: exclusión de sensibles, redacción de
    /// secretos, tope de contexto, consentimiento por perfil y proveedor, cola y cancelación.
    pub async fn ai_generate_pull_request(
        &self,
        tab_id: &str,
        options: AiPullRequestOptions,
    ) -> ApiResult<AiPullRequestSuggestion> {
        let st = self.ai().snapshot();
        if !st.settings.enabled {
            return Err(ApiError::from(AiError::AiDisabled));
        }
        self.live(tab_id)?;
        let base = options
            .base
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(str::to_owned);
        if let Some(b) = &base {
            if !valid_base_ref(b) {
                return Err(invalid("base", b));
            }
        }

        let run = self.ai().begin(tab_id);
        let label = "IA: descripción de PR".to_owned();
        self.ai_event(
            tab_id,
            ApiSessionEventKind::OperationStarted {
                operation_id: run.op_id,
                label: label.clone(),
            },
        );
        let result = self.ai_pr_inner(tab_id, &options, base, &run).await;
        self.ai_event(
            tab_id,
            ApiSessionEventKind::OperationFinished {
                operation_id: run.op_id,
                label,
                ok: result.is_ok(),
                error: result.as_ref().err().cloned(),
            },
        );
        drop(run);
        result
    }

    async fn ai_pr_inner(
        &self,
        tab_id: &str,
        options: &AiPullRequestOptions,
        base: Option<String>,
        run: &Run,
    ) -> ApiResult<AiPullRequestSuggestion> {
        let svc = self.ai();
        let rule = self.rule_for_tab(tab_id);
        let mut st = svc.snapshot_with(rule.as_ref());
        let max_bytes = st.settings.max_context_bytes;

        let (session, _) = self.live(tab_id)?;
        let revision = Self::fresh_snapshot(&session).await.revision;
        let raw = self.ai_read_pr(tab_id, base.clone()).await?;
        if run.token.is_cancelled() {
            return Err(AiError::Cancelled.into());
        }
        let PrRaw::Found {
            base: base_name,
            head,
            commits,
            commit_count,
            diffs,
        } = raw
        else {
            return Err(ApiError::new(
                "AI_BASE_NOT_FOUND",
                "No encuentro la rama base para comparar. Indica una rama base que exista en este repositorio y que comparta historia con la rama actual.",
                base,
            ));
        };
        if diffs.is_empty() && commits.is_empty() {
            return Err(ApiError::new(
                "AI_NO_BRANCH_CHANGES",
                format!("La rama {head} no tiene cambios respecto a {base_name}."),
                None,
            ));
        }

        let classified = classify(&diffs, max_bytes);
        let diff: String = classified
            .included
            .iter()
            .map(|(_, t)| t.as_str())
            .collect();
        let files: Vec<AiPreviewFile> = classified
            .included
            .iter()
            .map(|(i, text)| AiPreviewFile {
                path: diffs[*i].path.clone(),
                bytes: redact_secrets(text).0.len(),
            })
            .collect();
        let input = PullRequestInput {
            base: base_name.clone(),
            head: head.clone(),
            commits,
            diff: diff.clone(),
            template: options.template.clone(),
            language: options.language,
        };
        let prompt = PromptBuilder::pull_request(&input);
        let gathered = Gathered {
            diff,
            files,
            excluded: classified.excluded,
            opts: CommitOptions::default(),
            prompt,
            revision,
            max_bytes,
            largest_too_large: classified.largest_too_large,
            nothing_staged: false,
        };
        if let Some(e) = gathered.empty_error() {
            return Err(e);
        }
        AiContextPolicy::new(gathered.max_bytes)
            .validate_len(gathered.prompt.context_bytes)
            .map_err(ApiError::from)?;

        if let Plan::NeedConsent(provider) = svc.plan(&st)? {
            self.ai_ask_consent(
                provider,
                st.active_profile,
                Consent {
                    files: &gathered.files,
                    excluded: gathered.excluded.len(),
                    context_bytes: gathered.prompt.context_bytes,
                    purpose: Purpose::PullRequest,
                },
                run,
            )
            .await?;
            st = svc.snapshot_with(rule.as_ref());
        }
        if run.token.is_cancelled() {
            return Err(AiError::Cancelled.into());
        }

        let providers: Vec<Arc<dyn AiProvider>> = ProviderId::AUTO_ORDER
            .into_iter()
            .map(|id| Arc::new(svc.provider(id, &st)) as Arc<dyn AiProvider>)
            .collect();
        let orchestrator = Orchestrator::new(providers, Arc::clone(&svc.shared.limiter));
        let call = CallContext::new(revision.to_string()).with_cancel(run.token.clone());
        let suggestion = orchestrator
            .suggest_pull_request(&svc.call_profile(&st), st.settings.provider, &call, &input)
            .await
            .map_err(ApiError::from)?;

        Ok(AiPullRequestSuggestion {
            title: suggestion.subject.clone().unwrap_or_default(),
            body: suggestion.body.clone(),
            provider: suggestion.provider.binary_name().to_owned(),
            command_summary: suggestion.command_summary.clone(),
            repo_revision: revision,
            base: base_name,
            head,
            commit_count,
        })
    }

    /// Diálogo nativo de primer uso: qué se envía y a quién. Se guarda lo que decida.
    ///
    /// Sirve para commit, pull request y conflicto: lo único que cambia es el texto (`purpose`)
    /// y la lista de archivos/bytes de contexto, que siempre viene del mismo cálculo.
    async fn ai_ask_consent(
        &self,
        provider: ProviderId,
        kind: ProfileKind,
        c: Consent<'_>,
        run: &Run,
    ) -> ApiResult<()> {
        let (files, excluded, context_bytes, purpose) =
            (c.files, c.excluded, c.context_bytes, c.purpose);
        let svc = self.ai();
        // Dos pestañas que piden consentimiento a la vez no abren dos diálogos.
        let _guard = svc.shared.consent_lock.lock().await;
        let mut st = svc.snapshot();
        st.active_profile = kind;
        if st.profile().has_consent(provider) {
            return Ok(());
        }
        let name = provider.display_name();
        let total: usize = files.iter().map(|f| f.bytes).sum();
        let mut lines: Vec<String> = files
            .iter()
            .take(SHOWN_FILES)
            .map(|f| f.path.to_string_lossy().into_owned())
            .collect();
        if files.len() > SHOWN_FILES {
            lines.push(format!("… y {} más", files.len() - SHOWN_FILES));
        }
        if excluded > 0 {
            lines.push(format!(
                "(no se envían {excluded} archivo(s): sensibles, binarios o demasiado grandes)"
            ));
        }
        let (what, goal, guarantee) = purpose.describe();
        let message = format!(
            "Ariadna enviará a {name} {what} para {goal}.\n\n\
             • {} archivo(s), {} (contexto total: {}).\n\
             • El contenido sale de tu equipo hacia {name}, mediante su CLI y tu sesión (perfil {}).\n\
             • Se excluyen .env, claves, certificados y binarios, y se ocultan los secretos detectados.\n\
             • {guarantee}\n\n\
             Se te pregunta una vez por perfil y proveedor.",
            files.len(),
            human_size(total),
            human_size(context_bytes),
            st.profile().name,
        );
        let title = format!("Enviar contexto a {name}");
        let detail = lines.join("\n");
        let accepted = tokio::select! {
            ok = self.inner.prompter.confirm(&title, &message, &detail) => ok,
            () = run.token.cancelled() => return Err(AiError::Cancelled.into()),
        };
        if !accepted {
            return Err(AiError::ConsentRequired { provider }.into());
        }
        svc.grant_consent(kind, provider);
        Ok(())
    }
}
