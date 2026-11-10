# Tareas — migración a Rust

`[ ]` pendiente · `[x]` hecho. Cada tarea cita el requisito del spec (G-xx, RF-IAx, NF-x, §) y, si aplica, el test Swift que define la paridad.

## M0 — Preparación
- [x] Spec v1 revisada con EGM (3 revisores de contexto limpio): `revision-egm.md`
- [ ] Spike CLI de IA: verificar `codex` y `copilot` (flags, stdin, formato de salida, sandbox en Windows). Documentar versiones
- [ ] Spike `claude`: probar `--setting-sources ""`, salida JSON, detección de sesión sin leer credenciales
- [ ] Spike WebView2: bootstrapper en NSIS y Ctrl+Tab en WebView2
- [ ] Leer y citar con fecha los términos de Anthropic, OpenAI y GitHub **para uso en empresa**: planes individuales vs organización, y apps de terceros que lanzan el CLI oficial (RF-IA10)
- [x] Licencia MIT (`LICENSE`, D18)
- [ ] Spike de perfiles: aislar el login por proveedor con directorio de configuración propio (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, Copilot a verificar) y detectar la cuenta activa
- [ ] Solicitar SignPath Foundation para el proyecto (D17) y leer sus condiciones
- [ ] Matriz de trazabilidad: G-01…G-15, RF-IA → tarea → test (generar desde `specs/spec.md` y `Sources/`)
- [ ] Inventario de los 58 + 8 tests Swift con su tarea Rust
- [ ] Generar y versionar fixtures de git (con metadatos de versión)
- [ ] Mockups de las 5 pantallas clave y tokens de diseño
- [ ] Ampliar `.gitignore` (`target/`, `node_modules/`) y `.gitleaks.toml`
- [ ] Javier decide las PROPUESTAS D7, D10, D11, D13

## M1 — Fundación

> Estado: implementado en `crates/ariadna-process` y `crates/ariadna-git` (203 tests en verde entre los tres crates de Rust, probados en Linux/WSL; la CI valida macOS y Windows). Pendiente: fixtures contra git real en CI, fuzz de parsers y `core.longpaths`.
- [ ] Cambiar a macOS 15 como mínimo en la configuración de Tauri y en la CI (D15)
- [x] `Cargo.toml` workspace, `rust-toolchain.toml` (≥ 1.77.2), rustfmt, clippy, cargo-deny
- [ ] CI Rust: macos-15, windows-2025, windows-11-arm, ubuntu; job agregador; caché
- [x] `ProcessRunner`: timeout, cancelación, stdin en tarea aparte, stdout/stderr en paralelo, tope de bytes, Job Object (kill-on-close, suspendido) / grupo de procesos, `CREATE_NO_WINDOW` (NF-7) · ProcessAndStoreTests
- [x] Localizar `git` una vez, ruta absoluta, versión ≥ 2.39 (§4.1)
- [x] `PathSafety` con la lista cerrada de §4.2 · ParserTests / GitCLIEngineTests
- [x] `PorcelainParser` v2 -z · ParserTests
- [x] `DiffParser` · ParserTests
- [x] `ConflictParser` con diff3 · ParserTests
- [x] Almacén de recientes y favoritos + importación del `recents.json` Swift (G-15)
- [ ] Fuzz/propiedades de los parsers
- [x] Parsers de log, ramas y remotos, y `AmendDraft` (los necesitan 7 tests Swift)
- [ ] Atajos ⌘O y ⌘R, menús nativos y tamaño mínimo de ventana 980×640 (huecos detectados en la matriz)

## M2 — Motor Git

> Estado: implementado (`GitCliEngine`, `GitRunner`, `RepositorySession`). 125 tests nuevos en `ariadna-git` contra git real. Pendiente: contrato IPC de Tauri (M4), el test de 50 000 archivos (hoy se prueba con 5 000 rutas) y `core.longpaths` en CI de Windows.
- [x] Trait `GitServing` y `GitCliEngine` con endurecimiento de §4.1
- [x] Cola serial por clave de repo; lecturas con `GIT_OPTIONAL_LOCKS=0`; refrescos coalescidos con `revision` (§3.2) · OperationQueueTests
- [x] G-01 abrir repo, error claro
- [x] G-02 status
- [x] G-03 stage/unstage por archivo
- [x] G-04 stage/unstage por hunk, incluida la matriz de `core.autocrlf` y `.gitattributes`
- [x] G-06 commit; mensaje vacío rechazado; aviso en HEAD detached
- [x] G-07 amend seguro
- [x] G-08 log y diff · G-09 búsqueda global
- [ ] Contrato IPC: comandos, enum de errores, eventos, `operation_id`, `cancel_operation` (§3.3)
- [x] Reintento acotado ante `index.lock` ajeno
- [ ] Tests con 50 000 archivos (rutas por stdin, NF-3)

## M3 — Ramas, remotos, merge

> Estado: implementado y probado con remoto bare real, rebase y cherry-pick con conflicto. Pendiente: flujo de confirmación de `safe.directory` (el error `DubiousOwnership` ya se detecta).
- [ ] `RemoteInfo::parse` (GitHub, GHE, Azure DevOps en sus tres formas, GitLab, Bitbucket, otros) con ≥ 20 URLs de prueba y redacción de credenciales (D19)
- [ ] Error tipado `Auth` para fetch/pull/push con mensaje accionable (única excepción de parseo, aislada y con tests)
- [ ] G-16 `clone_repository`: validación de URL (`ext::`, `-`), destino vacío, cancelación que borra solo lo creado, progreso, sin submódulos; tests con remoto bare local
- [ ] Credenciales: `credential approve / reject / ls-remote` por stdin, zeroize del secreto, sin registro ni eco en errores; tests con un credential helper falso
- [ ] Ayuda de PAT por proveedor (Azure DevOps, GitHub, GitLab, Bitbucket) con enlaces https y permisos mínimos
- [x] G-10 ramas
- [x] G-11 fetch, pull merge/rebase, push `--set-upstream`; timeouts de red y GCM
- [x] G-12 merge ff / no-ff / ff-only / ya al día
- [x] G-13 conflictos a tres vías, escritura binaria preservando EOL y BOM
- [x] G-14 marcar resuelto, continuar, abortar
- [x] G-05 descartar con confirmación
- [x] Test `pull --rebase` con conflicto, abort y continue
- [x] Test de cherry-pick con conflicto (pendiente heredado)
- [ ] `DubiousOwnership` con confirmación para `safe.directory`

## M4 — Shell de la app

> Segunda oleada hecha: rediseño visual con barra de título propia, botón de IA para el commit (`ai_generate_commit_message`, consentimiento Personal/Empresa), clonado con PAT (askpass temporal, nunca guardado), historial virtualizado con grafo y ramas (crear, cambiar, renombrar, borrar, fusionar sin conflictos). Deuda: test de cancelar clonado ignorado con git 2.53; retirar el aviso de la barra lateral sobre ramas. (Panel de remotos, menú nativo, hibernación y `opener` hechos en la tercera oleada.)

> Estado: backend Tauri (`AppCore`, pestañas, 34 comandos IPC, `session.json`, fixtures) e interfaz Svelte (diseño claro/oscuro, pestañas, bienvenida, cambios, diff por hunk, commit, paleta) **hechos y validados en CI** (Windows x64/ARM64, macOS 15, Ubuntu); el instalador NSIS y el `.dmg` se construyen. Pendiente: pantalla de recuperación, clonado (D20) en la bienvenida. Hechos después: hibernación de pestañas, `opener` y menú nativo.
- [x] Proyecto Tauri 2 + Svelte 5 + TS; `tauri-specta`; CI falla si el binding difiere
- [x] CSP estricta y capabilities mínimas; sin `{@html}`
- [ ] **Barra de título duplicada** (hallazgo en la ventana nativa): Tauri dibuja la barra del sistema y la UI dibuja la suya. Poner `decorations: false` con controles propios (minimizar, maximizar, cerrar) en Windows y Linux, y `titleBarStyle: Overlay` con los semáforos en macOS; verificar el arrastre (`data-tauri-drag-region`)
- [x] Tokens, tema claro/oscuro/sistema, Inter y JetBrains Mono
- [x] `SessionManager`: ciclo de vida (§3.1), clave de repo, dedupe, cierre con confirmación
- [x] Confirmaciones nativas desde Rust con token de un solo uso (§6)
- [x] Pestañas: tope blando de 20 con aviso (Rust `OpenResult.warning` + UI al crear pestaña), reorden por ratón y teclado (`Alt+Mayús+←/→`, que ya no roba «seleccionar por palabras» a los campos de texto), indicadores de cambios/conflictos/ocupado/missing revisados (a11y y tests), hibernación a los 10 min (la UI libera snapshot e historial; Rust ya soltaba el vigilante de las ocultas) — `ipc.md` «Pestañas: tope, orden con teclado, indicadores e hibernación». Sin hacer: sondeo ligero de contadores de pestañas ocultas (spec §8, cada 60 s)
- [x] Menú nativo (Archivo, Editar, Ver, Ventana, Ayuda; en macOS también «Ariadna») generado de `core/menu.rs` con aceleradores del registro único (`⌘/Ctrl+O` abrir repo, `⌘/Ctrl+R` refrescar…), `menu-action` → `runMenuAction`, tamaño mínimo 980×640 y config de ventana verificadas por test (`window_config`), `trafficLightPosition` para los semáforos de macOS. **Sin verificar a ojo**: macOS (semáforos, menú) y que Windows/Linux no necesiten barra nativa (no se adjunta)
- [x] `session.json` versionado y atómico, con respaldo y recuperación
- [x] Watcher (`notify`): debounce 300 ms/tope 2 s, `.git/` aparte, solo la pestaña activa, overflow → refresco completo + sondeo (UNC, 9p/WSL), refresco al recuperar el foco, toast único si degrada; la revisión sube con cada cambio externo y un refresco sin cambios no publica
- [ ] Bienvenida: ruta, Examinar, recientes, favoritos, **Clonar desde URL** y formulario de credenciales (PAT o «iniciar sesión con el navegador») (D20)
- [x] Plugin `opener` con capability mínima para «Abrir en el navegador» y «Crear pull request» (D19): la ventana no tiene ningún permiso `opener:*`; `open_remote_url` (Rust) construye la URL desde la config real del remoto y valida https + host del remoto, `open_external_url` mantiene su lista cerrada (`ipc.md` «Remotos y “Abrir en el navegador”»)
- [x] Paleta de comandos y registro único de atajos (ES/US/DE)
- [x] Errores: toast, banner de pestaña, pantalla de recuperación

## M5 — Pantallas (→ alpha.1)

> Estado: Cambios, diff por hunk y barra de commit hechos (225 tests de UI). Pendiente: historial real, ramas (checkout/crear/fusionar), conflictos, ajustes, regresión visual.
- [x] Cambios: preparados / sin preparar / sin seguimiento / ignorados / conflictos
- [x] Diff por hunk (CodeMirror 6), virtualizado, límites de NF-5
- [x] Barra de commit (Ctrl/⌘+Intro), amend
- [ ] Historial paginado con filtro local y búsqueda global (NF-4)
- [x] Ramas y remotos, con icono del proveedor y acciones «Abrir en el navegador» / «Crear pull request» (D19): sección «Remotos» en el panel Ramas (lista con proveedor, traer/bajar/subir la rama actual, añadir/renombrar/quitar con confirmación nativa) y botones en el detalle de rama. `list_remotes`, `add_remote`, `rename_remote`, `remove_remote`, `open_remote_url`. Solo enlaces: la API de PR es M8
- [x] Estudio de conflicto a tres paneles + resultado (panel «Conflictos», `merge_start`/`merge_state`/`resolve_conflict`/`merge_continue`/`merge_abort`; textarea con resultado editable, sin CodeMirror: no está en el proyecto)
- [x] Ajustes: pantalla (General, Git, IA, Cuentas, Acerca de), `settings.json` versionado y atómico, pull con rebase, ruta de git con «Detectar», reglas repo → perfil (`ipc.md` «Ajustes»)
- [x] Vitest de componentes
- [ ] Publicar `0.1.0-alpha.1` (macOS y Windows x64)

## M6 — IA (→ alpha.2)

> Estado: la capa `crates/ariadna-ai` está implementada con un CLI falso. **Codex y Copilot siguen sin verificar** (flags como constantes `SPIKE` en `commands.rs`); los fixtures de salida de los tres son reconstruidos, no capturados. Falta la UI (hoja de propuesta, Ajustes, botón «Iniciar sesión») y el humo manual con CLI reales.
- [x] Trait `AiProvider`, `AiSuggestion` con `revision` y estado obsoleto
- [ ] Consentimiento por proveedor, vista previa, exclusión de sensibles, redactor de secretos, marcado de contenido no fiable (§5.1)
- [x] `ClaudeCli` (RF-IA1/3/5/6/7)
- [x] `CodexCli` · `CopilotCli` (según resultado del spike; experimental si no se verifican)
- [x] Resolución de binarios en Windows y shims (RF-IA2); test de lista blanca de argumentos
- [x] Entorno por lista blanca y variables prohibidas (RF-IA3)
- [x] Detección de disponibilidad, versión y sesión ejecutando el CLI (RF-IA4)
- [x] Cola de IA: 2 simultáneas, límite por minuto, cancelación al cerrar pestaña (RF-IA8)
- [x] Modo Auto con reglas de RF-IA9
- [ ] Perfiles Personal/Empresa: modelo hecho en `ariadna-ai`; falta asociación repo→perfil con reglas por remoto, confirmación al cambiar, perfil visible en pestaña y hoja (D16)
- [ ] Directorio de configuración aislado por perfil y botón «Iniciar sesión» del CLI; aviso si el proveedor comparte login
- [x] Consentimiento por perfil y proveedor; Claude desactivado por defecto en Empresa
- [ ] Mostrar cuenta activa por proveedor cuando el CLI la exponga (RF-IA12)
- [x] Acciones: mensaje de commit, resumen, explicar commit, propuesta de conflicto
- [x] Rechazo de propuestas de conflicto que salen de los hunks
- [ ] Hoja de propuesta: copiar, usar, regenerar, descartar
- [x] CLI falso en CI: entorno, argv, stdin, secreto de prueba · AITests
- [ ] Humo manual con CLI reales en macOS y Windows, con cuentas de organización (checklist)
- [ ] Publicar `0.1.0-alpha.2`

## M8 — Pull requests (→ alpha.3)
- [ ] Spike con PAT real: endpoints y permisos mínimos de Azure DevOps REST y GitHub REST para listar, crear, hilos, voto y completar (PR-02…PR-08); documentar versiones de API
- [ ] Crate `ariadna-hosting`: `RemoteInfo`, `trait PullRequestProvider`, `AuthMethod`, cliente `reqwest` con `rustls`, límites de seguridad de red (§4.4)
- [ ] Obtención del secreto con `git credential fill` no interactivo, `zeroize`, sin registro ni eco (D22)
- [ ] Azure DevOps: listar, detalle (revisores, políticas, work items), crear, abandonar/reabrir
- [ ] Azure DevOps: diff de la PR, hilos (ver, crear, responder, resolver), voto, completar con estrategia y *auto-complete*
- [ ] GitHub: listar, detalle, crear, reviews, comentarios en línea, merge/squash/rebase, cerrar/reabrir
- [ ] Bajar la rama de una PR (incluido `refs/pull/N/head` de forks)
- [ ] Plantillas de PR del repo (`pull_request_template.md`) y push previo si la rama no está publicada
- [ ] Errores tipados `AUTH` (con el permiso que falta), `NOT_FOUND`, `POLICY`, `RATE_LIMITED`, `NETWORK`
- [ ] Comandos IPC de PR (ipc.md) y confirmaciones nativas para completar, abandonar y votar con rechazo
- [ ] UI: panel de PR (lista con filtros, detalle con Markdown sin `{@html}`, pestaña de archivos con comentarios en línea, votar, completar, crear)
- [ ] «Generar título y descripción con IA» (propuesta) enlazado a M6
- [ ] Pruebas contra servidor simulado (`wiremock`) con 401/403/404/409/429 y humo manual con cuentas reales (checklist)
- [ ] Publicar `0.1.0-alpha.3`

## M7 — Release
- [ ] Accesibilidad: ARIA, `forced-colors`, zoom 200 %, axe en E2E; revisión NVDA y VoiceOver
- [ ] Rendimiento contra NF-1…NF-6 con fixtures sintéticos
- [ ] E2E con `tauri-driver` (macOS y Windows x64)
- [ ] Regresión visual con baselines por SO
- [ ] Capturas automáticas en CI
- [ ] Respetar proxy y CA del sistema (mejor esfuerzo, sin pruebas dedicadas)
- [ ] `.dmg` universal con firma ad-hoc y guía de «Abrir igualmente» (Developer ID y notarización solo si hay Apple Developer Program); NSIS por usuario (x64, ARM64 mejor esfuerzo) con comprobación de WebView2
- [ ] Firma de binarios e instalador según la opción elegida en D17 (requisito de `v0.1.0`; alfas sin firmar)
- [ ] Prueba de instalación y desinstalación limpias en VM
- [ ] Documentar apertura de las alfas sin firmar (Gatekeeper / SmartScreen)
- [ ] CHANGELOG y README nuevo
- [ ] Tag `swift-final`, rama `legacy-swift`, retirar Swift de `main`
- [ ] Publicar `v0.1.0`

## Después de v0.1 (no entra en esta entrega)
- [ ] `policy.json` administrable: desactivar IA, proveedores permitidos, tope de contexto (RF-IA11)
- [ ] Instalador MSI para despliegue gestionado
- [ ] Mica y vibrancy (D11)
- [ ] Identidad de commit por perfil
- [ ] Actualizador automático
