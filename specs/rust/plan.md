# Plan de migración a Rust

Deriva de [spec.md](spec.md) v1. Estado de cada tarea en [tasks.md](tasks.md).

## Estructura del repo durante la migración
```
Cargo.toml                 workspace
crates/ariadna-git/        núcleo git (sin UI)
crates/ariadna-ai/         proveedores claude / codex / copilot
src-tauri/                 app Tauri (comandos IPC, sesiones, pestañas)
ui/                        Svelte 5 + TypeScript
Sources/, Tests/, Package.swift   Swift actual: congelado, solo correcciones críticas
specs/                     Swift: spec.md, plan.md, tasks.md · Rust: specs/rust/
```
- Rama `rust-migration`. El Swift sigue compilando y releaseable en `main` hasta la paridad. Se fusiona `main` en `rust-migration` periódicamente.
- CI Rust con filtros por ruta (`crates/**`, `src-tauri/**`, `ui/**`) y CI Swift para `Sources/**`. Un **job agregador** siempre corre.
- Al llegar a la paridad: tag `swift-final`, rama `legacy-swift`, borrar `Sources/`, `Tests/`, `Package.swift` de `main`, README nuevo.

## Principios
1. **Paridad antes que novedades.** Cada test Swift tiene equivalente Rust antes de borrar nada.
2. **La fuente de verdad es git.** El Swift sirve de contraste, no de oráculo.
3. **Núcleo sin UI**, probado en macOS, Windows y Linux.
4. **Estado en Rust**, UI como vista.
5. **IA solo propone.** Confirmaciones abiertas desde Rust, token de un solo uso.
6. **Una cola de mutaciones por clave de repo.**
7. **Contrato IPC antes de la UI.**

## Hitos

| Hito | Contenido | Criterio de salida |
|---|---|---|
| M0 | Spikes (CLI de IA reales, Mica descartado, WebView2), matriz de trazabilidad, inventario de tests, términos de uso, mockups base, rama y andamiaje de specs | Cero hallazgos altos abiertos; spikes documentados |
| M1 | Workspace, CI en 4 runners, `ProcessRunner`, localización de git, `PathSafety`, parsers y fixtures | Tests de parsers y proceso en verde en macOS, Windows, Linux |
| M2 | `GitEngine`: status, stage, commit, amend, log, búsqueda, diff. Contrato de comandos, errores y eventos | Tests portados = tests Swift del área |
| M3 | Ramas, remotos, merge, conflictos 3 vías, abort/continue | Tests portados = tests Swift del área |
| M4 | Shell Tauri sobre `GitServing` simulado: ventana, tokens, Bienvenida, pestañas, sesión persistente, watcher | Varios repos en pestañas, restaurados al reabrir |
| M5 | Pantallas: cambios + diff por hunk, commit, historial, ramas, conflictos | **`0.1.0-alpha.1`**: Git completo sin IA, macOS y Windows |
| M6 | IA: trait, Claude, Codex, Copilot, consentimiento, stdin, shims, Ajustes, hoja de propuesta | **`0.1.0-alpha.2`**: IA con el CLI falso en CI y humo manual |
| M7 | Pulido, accesibilidad, rendimiento, instaladores, release, retirada del Swift | `v0.1.0` |

## Dependencias
- M1 → M2 → M3 (mismo núcleo).
- M4 arranca con un esqueleto sobre un `GitServing` simulado cuando el contrato IPC de M2 esté definido.
- M6 depende de M1 (ProcessRunner), M4 (sesión por pestaña) y de la hoja de propuesta de M5. Se puede avanzar en paralelo la parte de `ariadna-ai` con el CLI falso.
- Los spikes de M0 bloquean M6 pero no M1–M5.

## Riesgos a vigilar
Ver tabla §12 del spec. Los más probables: calendario (aprender tres tecnologías), flags de Codex y Copilot, y términos de uso.
