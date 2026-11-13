//! `trait PullRequestProvider` y la fábrica que elige Azure DevOps o GitHub según el remoto.

use std::sync::Arc;

use async_trait::async_trait;
use url::Url;

use crate::auth::CredentialSource;
use crate::azure::AzureDevOps;
use crate::error::{HostingError, HostingResult};
use crate::github::GitHub;
use crate::http::HttpConfig;
use crate::model::{
    Capabilities, CompleteOptions, FilePatch, ListQuery, NewPullRequest, NewThread, PrUser,
    PullRequestDetail, PullRequestPage, PullRequestSummary, ReviewThread, Vote,
};
use crate::remote::{Provider, RemoteInfo};

/// Operaciones de pull request. No conoce Tauri ni la interfaz. Cada método pide la credencial
/// a la [`CredentialSource`], la usa en memoria y la suelta (D22): **nada se cachea**.
#[async_trait]
pub trait PullRequestProvider: Send + Sync {
    fn provider(&self) -> Provider;

    /// Qué admite este proveedor (votos, estrategias, finalización automática…).
    fn capabilities(&self) -> Capabilities;

    /// Usuario autenticado (sirve también para comprobar que el token vale).
    async fn current_user(&self) -> HostingResult<PrUser>;

    /// PR-02. Páginas de `ListQuery::per_page` (empieza en 1).
    async fn list(&self, query: &ListQuery) -> HostingResult<PullRequestPage>;

    /// Datos básicos de una PR con una sola petición (confirmaciones y refrescos ligeros).
    async fn summary(&self, id: u64) -> HostingResult<PullRequestSummary>;

    /// PR-03: revisores, políticas/comprobaciones, work items, commits y archivos.
    async fn detail(&self, id: u64) -> HostingResult<PullRequestDetail>;

    /// PR-04.
    async fn create(&self, new: &NewPullRequest) -> HostingResult<PullRequestDetail>;

    /// PR-08: abandonar (Azure DevOps) o cerrar sin fusionar (GitHub).
    async fn close(&self, id: u64) -> HostingResult<PullRequestDetail>;

    /// PR-08: reabrir una PR abandonada/cerrada (una fusionada no se puede).
    async fn reopen(&self, id: u64) -> HostingResult<PullRequestDetail>;

    /// PR-06: parche de cada archivo (merge-base…origen).
    async fn diff(&self, id: u64) -> HostingResult<Vec<FilePatch>>;

    /// PR-06: hilos de comentarios (en línea y generales).
    async fn threads(&self, id: u64) -> HostingResult<Vec<ReviewThread>>;

    async fn create_thread(&self, id: u64, thread: &NewThread) -> HostingResult<ReviewThread>;

    async fn reply(&self, id: u64, thread_id: &str, body: &str) -> HostingResult<ReviewThread>;

    async fn set_thread_resolved(
        &self,
        id: u64,
        thread_id: &str,
        resolved: bool,
    ) -> HostingResult<ReviewThread>;

    /// PR-07. `comment` acompaña al voto (GitHub: cuerpo de la revisión; Azure DevOps: comentario
    /// general aparte).
    async fn vote(
        &self,
        id: u64,
        vote: Vote,
        comment: Option<&str>,
    ) -> HostingResult<PullRequestDetail>;

    /// PR-08: completar/fusionar. Las políticas del proveedor mandan: si no se cumplen, el
    /// servidor lo rechaza y el error es `POLICY`.
    async fn complete(
        &self,
        id: u64,
        options: &CompleteOptions,
    ) -> HostingResult<PullRequestDetail>;
}

/// Ajustes de la fábrica.
#[derive(Debug, Clone, Default)]
pub struct ProviderSettings {
    pub http: HttpConfig,
    /// Solo pruebas: sustituye la base de la API (p. ej. un servidor simulado). En producción
    /// siempre `None`: la base sale del host del remoto.
    pub api_base: Option<Url>,
}

/// Crea el proveedor adecuado al remoto, o `Invalid` si no admite PR dentro de la app.
pub fn provider_for(
    info: &RemoteInfo,
    creds: Arc<dyn CredentialSource>,
    settings: ProviderSettings,
) -> HostingResult<Box<dyn PullRequestProvider>> {
    if !info.is_complete() || info.web_url.is_none() {
        return Err(HostingError::invalid(
            "No se reconoce el repositorio remoto: no se pueden gestionar pull requests.",
        ));
    }
    match info.provider {
        Provider::AzureDevOps => Ok(Box::new(AzureDevOps::new(info, creds, settings)?)),
        Provider::GitHub => Ok(Box::new(GitHub::new(info, creds, settings)?)),
        other => Err(HostingError::invalid(format!(
            "Las pull requests de {} se abren en el navegador, no dentro de Ariadna.",
            other.label()
        ))),
    }
}

// --- utilidades comunes ---------------------------------------------------------------------

/// `base` + segmentos (codificados) + parámetros de consulta.
pub(crate) fn build_url(
    base: &Url,
    segments: &[&str],
    query: &[(&str, &str)],
) -> HostingResult<Url> {
    let mut url = base.clone();
    url.path_segments_mut()
        .map_err(|_| HostingError::invalid("La dirección del proveedor no es válida."))?
        .pop_if_empty()
        .extend(segments);
    if !query.is_empty() {
        let mut q = url.query_pairs_mut();
        for (k, v) in query {
            q.append_pair(k, v);
        }
    }
    Ok(url)
}

/// `refs/heads/x` → `x`.
pub(crate) fn short_branch(r: &str) -> String {
    r.strip_prefix("refs/heads/").unwrap_or(r).to_owned()
}

/// Nombre de rama aceptable para enviar al proveedor.
pub(crate) fn check_branch(name: &str, what: &str) -> HostingResult<()> {
    let t = name.trim();
    if t.is_empty()
        || t.len() > 255
        || t.starts_with('-')
        || t.chars()
            .any(|c| c.is_control() || c == ' ' || c == '~' || c == '^' || c == ':')
    {
        return Err(HostingError::invalid(format!(
            "El nombre de la rama {what} no es válido."
        )));
    }
    Ok(())
}

/// Ruta de archivo que viene del servidor: no puede llevar caracteres de control (acabaría en
/// una cabecera de diff).
pub(crate) fn safe_path(p: &str) -> Option<String> {
    let t = p.trim_start_matches('/');
    (!t.is_empty() && !t.chars().any(char::is_control)).then(|| t.to_owned())
}

pub(crate) fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_owned()
}
