# Tareas — migración a Rust

`[ ]` pendiente · `[x]` hecho. Cada tarea cita el requisito del spec (G-xx, RF-IAx, NF-x, §) y, si aplica, el test Swift que define la paridad.

## M0 — Preparación
- [x] Spec v1 revisada con EGM (3 revisores de contexto limpio): `revision-egm.md`
- [ ] Spike CLI de IA: verificar `codex` y `copilot` (flags, stdin, formato de salida, sandbox en Windows). Documentar versiones
- [ ] Spike `claude`: probar `--setting-sources ""`, salida JSON, detección de sesión sin leer credenciales
- [ ] Spike WebView2: bootstrapper en NSIS y Ctrl+Tab en WebView2
- [ ] Leer y citar con fecha los términos de uso de Anthropic, OpenAI y GitHub sobre suscripción en apps de terceros (RF-IA10)
- [ ] Matriz de trazabilidad: G-01…G-15, RF-IA → tarea → test (generar desde `specs/spec.md` y `Sources/`)
- [ ] Inventario de los 36 + 8 tests Swift con su tarea Rust
- [ ] Generar y versionar fixtures de git (con metadatos de versión)
- [ ] Mockups de las 5 pantallas clave y tokens de diseño
- [ ] Ampliar `.gitignore` (`target/`, `node_modules/`) y `.gitleaks.toml`
- [ ] Javier decide las PROPUESTAS D7, D10, D11, D13

## M1 — Fundación
- [ ] `Cargo.toml` workspace, `rust-toolchain.toml` (≥ 1.77.2), rustfmt, clippy, cargo-deny
- [ ] CI Rust: macos-15, windows-2025, windows-11-arm, ubuntu; job agregador; caché
- [ ] `ProcessRunner`: timeout, cancelación, stdin en tarea aparte, stdout/stderr en paralelo, tope de bytes, Job Object (kill-on-close, suspendido) / grupo de procesos, `CREATE_NO_WINDOW` (NF-7) · ProcessAndStoreTests
- [ ] Localizar `git` una vez, ruta absoluta, versión ≥ 2.39 (§4.1)
- [ ] `PathSafety` con la lista cerrada de §4.2 · ParserTests / GitCLIEngineTests
- [ ] `PorcelainParser` v2 -z · ParserTests
- [ ] `DiffParser` · ParserTests
- [ ] `ConflictParser` con diff3 · ParserTests
- [ ] Almacén de recientes y favoritos + importación del `recents.json` Swift (G-15)
- [ ] Fuzz/propiedades de los parsers

## M2 — Motor Git
- [ ] Trait `GitServing` y `GitCliEngine` con endurecimiento de §4.1
- [ ] Cola serial por clave de repo; lecturas con `GIT_OPTIONAL_LOCKS=0`; refrescos coalescidos con `revision` (§3.2) · OperationQueueTests
- [ ] G-01 abrir repo, error claro
- [ ] G-02 status
- [ ] G-03 stage/unstage por archivo
- [ ] G-04 stage/unstage por hunk, incluida la matriz de `core.autocrlf` y `.gitattributes`
- [ ] G-06 commit; mensaje vacío rechazado; aviso en HEAD detached
- [ ] G-07 amend seguro
- [ ] G-08 log y diff · G-09 búsqueda global
- [ ] Contrato IPC: comandos, enum de errores, eventos, `operation_id`, `cancel_operation` (§3.3)
- [ ] Reintento acotado ante `index.lock` ajeno
- [ ] Tests con 50 000 archivos (rutas por stdin, NF-3)

## M3 — Ramas, remotos, merge
- [ ] G-10 ramas
- [ ] G-11 fetch, pull merge/rebase, push `--set-upstream`; timeouts de red y GCM
- [ ] G-12 merge ff / no-ff / ff-only / ya al día
- [ ] G-13 conflictos a tres vías, escritura binaria preservando EOL y BOM
- [ ] G-14 marcar resuelto, continuar, abortar
- [ ] G-05 descartar con confirmación
- [ ] Test `pull --rebase` con conflicto, abort y continue
- [ ] Test de cherry-pick con conflicto (pendiente heredado)
- [ ] `DubiousOwnership` con confirmación para `safe.directory`

## M4 — Shell de la app
- [ ] Proyecto Tauri 2 + Svelte 5 + TS; `tauri-specta`; CI falla si el binding difiere
- [ ] CSP estricta y capabilities mínimas; sin `{@html}`
- [ ] Tokens, tema claro/oscuro/sistema, Inter y JetBrains Mono
- [ ] `SessionManager`: ciclo de vida (§3.1), clave de repo, dedupe, cierre con confirmación
- [ ] Confirmaciones nativas desde Rust con token de un solo uso (§6)
- [ ] Pestañas: hibernación, reorden por teclado, indicadores, tope de 20
- [ ] `session.json` versionado y atómico, con respaldo y recuperación
- [ ] Watcher (`notify`): debounce, `.git/` aparte, overflow → refresco completo, sondeo en UNC
- [ ] Bienvenida: ruta, Examinar, recientes, favoritos
- [ ] Paleta de comandos y registro único de atajos (ES/US/DE)
- [ ] Errores: toast, banner de pestaña, pantalla de recuperación

## M5 — Pantallas (→ alpha.1)
- [ ] Cambios: preparados / sin preparar / sin seguimiento / ignorados / conflictos
- [ ] Diff por hunk (CodeMirror 6), virtualizado, límites de NF-5
- [ ] Barra de commit (Ctrl/⌘+Intro), amend
- [ ] Historial paginado con filtro local y búsqueda global (NF-4)
- [ ] Ramas y remotos
- [ ] Estudio de conflicto a tres paneles + resultado
- [ ] Ajustes: pull con rebase, ruta de git
- [ ] Vitest de componentes
- [ ] Publicar `0.1.0-alpha.1` (macOS y Windows x64)

## M6 — IA (→ alpha.2)
- [ ] Trait `AiProvider`, `AiSuggestion` con `revision` y estado obsoleto
- [ ] Consentimiento por proveedor, vista previa, exclusión de sensibles, redactor de secretos, marcado de contenido no fiable (§5.1)
- [ ] `ClaudeCli` (RF-IA1/3/5/6/7)
- [ ] `CodexCli` · `CopilotCli` (según resultado del spike; experimental si no se verifican)
- [ ] Resolución de binarios en Windows y shims (RF-IA2); test de lista blanca de argumentos
- [ ] Entorno por lista blanca y variables prohibidas (RF-IA3)
- [ ] Detección de disponibilidad, versión y sesión ejecutando el CLI (RF-IA4)
- [ ] Cola de IA: 2 simultáneas, límite por minuto, cancelación al cerrar pestaña (RF-IA8)
- [ ] Modo Auto con reglas de RF-IA9
- [ ] Acciones: mensaje de commit, resumen, explicar commit, propuesta de conflicto
- [ ] Rechazo de propuestas de conflicto que salen de los hunks
- [ ] Hoja de propuesta: copiar, usar, regenerar, descartar
- [ ] CLI falso en CI: entorno, argv, stdin, secreto de prueba · AITests
- [ ] Humo manual con CLI reales en macOS y Windows (checklist)
- [ ] Publicar `0.1.0-alpha.2`

## M7 — Release
- [ ] Accesibilidad: ARIA, `forced-colors`, zoom 200 %, axe en E2E; revisión NVDA y VoiceOver
- [ ] Rendimiento contra NF-1…NF-6 con fixtures sintéticos
- [ ] E2E con `tauri-driver` (macOS y Windows x64)
- [ ] Regresión visual con baselines por SO
- [ ] Capturas automáticas en CI
- [ ] `.dmg` universal; NSIS x64 y ARM64 con bootstrapper de WebView2
- [ ] Prueba de instalación y desinstalación limpias en VM
- [ ] Documentar apertura de app no notarizada y aviso de SmartScreen
- [ ] CHANGELOG y README nuevo
- [ ] Tag `swift-final`, rama `legacy-swift`, retirar Swift de `main`
- [ ] Publicar `v0.1.0`
