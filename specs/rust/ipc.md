# Contrato IPC (Tauri ↔ interfaz)

Spec §3.3. **El estado vive en Rust**; la interfaz es una vista. Este documento fija nombres y formas para que el backend (`src-tauri`) y la UI (`ui/`) se construyan por separado.

## Reglas generales

- Los comandos son funciones `#[tauri::command]` en `snake_case`; la UI los llama con `invoke("nombre", { argumentos en camelCase })`.
- Las cargas son los tipos de `crates/ariadna-git` (`models.rs`) serializados con `serde` en **camelCase**, tal cual. No se reinventan tipos en el medio.
- `GitPath` viaja como **cadena** si es UTF-8 y como **lista de bytes** (`number[]`) si no lo es. La UI debe aceptar ambas (`type GitPath = string | number[]`) y devolverlas sin tocar.
- Toda llamada de pestaña lleva `tabId` (el `session_id` de Rust). Nunca hay estado global implícito.
- Las rutas del sistema (carpeta de repo) viajan como `string`.
- **Errores**: un comando fallido rechaza la promesa con un objeto `ApiError { code, message, detail? }`. `code` es estable (`NOT_A_REPOSITORY`, `GIT_NOT_FOUND`, `COMMAND_FAILED`, `NOTHING_TO_COMMIT`, `EMPTY_COMMIT_MESSAGE`, `DETACHED_HEAD_COMMIT`, `CANNOT_AMEND`, `LOCAL_CHANGES_BLOCK_CHECKOUT`, `DIVERGED_LOCAL_BRANCH`, `INVALID_REFERENCE`, `HUNK_NOT_FOUND`, `NO_INTEGRATION`, `CONFIRMATION_REQUIRED`, `PATH_ESCAPES_REPOSITORY`, `TIMEOUT`, `CANCELLED`, `PARSE`, `LOCKED`, `DUBIOUS_OWNERSHIP`, `IO`, `REPO_MISSING`, `AUTH`, `AI_DISABLED`, `AI_NOT_INSTALLED`, `AI_NOT_LOGGED_IN`, `AI_QUOTA`, `AI_BAD_OUTPUT`, `AI_CONTEXT_TOO_LARGE`, `AI_TIMEOUT`, `AI_CONSENT_REQUIRED`, `AI_NO_STAGED_CHANGES`, `AI_SENSITIVE_FILE`, `AI_NO_BRANCH_CHANGES`, `AI_BASE_NOT_FOUND`, `MERGE_CONFLICTS`, `CONFLICTS_PENDING`, `CONFLICT_MARKERS_REMAIN`, `CONFLICT_NOT_TEXT`, `CONFLICT_NOT_FOUND`, `NOT_FOUND`, `POLICY`, `CONFLICT`, `RATE_LIMITED`, `NETWORK`). `message` ya está en español y listo para mostrar. La UI **nunca** interpreta `message`.
- **Confirmaciones destructivas**: la UI **no** envía ningún "confirmed". Los comandos destructivos abren un **diálogo nativo desde Rust** que nombra repo, rama y ruta, y devuelven `{ done: false }` si el usuario cancela. Lo hacen `discard`, `write_conflict_resolution`, `resolve_conflict`, `abort_integration`, `merge_abort`, `checkout` cuando hay divergencia, `commit` en HEAD detached y cerrar una pestaña con trabajo pendiente.
- **Revisión**: cada `Snapshot` lleva `revision` monótona. La UI descarta un snapshot con revisión menor **o igual** que la que ya tiene. Un cambio externo (editor, `git` en otra terminal) detectado por la vigilancia sube la revisión en 1; un refresco que no encuentra cambios **no publica nada** (ni `snapshot` ni `SnapshotPublished`), así que los ecos de nuestras propias mutaciones no provocan cascadas.
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
| `clone_repository` | `url, destination: string, branch: string \| null, username: string \| null, secret: string \| null` | `OpenResult` (clona y abre una pestaña; emite `clone-progress`; se cancela con `cancel_operation` pasando el `operationId` del evento y un `tabId` vacío). `secret` es un **PAT de un solo uso**, ver «Clonar y PAT» |
| `store_credential` | `url, username, secret` | `void` (acción **explícita** de guardar: entrega el secreto a `git credential approve` por stdin; **nunca** vuelve a la UI; la UI de clonado no la usa) |
| `test_credential` | `url` | `{ ok: boolean, code: string \| null }` (`git ls-remote`) |
| `forget_credential` | `url, username` | `void` (`git credential reject`) |
| `pull_request_support` | `tabId` | `PrSupport { supported, remote, provider, providerLabel, host, repoLabel, capabilities: PrCapabilities \| null, reason }` (sin red; ver «Pull requests (M8)») |
| `list_pull_requests` | `tabId, filter: "open" \| "mine" \| "reviewing" \| "closed", page` (desde 1) | `PullRequestPage { items: PullRequestSummary[], page, hasMore }` |
| `get_pull_request` | `tabId, id` | `PullRequestDetail` (descripción en Markdown, revisores, políticas, work items, commits, archivos) |
| `create_pull_request` | `tabId, draft: NewPullRequest` | `PullRequestDetail` |
| `pull_request_template` | `tabId` | `string \| null` (plantilla de descripción del repo, leída por Rust) |
| `pull_request_diff` | `tabId, id` | `PrDiff { files: PrFileChange[], diffs: FileDiff[], omitted: { path, reason }[] }` |
| `pull_request_threads` | `tabId, id` | `ReviewThread[]` |
| `add_pull_request_comment` | `tabId, id, thread: { kind: "new", body, anchor? } \| { kind: "reply", threadId, body }` | `ReviewThread` |
| `resolve_pull_request_thread` | `tabId, id, threadId, resolved` | `ReviewThread` |
| `vote_pull_request` | `tabId, id, vote, comment?: string \| null` | `PrAction { done, detail }` (**confirmación nativa** si el voto es de rechazo: `reject` o `waitForAuthor`) |
| `complete_pull_request` | `tabId, id, options: CompleteOptions` | `PrAction` (**confirmación nativa**; las políticas del proveedor mandan) |
| `abandon_pull_request` | `tabId, id` | `PrAction` (**confirmación nativa**; en GitHub es «cerrar») |
| `reopen_pull_request` | `tabId, id` | `PrAction` (sin confirmación) |
| `checkout_pull_request` | `tabId, id` | `{ done: boolean }` — **pendiente** (siguiente tarea: bajar la rama) |
| `open_remote_url` | `tabId, remote, kind: "repo" \| "branch" \| "pull-request", branch: string \| null` | `void` (abre el navegador del sistema; ver «Remotos»: la URL la construye y valida Rust; solo `https` y solo el host del remoto) |
| `list_remotes` | `tabId` | `RemoteView[]` (ver «Remotos») |
| `add_remote` | `tabId, name, url` | `{ done: boolean }` (confirmación nativa) |
| `rename_remote` | `tabId, old, new` | `{ done: boolean }` (confirmación nativa) |
| `remove_remote` | `tabId, name` | `{ done: boolean }` (confirmación nativa) |

`TabInfo { tabId, path, name, branch: string \| null, detached: boolean, changeCount: number, conflictCount: number, busy: string \| null, missing: boolean }`. `GitInfo { path, version }`. `OpenResult { tab, alreadyOpen, warning?: string }`: `warning` (ausente si no hay nada que avisar) sale al pasar del **tope blando de 20 pestañas** («Tienes 21 pestañas abiertas. Más de 20 puede ir lento: cierra las que no uses.») o al abrir un worktree de un repo ya abierto; la interfaz lo muestra como aviso y **no impide** abrir.

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
| `log_page` | `skip, limit (≤ 500), query: string \| null` | `Commit[]` (página del historial de HEAD con `git log --skip`; con `query` busca por mensaje, autor o hash y pagina el resultado; cada `Commit` lleva `parents` y `refs` para pintar el grafo) |
| `create_branch` | `name, startPoint: string \| null` | `void` |
| `rename_branch` | `old, new` | `void` |
| `checkout` | `reference` | `{ done: boolean }` (Rust gestiona la divergencia con diálogo) |
| `delete_branch` | `name, force` | `{ done: boolean }` (confirmación nativa que nombra repo, rama y ruta; si git la considera **no fusionada** pide una segunda confirmación y reintenta con `-D`) |
| `merge` | `branch, fastForward: "allow" \| "only" \| "never"` | `MergeResult` (si hay conflictos deja el merge en curso; ver `merge_start`) |
| `merge_clean` | `branch, fastForward` | `MergeResult` sin conflictos; si los hay **aborta el merge** (el repo queda como estaba, sin diálogo) y rechaza con `MERGE_CONFLICTS` (`detail`: rutas, una por línea) |
| `continue_integration` | — | `void` |
| `abort_integration` | — | `{ done: boolean }` (confirmación nativa) |
| `get_conflicts` | — | `ConflictFile[]` |
| `write_conflict_resolution` | `path: GitPath, contents: number[]` | `{ done: boolean }` (confirmación nativa; escribe bytes, no hace `git add`) |
| `mark_resolved` | `path: GitPath` | `void` |
| `merge_start` | `branch, fastForward` | `MergeStartResult { result: MergeResult, state: MergeState \| null }`. Como `merge`, pero con conflictos **deja el merge en curso** y devuelve ya el `MergeState` para abrir el estudio de conflictos. `merge_clean` sigue existiendo (aborta si hay conflictos) |
| `merge_state` | — | `MergeState { integration, otherHead: string \| null, sourceLabel: string \| null, message: string, conflicts: ConflictSummary[], canContinue: boolean }`. Sirve para merge, cherry-pick y rebase; `message` es `MERGE_MSG` sin comentarios (mensaje por defecto editable); `otherHead` es `MERGE_HEAD`/`CHERRY_PICK_HEAD`. `ConflictSummary { path, kind, binary, hunkCount, textSafe, oursPresent, basePresent, theirsPresent }` (`kind` = UU/AA/DD/AU/UA/DU/UD como `bothModified`…) |
| `resolve_conflict` | `path: GitPath, resolution` | `{ done: boolean }`. `resolution` = `{ side: "ours" \| "theirs" \| "base" }` (versión entera del índice `:2`/`:3`/`:1`; si ese lado no existe —borrado— **borra** el archivo), `{ hunks: [{ id, choice: "ours" \| "theirs" \| "base" \| "both" }] }` (debe cubrir todos los bloques) o `{ manual: number[] }` (bytes del texto final). **Confirmación nativa antes de tocar nada**; luego escribe (atómico, bytes tal cual) o borra y hace `git add` en una sola operación. Errores: `CONFLICT_NOT_FOUND` (el archivo ya no está en conflicto, o pide `base` sin base), `CONFLICT_NOT_TEXT` (binario o no UTF-8 con `hunks`/`manual`), `CONFLICT_MARKERS_REMAIN` (el resultado conserva `<<<<<<<`, o falta elegir algún bloque, que se nombra), `HUNK_NOT_FOUND` (un `id` repetido o que no existe) — todos antes de preguntar |
| `merge_continue` | `message: string \| null` | `ContinueResult { done, sha: string \| null, integration }`. Falla con `CONFLICTS_PENDING` (`detail`: rutas) si quedan sin resolver. Merge con `message` → commit con ese mensaje; sin él, el de git. Cherry-pick/rebase: continúa (`message` se ignora) y `integration` indica si hay un nuevo paso en curso |
| `merge_abort` | — | `{ done: boolean }` (confirmación nativa). Solo merge: con rebase/cherry-pick responde `NO_INTEGRATION`; para esos, `abort_integration` |
| `fetch` | `remote` | `void` |
| `pull` | `remote, refspec: string \| null, rebase?: boolean \| null` | `MergeResult` (sin `rebase` usa el ajuste `pullRebase`) |
| `push` | `remote, refspec: string \| null, setUpstream` | `void` |

### Estudio de conflictos

- **Tres vías**: las versiones son las etapas del índice (`get_conflicts` y `Snapshot.conflicts`: `ours` = `:2`, `base` = `:1`, `theirs` = `:3`, más `workingTree` con marcadores y `hunks`). Los contenidos de texto viajan como cadena (con pérdida si no es UTF-8, de ahí `ConflictSummary.textSafe`); los binarios no llevan contenido.
- **Nada se escribe hasta confirmar**: la UI edita el resultado en memoria; `resolve_conflict` pregunta (diálogo nativo con repo, rama y ruta) y entonces escribe/borra + `git add`. Cancelar deja el archivo intacto.
- **Especiales**: borrado-vs-modificado (`deletedByUs`/`deletedByThem`) y añadido por un solo lado se resuelven con `side` (aceptar el borrado = borrar + `git add`); binarios y no UTF-8 solo admiten `side`; `bothDeleted` (DD) también.
- **Fin de línea y BOM**: el motor trabaja con bytes. La UI restituye el EOL dominante del archivo al editar (el `textarea` normaliza a LF); un archivo con EOL mezclados se normaliza al dominante al editarlo a mano (con `side`/`hunks` en Rust se conserva byte a byte).
- **UI**: el panel «Conflictos» (cuarta pestaña del workspace) solo existe con `integration != none`; banner con contador en la pestaña; atajos de ámbito `editor`: `Alt+↓/↑` (siguiente/anterior bloque), `Alt+1/2/3` (nuestro/suyo/ambos). «Marcar como resuelto» está deshabilitado mientras queden marcadores. Abortar con ediciones sin aplicar avisa antes en la UI y luego Rust confirma.

Para ver los archivos y el diff de un commit se usa `diff` con `scope: { commit: sha }` (primer padre en merges; los commits raíz también). El historial de la UI es una lista virtualizada que pide `log_page` al acercarse al final; el grafo de ramas se calcula en la UI a partir de `parents` (algoritmo incremental, `ui/src/lib/util/graph.ts`).

### Remotos y «Abrir en el navegador» (D19)

Sección «Remotos» al principio del panel **Ramas** (`RemotesSection`) y botones «Abrir rama en el navegador» / «Crear pull request» en el detalle de rama (`BranchDetail`). `fetch`, `pull` y `push` son los comandos de pestaña de siempre; la UI los usa sobre la rama actual (`pull` con refspec si la rama no sigue a ese remoto; `push` con `--set-upstream` si todavía no tiene origen).

`RemoteView { name, fetchUrl, pushUrl, provider: "azureDevOps" \| "github" \| "gitlab" \| "bitbucket" \| "other", providerLabel, host, canOpen, canCreatePullRequest, ssh }`. Lo calcula Rust con `ariadna-hosting` (`RemoteInfo::parse`) sobre el snapshot (las credenciales de la URL ya vienen redactadas); `host` es el normalizado (`dev.azure.com` para toda forma de Azure DevOps) y vacío en remotos locales. `canOpen` = hay página web del repo y su host pasa la política; `canCreatePullRequest` = además el proveedor tiene página de «nueva PR» (GitHub, Azure DevOps, GitLab, Bitbucket; no `other`).

- **Añadir / renombrar / quitar**: diálogo **nativo desde Rust** (nombra repo, remoto y URL; «quitar» cuenta las ramas de seguimiento que se borran y las ramas locales que pierden su origen y avisa de que no se toca el servidor); `{ done: false }` si se cancela. Se valida **antes** de preguntar: nombre de remoto válido y existente, y para `add_remote` la URL no puede estar vacía, empezar por `-`, llevar caracteres de control, usar transportes auxiliares `algo::…` (p. ej. `ext::`) ni llevar **contraseña o token** (`https://usuario:secreto@…`, también con `%3A`, o un usuario que parece un token: prefijos `ghp_`, `github_pat_`, `glpat-`… o 32 caracteres o más sin separadores; un usuario normal sí vale) — se rechazan con `INVALID_REFERENCE` y sin repetir el secreto en el mensaje. `git remote rename` actualiza ramas de seguimiento y `branch.<x>.remote`; `remove` borra las ramas de seguimiento.
- **`open_remote_url`**: la UI envía solo `remote` y `branch`. Rust busca el remoto en la configuración real del repo, valida la rama (nombre de rama de git válido; en `branch` y `pull-request` es obligatoria) y construye la URL con `RemoteInfo` (`repo_web_url`, `branch_web_url`, `new_pull_request_url`: GitHub `/tree/{rama}` y `/pull/new/{rama}`; Azure DevOps `?version=GB{rama}` y `/pullrequestcreate?sourceRef={rama}`; GitLab `/-/tree/{rama}` y `/-/merge_requests/new?…`; Bitbucket `/branch/{rama}` y `/pull-requests/new?source=…`; la rama va codificada). Después `validate_remote_web_url`: **solo `https`**, sin credenciales, puerto estándar, nombre de dominio (nada de IP ni `localhost`) y **host idéntico al del remoto**. Sin página conocida (remoto local, proveedor `other` en PR, sin dueño en la ruta…) rechaza con `POLICY`. Solo es un enlace: la integración por API es M8. El puerto de una URL https no se conserva (no se distingue del de SSH): el enlace va al puerto estándar del mismo host.
- **Capability del plugin `opener`**: la ventana **no tiene ningún permiso `opener:*`**. Abrir el navegador lo hacen únicamente `open_external_url` (lista cerrada de hosts de ayuda) y `open_remote_url` (host del remoto) desde Rust, que validan antes de llamar al plugin; la interfaz no puede abrir URLs por su cuenta. Un test (`window_config`) lo comprueba.

### Clonar y PAT (D20)

El PAT es **de un solo uso** y no se guarda en ningún sitio:

- Viaja de la UI a Rust una sola vez (`secret`, tipo `Secret`: sin `Clone`/`Serialize`, `Debug` redactado, `zeroize` al soltarlo). La UI lo borra del formulario en cuanto pulsa «Clonar», haya éxito o error.
- Rust lo pasa a git **solo por `GIT_ASKPASS`**: un script temporal en una carpeta privada (`0700`, sin secretos dentro) imprime el usuario o el secreto leyendo dos variables de entorno que existen **solo en el proceso hijo** de git. Nunca va en `argv`, ni en la URL (las contraseñas incrustadas se rechazan), ni en `.git/config`, ni en un fichero, ni en un registro.
- Se ejecuta con `-c credential.helper=` (lista vacía): ni el Git Credential Manager ni el llavero reciben `credential approve`, así que nada lo persiste. La carpeta temporal se borra al terminar (también al fallar o cancelar).
- Todo texto de error se limpia (`scrub_error`: el secreto y las credenciales de URL se sustituyen por `***`) antes de salir del motor.
- Errores: URL inválida (`INVALID_REFERENCE`), destino ocupado/relativo (`IO`), 401/403 (`AUTH`, con pista de qué hacer), cancelación (`CANCELLED`, borra solo lo creado).
- `clone-progress` lleva `phase` ya en español y `percent`; el primer evento (fase «Preparando el clonado») trae el `operationId` para cancelar.

Tras cualquier mutación con éxito, Rust ya ha programado un refresco: la UI no tiene que pedirlo, recibirá `snapshot`.

### Pull requests (M8)

Spec §4.4, D21/D22. Crate `ariadna-hosting` (`trait PullRequestProvider` con Azure DevOps y GitHub); el núcleo (`core/pr_ops.rs`) solo elige el remoto y confirma. **Estado: probado contra un servidor simulado; los endpoints reales NO están verificados** (`specs/rust/spike-pr.md`, con el checklist para el spike con PAT reales).

- **Proveedor y remoto**: lo decide Rust, nunca la UI. Se usa `origin` si su proveedor admite PR (Azure DevOps o GitHub, incluidos GitHub Enterprise por host, `{org}.visualstudio.com` y SSH → HTTPS); si no, el primer remoto que las admita. GitLab, Bitbucket y `other`: `POLICY` («se abren en el navegador») y `pull_request_support.supported = false`.
- **Sin secretos en el IPC**: ningún comando recibe, devuelve ni guarda un token. En **cada** llamada Rust obtiene la credencial con `git credential fill` (stdin/stdout, `GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never`, entorno saneado), la usa en memoria (`zeroize`) y la suelta. Nunca aparece en `argv`, URL, registro, `Debug` ni mensaje de error (los textos del servidor se redactan). Si el helper no tiene token: `AUTH` y la UI abre el formulario de credenciales de «Clonar y PAT».
- **Red**: solo cuando la UI llama (sin sondeo; el refresco manual o a intervalo ≥ 60 s con el panel abierto lo pide la UI). TLS verificado, solo el origen del proveedor (redirecciones y enlaces `next` a otros orígenes no se siguen), timeout 30 s, respuesta ≤ 8 MiB, reintentos acotados (429 con `Retry-After` ≤ 10 s; 502/503/504 y red solo en lecturas).
- **Errores** (`ApiError`): `AUTH` (`detail` = permiso que falta si el servidor lo indica, p. ej. `pull_requests=write`), `NOT_FOUND` (en GitHub también es lo que ve un token sin acceso), `POLICY` (política o regla incumplida, o acción no permitida: el motivo del proveedor va en `message`), `CONFLICT` (409 / estado obsoleto / PR duplicada), `RATE_LIMITED` (`detail` = segundos de espera si se conocen), `NETWORK` (sin conexión, TLS, tiempo agotado, 5xx), `INVALID_REFERENCE` (petición que el proveedor no admite: voto, estrategia, hilo), `PARSE` (respuesta mal formada o por encima del tope), `IO`. `message` ya está en español.
- **Formas** (camelCase, `ui/src/lib/api/prTypes.ts`; fixtures reales de Rust en `ui/src/lib/api/fixtures-pr/`, comprobados por `pr.contract.test.ts`):
  - `PullRequestSummary { id, title, author: PrUser, sourceBranch, targetBranch, state: "active" | "completed" | "abandoned", isDraft, createdAt, updatedAt, reviewers: { user, vote, required }[], checks: { passing, failing, pending } | null, mergeStatus: "clean" | "conflicts" | "unknown", webUrl }`. `vote`: `approve`, `approveWithSuggestions`, `noVote`, `waitForAuthor`, `reject`, `comment`.
  - `PullRequestDetail` = `PullRequestSummary` + `{ description, policies: { name, status: "passing" | "failing" | "pending" | "notApplicable" | "broken", blocking: boolean | null, url }[], workItems: { id, url }[], commits: { sha, subject, author, date }[], files: PrFileChange[], sourceCommit, targetCommit, checkoutRef, isFork, autoComplete, mergeDetail }`. `description` y los comentarios son **Markdown**: se renderizan desde el AST, nunca con `{@html}`.
  - `NewPullRequest { title, description, sourceBranch, targetBranch, draft, reviewers: string[], workItemIds: number[] }` (revisores: GUID/correo en Azure DevOps, login en GitHub; work items solo Azure DevOps).
  - `ReviewThread { id, status: "active" | "resolved" | "closed", anchor: { path, line, side: "old" | "new" } | null, outdated, comments: { id, author, body, createdAt }[], canResolve }`. En GitHub los comentarios generales tienen `id` `issue:{n}`, no se anidan ni se resuelven (`canResolve: false`).
  - `CompleteOptions { strategy: "merge" | "squash" | "rebase" | "rebaseMerge", deleteSourceBranch, completeWorkItems, autoComplete, commitMessage: string | null }`.
  - `PrCapabilities { provider, votes, strategies, autoComplete, resolveThreads, workItems, draft }`: la UI ofrece solo lo que el proveedor admite (Azure DevOps: 5 votos, 4 estrategias, auto-complete y work items; GitHub: `approve`/`reject` (solicitar cambios)/`comment`, `merge`/`squash`/`rebase`).
  - `PrAction { done, detail }`: `done: false` si el usuario cancela la confirmación nativa; con `done: true`, `detail` es el estado actualizado.
- **Confirmaciones nativas** (desde Rust; la UI no manda ningún «confirmed»): abandonar/cerrar, completar (también al activar *auto-complete*) y votar `reject`/`waitForAuthor`. El diálogo nombra repo (`host/propietario/proyecto/repo`), carpeta, PR (#id y título), rama de origen y **rama destino**, y la estrategia y opciones al completar. Todo se valida **antes** de preguntar (voto o estrategia no soportados → `INVALID_REFERENCE`; PR ya no abierta → `POLICY`). Crear, comentar, resolver hilos, aprobar y reabrir no preguntan.
- **Diff**: `pull_request_diff` convierte los parches del proveedor al mismo `FileDiff` del visor (Azure DevOps lo calcula pidiendo las dos versiones de cada archivo; GitHub usa el `patch` de `pulls/{n}/files`). Los archivos sin diff (binarios, enormes, solo renombrados) van en `omitted` con el motivo.
- **Plantilla**: `pull_request_template` busca `pull_request_template.md` (sin distinguir mayúsculas) en `.azuredevops/`, la raíz y `docs/` (Azure DevOps) o `.github/`, la raíz y `docs/` (GitHub); solo ficheros regulares ≤ 64 KiB.
- **Pendiente (siguiente tarea)**: UI del panel de PR, `checkout_pull_request` (bajar la rama, incluido `refs/pull/N/head`), push previo si la rama no está publicada y «Generar título y descripción con IA».
- **Backend simulado de la UI** (`mockPr.ts`): mismos comandos, formas y errores con datos en memoria; escenarios `?scenario=pr-auth`, `pr-rate-limit`, `pr-network`, `pr-empty`.

## Ajustes (`settings.json`)

Ajustes de la aplicación, **sin ningún secreto** (Ariadna no guarda tokens ni contraseñas). Pantalla: diálogo `SettingsDialog` (barra de título, `Mod+,` y paleta) con General, Git, Inteligencia artificial, Cuentas y credenciales y Acerca de. Los ajustes de IA (activar, perfil, proveedor, habilitar por perfil) siguen en `ai-settings.json` y se aplican al momento con `ai_update_settings`; el resto se edita como borrador y se guarda de una vez con `save_settings`.

| Comando | Argumentos | Devuelve |
|---|---|---|
| `get_settings` | – | `SettingsView { settings: Settings, load: { status: "ok" \| "missing" \| "migrated" \| "resetCorrupt", warning: string \| null }, about: { version, license: "MIT", repository, issues }, git: GitInfo \| null, gitProblem: string \| null }` |
| `save_settings` | `settings: Settings` | `SettingsView` (valida, guarda de forma atómica y aplica; ver errores) |
| `detect_git` | `path: string \| null` | `GitDetect { ok, source: "configured" \| "auto", path: string \| null, version: string \| null, problem: string \| null }`. `null` = búsqueda automática (PATH y ubicaciones conocidas); una ruta se valida **sola** (sin buscar otro git en silencio). No guarda nada |
| `git_identity` | – | `{ name: string \| null, email: string \| null }` (`user.name`/`user.email` de la config de git; solo lectura) |
| `get_cli_status` | – | `CliStatus[]` (orden Claude, Codex, Copilot): `{ id, displayName, installed, availability: "available" \| "missing" \| "failed", version, path, hint, loginCommand }`. Ejecuta `--version` de cada CLI con el resolutor de `ariadna-ai` (rutas manuales, shims `.cmd`/`node.exe`) |

`Settings` (camelCase): `{ version: 1, language: "es", theme: "system" \| "light" \| "dark", diffFontSize: 10..=24, gitPath: string \| null, pullRebase: boolean, profileRules: { pattern: string, profile: "personal" \| "empresa" }[] }`.

- **Archivo**: `settings.json` en la carpeta de datos. Escritura atómica (tmp + `sync_all` + rename). Versionado: sin campo `version` (o anterior) se migra al esquema actual y se reescribe; una versión **futura** se trata como no legible. Los campos desconocidos se toleran.
- **Archivo corrupto** (JSON roto, tipos o valores inválidos, versión futura, ilegible): **no impide arrancar**. Se renombra a `settings.json.bak`, se usan los valores por defecto y `get_settings.load = { status: "resetCorrupt", warning }`; la UI muestra el aviso (banner en Ajustes y aviso al arrancar). El aviso vale para esa ejecución.
- **Validación estricta** (errores `PARSE` con el campo en `detail`): `language` solo `es`; `diffFontSize` entero 10–24; `gitPath` absoluta, ≤ 1024 caracteres, sin NUL (vacía = automática); `profileRules` ≤ 50, patrón normalizado (minúsculas, sin `/` final) con forma `host[/propietario[/proyecto[/repo]]]`, solo `[a-z0-9._-~/]`, sin esquema ni usuario, host con punto, sin segmentos vacíos, **sin duplicados**. Nada se guarda si algo falla.
- **Ruta de git**: al cambiar `gitPath`, `save_settings` la valida con el localizador (versión mínima incluida) y rechaza con `GIT_NOT_FOUND` si no sirve. Si es válida, el motor usa ese git en las pestañas que se abran **desde ese momento** (las abiertas conservan el anterior hasta reabrirlas) y se emite `tabs-changed` con el nuevo `git`/`gitProblem`. Vacía = detección automática. Al arrancar manda la ruta guardada; si ya no sirve se usa la detectada y `load.warning` lo dice.
- **`pull`**: el argumento `rebase` es ahora **opcional** (`null`/omitido = ajuste `pullRebase`); si se envía, manda sobre el ajuste.
- **Reglas repo → perfil (D16)**: al generar con IA (`ai_state`, `ai_preview_context`, `ai_generate_commit_message`) Rust toma el remoto de la pestaña (`origin`, o el primero), lo normaliza con `ariadna-hosting` (Azure DevOps y sus formas SSH → `dev.azure.com`) y compara **por segmentos**; gana la regla más específica. Si hay coincidencia, el perfil de la regla manda sobre el perfil elegido en Ajustes y `AiState.profile.rule` lleva el patrón (ausente si no hay regla). El **consentimiento se guarda en el perfil de la regla**, no en el «activo». Reglas por defecto: `dev.azure.com` → Empresa, `github.com` → Personal. Empresa tiene Claude desactivado por defecto.
- `ai_update_settings` admite `tabId?` para que el `AiState` devuelto aplique la regla de esa pestaña; con un parche vacío es la lectura de IA que no necesita pestaña (la usa Ajustes).
- Tema y tamaño del diff: la UI los aplica al instante, los guarda en `settings.json` y arranca con lo que diga Rust (copia en `localStorage` solo para el primer pintado).

## Comandos de IA (spec §5)

La IA **solo propone**: nunca hace commit ni toca el repo. La UI **nunca** lee el diff para la IA: lo lee Rust (`staged`), lo filtra y lo envía al CLI del proveedor.

| Comando | Argumentos | Devuelve |
|---|---|---|
| `ai_state` | `tabId` | `AiState { enabled: boolean, profile: { name, kind: "personal" \| "empresa", isolated: boolean, rule?: string }, choice: "auto" \| "claude" \| "codex" \| "copilot", providers: ProviderState[] }` con `ProviderState { id, displayName, availability: "available" \| "missing" \| "failed", version: string \| null, hint: string \| null, enabled: boolean, consented: boolean, loginCommand: string, sharesLogin: boolean, isolated: boolean, isolationVerified: boolean }` |
| `ai_update_settings` | `patch: { tabId?: string, enabled?: boolean, choice?: string, profile?: "personal" \| "empresa", providerEnabled?: { id: string, enabled: boolean }, binaryPath?: { id: string, path: string \| null }, isolated?: boolean }` | `AiState` (se guarda en `ai-settings.json`, escritura atómica; ajuste global, no por pestaña) |
| `ai_preview_context` | `tabId` | `ContextPreview { provider: string \| null, bytes: number, maxBytes: number, files: { path: GitPath, bytes: number }[], excluded: { path: GitPath, reason: "sensitive" \| "binary" \| "too-large" }[], redactions: number, recentSubjects: number }` (lo que **se enviaría**, sin enviarlo) |
| `ai_generate_commit_message` | `tabId, options: { template: string \| null, language: "auto" \| "es" \| "en", style: "plain" \| "conventional" }` | `AiCommitSuggestion { subject: string, body: string, provider: string, commandSummary: string, repoRevision: number }` |
| `ai_cancel` | `tabId` | `boolean` |
| `ai_accounts` | `request?: { tabId?: string, profile?: "personal" \| "empresa" }` | `AiAccounts { profile: { name, kind, isolated, rule? }, accounts: AiAccount[] }` (orden Claude, Codex, Copilot) con `AiAccount { provider, state: "loggedIn" \| "loggedOut" \| "unavailable", email, organization, plan, authMethod, configDir, reason }` (todo `string \| null`) |
| `ai_login` | `request: { tabId?: string, profile?: "personal" \| "empresa", provider: string, launch?: boolean }` | `AiLoginInfo { provider, profile, command: string \| null, shell: "posix" \| "cmd", configDir: string \| null, isolated, sharesLogin, launched, verified, note: string \| null }` |
| `ai_generate_pull_request` | `tabId, options?: { base?: string \| null, template?: string \| null, language?: "auto" \| "es" \| "en" }` | `AiPullRequestSuggestion { title, body, provider, commandSummary, repoRevision, base, head, commitCount }` |

Reglas:
- Genera el mensaje a partir del **diff preparado (staged)**. Sin nada preparado: `AI_NO_STAGED_CHANGES`.
- Primer uso por perfil y proveedor: Rust abre un **diálogo nativo** de consentimiento (qué se envía, a quién, tamaño). Si el usuario rechaza: `AI_CONSENT_REQUIRED`. El consentimiento se guarda por perfil y proveedor.
- `language: "auto"` imita el idioma y el estilo de los últimos commits del repo (asuntos de los últimos 20, incluidos en la vista previa).
- Se excluyen del contexto `.env*`, claves, certificados y binarios, y se redactan secretos (spec §5.1). Si **todos** los archivos preparados son sensibles: `AI_SENSITIVE_FILE`.
- `AI_NOT_LOGGED_IN` y `AI_NOT_INSTALLED` llevan en `detail` el comando que hay que ejecutar (`claude auth login`, `codex login`, `copilot login`).
- La sugerencia lleva `repoRevision`: si el repo cambió mientras la IA pensaba, la UI debe avisar de que puede estar obsoleta.
- El resultado **rellena** los campos de la UI; el usuario siempre revisa y pulsa «Crear commit».

Comportamiento de `ai_generate_commit_message` (el comando que genera asunto y descripción; no existe un `propose_commit_message` aparte):
- **Contexto**: Rust lee `diff --staged` y hasta 20 asuntos recientes (`git log`); excluye sensibles, binarios y archivos que por sí solos superan el tope (`max_context_bytes`, 100 000 por defecto); redacta secretos. Si lo que queda supera el tope: `AI_CONTEXT_TOO_LARGE` sin lanzar el CLI. El prompt viaja por **stdin**, nunca por argv.
- **Convención** (en el prompt): asunto imperativo ≤ 72 caracteres; en español salvo que `language`/historial reciente indiquen otro idioma o Conventional Commits (`style`); descripción en párrafos o viñetas.
- **Orden de comprobaciones y errores**: IA desactivada → `AI_DISABLED`; nada preparado → `AI_NO_STAGED_CHANGES`; todo sensible → `AI_SENSITIVE_FILE`; demasiado grande → `AI_CONTEXT_TOO_LARGE`; sin proveedor habilitado e instalado → `AI_NOT_INSTALLED` (o el error de «ningún proveedor»); consentimiento rechazado → `AI_CONSENT_REQUIRED`; el CLI tarda más de 120 s → `AI_TIMEOUT`; salida ilegible → `AI_BAD_OUTPUT`; sin sesión → `AI_NOT_LOGGED_IN`; cuota → `AI_QUOTA`; cancelada → `CANCELLED`; otro fallo del CLI → `COMMAND_FAILED`.
- **Una sola generación por pestaña**: una nueva cancela la anterior (que termina con `CANCELLED`). `ai_cancel(tabId)` y `cancel_operation(tabId, operationId)` matan el proceso hijo. Cerrar la pestaña o salir de la app también cancela.
- **Progreso**: emite `session-event` con `OperationStarted { operationId, label: "IA: mensaje de commit" }` al empezar y `OperationFinished { ok, error }` al terminar (con `error` si falló o se canceló).
- **Nunca aplica nada**: no hace commit, no prepara ni descarta archivos. No se registran ni el prompt ni el diff.
- **Interfaz** (`CommitBar`): botón ✨ «Generar con IA» junto al asunto y atajo `Mod+Shift+G` (`ai.generate`, también en la paleta). Inactivo con tooltip si no hay nada preparado. Mientras genera: spinner, botón «Cancelar» (y `Esc`), campos en solo lectura. Si hay texto previo no se pierde: aviso «Propuesto por <proveedor>» con **Deshacer**. IA desactivada o `AI_CONSENT_REQUIRED`: aviso con acción que abre el diálogo de consentimiento y ajustes (`AiConsentDialog`: qué se envía y a quién, perfil Personal/Empresa, proveedor Auto/Claude/Codex/Copilot, activar/desactivar). El diálogo nativo de Rust sigue confirmando el primer envío por perfil y proveedor.

### Perfiles con sesión aislada, cuenta activa e inicio de sesión (M6, alpha.2)

**Aislamiento de la sesión por perfil y proveedor (D16).** Cada perfil tiene `isolated`: `true` = cada CLI que lo permita usa su propio directorio de configuración `<datos de Ariadna>/ai-profiles/<personal|empresa>/<proveedor>` (variable `CLAUDE_CONFIG_DIR` o `CODEX_HOME`, que Rust fija en el entorno del hijo; el directorio se crea al usarlo). Por defecto **Empresa aísla y Personal no**: Personal conserva la sesión normal del CLI para que nadie pierda su login al actualizar; activar `isolated` en Personal le da su propio directorio, distinto del de Empresa. `ai_update_settings({ isolated })` cambia el aislamiento del perfil activo. Lo aplican el mensaje de commit, la descripción de PR, la consulta de cuenta y el login.
- **Claude**: aislamiento **verificado** con `claude 2.1.319` (`CLAUDE_CONFIG_DIR` vacío → `claude auth status` devuelve `loggedIn: false` y `configDirectory` con ese directorio).
- **Codex**: `CODEX_HOME` según la documentación, **NO verificado** (CLI no instalado en la máquina de desarrollo). `ProviderState.isolationVerified = false` y la UI lo dice.
- **Copilot**: no hay mecanismo confirmado (`COPILOT_HOME` queda como candidata sin usar), así que **comparte login entre perfiles**: `ProviderState.sharesLogin = true`, la UI lo avisa en Ajustes y en el login, y Ariadna solo controla si se permite el proveedor, no con qué cuenta.

**`ai_accounts`** (RF-IA12) ejecuta el subcomando de estado del propio CLI con el entorno del perfil; **no lee** `.credentials.json`, llaveros ni variables. Solo Claude lo expone de forma verificada (`claude auth status --json`: `loggedIn`, `email`, `orgName`, `subscriptionType`, `authMethod`, `configDirectory`; el CLI sale con código 1 sin sesión pero imprime igualmente el JSON). Del resto solo se propagan esos campos (sin `orgId` ni nada más), sin caracteres de control y acotados a 200. Codex y Copilot devuelven `state: "unavailable"` con `reason` y **no se lanza ningún proceso**. Sin `profile`, el perfil sale de la regla repo → perfil de `tabId` y, si no, del activo. Un perfil desconocido: `PARSE`.

**`ai_login`** prepara el inicio de sesión **oficial** del CLI con el entorno del perfil; Ariadna nunca pide ni ve credenciales. Con `launch: false` solo devuelve `command` (listo para copiar: `CLAUDE_CONFIG_DIR='…' claude auth login` en POSIX, `set "CLAUDE_CONFIG_DIR=…" && claude auth login` en `cmd.exe`). Con `launch: true` escribe un script de arranque `login-<proveedor>.command|.cmd` en `ai-profiles/<perfil>/` y abre una terminal: `cmd /c start` en Windows y `open` (Terminal.app) en macOS; en otros sistemas no se abre nada. El script solo contiene constantes y la ruta del perfil (nada del repo), elimina las variables prohibidas de RF-IA3 (`ANTHROPIC_API_KEY`…) y fija el directorio aislado. Si un valor no se puede entrecomillar con seguridad (en `cmd.exe`: `"`, `%`, `^`, `!`, saltos de línea) o no se puede abrir la terminal, **no se lanza nada**: `launched = false`, `command` puede ser `null` y `note` explica el motivo para que la UI muestre el comando. Errores: proveedor no instalado → `AI_NOT_INSTALLED` (con el comando en `detail`); desactivado en el perfil → `AI_DISABLED`; proveedor o perfil desconocidos → `PARSE`. `verified = true` solo para Claude (`claude auth login`); `codex login` y `copilot login` no están verificados.

**`ai_generate_pull_request`** (backend de M8; todavía sin pantalla de PR) propone `title` y `body` (Markdown) a partir de la rama actual, sus commits y el diff **contra el ancestro común** con la base (`git merge-base`), de solo lectura. La base es `options.base` o, si falta, `origin/HEAD`, `origin/main`, `origin/master`, `main`, `master` (la primera que exista y no sea la propia rama). `base` solo admite caracteres de nombre de referencia (sin `..`, espacios ni `-` inicial): si no, `PARSE`; Rust resuelve la base a un SHA y el resto de comandos usan solo SHAs. Se envían hasta 50 commits (asunto y cuerpo, más antiguos primero) y el diff, **siempre por stdin**. Mismas garantías que el mensaje de commit: exclusión de sensibles y binarios, redacción de secretos también en mensajes de commit y nombres de rama, tope de contexto, consentimiento por perfil y proveedor (el diálogo nombra «pull request»), cola de IA, una generación por pestaña, cancelación y `session-event` (`IA: descripción de PR`). **Solo propone**: no crea la PR, no hace commit ni toca el repo. Errores añadidos: `AI_BASE_NOT_FOUND` (sin base utilizable o sin historia común) y `AI_NO_BRANCH_CHANGES` (la rama no aporta commits ni cambios); el resto como `ai_generate_commit_message` (`AI_DISABLED`, `AI_CONSENT_REQUIRED`, `AI_SENSITIVE_FILE`, `AI_CONTEXT_TOO_LARGE`…).

**Interfaz.** `ProposalSheet` es la hoja de propuesta reutilizable (modal con foco atrapado, `Esc` cierra): proveedor, perfil, cuenta (o «Cuenta no disponible»), asunto/título y descripción editables y cuatro acciones: **Copiar** (lo que se ve, incluido lo editado), **Usar** (escribe en los campos; nunca hace commit), **Regenerar** (otra propuesta sin cerrar la hoja; conserva el «antes» original) y **Descartar** (restaura el texto de antes de la IA). En `CommitBar` el aviso «Propuesto por …» gana el botón «Ver propuesta»; el flujo anterior (campos rellenos + «Deshacer») no cambia. Regenerar solo reescribe los campos si aún contenían la propuesta anterior; si el usuario los tocó, decide «Usar». En Ajustes → Inteligencia artificial: casilla «Sesión propia para este perfil», insignia por proveedor (sesión propia / normal / comparte sesión) con su aviso, la cuenta activa, el botón «Iniciar sesión» (muestra el comando con el entorno del perfil y «Abrir terminal») y «Actualizar cuentas».

## Ventana

- Windows y Linux: ventana **sin decoraciones** del sistema; la UI dibuja los controles (minimizar, maximizar/restaurar, cerrar) con `@tauri-apps/api/window` y una región arrastrable (`data-tauri-drag-region`).
- macOS: barra de título integrada (`titleBarStyle: "Overlay"`, `hiddenTitle`); la UI deja hueco a la izquierda para los semáforos.
- La configuración es por plataforma (`tauri.windows.conf.json`, `tauri.linux.conf.json`, `tauri.macos.conf.json`) y las capabilities de ventana se limitan a `core:window:allow-minimize|toggle-maximize|close|start-dragging|is-maximized` (más `core:default` y `dialog:default`; ningún `opener:*`).
- **Tamaño mínimo 980×640** (`minWidth`/`minHeight` en las cuatro configuraciones; tamaño inicial 1280×800). Lo comprueba el test `window_config`.
- **macOS**: `trafficLightPosition { x: 16, y: 13 }` sitúa los semáforos centrados en la barra de 40 px (`--titlebar-h`); la UI deja 68 px de hueco (`.mac-inset`) más el relleno de la barra. **No verificado a ojo en macOS** (solo hay CI de compilación): comprobar que los semáforos no pisan la marca ni la primera pestaña y que el arrastre funciona en el hueco.

### Menú nativo y atajos

El menú es **dato** (`core/menu.rs`, `app_menu(mac)`) y la capa Tauri lo construye: Archivo (Abrir repositorio… `⌘/Ctrl+O`, Clonar… `⌘/Ctrl+Mayús+O`, Nueva pestaña `⌘/Ctrl+T`, Cerrar pestaña `⌘/Ctrl+W`; en Windows/Linux también Ajustes… y Salir), Editar (deshacer, rehacer, cortar, copiar, pegar, seleccionar todo), Ver (Refrescar `⌘/Ctrl+R`, Paleta de comandos `⌘/Ctrl+K`, pantalla completa), Ventana (minimizar, zoom, pestaña siguiente/anterior `Ctrl+Tab`/`Ctrl+Mayús+Tab`) y Ayuda (documentación e incidencias, enlaces fijos que pasan por `validate_external_url`). macOS añade el menú «Ariadna» (Acerca de, Ajustes… `⌘,`, ocultar, Salir `⌘Q`).

- **Solo macOS adjunta el menú** (barra de menús del sistema). En Windows y Linux la barra de título es la de la interfaz y una barra nativa la duplicaría: no se adjunta y las mismas acciones están en los atajos y en la paleta. Sin verificar en una ventana real de Windows/Linux (decisión de diseño, no probada visualmente).
- **Un solo registro de atajos**: cada elemento propio lleva el **id del registro** (`ui/src/lib/shortcuts/registry.ts`) y su acelerador es el mismo combo (`Mod` → `CmdOrCtrl`). Al pulsarlo, Rust emite `menu-action { id }` y la UI lo ejecuta con `runMenuAction(id)`, que ignora el eco si la misma tecla acaba de ejecutarlo (250 ms; nunca se ejecuta dos veces). El test de Rust genera `ui/src/lib/api/fixtures/menu.json` (falla si cambia sin regenerar) y Vitest comprueba que cada acelerador coincide con el registro. Salir (`app.quit`) y los enlaces de ayuda los resuelve Rust: salir sigue el mismo camino que cerrar la ventana (confirma trabajo pendiente).
- «Mover pestaña» (`Alt+Mayús+←/→`) **no** está en el menú: en macOS un acelerador de menú impediría «seleccionar por palabras» en los campos de texto. Además el atajo se ignora cuando el foco está en un campo de texto fuera de la barra de pestañas.
- `⌘/Ctrl+R` se **consume siempre** (sin repo abierto no hace nada): si no, la WebView recargaría la página y perdería el estado de la interfaz.
- Atajos nuevos: `repo.open` (`Mod+O`, global, también en la paleta y en el menú).

### Pestañas: tope, orden con teclado, indicadores e hibernación

- **Tope blando de 20** (spec §8): Rust avisa en `OpenResult.warning` y la UI avisa también al crear una pestaña nueva a partir de 20 (una sola vez por pestaña nueva). No impide abrir más.
- **Reordenar**: ratón (arrastrar) o teclado (`Alt+Mayús+←/→`, atajo global `tab.moveLeft`/`tab.moveRight`; las flechas sin modificador solo mueven el foco entre pestañas). Persiste con `reorder_tabs`.
- **Indicadores** (`TabInfo`): rama, nº de cambios, nº de conflictos, ocupado y `missing`, accesibles en el nombre de la pestaña. Las pestañas ocultas muestran el último contador conocido; se corrige al activarlas (spec §8: el sondeo ligero cada 60 s queda para después).
- **Hibernación**: Rust ya libera el vigilante de las pestañas no activas y las refresca al activarse (ver «Vigilancia de archivos»). La UI, además, tras **10 minutos** oculta (`HIBERNATE_AFTER_MS`, revisión cada minuto) libera el `Snapshot` y el historial cargado (commits, grafo, diffs) de la pestaña (`TabState.hibernate()`); conserva borrador del commit, panel y contadores. Una pestaña hibernada **ignora** los `snapshot` que lleguen; al activarse se rehidrata (`load()`; Rust refresca además al activar). No hiberna con operación en curso, commit/IA en marcha, conflictos o integración en curso (hay ediciones sin aplicar).

## Eventos (Rust → UI)

| Evento | Carga |
|---|---|
| `snapshot` | `{ tabId, snapshot: Snapshot }` |
| `tab-updated` | `TabInfo` (rama, nº de cambios, ocupado…) |
| `session-event` | `SessionEvent` de `ariadna-git` (`sessionId`, `revision`, `kind`) |
| `tabs-changed` | `AppState` (abrir, cerrar, reordenar, restaurar) |
| `notice` | `{ kind: string, message: string }`: aviso no intrusivo (toast de advertencia). Hoy solo `watcherFallback`: la vigilancia nativa no pudo arrancar o desbordó (p. ej. límite de inotify) y se sondea; sale **una sola vez** por ejecución |
| `menu-action` | `{ id: string }`: elemento del menú nativo pulsado (solo macOS); `id` es un id del registro de atajos (ver «Menú nativo y atajos») |
| `clone-progress` | `{ operationId, phase: string, percent: number \| null }` (`operationId` en un rango propio, no pertenece a ninguna pestaña) |

## Vigilancia de archivos

No hay comando: la vigilancia es interna de Rust y solo se manifiesta como `snapshot` / `tab-updated`.

- **Qué vigila**: solo la pestaña **activa** (las demás liberan su vigilante y se refrescan al activarse). El worktree de forma recursiva y `.git/{HEAD,index,MERGE_HEAD,CHERRY_PICK_HEAD,REVERT_HEAD,REBASE_HEAD,packed-refs,shallow,config}`, `.git/refs/**`, `rebase-merge/`, `rebase-apply/` y `sequencer/`. Se ignoran `.git/objects`, `.git/logs`, `*.lock`, `node_modules`, `target`, `dist`, `.svelte-kit`, `.vs` y similares, y temporales de editor (`*.swp`, `*~`, `.#*`, `.DS_Store`). `bin`, `obj` y `packages` (compilación de .NET/NuGet) solo se ignoran si git los ignora (`git check-ignore`, una vez por carpeta), porque `bin/` también puede llevar scripts versionados. En Linux el tope de carpetas vigiladas (8.192) es uno para toda la vida del vigilante, y mientras dura un refresco los eventos nuevos solo dejan un aviso pendiente, no una cola.
- **Debounce**: 300 ms de silencio, con tope de 2 s desde el primer evento; luego se pide el refresco coalescido de la sesión.
- **Respaldo**: si no arranca, desborda, o el sistema de archivos no emite eventos (UNC en Windows; 9p/drvfs/NFS/CIFS en Linux, p. ej. `/mnt/c` en WSL), se sondea cada 2,5 s solo con la ventana enfocada (`WindowEvent::Focused` en Rust). Un fallo inesperado emite `notice`; los entornos conocidos sin eventos sondean sin avisar.
- **Foco**: al recuperar el foco, Rust y la UI (`window` `focus` → `refresh(tabId)`) refrescan la pestaña activa.

## Verificación del contrato

- `src-tauri` genera **fixtures JSON reales** (salida del motor sobre repos git temporales) en `ui/src/lib/api/fixtures/` mediante un test, y falla si cambian sin regenerarlos.
- `ui` tiene tests (Vitest + `svelte-check`) que cargan esos fixtures y los comprueban contra `types.ts`, de modo que un cambio en los modelos de Rust rompe la compilación de la UI.
- La UI trae un **backend simulado** (`mockApi`) con la misma interfaz `Api`, para desarrollar y previsualizar sin Tauri y para los tests de componentes.
