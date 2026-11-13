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

> Estado: implementado en `crates/ariadna-process` y `crates/ariadna-git` (203 tests en verde entre los tres crates de Rust, probados en Linux/WSL; la CI valida macOS y Windows). Pendiente: fixtures contra git real en CI y `core.longpaths`.
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
- [x] Fuzz/propiedades de los parsers (`proptest`, `tests/parser_props.rs`: sin pánicos, roundtrip del diff, contadores)
- [x] Parsers de log, ramas y remotos, y `AmendDraft` (los necesitan 7 tests Swift)
- [ ] Atajos ⌘O y ⌘R, menús nativos y tamaño mínimo de ventana 980×640 (huecos detectados en la matriz)

## M2 — Motor Git

> Estado: implementado (`GitCliEngine`, `GitRunner`, `RepositorySession`). 125 tests nuevos en `ariadna-git` contra git real. Pendiente: contrato IPC de Tauri (M4), `core.longpaths` en CI de Windows.
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
- [x] Tests con 50 000 archivos (rutas por stdin, NF-3): `tests/perf.rs` (`#[ignore]`) y `tests/bulk_paths.rs`; informe en `rendimiento.md`

## M3 — Ramas, remotos, merge

> Estado: implementado y probado con remoto bare real, rebase y cherry-pick con conflicto. `DubiousOwnership` se detecta y se ofrece `safe.directory` con confirmación nativa (solo la carpeta concreta).
- [ ] `RemoteInfo::parse` (GitHub, GHE, Azure DevOps en sus tres formas, GitLab, Bitbucket, otros) con ≥ 20 URLs de prueba y redacción de credenciales (D19)
- [x] Error tipado `Auth` para fetch/pull/push con mensaje accionable (única excepción de parseo, aislada y con tests: `ariadna-git::auth` + `tests/auth_network.rs`, incluido el `pull` que sale con 1)
- [ ] G-16 `clone_repository`: validación de URL (`ext::`, `-`), destino vacío, cancelación que borra solo lo creado, progreso, sin submódulos; tests con remoto bare local
- [x] Credenciales: `credential approve / reject / ls-remote` por stdin, zeroize del secreto, sin registro ni eco en errores; tests con un credential helper falso (`ariadna-git::credentials` + `tests/credentials.rs`, `Secret` sin `Clone`/`Serialize`)
- [ ] Ayuda de PAT por proveedor (Azure DevOps, GitHub, GitLab, Bitbucket) con enlaces https y permisos mínimos
- [x] G-10 ramas
- [x] G-11 fetch, pull merge/rebase, push `--set-upstream`; timeouts de red y GCM
- [x] G-12 merge ff / no-ff / ff-only / ya al día
- [x] G-13 conflictos a tres vías, escritura binaria preservando EOL y BOM
- [x] G-14 marcar resuelto, continuar, abortar
- [x] G-05 descartar con confirmación
- [x] Test `pull --rebase` con conflicto, abort y continue
- [x] Test de cherry-pick con conflicto (pendiente heredado)
- [x] `DubiousOwnership` con confirmación para `safe.directory`: `open_repository` pregunta, añade **solo esa carpeta** (`git config --global --add safe.directory`, nunca `*`) y reintenta una vez; `ariadna_git::safe_directory` + tests con `GIT_CONFIG_GLOBAL` temporal

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
- [x] Historial por cursor (NF-4): `log_page` acepta `cursor` opcional (la *frontera* del historial cargado, no solo el último hash: con merges hace falta toda la cola de `git log`) y continúa con `git log <frontera>` sin `--skip`; coste constante en repos grandes (medidas en `rendimiento.md`). Probado contra `--skip` con historiales generados con merges, octopus y fechas repetidas; el contrato antiguo (`skip`) sigue vigente. `ariadna_git::history_cursor`, `GitServing::log_after`, `Frontier` en la UI
- [x] Etiquetas (tags): `list_tags`, `create_tag` (HEAD o un commit; con mensaje = anotada), `delete_tag` y `delete_remote_tag` (confirmación nativa), `push_tag`; validación de `git check-ref-format` y errores tipados `INVALID_TAG_NAME` / `TAG_EXISTS` / `TAG_NOT_FOUND`. UI: sección «Etiquetas» en Ramas, «Crear etiqueta aquí» en el detalle del commit y chips en la lista del historial. Sin estado «publicada» (exigiría red)
- [x] Estudio de conflicto a tres paneles + resultado (panel «Conflictos», `merge_start`/`merge_state`/`resolve_conflict`/`merge_continue`/`merge_abort`; textarea con resultado editable, sin CodeMirror: no está en el proyecto)
- [x] Ajustes: pantalla (General, Git, IA, Cuentas, Acerca de), `settings.json` versionado y atómico, pull con rebase, ruta de git con «Detectar», reglas repo → perfil (`ipc.md` «Ajustes»)
- [x] Vitest de componentes
- [x] Líneas sueltas dentro de un hunk (`stage_lines` / `unstage_lines`): casillas por línea, clic, Mayús+clic, teclado (↑/↓, Espacio, Mayús+↑/↓, Esc, `Mod+Shift+Intro`); parche parcial (`ariadna-git::partial`) con `git apply --cached --recount` por stdin; CRLF, sin salto final, archivos nuevos, rutas con espacios/Unicode; rechazo claro (`PATCH_REJECTED`)
- [x] Stash: `stash_list/push/apply/pop/drop/diff` (identidad por SHA, `drop` con confirmación nativa, conflictos al aplicar/sacar → panel de Conflictos); panel «Stashes», sección en la barra lateral, botón «Guardar cambios en stash» en Cambios, atajos y paleta; mock en el backend simulado; i18n
- [ ] Publicar `0.1.0-alpha.1` (macOS y Windows x64)

## M6 — IA (→ alpha.2)

> Estado: la capa `crates/ariadna-ai` está implementada con un CLI falso y la UI de IA (hoja de propuesta, Ajustes con perfiles aislados, cuenta activa y botón «Iniciar sesión») está hecha sobre el backend simulado y sobre el CLI falso de Rust. **Codex y Copilot siguen sin verificar** (flags y `codex login`/`copilot login` como constantes `SPIKE` en `commands.rs`; `CODEX_HOME` según la documentación; Copilot sin mecanismo de aislamiento); los fixtures de salida de los tres son reconstruidos, no capturados. **Claude sí está verificado** con `claude 2.1.319`: `claude auth status --json` (cuenta) y `CLAUDE_CONFIG_DIR` (aislamiento). Falta el humo manual con CLI reales: [checklist-humo-ia.md](checklist-humo-ia.md).
- [x] Trait `AiProvider`, `AiSuggestion` con `revision` y estado obsoleto
- [ ] Consentimiento por proveedor, vista previa, exclusión de sensibles, redactor de secretos, marcado de contenido no fiable (§5.1)
- [x] `ClaudeCli` (RF-IA1/3/5/6/7)
- [x] `CodexCli` · `CopilotCli` (según resultado del spike; experimental si no se verifican)
- [x] Resolución de binarios en Windows y shims (RF-IA2); test de lista blanca de argumentos
- [x] Entorno por lista blanca y variables prohibidas (RF-IA3)
- [x] Detección de disponibilidad, versión y sesión ejecutando el CLI (RF-IA4)
- [x] Cola de IA: 2 simultáneas, límite por minuto, cancelación al cerrar pestaña (RF-IA8)
- [x] Modo Auto con reglas de RF-IA9
- [ ] Perfiles Personal/Empresa: modelo, reglas repo → perfil por remoto (Ajustes), consentimiento por perfil, aislamiento de sesión y perfil visible en la hoja de propuesta hechos; falta confirmación al cambiar el perfil de un repo y el perfil visible en la pestaña (D16)
- [x] Directorio de configuración aislado por perfil y proveedor (`ai-profiles/<perfil>/<proveedor>` con `CLAUDE_CONFIG_DIR` / `CODEX_HOME`; Empresa aísla por defecto, Personal usa la sesión normal y puede aislar) y botón «Iniciar sesión» (`ai_login`: abre terminal en Windows/macOS o muestra el comando; nunca ve credenciales). Aviso cuando el proveedor comparte login (Copilot). **Verificado solo con Claude**; Codex según documentación, Copilot sin aislamiento
- [x] Consentimiento por perfil y proveedor; Claude desactivado por defecto en Empresa
- [x] Mostrar cuenta activa por proveedor cuando el CLI la exponga (RF-IA12): `ai_accounts` con `claude auth status --json` (verificado); Codex y Copilot «no disponible» sin ejecutar nada ni leer credenciales
- [x] Acciones: mensaje de commit, resumen, explicar commit, propuesta de conflicto
- [x] Rechazo de propuestas de conflicto que salen de los hunks
- [x] Hoja de propuesta reutilizable (`ProposalSheet`): copiar, usar, regenerar, descartar; integrada en `CommitBar` («Ver propuesta») sin cambiar el flujo de relleno + «Deshacer»
- [x] `ai_generate_pull_request` (backend, sin pantalla de PR todavía): título y descripción a partir de rama, commits y diff contra el ancestro común; solo propone; mismos filtros, redacción y consentimiento que el commit
- [x] CLI falso en CI: entorno, argv, stdin, secreto de prueba · AITests
- [ ] Humo manual con CLI reales en macOS y Windows, con cuentas de organización: checklist escrito en [checklist-humo-ia.md](checklist-humo-ia.md); falta ejecutarlo
- [ ] Publicar `0.1.0-alpha.2`

## M8 — Pull requests (→ alpha.3)
> Estado del backend (`crates/ariadna-hosting` + `core/pr_ops.rs`): implementado y probado **solo contra un servidor simulado** (`wiremock`, JSON reconstruido de la documentación). **Nada está verificado con un servidor real**: ver `spike-pr.md` (todo marcado «NO VERIFICADO» y checklist para el spike con PAT reales). Excepción: el `fetch` de `refs/pull/N/head` (bajar la rama de una PR) se ha ejecutado contra GitHub real con `crates/ariadna-git/tests/pr_checkout.rs` (`#[ignore]`, PR #23 de `jamartif/ariadna-spike-pr`).
- [ ] Spike con PAT real: endpoints y permisos mínimos de Azure DevOps REST y GitHub REST para listar, crear, hilos, voto y completar (PR-02…PR-08); documentar versiones de API — *documentado desde la documentación pública en `spike-pr.md`; falta el humo con PAT reales (checklist al final del documento)*
- [x] Crate `ariadna-hosting`: `RemoteInfo`, `trait PullRequestProvider`, `AuthMethod`, cliente `reqwest` con `rustls` (proveedor `ring`), límites de seguridad de red (§4.4)
- [x] Obtención del secreto con `git credential fill` no interactivo, `zeroize`, sin registro ni eco (D22) — probado con un `git` falso; GCM real sin verificar
- [x] Azure DevOps: listar, detalle (revisores, políticas, work items), crear, abandonar/reabrir *(simulado)*
- [x] Azure DevOps: diff de la PR, hilos (ver, crear, responder, resolver), voto, completar con estrategia y *auto-complete* *(simulado)*
- [x] GitHub: listar, detalle, crear, reviews, comentarios en línea, merge/squash/rebase, cerrar/reabrir *(simulado; incluye GHE por host)*
- [x] Bajar la rama de una PR (incluido `refs/pull/N/head` de forks): `checkout_pull_request` → `GitServing::checkout_remote_ref` (fetch del ref del remoto de la PR, rama local `pr-<id>-<autor>-<rama>`, confirmación si la rama existe con otra base o hay cambios sin confirmar —se guardan en el stash—; `MissingRemoteRef` si el remoto no la tiene). Tests con bare local que publica `refs/pull/1/head` y `#[ignore]` contra GitHub real
- [x] Plantillas de PR del repo (`pull_request_template.md`) — `pull_request_template`
- [x] Push previo si la rama no está publicada: `create_pull_request` pregunta y hace `git push --set-upstream <remoto> <rama>` antes de crear; cancelar devuelve `CANCELLED`
- [x] Errores tipados `AUTH` (con el permiso que falta), `NOT_FOUND`, `POLICY`, `CONFLICT`, `RATE_LIMITED`, `NETWORK`
- [x] Comandos IPC de PR (ipc.md) y confirmaciones nativas para completar, abandonar y votar con rechazo; contrato, tipos TS y backend simulado de la UI
- [ ] UI: panel de PR (lista con filtros, detalle con Markdown sin `{@html}`, pestaña de archivos con comentarios en línea, votar, completar, crear)
- [ ] «Generar título y descripción con IA» (propuesta) enlazado a M6
- [x] Pruebas contra servidor simulado (`wiremock`) con 401/403/404/409/429
- [ ] Humo manual con cuentas reales (checklist de `spike-pr.md`)
- [ ] Publicar `0.1.0-alpha.3`

## M7 — Release
- [ ] Accesibilidad: ARIA, `forced-colors`, zoom 200 %, axe en E2E; revisión NVDA y VoiceOver
  - [x] axe-core (vitest-axe) en jsdom: una prueba por pantalla principal (PR, conflictos, ajustes, ramas/remotos, historial, paleta, espacio de trabajo) sin violaciones serias/críticas (`ui/src/lib/a11y.test.ts`; jsdom no calcula contraste)
  - [x] Primera pasada visual en navegador (1440×900 y 1000×700, claro/oscuro, `forced-colors` y `prefers-reduced-motion` emulados): ver `specs/rust/qa-alpha2.md`
  - [x] Contraste AA medido con `ui/scripts/contrast.mjs` (48 pares, sin dependencias); informe en `specs/rust/contraste.md`
  - [x] Zoom 200 % y reflujo (720×450 CSS: sidebar oculta, diff con altura útil y scroll, conflictos con paneles de 220 px y scroll propio)
  - [ ] axe en E2E contra la app real (`tauri-driver`)
  - [ ] Revisión manual con NVDA y VoiceOver (no realizada)
- [ ] Rendimiento contra NF-1…NF-6 con fixtures sintéticos — hecho el motor (NF-3, NF-4, NF-5 en `rendimiento.md`); faltan NF-1, NF-2, NF-6 y la interfaz (requieren ventana real)
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
