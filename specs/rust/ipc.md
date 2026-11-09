# Contrato IPC (Tauri ↔ interfaz)

Spec §3.3. **El estado vive en Rust**; la interfaz es una vista. Este documento fija nombres y formas para que el backend (`src-tauri`) y la UI (`ui/`) se construyan por separado.

## Reglas generales

- Los comandos son funciones `#[tauri::command]` en `snake_case`; la UI los llama con `invoke("nombre", { argumentos en camelCase })`.
- Las cargas son los tipos de `crates/ariadna-git` (`models.rs`) serializados con `serde` en **camelCase**, tal cual. No se reinventan tipos en el medio.
- `GitPath` viaja como **cadena** si es UTF-8 y como **lista de bytes** (`number[]`) si no lo es. La UI debe aceptar ambas (`type GitPath = string | number[]`) y devolverlas sin tocar.
- Toda llamada de pestaña lleva `tabId` (el `session_id` de Rust). Nunca hay estado global implícito.
- Las rutas del sistema (carpeta de repo) viajan como `string`.
- **Errores**: un comando fallido rechaza la promesa con un objeto `ApiError { code, message, detail? }`. `code` es estable (`NOT_A_REPOSITORY`, `GIT_NOT_FOUND`, `COMMAND_FAILED`, `NOTHING_TO_COMMIT`, `EMPTY_COMMIT_MESSAGE`, `DETACHED_HEAD_COMMIT`, `CANNOT_AMEND`, `LOCAL_CHANGES_BLOCK_CHECKOUT`, `DIVERGED_LOCAL_BRANCH`, `INVALID_REFERENCE`, `HUNK_NOT_FOUND`, `NO_INTEGRATION`, `CONFIRMATION_REQUIRED`, `PATH_ESCAPES_REPOSITORY`, `TIMEOUT`, `CANCELLED`, `PARSE`, `LOCKED`, `DUBIOUS_OWNERSHIP`, `IO`, `REPO_MISSING`, `AUTH`, `AI_DISABLED`, `AI_NOT_INSTALLED`, `AI_NOT_LOGGED_IN`, `AI_QUOTA`, `AI_BAD_OUTPUT`, `AI_CONTEXT_TOO_LARGE`, `AI_TIMEOUT`, `AI_CONSENT_REQUIRED`, `AI_NO_STAGED_CHANGES`, `AI_SENSITIVE_FILE`, `NOT_FOUND`, `POLICY`, `RATE_LIMITED`, `NETWORK`). `message` ya está en español y listo para mostrar. La UI **nunca** interpreta `message`.
- **Confirmaciones destructivas**: la UI **no** envía ningún "confirmed". Los comandos destructivos abren un **diálogo nativo desde Rust** que nombra repo, rama y ruta, y devuelven `{ done: false }` si el usuario cancela. Lo hacen `discard`, `write_conflict_resolution`, `abort_integration`, `checkout` cuando hay divergencia, `commit` en HEAD detached y cerrar una pestaña con trabajo pendiente.
- **Revisión**: cada `Snapshot` lleva `revision` monótona. La UI descarta un snapshot con revisión menor que la que ya tiene.
- Operaciones largas (`fetch`, `pull`, `push`, IA): emiten `session-event` con `OperationStarted/Finished`; la UI las muestra y puede llamar a `cancel_operation`.

## Comandos de aplicación

| Comando | Argumentos | Devuelve |
|---|---|---|
| `app_state` | — | `AppState { tabs: TabInfo[], activeTabId: string \| null, recents: RecentRepository[], git: GitInfo \| null, gitProblem: string \| null }` |
| `pick_folder` | — | `string \| null` (diálogo nativo) |
| `open_repository` | `path` | `OpenResult { tab: TabInfo, alreadyOpen: boolean }` (si el repo ya está abierto, devuelve la pestaña existente y la activa) |
| `close_tab` | `tabId` | `{ closed: boolean }` (si hay trabajo pendiente, Rust pregunta con diálogo nativo) |
| `activate_tab` | `tabId` | `void` |
| `reorder_tabs` | `tabIds: string[]` | `void` |
| `toggle_favorite` | `path` | `RecentRepository[]` |
| `forget_recent` | `path` | `RecentRepository[]` |
| `cancel_operation` | `tabId, operationId` | `boolean` |
| `open_external_url` | `url` | `void` (abre el navegador del sistema; **solo `https`** y solo hosts de una lista: `learn.microsoft.com`, `dev.azure.com`, `*.visualstudio.com`, `github.com`, `docs.github.com`, `gitlab.com`, `docs.gitlab.com`, `bitbucket.org`, `support.atlassian.com`; sirve para las ayudas de «cómo crear un PAT») |
| `clone_repository` | `url, destination: string, branch: string \| null` | `OpenResult` (clona y abre una pestaña; emite `clone-progress`; cancelable con `cancel_operation`) |
| `store_credential` | `url, username, secret` | `void` (entrega el secreto a `git credential approve` por stdin; **nunca** vuelve a la UI) |
| `test_credential` | `url` | `{ ok: boolean, code: string \| null }` (`git ls-remote`) |
| `forget_credential` | `url, username` | `void` (`git credential reject`) |
| `list_pull_requests` | `tabId, filter: "open" \| "mine" \| "reviewing" \| "closed", page` | `PullRequestSummary[]` |
| `get_pull_request` | `tabId, id` | `PullRequestDetail` (descripción en Markdown, revisores, políticas, work items, commits) |
| `create_pull_request` | `tabId, draft: NewPullRequest` | `PullRequestDetail` |
| `pull_request_template` | `tabId` | `string \| null` |
| `pull_request_diff` | `tabId, id` | `FileDiff[]` |
| `pull_request_threads` | `tabId, id` | `ReviewThread[]` |
| `add_pull_request_comment` | `tabId, id, thread: NewThread \| { replyTo }` | `ReviewThread` |
| `resolve_pull_request_thread` | `tabId, id, threadId, resolved` | `ReviewThread` |
| `vote_pull_request` | `tabId, id, vote` | `PullRequestDetail` (confirmación nativa si el voto es de rechazo) |
| `complete_pull_request` | `tabId, id, options` | `{ done: boolean }` (confirmación nativa; respeta las políticas del proveedor) |
| `abandon_pull_request` / `reopen_pull_request` | `tabId, id` | `{ done: boolean }` |
| `checkout_pull_request` | `tabId, id` | `{ done: boolean }` |
| `open_remote_url` | `tabId, remote, kind: "repo" \| "pull-request", branch: string \| null` | `void` (abre el navegador del sistema; solo `https` y solo el host del remoto) |

`TabInfo { tabId, path, name, branch: string \| null, detached: boolean, changeCount: number, conflictCount: number, busy: string \| null, missing: boolean }`. `GitInfo { path, version }`.

La sesión (pestañas abiertas y orden) se guarda en `session.json` (versionado, escritura atómica) y se restaura al arrancar; una ruta que ya no existe aparece como pestaña con `missing: true`.

## Comandos de pestaña (todos con `tabId`)

| Comando | Argumentos | Devuelve |
|---|---|---|
| `get_snapshot` | — | `Snapshot` (`revision`, `status`, `branches`, `remoteBranches`, `remotes`, `integration`, `conflicts`) |
| `refresh` | — | `Snapshot` o `null` si se omitió |
| `stage` / `unstage` | `paths: GitPath[]` | `void` |
| `stage_hunk` / `unstage_hunk` | `path: GitPath, hunk: DiffHunk` | `void` |
| `discard` | `paths: GitPath[]` | `{ done: boolean }` (confirmación nativa) |
| `commit` | `message, allowDetached` | `{ done: boolean, sha: string \| null }` (si HEAD está detached y `allowDetached` es falso, Rust pregunta) |
| `amend` | `message` | `string` (sha) |
| `amend_draft` | — | `{ message: string, canAmend: boolean, reason: string \| null }` |
| `diff` | `scope: DiffScope, paths: GitPath[]` | `FileDiff[]` |
| `log` | `limit, query: string \| null` | `Commit[]` (con `query`, busca en todo el historial) |
| `create_branch` | `name, startPoint: string \| null` | `void` |
| `rename_branch` | `old, new` | `void` |
| `checkout` | `reference` | `{ done: boolean }` (Rust gestiona la divergencia con diálogo) |
| `delete_branch` | `name, force` | `void` |
| `merge` | `branch, fastForward: "allow" \| "only" \| "never"` | `MergeResult` |
| `continue_integration` | — | `void` |
| `abort_integration` | — | `{ done: boolean }` (confirmación nativa) |
| `get_conflicts` | — | `ConflictFile[]` |
| `write_conflict_resolution` | `path: GitPath, contents: number[]` | `{ done: boolean }` (confirmación nativa; escribe bytes, no hace `git add`) |
| `mark_resolved` | `path: GitPath` | `void` |
| `fetch` | `remote` | `void` |
| `pull` | `remote, refspec: string \| null, rebase` | `MergeResult` |
| `push` | `remote, refspec: string \| null, setUpstream` | `void` |

Tras cualquier mutación con éxito, Rust ya ha programado un refresco: la UI no tiene que pedirlo, recibirá `snapshot`.

## Comandos de IA (spec §5)

La IA **solo propone**: nunca hace commit ni toca el repo. La UI **nunca** lee el diff para la IA: lo lee Rust (`staged`), lo filtra y lo envía al CLI del proveedor.

| Comando | Argumentos | Devuelve |
|---|---|---|
| `ai_state` | `tabId` | `AiState { enabled: boolean, profile: { name, kind: "personal" \| "empresa" }, choice: "auto" \| "claude" \| "codex" \| "copilot", providers: ProviderState[] }` con `ProviderState { id, displayName, availability: "available" \| "missing" \| "failed", version: string \| null, hint: string \| null, enabled: boolean, consented: boolean, loginCommand: string }` |
| `ai_update_settings` | `patch: { enabled?: boolean, choice?: string, profile?: "personal" \| "empresa", providerEnabled?: { id: string, enabled: boolean }, binaryPath?: { id: string, path: string \| null } }` | `AiState` (se guarda en `ai-settings.json`, escritura atómica; ajuste global, no por pestaña) |
| `ai_preview_context` | `tabId` | `ContextPreview { provider: string \| null, bytes: number, maxBytes: number, files: { path: GitPath, bytes: number }[], excluded: { path: GitPath, reason: "sensitive" \| "binary" \| "too-large" }[], redactions: number, recentSubjects: number }` (lo que **se enviaría**, sin enviarlo) |
| `ai_generate_commit_message` | `tabId, options: { template: string \| null, language: "auto" \| "es" \| "en", style: "plain" \| "conventional" }` | `AiCommitSuggestion { subject: string, body: string, provider: string, commandSummary: string, repoRevision: number }` |
| `ai_cancel` | `tabId` | `boolean` |

Reglas:
- Genera el mensaje a partir del **diff preparado (staged)**. Sin nada preparado: `AI_NO_STAGED_CHANGES`.
- Primer uso por perfil y proveedor: Rust abre un **diálogo nativo** de consentimiento (qué se envía, a quién, tamaño). Si el usuario rechaza: `AI_CONSENT_REQUIRED`. El consentimiento se guarda por perfil y proveedor.
- `language: "auto"` imita el idioma y el estilo de los últimos commits del repo (asuntos de los últimos 20, incluidos en la vista previa).
- Se excluyen del contexto `.env*`, claves, certificados y binarios, y se redactan secretos (spec §5.1). Si **todos** los archivos preparados son sensibles: `AI_SENSITIVE_FILE`.
- `AI_NOT_LOGGED_IN` y `AI_NOT_INSTALLED` llevan en `detail` el comando que hay que ejecutar (`claude`, `codex login`, `copilot login`).
- La sugerencia lleva `repoRevision`: si el repo cambió mientras la IA pensaba, la UI debe avisar de que puede estar obsoleta.
- El resultado **rellena** los campos de la UI; el usuario siempre revisa y pulsa «Crear commit».

## Ventana

- Windows y Linux: ventana **sin decoraciones** del sistema; la UI dibuja los controles (minimizar, maximizar/restaurar, cerrar) con `@tauri-apps/api/window` y una región arrastrable (`data-tauri-drag-region`).
- macOS: barra de título integrada (`titleBarStyle: "Overlay"`, `hiddenTitle`); la UI deja hueco a la izquierda para los semáforos.
- La configuración es por plataforma (`tauri.windows.conf.json`, `tauri.linux.conf.json`, `tauri.macos.conf.json`) y las capabilities de ventana se limitan a `core:window:allow-minimize|toggle-maximize|close|start-dragging|is-maximized`.

## Eventos (Rust → UI)

| Evento | Carga |
|---|---|
| `snapshot` | `{ tabId, snapshot: Snapshot }` |
| `tab-updated` | `TabInfo` (rama, nº de cambios, ocupado…) |
| `session-event` | `SessionEvent` de `ariadna-git` (`sessionId`, `revision`, `kind`) |
| `tabs-changed` | `AppState` (abrir, cerrar, reordenar, restaurar) |
| `clone-progress` | `{ operationId, phase: string, percent: number \| null }` |

## Verificación del contrato

- `src-tauri` genera **fixtures JSON reales** (salida del motor sobre repos git temporales) en `ui/src/lib/api/fixtures/` mediante un test, y falla si cambian sin regenerarlos.
- `ui` tiene tests (Vitest + `svelte-check`) que cargan esos fixtures y los comprueban contra `types.ts`, de modo que un cambio en los modelos de Rust rompe la compilación de la UI.
- La UI trae un **backend simulado** (`mockApi`) con la misma interfaz `Api`, para desarrollar y previsualizar sin Tauri y para los tests de componentes.
