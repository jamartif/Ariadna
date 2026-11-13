//! Ajustes de la aplicación (`settings.json`) y sus comandos: ruta de `git`, pull con rebase,
//! tema, tamaño del texto del diff y reglas «remoto → perfil de IA» (spec D16, H-16).
//!
//! * Archivo versionado y de escritura atómica (tmp + rename). Si está dañado o es de una
//!   versión futura, se aparta como `settings.json.bak`, se arranca con los valores por
//!   defecto y la UI recibe un aviso: **un archivo corrupto nunca impide arrancar**.
//! * Validación estricta de cada campo; los errores son `PARSE` con el campo en `detail`.
//! * Ariadna **no guarda tokens**: este archivo no contiene ningún secreto.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use ariadna_ai::{Availability, ProfileKind, ProviderId};
use ariadna_git::{find_git, CommandKind, GitInstall, GitRunner, LocatorOptions};
use ariadna_hosting::RemoteInfo;
use serde::{Deserialize, Serialize};

use super::api::GitInfo;
use super::app::AppCore;
use super::error::{ApiError, ApiResult};

/// Versión actual de `settings.json`.
pub const SETTINGS_VERSION: u32 = 1;
/// Tamaño del texto del diff (px).
pub const DIFF_FONT_RANGE: std::ops::RangeInclusive<u8> = 10..=24;
/// Tope de reglas de perfil.
pub const MAX_RULES: usize = 50;
const MAX_PATTERN: usize = 200;
const MAX_PATH: usize = 1024;

/// Repositorio y licencia que muestra «Acerca de».
const REPOSITORY: &str = "https://github.com/jamartif/Ariadna";

// ------------------------------------------------------------------------------- modelo

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

/// Regla «los repos cuyo remoto empieza por `pattern` usan este perfil de IA».
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileRule {
    /// `host` o `host/propietario[/proyecto[/repo]]`, en minúsculas (`dev.azure.com`).
    pub pattern: String,
    pub profile: ProfileKind,
}

/// Contenido de `settings.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub version: u32,
    /// Idioma de la interfaz: `es`, `en` o `system` (sigue al idioma del sistema operativo).
    pub language: String,
    pub theme: Theme,
    pub diff_font_size: u8,
    /// Ruta del ejecutable de git; `None` = detección automática.
    pub git_path: Option<String>,
    /// `git pull --rebase` en lugar de merge.
    pub pull_rebase: bool,
    pub profile_rules: Vec<ProfileRule>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            language: "es".to_owned(),
            theme: Theme::System,
            diff_font_size: 13,
            git_path: None,
            pull_rebase: false,
            profile_rules: vec![
                ProfileRule {
                    pattern: "dev.azure.com".to_owned(),
                    profile: ProfileKind::Empresa,
                },
                ProfileRule {
                    pattern: "github.com".to_owned(),
                    profile: ProfileKind::Personal,
                },
            ],
        }
    }
}

fn bad(field: &str, why: &str) -> ApiError {
    ApiError::new(
        "PARSE",
        format!("Ajuste no válido «{field}»: {why}."),
        Some(field.to_owned()),
    )
}

/// Normaliza un patrón de regla (`DEV.azure.com/` → `dev.azure.com`) y lo valida.
fn normalize_pattern(raw: &str) -> Result<String, &'static str> {
    let p = raw.trim().trim_matches('/').to_ascii_lowercase();
    if p.is_empty() {
        return Err("el patrón está vacío");
    }
    if p.len() > MAX_PATTERN {
        return Err("el patrón es demasiado largo");
    }
    if p.contains("://") || p.contains('@') {
        return Err("escribe solo host/propietario, sin https:// ni usuario");
    }
    if !p
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'/' | b'~'))
    {
        return Err("solo letras, números y . - _ /");
    }
    let mut segs = p.split('/');
    let host = segs.next().unwrap_or_default();
    if host.is_empty() || host.starts_with('.') || host.ends_with('.') || host.starts_with('-') {
        return Err("el host no es válido");
    }
    if !host.contains('.') && host != "localhost" {
        return Err("empieza por el nombre del servidor (p. ej. github.com)");
    }
    if segs.any(str::is_empty) {
        return Err("hay un segmento vacío");
    }
    Ok(p)
}

impl Settings {
    /// Valida y normaliza. No cambia nada si hay un error (trabaja sobre una copia).
    pub fn validate(&mut self) -> ApiResult<()> {
        self.version = SETTINGS_VERSION;
        // El idioma llega del `<select>` de Ajustes; se normaliza para no rechazar «EN » por un espacio.
        let language = self.language.trim().to_ascii_lowercase();
        if !matches!(language.as_str(), "es" | "en" | "system") {
            return Err(bad("language", "solo se admite «system», «es» o «en»"));
        }
        self.language = language;
        if !DIFF_FONT_RANGE.contains(&self.diff_font_size) {
            return Err(bad(
                "diffFontSize",
                &format!(
                    "debe estar entre {} y {}",
                    DIFF_FONT_RANGE.start(),
                    DIFF_FONT_RANGE.end()
                ),
            ));
        }
        if let Some(p) = self.git_path.take() {
            let p = p.trim().to_owned();
            if !p.is_empty() {
                if p.len() > MAX_PATH || p.contains('\0') {
                    return Err(bad("gitPath", "la ruta no es válida"));
                }
                if !Path::new(&p).is_absolute() {
                    return Err(bad("gitPath", "debe ser una ruta absoluta"));
                }
                self.git_path = Some(p);
            }
        }
        if self.profile_rules.len() > MAX_RULES {
            return Err(bad("profileRules", "hay demasiadas reglas"));
        }
        let mut seen = std::collections::HashSet::new();
        for r in &mut self.profile_rules {
            r.pattern = normalize_pattern(&r.pattern).map_err(|w| bad("profileRules", w))?;
            if !seen.insert(r.pattern.clone()) {
                return Err(bad("profileRules", "hay dos reglas con el mismo patrón"));
            }
        }
        Ok(())
    }
}

// --------------------------------------------------------------------- reglas repo → perfil

/// Regla que se aplicó a un remoto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleMatch {
    pub kind: ProfileKind,
    pub pattern: String,
}

/// Segmentos `host/propietario/proyecto/repo` (minúsculas) de la URL de un remoto. Azure
/// DevOps y sus variantes SSH se normalizan a `dev.azure.com` (ariadna-hosting).
fn remote_segments(url: &str) -> Vec<String> {
    let info = RemoteInfo::parse(url);
    if info.host.is_empty() {
        return Vec::new();
    }
    let mut v = vec![info.host.to_ascii_lowercase()];
    v.extend(
        info.owner
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_ascii_lowercase),
    );
    if let Some(p) = info.project.filter(|p| !p.is_empty()) {
        v.push(p.to_ascii_lowercase());
    }
    if !info.repo.is_empty() {
        v.push(info.repo.to_ascii_lowercase());
    }
    v
}

/// La regla más específica (más segmentos) cuyo patrón es prefijo del remoto.
pub fn match_rule(rules: &[ProfileRule], remote_url: &str) -> Option<RuleMatch> {
    let segs = remote_segments(remote_url);
    if segs.is_empty() {
        return None;
    }
    rules
        .iter()
        .filter_map(|r| {
            let ps: Vec<&str> = r.pattern.split('/').collect();
            let hit = ps.len() <= segs.len() && ps.iter().zip(&segs).all(|(a, b)| a == b);
            hit.then_some((ps.len(), r))
        })
        .max_by_key(|(n, _)| *n)
        .map(|(_, r)| RuleMatch {
            kind: r.profile,
            pattern: r.pattern.clone(),
        })
}

// ---------------------------------------------------------------------------- persistencia

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SettingsLoadStatus {
    /// Leído sin problemas.
    Ok,
    /// No existía: valores por defecto.
    Missing,
    /// De una versión anterior: migrado al esquema actual.
    Migrated,
    /// Dañado o ilegible: apartado como `.bak`; valores por defecto.
    ResetCorrupt,
}

/// Cómo se cargaron los ajustes (la UI muestra `warning` si existe).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsLoadInfo {
    pub status: SettingsLoadStatus,
    pub warning: Option<String>,
}

fn sibling(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    file.with_file_name(name)
}

/// Lleva el JSON de cualquier versión conocida al esquema actual.
fn migrate(mut value: serde_json::Value) -> Result<(Settings, bool), String> {
    let serde_json::Value::Object(map) = &mut value else {
        return Err("no es un objeto JSON".to_owned());
    };
    let version = match map.get("version") {
        None => 0,
        Some(v) => v
            .as_u64()
            .ok_or_else(|| "«version» no es un número".to_owned())?,
    };
    if version > u64::from(SETTINGS_VERSION) {
        return Err(format!(
            "es de una versión más reciente de Ariadna (esquema {version})"
        ));
    }
    // v0 (sin campo de versión): mismos campos; los que falten toman su valor por defecto.
    let migrated = version < u64::from(SETTINGS_VERSION);
    let mut s: Settings = serde_json::from_value(value).map_err(|e| e.to_string())?;
    s.validate().map_err(|e| e.message)?;
    Ok((s, migrated))
}

/// `settings.json` en la carpeta de datos.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    file: PathBuf,
}

impl SettingsStore {
    pub fn new(file: impl Into<PathBuf>) -> Self {
        Self { file: file.into() }
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    pub fn load(&self) -> (Settings, SettingsLoadInfo) {
        let info = |status, warning| SettingsLoadInfo { status, warning };
        let bytes = match fs::read(&self.file) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return (Settings::default(), info(SettingsLoadStatus::Missing, None));
            }
            Err(e) => return self.reset_corrupt(&format!("no se pudo leer: {e}")),
        };
        let parsed = serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|e| e.to_string())
            .and_then(migrate);
        match parsed {
            Ok((s, migrated)) => {
                if migrated {
                    // Se reescribe ya en el esquema actual; si falla se reintentará al guardar.
                    let _ = self.save(&s);
                }
                let status = if migrated {
                    SettingsLoadStatus::Migrated
                } else {
                    SettingsLoadStatus::Ok
                };
                (s, info(status, None))
            }
            Err(why) => self.reset_corrupt(&why),
        }
    }

    /// Aparta el archivo como `.bak` (sin borrarlo) y devuelve los valores por defecto.
    fn reset_corrupt(&self, why: &str) -> (Settings, SettingsLoadInfo) {
        let bak = sibling(&self.file, ".bak");
        let moved = fs::rename(&self.file, &bak).is_ok();
        let name = self
            .file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let warning = if moved {
            format!(
                "{name} no se pudo leer ({why}). Se guardó una copia como {name}.bak y se usan los valores por defecto."
            )
        } else {
            format!("{name} no se pudo leer ({why}). Se usan los valores por defecto.")
        };
        (
            Settings::default(),
            SettingsLoadInfo {
                status: SettingsLoadStatus::ResetCorrupt,
                warning: Some(warning),
            },
        )
    }

    /// Escritura atómica: archivo temporal, `sync_all` y renombrado.
    pub fn save(&self, data: &Settings) -> std::io::Result<()> {
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
        fs::rename(&tmp, &self.file).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    }
}

// ------------------------------------------------------------------------------- servicio

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Ajustes en memoria + archivo.
pub(crate) struct SettingsService {
    store: SettingsStore,
    state: Mutex<Settings>,
    load: Mutex<SettingsLoadInfo>,
}

impl SettingsService {
    pub fn new(data_dir: &Path) -> Self {
        let store = SettingsStore::new(data_dir.join("settings.json"));
        let (settings, load) = store.load();
        Self {
            store,
            state: Mutex::new(settings),
            load: Mutex::new(load),
        }
    }

    pub fn snapshot(&self) -> Settings {
        lock(&self.state).clone()
    }

    pub fn load_info(&self) -> SettingsLoadInfo {
        lock(&self.load).clone()
    }

    /// Añade un aviso de arranque (p. ej. una ruta de git que ya no sirve).
    pub fn warn(&self, text: String) {
        let mut l = lock(&self.load);
        l.warning = Some(match l.warning.take() {
            Some(prev) => format!("{prev} {text}"),
            None => text,
        });
    }

    /// Guarda (atómico) y publica. Si no se puede guardar, no cambia nada.
    pub fn replace(&self, next: Settings) -> ApiResult<()> {
        let mut guard = lock(&self.state);
        if *guard == next {
            return Ok(());
        }
        self.store
            .save(&next)
            .map_err(|e| ApiError::io(format!("no se pudo guardar settings.json: {e}")))?;
        *guard = next;
        Ok(())
    }

    #[cfg(test)]
    pub fn file(&self) -> PathBuf {
        self.store.file().to_path_buf()
    }
}

/// `git` en uso: instalación, problema (si no hay) y ejecutor. Se puede cambiar en caliente.
pub(crate) struct GitSlot {
    pub install: Option<GitInstall>,
    pub problem: Option<String>,
    pub runner: Option<GitRunner>,
}

impl GitSlot {
    pub fn from_result(r: Result<GitInstall, String>) -> Self {
        match r {
            Ok(g) => Self {
                runner: Some(GitRunner::new(&g)),
                install: Some(g),
                problem: None,
            },
            Err(p) => Self {
                install: None,
                problem: Some(p),
                runner: None,
            },
        }
    }
}

/// Valida un git elegido a mano (solo esa ruta: sin buscar otros en silencio).
pub fn locate_configured(path: &str) -> Result<GitInstall, String> {
    let opts = LocatorOptions {
        extra_candidates: vec![PathBuf::from(path)],
        include_known_locations: false,
        include_path: false,
        ..LocatorOptions::default()
    };
    find_git(&opts).map_err(|e| match e {
        ariadna_git::GitLocateError::NotFound { .. } => {
            format!("No hay un ejecutable de git en «{path}».")
        }
        other => other.to_string(),
    })
}

// ----------------------------------------------------------------------------------- tipos IPC

/// «Acerca de».
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutInfo {
    pub version: String,
    pub license: String,
    pub repository: String,
    pub issues: String,
}

/// Respuesta de `get_settings` / `save_settings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub settings: Settings,
    pub load: SettingsLoadInfo,
    pub about: AboutInfo,
    /// `git` en uso (`null` si no hay).
    pub git: Option<GitInfo>,
    pub git_problem: Option<String>,
}

/// Resultado de `detect_git`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDetect {
    pub ok: bool,
    /// `configured` (la ruta dada) o `auto` (búsqueda en PATH y ubicaciones conocidas).
    pub source: String,
    pub path: Option<String>,
    pub version: Option<String>,
    pub problem: Option<String>,
}

/// Nombre y correo de la configuración de git (solo lectura).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitIdentity {
    pub name: Option<String>,
    pub email: Option<String>,
}

/// Estado de un CLI de IA para Ajustes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliStatus {
    pub id: ProviderId,
    pub display_name: String,
    pub installed: bool,
    /// `available` | `missing` | `failed`.
    pub availability: String,
    pub version: Option<String>,
    pub path: Option<String>,
    pub hint: Option<String>,
    /// Comando oficial para iniciar sesión (lo ejecuta el usuario; Ariadna no ve credenciales).
    pub login_command: String,
}

fn shown(path: &Path) -> String {
    dunce::simplified(path).to_string_lossy().into_owned()
}

fn version_text(raw: &str) -> Option<String> {
    Some(raw.trim().to_owned()).filter(|v| !v.is_empty())
}

// ------------------------------------------------------------------------------- comandos

impl AppCore {
    fn settings_svc(&self) -> &SettingsService {
        &self.inner.settings
    }

    /// Ajustes actuales (copia).
    pub fn settings_snapshot(&self) -> Settings {
        self.settings_svc().snapshot()
    }

    /// `pull` sin indicación explícita usa el ajuste.
    pub fn pull_rebase_setting(&self) -> bool {
        self.settings_svc().snapshot().pull_rebase
    }

    /// `get_settings`.
    pub fn get_settings(&self) -> SettingsView {
        let slot = lock(&self.inner.gitslot);
        SettingsView {
            settings: self.settings_svc().snapshot(),
            load: self.settings_svc().load_info(),
            about: AboutInfo {
                version: env!("CARGO_PKG_VERSION").to_owned(),
                license: "MIT".to_owned(),
                repository: REPOSITORY.to_owned(),
                issues: format!("{REPOSITORY}/issues"),
            },
            git: slot.install.as_ref().map(|g| GitInfo {
                path: shown(&g.path),
                version: g.version.to_string(),
            }),
            git_problem: slot.problem.clone(),
        }
    }

    /// `save_settings`: valida, comprueba la ruta de git (si cambió), guarda y aplica.
    /// Las pestañas ya abiertas conservan su `git` hasta reabrirlas.
    pub async fn save_settings(&self, mut next: Settings) -> ApiResult<SettingsView> {
        next.validate()?;
        let current = self.settings_svc().snapshot();
        let mut new_slot = None;
        if next.git_path != current.git_path {
            let path = next.git_path.clone();
            let located = tokio::task::spawn_blocking(move || match path {
                Some(p) => locate_configured(&p).map_err(|e| (true, e)),
                None => super::app::locate_git().map_err(|e| (false, e)),
            })
            .await
            .map_err(|e| ApiError::io(e.to_string()))?;
            match located {
                Ok(install) => new_slot = Some(GitSlot::from_result(Ok(install))),
                // Una ruta elegida a mano que no sirve se rechaza; la detección automática
                // que falla solo deja constancia (la app sigue sin git, con su aviso).
                Err((true, why)) => {
                    return Err(ApiError::new(
                        "GIT_NOT_FOUND",
                        format!("La ruta de git no es válida. {why}"),
                        Some(why),
                    ));
                }
                Err((false, why)) => new_slot = Some(GitSlot::from_result(Err(why))),
            }
        }
        self.settings_svc().replace(next)?;
        if let Some(slot) = new_slot {
            *lock(&self.inner.gitslot) = slot;
            self.emit_tabs_changed();
        }
        Ok(self.get_settings())
    }

    /// `detect_git`: prueba una ruta (o busca automáticamente) y dice qué versión es.
    /// No guarda nada.
    pub async fn detect_git(&self, path: Option<String>) -> GitDetect {
        let path = path.map(|p| p.trim().to_owned()).filter(|p| !p.is_empty());
        let configured = path.is_some();
        let result = tokio::task::spawn_blocking(move || match path {
            Some(p) if !Path::new(&p).is_absolute() => Err(
                "La ruta debe ser absoluta (p. ej. C:\\Program Files\\Git\\cmd\\git.exe)."
                    .to_owned(),
            ),
            Some(p) => locate_configured(&p),
            None => super::app::locate_git(),
        })
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
        let source = if configured { "configured" } else { "auto" }.to_owned();
        match result {
            Ok(g) => GitDetect {
                ok: true,
                source,
                path: Some(shown(&g.path)),
                version: Some(g.version.to_string()),
                problem: None,
            },
            Err(p) => GitDetect {
                ok: false,
                source,
                path: None,
                version: None,
                problem: Some(p),
            },
        }
    }

    /// `git_identity`: `user.name` y `user.email` de la configuración de git (solo lectura).
    pub async fn git_identity(&self) -> GitIdentity {
        let runner = lock(&self.inner.gitslot).runner.clone();
        let Some(runner) = runner else {
            return GitIdentity {
                name: None,
                email: None,
            };
        };
        let cwd = std::env::temp_dir();
        let get = |key: &'static str| {
            let runner = runner.clone();
            let cwd = cwd.clone();
            async move {
                let out = runner
                    .run(
                        &cwd,
                        CommandKind::Read,
                        &["config".into(), "--get".into(), key.into()],
                        None,
                    )
                    .await
                    .ok()?;
                let v = out.stdout_lossy().trim().to_owned();
                (out.success() && !v.is_empty()).then_some(v)
            }
        };
        let (name, email) = tokio::join!(get("user.name"), get("user.email"));
        GitIdentity { name, email }
    }

    /// `get_cli_status`: ejecuta cada CLI de IA (`--version`) con el resolutor de `ariadna-ai`.
    pub async fn get_cli_status(&self) -> Vec<CliStatus> {
        let st = self.inner.ai.snapshot();
        self.ai_availability(&st)
            .await
            .into_iter()
            .map(|(id, a)| {
                let (availability, version, path, hint) = match &a {
                    Availability::Available { binary, version } => (
                        "available",
                        version_text(version),
                        Some(shown(binary)),
                        None,
                    ),
                    Availability::Missing { hint } => ("missing", None, None, Some(hint.clone())),
                    Availability::Failed { detail } => ("failed", None, None, Some(detail.clone())),
                };
                CliStatus {
                    id,
                    display_name: id.display_name().to_owned(),
                    installed: availability == "available",
                    availability: availability.to_owned(),
                    version,
                    path,
                    hint,
                    login_command: id.login_invocation().to_owned(),
                }
            })
            .collect()
    }

    /// Regla repo → perfil que aplica a la pestaña (por su remoto `origin`, o el primero).
    pub(crate) fn rule_for_tab(&self, tab_id: &str) -> Option<RuleMatch> {
        let (session, _) = self.live(tab_id).ok()?;
        let snap = session.snapshot();
        let remote = snap
            .remotes
            .iter()
            .find(|r| r.name == "origin")
            .or_else(|| snap.remotes.first())?;
        match_rule(
            &self.settings_svc().snapshot().profile_rules,
            &remote.fetch_url,
        )
    }

    /// Estado de `git` para `AppState`.
    pub(crate) fn git_state(&self) -> (Option<GitInfo>, Option<String>) {
        let slot = lock(&self.inner.gitslot);
        (
            slot.install.as_ref().map(|g| GitInfo {
                path: shown(&g.path),
                version: g.version.to_string(),
            }),
            slot.problem.clone(),
        )
    }

    pub(crate) fn git_runner(&self) -> Option<GitRunner> {
        lock(&self.inner.gitslot).runner.clone()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn rule(p: &str, k: ProfileKind) -> ProfileRule {
        ProfileRule {
            pattern: p.into(),
            profile: k,
        }
    }

    #[test]
    fn valores_por_defecto_son_validos() {
        let mut s = Settings::default();
        s.validate().unwrap();
        assert_eq!(s, Settings::default());
        assert_eq!(s.profile_rules.len(), 2);
    }

    #[test]
    fn validacion_estricta_de_cada_campo() {
        let field = |mut s: Settings| s.validate().unwrap_err().detail.unwrap();
        let s = |f: &dyn Fn(&mut Settings)| {
            let mut x = Settings::default();
            f(&mut x);
            x
        };
        assert_eq!(field(s(&|x| x.language = "fr".into())), "language");
        // Los tres valores del contrato son válidos (y se normalizan a minúsculas).
        for ok in ["es", "en", "system", " EN "] {
            let mut x = s(&|x| x.language = ok.into());
            x.validate().unwrap();
            assert_eq!(x.language, ok.trim().to_lowercase());
        }
        assert_eq!(field(s(&|x| x.diff_font_size = 9)), "diffFontSize");
        assert_eq!(field(s(&|x| x.diff_font_size = 25)), "diffFontSize");
        assert_eq!(field(s(&|x| x.git_path = Some("git".into()))), "gitPath");
        assert_eq!(
            field(s(&|x| x.git_path = Some("rel/git".into()))),
            "gitPath"
        );
        assert_eq!(
            field(s(
                &|x| x.profile_rules = vec![rule("", ProfileKind::Empresa)]
            )),
            "profileRules"
        );
        for malo in [
            "https://github.com",
            "user@github.com",
            "github.com/a b",
            "github",
            "github.com//x",
            ".github.com",
            "git hub.com",
        ] {
            assert_eq!(
                field(s(
                    &|x| x.profile_rules = vec![rule(malo, ProfileKind::Personal)]
                )),
                "profileRules",
                "{malo}"
            );
        }
        let dup = vec![
            rule("github.com", ProfileKind::Personal),
            rule("GitHub.com/", ProfileKind::Empresa),
        ];
        assert_eq!(field(s(&|x| x.profile_rules = dup.clone())), "profileRules");
        let many = (0..=MAX_RULES)
            .map(|i| rule(&format!("h{i}.com"), ProfileKind::Personal))
            .collect::<Vec<_>>();
        assert_eq!(
            field(s(&|x| x.profile_rules = many.clone())),
            "profileRules"
        );
    }

    #[test]
    fn normaliza_patrones_y_ruta_vacia() {
        let mut s = Settings {
            git_path: Some("   ".into()),
            profile_rules: vec![rule("  Dev.Azure.com/Mi-Org/ ", ProfileKind::Empresa)],
            ..Settings::default()
        };
        s.validate().unwrap();
        assert_eq!(s.git_path, None);
        assert_eq!(s.profile_rules[0].pattern, "dev.azure.com/mi-org");
    }

    #[test]
    fn reglas_por_remoto_la_mas_especifica_gana() {
        let rules = vec![
            rule("dev.azure.com", ProfileKind::Empresa),
            rule("github.com", ProfileKind::Personal),
            rule("github.com/mi-empresa", ProfileKind::Empresa),
        ];
        let m = |u: &str| match_rule(&rules, u).map(|m| (m.kind, m.pattern));
        assert_eq!(
            m("https://dev.azure.com/acme/proj/_git/repo"),
            Some((ProfileKind::Empresa, "dev.azure.com".into()))
        );
        assert_eq!(
            m("git@ssh.dev.azure.com:v3/acme/proj/repo"),
            Some((ProfileKind::Empresa, "dev.azure.com".into()))
        );
        assert_eq!(
            m("https://github.com/Javier/ariadna.git"),
            Some((ProfileKind::Personal, "github.com".into()))
        );
        assert_eq!(
            m("git@github.com:Mi-Empresa/api.git"),
            Some((ProfileKind::Empresa, "github.com/mi-empresa".into()))
        );
        // `github.com/mi-empresa-2` NO cae bajo `github.com/mi-empresa` (se compara por segmentos).
        assert_eq!(
            m("https://github.com/mi-empresa-2/x.git"),
            Some((ProfileKind::Personal, "github.com".into()))
        );
        assert_eq!(m("https://gitlab.com/a/b.git"), None);
        assert_eq!(m(""), None);
        assert_eq!(m("/ruta/local/repo"), None);
    }

    fn store() -> (tempfile::TempDir, SettingsStore) {
        let d = tempfile::tempdir().unwrap();
        let s = SettingsStore::new(d.path().join("data").join("settings.json"));
        (d, s)
    }

    #[test]
    fn sin_archivo_son_los_valores_por_defecto() {
        let (_d, s) = store();
        let (v, info) = s.load();
        assert_eq!(v, Settings::default());
        assert_eq!(info.status, SettingsLoadStatus::Missing);
        assert_eq!(info.warning, None);
    }

    #[test]
    fn guardar_y_cargar_ida_y_vuelta_sin_restos_tmp() {
        let (_d, s) = store();
        let mut v = Settings {
            theme: Theme::Dark,
            pull_rebase: true,
            diff_font_size: 15,
            ..Settings::default()
        };
        v.validate().unwrap();
        s.save(&v).unwrap();
        s.save(&v).unwrap(); // sobrescribir sobre un archivo existente
        let (back, info) = s.load();
        assert_eq!(back, v);
        assert_eq!(info.status, SettingsLoadStatus::Ok);
        assert!(!sibling(s.file(), ".tmp").exists());
        let text = fs::read_to_string(s.file()).unwrap();
        assert!(text.contains("\"version\": 1"));
        assert!(text.contains("\"pullRebase\": true"));
    }

    #[test]
    fn archivo_corrupto_se_aparta_como_bak_y_no_rompe_el_arranque() {
        for contenido in [
            &b"{ esto no es json"[..],
            b"",
            b"[1,2,3]",
            b"{\"theme\":\"fucsia\"}",
            b"{\"diffFontSize\":99}",
            b"{\"version\":1,\"pullRebase\":\"si\"}",
            b"{\"version\":99}",
            &[0xff, 0xfe, 0x00][..],
        ] {
            let (_d, s) = store();
            fs::create_dir_all(s.file().parent().unwrap()).unwrap();
            fs::write(s.file(), contenido).unwrap();
            let (v, info) = s.load();
            assert_eq!(v, Settings::default(), "{contenido:?}");
            assert_eq!(info.status, SettingsLoadStatus::ResetCorrupt);
            let w = info.warning.unwrap();
            assert!(w.contains("settings.json.bak"), "{w}");
            let bak = sibling(s.file(), ".bak");
            assert_eq!(
                fs::read(&bak).unwrap(),
                contenido,
                "la copia conserva el original"
            );
            assert!(!s.file().exists());
            // Y se puede volver a guardar con normalidad.
            s.save(&v).unwrap();
            assert_eq!(s.load().1.status, SettingsLoadStatus::Ok);
        }
    }

    #[test]
    fn migra_la_version_anterior_y_completa_los_campos() {
        let (_d, s) = store();
        fs::create_dir_all(s.file().parent().unwrap()).unwrap();
        fs::write(s.file(), br#"{"pullRebase": true, "theme": "light"}"#).unwrap();
        let (v, info) = s.load();
        assert_eq!(info.status, SettingsLoadStatus::Migrated);
        assert!(v.pull_rebase);
        assert_eq!(v.theme, Theme::Light);
        assert_eq!(v.diff_font_size, 13);
        // Quedó reescrito en el esquema actual.
        let (again, info2) = s.load();
        assert_eq!(again, v);
        assert_eq!(info2.status, SettingsLoadStatus::Ok);
        assert!(fs::read_to_string(s.file())
            .unwrap()
            .contains("\"version\": 1"));
    }

    #[test]
    fn campos_desconocidos_se_toleran() {
        let (_d, s) = store();
        fs::create_dir_all(s.file().parent().unwrap()).unwrap();
        fs::write(
            s.file(),
            br#"{"version":1,"futuro":{"a":1},"pullRebase":true}"#,
        )
        .unwrap();
        let (v, info) = s.load();
        assert_eq!(info.status, SettingsLoadStatus::Ok);
        assert!(v.pull_rebase);
    }

    #[test]
    fn locate_configured_rechaza_lo_que_no_es_git() {
        let d = tempfile::tempdir().unwrap();
        let falso = d.path().join(if cfg!(windows) { "git.exe" } else { "git" });
        fs::write(&falso, b"no soy git").unwrap();
        assert!(locate_configured(&falso.to_string_lossy()).is_err());
        let msg = locate_configured(&d.path().join("nada").to_string_lossy()).unwrap_err();
        assert!(msg.contains("No hay un ejecutable de git"), "{msg}");
    }

    #[test]
    fn servicio_no_cambia_nada_si_no_puede_guardar() {
        let d = tempfile::tempdir().unwrap();
        // La «carpeta de datos» es un archivo: guardar falla.
        let bloqueo = d.path().join("data");
        fs::write(&bloqueo, b"x").unwrap();
        let svc = SettingsService::new(&bloqueo);
        let mut cambio = svc.snapshot();
        cambio.pull_rebase = true;
        assert_eq!(svc.replace(cambio).unwrap_err().code, "IO");
        assert!(!svc.snapshot().pull_rebase);
        let _ = svc.file();
    }
}
