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

use ariadna_ai::orchestrator::{Limiter, Orchestrator, Profile, ProfileKind};
use ariadna_ai::prompt::{is_sensitive_path, redact_secrets, PromptBuilder, PromptPreview};
use ariadna_ai::provider::DEFAULT_TIMEOUT;
use ariadna_ai::resolve::{resolve_binary, SearchEnv};
use ariadna_ai::{
    AiContextPolicy, AiError, AiProvider, AiSettings, Availability, CallContext, CliProvider,
    CliProviderConfig, CommitLanguage, CommitOptions, ProviderChoice, ProviderId,
};
use ariadna_git::{DiffScope, FileDiff, GitPath};
use ariadna_process::CancellationToken;
use serde::{Deserialize, Serialize};

use super::api::{
    AiCommitOptions, AiCommitSuggestion, AiContextPreview, AiExcludeReason, AiExcludedFile,
    AiPreviewFile, AiProfileInfo, AiProviderState, AiSettingsPatch, AiState, ApiSessionEvent,
    ApiSessionEventKind, AppEvent,
};
use super::app::AppCore;
use super::error::{ApiError, ApiResult};

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

    fn grant_consent(&self, id: ProviderId) {
        let mut guard = lock(&self.shared.state);
        let mut next = guard.clone();
        next.profile_mut().grant_consent(id);
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
            }
        })
        .collect();
    AiState {
        enabled: st.settings.enabled,
        profile: AiProfileInfo {
            name: profile.name.clone(),
            kind: profile.kind,
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
        let has = |r: AiExcludeReason| self.excluded.iter().any(|e| e.reason == r);
        Some(if has(AiExcludeReason::Sensitive) {
            ApiError::new(
                "AI_SENSITIVE_FILE",
                "Lo preparado son archivos sensibles (.env, claves, certificados…) o binarios, y no se envían a la IA.",
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

    async fn ai_availability(&self, st: &StoredAi) -> Vec<(ProviderId, Availability)> {
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

    async fn ai_state_now(&self) -> AiState {
        let st = self.ai().snapshot();
        let avail = self.ai_availability(&st).await;
        compose_state(&st, &avail)
    }

    /// `ai_state`: ajustes, perfil y disponibilidad real de cada CLI (ejecutando `--version`).
    pub async fn ai_state(&self, tab_id: &str) -> ApiResult<AiState> {
        self.live(tab_id)?;
        Ok(self.ai_state_now().await)
    }

    /// `ai_update_settings`: ajuste global, guardado en `ai-settings.json`.
    pub async fn ai_update_settings(&self, patch: AiSettingsPatch) -> ApiResult<AiState> {
        self.ai().update(&patch)?;
        Ok(self.ai_state_now().await)
    }

    /// `ai_preview_context`: lo que se enviaría, sin enviarlo ni preguntar nada.
    pub async fn ai_preview_context(
        &self,
        tab_id: &str,
        options: Option<AiCommitOptions>,
    ) -> ApiResult<AiContextPreview> {
        let st = self.ai().snapshot();
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
        let mut st = svc.snapshot();

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
            self.ai_ask_consent(provider, &gathered, run).await?;
            st = svc.snapshot();
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
                &st.call_profile(),
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

    /// Diálogo nativo de primer uso: qué se envía y a quién. Se guarda lo que decida.
    async fn ai_ask_consent(&self, provider: ProviderId, g: &Gathered, run: &Run) -> ApiResult<()> {
        let svc = self.ai();
        // Dos pestañas que piden consentimiento a la vez no abren dos diálogos.
        let _guard = svc.shared.consent_lock.lock().await;
        let st = svc.snapshot();
        if st.profile().has_consent(provider) {
            return Ok(());
        }
        let name = provider.display_name();
        let total: usize = g.files.iter().map(|f| f.bytes).sum();
        let mut lines: Vec<String> = g
            .files
            .iter()
            .take(SHOWN_FILES)
            .map(|f| f.path.to_string_lossy().into_owned())
            .collect();
        if g.files.len() > SHOWN_FILES {
            lines.push(format!("… y {} más", g.files.len() - SHOWN_FILES));
        }
        if !g.excluded.is_empty() {
            lines.push(format!(
                "(no se envían {} archivo(s): sensibles, binarios o demasiado grandes)",
                g.excluded.len()
            ));
        }
        let message = format!(
            "Ariadna enviará a {name} el diff PREPARADO de este repositorio para proponer un mensaje de commit.\n\n\
             • {} archivo(s), {} (contexto total: {}).\n\
             • El contenido sale de tu equipo hacia {name}, mediante su CLI y tu sesión (perfil {}).\n\
             • Se excluyen .env, claves, certificados y binarios, y se ocultan los secretos detectados.\n\
             • La IA solo propone: nunca hace commit ni toca el repositorio.\n\n\
             Se te pregunta una vez por perfil y proveedor.",
            g.files.len(),
            human_size(total),
            human_size(g.prompt.context_bytes),
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
        svc.grant_consent(provider);
        Ok(())
    }
}
