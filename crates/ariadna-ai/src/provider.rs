//! Trait [`AiProvider`] e implementación genérica [`CliProvider`] sobre `ariadna_process`.
//!
//! Garantías de [`CliProvider`]: prompt por stdin (RF-IA1), entorno por lista blanca
//! (RF-IA3), `cwd` = carpeta vacía dedicada borrada tras cada llamada (RF-IA5), sin TTY,
//! timeout duro de 120 s con cancelación y tope de salida (RF-IA6). La disponibilidad
//! se detecta ejecutando `--version` (RF-IA4); nunca se leen credenciales.
//! **No se registra el prompt en ningún sitio.**

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use ariadna_process::{CancellationToken, ProcessError, ProcessSpec};
use async_trait::async_trait;

use crate::commands::{account_args, build_args, command_summary, prompt_via_stdin, version_args};
use crate::env::{provider_env, EnvOptions};
use crate::models::{
    new_suggestion_id, now_millis, AccountInfo, AiContextPolicy, AiError, AiSuggestion,
    Availability, CommitOptions, ProviderId, SuggestionKind, SuggestionStatus, SummaryStyle,
};
use crate::parse::{
    error_from_failed_run, parse_claude_auth_status, parse_conflict_proposal, parse_output,
    split_commit_message, strip_fences, ParsedReply,
};
use crate::prompt::{
    validate_conflict_proposal, ConflictInput, PromptBuilder, PromptPreview, PullRequestInput,
};
use crate::resolve::{resolve_binary, ResolvedBinary, SearchEnv};

/// Timeout duro de una llamada (RF-IA6).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
/// Timeout de `--version`.
pub const VERSION_TIMEOUT: Duration = Duration::from_secs(20);
/// Tope de bytes de stdout/stderr que se conservan.
pub const DEFAULT_MAX_OUTPUT: usize = 1024 * 1024;

/// Datos de una llamada concreta.
#[derive(Debug, Clone)]
pub struct CallContext {
    /// Revisión del repo sobre la que se pide (para marcar la propuesta como obsoleta).
    pub repo_revision: String,
    /// Cancela la llamada (cierre de pestaña, botón «Cancelar»).
    pub cancel: CancellationToken,
    /// Directorio de configuración aislado del perfil para este proveedor.
    pub config_dir: Option<PathBuf>,
    /// Tope de contexto del perfil; si es `None`, el de la política del proveedor.
    pub max_context_bytes: Option<usize>,
}

impl CallContext {
    pub fn new(repo_revision: impl Into<String>) -> Self {
        Self {
            repo_revision: repo_revision.into(),
            cancel: CancellationToken::new(),
            config_dir: None,
            max_context_bytes: None,
        }
    }

    pub fn with_cancel(mut self, cancel: CancellationToken) -> Self {
        self.cancel = cancel;
        self
    }
}

/// Un proveedor de IA. Ningún método escribe en el repo: solo devuelve propuestas.
#[async_trait]
pub trait AiProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    async fn availability(&self) -> Availability;
    async fn suggest_commit_message(
        &self,
        ctx: &CallContext,
        diff: &str,
        template: Option<&str>,
    ) -> Result<AiSuggestion, AiError>;
    /// Como [`suggest_commit_message`](Self::suggest_commit_message) pero con idioma, estilo
    /// y asuntos recientes. Por defecto ignora todo salvo la plantilla.
    async fn suggest_commit_message_with(
        &self,
        ctx: &CallContext,
        diff: &str,
        opts: &CommitOptions,
    ) -> Result<AiSuggestion, AiError> {
        self.suggest_commit_message(ctx, diff, opts.template.as_deref())
            .await
    }
    /// Título (en `subject`) y descripción (en `body`) de una pull request. Por defecto no
    /// está soportado.
    async fn suggest_pull_request(
        &self,
        _ctx: &CallContext,
        _input: &PullRequestInput,
    ) -> Result<AiSuggestion, AiError> {
        Err(AiError::Io(
            "este proveedor no sabe describir pull requests".into(),
        ))
    }
    /// Cuenta activa según el propio CLI (RF-IA12), con el directorio de configuración del
    /// perfil. Por defecto «no disponible».
    async fn account(&self, _config_dir: Option<&Path>) -> AccountInfo {
        AccountInfo::unavailable(self.id(), "Este CLI no expone la cuenta activa.")
    }
    async fn summarize_diff(
        &self,
        ctx: &CallContext,
        diff: &str,
        style: SummaryStyle,
    ) -> Result<AiSuggestion, AiError>;
    async fn explain_commit(
        &self,
        ctx: &CallContext,
        sha: &str,
        patch: &str,
        subject: &str,
    ) -> Result<AiSuggestion, AiError>;
    async fn propose_conflict_resolution(
        &self,
        ctx: &CallContext,
        conflict: &ConflictInput,
    ) -> Result<AiSuggestion, AiError>;
}

/// Configuración de un [`CliProvider`].
#[derive(Debug, Clone)]
pub struct CliProviderConfig {
    pub id: ProviderId,
    /// Ruta del binario fijada en Ajustes.
    pub configured_binary: Option<PathBuf>,
    pub search: SearchEnv,
    /// Carpeta base de las carpetas de trabajo vacías (`…/Ariadna/ai-sandbox`).
    pub sandbox_dir: PathBuf,
    pub policy: AiContextPolicy,
    pub timeout: Duration,
    pub version_timeout: Duration,
    pub max_output_bytes: usize,
    pub env: EnvOptions,
    /// Variables fijas añadidas tras la lista blanca (ganchos de prueba).
    pub extra_env: Vec<(OsString, OsString)>,
}

impl CliProviderConfig {
    pub fn new(id: ProviderId, sandbox_dir: impl Into<PathBuf>) -> Self {
        Self {
            id,
            configured_binary: None,
            search: SearchEnv::from_process(),
            sandbox_dir: sandbox_dir.into(),
            policy: AiContextPolicy::default(),
            timeout: DEFAULT_TIMEOUT,
            version_timeout: VERSION_TIMEOUT,
            max_output_bytes: DEFAULT_MAX_OUTPUT,
            env: EnvOptions::default(),
            extra_env: Vec::new(),
        }
    }
}

/// Proveedor que lanza el CLI oficial.
#[derive(Debug, Clone)]
pub struct CliProvider {
    cfg: CliProviderConfig,
}

static CALL_SEQ: AtomicU64 = AtomicU64::new(0);

/// Carpeta vacía de una llamada; se borra al soltarla, también si hay error o cancelación.
struct CallDir(PathBuf);

impl CallDir {
    fn create(base: &Path) -> Result<Self, AiError> {
        let dir = base.join(format!(
            "call-{}-{}",
            std::process::id(),
            CALL_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).map_err(|e| AiError::Io(e.to_string()))?;
        Ok(Self(dir))
    }
}

impl Drop for CallDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl CliProvider {
    pub fn new(cfg: CliProviderConfig) -> Self {
        Self { cfg }
    }

    pub fn config(&self) -> &CliProviderConfig {
        &self.cfg
    }

    fn resolve(&self) -> Result<ResolvedBinary, AiError> {
        let id = self.cfg.id;
        resolve_binary(id, self.cfg.configured_binary.as_deref(), &self.cfg.search).ok_or_else(
            || AiError::NotInstalled {
                provider: id,
                hint: format!(
                    "No encuentro `{}`. Instálalo y ejecuta `{}`.",
                    id.binary_name(),
                    id.login_command()
                ),
            },
        )
    }

    fn spec(
        &self,
        resolved: &ResolvedBinary,
        args: &[OsString],
        cwd: &Path,
        ctx: Option<&CallContext>,
    ) -> Result<ProcessSpec, AiError> {
        let (program, args) = resolved
            .command(args)
            .map_err(|e| AiError::Io(e.to_string()))?;
        let mut opts = self.cfg.env.clone();
        if let Some(dir) = ctx.and_then(|c| c.config_dir.clone()) {
            opts.config_dir = Some(dir);
        }
        let penv = provider_env(self.cfg.id, &opts);
        let mut extra = penv.extra;
        extra.extend(self.cfg.extra_env.iter().cloned());
        let mut spec = ProcessSpec::new(program)
            .args(args)
            .cwd(cwd)
            .env_policy(penv.policy)
            .max_output_bytes(self.cfg.max_output_bytes);
        spec.extra_env = extra;
        Ok(spec)
    }

    fn map_process_error(&self, e: ProcessError) -> AiError {
        match e {
            ProcessError::Spawn { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
                AiError::NotInstalled {
                    provider: self.cfg.id,
                    hint: format!("No encuentro `{}`.", self.cfg.id.binary_name()),
                }
            }
            ProcessError::Spawn { source, .. } => AiError::ProviderFailed {
                provider: self.cfg.id,
                exit_code: None,
                detail: source.to_string(),
            },
            ProcessError::Io(e) => AiError::Io(e.to_string()),
        }
    }

    /// Valida el contexto, lanza el CLI con el prompt por stdin y parsea la respuesta.
    async fn complete(
        &self,
        ctx: &CallContext,
        preview: &PromptPreview,
    ) -> Result<ParsedReply, AiError> {
        let id = self.cfg.id;
        let policy = ctx
            .max_context_bytes
            .map(AiContextPolicy::new)
            .unwrap_or(self.cfg.policy);
        // Antes de nada: si se supera el tope no se llama al CLI.
        policy.validate_len(preview.context_bytes)?;
        if ctx.cancel.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        if !prompt_via_stdin(id) {
            // SPIKE: si un CLI no lee stdin habría que reducir el contexto (RF-IA1).
            return Err(AiError::Io(
                "este proveedor no lee el prompt por stdin".into(),
            ));
        }
        let resolved = self.resolve()?;
        let dir = CallDir::create(&self.cfg.sandbox_dir)?;
        let mut spec = self.spec(&resolved, &build_args(id), &dir.0, Some(ctx))?;
        spec = spec
            .stdin(preview.text.as_bytes().to_vec())
            .timeout(Some(self.cfg.timeout));

        let out = ariadna_process::run(spec, ctx.cancel.clone())
            .await
            .map_err(|e| self.map_process_error(e))?;
        drop(dir);

        if out.cancelled {
            return Err(AiError::Cancelled);
        }
        if out.timed_out {
            return Err(AiError::Timeout { provider: id });
        }
        let (stdout, stderr) = (out.stdout_lossy(), out.stderr_lossy());
        if !out.success() {
            return Err(error_from_failed_run(id, out.code, &stdout, &stderr));
        }
        if out.truncated {
            return Err(AiError::BadOutput(
                "la salida superó el tope y se descartó".into(),
            ));
        }
        parse_output(id, &stdout)
    }

    #[allow(clippy::too_many_arguments)]
    fn make(
        &self,
        ctx: &CallContext,
        kind: SuggestionKind,
        preview: &PromptPreview,
        subject: Option<String>,
        body: String,
        rationale: Option<String>,
        proposed_content: Option<String>,
        note: Option<String>,
    ) -> AiSuggestion {
        AiSuggestion {
            id: new_suggestion_id(),
            kind,
            provider: self.cfg.id,
            command_summary: command_summary(self.cfg.id, preview.total_bytes()),
            subject,
            body,
            rationale,
            proposed_content,
            created_at: now_millis(),
            status: SuggestionStatus::Pending,
            repo_revision: ctx.repo_revision.clone(),
            transparency_note: note,
        }
    }
}

#[async_trait]
impl AiProvider for CliProvider {
    fn id(&self) -> ProviderId {
        self.cfg.id
    }

    async fn availability(&self) -> Availability {
        let id = self.cfg.id;
        let resolved = match self.resolve() {
            Ok(r) => r,
            Err(e) => {
                return Availability::Missing {
                    hint: e.to_string(),
                }
            }
        };
        let dir = match CallDir::create(&self.cfg.sandbox_dir) {
            Ok(d) => d,
            Err(e) => {
                return Availability::Failed {
                    detail: e.to_string(),
                }
            }
        };
        let spec = match self.spec(&resolved, &version_args(), &dir.0, None) {
            Ok(s) => s.timeout(Some(self.cfg.version_timeout)),
            Err(e) => {
                return Availability::Failed {
                    detail: e.to_string(),
                }
            }
        };
        match ariadna_process::run(spec, CancellationToken::new()).await {
            Err(e) => match self.map_process_error(e) {
                AiError::NotInstalled { hint, .. } => Availability::Missing { hint },
                other => Availability::Failed {
                    detail: other.to_string(),
                },
            },
            Ok(out) if out.timed_out => Availability::Failed {
                detail: format!("Timeout al comprobar `{}`.", id.binary_name()),
            },
            Ok(out) if out.success() => {
                let text = out.stdout_lossy();
                let version = text
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or_default()
                    .to_string();
                Availability::Available {
                    binary: resolved.display_path().to_path_buf(),
                    version,
                }
            }
            Ok(out) => Availability::Failed {
                detail: crate::parse::truncate_detail(
                    &format!("{}{}", out.stderr_lossy(), out.stdout_lossy()),
                    400,
                ),
            },
        }
    }

    async fn suggest_commit_message(
        &self,
        ctx: &CallContext,
        diff: &str,
        template: Option<&str>,
    ) -> Result<AiSuggestion, AiError> {
        let opts = CommitOptions {
            template: template.map(String::from),
            ..CommitOptions::default()
        };
        self.suggest_commit_message_with(ctx, diff, &opts).await
    }

    async fn suggest_commit_message_with(
        &self,
        ctx: &CallContext,
        diff: &str,
        opts: &CommitOptions,
    ) -> Result<AiSuggestion, AiError> {
        let preview = PromptBuilder::commit_message_with(diff, opts);
        let reply = self.complete(ctx, &preview).await?;
        let (subject, body) = split_commit_message(&reply.text);
        if subject.is_empty() {
            return Err(AiError::BadOutput("respuesta vacía".into()));
        }
        Ok(self.make(
            ctx,
            SuggestionKind::CommitMessage,
            &preview,
            Some(subject),
            body,
            None,
            None,
            reply.note,
        ))
    }

    async fn suggest_pull_request(
        &self,
        ctx: &CallContext,
        input: &PullRequestInput,
    ) -> Result<AiSuggestion, AiError> {
        let preview = PromptBuilder::pull_request(input);
        let reply = self.complete(ctx, &preview).await?;
        let (title, body) = split_commit_message(&reply.text);
        if title.is_empty() {
            return Err(AiError::BadOutput("respuesta vacía".into()));
        }
        Ok(self.make(
            ctx,
            SuggestionKind::PullRequest,
            &preview,
            Some(title),
            body,
            None,
            None,
            reply.note,
        ))
    }

    async fn account(&self, config_dir: Option<&Path>) -> AccountInfo {
        let id = self.cfg.id;
        let Some(args) = account_args(id) else {
            return AccountInfo::unavailable(
                id,
                "El CLI no expone la cuenta activa de forma verificada.",
            );
        };
        let resolved = match self.resolve() {
            Ok(r) => r,
            Err(e) => return AccountInfo::unavailable(id, e.to_string()),
        };
        let dir = match CallDir::create(&self.cfg.sandbox_dir) {
            Ok(d) => d,
            Err(e) => return AccountInfo::unavailable(id, e.to_string()),
        };
        let mut ctx = CallContext::new("");
        ctx.config_dir = config_dir.map(Path::to_path_buf);
        let spec = match self.spec(&resolved, &args, &dir.0, Some(&ctx)) {
            Ok(s) => s.timeout(Some(self.cfg.version_timeout)),
            Err(e) => return AccountInfo::unavailable(id, e.to_string()),
        };
        let out = match ariadna_process::run(spec, CancellationToken::new()).await {
            Ok(o) => o,
            Err(e) => return AccountInfo::unavailable(id, self.map_process_error(e).to_string()),
        };
        drop(dir);
        if out.timed_out || out.truncated {
            return AccountInfo::unavailable(id, "El CLI no respondió a tiempo.");
        }
        // `claude auth status` sale con 1 sin sesión pero imprime el JSON igualmente.
        match id {
            ProviderId::Claude => parse_claude_auth_status(&out.stdout_lossy()),
            _ => AccountInfo::unavailable(id, "El CLI no expone la cuenta activa."),
        }
    }

    async fn summarize_diff(
        &self,
        ctx: &CallContext,
        diff: &str,
        style: SummaryStyle,
    ) -> Result<AiSuggestion, AiError> {
        let preview = PromptBuilder::summarize(diff, style);
        let reply = self.complete(ctx, &preview).await?;
        Ok(self.make(
            ctx,
            SuggestionKind::SummarizeDiff,
            &preview,
            None,
            strip_fences(&reply.text),
            None,
            None,
            reply.note,
        ))
    }

    async fn explain_commit(
        &self,
        ctx: &CallContext,
        sha: &str,
        patch: &str,
        subject: &str,
    ) -> Result<AiSuggestion, AiError> {
        let preview = PromptBuilder::explain(sha, subject, patch);
        let reply = self.complete(ctx, &preview).await?;
        Ok(self.make(
            ctx,
            SuggestionKind::ExplainCommit,
            &preview,
            Some(subject.to_string()),
            strip_fences(&reply.text),
            None,
            None,
            reply.note,
        ))
    }

    async fn propose_conflict_resolution(
        &self,
        ctx: &CallContext,
        conflict: &ConflictInput,
    ) -> Result<AiSuggestion, AiError> {
        let preview = PromptBuilder::resolve_conflict(conflict)?;
        let reply = self.complete(ctx, &preview).await?;
        let (content, rationale) = parse_conflict_proposal(&reply.text);
        if content.is_empty() {
            return Err(AiError::BadOutput("respuesta vacía".into()));
        }
        let Some(original) = conflict.working_tree.as_deref() else {
            return Err(AiError::ConflictProposalRejected(
                "falta el fichero original con marcadores para validar la propuesta".into(),
            ));
        };
        validate_conflict_proposal(original, &content)?;
        let body = rationale.clone().unwrap_or_else(|| {
            "Propuesta lista para revisar. No se ha escrito en el repositorio.".into()
        });
        Ok(self.make(
            ctx,
            SuggestionKind::ConflictResolution,
            &preview,
            Some(conflict.path.clone()),
            body,
            rationale,
            Some(content),
            reply.note,
        ))
    }
}
