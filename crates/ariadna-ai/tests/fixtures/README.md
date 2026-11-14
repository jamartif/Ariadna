# Fixtures de salida de los CLI de IA

| Fichero | Proveedor | Estado |
|---|---|---|
| `claude_*.json` | Claude `--output-format json` | **Reconstruido a partir del formato documentado de `claude 2.1.x`, NO capturado**: en esta tarea no se ejecutan prompts reales. Sustituir por golden files reales en el humo manual de M6. El primero lleva una línea de aviso previa para probar la tolerancia a banners. |
| `codex_*.jsonl` | Codex `exec --json` | **NO VERIFICADO**: inventado razonablemente a partir del parser del Swift original (`thread.started`, `item.completed`, `turn.completed`, `turn.failed`). Codex no está instalado. |
| `copilot_ok.jsonl` | Copilot `--output-format json` | **Capturado** de `GitHub Copilot CLI 1.0.112` (Windows 11 x64, 2026-11-14) con los argumentos de `commands.rs::build_args(Copilot)` y un prompt real de mensaje de commit: `session.start`, dos `session.info` (catálogo de herramientas vacío), `session.mcp_servers_loaded` (todo `disabled`), `user.message`, `assistant.message_start`, el `assistant.message` final (texto **real** del modelo) y `result` con `exitCode: 0`, más el `session.usage_checkpoint` **recortado** a los `tokenDetails` que usa el parser. Sustituidos ids, marcas de tiempo, rutas y modelo: sin datos personales. |
| `copilot_error.jsonl` | Copilot `--output-format json` | **Capturado** del mismo CLI con `--model modelo-que-no-existe`: `session.error` + `result` con `exitCode: 1`. Solo se sustituyeron el id de sesión y la ruta de trabajo. |

No hay fixture de «sin sesión» de Copilot: cerrar la sesión exigiría tocar el llavero del
usuario. El clasificador se prueba con textos de patrón en `parse.rs` y queda **NO VERIFICADO**
en `specs/rust/checklist-humo-ia.md`.

Cuando se verifiquen los CLI reales (spike M0), reemplazar estos ficheros por salida real
y actualizar esta tabla.
