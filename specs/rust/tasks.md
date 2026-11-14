# Tareas — migración a Rust

`[ ]` pendiente · `[x]` hecho. Cada tarea cita el requisito del spec (G-xx, RF-IAx, NF-x, §) y, si aplica, el test Swift que define la paridad.

## M0 — Preparación
- [x] Spec v1 revisada con EGM (3 revisores de contexto limpio): `revision-egm.md`
- [x] Spike CLI de IA: **Copilot verificado** (`GitHub Copilot CLI 1.0.112`, 2026-11-14: flags, stdin sin `-p`, JSONL, herramientas denegadas; `docs/ia-copilot.md`); `claude` verificado (`claude 2.1.319`). **Codex sigue sin verificar** (CLI no instalado): ver la tarea abierta de M6
- [x] Spike `claude`: probar `--setting-sources ""`, salida JSON, detección de sesión sin leer credenciales — verificado: `--tools ""`, `--output-format json`, `claude auth status --json`
- [ ] Spike WebView2: bootstrapper en NSIS y Ctrl+Tab en WebView2
- [x] Leer y citar con fecha los términos de Anthropic, OpenAI y GitHub **para uso en empresa**: planes individuales vs organización, y apps de terceros que lanzan el CLI oficial (RF-IA10) — [`terminos-ia.md`](terminos-ia.md), consulta del 2026-11-13
- [x] Licencia MIT (`LICENSE`, D18)
- [x] Spike de perfiles: aislar el login por proveedor con directorio de configuración propio (`CLAUDE_CONFIG_DIR` / `CODEX_HOME`; en Copilot **verificado que no se puede aislar**, el token vive en el llavero) y detectar la cuenta activa (`claude auth status --json`)
- [ ] Solicitar SignPath Foundation para el proyecto (D17) y leer sus condiciones — la solicitud está escrita y lista para enviar (`docs/solicitud-signpath.md`)
- [x] Matriz de trazabilidad: G-01…G-15, RF-IA → tarea → test (`specs/rust/matriz-trazabilidad.md`)
- [ ] Inventario de los 58 + 8 tests Swift con su tarea Rust
- [x] Generar y versionar fixtures de git (con metadatos de versión)
- [x] Mockups de las 5 pantallas clave y tokens de diseño (`diseno-v2.md`, tokens en `ui/src`)
- [x] Ampliar `.gitignore` (`target/`, `node_modules/`) y `.gitleaks.toml`
- [ ] Javier decide las PROPUESTAS D7, D10, D11, D13

## M1 — Fundación

> Estado: implementado en `crates/ariadna-process` y `crates/ariadna-git` (203 tests en verde entre los tres crates de Rust, probados en Linux/WSL; la CI valida macOS y Windows). Pendiente: fixtures contra git real en CI y `core.longpaths`.
- [x] Cambiar a macOS 15 como mínimo en la configuración de Tauri y en la CI (D15): `minimumSystemVersion: "15.0"` y runners `macos-15`
- [x] `Cargo.toml` workspace, `rust-toolchain.toml` (≥ 1.77.2), rustfmt, clippy, cargo-deny
- [x] CI Rust: macos-15, windows-2025, windows-11-arm, ubuntu; job agregador; caché (`.github/workflows/rust.yml`)
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
- [x] Contrato IPC: comandos, enum de errores, eventos, `operation_id`, `cancel_operation` (§3.3) — `specs/rust/ipc.md`
- [x] Reintento acotado ante `index.lock` ajeno
- [x] Tests con 50 000 archivos (rutas por stdin, NF-3): `tests/perf.rs` (`#[ignore]`) y `tests/bulk_paths.rs`; informe en `rendimiento.md`

## M3 — Ramas, remotos, merge

> Estado: implementado y probado con remoto bare real, rebase y cherry-pick con conflicto. `DubiousOwnership` se detecta y se ofrece `safe.directory` con confirmación nativa (solo la carpeta concreta).
- [x] `RemoteInfo::parse` (GitHub, GHE, Azure DevOps en sus tres formas, GitLab, Bitbucket, otros) con ≥ 20 URLs de prueba y redacción de credenciales (D19) — `crates/ariadna-hosting/src/remote.rs` y sus tests
- [x] Error tipado `Auth` para fetch/pull/push con mensaje accionable (única excepción de parseo, aislada y con tests: `ariadna-git::auth` + `tests/auth_network.rs`, incluido el `pull` que sale con 1)
- [x] G-16 `clone_repository`: validación de URL (`ext::`, `-`), destino vacío, cancelación que borra solo lo creado, progreso, sin submódulos; tests con remoto bare local (`crates/ariadna-git/tests/clone.rs`)
- [x] Credenciales: `credential approve / reject / ls-remote` por stdin, zeroize del secreto, sin registro ni eco en errores; tests con un credential helper falso (`ariadna-git::credentials` + `tests/credentials.rs`, `Secret` sin `Clone`/`Serialize`)
- [x] Ayuda de PAT por proveedor (Azure DevOps, GitHub, GitLab, Bitbucket) con enlaces https y permisos mínimos (Ajustes › Cuentas, `settings.accounts.*`)
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

> Segunda oleada hecha: rediseño visual con barra de título propia, botón de IA para el commit (`ai_generate_commit_message`, consentimiento Personal/Empresa), clonado con PAT (askpass temporal, nunca guardado), historial virtualizado con grafo y ramas (crear, cambiar, renombrar, borrar, fusionar sin conflictos). Deuda: test de cancelar clonado ignorado con git 2.53. (Panel de remotos, menú nativo, hibernación y `opener` hechos en la tercera oleada; el clonado con sesión de `gh`/`glab` y el riel de ramas, después.)

> Estado: backend Tauri (`AppCore`, pestañas, comandos IPC, `session.json`, fixtures) e interfaz Svelte (diseño claro/oscuro, pestañas, bienvenida, cambios, diff por hunk, commit, paleta) **hechos y validados en CI** (Windows x64/ARM64, macOS 15, Ubuntu); el instalador NSIS y el `.dmg` se construyen. Hecho también: pantalla de recuperación, clonado (D20) en la bienvenida, hibernación de pestañas, `opener` y menú nativo.
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
- [x] Bienvenida: ruta, Examinar, recientes, favoritos, **Clonar desde URL** y formulario de credenciales (PAT, sesión de `gh`/`glab` u OAuth por código de dispositivo) (D20) — `ipc.md` «Clonar y credenciales»
- [x] Plugin `opener` con capability mínima para «Abrir en el navegador» y «Crear pull request» (D19): la ventana no tiene ningún permiso `opener:*`; `open_remote_url` (Rust) construye la URL desde la config real del remoto y valida https + host del remoto, `open_external_url` mantiene su lista cerrada (`ipc.md` «Remotos y “Abrir en el navegador”»)
- [x] Paleta de comandos y registro único de atajos (ES/US/DE)
- [x] Errores: toast, banner de pestaña, pantalla de recuperación

## M5 — Pantallas (→ alpha.1)

> Estado: Cambios, diff por hunk, barra de commit, historial por cursor, ramas con riel y menú contextual, conflictos (con IA), stashes, etiquetas, ajustes y PR **hechos**; regresión visual con baselines de Linux en CI.
- [x] Cambios: preparados / sin preparar / sin seguimiento / ignorados / conflictos
- [x] Diff por hunk (CodeMirror 6), virtualizado, límites de NF-5
- [x] Barra de commit (Ctrl/⌘+Intro), amend
- [x] Historial paginado con filtro local y búsqueda global (NF-4)
- [x] Ramas y remotos, con icono del proveedor y acciones «Abrir en el navegador» / «Crear pull request» (D19): sección «Remotos» en el panel Ramas (lista con proveedor, traer/bajar/subir la rama actual, añadir/renombrar/quitar con confirmación nativa) y botones en el detalle de rama. `list_remotes`, `add_remote`, `rename_remote`, `remove_remote`, `open_remote_url`. Solo enlaces: la API de PR es M8
- [x] Historial por cursor (NF-4): `log_page` acepta `cursor` opcional (la *frontera* del historial cargado, no solo el último hash: con merges hace falta toda la cola de `git log`) y continúa con `git log <frontera>` sin `--skip`; coste constante en repos grandes (medidas en `rendimiento.md`). Probado contra `--skip` con historiales generados con merges, octopus y fechas repetidas; el contrato antiguo (`skip`) sigue vigente. `ariadna_git::history_cursor`, `GitServing::log_after`, `Frontier` en la UI
- [x] Etiquetas (tags): `list_tags`, `create_tag` (HEAD o un commit; con mensaje = anotada), `delete_tag` y `delete_remote_tag` (confirmación nativa), `push_tag`; validación de `git check-ref-format` y errores tipados `INVALID_TAG_NAME` / `TAG_EXISTS` / `TAG_NOT_FOUND`. UI: sección «Etiquetas» en Ramas, «Crear etiqueta aquí» en el detalle del commit y chips en la lista del historial. Sin estado «publicada» (exigiría red)
- [x] Estudio de conflicto a tres paneles + resultado (panel «Conflictos», `merge_start`/`merge_state`/`resolve_conflict`/`merge_continue`/`merge_abort`; textarea con resultado editable, sin CodeMirror: no está en el proyecto)
- [x] Ajustes: pantalla (General, Git, IA, Cuentas, Acerca de), `settings.json` versionado y atómico, pull con rebase, ruta de git con «Detectar», reglas repo → perfil (`ipc.md` «Ajustes»)
- [x] Vitest de componentes
- [x] Rediseño con **riel de ramas persistente** (izquierda), vista central segmentada (Cambios · Historial · Stashes · Pull requests · Conflictos) y panel derecho de detalle/diff: la gestión de ramas deja de ser una pestaña del centro (`BranchRail.svelte`)
- [x] **Menú contextual de cada rama del riel**: clic derecho o tecla Menú / `Mayús+F10`, `role="menu"` con navegación por teclado, `Esc` devuelve el foco a la fila, reposicionamiento dentro de la ventana, acciones no aplicables deshabilitadas con motivo y diff `A...B` en el panel derecho. Pruebas: `BranchMenu.test.ts`, `branchesMenu.test.ts`, `src-tauri/src/core/tests/branch_menu.rs`, `specs/rust/contextmenu/` y E2E `10-menu-contextual-rama` (`ipc.md` «Menú contextual del riel de ramas» y [`docs/menu-ramas.md`](../../docs/menu-ramas.md))
- [x] **Interfaz bilingüe español/inglés** con el idioma aplicado en caliente: catálogos `en*.ts` con paridad exacta de claves, plurales y parámetros; ajuste `language` (`system` | `es` | `en`) en `settings.json` y selector en Ajustes › General; fechas y números en el idioma activo; pruebas de paridad, de detección y persistencia y axe en inglés
- [x] Líneas sueltas dentro de un hunk (`stage_lines` / `unstage_lines`): casillas por línea, clic, Mayús+clic, teclado (↑/↓, Espacio, Mayús+↑/↓, Esc, `Mod+Shift+Intro`); parche parcial (`ariadna-git::partial`) con `git apply --cached --recount` por stdin; CRLF, sin salto final, archivos nuevos, rutas con espacios/Unicode; rechazo claro (`PATCH_REJECTED`)
- [x] Stash: `stash_list/push/apply/pop/drop/diff` (identidad por SHA, `drop` con confirmación nativa, conflictos al aplicar/sacar → panel de Conflictos); panel «Stashes», sección en la barra lateral, botón «Guardar cambios en stash» en Cambios, atajos y paleta; mock en el backend simulado; i18n
- [x] Publicar `0.1.0-alpha.1` (macOS y Windows x64)

## M6 — IA (→ alpha.2)

> Estado: la capa `crates/ariadna-ai` está implementada y probada con un CLI falso y contra los CLI reales que están instalados. **Verificados**: Claude (`claude 2.1.319`) y **Copilot** (`GitHub Copilot CLI 1.0.112`, 2026-11-14: flags, prompt por stdin sin `-p`, JSONL real como fixture, herramientas denegadas, detección del binario; [`docs/ia-copilot.md`](../../docs/ia-copilot.md)). Copilot **ya no es experimental** (`ProviderId::is_experimental` = `false`). **Pendiente de verificar: Codex** (CLI no instalado en la máquina de desarrollo: `codex exec --json --sandbox read-only` y `CODEX_HOME` siguen como constantes `SPIKE`).
- [x] Trait `AiProvider`, `AiSuggestion` con `revision` y estado obsoleto
- [x] Consentimiento por proveedor, vista previa, exclusión de sensibles, redactor de secretos, marcado de contenido no fiable (§5.1)
- [x] `ClaudeCli` (RF-IA1/3/5/6/7)
- [x] `CodexCli` · `CopilotCli`: Copilot **verificado** (1.0.112); Codex implementado sobre flags documentados pero **no verificado**
- [x] Resolución de binarios en Windows y shims (RF-IA2); test de lista blanca de argumentos
- [x] Entorno por lista blanca y variables prohibidas (RF-IA3)
- [x] Detección de disponibilidad, versión y sesión ejecutando el CLI (RF-IA4)
- [x] Cola de IA: 2 simultáneas, límite por minuto, cancelación al cerrar pestaña (RF-IA8)
- [x] Modo Auto con reglas de RF-IA9
- [ ] Perfiles Personal/Empresa (D16): modelo, reglas repo → perfil por remoto, consentimiento por perfil, aislamiento de sesión y perfil visible en la hoja de propuesta **hechos**; falta la **confirmación al cambiar el perfil de un repo** y el perfil visible en la pestaña, y la decisión D16
- [x] Directorio de configuración aislado por perfil y proveedor (`ai-profiles/<perfil>/<proveedor>` con `CLAUDE_CONFIG_DIR` / `CODEX_HOME`; Empresa aísla por defecto, Personal usa la sesión normal y puede aislar) y botón «Iniciar sesión» (`ai_login`). **Verificado con Claude** (`CLAUDE_CONFIG_DIR` vacío → `loggedIn: false`); Codex según documentación; en **Copilot verificado que no se puede aislar** (`COPILOT_HOME` no mueve el token del llavero): comparte login y la UI lo avisa
- [x] Consentimiento por perfil y proveedor; Claude desactivado por defecto en Empresa
- [x] Mostrar cuenta activa por proveedor cuando el CLI la exponga (RF-IA12): `ai_accounts` con `claude auth status --json` (verificado); Copilot **no tiene** subcomando de cuenta y Codex no está verificado: «no disponible» sin ejecutar nada ni leer credenciales
- [x] Acciones: mensaje de commit, resumen, explicar commit, propuesta de conflicto y título/descripción de PR
- [x] Rechazo de propuestas de conflicto que salen de los hunks
- [x] **Resolver conflictos con IA**: `ai_resolve_conflict(tabId, path)` propone el archivo completo desde base/nuestra/suya con los mismos filtros y garantías, y valida que no queden marcadores; botón ✨ «Resolver con IA» (`Alt+R`) en el editor de conflictos con panel de revisión (justificación, diferencias y «Usar» sin escribir nada), «Regenerar» y «Descartar»
- [x] Hoja de propuesta reutilizable (`ProposalSheet`): copiar, usar, regenerar, descartar; integrada en `CommitBar` («Ver propuesta») sin cambiar el flujo de relleno + «Deshacer»
- [x] `ai_generate_pull_request` (backend, sin botón en el panel de PR todavía): título y descripción a partir de rama, commits y diff contra el ancestro común; solo propone; mismos filtros, redacción y consentimiento que el commit
- [x] CLI falso en CI: entorno, argv, stdin, secreto de prueba · AITests
- [ ] **Codex**: humo con el CLI real (instalarlo, `codex --version`, flags de `exec`, stdin con `-`, `codex login` y si `CODEX_HOME` aísla la sesión). Mientras no se haga, Codex se publica como experimental
- [ ] Humo manual con CLI reales en macOS y con cuentas de organización: [checklist-humo-ia.md](checklist-humo-ia.md) — hecho para Copilot en Windows (flags, stdin, herramientas); faltan macOS, la organización y el flujo OAuth del navegador de `copilot login` (`ai_login.verified` sigue en `false`)
- [x] Publicar `0.1.0-alpha.2`

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
- [x] UI: panel de PR (lista con filtros, detalle con Markdown sin `{@html}`, pestaña de archivos con comentarios en línea, votar, completar, crear) — `PullRequestsPanel.svelte`, `PrDetail.svelte`, `PrFiles.svelte`, `PrComposer.svelte`
- [ ] «Generar título y descripción con IA» (propuesta) en el panel de PR: el comando y el método del store (`generate`) existen y están probados; falta el botón con el panel de revisión en `PrComposer`
- [x] Pruebas contra servidor simulado (`wiremock`) con 401/403/404/409/429
- [ ] Humo manual con cuentas reales (checklist de `spike-pr.md`)
- [x] Publicar `0.1.0-alpha.3`

## M7 — Release
- [ ] Accesibilidad: ARIA, `forced-colors`, zoom 200 %, axe en E2E; revisión NVDA y VoiceOver
  - [x] axe-core (vitest-axe) en jsdom: una prueba por pantalla principal (PR, conflictos, ajustes, ramas/remotos, historial, paleta, espacio de trabajo) sin violaciones serias/críticas (`ui/src/lib/a11y.test.ts`; jsdom no calcula contraste)
  - [x] Primera pasada visual en navegador (1440×900 y 1000×700, claro/oscuro, `forced-colors` y `prefers-reduced-motion` emulados): ver `specs/rust/qa-alpha2.md`
  - [x] Contraste AA medido con `ui/scripts/contrast.mjs` (48 pares, sin dependencias); informe en `specs/rust/contraste.md`
  - [x] Zoom 200 % y reflujo (720×450 CSS: sidebar oculta, diff con altura útil y scroll, conflictos con paneles de 220 px y scroll propio)
  - [ ] axe en E2E contra la app real (`tauri-driver`)
  - [ ] Revisión manual con NVDA y VoiceOver (no realizada)
- [ ] Rendimiento contra NF-1…NF-6 con fixtures sintéticos — hecho el motor (NF-3, NF-4, NF-5 en `rendimiento.md`); faltan NF-1, NF-2, NF-6 y la interfaz (requieren ventana real)
- [x] E2E con `tauri-driver` en Linux (WSL/CI): 10 flujos sobre la app empaquetada, incluidos el riel de ramas y su menú contextual (`e2e/README.md`, job `e2e`)
- [ ] E2E en macOS y Windows x64 con `tauri-driver` (solo se han ejecutado en Linux)
- [x] Regresión visual con baselines por SO: 16 capturas por sistema operativo con `pixelmatch`; **baselines versionados de Linux** (`e2e/baselines/linux`)
- [ ] Baselines de `darwin` y `win32` (la suite los compara y todavía no existen: hay que generarlos en cada sistema)
- [x] Capturas automáticas en CI: el job `e2e` sube `e2e-artifacts` (capturas de cada prueba, también de las que fallan) y puede regenerar los baselines de Linux (`update_baselines`)
- [x] Respetar proxy y CA del sistema (mejor esfuerzo, sin pruebas dedicadas): `crates/ariadna-hosting/src/net.rs`, probado
- [x] `.dmg` universal con firma ad-hoc y guía de «Abrir igualmente» (Developer ID y notarización solo si hay Apple Developer Program); NSIS por usuario (x64, ARM64 mejor esfuerzo) con comprobación de WebView2
- [ ] Firma de binarios e instalador según la opción elegida en D17 (requisito de `v0.1.0`; alfas sin firmar): política escrita y solicitud a SignPath lista para enviar (`docs/firma-de-codigo.md`, `docs/solicitud-signpath.md`)
- [ ] Prueba de instalación y desinstalación limpias en VM
- [x] Documentar apertura de las alfas sin firmar (Gatekeeper / SmartScreen): [`docs/instalacion-sin-firma.md`](../../docs/instalacion-sin-firma.md) y [`docs/windows-sin-instalar.md`](../../docs/windows-sin-instalar.md)
- [x] CHANGELOG y README nuevo (README en inglés, con tabla de verificación)
- [x] Tag `swift-final`, rama `legacy-swift`, retirar Swift de `main`
- [ ] Publicar `v0.1.0`

## Qué falta para v0.1.0

Lo que queda de verdad, en orden de riesgo:

1. **Azure DevOps con una cuenta real**: hoy sus PR están probados solo contra `wiremock` (GitHub sí se verificó con la API real). Checklist en `spike-pr.md`.
2. **Firma SignPath**: enviar la solicitud, firmar NSIS e instalador en CI desde un commit validado y dejar el `.dmg` en ad-hoc. Sin firma, la instalación en Windows con Smart App Control es imposible.
3. **NVDA y VoiceOver**: revisión manual de las pantallas principales (axe ya pasa en jsdom y falta en E2E); además `axe` contra la app real con `tauri-driver`.
4. **Baselines de `darwin` y `win32`** y una pasada de E2E en macOS y Windows x64.
5. **Decisión D16** (perfiles Personal/Empresa): confirmar el alcance de la confirmación al cambiar el perfil de un repo y el perfil en la pestaña.
6. Codex: instalarlo y verificar flags, stdin, `codex login` y si `CODEX_HOME` aísla la sesión (si no, marcarlo como comparte login).
7. Flujo OAuth del navegador de `copilot login` para poder poner `ai_login.verified = true` también en Copilot.

## Después de v0.1 (no entra en esta entrega)
- [ ] `policy.json` administrable: desactivar IA, proveedores permitidos, tope de contexto (RF-IA11)
- [ ] Instalador MSI para despliegue gestionado
- [ ] Mica y vibrancy (D11)
- [ ] Identidad de commit por perfil
- [ ] Actualizador automático
