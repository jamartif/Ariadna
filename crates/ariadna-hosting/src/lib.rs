//! Proveedores de alojamiento remoto (spec §4.3 y §4.4).
//!
//! Hoy contiene el reconocimiento **puro** del proveedor a partir de la URL de un remoto
//! ([`RemoteInfo::parse`]), el enlace «abrir PR en el navegador» y la ayuda para crear un PAT.
//! No hace E/S ni red. Las pull requests (M8) se añadirán aquí sin romper esta API.

mod remote;

pub use remote::{HostHints, PatHelp, Provider, RemoteInfo, ALLOWED_HELP_HOSTS};
