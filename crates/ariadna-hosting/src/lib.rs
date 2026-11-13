//! Proveedores de alojamiento remoto (spec §4.3 y §4.4).
//!
//! * [`RemoteInfo::parse`]: reconocimiento **puro** del proveedor a partir de la URL de un
//!   remoto, enlace «abrir PR en el navegador» y ayuda para crear un PAT (sin E/S ni red).
//! * Pull requests (M8, D21/D22): [`PullRequestProvider`] con [`AzureDevOps`] y [`GitHub`],
//!   cliente HTTP con `reqwest` + `rustls`, errores tipados ([`HostingError`]) y
//!   autenticación sin guardar secretos ([`AuthMethod`], [`CredentialSource`],
//!   [`GitCredentialFill`]).
//!
//! **Regla inamovible:** el token no se guarda, no se cachea y no se registra. Se obtiene en
//! cada operación con `git credential fill`, vive en memoria con `zeroize` y nunca aparece en
//! `argv`, URL, `Debug` ni mensajes de error.

mod auth;
mod azure;
mod error;
mod github;
mod http;
mod model;
mod net;
mod provider;
mod remote;

pub use auth::{
    AuthMethod, CredentialSource, GitCredentialFill, Scrubber, Secret, StaticCredentials,
};
pub use azure::AzureDevOps;
pub use error::{HostingError, HostingResult};
pub use github::GitHub;
pub use http::HttpConfig;
pub use model::*;
pub use net::{apply_system_networking, build_http_client, NetSettings};
pub use provider::{provider_for, ProviderSettings, PullRequestProvider};
pub use remote::{HostHints, PatHelp, Provider, RemoteInfo, ALLOWED_HELP_HOSTS};
