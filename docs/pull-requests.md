# Spike de pull requests (M8): endpoints, permisos y lo que NO está verificado

> **Estado: GitHub VERIFICADO con servidor real (2026-11-11, ver «Resultado del spike REAL»); Azure DevOps NO VERIFICADO.** No existe ningún token real en este entorno y no se ha
> contactado con ninguna API real. Todo lo de abajo sale de la **documentación publicada** de Azure
> DevOps Services REST 7.1 y de la API REST/GraphQL de GitHub, y se ha probado solo contra un
> servidor simulado (`wiremock`) con JSON reconstruido de esa documentación. Cada fila marcada
> «NO VERIFICADO» debe comprobarse con PAT reales (checklist al final) antes de publicar `alpha.3`.

Código: `crates/ariadna-hosting` (`azure.rs`, `github.rs`, `http.rs`, `auth.rs`). Pruebas:
`crates/ariadna-hosting/tests/{azure_devops,github,credential_fill}.rs` y
`src-tauri/src/core/tests/pull_requests.rs`.

## Resultado del spike REAL de GitHub (2026-11-11)

Ejecutado con `crates/ariadna-hosting/tests/github_real.rs` (`#[ignore]`, `ARIADNA_SPIKE_REPO=jamartif/ariadna-spike-pr`) contra un repo privado desechable. Credencial por `git credential fill` (helper `gh.exe auth git-credential`), sin imprimir ni guardar el token. **72 OK, 5 LÍMITE, 0 FALLO.**

**VERIFICADO en GitHub real:** capacidades, crear PR (normal y borrador), duplicada/sin cambios/rama inexistente (422 → CONFLICT / INVALID_REFERENCE), listar (abiertas, mías, revisor, cerradas, borradores), paginación por `Link`, detalle (mergeable, checks, revisores), archivos y parche, hilos en línea (RIGHT/LEFT), comentario general, responder, resolver y reabrir hilo (GraphQL), review COMMENT, cerrar y reabrir, merge/squash/rebase, merge con conflicto (405 → POLICY), merge con sha obsoleto (409), 404 y 401 (AUTH).

**Límites reales de GitHub (no son fallos del cliente):**
- No se puede aprobar ni pedir cambios en la PR propia (422) ni pedir revisión al autor.
- No se puede completar un borrador (405/422).
- No se puede reabrir una PR ya fusionada (422).
- Borrar el repositorio exige un permiso que el token no tiene; borrar ramas por API sí funciona.

**Sigue NO VERIFICADO:** todo Azure DevOps (no hay cuenta), `*.ghe.com`/GHES, `git credential fill` con GCM real de macOS/Windows, TLS contra servidor real distinto de GitHub, y aprobar/rechazar con una segunda cuenta.

## Reglas que NO dependen del spike (ya verificadas por pruebas)

- El token **no se guarda, no se cachea y no se registra**. Cada operación obtiene la credencial con
  `git credential fill` (stdin/stdout, `GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never`, entorno
  saneado por lista blanca), la usa en memoria (`zeroize`) y la suelta al terminar.
- Nunca va en `argv`, URL, cuerpo, `Debug`, registro ni mensaje de error: todo texto del servidor pasa
  por un redactor que sustituye el secreto y su forma Base64 por `***` (prueba con un servidor hostil
  que repite la cabecera `Authorization`).
- Solo se contacta el origen (esquema + host + puerto) del proveedor; un `Link: rel="next"` o una
  redirección a otro origen **no se siguen** (el token no viaja).
- Timeout 30 s, tope de respuesta 8 MiB, reintentos acotados (429 con `Retry-After`; 502/503/504 y
  errores de red solo en lecturas; nunca una escritura), `Retry-After` > 10 s no se espera.

## Autenticación

| Aspecto | Supuesto | Estado |
|---|---|---|
| Cabecera | `Authorization: Basic base64(usuario:token)` para ambos proveedores (spec §4.4). Usuario = el que devuelve el helper (puede ser cualquiera en Azure DevOps) | NO VERIFICADO (GitHub acepta Basic con PAT en `api.github.com`; confirmar también con el token OAuth que entrega el GCM) |
| `git credential fill` | Se envía `url=https://host/ruta` y se lee `username=`/`password=`. Si el helper no lo tiene o falla → `AUTH` | Verificado con un `git` falso; **NO VERIFICADO** con GCM real en Windows/macOS |
| Entra ID | `AuthMethod::Bearer(token)` existe pero no se usa; llega en v0.2 (D22) | Reservado |

## Azure DevOps (`api-version=7.1`; `7.1-preview.1` donde se indica)

Base: `https://dev.azure.com/{org}` (también `{org}.visualstudio.com` y SSH, normalizados por
`RemoteInfo`). Repo: `/{project}/_apis/git/repositories/{repo}/…`. Si la URL no trae proyecto se usa el
nombre del repo como proyecto (**NO VERIFICADO** para URLs `…/_git/repo` sin proyecto).

| Operación | Método y ruta | Permiso mínimo (PAT) | Estado |
|---|---|---|---|
| Usuario actual (para `mine`, `reviewing`, voto, auto-complete) | `GET /_apis/connectionData?api-version=7.1-preview.1` → `authenticatedUser.id` | cualquier PAT válido (¿Identity read?) | NO VERIFICADO |
| Listar | `GET …/pullrequests?searchCriteria.status={active\|all}&searchCriteria.creatorId=&searchCriteria.reviewerId=&$top={n+1}&$skip=` | Code (Read) | NO VERIFICADO. «Cerradas» pide `all` y descarta las activas en cliente (las páginas pueden quedar cortas) |
| Detalle | `GET …/pullrequests/{id}` | Code (Read) | NO VERIFICADO |
| Políticas | `GET /{project}/_apis/policy/evaluations?artifactId=vstfs:///CodeReview/CodeReviewId/{projectId}/{prId}&api-version=7.1-preview.1` | Code (Read) (¿otro ámbito?) | NO VERIFICADO; si falla con 401/403/404 el detalle sigue sin políticas |
| Work items vinculados | `GET …/pullrequests/{id}/workitems` | Work Items (Read) | NO VERIFICADO; solo `id` y `url` (sin título) |
| Commits | `GET …/pullrequests/{id}/commits?$top=100` | Code (Read) | NO VERIFICADO (sin paginar más allá de 100) |
| Archivos cambiados | `GET …/pullrequests/{id}/iterations` y `…/iterations/{n}/changes?$top=100&$skip=` (`changeEntries`, `nextSkip`) | Code (Read) | NO VERIFICADO (forma de `changeEntries` y `nextSkip`) |
| Diff | Para cada archivo: `GET …/items?path=&versionDescriptor.version={sha}&versionDescriptor.versionType=commit&$format=text` con `Accept: text/plain`; base = `commonRefCommit`, cabeza = `sourceRefCommit` de la última iteración; el parche se calcula en cliente (`similar`) | Code (Read) | **NO VERIFICADO** (¿`$format=text` devuelve el contenido en bruto? ¿qué pasa con binarios, BOM, LFS?). Tope: 100 archivos y 1 MiB cada uno |
| Crear | `POST …/pullrequests` `{sourceRefName,targetRefName,title,description,isDraft,reviewers:[{id}],workItemRefs:[{id}]}` | Code (Read & write) | NO VERIFICADO (¿acepta correo o solo GUID en `reviewers.id`? Título ≤ 400, descripción ≤ 4000) |
| Abandonar / reabrir | `PATCH …/pullrequests/{id}` `{status:"abandoned"\|"active"}` | Code (Read & write) | NO VERIFICADO |
| Hilos (ver) | `GET …/pullrequests/{id}/threads` | Code (Read) | NO VERIFICADO (se descartan los hilos `isDeleted` y los de comentarios `system`) |
| Hilo nuevo | `POST …/threads` `{comments:[{parentCommentId:0,content,commentType:"text"}],status:"active",threadContext:{filePath,rightFileStart/End\|leftFileStart/End:{line,offset:1}}}` | Code (Read & write) | NO VERIFICADO |
| Responder | `POST …/threads/{tid}/comments` `{content,parentCommentId:1,commentType:"text"}` y relectura `GET …/threads/{tid}` | Code (Read & write) | NO VERIFICADO (¿`parentCommentId` correcto para responder al primero?) |
| Resolver / reabrir hilo | `PATCH …/threads/{tid}` `{status:"fixed"\|"active"}` | Code (Read & write) | NO VERIFICADO |
| Voto | `PUT …/pullrequests/{id}/reviewers/{miId}` `{vote:10\|5\|0\|-5\|-10}` (+ hilo general si hay comentario) | Code (Read & write) | NO VERIFICADO |
| Completar | `PATCH …/pullrequests/{id}` `{status:"completed",lastMergeSourceCommit:{commitId},completionOptions:{mergeStrategy:"noFastForward"\|"squash"\|"rebase"\|"rebaseMerge",deleteSourceBranch,transitionWorkItems,mergeCommitMessage}}` | Code (Read & write) + «Contribute to pull requests» | **NO VERIFICADO**: la finalización es asíncrona (la respuesta puede seguir `active`); cómo informa un fallo de política (¿400/409?, ¿`typeKey`?) |
| Auto-complete | `PATCH …/pullrequests/{id}` `{autoCompleteSetBy:{id},completionOptions}` (sin `status`) | Code (Read & write) | NO VERIFICADO |

Errores Azure DevOps (clasificación en `azure.rs::classify`), **NO VERIFICADO**: `203`/`30x` a la página
de inicio de sesión y `401` → `AUTH`; `403` → `AUTH` con el permiso extraído de «You need the X
permission» si aparece; `404` → `NOT_FOUND`; `409` → `CONFLICT`; `400/412/422` con `typeKey` o texto que
mencione *policy* → `POLICY`, si no `INVALID_REFERENCE`; `429` con `Retry-After` → `RATE_LIMITED`; 5xx →
`NETWORK`.

## GitHub (REST `X-GitHub-Api-Version: 2022-11-28` + GraphQL)

Base: `https://api.github.com`; `*.ghe.com` → `https://api.{sub}.ghe.com`; GHES → `https://{host}/api/v3`
(GraphQL `https://{host}/api/graphql`). **NO VERIFICADO** para `*.ghe.com` y GHES.

| Operación | Método y ruta | Permiso mínimo (fine-grained / clásico) | Estado |
|---|---|---|---|
| Usuario | `GET /user` | — | NO VERIFICADO |
| Listar | `GET /repos/{o}/{r}/pulls?state={open\|closed}&sort=created&direction=desc&per_page&page` (+ `Link: rel="next"`) | Pull requests: read · `repo`/`public_repo` | NO VERIFICADO. «Mías» y «revisor» se filtran **en cliente** por página (autor / `requested_reviewers`); el listado no trae votos ni comprobaciones |
| Detalle | `GET …/pulls/{n}`, `…/pulls/{n}/reviews`, `…/commits/{sha}/check-runs`, `…/commits/{sha}/status`, `…/pulls/{n}/commits`, `…/pulls/{n}/files` | Pull requests: read; Checks: read; Commit statuses: read; Contents: read | NO VERIFICADO. Voto efectivo = última revisión no `COMMENTED`; si el usuario vuelve a estar en `requested_reviewers` cuenta como sin voto. Sin información de qué comprobaciones son obligatorias |
| Diff | `GET …/pulls/{n}/files` (`patch` por archivo) | Pull requests: read | NO VERIFICADO: sin `patch` en binarios y archivos enormes (se informa); máx. 3000 archivos |
| Crear | `POST …/pulls` `{title,head,base,body,draft}` + `POST …/pulls/{n}/requested_reviewers` | Pull requests: write (+ Contents: read) | NO VERIFICADO. PR desde forks (`head: "usuario:rama"`) sin probar |
| Cerrar / reabrir | `PATCH …/pulls/{n}` `{state}` | Pull requests: write | NO VERIFICADO |
| Hilos en línea (ver) | GraphQL `repository.pullRequest.reviewThreads(first:100){…comments(first:100)}` | Pull requests: read | **NO VERIFICADO**: sin paginar más de 100 hilos/100 comentarios |
| Comentarios generales | `GET/POST …/issues/{n}/comments` | Pull requests: write (¿o Issues: write?) | NO VERIFICADO; no se anidan ni se resuelven |
| Hilo nuevo en línea | `POST …/pulls/{n}/comments` `{body,commit_id,path,line,side}` y relectura GraphQL | Pull requests: write | NO VERIFICADO (líneas fuera del diff → 422) |
| Responder | GraphQL `addPullRequestReviewThreadReply` | Pull requests: write | NO VERIFICADO |
| Resolver / reabrir | GraphQL `resolveReviewThread` / `unresolveReviewThread` | Pull requests: write | NO VERIFICADO |
| Voto | `POST …/pulls/{n}/reviews` `{event:APPROVE\|REQUEST_CHANGES\|COMMENT,body,commit_id}` | Pull requests: write | NO VERIFICADO (exige `body` salvo en `APPROVE`; aprobar la propia PR → 422 → `POLICY`) |
| Completar | `PUT …/pulls/{n}/merge` `{merge_method,sha,commit_title,commit_message}`; luego `DELETE …/git/refs/heads/{rama}` (solo si no es de fork) | Pull requests: write + Contents: write | NO VERIFICADO: `405` (no fusionable) → `POLICY`, `409` (cambió la cabeza) → `CONFLICT`; borrar la rama es mejor esfuerzo (si falla se anota en `mergeDetail`) |

Errores GitHub (`github.rs::classify`), **NO VERIFICADO**: `401` → `AUTH`; `403` con `x-ratelimit-remaining:
0`, o texto *rate limit/abuse/secondary*, o `429` → `RATE_LIMITED` (espera = `Retry-After` o
`x-ratelimit-reset`); `403` con `x-github-sso` → `AUTH` (SSO); otro `403` → `AUTH` con el permiso de
`x-accepted-github-permissions` / `x-accepted-oauth-scopes`; `404` → `NOT_FOUND`; `405` → `POLICY`; `409` →
`CONFLICT`; `422` con *already exists* → `CONFLICT`, con *protected/review/required/own pull request* →
`POLICY`, resto → `INVALID_REFERENCE`; 5xx → `NETWORK`. Los errores de GraphQL llegan con HTTP 200
(`errors[].type`: `NOT_FOUND`, `FORBIDDEN`, `INSUFFICIENT_SCOPES`, `RATE_LIMITED`).

## Checklist del spike con PAT reales

Hacer con **un repositorio de prueba** por proveedor y dos usuarios (autor y revisor). Anotar aquí el
resultado de cada casilla y corregir `azure.rs`/`github.rs` si la realidad difiere.

### Antes de empezar
- [ ] Crear los PAT con los **permisos mínimos** de las tablas y comprobar qué falla con menos (cada operación que falle con 401/403 debe mostrar `AUTH` y, si el servidor lo indica, el permiso en `detail`).
- [ ] `git credential fill` con el GCM real (Windows y macOS): ¿devuelve usuario y token para `dev.azure.com`, `github.com` y GHE? ¿con `credential.useHttpPath`? ¿se abre alguna ventana (no debería, `GCM_INTERACTIVE=never`)?
- [ ] Con GitHub vía GCM (token OAuth `gho_`): ¿Basic funciona o hace falta `Authorization: Bearer`?
- [ ] Confirmar que ninguna petición lleva el token fuera de la cabecera (proxy local o `mitmproxy` solo contra repos de prueba) y que no hay rastro en `argv` (`ps`/Process Explorer durante la llamada).

### Azure DevOps
- [ ] `connectionData`: forma de `authenticatedUser` y versión de API aceptada.
- [ ] Listar (`active`, `all`, `creatorId`, `reviewerId`), paginación `$top/$skip` y orden.
- [ ] Detalle: `reviewers[].vote`, `mergeStatus`, `isDraft`, `lastMergeSourceCommit`, `autoCompleteSetBy`.
- [ ] Políticas: ¿qué ámbito exige `policy/evaluations`? ¿forma de `configuration.type.displayName`?
- [ ] Work items vinculados: forma de `workitems` y ámbito.
- [ ] Iteraciones y `changes`: forma de `changeEntries`, paginación `nextSkip`, renombrados, carpetas, `changeType` compuesto (`edit, rename`).
- [ ] `items?...&$format=text`: ¿contenido en bruto? binarios, archivos grandes, BOM, finales de línea, rutas con espacios/acentos.
- [ ] Crear: `reviewers.id` con GUID y con correo; borrador; work items; duplicada (¿409 y `typeKey`?).
- [ ] Abandonar y reabrir; reabrir una completada (¿error?).
- [ ] Hilos: ver, crear en línea (derecha e izquierda), general, responder, resolver (`fixed`) y reabrir; comentarios `system`.
- [ ] Votar con los cinco valores; ¿el voto de rechazo bloquea la completación?
- [ ] Completar con las cuatro estrategias, borrar rama, work items, mensaje; ¿asíncrono? ¿cómo se ve el fallo de política y el de conflicto (códigos HTTP, `typeKey`, texto)?
- [ ] Auto-complete (activar y que complete al cumplirse las políticas).
- [ ] 403 sin permiso: ¿el mensaje trae «You need the X permission»? 401 por PAT caducado/revocado: ¿401, 203 o redirección?
- [ ] 429 real: ¿`Retry-After`? ¿cabeceras `X-RateLimit-*`/`Retry-After` de TSTU?
- [ ] URLs `{org}.visualstudio.com/{proyecto}/_git/{repo}`, `…/DefaultCollection/…` y SSH: ¿misma base `dev.azure.com/{org}`? proyecto con espacios.

### GitHub
- [ ] `GET /user` con PAT clásico y fine-grained; Basic frente a Bearer.
- [ ] Listar `open`/`closed`; `Link: rel="next"`; coste de filtrar «mías»/«revisor» en cliente (¿hace falta la API de búsqueda?).
- [ ] Detalle: `mergeable`/`mergeable_state` (¿`null` mientras calcula? ¿reintentar?), `requested_reviewers`, `reviews`, `check-runs`, `status`; PR de fork.
- [ ] `pulls/{n}/files`: `patch` ausente en binarios/grandes; renombrados; límite de archivos.
- [ ] Crear (mismo repo y desde fork), borrador, revisores, duplicada (¿422 con «already exists»?).
- [ ] Cerrar/reabrir; reabrir una fusionada.
- [ ] GraphQL: `reviewThreads` (outdated, `line` nulo, `diffSide`), responder, resolver/reabrir; ¿límite de 100?
- [ ] Comentario en línea con `line` fuera del diff (422), `side: LEFT`.
- [ ] Revisión `APPROVE` / `REQUEST_CHANGES` / `COMMENT`, aprobar la propia PR, `body` obligatorio.
- [ ] Merge con `merge|squash|rebase`, `sha` obsoleto (409), rama protegida / checks pendientes (405), borrar rama de origen.
- [ ] Límite primario y secundario (403/429): cabeceras `x-ratelimit-*` y `retry-after`.
- [ ] SSO de organización (`x-github-sso`), permiso faltante (`x-accepted-github-permissions`) y 404 por falta de acceso.
- [ ] GitHub Enterprise Server (`/api/v3`, `/api/graphql`) y `*.ghe.com` (`api.{sub}.ghe.com`).

### Red y TLS
- [ ] TLS contra los servidores reales con las raíces del sistema (`rustls-platform-verifier`), también tras proxy corporativo con CA propia; el certificado inválido debe dar `NETWORK`, nunca continuar.
- [ ] Redirecciones: confirmar que ningún flujo legítimo necesita salir del host (si lo necesita, decidir caso por caso).
- [ ] Tiempos: 30 s por petición; cuánto tarda un detalle completo (≈ 6–8 peticiones) y un diff de Azure DevOps de 100 archivos (≈ 200 peticiones: ¿hace falta paralelizar o pedir menos?).

## Contraste de Azure DevOps con la documentación oficial (2026-11-14)

Sin una cuenta no se puede probar el comportamiento, pero sí comprobar que **las rutas y los parámetros que construye el cliente existen y coinciden** con la referencia de Microsoft (Azure DevOps REST 7.1, `learn.microsoft.com/rest/api/azure/devops/git`). Revisado con `curl` en la fecha indicada:

| Lo que hace Ariadna | Documentación 7.1 | Estado |
|---|---|---|
| `POST /{org}/{project}/_apis/git/repositories/{id}/pullrequests?api-version=7.1` | Idéntico (incluido `{project}` en la ruta) | Coincide |
| `GET …/pullRequests/{n}/threads?api-version=7.1` | Idéntico (mayúsculas incluidas) | Coincide |
| `update`/completar con `lastMergeSourceCommit` | El campo existe; la respuesta asíncrona (202) y su reintento siguen sin confirmar | Parcial |
| `GET …/items?path=…&versionDescriptor.versionType=commit&version=…&$format=text` (diff en cliente) | La ruta y `versionDescriptor` existen; **que devuelva el contenido de la versión pedida para cualquier archivo sigue sin confirmar** | Parcial |
| `GET /_apis/connectionData` | Existe | Coincide |
| `GET /{project}/_apis/policy/evaluations?artifactId=…` | Existe | Coincide |
| Permisos del PAT (Code Read & Write, Pull Request Threads) | Son los que documenta Microsoft para esas operaciones | Coincide |

**Sigue NO VERIFICADO** (necesita cuenta real): forma exacta de los errores (`401` frente a `203`, mensaje de política incumplida), finalización asíncrona de una PR y su `retry-after`, respuesta real del diff archivo a archivo con repos grandes, PR entre forks y políticas de rama de la organización.

