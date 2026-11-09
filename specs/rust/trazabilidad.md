# Trazabilidad Swift → Rust

Matriz de trazabilidad e inventario de tests de la migración (tarea M0 «Matriz de trazabilidad» e «Inventario de los 36 + 8 tests»). Solo documentación.

- Fuentes leídas: `specs/rust/spec.md` (v1.1: §1.1, §4, §5, §9), `specs/rust/tasks.md`, `specs/spec.md` y `specs/tasks.md` (Swift), `Sources/**` y `Tests/**` completos.
- Rama `rust-migration`, commit base `1501b28`. Los tests se han contado con `grep -c "func test"` y se han leído todos.
- **Nombres de módulo Rust: propuestos.** El spec solo fija los crates `ariadna-git`, `ariadna-ai`, `ariadna-process` (existen), `src-tauri` y `ui`. Los submódulos (`parse::porcelain`, `engine`, `queue`…) son una sugerencia para poder trazar.
- **IDs de tarea:** `tasks.md` no tiene IDs. Aquí `M2.6` = sexta tarea de la sección M2 por orden de aparición (anexo A). Recomendación: copiar esos IDs a `tasks.md`.

## 0. Resumen

| Concepto | Valor |
|---|---|
| Tests Swift reales en `GitCoreTests` | **58** (no 36) |
| Tests Swift reales en `AriadnaAITests` | **8** |
| Total real | **66** (no 44) |
| Tests Swift con requisito G-xx / RF-IA / NF directo | 57 |
| Tests Swift sin ID de requisito (infraestructura §3.2/§4.1/§4.2/§6, tooling o identidad) | 9 |
| Tests que no se pueden portar literalmente | 18: 17 ADAPTAR + 1 NO PORTAR |
| Requisitos G-xx con algún test Swift | 15 de 15 (con casos internos sin test, ver §2) |
| Requisitos RF-IA sin ningún test Swift | RF-IA2, RF-IA3, RF-IA6, RF-IA8, RF-IA10, RF-IA11, RF-IA12 |
| Requisitos NF sin test heredado | NF-1…NF-6, NF-8 (NF-7 solo parcial) |

**Consecuencia para el plan:** el criterio de salida de M2 y M3 («nº de tests portados = nº de tests Swift del área», spec §11, plan.md) se calculaba sobre 36. Con 58 hay que reescribirlo por área (tabla §1.5). El «36» aparece también en `specs/tasks.md` del Swift (líneas «Comprobación de esta entrega»): ese dato del Swift está desfasado.

Marcas en la descripción de cada test: **ADAPTAR** = la intención se porta, el test no puede copiarse tal cual (motivo indicado); **NO PORTAR** = sin equivalente Rust razonable.

## 1. Inventario de los tests Swift

### 1.1 `Tests/GitCoreTests/GitCLIEngineTests.swift` — 28 tests

| Archivo | Test | Qué comprueba | Req. | Módulo Rust destino | Tarea |
|---|---|---|---|---|---|
| GitCLIEngineTests | testOpenStatusStageCommitAndLog | Abrir desde la ruta de un archivo sube a la raíz; repo sin commits (`isUnborn`); stage de un archivo; commit devuelve 40 hex; log con asunto y cuerpo; carpeta sin repo → `notARepository` | G-01, G-02, G-03, G-06, G-08 | `ariadna-git::engine` | M2.3, M2.4, M2.5, M2.7, M2.9 |
| GitCLIEngineTests | testEmptyCommitAndNothingStaged | Mensaje vacío rechazado; segundo commit sin nada preparado → `nothingToCommit`. **ADAPTAR:** Swift detecta «nothing to commit» en texto; Rust no parsea mensajes (§4.1) | G-06 | `ariadna-git::engine` | M2.7 |
| GitCLIEngineTests | testStageSingleHunkAndUnstageIt | Archivo con 2 hunks: preparar solo el primero; preparado y resto sin mezclarse; quitar el hunk deja el índice vacío | G-04 | `ariadna-git::engine` (hunk) + `parse::diff` | M2.6 |
| GitCLIEngineTests | testBranchesCheckoutRenameAndDelete | Crear, cambiar, renombrar rama; checkout a SHA → detached; volver; borrar; no se puede borrar la rama actual | G-10 | `ariadna-git::engine` | M3.1 |
| GitCLIEngineTests | testFastForwardNoFastForwardAndUpToDate | Merge `allow` → fast-forward; `never` → merge commit de 2 padres; repetir → ya al día. **ADAPTAR:** Swift clasifica leyendo «Fast-forward»/«Already up to date» | G-12 | `ariadna-git::engine` | M3.3 |
| GitCLIEngineTests | testConflictModelResolutionAndContinue | Merge con conflicto: estado `merging`, modelo ours/base/theirs, 1 hunk, tomar lado; escribir sin confirmar → `confirmationRequired` y archivo intacto; con confirmación + marcar resuelto + continuar → commit de 2 padres | G-13, G-14 | `ariadna-git::engine` + `parse::conflict` | M3.4, M3.5 |
| GitCLIEngineTests | testAbortMergeRestoresCleanTree | Abortar merge deja árbol limpio y estado `none`; abortar de nuevo → `noIntegrationInProgress` | G-14 | `ariadna-git::engine` | M3.5 |
| GitCLIEngineTests | testPathEscapeDoesNotWrite | `../archivo` con `confirmed: true` → `pathEscapesRepository` y no se crea nada fuera. Único test Swift de `PathSafety` | G-13, §4.2 | `ariadna-git::path_safety` | M1.6, M3.4 |
| GitCLIEngineTests | testRestoreRequiresConfirmation | Descartar sin confirmar → `confirmationRequired`, archivo intacto; confirmado → restaura. **ADAPTAR:** en Rust la confirmación es un token de un solo uso emitido por el diálogo nativo (§6), no un `bool` | G-05 | `ariadna-git::engine` + `src-tauri` (confirmaciones) | M3.6, M4.5 |
| GitCLIEngineTests | testIgnoredFilesAndSessionGate | `status` con y sin ignorados; la sesión también exige confirmación para escribir una resolución | G-02, G-13, §6 | `ariadna-git::engine`, `ariadna-git::session` | M2.4, M4.4, M4.5 |
| GitCLIEngineTests | testPushFetchAndPullAgainstBareRemote | `addRemote`, push con `--set-upstream` a un bare, `listRemotes`, otro clon empuja, pull fast-forward. **No llama a `fetch`** pese al nombre | G-11 | `ariadna-git::engine` | M3.2 |
| GitCLIEngineTests | testDetachedHeadWarnsBeforeCommit | HEAD detached: `detachedCommitWarning`; commit → `detachedHeadCommit`; con `allowDetached` crea el commit | G-06 | `ariadna-git::engine` | M2.7 |
| GitCLIEngineTests | testStageIsolatedAndContiguousHunks | Hunks aislados (hueco 18) y contiguos (hueco 8): preparar el primero y quitarlo | G-04 | `ariadna-git::engine` (hunk) | M2.6 |
| GitCLIEngineTests | testStageHunkWithCRLFAndMissingFinalNewline | Hunk en archivo CRLF y en archivo sin salto final | G-04 | `ariadna-git::engine` (hunk) | M2.6 |
| GitCLIEngineTests | testStageHunkInUnicodePathWithSpaces | Hunk en `notas/cañón archivo.txt` | G-04 | `ariadna-git::engine` (hunk), `path_safety` | M2.6, M1.6 |
| GitCLIEngineTests | testPullRebaseReplaysLocalCommit | `pull --rebase` con commit local y remoto distinto: queda el local encima del remoto | G-11 | `ariadna-git::engine` | M3.2 |
| GitCLIEngineTests | testOpeningRepoDoesNotWriteHooksPath | Abrir un repo no escribe `core.hooksPath` ni ejecuta hooks | G-01, §6 | `ariadna-git::engine` | M2.3, M2.1 |
| GitCLIEngineTests | testCheckoutRemoteTrackingCreatesLocalBranch | `checkout origin/feature` crea la local con upstream y la deja activa | G-10 | `ariadna-git::engine` | M3.1 |
| GitCLIEngineTests | testCheckoutDirtyTreeExplainsInSpanish | Checkout con cambios locales en conflicto → `localChangesBlockCheckout` con texto en español; árbol intacto. **ADAPTAR:** Swift detecta «would be overwritten» en stderr; Rust no parsea texto (§4.1) y el enum de errores de §3.3 no tiene esta variante | G-10 | `ariadna-git::engine`, `ariadna-git::error` | M3.1, M2.10 |
| GitCLIEngineTests | testStageHunkOfUntrackedFile | Hunk de un archivo nuevo (sin seguimiento) pasa a preparado | G-04 | `ariadna-git::engine` (hunk) | M2.6 |
| GitCLIEngineTests | testFailedUntrackedHunkDoesNotLeaveIntentToAdd | Hunk inexistente → `hunkNotFound` y el archivo sigue sin seguimiento (no queda `intent-to-add`) | G-04 | `ariadna-git::engine` (hunk) | M2.6 |
| GitCLIEngineTests | testCheckoutRemoteDoesNotSwitchDivergedLocal | `checkout origin/feature` con local divergente → `divergedLocalBranch` sin cambiar; con `allowDivergedLocal` sí | G-10 | `ariadna-git::engine` | M3.1 |
| GitCLIEngineTests | testPullRebaseConflictAbortAndContinue | `pull --rebase` con conflicto → `conflicts`, estado `rebasing`; abortar; repetir, escribir resolución, marcar resuelto, continuar | G-11, G-14, G-13 | `ariadna-git::engine` | M3.7, M3.5 |
| GitCLIEngineTests | testAmendRewritesUnpublishedCommitOnly | Amend: sin commits → `cannotAmend`; mensaje vacío rechazado; amend con lo preparado funciona; tras push a `origin/main` se niega (la razón cita `origin/main`) | G-07 | `ariadna-git::engine` | M2.8 |
| GitCLIEngineTests | testAmendRefusesDetachedHead | Amend con HEAD detached se niega (razón menciona «rama»); log intacto | G-07 | `ariadna-git::engine` | M2.8 |
| GitCLIEngineTests | testAmendRefusesDuringMerge | Amend con merge en curso se niega (razón menciona «merge»); sigue `merging` | G-07 | `ariadna-git::engine` | M2.8 |
| GitCLIEngineTests | testSearchByHashIgnoresCommitsOnOtherBranches | Buscar por prefijo de hash solo encuentra commits del historial de HEAD | G-09 | `ariadna-git::engine` (`search_log`) | M2.9 |
| GitCLIEngineTests | testSearchLogMatchesMessageAuthorAndHash | Búsqueda por mensaje, cuerpo, autor y hash; sin distinguir mayúsculas; texto literal (no regex); varias palabras; orden nuevo→viejo; consulta vacía = log | G-09 | `ariadna-git::engine` (`search_log`) | M2.9 |

### 1.2 `Tests/GitCoreTests/OperationQueueTests.swift` — 8 tests

| Archivo | Test | Qué comprueba | Req. | Módulo Rust destino | Tarea |
|---|---|---|---|---|---|
| OperationQueueTests | testMutationCommandsAreClassified | Clasificación mutación/lectura de subcomandos (`status`, `diff`, `log`, `remote -v` = lectura; `add`, `commit`, `remote add`, `pull`, `push` = mutación) | §3.2 (sin G-xx) | `ariadna-git::queue` | M2.2 |
| OperationQueueTests | testMutationsAreSerialAndReadsOverlap | Dos lecturas se solapan (máx. 2); dos mutaciones nunca (máx. 1). **ADAPTAR:** en Rust la cola es por clave de repo y las mutaciones tienen prioridad | §3.2 (sin G-xx) | `ariadna-git::queue` | M2.2 |
| OperationQueueTests | testGitRunsWithEnglishMessages | Git corre con `LC_ALL`/`LANG` = locale inglés y `LANGUAGE=en`; conserva `HOME`. **ADAPTAR:** el spec fija `LC_ALL=C` «como apoyo» y prohíbe parsear mensajes | §4.1 (sin G-xx) | `ariadna-process::env`, `ariadna-git::engine` | M2.1, M1.4 |
| OperationQueueTests | testHashDetection | `looksLikeHash` acepta 7–40 hex y rechaza `abc`, `marea` (decide si la búsqueda es por hash) | G-09 | `ariadna-git::engine` (`search_log`) | M2.9 |
| OperationQueueTests | testCancelledRefreshDoesNotPublishStatus | Refresco cancelado no publica estado a medias y falla con cancelación | §3.2 (refrescos), NF-7 | `ariadna-git::session` | M2.2, M4.4 |
| OperationQueueTests | testLoginShellPathIsCaptured | Obtiene el `PATH` del login shell (`$SHELL -l`) y lo usa en el entorno. **ADAPTAR/DECIDIR:** el spec Rust no usa login shell: «PATH ampliado» (§4.1). Solo macOS | §4.1 (sin G-xx) | `ariadna-git::locate` / `ariadna-process::env` | M1.5 |
| OperationQueueTests | testInstallScriptConfiguresHooksPath | El script `scripts/install-git-hooks.sh` contiene el `git config core.hooksPath .githooks`. **NO PORTAR:** es tooling del repo, no del producto | (tooling, §6 gitleaks pre-commit) | — (comprobación en CI si se mantiene el script) | M0.13 |
| OperationQueueTests | testBundleIdentifier | Constantes `es.javiermartinez.ariadna`, `Ariadna`, `15.0`. **ADAPTAR:** comprobar `tauri.conf.json` (`identifier`, `minimumSystemVersion`); el spec Rust no fija el identificador | D15 | `src-tauri` (`tauri.conf.json`) | M1.1, M4.1 |

### 1.3 `Tests/GitCoreTests/ParserTests.swift` — 18 tests

| Archivo | Test | Qué comprueba | Req. | Módulo Rust destino | Tarea |
|---|---|---|---|---|---|
| ParserTests | testPorcelainOrdinaryUntrackedAndBranchHeaders | Cabeceras `branch.oid/head/upstream/ab`, entrada ordinaria y sin seguimiento con espacios | G-02 | `ariadna-git::parse::porcelain` | M1.7 |
| ParserTests | testPorcelainRenameWithSpacesAndUnicode | Entrada de renombrado `2 R.` con ruta Unicode y espacios y ruta original tras NUL | G-02 | `ariadna-git::parse::porcelain` | M1.7 |
| ParserTests | testPorcelainRenameAndConflict | Renombrado + modificado (`RM`) y entrada de conflicto `u UU` (`bothModified`, no preparada) | G-02 | `ariadna-git::parse::porcelain` | M1.7 |
| ParserTests | testPorcelainDetachedAndUnborn | `(detached)` y `(initial)` (repo sin commits) | G-02 | `ariadna-git::parse::porcelain` | M1.7 |
| ParserTests | testCommitLogParserKeepsMultilineBody | Log con separadores US/RS: cuerpo multilínea, padres, refs, SHA corto | G-08 | `ariadna-git::parse::log` | **sin tarea en M1**; M2.9 |
| ParserTests | testCommitLogParserTrimsSeparatorNewlineFromHashes | Un `\n` delante del hash a partir del 2.º commit no contamina el SHA | G-08 | `ariadna-git::parse::log` | **sin tarea en M1**; M2.9 |
| ParserTests | testAmendDraftPrefillsAndClearsUntouchedMessage | `AmendDraft`: precarga asunto+cuerpo; al desactivar borra el mensaje no tocado, respeta el editado o el escrito por el usuario | G-07 (**regla de UI no escrita en el spec**) | `ariadna-git::amend` (lógica en Rust, §3 «la UI no tiene lógica») | M5.3 |
| ParserTests | testAmendDraftWarnsOnMergeCommit | Aviso si el commit a corregir es un merge (≥2 padres) | G-07 (idem) | `ariadna-git::amend` | M5.3 |
| ParserTests | testBranchTrackParser | `[ahead 2, behind 3]` y lista de ramas (HEAD, upstream, ahead) | G-10 | `ariadna-git::parse::branch` | **sin tarea en M1**; M3.1 |
| ParserTests | testRemoteParser | `remote -v`: nombre, URL fetch/push, orden alfabético | G-11 | `ariadna-git::parse::remote` | **sin tarea en M1**; M3.2 |
| ParserTests | testDiffParserKeepsCRLFHunksSeparate | Dos hunks con `\r\n`; la línea añadida conserva `\r` | G-04, G-08 | `ariadna-git::parse::diff` | M1.8 |
| ParserTests | testDiffParserBinaryPathWithSpacesDoesNotSwallowTheOtherSide | `Binary files … differ` con espacios; ruta correcta, `isBinary`; renombrado con espacios | G-08, NF-5 (binarios) | `ariadna-git::parse::diff` | M1.8 |
| ParserTests | testDiffParserPathContainingBSlashWithoutPlusHeaders | Ruta con ` b/` dentro, solo cambio de modo/renombrado y sin cabeceras `---/+++` | G-08 | `ariadna-git::parse::diff` | M1.8 |
| ParserTests | testGitCStyleAndOctalPathEscapes | Desescapado de rutas estilo C y octales (`\303\251`, `\t`, `\\`) en cabeceras | G-08, §4.1 (codificación) | `ariadna-git::parse::diff` | M1.8 |
| ParserTests | testDiffParserTrimsTabAfterSpacedPath | Tabulador que git añade tras rutas con espacios | G-08 | `ariadna-git::parse::diff` | M1.8 |
| ParserTests | testDiffParserTwoHunks | Dos hunks (inicios, líneas añadidas, `unifiedText`); `patch(selecting:)` construye parche solo del hunk elegido. **ADAPTAR:** el spec manda construir parches desde bytes de `git diff` | G-04, G-08 | `ariadna-git::parse::diff` | M1.8, M2.6 |
| ParserTests | testConflictMarkersResolveOneHunk | Marcadores diff3: ours/base/theirs de 1 hunk; resolver con `ours`; sin elecciones deja marcadores | G-13 | `ariadna-git::parse::conflict` | M1.9 |
| ParserTests | testUnmergedIndexParser | `ls-files -u -z`: etapas 1/2/3 de un archivo | G-13 | `ariadna-git::parse::conflict` | **sin tarea en M1**; M3.4 |

### 1.4 `Tests/GitCoreTests/ProcessAndStoreTests.swift` — 4 tests

| Archivo | Test | Qué comprueba | Req. | Módulo Rust destino | Tarea |
|---|---|---|---|---|---|
| ProcessAndStoreTests | testSanitizedEnvironmentDropsTokenLikeVariables | Entorno de git: conserva `PATH`, `HOME`, `GIT_AUTHOR_NAME`, `SSH_AUTH_SOCK`; elimina `GIT_TOKEN`, `OPENAI_API_KEY`, `AWS_SECRET_ACCESS_KEY`, `RANDOM_THING`. **ADAPTAR (conflicto):** `EnvPolicy::git_default` hereda todo y exime `GIT_*`, así que `GIT_TOKEN` y `RANDOM_THING` pasarían (ver §5 D3) | §6 (entorno de git) | `ariadna-process::env` | M1.4 |
| ProcessAndStoreTests | testProcessRunnerEchoAndStdin | Captura stdout y código de salida; stdin llega al hijo. **ADAPTAR:** usa `/bin/echo`, `/bin/cat`; Rust ya tiene `tests/runner.rs` con un binario falso (`captura_stdout_y_codigo_de_salida`, `stdin_grande_sin_bloqueo`) | §9 procesos, RF-IA6 (stdin) | `ariadna-process::runner` | M1.4 |
| ProcessAndStoreTests | testProcessRunnerTimeoutAndMissingBinary | Timeout → `timedOut`; binario inexistente → `executableNotFound`. **ADAPTAR:** usa `/bin/sleep`; Rust: `timeout_termina_el_proceso`, `programa_inexistente_da_error_de_spawn` | NF-7, §9 | `ariadna-process::runner` | M1.4 |
| ProcessAndStoreTests | testRecentRepositoryRoundTrip | Guardar reciente, marcar favorito, volver a recordar con otra rama/fecha: 1 entrada, favorito y `lastBranch` conservados, fecha actualizada | G-15 | `ariadna-git::store::recents` | M1.10 |

### 1.5 `Tests/AriadnaAITests/AITests.swift` — 8 tests

| Archivo | Test | Qué comprueba | Req. | Módulo Rust destino | Tarea |
|---|---|---|---|---|---|
| AITests | testCopilotCommitMessageDoesNotInvokeGitOrWrite | Sugerencia de mensaje `pending`, asunto/cuerpo parseados, sin contenido propuesto; `commandSummary` sin volcar el diff; ejecutable `copilot`, cwd dedicado; flags `-p -s --no-ask-user --deny-tool shell/write`, nunca `allow-tool` ni `git`; **el prompt va dentro de argv** tras `-p`. **ADAPTAR:** RF-IA1 prohíbe prompt en argv; hay que invertir esa aserción | RF-IA1, RF-IA5, §5 (IA solo propone) | `ariadna-ai::copilot` | M6.4, M6.14, M6.17 |
| AITests | testContextLimitBlocksTheProcess | Diff mayor que el tope (8 bytes) → `contextTooLarge` y el CLI no se llama | §5.1 (tope 100 000 bytes) | `ariadna-ai::context` | M6.2 |
| AITests | testCodexParsesJSONLAndProposesConflictWithoutApplying | Parsea JSONL de Codex (`agent_message`), propuesta de conflicto con contenido y razón, nota de tokens; prefijo de args `exec --json --sandbox read-only --skip-git-repo-check`; cwd dedicado. **ADAPTAR:** prompt por stdin (candidato `… -`) | RF-IA7, RF-IA1, RF-IA5 | `ariadna-ai::codex`, `ariadna-ai::parse` | M6.4, M6.14 |
| AITests | testCodexLegacyAssistantMessageAndFailure | Formato antiguo `assistant_message`; `turn.failed` lanza error | RF-IA7 | `ariadna-ai::parse` | M6.4 |
| AITests | testOrchestratorAutoFallsBackAndExplicitChoiceDoesNot | Auto salta de Copilot (no instalado) a Codex; aceptar/rechazar solo cambia estado; elección explícita sin fallback → `providerUnavailable`. **ADAPTAR:** orden Claude→Codex→Copilot, consentimiento por proveedor y salto solo por «no instalado» (RF-IA9) | RF-IA9 | `ariadna-ai::orchestrator` | M6.9 |
| AITests | testAuthenticationErrorIsActionable | stderr «Please run `codex login`» → `notAuthenticated` con pista `codex login`. **ADAPTAR:** el spec no define detección por texto; define `AiNotLoggedIn` por prompt interactivo (RF-IA6) y sondeo (RF-IA4) | RF-IA4, RF-IA6, §3.3 (`AiNotLoggedIn`) | `ariadna-ai::availability`, `ariadna-ai::error` | M6.7 |
| AITests | testSettingsRoundTrip | Guardar/cargar `AISettings` (proveedor, tope de contexto). **ADAPTAR:** el modelo cambia a perfiles + consentimiento (D16) | D10, D16, §5.1 | `ariadna-ai::settings` | M6.10, M6.12 |
| AITests | testCommitMessageFencesAreStripped | Quita vallas ```` ```text ```` de la respuesta y separa asunto/cuerpo | RF-IA7 (parseo), acción «mensaje de commit» | `ariadna-ai::parse` | M6.14 |

### 1.6 Totales y comparación con 36 + 8

| Archivo Swift | Tests reales | Documentado |
|---|---|---|
| GitCLIEngineTests.swift | 28 | — |
| OperationQueueTests.swift | 8 | — |
| ParserTests.swift | 18 | — |
| ProcessAndStoreTests.swift | 4 | — |
| **GitCoreTests (suma)** | **58** | **36** → +22 |
| AITests.swift (AriadnaAITests) | **8** | 8 → coincide |
| **Total** | **66** | **44** → +22 |

Reparto de los 66 tests por hito destino (para reescribir el criterio de salida de spec §11; verificado por script contra la tabla 1.x):

| Hito | Área | Tests Swift |
|---|---|---|
| M1 | `ParserTests` (porcelain 4, diff 6, log 2, rama 1, remoto 1, conflicto 2, AmendDraft 2) | 18 |
| M1 | `ProcessAndStoreTests` (entorno, runner ×2, recientes) | 4 |
| M1/M2 | `OperationQueueTests` (cola, locale, hash, refresco cancelado, login shell, script de hooks, bundle id) | 8 |
| M2 | `GitCLIEngineTests`: open/status/stage/commit/hunks/amend/log/búsqueda | 16 |
| M3 | `GitCLIEngineTests`: ramas, remotos, merge, conflictos, restore, rebase | 12 |
| M6 | `AITests` | 8 |
| | **Total** | **66** |

Criterio de salida propuesto: M1 = 22 (`ParserTests` + `ProcessAndStoreTests`) + `testLoginShellPathIsCaptured` (23 si se decide mantener) · M2 = 16 + 4 de `OperationQueueTests` (clasificación, serie/solape, locale, hash) + refresco cancelado = 21 · M3 = 12 · M6 = 8. Los dos de identidad/tooling (`testInstallScriptConfiguresHooksPath` NO PORTAR; `testBundleIdentifier` pasa a `src-tauri`) no cuentan para M2/M3.

## 2. Trazabilidad por requisito

Estado: `cubierto` = hay test Swift para el caso principal; `parcial` = hay test pero faltan casos del requisito; `sin test heredado: crear` = ningún test Swift. Todos los estados de tarea son `pendiente` salvo que se indique (en `tasks.md` solo está hecho M0.1 y M0.6; `ariadna-process` tiene código y 13 tests Rust aunque M1.4 sigue sin marcar).

### 2.1 Git (G-01…G-15)

| ID | Tests Swift que lo cubren | Tarea(s) Rust | Estado |
|---|---|---|---|
| G-01 abrir repo | testOpenStatusStageCommitAndLog (sube a raíz, no-repo), testOpeningRepoDoesNotWriteHooksPath | M2.3, M2.1 | cubierto. Sin test: `DubiousOwnership` (M3.9), worktrees (aviso), unidad desconectada |
| G-02 estado porcelain v2 | PorcelainOrdinary…, RenameWithSpaces…, RenameAndConflict, DetachedAndUnborn, testOpenStatus…, testIgnoredFilesAndSessionGate | M1.7, M2.4 | cubierto. Sin test: 50 000 archivos (M2.12), `--no-renames -uno` del estado ligero |
| G-03 stage/unstage por archivo | testOpenStatusStageCommitAndLog (stage). No hay test de unstage por archivo ni de listas largas | M2.5 | parcial: **crear** unstage por archivo y rutas por stdin |
| G-04 stage/unstage por hunk | testStageSingleHunkAndUnstageIt, StageIsolatedAndContiguousHunks, StageHunkWithCRLF…, StageHunkInUnicodePathWithSpaces, StageHunkOfUntrackedFile, FailedUntrackedHunk…, DiffParserKeepsCRLF…, DiffParserTwoHunks | M1.8, M2.6 | cubierto. Sin test: matriz `autocrlf`/`.gitattributes` (M2.6 la exige), UTF-16/BOM |
| G-05 descartar con confirmación | testRestoreRequiresConfirmation | M3.6, M4.5 | parcial: **crear** test del token de confirmación (§6) y de rutas con símbolos de pathspec |
| G-06 commit | testEmptyCommitAndNothingStaged, testDetachedHeadWarnsBeforeCommit, testOpenStatus… | M2.7 | cubierto |
| G-07 amend seguro | testAmendRewritesUnpublishedCommitOnly, AmendRefusesDetachedHead, AmendRefusesDuringMerge, AmendDraft×2 | M2.8, M5.3 | parcial: **crear** amend con rebase en curso (el spec lo exige, Swift solo prueba merge) |
| G-08 log y diff de commit | testOpenStatus… (log), CommitLogParser×2, DiffParser×6 | M1.8, M2.9 | cubierto. Sin test: diff de un commit concreto, paginación (NF-4), diff de merge |
| G-09 búsqueda | testSearchByHashIgnoresCommitsOnOtherBranches, testSearchLogMatchesMessageAuthorAndHash, testHashDetection | M2.9 | cubierto. Sin test: «filtro local» (es UI) |
| G-10 ramas | testBranchesCheckoutRenameAndDelete, testCheckoutRemoteTrackingCreatesLocalBranch, testCheckoutDirtyTreeExplainsInSpanish, testCheckoutRemoteDoesNotSwitchDivergedLocal, testBranchTrackParser | M3.1 | cubierto (los casos diverged/dirty no están en el texto de G-10: ver hueco H-09) |
| G-11 fetch / pull / push | testPushFetchAndPullAgainstBareRemote, testPullRebaseReplaysLocalCommit, testPullRebaseConflictAbortAndContinue, testRemoteParser | M3.2, M3.7 | parcial: **crear** `fetch` (el test no lo llama), timeout de red, cancelación y pull cuando hay divergencia sin rebase |
| G-12 merge | testFastForwardNoFastForwardAndUpToDate | M3.3 | parcial: **crear** `--ff-only` (el motor Swift lo implementa, ningún test) |
| G-13 conflictos 3 vías | testConflictModelResolutionAndContinue, testConflictMarkersResolveOneHunk, testUnmergedIndexParser, testPathEscapeDoesNotWrite, testPullRebaseConflictAbortAndContinue | M1.9, M3.4 | parcial: **crear** escritura binaria con EOL y BOM (§4.1), conflictos delete/modify y add/add, binarios |
| G-14 resuelto / continuar / abortar | testConflictModelResolutionAndContinue, testAbortMergeRestoresCleanTree, testPullRebaseConflictAbortAndContinue | M3.5, M3.7 | parcial: **crear** cherry-pick con conflicto (pendiente heredado, M3.8) |
| G-15 recientes y favoritos | testRecentRepositoryRoundTrip | M1.10 | parcial: **crear** importación del `recents.json` Swift (formato ISO-8601) y el tope de 30 (+ favoritos) |

### 2.2 IA (RF-IA1…RF-IA12)

| ID | Tests Swift que lo cubren | Tarea(s) Rust | Estado |
|---|---|---|---|
| RF-IA1 prompt por stdin | Ninguno. testCopilotCommitMessage… y testCodexParses… aseguran lo contrario (prompt en argv) | M6.3, M6.4, M6.17 | **sin test heredado: crear** (los dos tests Swift se invierten) |
| RF-IA2 shims `.cmd` / `node.exe` | Ninguno | M6.5 | **sin test heredado: crear** (Windows; lista blanca de argumentos) |
| RF-IA3 entorno por lista blanca | Ninguno (el único test de entorno es el de git: ProcessAndStoreTests) | M6.6, M6.17 | **sin test heredado: crear** (CLI falso que vuelca entorno) |
| RF-IA4 detección ejecutando el CLI | testOrchestratorAuto… (`--version` simulado), testCopilotCommitMessage… | M6.7 | parcial: **crear** estado de sesión sin leer credenciales |
| RF-IA5 directorio de trabajo vacío | testCopilotCommitMessage… y testCodexParses… (cwd = directorio dedicado) | M6.3 | parcial: **crear** «limpiado tras cada llamada» y «sin ficheros de instrucciones» |
| RF-IA6 stdin aparte, tope de salida, sin TTY, timeout 120 s | testProcessRunnerEchoAndStdin y TimeoutAndMissingBinary (genéricos, de `ProcessRunner`) | M1.4, M6.3 | **sin test heredado de IA: crear** (prompt interactivo → `AiNotLoggedIn`, timeout 120 s) |
| RF-IA7 parseo por proveedor y versión | testCodexParsesJSONL…, testCodexLegacyAssistantMessageAndFailure, testCommitMessageFencesAreStripped | M6.3, M6.4, M6.14 | parcial: solo Codex y vallas. **Crear** golden files de Claude y Copilot y «formato no reconocido» |
| RF-IA8 cola de IA | Ninguno | M6.8 | **sin test heredado: crear** |
| RF-IA9 modo Auto | testOrchestratorAutoFallsBackAndExplicitChoiceDoesNot | M6.9 | parcial: la semántica cambia (ver D4); **crear** consentimiento y «sin fallback por error de contenido» |
| RF-IA10 términos de uso | Ninguno | M0.5 | **sin test heredado: crear** (es documental/cumplimiento, no test automático) |
| RF-IA11 política administrable (post-v0.1) | Ninguno | «Después de v0.1» | **sin test heredado: crear** (fuera de v0.1) |
| RF-IA12 cuentas de organización | Ninguno | M6.13, M0.7 | **sin test heredado: crear** (humo manual con cuentas de organización) |

Además, sin ID pero requeridos en §5: tope de contexto → testContextLimitBlocksTheProcess (M6.2, cubierto); la IA solo propone / `AiSuggestion` → testCopilotCommitMessage…, testCodexParses…, testOrchestratorAuto… (M6.1, parcial: falta estado obsoleto por `revision`); consentimiento, vista previa, redactor de secretos, marcado de contenido no fiable, rechazo de propuestas fuera de hunks (M6.2, M6.15) → **sin test heredado: crear**.

### 2.3 No funcionales (NF-1…NF-8)

| ID | Tests Swift que lo cubren | Tarea(s) Rust | Estado |
|---|---|---|---|
| NF-1 arranque ≤ 1,5 s | Ninguno | M7.2 | **sin test heredado: crear** |
| NF-2 primer pintado ≤ 300 ms | Ninguno | M7.2 | **sin test heredado: crear** |
| NF-3 status de 50 000 archivos ≤ 2 s | Ninguno | M2.12, M7.2 | **sin test heredado: crear** |
| NF-4 historial 100 000 commits | Ninguno (los límites 200/500 del Swift no tienen test) | M5.4, M7.2 | **sin test heredado: crear** |
| NF-5 diff grande | testDiffParserBinaryPath… (binario detectado). Resto sin test | M5.2, M7.2 | parcial: **crear** colapsado > 1 MB / 5 000 líneas y truncado de líneas largas |
| NF-6 20 pestañas | Ninguno (Swift: una ventana, un repo) | M4.6, M7.2 | **sin test heredado: crear** |
| NF-7 cancelación ≤ 5 s | testProcessRunnerTimeoutAndMissingBinary, testCancelledRefreshDoesNotPublishStatus | M1.4, M2.2 | parcial: **crear** medida del árbol de procesos (Rust ya tiene `timeout_mata_tambien_al_nieto`, `cancelacion_termina_el_proceso`) |
| NF-8 sin telemetría de código | Ninguno (Swift pasa `--disable-client-telemetry` a Copilot, sin test) | M6.4 | **sin test heredado: crear** |

### 2.4 Requisitos del spec Rust sin ID que sí tienen test Swift

| Requisito (sección) | Tests Swift | Tarea | Estado |
|---|---|---|---|
| §3.2 mutaciones en serie, lecturas solapadas, clasificación | testMutationCommandsAreClassified, testMutationsAreSerialAndReadsOverlap | M2.2 | cubierto (falta prioridad, clave de repo, coalescencia, `index.lock`) |
| §3.2 refresco cancelado no publica | testCancelledRefreshDoesNotPublishStatus | M2.2 | parcial (falta `revision` monótona) |
| §4.1 locale / codificación | testGitRunsWithEnglishMessages | M2.1 | ADAPTAR (D7) |
| §4.2 PathSafety | testPathEscapeDoesNotWrite | M1.6 | parcial: **crear** toda la lista cerrada de §4.2 (18 casos), solo hay 1 caso heredado |
| §6 entorno de git | testSanitizedEnvironmentDropsTokenLikeVariables | M1.4 | conflicto con el código Rust actual (D3) |
| §6 no escribir `core.hooksPath` en repos ajenos | testOpeningRepoDoesNotWriteHooksPath | M2.1 | cubierto |
| §6 confirmaciones | testRestoreRequiresConfirmation, testConflictModelResolutionAndContinue, testIgnoredFilesAndSessionGate | M4.5 | parcial: el modelo cambia a token ligado a `revision` |
| §3.1 ciclo de vida, §3.3 IPC, §8 pestañas, §7 diseño | Ninguno (Swift: una ventana, un repo) | M4.x, M5.x | **sin test heredado: crear** (todo es novedad) |

### 2.5 Requisitos del Swift actual → requisito Rust (cuadro inverso)

Origen: `specs/spec.md` y `specs/tasks.md` del Swift.

| Requisito Swift | Requisito Rust | Tarea | Test Swift |
|---|---|---|---|
| §3 Abrir carpeta; detectar `.git` | G-01 | M2.3 | testOpenStatus… |
| §3 Recientes y favoritos | G-15 | M1.10, M4.9 | testRecentRepositoryRoundTrip |
| §3 Estados (modificado, sin seguimiento, preparado, conflicto, ignorado con toggle) | G-02 | M2.4, M5.1 | Porcelain×4, testIgnored… (el toggle no existe en la UI Swift, ver D9) |
| §3 Stage/unstage por archivo y hunk | G-03, G-04 | M2.5, M2.6 | ver §2.1 |
| §3 Descartar con confirmación | G-05 | M3.6 | testRestoreRequiresConfirmation |
| §3 Commit; mensaje vacío rechazado; HEAD detached con aviso | G-06 | M2.7 | testEmptyCommit…, testDetachedHead… |
| §3 «Amend queda fuera» (**obsoleto**: tasks.md y código lo incluyen) | G-07 | M2.8 | Amend×3, AmendDraft×2 |
| §3 Log, diff unificado de worktree/índice/commit | G-08 | M2.9 | CommitLogParser×2, DiffParser×6 |
| tasks.md: búsqueda en todo el historial | G-09 | M2.9 | Search×2, HashDetection |
| §3 Ramas listar/crear/renombrar/borrar/checkout; HEAD detached visible | G-10 | M3.1 | Branches…, CheckoutRemote…, BranchTrackParser |
| §3 Remotos: listar, **añadir**, fetch, pull, push | G-11 (listar/añadir no figuran) | M3.2 | PushFetchPull…, RemoteParser |
| §3 Merge `--ff`, `--no-ff`, `--ff-only` | G-12 | M3.3 | testFastForward… |
| §3 Conflictos: etapas, marcadores diff3, lado por archivo/hunk, escribir con confirmación | G-13 | M1.9, M3.4 | ConflictModel…, ConflictMarkers…, UnmergedIndexParser |
| §3 Marcar resuelto, continuar, abortar (merge, rebase, cherry-pick) | G-14 | M3.5 | ConflictModel…, AbortMerge…, PullRebaseConflict… |
| §4 IA: mensaje, resumen, explicar, resolver conflicto | §5 + acciones | M6.14 | AITests |
| §4 Reglas 1–5 (no escribir, tope 100 000, conflicto en seco, comando visible sin prompt, error con `copilot login`/`codex login`) | §5, §5.1, RF-IA4/6 | M6.2, M6.16, M6.7 | ContextLimit…, AuthenticationError…, Copilot… |
| §4 Invocación Copilot/Codex con denegación de shell/write y sandbox read-only | §5.2, RF-IA1/2 | M6.4 | Copilot…, CodexParses… |
| §4 Entorno reducido de los CLI de IA | RF-IA3 | M6.6 | (ninguno) |
| §5 Cola serial por motor; IA fuera de la cola | §3.2, RF-IA8 | M2.2, M6.8 | OperationQueueTests |
| §5 Persistencia JSON en Application Support | §4.2 (rutas), `session.json` §8 | M1.10, M4.7 | testRecentRepositoryRoundTrip, testSettingsRoundTrip |
| §7 Pantallas (Bienvenida, Workspace, barra de commit, estudio de conflicto, historial, Ajustes, hoja de propuesta) | §7, §8 | M4.9, M5.1–M5.7, M6.16 | (ninguno: UI) |
| §7 Atajos ⌘O, ⌘↩ | §7 atajos, §8 | M4.10, M5.3 | (ninguno) — ⌘O y ⌘R no figuran en el spec Rust |
| §8 Timeout y cancelación | NF-7, §9 | M1.4 | ProcessAndStoreTests |
| §8 Sin telemetría de código | NF-8 | — | (ninguno) |
| §8 Accesibilidad: etiquetas | §7 accesibilidad | M7.1 | (ninguno) |
| §8 Núcleo compila en Linux | §10 CI (ubuntu) | M1.3 | toda la suite |
| §11.3 Una ventana, un repo | §8 pestañas [N] | M4.6 | (ninguno) |

## 3. Funcionalidad del Swift sin requisito en el spec Rust (huecos)

Recomendación: **Añadir** = incorporar al spec/tareas como requisito; **Fuera de v0.1** = decisión explícita; **Cubierto** = ya está en otra sección o tarea.

| # | Funcionalidad en Sources/** | Dónde | Recomendación |
|---|---|---|---|
| H-01 | Menú «Abrir repositorio…» (⌘O, vuelve a la bienvenida), refrescar con ⌘R, commit con ⌘↩ | `AriadnaMain.swift`, `RootView.swift`, `WorkspaceViews.swift` | **Añadir:** el spec Rust lista ⌘K, ⌘T/W/1–9, Ctrl+Intro (M5.3) pero no ⌘O ni ⌘R. Añadir a §7 «Atajos» y a la tabla de atajos con ámbito |
| H-02 | Barra de menú nativa con comando de apertura; ventana de Ajustes como escena propia; tamaño mínimo 980×640 | `AriadnaMain.swift` | **Añadir:** §7 dice «menú nativo de Tauri donde se pueda» sin contenido. Definir menús (Archivo/Ver/Ayuda), Ajustes y tamaño mínimo de ventana |
| H-03 | `AmendDraft`: precarga del último mensaje al activar el amend, borrado del mensaje no tocado al desactivar, aviso si el commit es un merge, casilla «Corregir el último commit», botón «Corregir commit» | `GitCore/AmendDraft.swift`, `AppModel.setAmend`, `CommitBar` | **Añadir** a G-07 (o G-07b): comportamiento de la barra de commit. Lógica en `ariadna-git::amend`, no en la UI. Tiene 2 tests heredados |
| H-04 | Barra de commit: contador «Asunto n/72» con aviso naranja, plantilla opcional del mensaje, reglas de habilitación del botón (mensaje vacío, nada preparado salvo amend) | `WorkspaceViews.CommitBar`, `AppModel.commitTemplate` | **Añadir** (el spec Swift §11.9 ya decía «plantilla opcional»; la plantilla además alimenta a la IA) |
| H-05 | Ignorados con interruptor: `includeIgnored` existe en el modelo pero **ninguna vista lo activa** (la sección «Ignorados» es inalcanzable) | `AppModel.includeIgnored`, `ChangesList` | **Añadir** el interruptor en M5.1 (el spec Swift §3 y el Rust G-02/M5.1 lo dan por hecho). Es un hueco del Swift, no paridad |
| H-06 | Acciones masivas «Preparar todo» / «Quitar todo» por sección; contadores por sección; estados vacíos («Todo al día», «Nada preparado») | `ChangesList` | **Añadir** a M5.1 (estados vacíos ya están en §7 punto 6; las acciones masivas no) |
| H-07 | Menú contextual: copiar ruta, descartar cambios (archivo), borrar rama (deshabilitado en la actual); doble clic en reciente; favoritos ordenados primero; estrella | `StatusRow`, `BranchSidebar`, `WelcomeView` | **Añadir** a M4.9/M5.1/M5.5 |
| H-08 | Borrado de rama con diálogo y «Forzar borrado» (`-D`); el motor impide borrar la rama actual | `WorkspaceView`, `AppModel.deleteBranch` | **Añadir** a G-10 (borrado forzado y confirmación) |
| H-09 | Checkout con reglas finas: rama remota crea local con seguimiento; si la local diverge pide confirmación; cambios locales que bloquean el cambio se explican | `GitCLIEngine.checkout`, `RootView` (diálogo) | **Añadir** a G-10 (3 casos con test heredado: Remote, Diverged, DirtyTree) |
| H-10 | Solo se opera con el **primer remoto** (`remotes.first`) en fetch/pull/push; `addRemote` y `renameBranch` existen en el motor y **no tienen UI** | `AppModel.fetch/pull/push` | **Añadir** decisión a G-11: ¿varios remotos? ¿añadir remoto en v0.1? Recomendado: motor sí, UI «fuera de v0.1» salvo selector de remoto |
| H-11 | Merge solo con `fastForward: .allow` desde la UI; no-ff y ff-only solo en el motor | `AppModel.merge`, `BranchSidebar` | **Fuera de v0.1** en UI o **añadir** selector; G-12 pide los tres modos |
| H-12 | Banner de operación en curso (merge/rebase/cherry-pick) con Continuar (deshabilitado si hay conflictos) y Abortar con confirmación; cambio automático a la pestaña de conflictos | `WorkspaceView.integrationBanner`, `AppModel` | **Añadir** a M5.6/G-14 |
| H-13 | Estudio de conflicto: 8 tipos de conflicto con etiqueta, lados ausentes y binarios con mensaje, botones por hunk incluida «Base», indicador de resolución sin escribir, contador «Conflictos (n)» | `ConflictAndSettings.swift` | **Añadir** a G-13/M5.6 (tipos y estados de lado) |
| H-14 | Historial: filtro local + «Buscar en todo» + «Ver los últimos»; límites 200 (log) y 500 (búsqueda); copiar hash; insignia de merge; fecha absoluta en tooltip; «Explicar con IA» | `HistoryList`, `HistoryDetail`, `AppModel` | **Cubierto** en G-09 y NF-4 (paginado 200); **añadir** límite de la búsqueda |
| H-15 | Recientes: máx. 30 más favoritos extra, orden por último uso, `lastBranch`, fechas ISO-8601, escritura atómica | `RecentRepositoryStore.swift` | **Añadir** a G-15 (esquema y tope). Solo se importa `recents.json`, pero no `settings.json` ni `ai-settings.json` (ver H-16) |
| H-16 | Ajustes persistentes `settings.json` (`gitBinary`, `pullUsesRebase`) y `ai-settings.json`; al guardar se reabre el repo | `AriadnaIdentity.swift`, `AppModel.saveSettings` | **Añadir:** esquema de ajustes de Rust y decidir si se importan los de Swift. M5.7 menciona pull con rebase y ruta de git pero no persistencia |
| H-17 | Rutas manuales de los binarios `copilot`/`codex` en Ajustes y tope de contexto editable | `SettingsView`, `AISettings` | **Añadir** a M6 (RF-IA2 resuelve el binario pero no permite fijarlo a mano) |
| H-18 | Hoja de propuesta: muestra proveedor, resumen del comando (sin el prompt), nota de transparencia (tokens usados), línea de seguridad por tipo, texto editable, «Usar este mensaje», «Aplicar al editor» (solo copia al editor de resultado), copiar, regenerar | `SuggestionSheet`, `AppModel` | **Añadir** a M6.16: el spec Rust la cambia por «vista previa exacta» y no menciona resumen de comando ni tokens; «Aplicar al editor» ≠ escribir en disco |
| H-19 | Resumen con dos estilos (`bullets`, `pullRequest`); la UI solo usa `pullRequest` | `AIModels.SummaryStyle`, `AppModel.summarizeStaged` | **Añadir** decisión a M6.14 (un estilo o dos) |
| H-20 | Pista accionable por proveedor (`copilot login`, `codex login`) y mensajes «No se ha enviado nada» al superar el tope | `AIError.swift` | **Cubierto** parcialmente por `AiNotLoggedIn` y «acción sugerida» (§3.3); **añadir** texto por proveedor, incluido `claude` |
| H-21 | Flags extra de Copilot: `--disable-builtin-mcps`, `--disable-client-telemetry`, `--excluded-tools bash,powershell,create,edit,apply_patch,task,write_bash,write_powershell` | `CLICommands.swift` | **Añadir** al spike M0.2/§5.2: el candidato Rust («lista blanca de herramientas») no los recoge. Enlaza con NF-8 |
| H-22 | Sondeo de disponibilidad con tope de 20 s; mapea no encontrado, timeout, cancelado | `CLIInvocation.swift` | **Cubierto** por RF-IA4; añadir el tope |
| H-23 | `GIT_MERGE_AUTOEDIT=no`, `GIT_SEQUENCE_EDITOR=true`, `GIT_EDITOR=true` | `GitCLIEngine.nonInteractiveGit` | **Añadir:** §4.1 solo cita `GIT_EDITOR=true`; falta `GIT_SEQUENCE_EDITOR` y `GIT_MERGE_AUTOEDIT` |
| H-24 | `status` con `--untracked-files=all` y `--ignored=matching`; diffs sintéticos para archivos sin seguimiento; `intent-to-add` deshecho si el hunk falla | `GitCLIEngine` | **Añadir** a G-02/G-04 (el estado completo no define `-uall`; el ligero usa `-uno`) |
| H-25 | Timeout de git 180 s por operación; AI 180 s | `GitCLIEngine`, providers | **Añadir:** §4.1 habla de timeout de red; falta el de operaciones locales (Rust fija IA en 120 s) |
| H-26 | `Info.plist` (bundle id `es.javiermartinez.ariadna`, versión, `LSMinimumSystemVersion` 15.0) | `Sources/App/Info.plist` | **Añadir** el identificador a §10 / M1.1 (D15 solo fija macOS 15) |
| H-27 | `scripts/build-app.sh`: binario universal, versión desde la etiqueta (`0.0.0-<sha>` si no hay), `.app` + `.dmg` + **`.zip`**, firma ad-hoc, Developer ID y notarización con 5 secrets | `scripts/build-app.sh` | **Cubierto** en §10/M7.7 salvo el `.zip` y la regla de versión: añadir |
| H-28 | Capturas automáticas: `SnapshotMain.swift`, `scripts/render-snapshots.sh`, workflow `screenshots.yml`, 6 PNG en `docs/screenshots` | `SnapshotMain.swift` (429 líneas) | **Cubierto** por M7.5 (sin requisito en el spec): añadir una línea en §10 |
| H-29 | Workflows `ci.yml` (dos pasadas con `ARIADNA_AI=1`), `release.yml`, `rust.yml`, `screenshots.yml` | `.github/workflows` | **Cubierto** en §10 para CI; **añadir** tarea del workflow de release de Rust (M7 solo habla de artefactos) |
| H-30 | `.githooks/pre-commit` (gitleaks) y `scripts/install-git-hooks.sh` | `.githooks`, `scripts` | **Cubierto** por §6 y M0.13; el test Swift asociado no se porta |
| H-31 | Flag de compilación `ARIADNA_AI` y pantalla de Ajustes alternativa sin IA | `Package.swift`, `SettingsView` | **Cubierto** por D10 (IA siempre en el binario, apagada) |
| H-32 | `PathSafety.validateRef`: rechaza nombres vacíos, con prefijo `-`, `..`, `@{`, `.lock`, caracteres prohibidos | `PathSafety.swift` | **Cubierto** por §4.1 (`git check-ref-format`, rechazo de `-`) |
| H-33 | Mensajes informativos tras cada acción («Commit creado. No se ha hecho push.», «Operación abortada…») y alerta «No se pudo completar» | `AppModel`, `RootView` | **Cubierto** en §3.3 (toast/banner); **añadir** el catálogo de textos en español al backlog de UI |
| H-34 | Idioma: UI en español; mensajes de git se muestran tal cual los emite el CLI (spec Swift §7) | `spec.md` Swift §7 | **Añadir:** el spec Rust habla de «texto localizable» sin decidir idiomas de v0.1 |
| H-35 | Botones Pull/Push muestran `behind`/`ahead`; etiqueta de rama y aviso de HEAD detached en la barra | `WorkspaceView.toolbar` | **Añadir** a M5.5 |
| H-36 | Linux: el núcleo compila y se prueba sin Mac | `Package.swift`, `ConflictAndSettings` | **Cubierto** (CI ubuntu §10) |
| H-37 | Selector de carpeta nativo (`NSOpenPanel`) en la bienvenida | `AppModel.FolderPicker` | **Cubierto** por M4.9 («Examinar»); plugin dialog de Tauri |
| H-38 | `git clean` de no rastreados «a propósito no está» y `Stage por líneas` pendiente | `specs/tasks.md` | **Fuera de v0.1** (el spec Rust ya lo declara) |
| H-39 | `DiffScope.between(base, head)` en el motor, sin uso en la UI | `GitServing` | **Fuera de v0.1** |

## 4. Novedades del spec Rust (no existen en Swift)

Comprobación de que están marcadas `[N]` en §1.1 o, si no lo están, de dónde aparecen.

| Novedad | Dónde | ¿Marcada `[N]` en §1.1? | Acción |
|---|---|---|---|
| IA con Claude | §5.2, M6.3 | Sí | — |
| Modo Auto como novedad | §1.1 fila «Modo Auto, consentimiento por proveedor, ajustes por proveedor» | **Parcialmente erróneo:** el Swift ya tiene Auto (Copilot→Codex) | Separar: Auto = `[P]` con semántica cambiada (D4); consentimiento y ajustes por proveedor = `[N]` |
| Consentimiento por proveedor y perfil | §5.1 | Sí (proveedor); **no** (perfil) | Añadir el perfil a la fila |
| Pestañas por repo | §8 | Sí | — |
| Windows (x64, ARM64) | §4.2, §10, D7 | Sí | — |
| Tema moderno, paleta de comandos | §7 | Sí | — |
| Perfiles Personal/Empresa, doble uso, cuenta activa (RF-IA12), política administrable (RF-IA11) | D14, D16, §5.1.1 | **No** | Añadir fila `[N]` |
| Interruptor global de IA, IA siempre en el binario (D10) | §5.1, D10 | **No** | Añadir a la fila de IA |
| Vista previa exacta del envío, exclusión de sensibles, redactor de secretos, marcado de contenido no fiable, registro local | §5.1 | **No** | Añadir fila `[N]` «privacidad de la IA» |
| Estado obsoleto de propuestas por `revision` | §5 | **No** | Añadir |
| Cola de IA (2 simultáneas, límite por minuto) | RF-IA8 | **No** | Añadir |
| Prompt por stdin, shims `.cmd`/`node.exe`, entorno por lista blanca, directorio de trabajo vacío | RF-IA1/2/3/5 | **No** (en Swift: argv y cwd temporal) | Añadir; afecta a tests ADAPTAR |
| `SessionManager`, `RepositorySession` con ciclo de vida y cierre con confirmación | §3.1 | **No** | Añadir |
| Cola por clave de repo con prioridad, refrescos coalescidos, `revision` por snapshot | §3.2 | **No** (Swift: cola por motor) | Añadir |
| Contrato IPC tipado, `operation_id`, `cancel_operation`, eventos delta | §3.3 | **No** | Añadir |
| Errores tipados con código estable (enum) | §3.3 | **No** | Añadir (y completar variantes, D8) |
| Endurecimiento de git: ruta absoluta, versión ≥ 2.39, `--literal-pathspecs`, `-c core.fsmonitor=false`, rutas por stdin, `core.longpaths`, `safe.directory` | §4.1 | **No** | Añadir fila `[N]` «endurecimiento» |
| Reintento acotado ante `index.lock` ajeno, borrado condicionado de `index.lock` | §3.1, §3.2, M2.11 | **No** | Añadir |
| `DubiousOwnership` con confirmación | §4.1, M3.9 | **No** | Añadir |
| `PathSafety` con lista cerrada Windows/macOS (UNC, NTFS, 8.3, NFD/NFC) | §4.2 | **No** (Swift: 3 comprobaciones) | Añadir |
| Matriz `core.autocrlf` / `.gitattributes` / UTF-16 / BOM | §4.2, M2.6 | **No** | Añadir |
| Token de confirmación de un solo uso, diálogo nativo desde Rust | §6 | **No** (Swift: `confirmed: Bool`) | Añadir |
| CSP estricta, capabilities mínimas, sin `{@html}` | §6, M4.2 | **No** | Añadir |
| Vigilancia de archivos (`notify`), hibernación de pestañas, `session.json` | §8 | Pestañas sí; **watcher no** (Swift refresca a mano) | Añadir el watcher |
| Tokens de diseño, ratios de contraste, densidades, fuentes Inter/JetBrains Mono, accesibilidad ampliada | §7 | Parcial («tema moderno») | Cubierto |
| Requisitos NF-1…NF-8 con método de medida | §9 | **No** | Añadir (Swift solo tenía timeout y cancelación) |
| Job Object de Windows, `CREATE_NO_WINDOW`, grupo de procesos con `SIGTERM`/`SIGKILL` | §9 | **No** | Parcialmente implementado en `ariadna-process` |
| Distribución: NSIS, WebView2, SignPath, MSI posterior, CI en 4 runners, regresión visual, axe | §10 | Windows sí; el resto **no** | Añadir |
| Importación de `recents.json` | G-15 | `[P]` (es migración) | — |
| Licencia MIT (D18) | §2 | n/a | — |

## 5. Discrepancias entre el spec Rust y el Swift real

| ID | Discrepancia | Evidencia | Impacto / acción |
|---|---|---|---|
| D1 | **Recuento de tests:** hay 58 en `GitCoreTests` (28+8+18+4) y 8 en `AriadnaAITests`, no 36 + 8 | `grep -c "func test"` por archivo; el «36» está en spec Rust §11, `tasks.md` M0 y `specs/tasks.md` Swift | Corregir los tres sitios; rehacer el criterio de salida de M2/M3 sobre 58 |
| D2 | `§1.1` marca «Modo Auto» como `[N]` pero el Swift ya lo tiene (`AIProviderChoice.auto`, `AIOrchestrator`) | `AIModels.swift`, `ConflictAndSettings.swift` | Re-marcar: Auto `[P]` con cambios de semántica |
| D3 | **Filtro de entorno de git:** §6 dice que el Swift filtraba por subcadena. En realidad es una lista blanca por nombre (`PATH`, `HOME`, `LC_*`, `SSH_AUTH_SOCK`…) más prefijos `GIT_*`/`SSH_*` que no parezcan secretos. El test asegura que `GIT_TOKEN`, `OPENAI_API_KEY`, `AWS_SECRET_ACCESS_KEY` y `RANDOM_THING` se eliminan. El código Rust `EnvPolicy::git_default()` hereda todo y exime `GIT_*`, así que **`GIT_TOKEN` y `RANDOM_THING` pasarían**: el test no se puede portar sin decidir | `ProcessRunner.swift:70-100`, `ProcessAndStoreTests`, `crates/ariadna-process/src/env.rs` | Decidir precedencia (denegar subcadena sobre `GIT_*`?) y si se mantiene «lista blanca» o «heredar y filtrar». Añadir el test heredado al crate |
| D4 | **Modo Auto:** Swift salta de proveedor ante `notAuthenticated`, `providerFailed`, `timedOut`, `emptyResponse` y `providerUnavailable`; orden Copilot→Codex. RF-IA9 solo salta si el CLI **no está instalado**, y el orden es Claude→Codex→Copilot, solo entre consentidos | `AIError.allowsFallback`, `AIOrchestrator.swift` | Cambio de comportamiento intencional: marcarlo `[N]` y reescribir `testOrchestratorAuto…` |
| D5 | **Prompt en argv:** Swift pasa el prompt como argumento (`-p <prompt>`; Codex lo pone al final) y los tests lo comprueban. RF-IA1 lo prohíbe | `CLICommands.swift`, `AITests` (líneas 51-53, 100) | Los dos tests de proveedor se invierten; el resumen del comando deja de ser relevante |
| D6 | **Flags de Copilot:** Swift usa `--disable-builtin-mcps`, `--disable-client-telemetry`, `--deny-tool shell/write`, `--excluded-tools …`. El candidato de §5.2 («lista blanca de herramientas») no los menciona | `CLICommands.swift` | Pasarlos al spike M0.2 |
| D7 | **Parseo de texto humano:** el motor Swift decide por texto en inglés («nothing to commit», «Already up to date», «Fast-forward», «CONFLICT», «would be overwritten») y por eso fuerza el locale. §4.1 Rust prohíbe parsear mensajes de error y solo apoya `LC_ALL=C` | `GitCLIEngine.classifyMerge`, `blocksBranchSwitch`, `commit` | Afecta a 4 tests: EmptyCommit/NothingStaged, FastForward…, CheckoutDirtyTree…, y el de locale. Diseñar detección por códigos de salida, comparación de HEAD antes/después y `status` |
| D8 | **Enum de errores §3.3 incompleto:** faltan `EmptyCommitMessage`, `NothingToCommit`, `DetachedHeadCommit`, `CannotAmend`, `LocalChangesBlockCheckout`, `DivergedLocalBranch`, `InvalidReference`, `HunkNotFound`, `NoIntegrationInProgress`, `ConfirmationRequired`, `PathEscapesRepository`, `Parse`. Los tests Swift los usan | `GitError.swift`, `GitCLIEngineTests` | Ampliar §3.3 antes de portar M2/M3 |
| D9 | El spec Swift (§3) y el Rust (G-02, M5.1) dan por hecho un interruptor de «ignorados»; el código Swift no lo tiene | `AppModel.includeIgnored` nunca se activa | Añadirlo como trabajo nuevo, no paridad |
| D10 | **Timeouts:** Swift 180 s en git y en IA; Rust fija 120 s para IA y no fija el de git local | `GitCLIEngine.init`, `CodexExecProvider.init`; RF-IA6 | Documentar el cambio y fijar el de git |
| D11 | El spec Swift §3 dice «Amend queda fuera de esta entrega»; `tasks.md` Swift y el código lo incluyen (G-07 es correcto) | `specs/spec.md:34` vs `specs/tasks.md:29` | Corregir el spec Swift si se conserva como oráculo |
| D12 | **Fetch sin test:** `testPushFetchAndPullAgainstBareRemote` no llama a `fetch` | `GitCLIEngineTests.swift:277-312` | Crear test (G-11) |
| D13 | **`--ff-only` sin test**, aunque G-12 lo exige y el motor lo implementa | `GitCLIEngine.merge` | Crear test (G-12) |
| D14 | `tasks.md` M1 no tiene tarea para `CommitLogParser`, `BranchListParser`, `RemoteListParser`, `UnmergedIndexParser`, `AmendDraft` ni para el PATH del login shell, pero 7 tests de `ParserTests`/`OperationQueueTests` dependen de ellos | `tasks.md` M1 | Añadir tareas a M1 (o mover explícitamente a M2/M3) |
| D15 | **Estado de M1.4:** `ariadna-process` existe con 9 tests de integración y 4 unitarios (commit `1501b28`), pero la tarea sigue `[ ]` | `crates/ariadna-process`, `tasks.md` | Marcar como parcial; solo falta lo del Job Object/`CREATE_NO_WINDOW` si no está |
| D16 | **`PathSafety`:** la lista cerrada de §4.2 tiene ≈18 casos; el Swift solo tiene un test indirecto (`testPathEscapeDoesNotWrite`) | `PathSafety.swift` | La mayoría son tests nuevos |
| D17 | **Auth por texto:** el Swift detecta «no hay sesión» por frases en stderr (`not logged in`, `copilot login`, `unauthorized`…); Rust RF-IA4/6 pide ejecutar el CLI y tratar el prompt interactivo como `AiNotLoggedIn`, sin reglas de texto | `AIError.swift` (`AIAuth`) | Decidir si se conserva la heurística por proveedor |
| D18 | **Login shell:** Swift obtiene el `PATH` con `$SHELL -l`; Rust propone «PATH ampliado» fijo | `ProcessRunner.pathFromLoginShell`, `OperationQueueTests` | Decidir; si el PATH ampliado no basta (nvm, asdf), el test se mantiene |
| D19 | **Remotos:** el spec Swift lista «añadir remoto»; G-11 no. El motor tiene `addRemote`; la UI no | `GitServing.addRemote` | Decidir (H-10) |

## 6. Cómo mantener esta matriz

1. Al portar un test, anotar su nombre Rust junto a la fila (columna Tarea) y marcar la tarea.
2. Un test Swift solo se da por portado cuando su fila 1.x apunta a un test Rust que pasa en macOS y Windows.
3. Cualquier requisito de la tabla §2 con estado **sin test heredado: crear** necesita un test nuevo antes de cerrar su tarea.
4. Regenerar el recuento con: `grep -c "func test" Tests/*/*.swift`.

## Anexo A. Numeración de tareas usada

`Mn.k` = k-ésima tarea de la sección Mn de `specs/rust/tasks.md`, en orden de aparición.

| ID | Tarea |
|---|---|
| M0.1 | Spec v1 revisada con EGM |
| M0.2 | Spike CLI de IA: `codex` y `copilot` |
| M0.3 | Spike `claude` |
| M0.4 | Spike WebView2 |
| M0.5 | Términos de uso (RF-IA10) |
| M0.6 | Licencia MIT |
| M0.7 | Spike de perfiles |
| M0.8 | Solicitar SignPath |
| M0.9 | Matriz de trazabilidad (este documento) |
| M0.10 | Inventario de tests Swift (este documento) |
| M0.11 | Fixtures de git |
| M0.12 | Mockups y tokens |
| M0.13 | `.gitignore` y `.gitleaks.toml` |
| M0.14 | Javier decide las PROPUESTAS |
| M1.1 | macOS 15 mínimo (Tauri y CI) |
| M1.2 | `Cargo.toml` workspace, toolchain, rustfmt, clippy, deny |
| M1.3 | CI Rust (4 runners, agregador) |
| M1.4 | `ProcessRunner` |
| M1.5 | Localizar `git` (ruta absoluta, ≥ 2.39) |
| M1.6 | `PathSafety` |
| M1.7 | `PorcelainParser` v2 -z |
| M1.8 | `DiffParser` |
| M1.9 | `ConflictParser` con diff3 |
| M1.10 | Almacén de recientes y favoritos + importación |
| M1.11 | Fuzz/propiedades de los parsers |
| M2.1 | Trait `GitServing` y `GitCliEngine` con endurecimiento |
| M2.2 | Cola serial por clave de repo, lecturas sin lock, refrescos coalescidos |
| M2.3 | G-01 abrir repo |
| M2.4 | G-02 status |
| M2.5 | G-03 stage/unstage por archivo |
| M2.6 | G-04 stage/unstage por hunk |
| M2.7 | G-06 commit |
| M2.8 | G-07 amend seguro |
| M2.9 | G-08 log y diff · G-09 búsqueda global |
| M2.10 | Contrato IPC |
| M2.11 | Reintento ante `index.lock` |
| M2.12 | Tests con 50 000 archivos |
| M3.1 | G-10 ramas |
| M3.2 | G-11 fetch / pull / push |
| M3.3 | G-12 merge |
| M3.4 | G-13 conflictos a tres vías |
| M3.5 | G-14 resuelto / continuar / abortar |
| M3.6 | G-05 descartar con confirmación |
| M3.7 | Test `pull --rebase` con conflicto |
| M3.8 | Test cherry-pick con conflicto |
| M3.9 | `DubiousOwnership` |
| M4.1 | Proyecto Tauri 2 + Svelte 5 + `tauri-specta` |
| M4.2 | CSP y capabilities |
| M4.3 | Tokens y temas |
| M4.4 | `SessionManager` |
| M4.5 | Confirmaciones nativas desde Rust |
| M4.6 | Pestañas |
| M4.7 | `session.json` |
| M4.8 | Watcher |
| M4.9 | Bienvenida |
| M4.10 | Paleta de comandos y atajos |
| M4.11 | Errores (toast, banner, recuperación) |
| M5.1 | Pantalla Cambios |
| M5.2 | Diff por hunk (CodeMirror 6) |
| M5.3 | Barra de commit y amend |
| M5.4 | Historial |
| M5.5 | Ramas y remotos |
| M5.6 | Estudio de conflicto |
| M5.7 | Ajustes |
| M5.8 | Vitest |
| M5.9 | Publicar `alpha.1` |
| M6.1 | Trait `AiProvider`, `AiSuggestion` |
| M6.2 | Consentimiento, vista previa, exclusiones, redactor, contenido no fiable |
| M6.3 | `ClaudeCli` |
| M6.4 | `CodexCli` · `CopilotCli` |
| M6.5 | Resolución de binarios en Windows y shims |
| M6.6 | Entorno por lista blanca |
| M6.7 | Detección de disponibilidad y sesión |
| M6.8 | Cola de IA |
| M6.9 | Modo Auto |
| M6.10 | Perfiles Personal/Empresa |
| M6.11 | Directorio de configuración aislado |
| M6.12 | Consentimiento por perfil y proveedor |
| M6.13 | Cuenta activa por proveedor (RF-IA12) |
| M6.14 | Acciones de IA (mensaje, resumen, explicar, conflicto) |
| M6.15 | Rechazo de propuestas fuera de los hunks |
| M6.16 | Hoja de propuesta |
| M6.17 | CLI falso en CI · AITests |
| M6.18 | Humo manual con CLI reales |
| M6.19 | Publicar `alpha.2` |
| M7.1 | Accesibilidad |
| M7.2 | Rendimiento NF-1…NF-6 |
| M7.3 | E2E con `tauri-driver` |
| M7.4 | Regresión visual |
| M7.5 | Capturas automáticas |
| M7.6 | Proxy y CA del sistema |
| M7.7 | `.dmg` y NSIS |
| M7.8 | Firma de binarios |
| M7.9 | Prueba de instalación en VM |
| M7.10 | Documentar apertura de alfas sin firmar |
| M7.11 | CHANGELOG y README |
| M7.12 | Tag `swift-final`, rama `legacy-swift` |
| M7.13 | Publicar `v0.1.0` |
