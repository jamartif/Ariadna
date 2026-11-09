//! Ariadna: backend Tauri 2. La lógica está en [`core`] (sin ventana, probada con git real);
//! [`tauri_layer`] solo traduce comandos, diálogos nativos y eventos.

pub mod core;
mod tauri_layer;

pub use tauri_layer::run;
