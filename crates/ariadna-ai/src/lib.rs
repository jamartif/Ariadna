//! Capa de IA de Ariadna (spec §5): Claude, Codex y Copilot mediante su CLI oficial.
//!
//! Principio: **la IA solo propone**. Nada de este crate escribe en el worktree,
//! el índice ni las refs; toda respuesta es una [`AiSuggestion`] en memoria.
//! El crate no depende de `ariadna-git`: recibe diffs y conflictos como texto.
//!
//! Módulos:
//! - [`models`]: tipos de dominio, errores y ajustes.
//! - [`env`]: listas blancas de entorno por proveedor (RF-IA3).
//! - [`commands`]: argumentos de cada CLI, aislados en un único módulo (RF-IA1).
//! - [`resolve`]: localización del binario y shims de Windows (RF-IA2).
//! - [`parse`]: parsers de salida y detección de login/cuota (RF-IA7).
//! - [`prompt`]: construcción de prompts, exclusión y redacción (§5.1).
//! - [`provider`]: trait [`AiProvider`] e implementación [`CliProvider`].
//! - [`orchestrator`]: perfiles, modo Auto y cola (RF-IA8/IA9, §5.1.1).

pub mod commands;
pub mod env;
pub mod models;
pub mod orchestrator;
pub mod parse;
pub mod prompt;
pub mod provider;
pub mod resolve;

pub use models::{
    AiContextPolicy, AiError, AiSettings, AiSuggestion, Availability, ProviderChoice, ProviderId,
    SuggestionKind, SuggestionStatus, SummaryStyle,
};
pub use orchestrator::{Limiter, Orchestrator, Profile, ProfileKind};
pub use provider::{AiProvider, CallContext, CliProvider, CliProviderConfig};
