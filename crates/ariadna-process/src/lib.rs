//! Lanzador de procesos compartido por el núcleo git y la capa de IA.
//!
//! Garantías: timeout y cancelación que terminan el **árbol** de procesos
//! (Job Object en Windows, grupo de procesos en Unix), stdin escrito en una
//! tarea aparte, stdout/stderr drenados en paralelo con tope de bytes, entorno
//! saneado y sin ventana de consola en Windows.

mod env;
mod runner;

pub use env::{build_env, name_matches, EnvPolicy};
pub use runner::{run, run_streaming, ProcessError, ProcessOutput, ProcessSpec};
pub use tokio_util::sync::CancellationToken;
