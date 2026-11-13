//! Diálogos nativos detrás de un trait, para que el núcleo no dependa de la ventana.

use std::path::PathBuf;

use async_trait::async_trait;

/// Quien pregunta al usuario. En producción abre diálogos nativos (plugin dialog de Tauri);
/// en los tests es un doble que acepta o rechaza.
#[async_trait]
pub trait Prompter: Send + Sync {
    /// Pide confirmación. `true` = el usuario acepta. Un fallo al abrir el diálogo cuenta como
    /// rechazo: ante la duda no se hace nada destructivo.
    async fn confirm(&self, title: &str, message: &str, detail: &str) -> bool;

    /// Selector de carpeta. `None` si se cancela.
    async fn pick_folder(&self) -> Option<PathBuf>;
}
