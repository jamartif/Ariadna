# Ariadna

**Un cliente Git de escritorio para Windows y macOS, en español, con IA opcional.**
Hecho en Rust (Tauri 2) y Svelte 5. Por debajo usa el `git` que ya tienes instalado: hooks,
credenciales, `credential.helper` y configuración son los mismos que en la terminal, sin
capa intermedia que pueda desincronizarse. Añade una interfaz clara para preparar cambios
(línea a línea), revisar el historial, gestionar ramas, remotos, etiquetas y stashes, y
redactar mensajes de commit o pull requests con los CLI oficiales de IA que ya usas.

Regla fija: **nada que reescriba tu repositorio ocurre sin que lo confirmes**, y la IA
**solo propone**.

> **Versión 0.1.0.** La versión anterior en Swift/SwiftUI (solo macOS) se conserva en la
> rama [`legacy-swift`](https://github.com/jamartif/Ariadna/tree/legacy-swift) y en la
> etiqueta `swift-final` (la etiqueta `v0.1.0` de este repositorio es **la app Swift
> anterior**, no esta). La última compilación publicada de esta migración es la alfa
> [`v0.1.0-alpha.2`](https://github.com/jamartif/Ariadna/releases/tag/v0.1.0-alpha.2).
> La especificación está en [`specs/rust/`](specs/rust/) y el detalle de tareas y deuda en
> [`specs/rust/tasks.md`](specs/rust/tasks.md).

## Estado de verificación

Lo que se ha probado de verdad y dónde. Si algo no se ha ejecutado contra el servicio real,
aquí pone «no verificado»:

| Área | Estado | Evidencia |
|---|---|---|
| Motor Git: status, stage por *hunk* y por línea, commit, amend, log por cursor, ramas, merge, rebase, cherry-pick, conflictos, remotos, stash, etiquetas, clonado con PAT | **Verificado** contra `git` real | 760 funciones de prueba en Rust (72 ficheros); la CI las ejecuta en Linux, macOS y Windows |
| Interfaz | **Verificado** con Vitest y axe en jsdom; revisión visual documentada | 498 casos de prueba de UI (50 ficheros); Vitest en CI; [`specs/rust/qa-alpha2.md`](specs/rust/qa-alpha2.md) |
| Pull requests de **GitHub** | **VERIFICADO con la API real** (2026-11-11), repo privado desechable: 72 comprobaciones OK, 5 límites del propio GitHub, 0 fallos | [`specs/rust/spike-pr.md`](specs/rust/spike-pr.md) |
| Pull requests de **Azure DevOps** | **NO VERIFICADO**: no hay cuenta ni PAT en el entorno; probado solo contra servidor simulado (`wiremock`) | [`specs/rust/spike-pr.md`](specs/rust/spike-pr.md) |
| IA con **Claude** | **Verificado** con `claude 2.1.319` (flags, `claude auth status --json`, `CLAUDE_CONFIG_DIR`) | [`specs/rust/checklist-humo-ia.md`](specs/rust/checklist-humo-ia.md) |
| IA con **Codex** y **Copilot** | **NO PROBADOS** con CLI real; flags y formatos siguen experimentales | [`specs/rust/tasks.md`](specs/rust/tasks.md) §M6 |
| Instaladores | Se construyen (NSIS x64 y `.dmg` universal) pero **sin firma** | [Instalación sin firma](docs/instalacion-sin-firma.md) |

## Qué hace

- **Pestañas por repositorio**, con la sesión guardada entre arranques, deduplicación por
  ruta real (los *worktrees* comparten pestaña) e hibernación de las ocultas.
- **Cambios**: diff por *hunk* y **por línea suelta** (casillas, clic, Mayús+clic y
  teclado), preparar/quitar por bloque o por línea, y barra de commit con `Ctrl/⌘+Intro`.
- **Stash**: guardar (con o sin no seguimiento), listar, ver diff, aplicar, sacar y borrar
  con confirmación; los conflictos al aplicar caen en el panel de Conflictos.
- **Historial** paginado **por cursor** (coste constante también en repos de 100 000
  commits), con grafo, filtro local, búsqueda global (mensaje, autor, hash) y diff por
  commit.
- **Ramas**: crear, cambiar, renombrar, borrar (con segunda confirmación si no está
  fusionada) y fusionar; checkout remoto con upstream y aviso cuando divergiría.
- **Remotos**: listar con proveedor reconocido, añadir, renombrar, quitar, traer, bajar y
  subir la rama actual, y «Abrir en el navegador» / «Crear pull request».
- **Conflictos** a tres vías (nuestro/base/suyo) en un panel con resultado editable;
  marcar resuelto, continuar o abortar merge, rebase y cherry-pick.
- **Etiquetas**: listar (ligeras y anotadas), crear en HEAD o en un commit, borrar en local
  y publicar/borrar en el remoto.
- **Clonar desde una URL** (GitHub, Azure DevOps, GitLab, Bitbucket o genérica) con
  **PAT opcional**, probado y olvidado sin que Ariadna lo guarde (ver *Seguridad*).
- **Pull requests** de GitHub y Azure DevOps: listar, detalle con Markdown seguro, crear,
  bajar la rama, revisar con comentarios en línea, votar y completar/abandonar.
- **Ajustes** (General, Git, IA, Cuentas), tema claro/oscuro/sistema, paleta de comandos,
  menú nativo y atajos ES/US/DE.

Fuera de esta versión: `init`, submódulos, LFS, *worktrees* más allá de detectarlos,
rebase interactivo, cherry-pick visual, grafo de commits como vista propia, diff lado a
lado, actualizador automático y tiendas de aplicaciones.

## Instalación

Requisitos: **Windows 10 (22H2) u 11, x64** · **macOS 15 o posterior** · `git` **2.39 o
posterior** en el `PATH`. En Windows el instalador comprueba WebView2 y lo instala si
falta; no necesita administrador.

Los instaladores de la 0.1.0 **no están firmados**, así que el sistema avisará o bloqueará
según la configuración del equipo. Se descargan de la página de *releases* (alfa actual:
`Ariadna_0.1.0_x64-setup.exe` para Windows y `Ariadna_0.1.0_universal.dmg` para macOS, con
`SHA256SUMS.txt` al lado). Guía paso a paso, con lo que cambia cuando haya firma y lo que
**no** prometemos: **[docs/instalacion-sin-firma.md](docs/instalacion-sin-firma.md)**.

Resumen:

- **Windows** — SmartScreen muestra «Windows protegió tu PC»: *Más información* → *Ejecutar
  de todas formas*. Si el equipo tiene **Smart App Control** activado, el instalador
  **no se puede ejecutar** (ni con «Ejecutar de todas formas»); lo único que lo desbloquea
  de verdad es la firma o desactivar Smart App Control, que es decisión del usuario o de
  su empresa, no de Ariadna.
- **macOS** — Gatekeeper dirá que no puede verificar la app. En macOS 15 el clic derecho →
  *Abrir* **ya no basta**: hay que ir a *Ajustes del Sistema → Privacidad y seguridad →
  Abrir igualmente* (o `xattr -dr com.apple.quarantine /Applications/Ariadna.app`). Los Mac
  gestionados por una empresa pueden bloquearlo por política.

## IA (opcional, apagada hasta que la enciendas)

Ariadna **no usa claves de API ni proxies de terceros**: lanza los **CLI oficiales** con la
sesión que ya tienes en el equipo, y el prompt viaja por **stdin** (nunca por `argv`).

| Proveedor | CLI | Estado |
|---|---|---|
| Claude (Pro/Max/Ultra o Team/Enterprise) | `claude` | Verificado con `claude 2.1.319` |
| Codex (ChatGPT Free/Go/Plus/Pro/Business/Enterprise) | `codex` | Experimental, sin probar |
| GitHub Copilot (cualquier plan) | `copilot` | Experimental, sin probar |

- **Modo Auto**: Claude → Codex → Copilot, saltando solo los que no están instalados y
  solo entre los proveedores consentidos.
- **Perfiles Personal y Empresa**: cada repo se asocia a un perfil; el perfil decide qué
  proveedores se permiten, con qué consentimiento y, cuando el CLI lo permite, con qué
  directorio de configuración (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`). En **Empresa** Claude
  queda desactivado por defecto y se aísla la sesión; Copilot **comparte login** entre
  perfiles (no hay mecanismo verificado para aislarlo) y la app lo avisa.
- **Consentimiento** la primera vez por perfil y proveedor, con vista previa de lo que se
  envía; se excluyen `.env*`, `*.pem`, `id_*`, `*.key`, `*.pfx` y se redactan secretos.
- **La IA solo propone**: el resultado es una propuesta que puedes aceptar, editar o
  descartar; nunca escribe en el worktree, el índice ni las refs.

**Antes de usar IA con código de la empresa**, revisa los términos de tu cuenta y las
reglas internas: los planes individuales (Claude Pro, ChatGPT Plus, Copilot Pro) pueden usar
tus entradas para entrenar salvo que lo desactives, mientras los planes de organización lo
excluyen por contrato. Resumen con fechas y enlaces:
**[specs/rust/terminos-ia.md](specs/rust/terminos-ia.md)**.

## Seguridad

- **Tokens**: Ariadna no guarda, no cachea y no registra PAT ni tokens. Para clonar, el
  secreto se entrega a `git` **una sola vez** mediante un `GIT_ASKPASS` temporal, se usa en
  memoria (`zeroize`) y se destruye; para las pull requests se pide a `git credential
  fill` en cada llamada. Nunca aparece en `argv`, URL, `Debug` ni mensaje de error.
- **Procesos**: `git` y los CLI se lanzan sin shell, con ruta absoluta, entorno por lista
  blanca (`GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never`, sin `*_API_KEY`), `fsmonitor` y
  hooks externos desactivados en las lecturas, y árbol de procesos que se termina al cerrar
  (Job Object en Windows, grupo de procesos en macOS).
- **Repos hostiles**: rutas validadas (`../`, UNC, `\\?\`, nombres reservados, `.git` con
  variantes NTFS, `-rf`, pathspecs mágicos), refs comprobadas con `git check-ref-format` y
  parches construidos desde bytes de `git diff`.
- **Red**: la única comunicación propia es la API del proveedor del repo para las pull
  requests, solo por acción tuya o con el panel abierto; TLS siempre verificado, sin
  telemetría. Sabe usar el **proxy** (`HTTPS_PROXY`/`HTTP_PROXY`/`NO_PROXY`) y los
  **certificados del sistema** (`SSL_CERT_FILE`, `SSL_CERT_DIR`), incluidos los que instala
  una empresa: está implementado y probado en `crates/ariadna-hosting/src/net.rs` (el
  cliente de pull requests todavía construye el suyo; ver la matriz, §7).
- **Repo público**: `gitleaks` revisa cada commit (`.githooks/pre-commit`, instalable con
  `sh scripts/install-git-hooks.sh`) y la CI lo pasa sobre todo el historial.

## Arquitectura

```
UI Svelte 5 ──IPC tipado──▶ comandos Tauri ──▶ SessionManager ──▶ RepositorySession
                                                          │              │
                                                          │              ▼
                                                          │        GitEngine ──▶ ProcessRunner ──▶ git
                                                          ├──▶ AiService ──▶ AiProvider ──▶ claude | codex | copilot
                                                          └──▶ ariadna-hosting ──▶ PullRequestProvider ──▶ GitHub / Azure DevOps REST
```

El estado vive en Rust; la interfaz es una vista sin lógica de git ni de IA. Cada pieza del
workspace:

| Crate | Contenido |
|---|---|
| `crates/ariadna-process` | Lanzador de procesos: sin shell, cancelación cooperativa, *timeout*, tope de bytes, Job Object / grupo de procesos |
| `crates/ariadna-git` | Motor Git y parsers (`status --porcelain=v2 -z`, diff, log, ramas, remotos, conflictos diff3); sesión con cola serial por repo, stash, parches por línea (`partial`), clonado, credenciales y *askpass* |
| `crates/ariadna-ai` | Proveedores de IA vía CLI oficial, entorno por lista blanca, prompts, redacción de secretos, perfiles y cola |
| `crates/ariadna-hosting` | Reconocimiento de proveedor (`RemoteInfo`), cliente HTTP (`reqwest` + `rustls`) con proxy y CA del sistema (`net.rs`), pull requests de GitHub y Azure DevOps |
| `src-tauri/` | Aplicación Tauri: núcleo testeable sin ventana, comandos IPC, vigilante de archivos, menú, ajustes y `session.json` |
| `ui/` | Interfaz Svelte 5 + TypeScript (CodeMirror 6 para el diff) |

## Desarrollo

Requisitos: Rust estable (`rust-toolchain.toml`), Node 22 y las dependencias de Tauri 2 de
tu sistema.

```sh
# Rust: formato, lint y pruebas
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast

# Interfaz
cd ui
npm ci
npm run check && npm test && npm run build
npm run dev          # Vite en 127.0.0.1:5173 (backend simulado si se abre en el navegador)
```

En Windows conviene compilar desde **WSL** (`cargo` sobre `/mnt/c`): algunos equipos
bloquean los *build scripts* recién compilados con Smart App Control y `cargo build` falla
con «Una directiva de Control de aplicaciones bloqueó este archivo» (error 4551). El perfil
de desarrollo ya compila esos build scripts con optimización ligera para reducirlo, pero
WSL es la vía fiable.

Pruebas de rendimiento (`#[ignore]`, repos sintéticos de 50 000 archivos y 100 000 commits):

```sh
cargo test -p ariadna-git --test perf -- --ignored --nocapture
```

## Documentación

| Documento | Contenido |
|---|---|
| [`docs/instalacion-sin-firma.md`](docs/instalacion-sin-firma.md) | Abrir las alfas sin firmar en Windows (SmartScreen, Smart App Control) y macOS (Gatekeeper) |
| [`specs/rust/spec.md`](specs/rust/spec.md) | Especificación de la migración (requisitos G-xx, PR-xx, RF-IAx, NF-x) |
| [`specs/rust/tasks.md`](specs/rust/tasks.md) | Hitos, tareas y deuda conocida |
| [`specs/rust/matriz-trazabilidad.md`](specs/rust/matriz-trazabilidad.md) | Requisito → tarea → prueba real, con huecos marcados |
| [`specs/rust/terminos-ia.md`](specs/rust/terminos-ia.md) | Términos de uso de Anthropic, OpenAI y GitHub Copilot: individual frente a organización |
| [`specs/rust/spike-pr.md`](specs/rust/spike-pr.md) | Qué está verificado de las pull requests y qué no |
| [`specs/rust/checklist-humo-ia.md`](specs/rust/checklist-humo-ia.md) | Humo manual con CLI de IA reales |
| [`specs/rust/ipc.md`](specs/rust/ipc.md) | Contrato IPC entre Rust y la interfaz |

## Code signing policy (política de firma de código)

> **Free code signing provided by [SignPath.io](https://signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).**

- **Artefactos firmados:** instalador NSIS de Windows y su ejecutable, construidos solo en GitHub Actions desde este repositorio.
- **Committers y revisores:** [@jamartif](https://github.com/jamartif).
- **Aprobadores de firma:** [@jamartif](https://github.com/jamartif); cada release se aprueba a mano.
- **Privacidad:** *This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.* Sin telemetría; la red solo se usa cuando tú lo pides (operaciones de git contra tus remotos y, si activas la IA, el CLI oficial que elijas).
- **macOS** no entra en esta política: el `.dmg` lleva firma ad-hoc hasta que haya un certificado Developer ID.

Detalle completo en [`docs/firma-de-codigo.md`](docs/firma-de-codigo.md).

## Licencia

[MIT](LICENSE). Úsalo, modifícalo y redistribúyelo; si publicas una versión modificada,
conserva el aviso de copyright.
