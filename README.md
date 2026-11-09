# Ariadna

**Ariadna es un cliente Git de escritorio para macOS y Windows**, escrito en Rust (Tauri 2) con una interfaz Svelte 5, en español. Por debajo usa el `git` de tu sistema, así que hooks, credenciales y configuración son los mismos que en la terminal. Añade una interfaz clara para preparar cambios por hunk, revisar el historial, gestionar ramas y, con ayuda opcional de IA, redactar mensajes de commit. Regla fija: **nada que reescriba tu repositorio ocurre sin que lo confirmes**.

> **Estado: alfa en desarrollo (`0.1.0-alpha`).** La versión anterior en Swift/SwiftUI (solo macOS) se conserva en la rama [`legacy-swift`](https://github.com/jamartif/Ariadna/tree/legacy-swift) y en la etiqueta `swift-final`. La especificación de la migración está en [`specs/rust/`](specs/rust/).

## Qué hay hoy

- **Pestañas por repositorio**, con la sesión guardada entre arranques.
- **Cambios** con diff por hunk, preparar y quitar de la preparación por bloque, y barra de commit.
- **Historial** paginado con grafo de ramas y diff por commit.
- **Ramas**: crear, cambiar, renombrar, borrar y fusionar (si hay conflictos, aborta limpiamente).
- **Clonar** desde una URL (GitHub, Azure DevOps, GitLab, Bitbucket, genérica) con PAT opcional.
- **IA opcional** para proponer asunto y descripción del commit a partir de lo preparado.
- Tema claro y oscuro, paleta de comandos y atajos de teclado.

Pendiente antes de la `0.1.0`: resolución de conflictos a tres paneles, ajustes, pull requests de Azure DevOps y GitHub, y firma de binarios. El detalle está en [`specs/rust/tasks.md`](specs/rust/tasks.md).

## IA (opcional)

Ariadna no usa claves de API: llama a los **CLI oficiales** con tu suscripción, y el prompt viaja por la entrada estándar, nunca por argumentos.

| Proveedor | CLI |
| --- | --- |
| Claude | `claude` |
| Codex (ChatGPT) | `codex` |
| GitHub Copilot | `copilot` |

- Orden en modo automático: Claude → Codex → Copilot.
- Perfiles **Personal** y **Empresa** (en Empresa, Claude queda desactivado por defecto).
- Pide consentimiento la primera vez, excluye archivos sensibles y redacta secretos del diff.
- **La IA solo propone; tú decides.**

## Seguridad

- Ariadna **no guarda, no cachea ni registra** tokens ni PAT. Para clonar, el PAT se entrega a git una sola vez mediante un `GIT_ASKPASS` temporal y se descarta.
- Los comandos de git se lanzan sin shell y con rutas validadas; `fsmonitor` se desactiva.
- El repositorio es público: `gitleaks` revisa cada commit (`.githooks/pre-commit`, instalable con `sh scripts/install-git-hooks.sh`) y la CI lo pasa sobre todo el historial.

## Instalación

Aún no hay versiones publicadas. Las alfas se generan con el workflow manual **bundle** (instalador NSIS para Windows x64 y `.dmg` universal para macOS) y **no están firmadas**:

- **Windows:** SmartScreen avisa; en equipos con *Smart App Control* activado el instalador se bloquea y no puede ejecutarse. La `v0.1.0` irá firmada.
- **macOS 15+:** clic derecho → *Abrir* la primera vez.

## Desarrollo

Requisitos: Rust estable (ver `rust-toolchain.toml`), Node 22 y las dependencias de Tauri 2 de tu sistema.

```sh
# Rust: formato, lint y tests
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast

# Interfaz
cd ui
npm ci
npm run check && npm test && npm run build
npm run dev          # Vite en 127.0.0.1:5173 (con backend simulado si se abre en el navegador)
```

Estructura:

```
crates/ariadna-process   ejecución de procesos (sin shell, cancelación, límites)
crates/ariadna-git       motor Git, parsers y sesiones
crates/ariadna-ai        proveedores de IA vía CLI, prompts, redacción de secretos
src-tauri/               aplicación Tauri (núcleo testeable sin ventana + comandos IPC)
ui/                      interfaz Svelte 5
specs/rust/              especificación, plan, tareas y contrato IPC
```

## Licencia

[MIT](LICENSE)
