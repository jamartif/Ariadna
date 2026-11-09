//! Perfiles, selección de proveedor y cola de IA (RF-IA8, RF-IA9, spec §5.1.1).
//!
//! - Un [`Profile`] fija qué proveedores están habilitados, cuáles tienen consentimiento
//!   y el tope de contexto. Personal: los tres; Empresa: Claude desactivado.
//! - Auto recorre Claude → Codex → Copilot **solo entre habilitados y con
//!   consentimiento**, y salta de proveedor **solo si el anterior no está instalado**
//!   (nunca por error de contenido, sesión o cuota).
//! - Cola global: 2 llamadas simultáneas (FIFO) y límite por minuto, con cancelación.
//! - `accept`/`reject` solo cambian el estado en memoria: no hay E/S de git.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::env::shares_login_between_profiles;
use crate::models::{
    AiError, AiSuggestion, Availability, CommitOptions, ProviderChoice, ProviderId,
    SuggestionStatus, SummaryStyle, DEFAULT_MAX_CONTEXT_BYTES,
};
use crate::prompt::ConflictInput;
use crate::provider::{AiProvider, CallContext};

/// Tipo de perfil (D16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileKind {
    Personal,
    Empresa,
}

/// Perfil de IA: separa el código personal del de la empresa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    pub kind: ProfileKind,
    pub enabled: BTreeSet<ProviderId>,
    /// Proveedores para los que el usuario ya aceptó el diálogo de consentimiento.
    pub consent: BTreeSet<ProviderId>,
    pub max_context_bytes: usize,
    /// Directorio de configuración aislado por proveedor (si el proveedor lo permite).
    pub config_dirs: BTreeMap<ProviderId, PathBuf>,
}

impl Profile {
    /// Perfil con los valores por defecto del spec: Personal = los tres proveedores;
    /// Empresa = Codex y Copilot (Claude desactivado). Sin consentimiento.
    pub fn new(name: impl Into<String>, kind: ProfileKind) -> Self {
        let enabled = match kind {
            ProfileKind::Personal => ProviderId::AUTO_ORDER.into_iter().collect(),
            ProfileKind::Empresa => [ProviderId::Codex, ProviderId::Copilot]
                .into_iter()
                .collect(),
        };
        Self {
            name: name.into(),
            kind,
            enabled,
            consent: BTreeSet::new(),
            max_context_bytes: DEFAULT_MAX_CONTEXT_BYTES,
            config_dirs: BTreeMap::new(),
        }
    }

    pub fn is_enabled(&self, p: ProviderId) -> bool {
        self.enabled.contains(&p)
    }

    pub fn has_consent(&self, p: ProviderId) -> bool {
        self.consent.contains(&p)
    }

    pub fn set_enabled(&mut self, p: ProviderId, on: bool) {
        if on {
            self.enabled.insert(p);
        } else {
            self.enabled.remove(&p);
        }
    }

    pub fn grant_consent(&mut self, p: ProviderId) {
        self.consent.insert(p);
    }

    pub fn revoke_consent(&mut self, p: ProviderId) {
        self.consent.remove(&p);
    }

    /// Habilitado **y** con consentimiento.
    pub fn is_usable(&self, p: ProviderId) -> bool {
        self.is_enabled(p) && self.has_consent(p)
    }

    /// Orden de Auto: Claude → Codex → Copilot, solo los usables.
    pub fn auto_order(&self) -> Vec<ProviderId> {
        ProviderId::AUTO_ORDER
            .into_iter()
            .filter(|p| self.is_usable(*p))
            .collect()
    }

    /// Error tipado si el proveedor no se puede usar en este perfil.
    pub fn check_usable(&self, p: ProviderId) -> Result<(), AiError> {
        if !self.is_enabled(p) {
            Err(AiError::ProviderDisabled { provider: p })
        } else if !self.has_consent(p) {
            Err(AiError::ConsentRequired { provider: p })
        } else {
            Ok(())
        }
    }
}

/// Estado visible de la cola.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueStatus {
    pub running: usize,
    pub waiting: usize,
}

/// Cola global: llamadas simultáneas (FIFO, el semáforo de tokio es justo) y límite por minuto.
#[derive(Debug)]
pub struct Limiter {
    sem: Arc<Semaphore>,
    concurrent: usize,
    per_minute: usize,
    window: Mutex<VecDeque<Instant>>,
    waiting: AtomicUsize,
}

impl Limiter {
    /// 2 llamadas simultáneas (RF-IA8).
    pub const DEFAULT_CONCURRENT: usize = 2;
    pub const DEFAULT_PER_MINUTE: usize = 20;

    pub fn new(concurrent: usize, per_minute: usize) -> Self {
        let concurrent = concurrent.max(1);
        Self {
            sem: Arc::new(Semaphore::new(concurrent)),
            concurrent,
            per_minute: per_minute.max(1),
            window: Mutex::new(VecDeque::new()),
            waiting: AtomicUsize::new(0),
        }
    }

    pub fn status(&self) -> QueueStatus {
        QueueStatus {
            running: self.concurrent - self.sem.available_permits().min(self.concurrent),
            waiting: self.waiting.load(Ordering::Relaxed),
        }
    }

    /// Espera turno. Se puede cancelar mientras se espera.
    pub async fn acquire(
        &self,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<OwnedSemaphorePermit, AiError> {
        self.waiting.fetch_add(1, Ordering::Relaxed);
        let permit = tokio::select! {
            p = self.sem.clone().acquire_owned() => p.map_err(|_| AiError::Cancelled),
            _ = cancel.cancelled() => Err(AiError::Cancelled),
        };
        self.waiting.fetch_sub(1, Ordering::Relaxed);
        let permit = permit?;

        let now = Instant::now();
        let mut w = self.window.lock().unwrap_or_else(|e| e.into_inner());
        while w
            .front()
            .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(60))
        {
            w.pop_front();
        }
        if w.len() >= self.per_minute {
            let oldest = w.front().copied().unwrap_or(now);
            let wait = Duration::from_secs(60).saturating_sub(now.duration_since(oldest));
            return Err(AiError::RateLimited {
                retry_after_secs: wait.as_secs() + 1,
            });
        }
        w.push_back(now);
        Ok(permit)
    }
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new(Self::DEFAULT_CONCURRENT, Self::DEFAULT_PER_MINUTE)
    }
}

/// Estado de un proveedor para la pantalla de Ajustes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderStatus {
    pub provider: ProviderId,
    pub availability: Availability,
    pub enabled: bool,
    pub consented: bool,
    /// `true` si el proveedor no puede aislar el login por perfil (hay que avisar).
    pub shares_login: bool,
    pub experimental: bool,
}

enum Action<'a> {
    CommitMessage {
        diff: &'a str,
        opts: &'a CommitOptions,
    },
    Summarize {
        diff: &'a str,
        style: SummaryStyle,
    },
    Explain {
        sha: &'a str,
        patch: &'a str,
        subject: &'a str,
    },
    Conflict(&'a ConflictInput),
}

async fn dispatch(
    p: &dyn AiProvider,
    ctx: &CallContext,
    a: &Action<'_>,
) -> Result<AiSuggestion, AiError> {
    match a {
        Action::CommitMessage { diff, opts } => {
            p.suggest_commit_message_with(ctx, diff, opts).await
        }
        Action::Summarize { diff, style } => p.summarize_diff(ctx, diff, *style).await,
        Action::Explain {
            sha,
            patch,
            subject,
        } => p.explain_commit(ctx, sha, patch, subject).await,
        Action::Conflict(c) => p.propose_conflict_resolution(ctx, c).await,
    }
}

/// Punto de entrada de la IA: elige proveedor, aplica la cola y devuelve propuestas `Pending`.
pub struct Orchestrator {
    providers: BTreeMap<ProviderId, Arc<dyn AiProvider>>,
    limiter: Arc<Limiter>,
    enabled: AtomicBool,
}

impl Orchestrator {
    pub fn new(providers: Vec<Arc<dyn AiProvider>>, limiter: Arc<Limiter>) -> Self {
        Self {
            providers: providers.into_iter().map(|p| (p.id(), p)).collect(),
            limiter,
            enabled: AtomicBool::new(true),
        }
    }

    /// Interruptor global de la IA.
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn queue_status(&self) -> QueueStatus {
        self.limiter.status()
    }

    /// Disponibilidad de cada proveedor registrado, con el estado del perfil.
    pub async fn statuses(&self, profile: &Profile) -> Vec<ProviderStatus> {
        let mut v = Vec::new();
        for (id, p) in &self.providers {
            v.push(ProviderStatus {
                provider: *id,
                availability: p.availability().await,
                enabled: profile.is_enabled(*id),
                consented: profile.has_consent(*id),
                shares_login: shares_login_between_profiles(*id),
                experimental: id.is_experimental(),
            });
        }
        v
    }

    pub async fn suggest_commit_message(
        &self,
        profile: &Profile,
        choice: ProviderChoice,
        ctx: &CallContext,
        diff: &str,
        template: Option<&str>,
    ) -> Result<AiSuggestion, AiError> {
        let opts = CommitOptions {
            template: template.map(String::from),
            ..CommitOptions::default()
        };
        self.suggest_commit_message_with(profile, choice, ctx, diff, &opts)
            .await
    }

    /// Mensaje de commit con idioma, estilo y asuntos recientes.
    pub async fn suggest_commit_message_with(
        &self,
        profile: &Profile,
        choice: ProviderChoice,
        ctx: &CallContext,
        diff: &str,
        opts: &CommitOptions,
    ) -> Result<AiSuggestion, AiError> {
        self.perform(profile, choice, ctx, &Action::CommitMessage { diff, opts })
            .await
    }

    pub async fn summarize_diff(
        &self,
        profile: &Profile,
        choice: ProviderChoice,
        ctx: &CallContext,
        diff: &str,
        style: SummaryStyle,
    ) -> Result<AiSuggestion, AiError> {
        self.perform(profile, choice, ctx, &Action::Summarize { diff, style })
            .await
    }

    pub async fn explain_commit(
        &self,
        profile: &Profile,
        choice: ProviderChoice,
        ctx: &CallContext,
        sha: &str,
        patch: &str,
        subject: &str,
    ) -> Result<AiSuggestion, AiError> {
        self.perform(
            profile,
            choice,
            ctx,
            &Action::Explain {
                sha,
                patch,
                subject,
            },
        )
        .await
    }

    pub async fn propose_conflict_resolution(
        &self,
        profile: &Profile,
        choice: ProviderChoice,
        ctx: &CallContext,
        conflict: &ConflictInput,
    ) -> Result<AiSuggestion, AiError> {
        self.perform(profile, choice, ctx, &Action::Conflict(conflict))
            .await
    }

    /// El usuario aceptó. Solo cambia el estado en memoria; no llama a git.
    pub fn accept(s: &AiSuggestion) -> AiSuggestion {
        s.with_status(SuggestionStatus::Accepted)
    }

    /// El usuario descartó la propuesta. No llama a git.
    pub fn reject(s: &AiSuggestion) -> AiSuggestion {
        s.with_status(SuggestionStatus::Rejected)
    }

    fn candidates(
        &self,
        profile: &Profile,
        choice: ProviderChoice,
    ) -> Result<Vec<ProviderId>, AiError> {
        if let Some(p) = choice.provider() {
            profile.check_usable(p)?;
            return Ok(vec![p]);
        }
        let order = profile.auto_order();
        if !order.is_empty() {
            return Ok(order);
        }
        // Nada usable: si hay habilitados sin consentimiento, lo pedimos.
        if let Some(p) = ProviderId::AUTO_ORDER
            .into_iter()
            .find(|p| profile.is_enabled(*p))
        {
            return Err(AiError::ConsentRequired { provider: p });
        }
        Err(AiError::NoProviderAvailable {
            hints: vec!["Ningún proveedor está habilitado en este perfil.".into()],
        })
    }

    async fn perform(
        &self,
        profile: &Profile,
        choice: ProviderChoice,
        ctx: &CallContext,
        action: &Action<'_>,
    ) -> Result<AiSuggestion, AiError> {
        if !self.enabled.load(Ordering::Relaxed) {
            return Err(AiError::AiDisabled);
        }
        let candidates = self.candidates(profile, choice)?;
        let auto = choice == ProviderChoice::Auto;
        let mut hints: Vec<String> = Vec::new();

        for id in candidates {
            let Some(provider) = self.providers.get(&id) else {
                let e = AiError::NotInstalled {
                    provider: id,
                    hint: "Proveedor no registrado.".into(),
                };
                if auto {
                    hints.push(e.to_string());
                    continue;
                }
                return Err(e);
            };
            let mut call_ctx = ctx.clone();
            call_ctx.config_dir = profile.config_dirs.get(&id).cloned();
            call_ctx.max_context_bytes = Some(profile.max_context_bytes);

            let permit = self.limiter.acquire(&ctx.cancel).await?;
            let result = dispatch(provider.as_ref(), &call_ctx, action).await;
            drop(permit);

            match result {
                Ok(s) => return Ok(s.with_status(SuggestionStatus::Pending)),
                // RF-IA9: el único motivo para saltar al siguiente es que no esté instalado.
                Err(AiError::NotInstalled { provider, hint }) if auto => {
                    hints.push(format!("{}: {hint}", provider.display_name()));
                }
                Err(e) => return Err(e),
            }
        }
        Err(AiError::NoProviderAvailable { hints })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::models::{new_suggestion_id, now_millis, SuggestionKind};
    use async_trait::async_trait;
    use std::sync::atomic::AtomicUsize;
    use tokio_util::sync::CancellationToken;

    struct Mock {
        id: ProviderId,
        outcome: Result<&'static str, AiError>,
        delay: Duration,
        calls: AtomicUsize,
        running: AtomicUsize,
        max_running: AtomicUsize,
        seen_max_ctx: Mutex<Option<usize>>,
    }

    impl Mock {
        fn new(id: ProviderId, outcome: Result<&'static str, AiError>) -> Arc<Self> {
            Arc::new(Self {
                id,
                outcome,
                delay: Duration::ZERO,
                calls: AtomicUsize::new(0),
                running: AtomicUsize::new(0),
                max_running: AtomicUsize::new(0),
                seen_max_ctx: Mutex::new(None),
            })
        }
        fn slow(id: ProviderId, ms: u64) -> Arc<Self> {
            Arc::new(Self {
                id,
                outcome: Ok("feat: lento"),
                delay: Duration::from_millis(ms),
                calls: AtomicUsize::new(0),
                running: AtomicUsize::new(0),
                max_running: AtomicUsize::new(0),
                seen_max_ctx: Mutex::new(None),
            })
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
        async fn respond(&self, ctx: &CallContext) -> Result<AiSuggestion, AiError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            *self.seen_max_ctx.lock().unwrap() = ctx.max_context_bytes;
            let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_running.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            self.running.fetch_sub(1, Ordering::SeqCst);
            let text = self.outcome.clone()?;
            Ok(AiSuggestion {
                id: new_suggestion_id(),
                kind: SuggestionKind::CommitMessage,
                provider: self.id,
                command_summary: "mock".into(),
                subject: Some(text.into()),
                body: String::new(),
                rationale: None,
                proposed_content: None,
                created_at: now_millis(),
                // El mock devuelve un estado raro a propósito: el orquestador lo normaliza.
                status: SuggestionStatus::Accepted,
                repo_revision: ctx.repo_revision.clone(),
                transparency_note: None,
            })
        }
    }

    #[async_trait]
    impl AiProvider for Mock {
        fn id(&self) -> ProviderId {
            self.id
        }
        async fn availability(&self) -> Availability {
            Availability::Missing {
                hint: "mock".into(),
            }
        }
        async fn suggest_commit_message(
            &self,
            ctx: &CallContext,
            _: &str,
            _: Option<&str>,
        ) -> Result<AiSuggestion, AiError> {
            self.respond(ctx).await
        }
        async fn summarize_diff(
            &self,
            ctx: &CallContext,
            _: &str,
            _: SummaryStyle,
        ) -> Result<AiSuggestion, AiError> {
            self.respond(ctx).await
        }
        async fn explain_commit(
            &self,
            ctx: &CallContext,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<AiSuggestion, AiError> {
            self.respond(ctx).await
        }
        async fn propose_conflict_resolution(
            &self,
            ctx: &CallContext,
            _: &ConflictInput,
        ) -> Result<AiSuggestion, AiError> {
            self.respond(ctx).await
        }
    }

    fn not_installed(p: ProviderId) -> AiError {
        AiError::NotInstalled {
            provider: p,
            hint: "falta".into(),
        }
    }

    fn orch(mocks: &[&Arc<Mock>]) -> Orchestrator {
        Orchestrator::new(
            mocks
                .iter()
                .map(|m| (*m).clone() as Arc<dyn AiProvider>)
                .collect(),
            Arc::new(Limiter::default()),
        )
    }

    fn personal_with_consent() -> Profile {
        let mut p = Profile::new("Personal", ProfileKind::Personal);
        for id in ProviderId::AUTO_ORDER {
            p.grant_consent(id);
        }
        p
    }

    async fn commit(
        o: &Orchestrator,
        p: &Profile,
        c: ProviderChoice,
    ) -> Result<AiSuggestion, AiError> {
        o.suggest_commit_message(p, c, &CallContext::new("rev1"), "diff", None)
            .await
    }

    #[test]
    fn valores_por_defecto_de_los_perfiles() {
        let p = Profile::new("P", ProfileKind::Personal);
        assert_eq!(p.enabled.len(), 3);
        assert!(p.consent.is_empty());
        assert_eq!(p.max_context_bytes, 100_000);
        let e = Profile::new("Empresa", ProfileKind::Empresa);
        assert!(!e.is_enabled(ProviderId::Claude));
        assert!(e.is_enabled(ProviderId::Codex) && e.is_enabled(ProviderId::Copilot));
        // Sin consentimiento no hay nada usable.
        assert!(p.auto_order().is_empty());
    }

    #[test]
    fn auto_order_solo_entre_habilitados_y_con_consentimiento() {
        let mut e = Profile::new("E", ProfileKind::Empresa);
        e.grant_consent(ProviderId::Claude);
        e.grant_consent(ProviderId::Copilot);
        assert_eq!(e.auto_order(), [ProviderId::Copilot]);
        e.grant_consent(ProviderId::Codex);
        assert_eq!(e.auto_order(), [ProviderId::Codex, ProviderId::Copilot]);
        e.set_enabled(ProviderId::Claude, true);
        assert_eq!(e.auto_order()[0], ProviderId::Claude);
        e.revoke_consent(ProviderId::Claude);
        assert_eq!(e.auto_order()[0], ProviderId::Codex);
    }

    #[test]
    fn perfil_serializa_ida_y_vuelta() {
        let mut p = personal_with_consent();
        p.config_dirs
            .insert(ProviderId::Claude, PathBuf::from("/perfiles/p/claude"));
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Profile>(&json).unwrap(), p);
    }

    /// Port de `testOrchestratorAutoFallsBackAndExplicitChoiceDoesNot`.
    #[tokio::test]
    async fn auto_salta_si_no_esta_instalado_y_la_eleccion_explicita_no() {
        let claude = Mock::new(ProviderId::Claude, Err(not_installed(ProviderId::Claude)));
        let codex = Mock::new(ProviderId::Codex, Ok("chore: limpiar"));
        let copilot = Mock::new(ProviderId::Copilot, Ok("no debe llamarse"));
        let o = orch(&[&claude, &codex, &copilot]);
        let profile = personal_with_consent();

        let s = commit(&o, &profile, ProviderChoice::Auto).await.unwrap();
        assert_eq!(s.provider, ProviderId::Codex);
        assert_eq!(s.subject.as_deref(), Some("chore: limpiar"));
        assert_eq!(s.status, SuggestionStatus::Pending);
        assert_eq!(s.repo_revision, "rev1");
        assert_eq!(Orchestrator::accept(&s).status, SuggestionStatus::Accepted);
        assert_eq!(Orchestrator::reject(&s).status, SuggestionStatus::Rejected);
        assert_eq!(copilot.calls(), 0);

        let codex_before = codex.calls();
        let e = commit(&o, &profile, ProviderChoice::Claude).await;
        assert!(matches!(e, Err(AiError::NotInstalled { .. })));
        assert_eq!(codex.calls(), codex_before, "no hay fallback explícito");
    }

    #[tokio::test]
    async fn nunca_hay_fallback_por_sesion_cuota_o_contenido() {
        for err in [
            AiError::NotLoggedIn {
                provider: ProviderId::Claude,
                login_command: "claude".into(),
            },
            AiError::Quota {
                provider: ProviderId::Claude,
                detail: String::new(),
            },
            AiError::BadOutput("x".into()),
            AiError::ContextTooLarge {
                bytes: 2,
                max_bytes: 1,
            },
            AiError::Timeout {
                provider: ProviderId::Claude,
            },
        ] {
            let claude = Mock::new(ProviderId::Claude, Err(err.clone()));
            let codex = Mock::new(ProviderId::Codex, Ok("x"));
            let o = orch(&[&claude, &codex]);
            let r = commit(&o, &personal_with_consent(), ProviderChoice::Auto).await;
            assert_eq!(r, Err(err));
            assert_eq!(codex.calls(), 0);
        }
    }

    #[tokio::test]
    async fn ninguno_instalado_en_auto() {
        let claude = Mock::new(ProviderId::Claude, Err(not_installed(ProviderId::Claude)));
        let codex = Mock::new(ProviderId::Codex, Err(not_installed(ProviderId::Codex)));
        let o = orch(&[&claude, &codex]);
        let r = commit(&o, &personal_with_consent(), ProviderChoice::Auto).await;
        let Err(AiError::NoProviderAvailable { hints }) = r else {
            panic!("{r:?}")
        };
        // Claude y Codex no instalados + Copilot sin registrar.
        assert_eq!(hints.len(), 3);
    }

    #[tokio::test]
    async fn consentimiento_y_habilitacion() {
        let claude = Mock::new(ProviderId::Claude, Ok("a"));
        let codex = Mock::new(ProviderId::Codex, Ok("b"));
        let o = orch(&[&claude, &codex]);

        // Sin consentimiento: explícito y Auto piden consentimiento.
        let p = Profile::new("P", ProfileKind::Personal);
        assert_eq!(
            commit(&o, &p, ProviderChoice::Codex).await,
            Err(AiError::ConsentRequired {
                provider: ProviderId::Codex
            })
        );
        assert_eq!(
            commit(&o, &p, ProviderChoice::Auto).await,
            Err(AiError::ConsentRequired {
                provider: ProviderId::Claude
            })
        );
        // Empresa: Claude desactivado aunque tenga consentimiento.
        let mut e = Profile::new("E", ProfileKind::Empresa);
        e.grant_consent(ProviderId::Claude);
        e.grant_consent(ProviderId::Codex);
        assert_eq!(
            commit(&o, &e, ProviderChoice::Claude).await,
            Err(AiError::ProviderDisabled {
                provider: ProviderId::Claude
            })
        );
        // Auto en Empresa se salta Claude y usa Codex.
        let s = commit(&o, &e, ProviderChoice::Auto).await.unwrap();
        assert_eq!(s.provider, ProviderId::Codex);
        assert_eq!(claude.calls(), 0);
        // Nada habilitado.
        let mut none = Profile::new("N", ProfileKind::Empresa);
        none.enabled.clear();
        assert!(matches!(
            commit(&o, &none, ProviderChoice::Auto).await,
            Err(AiError::NoProviderAvailable { .. })
        ));
    }

    #[tokio::test]
    async fn interruptor_global() {
        let claude = Mock::new(ProviderId::Claude, Ok("a"));
        let o = orch(&[&claude]);
        o.set_enabled(false);
        assert_eq!(
            commit(&o, &personal_with_consent(), ProviderChoice::Auto).await,
            Err(AiError::AiDisabled)
        );
        assert_eq!(claude.calls(), 0);
    }

    #[tokio::test]
    async fn el_tope_y_el_directorio_del_perfil_llegan_al_proveedor() {
        let claude = Mock::new(ProviderId::Claude, Ok("a"));
        let o = orch(&[&claude]);
        let mut p = personal_with_consent();
        p.max_context_bytes = 777;
        commit(&o, &p, ProviderChoice::Claude).await.unwrap();
        assert_eq!(*claude.seen_max_ctx.lock().unwrap(), Some(777));
    }

    #[tokio::test]
    async fn maximo_dos_llamadas_simultaneas() {
        let claude = Mock::slow(ProviderId::Claude, 60);
        let o = Arc::new(orch(&[&claude]));
        let p = Arc::new(personal_with_consent());
        let mut hs = Vec::new();
        for _ in 0..6 {
            let (o, p) = (o.clone(), p.clone());
            hs.push(tokio::spawn(async move {
                commit(&o, &p, ProviderChoice::Claude).await
            }));
        }
        for h in hs {
            h.await.unwrap().unwrap();
        }
        assert_eq!(claude.calls(), 6);
        assert_eq!(claude.max_running.load(Ordering::SeqCst), 2);
        assert_eq!(
            o.queue_status(),
            QueueStatus {
                running: 0,
                waiting: 0
            }
        );
    }

    #[tokio::test]
    async fn limite_por_minuto() {
        let claude = Mock::new(ProviderId::Claude, Ok("a"));
        let o = Orchestrator::new(
            vec![claude.clone() as Arc<dyn AiProvider>],
            Arc::new(Limiter::new(2, 2)),
        );
        let p = personal_with_consent();
        commit(&o, &p, ProviderChoice::Claude).await.unwrap();
        commit(&o, &p, ProviderChoice::Claude).await.unwrap();
        let r = commit(&o, &p, ProviderChoice::Claude).await;
        assert!(matches!(r, Err(AiError::RateLimited { .. })), "{r:?}");
        assert_eq!(claude.calls(), 2);
    }

    #[tokio::test]
    async fn cancelar_mientras_espera_turno() {
        let claude = Mock::slow(ProviderId::Claude, 300);
        let o = Arc::new(Orchestrator::new(
            vec![claude.clone() as Arc<dyn AiProvider>],
            Arc::new(Limiter::new(1, 100)),
        ));
        let p = Arc::new(personal_with_consent());
        let first = {
            let (o, p) = (o.clone(), p.clone());
            tokio::spawn(async move { commit(&o, &p, ProviderChoice::Claude).await })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        let token = CancellationToken::new();
        let second = {
            let (o, p, t) = (o.clone(), p.clone(), token.clone());
            tokio::spawn(async move {
                o.suggest_commit_message(
                    &p,
                    ProviderChoice::Claude,
                    &CallContext::new("r").with_cancel(t),
                    "d",
                    None,
                )
                .await
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(o.queue_status().waiting, 1);
        token.cancel();
        assert_eq!(second.await.unwrap(), Err(AiError::Cancelled));
        first.await.unwrap().unwrap();
        assert_eq!(claude.calls(), 1);
    }

    #[tokio::test]
    async fn estados_para_ajustes() {
        let claude = Mock::new(ProviderId::Claude, Ok("a"));
        let o = orch(&[&claude]);
        let st = o.statuses(&Profile::new("E", ProfileKind::Empresa)).await;
        assert_eq!(st.len(), 1);
        assert!(!st[0].enabled && !st[0].consented && !st[0].shares_login);
    }
}
