//! Tipos de dominio de la capa de IA: proveedores, sugerencias, errores y ajustes.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Tope de contexto por defecto (spec §5.1): 100 000 bytes.
pub const DEFAULT_MAX_CONTEXT_BYTES: usize = 100_000;

/// Proveedor concreto de IA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Claude,
    Codex,
    Copilot,
}

impl ProviderId {
    /// Orden de recorrido del modo Auto (RF-IA9): Claude → Codex → Copilot.
    pub const AUTO_ORDER: [ProviderId; 3] =
        [ProviderId::Claude, ProviderId::Codex, ProviderId::Copilot];

    /// Nombre para la interfaz.
    pub fn display_name(self) -> &'static str {
        match self {
            ProviderId::Claude => "Claude Code",
            ProviderId::Codex => "Codex CLI",
            ProviderId::Copilot => "GitHub Copilot CLI",
        }
    }

    /// Nombre del ejecutable (sin extensión).
    pub fn binary_name(self) -> &'static str {
        match self {
            ProviderId::Claude => "claude",
            ProviderId::Codex => "codex",
            ProviderId::Copilot => "copilot",
        }
    }

    /// Qué debe hacer el usuario para iniciar sesión (lo hace siempre el CLI).
    pub fn login_command(self) -> &'static str {
        match self {
            ProviderId::Claude => "claude auth login",
            ProviderId::Codex => "codex login",
            ProviderId::Copilot => "copilot login",
        }
    }

    /// Comando exacto que hay que ejecutar para iniciar sesión (sin explicaciones),
    /// para mostrarlo tal cual en la interfaz.
    pub fn login_invocation(self) -> &'static str {
        match self {
            ProviderId::Claude => "claude auth login",
            ProviderId::Codex => "codex login",
            ProviderId::Copilot => "copilot login",
        }
    }

    /// Versión del CLI con la que se verificaron los flags, el transporte del prompt y el
    /// formato de salida. `None` = proveedor **no verificado** (se publica como experimental).
    pub fn verified_with(self) -> Option<&'static str> {
        match self {
            ProviderId::Claude => Some("claude 2.1.319"),
            ProviderId::Copilot => Some("GitHub Copilot CLI 1.0.112"),
            ProviderId::Codex => None,
        }
    }

    /// Fecha (`YYYY-MM-DD`) de la verificación de [`ProviderId::verified_with`].
    pub fn verified_on(self) -> Option<&'static str> {
        match self {
            ProviderId::Claude => Some("2026-11-11"),
            ProviderId::Copilot => Some("2026-11-14"),
            ProviderId::Codex => None,
        }
    }

    /// `true` si el proveedor no tiene flags verificados y se publica como experimental.
    pub fn is_experimental(self) -> bool {
        self.verified_with().is_none()
    }
}

/// Elección del usuario: un proveedor o Auto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderChoice {
    Claude,
    Codex,
    Copilot,
    Auto,
}

impl ProviderChoice {
    /// El proveedor concreto elegido, o `None` en Auto.
    pub fn provider(self) -> Option<ProviderId> {
        match self {
            ProviderChoice::Claude => Some(ProviderId::Claude),
            ProviderChoice::Codex => Some(ProviderId::Codex),
            ProviderChoice::Copilot => Some(ProviderId::Copilot),
            ProviderChoice::Auto => None,
        }
    }
}

/// Idioma del mensaje de commit. `Auto` imita el de los últimos commits del repo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitLanguage {
    #[default]
    Auto,
    Es,
    En,
}

/// Estilo del mensaje de commit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitStyle {
    #[default]
    Plain,
    /// Conventional Commits (`tipo(ámbito): descripción`).
    Conventional,
}

/// Opciones para pedir un mensaje de commit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitOptions {
    pub template: Option<String>,
    pub language: CommitLanguage,
    pub style: CommitStyle,
    /// Asuntos de los últimos commits. Solo se envían con `language = Auto`.
    pub recent_subjects: Vec<String>,
}

/// Qué pidió el usuario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SuggestionKind {
    CommitMessage,
    SummarizeDiff,
    ExplainCommit,
    ConflictResolution,
    /// Título (en `subject`) y descripción (en `body`) de una pull request.
    PullRequest,
}

/// Estado de una propuesta. Cambiarlo NO escribe en el repo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionStatus {
    Pending,
    Accepted,
    Rejected,
}

/// Estilo del resumen de un diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SummaryStyle {
    Bullets,
    PullRequest,
}

static SEQ: AtomicU64 = AtomicU64::new(1);

/// Milisegundos desde la época Unix (0 si el reloj es anterior).
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Identificador único de proceso para una propuesta.
pub fn new_suggestion_id() -> String {
    format!(
        "ai-{:x}-{}",
        now_millis(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// Propuesta de la IA. Nace en `Pending` y vive en memoria.
///
/// No hay método `apply`: cambiar el estado no toca el repositorio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSuggestion {
    pub id: String,
    pub kind: SuggestionKind,
    pub provider: ProviderId,
    /// Resumen del comando lanzado. **Nunca** incluye el prompt, solo su tamaño.
    pub command_summary: String,
    pub subject: Option<String>,
    pub body: String,
    pub rationale: Option<String>,
    /// Texto de archivo propuesto (conflictos); sigue en memoria hasta que el usuario escriba.
    pub proposed_content: Option<String>,
    /// Milisegundos desde la época Unix.
    pub created_at: u64,
    pub status: SuggestionStatus,
    /// Revisión del repo sobre la que se generó (spec §5).
    pub repo_revision: String,
    /// Nota informativa (p. ej. tokens usados).
    pub transparency_note: Option<String>,
}

impl AiSuggestion {
    /// `true` si el repo ha cambiado desde que se generó la propuesta.
    pub fn is_stale(&self, current_revision: &str) -> bool {
        self.repo_revision != current_revision
    }

    /// Copia con otro estado. No hace E/S.
    pub fn with_status(&self, status: SuggestionStatus) -> Self {
        Self {
            status,
            ..self.clone()
        }
    }

    /// Texto listo para el cuadro de mensaje de commit.
    pub fn commit_message_text(&self) -> String {
        match self.subject.as_deref() {
            Some(s) if !s.is_empty() => {
                if self.body.is_empty() || self.body == s {
                    s.to_string()
                } else {
                    format!("{s}\n\n{}", self.body)
                }
            }
            _ => self.body.clone(),
        }
    }
}

/// Resultado de comprobar un CLI ejecutando `--version` (RF-IA4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    Available { binary: PathBuf, version: String },
    Missing { hint: String },
    Failed { detail: String },
}

impl Availability {
    pub fn is_available(&self) -> bool {
        matches!(self, Availability::Available { .. })
    }

    /// Texto de ayuda para la interfaz (vacío si está disponible).
    pub fn hint(&self) -> &str {
        match self {
            Availability::Available { .. } => "",
            Availability::Missing { hint } => hint,
            Availability::Failed { detail } => detail,
        }
    }
}

/// Estado de la sesión de un proveedor según el propio CLI (RF-IA12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountState {
    /// El CLI confirma que hay sesión.
    LoggedIn,
    /// El CLI confirma que no hay sesión.
    LoggedOut,
    /// No se sabe: el CLI no está, no expone la cuenta o su salida no es reconocible.
    Unavailable,
}

/// Cuenta activa de un proveedor, **solo** lo que el CLI cuenta por su subcomando de estado.
/// Ariadna no lee `.credentials.json`, llaveros ni variables con tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub provider: ProviderId,
    pub state: AccountState,
    pub email: Option<String>,
    pub organization: Option<String>,
    pub plan: Option<String>,
    /// Método de inicio de sesión que declara el CLI (`claude.ai`, `console`…).
    pub auth_method: Option<String>,
    /// Directorio de configuración que declara el CLI que está usando (prueba de aislamiento).
    pub config_dir: Option<String>,
    /// Por qué no hay datos (`state = Unavailable`) o nota para el usuario.
    pub reason: Option<String>,
}

impl AccountInfo {
    pub fn unavailable(provider: ProviderId, reason: impl Into<String>) -> Self {
        Self {
            provider,
            state: AccountState::Unavailable,
            email: None,
            organization: None,
            plan: None,
            auth_method: None,
            config_dir: None,
            reason: Some(reason.into()),
        }
    }
}

/// Errores tipados de la capa de IA. Ninguno contiene el prompt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AiError {
    #[error("{} no está instalado. {hint}", provider.display_name())]
    NotInstalled { provider: ProviderId, hint: String },
    #[error("{} no tiene sesión. Ejecuta `{login_command}` y vuelve a intentarlo.", provider.display_name())]
    NotLoggedIn {
        provider: ProviderId,
        login_command: String,
    },
    #[error("{} ha agotado su cuota o límite de uso. {detail}", provider.display_name())]
    Quota {
        provider: ProviderId,
        detail: String,
    },
    #[error("Respuesta de la IA no utilizable: {0}")]
    BadOutput(String),
    #[error("El contexto ocupa {bytes} bytes y el límite es {max_bytes}. Acorta el cambio o sube el límite en Ajustes. No se ha enviado nada.")]
    ContextTooLarge { bytes: usize, max_bytes: usize },
    #[error("{} tardó demasiado y se canceló la espera.", provider.display_name())]
    Timeout { provider: ProviderId },
    #[error("Petición a la IA cancelada.")]
    Cancelled,
    #[error("Falta el consentimiento para enviar contexto a {}.", provider.display_name())]
    ConsentRequired { provider: ProviderId },
    #[error("{} está desactivado en este perfil.", provider.display_name())]
    ProviderDisabled { provider: ProviderId },
    #[error("La IA está desactivada en los ajustes.")]
    AiDisabled,
    #[error("No hay un proveedor de IA disponible. {}", hints.join(" "))]
    NoProviderAvailable { hints: Vec<String> },
    #[error("{} terminó con error{}. {detail}", provider.display_name(), exit_code.map(|c| format!(" (código {c})")).unwrap_or_default())]
    ProviderFailed {
        provider: ProviderId,
        exit_code: Option<i32>,
        detail: String,
    },
    #[error("Propuesta de conflicto rechazada: {0}")]
    ConflictProposalRejected(String),
    #[error("El fichero {0} es sensible (.env, claves...) y no se envía a la IA.")]
    SensitiveFile(String),
    #[error("Demasiadas peticiones a la IA; espera {retry_after_secs} s.")]
    RateLimited { retry_after_secs: u64 },
    #[error("Error de E/S: {0}")]
    Io(String),
}

/// Política de tamaño del contexto. Si se supera, **no se llama al CLI**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiContextPolicy {
    pub max_bytes: usize,
}

impl Default for AiContextPolicy {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CONTEXT_BYTES)
    }
}

impl AiContextPolicy {
    /// El tope mínimo es 1 byte.
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes: max_bytes.max(1),
        }
    }

    /// Error [`AiError::ContextTooLarge`] si `payload` supera el tope.
    pub fn validate(&self, payload: &str) -> Result<(), AiError> {
        self.validate_len(payload.len())
    }

    /// Igual que [`validate`](Self::validate) pero con el tamaño ya calculado.
    pub fn validate_len(&self, bytes: usize) -> Result<(), AiError> {
        if bytes > self.max_bytes {
            Err(AiError::ContextTooLarge {
                bytes,
                max_bytes: self.max_bytes,
            })
        } else {
            Ok(())
        }
    }
}

/// Ajustes globales de IA (persistidos como JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiSettings {
    /// Interruptor global (D10): apagada hasta que el usuario consienta.
    pub enabled: bool,
    pub provider: ProviderChoice,
    pub max_context_bytes: usize,
    /// Ruta configurable del binario de cada CLI; `None` = buscarlo.
    pub claude_binary: Option<PathBuf>,
    pub codex_binary: Option<PathBuf>,
    pub copilot_binary: Option<PathBuf>,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: ProviderChoice::Auto,
            max_context_bytes: DEFAULT_MAX_CONTEXT_BYTES,
            claude_binary: None,
            codex_binary: None,
            copilot_binary: None,
        }
    }
}

impl AiSettings {
    /// Ruta configurada para un proveedor.
    pub fn binary_for(&self, p: ProviderId) -> Option<&Path> {
        match p {
            ProviderId::Claude => self.claude_binary.as_deref(),
            ProviderId::Codex => self.codex_binary.as_deref(),
            ProviderId::Copilot => self.copilot_binary.as_deref(),
        }
    }

    /// Carga los ajustes; si el fichero no existe devuelve los valores por defecto.
    pub fn load(path: &Path) -> Result<Self, AiError> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| AiError::Io(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(AiError::Io(e.to_string())),
        }
    }

    /// Guarda los ajustes creando los directorios que falten.
    pub fn save(&self, path: &Path) -> Result<(), AiError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AiError::Io(e.to_string()))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| AiError::Io(e.to_string()))?;
        std::fs::write(path, json).map_err(|e| AiError::Io(e.to_string()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sample() -> AiSuggestion {
        AiSuggestion {
            id: new_suggestion_id(),
            kind: SuggestionKind::CommitMessage,
            provider: ProviderId::Claude,
            command_summary: "claude -p".into(),
            subject: Some("feat: x".into()),
            body: "cuerpo".into(),
            rationale: None,
            proposed_content: None,
            created_at: now_millis(),
            status: SuggestionStatus::Pending,
            repo_revision: "rev1".into(),
            transparency_note: None,
        }
    }

    #[test]
    fn obsoleta_si_cambia_la_revision() {
        let s = sample();
        assert!(!s.is_stale("rev1"));
        assert!(s.is_stale("rev2"));
    }

    #[test]
    fn with_status_no_toca_el_resto() {
        let s = sample();
        let a = s.with_status(SuggestionStatus::Accepted);
        assert_eq!(a.status, SuggestionStatus::Accepted);
        assert_eq!(a.id, s.id);
        assert_eq!(s.status, SuggestionStatus::Pending);
    }

    #[test]
    fn texto_de_commit() {
        let mut s = sample();
        assert_eq!(s.commit_message_text(), "feat: x\n\ncuerpo");
        s.body.clear();
        assert_eq!(s.commit_message_text(), "feat: x");
        s.subject = None;
        s.body = "solo".into();
        assert_eq!(s.commit_message_text(), "solo");
    }

    #[test]
    fn ids_distintos() {
        assert_ne!(new_suggestion_id(), new_suggestion_id());
    }

    #[test]
    fn politica_de_contexto() {
        let p = AiContextPolicy::new(8);
        assert!(p.validate("12345678").is_ok());
        assert_eq!(
            p.validate("123456789"),
            Err(AiError::ContextTooLarge {
                bytes: 9,
                max_bytes: 8
            })
        );
        assert_eq!(AiContextPolicy::default().max_bytes, 100_000);
        assert_eq!(AiContextPolicy::new(0).max_bytes, 1);
    }

    #[test]
    fn login_command_por_proveedor() {
        assert_eq!(ProviderId::Claude.login_command(), "claude auth login");
        assert_eq!(ProviderId::Codex.login_command(), "codex login");
        assert_eq!(ProviderId::Copilot.login_command(), "copilot login");
        assert_eq!(ProviderId::Claude.login_invocation(), "claude auth login");
        assert_eq!(ProviderId::Codex.login_invocation(), "codex login");
        assert_eq!(ProviderId::Copilot.login_invocation(), "copilot login");
        assert!(ProviderId::Codex.is_experimental());
        assert!(!ProviderId::Claude.is_experimental());
        // Copilot dejó de ser experimental el 2026-11-14 (GitHub Copilot CLI 1.0.112).
        assert!(!ProviderId::Copilot.is_experimental());
        assert_eq!(
            ProviderId::Copilot.verified_with(),
            Some("GitHub Copilot CLI 1.0.112")
        );
        assert_eq!(ProviderId::Copilot.verified_on(), Some("2026-11-14"));
        assert_eq!(ProviderId::Codex.verified_with(), None);
        assert_eq!(ProviderId::Codex.verified_on(), None);
        assert!(ProviderId::Claude.verified_with().is_some());
        for p in ProviderId::AUTO_ORDER {
            assert_eq!(p.is_experimental(), p.verified_with().is_none());
        }
    }

    #[test]
    fn eleccion_de_proveedor() {
        assert_eq!(ProviderChoice::Auto.provider(), None);
        assert_eq!(ProviderChoice::Codex.provider(), Some(ProviderId::Codex));
    }

    #[test]
    fn errores_se_muestran_sin_prompt() {
        let e = AiError::NotLoggedIn {
            provider: ProviderId::Codex,
            login_command: "codex login".into(),
        };
        assert!(e.to_string().contains("codex login"));
    }

    #[test]
    fn ajustes_ida_y_vuelta_y_defecto() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("ai.json");
        assert_eq!(AiSettings::load(&path).unwrap(), AiSettings::default());
        let s = AiSettings {
            provider: ProviderChoice::Codex,
            max_context_bytes: 2048,
            ..AiSettings::default()
        };
        s.save(&path).unwrap();
        assert_eq!(AiSettings::load(&path).unwrap(), s);
    }
}
