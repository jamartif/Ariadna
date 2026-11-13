# Ariadna en Rust — especificación v1

Cliente Git de escritorio para **macOS y Windows**, con IA propuesta por CLI oficiales (Claude, Codex, Copilot) y aspecto moderno. Sustituye a la versión Swift (`Sources/`, `Tests/`), que queda como oráculo de comportamiento hasta alcanzar la paridad.

Uso previsto: **a la vez como particular y en la empresa de Javier** (D14, D16). Estado: v1.1, tras la revisión con contexto limpio de [revision-egm.md](revision-egm.md). Las decisiones marcadas **PROPUESTA** las ha tomado el PM para resolver la revisión y Javier puede vetarlas.

Método: Elephant-Goldfish Model (EGM v0.1.1): brainstorm, borrador, revisión con revisores sin contexto, resolución.

## 1. Objetivo y alcance

Paridad funcional con el Swift actual, más Windows y pestañas por repo. Nada más en v0.1.

| Plataforma | Estado en v0.1 |
|---|---|
| macOS 15+ (universal arm64 + x86_64) | Oficial |
| Windows 10 (22H2) y 11, x64 | Oficial |
| Windows 11 ARM64 | **PROPUESTA: compilado, con tests unitarios y de integración del núcleo en CI y publicado, sin garantía de IA ni E2E** (ver D7) |

### 1.1 Paridad frente a novedad

Cada requisito es `[P]` paridad con el Swift o `[N]` novedad. Las novedades no pueden retrasar la salida de una paridad.

| Área | Marca | Notas |
|---|---|---|
| Abrir repo, estado, stage/unstage por archivo y hunk, descartar, commit, amend seguro, log, búsqueda en todo el historial, diff | [P] | Ver §3 |
| Ramas, fetch/pull/push, merge ff/no-ff/ya al día, conflictos a tres vías, continuar/abortar | [P] | Ver §3 |
| Recientes y favoritos | [P] | Se importa el `recents.json` existente |
| IA: mensaje de commit, resumen, explicar commit, propuesta de conflicto | [P] | El Swift la tiene con Copilot y Codex tras flag |
| IA con Claude | [N] | Spike verificado, ver §5 |
| Modo Auto | [P] ampliado | El Swift ya hacía Auto (Copilot → Codex) y saltaba ante varios fallos; aquí salta solo si el CLI no está instalado (RF-IA9) |
| Consentimiento por proveedor, ajustes por proveedor | [N] | |
| Pestañas por repo | [N] | Ver §8 |
| Windows | [N] | Ver §4 |
| Tema moderno, paleta de comandos | [N] | Ver §7 |
| Clonar desde URL con PAT o inicio de sesión del GCM; Azure DevOps y GitHub reconocidos | [N] | Paridad de uso con GitKraken (§4.3, D19, D20) |
| Preparar/quitar líneas sueltas dentro de un hunk; stash (guardar, listar, aplicar, sacar, borrar con confirmación, ver diff) | [N] | Uso diario (`ipc.md` «Líneas sueltas y stash»); los conflictos al aplicar un stash van al panel de Conflictos |
| Pull requests: crear, listar, detalle, bajar rama, revisar y comentar, votar, completar | [N] | Azure DevOps y GitHub (§4.4, D21) |

**Fuera de v0.1 (igual que el Swift):** init, tags, submódulos, LFS, worktrees (solo se detectan y se avisa), rebase interactivo, cherry-pick visual, grafo de commits, diff lado a lado, actualizador automático, tiendas de aplicaciones.

## 2. Decisiones

| # | Decisión | Estado |
|---|---|---|
| D1 | Alcance v0.1 = paridad con Swift + Windows + pestañas | Cerrada (Javier) |
| D2 | IA por CLI oficiales `claude`, `codex`, `copilot`. Sin API keys ni proxies | Cerrada (Javier) |
| D3 | Tauri 2 + Svelte 5 + TypeScript. Núcleo en Rust. Editor de diff: CodeMirror 6. Fuentes: Inter y JetBrains Mono | **Cerrada** (PM, tras revisión) |
| D4 | Mismo repo `jamartif/Ariadna`, rama `rust-migration`. Se fusiona a `main` cuando haya paridad | Cerrada (Javier) |
| D5 | Git = el `git` del sistema. Versión mínima **2.39** | Heredada + revisión |
| D6 | Fuera de tiendas | Heredada |
| D7 | Windows en v0.1. ARM64 = mejor esfuerzo (compila, núcleo probado) | Windows: Cerrada (Javier). ARM64 mejor esfuerzo: **PROPUESTA** |
| D8 | Modo Auto: Claude → Codex → Copilot, solo entre proveedores consentidos | Cerrada (Javier) + revisión |
| D9 | Pestañas por repo | Cerrada (Javier) |
| D10 | La IA entra siempre en el binario, apagada hasta consentimiento. Hay un interruptor para desactivarla del todo | **PROPUESTA** (sustituye al flag `ARIADNA_AI`) |
| D11 | v0.1 sin Mica/vibrancy: fondo sólido con diseño propio. Mica y vibrancy en v0.2 | **PROPUESTA** |
| D12 | Instalador Windows: NSIS, con bootstrapper de WebView2 | Cerrada (PM) |
| D14 | **Doble uso: particular y empresa.** Ariadna usa los CLI oficiales con la sesión que el usuario tenga; **no se gestionan planes ni condiciones de cuenta de la empresa**. No hay proxy ni certificado de firma en la empresa. No se diseña para despliegue gestionado por TI | **Cerrada (Javier)** |
| D18 | Licencia **MIT** (`LICENSE` en la raíz) | **Cerrada (Javier)** |
| D16 | **Perfiles Personal y Empresa.** Cada repo se asocia a un perfil. El perfil decide qué proveedores de IA se permiten, con qué cuenta y con qué consentimiento. Claude queda desactivado en el perfil Empresa hasta que se habilite expresamente | **PROPUESTA** |
| D19 | **Alojamiento remoto agnóstico.** Ariadna solo habla con `git`: sirve igual con **Azure DevOps (el de la empresa de Javier)**, GitHub, GitLab, Bitbucket o un servidor propio. Reconoce el proveedor por la URL del remoto para sugerir perfil, mostrar su icono y abrir el navegador (repo / PR) (§4.3) | **PROPUESTA** |
| D20 | **Clonar y autenticar con PAT desde la app, en v0.1**, como en GitKraken: se introduce URL y token una vez. El token **no lo guarda Ariadna**: se entrega al *credential helper* de git (Git Credential Manager en Windows, Keychain en macOS) con `git credential approve`. También funciona el inicio de sesión interactivo del GCM (Microsoft/GitHub) sin PAT (§4.3) | **PROPUESTA** (sustituye a «clone fuera de v0.1») |
| D21 | **Pull requests completas en v0.1** para **Azure DevOps y GitHub**: crear, listar, ver detalle, bajar la rama, **revisar** (diff y comentarios en hilos), **votar** y **completar/abandonar** (§4.4). GitLab y Bitbucket: solo «abrir en el navegador». Usa la API REST del proveedor con el mismo PAT, obtenido del *credential helper* en cada llamada y **sin guardarlo** | **Cerrada (Javier)**; el alcance completo es decisión suya, el PM lo marca como riesgo (§12) |
| D22 | El cliente de proveedor abstrae la autenticación (`AuthMethod::Pat` hoy, `Bearer` mañana) para añadir **inicio de sesión con Microsoft (Entra ID)** en la fase siguiente sin reescribir nada | **Cerrada (Javier)**: Microsoft en v0.2 |
| D17 | **Firma de binarios por etapas:** alfas sin firmar; para `v0.1.0`, solicitar **SignPath Foundation** (gratis para OSS; la licencia MIT cumple el requisito de licencia, el resto de condiciones se leen al solicitarla). Si no se concede, se publica sin firmar con la guía de «Abrir igualmente» | **PROPUESTA** |
| D15 | macOS mínimo **15** (igual que el Swift) | **Cerrada (Javier)** |
| D13 | Hitos alfa publicables: `alpha.1` tras M5 (Git sin IA), `alpha.2` tras M6 (IA) | **PROPUESTA** |

## 3. Arquitectura

```
UI Svelte ─IPC tipado─▶ comando Tauri ─▶ SessionManager ─▶ RepositorySession ─▶ GitEngine ─▶ ProcessRunner ─▶ git
                                                     └──▶ AiService ─▶ AiProvider ─▶ ProcessRunner ─▶ claude | codex | copilot
```

Cargo workspace: `crates/ariadna-process` (lanzador de procesos), `crates/ariadna-git`, `crates/ariadna-ai`, `src-tauri`, `ui`.

Equivalencias Swift → Rust: `protocol` → `trait`; `actor` → tarea tokio con canal; `@MainActor` → la UI no tiene estado de verdad; `UserConfirmation.require` → ver §6.

**Regla de oro: el estado vive en Rust.** La UI es una vista sin lógica de git ni de IA.

### 3.1 Ciclo de vida de `RepositorySession`

`Abriendo → Activa → Cerrando → Cerrada`.

- Cerrar una pestaña, cerrar la ventana y salir de la app siguen el mismo camino.
- Si hay mutación en curso o en cola, **pide confirmación** (nombrando repo y operación). Tras confirmar: cancelación cooperativa, plazo de gracia (5 s) y luego se termina el árbol de procesos. La cola se vacía. Los eventos tardíos se descartan por `session_id`.
- `index.lock` solo se elimina si lo creó un proceso git de Ariadna que acabamos de terminar. Si no, se explica el bloqueo y no se borra nada.
- Repo borrado, movido o en unidad desconectada: banner de pestaña con **Reintentar / Localizar / Cerrar**.

### 3.2 Concurrencia

- Mutaciones: serie estricta por repo, con **prioridad** sobre lecturas.
- Lecturas (`status`, `diff`, `log`): se lanzan con `GIT_OPTIONAL_LOCKS=0` y `--no-optional-locks`. Pueden solapar entre sí.
- Refrescos: coalescidos (máx. uno en curso y uno pendiente por sesión) y descartados si una mutación ya está en cola.
- Cada snapshot lleva `revision` monótona. La UI ignora revisiones anteriores.
- Ante `index.lock` ajeno: reintento acotado con backoff (3 intentos, 200/400/800 ms) y mensaje accionable.
- Cola por **clave de repo** (no por pestaña). Clave = ruta canónica sin prefijo `\\?\`, comparada sin distinguir mayúsculas según el sistema de archivos, resuelta con `git rev-parse --git-common-dir`.

### 3.3 Contrato IPC

- Todo comando recibe `session_id`. Los tipos se generan con `tauri-specta`; la CI falla si el binding generado difiere del versionado.
- Operaciones largas devuelven `operation_id` y emiten `op:progress | done | error | cancelled` por `tauri::ipc::Channel`. Existe `cancel_operation(operation_id)`.
- Los eventos de estado llevan `session_id` y `revision`. Se envían **deltas por sección** (cambios, ramas, historial paginado).
- Rutas: `PathBuf`/bytes en todo el IPC, nunca `String` solo, para no perder nombres no UTF-8.
- Errores: enum tipado (`thiserror` + `specta`) con código estable, texto localizable, causa y acción sugerida: `GitNotFound`, `NotARepo`, `Locked`, `Conflict`, `Auth`, `Network`, `Timeout`, `Cancelled`, `DubiousOwnership`, `RepoMissing`, `AiNotInstalled`, `AiNotLoggedIn`, `AiQuota`, `AiBadOutput`. Presentación: error de operación = toast con detalle; error de sesión = banner de pestaña; fatal = pantalla de recuperación. El stderr se muestra tras redactar secretos.

## 4. Git

Heredado de `specs/spec.md` §3 del Swift. Requisitos por ID para poder trazar tareas y tests:

| ID | Requisito [P] |
|---|---|
| G-01 | Abrir carpeta dentro de un repo (sube a la raíz); error claro si no lo es |
| G-02 | Estado con `status --porcelain=v2 -z`: preparados, sin preparar, sin seguimiento, ignorados, conflictos |
| G-03 | Stage/unstage por archivo |
| G-04 | Stage/unstage por hunk (nuevos, CRLF, sin salto final, rutas con espacios y Unicode) |
| G-05 | Descartar cambios, siempre con confirmación |
| G-06 | Commit con mensaje obligatorio; aviso en HEAD detached |
| G-07 | Amend seguro: sin commits, fuera de rama, merge/rebase en curso o commit ya en un remoto → se niega |
| G-08 | Log con autor, fecha, refs y cuerpo; diff del commit |
| G-09 | Filtro local y búsqueda en todo el historial (mensaje, autor, hash) |
| G-10 | Ramas: listar, crear, cambiar, renombrar, borrar; remotas con seguimiento |
| G-11 | Fetch, pull (merge o rebase), push con `--set-upstream` |
| G-12 | Merge ff / no-ff / ff-only / ya al día |
| G-13 | Conflictos a tres vías (ours/base/theirs), diff3, elección por hunk o archivo, escribir solo con confirmación |
| G-14 | Marcar resuelto; continuar o abortar merge, rebase y cherry-pick |
| G-15 | Recientes y favoritos, con importación del `recents.json` del Swift |
| G-16 | Clonar desde URL (https, ssh, local) con credenciales vía credential helper; probar y olvidar credenciales **[N]** |

### 4.1 Endurecimiento de git (aplica a todas las llamadas)

- `git` se resuelve **una vez al arrancar** a ruta absoluta (nunca con el directorio actual): Windows → `HKLM\SOFTWARE\GitForWindows\InstallPath`, `Program Files\Git\cmd\git.exe`, luego PATH; macOS → PATH ampliado (`/usr/bin`, Homebrew, Command Line Tools). Se valida `git --version ≥ 2.39`.
- `--literal-pathspecs` (`GIT_LITERAL_PATHSPECS=1`), `--` antes de rutas, refs validadas con `git check-ref-format`, rechazo de nombres que empiecen por `-`.
- Rutas a git por stdin cuando son muchas: `--pathspec-from-file=- --pathspec-file-nul` o `update-index -z --stdin` (límite de línea de comandos en Windows).
- Para lecturas: `-c core.fsmonitor=false -c core.hooksPath=<vacío> --no-ext-diff --no-textconv`. `GIT_TERMINAL_PROMPT=0`, `GIT_PAGER=cat`, `GIT_EDITOR=true`. Ejecutar hooks solo en mutaciones, con aviso de confianza del repo la primera vez.
- Codificación: siempre `-z`, `-c core.quotepath=false`, `i18n.logOutputEncoding=UTF-8`. No se parsean mensajes humanos de error; se usan códigos de salida y porcelain. `LC_ALL=C` como apoyo, no como garantía.
- `core.longpaths=true` por proceso y manifiesto `longPathAware`.
- `safe.directory`: error accionable `DubiousOwnership`. Añadirlo solo con confirmación y nunca con `*`.
- Parches: se construyen **desde bytes de `git diff`** y se aplican con `git apply --cached` por stdin binario, sin tocar `\r` ni leer el worktree. Las resoluciones de conflicto se escriben en binario conservando EOL y BOM.
- Operaciones de red: timeout propio, cancelación manual y sin límite mientras haya diálogo de credenciales (GCM, SSH). Nunca se registran URLs con credenciales.

### 4.2 Rutas y plataforma

- `PathSafety` con lista cerrada de tests: `\`, unidades, UNC, `\\?\`, nombres reservados (CON, NUL, AUX…), `.git` y sus variantes NTFS (`.git.`, `git~1`, `.git `), flujos alternativos `fichero:stream`, final con punto o espacio, symlinks/junctions que escapan, 8.3, mayúsculas, NFD/NFC en macOS, nombres `-rf`, `:(glob)*`, `*.c`.
- Al proceso hijo se le pasa la ruta con `dunce::simplified()`.
- Persistencia: `~/Library/Application Support/Ariadna` y `%APPDATA%\Ariadna` (`directories`).
- Atajos: ⌘ en macOS, Ctrl en Windows.
- WSL (`\\wsl$`), OneDrive y unidades de red: soportados en modo lectura y sondeo; documentar limitaciones.
- Matriz de `core.autocrlf`: true, false, input, más `.gitattributes eol=`, ficheros mixtos, UTF-16 y BOM.

## 4.3 Alojamiento remoto: GitHub, Azure DevOps y otros (D19)

**Principio:** el control de versiones va **siempre por `git`** y por la autenticación que el usuario ya tiene (Git Credential Manager, agente SSH, `credential.helper`), así que clonar, fetch, pull y push funcionan igual con `github.com`, GitHub Enterprise, Azure DevOps, GitLab, Bitbucket o un servidor propio. Solo las **pull requests** (§4.4) hablan con la API REST del proveedor, y solo para Azure DevOps y GitHub.

**Reconocimiento del proveedor** (función pura y testeable, `RemoteInfo::parse(url)`), que devuelve `{ provider, host, owner, project?, repo, webUrl }`:

| Proveedor | Formas de URL |
|---|---|
| GitHub / GHE | `https://github.com/o/r(.git)`, `git@github.com:o/r.git`, `ssh://git@host/o/r` |
| Azure DevOps | `https://dev.azure.com/{org}/{project}/_git/{repo}` (también con `{org}@` delante), `git@ssh.dev.azure.com:v3/{org}/{project}/{repo}`, `https://{org}.visualstudio.com/[DefaultCollection/]{project}/_git/{repo}` |
| GitLab, Bitbucket, otros | host + ruta; proveedor `other` si no se reconoce |

Debe tolerar `%20` y espacios en nombres de proyecto, quitar credenciales embebidas (`user:pass@`) y no fallar nunca con basura.

**Para qué se usa (y solo para eso):**
1. **Sugerir perfil** (§5.1.1) con reglas del tipo `host/propietario` (`github.com/mi-org`, `dev.azure.com/mi-org`), no solo GitHub.
2. **Icono y etiqueta del proveedor** en la pestaña y en la lista de remotos.
3. **Abrir en el navegador** (repo, PR, página de creación de PR). GitHub: `/pull/new/{rama}`; Azure DevOps: `/pullrequestcreate?sourceRef={rama}`. Es la única vía de PR para GitLab y Bitbucket. Solo `https`, solo el host del remoto, mediante el plugin `opener` con una capability mínima.

**Autenticación:**
- `GIT_TERMINAL_PROMPT=0` evita que git se cuelgue esperando un teclado inexistente; el Git Credential Manager abre su propia ventana en Windows y usa el llavero en macOS.
- Nuevo error tipado `Auth` (`AUTH` en el contrato IPC) para fetch, pull y push, con texto accionable («inicia sesión con el Git Credential Manager», «comprueba `ssh-add`», «tu PAT ha caducado»). Detectarlo exige reconocer unos pocos textos de git (`Authentication failed`, `could not read Username`, `Permission denied (publickey)`): **es la única excepción a «no se parsean mensajes humanos»**; queda aislada en una función con tests y solo se aplica a operaciones de red con código de salida ≠ 0.
- El filtro de entorno de git sigue quitando variables con pinta de token; si un usuario depende de `GIT_ASKPASS` o de un `credential.helper`, siguen funcionando (`GIT_*` y `GCM_*` están permitidos).

**Particularidades de Azure DevOps a cubrir:**
- Las **políticas de rama** rechazan el push a ramas protegidas: se muestra el `remote:` tal cual llega, sin interpretarlo.
- Ariadna **nunca hace force push**, así que no choca con las políticas que lo prohíben.
- La URL que da el botón *Clone* de Azure DevOps lleva `{org}@` delante: es válida y se conserva.
- SSH exige claves con algoritmos modernos: se documenta en la guía de «Auth».

### Clonar y credenciales (D20)

Hoy los equipos clonan con GitKraken metiendo la URL de Azure DevOps y un PAT. Ariadna debe poder hacer lo mismo sin guardar ella el secreto.

**Clonar** (`clone_repository`): URL + carpeta destino + rama opcional. Reglas de seguridad:
- Esquemas permitidos: `https`, `ssh`, `git@host:ruta` y rutas locales. **Se rechaza `ext::`** y cualquier URL que empiece por `-`; se pasa `-c protocol.ext.allow=never` y `--` antes de la URL.
- El destino debe no existir o estar vacío; si el clon falla o se cancela, Ariadna borra **solo** lo que ella creó.
- Es una operación de red: sin timeout, con cancelación y progreso (`clone-progress`). Los submódulos no se descargan (`--no-recurse-submodules`).
- Al terminar, abre el repo en una pestaña nueva.
- Un proveedor reconocido (D19) rellena el nombre de carpeta (`repo`) y avisa de qué credencial hace falta.

**Credenciales por PAT** (sin que Ariadna guarde nada):
1. Formulario: URL (o host), usuario, token. El campo es de tipo contraseña; el token no se muestra, no se registra, no entra en `ApiError` ni en ningún fixture, y se borra de memoria al terminar (`zeroize`).
2. **Guardar** = `git credential approve` con `protocol/host/username/password` por **stdin** (nunca en argv ni en variables de entorno). Lo almacena el helper del sistema: Git Credential Manager (Windows), Keychain (macOS) o el que el usuario tenga configurado. Si no hay ninguno, se avisa y se ofrece «usar una vez».
3. **Probar conexión** = `git ls-remote` contra la URL, y devuelve éxito o `AUTH`.
4. **Olvidar** = `git credential reject`.
5. **Sin PAT**: si el usuario tiene GCM, basta lanzar fetch/clone y GCM abre su ventana (cuenta Microsoft para Azure DevOps, OAuth para GitHub). Ariadna ofrece las dos vías: «Iniciar sesión con el navegador» y «Usar un token (PAT)».
6. Ayuda por proveedor, con enlace `https` a su página de creación de tokens y los permisos mínimos (Azure DevOps: *Code → Read & write*; GitHub: *repo* o un token de grano fino con *Contents*; GitLab: `read_repository`/`write_repository`; Bitbucket: token de app con *Repositories → Write*). Los textos del usuario para PAT de Azure DevOps: el nombre de usuario puede ser cualquiera no vacío.
7. **Dos cuentas en el mismo host** (p. ej. GitHub personal y de empresa): v0.1 usa el usuario como discriminador (`https://usuario@github.com/...`); un perfil puede recordar qué usuario corresponde a cada repo. Soporte completo de varias cuentas: posterior a v0.1.

**Proveedores soportados de serie** (reconocimiento, icono, ayuda de PAT, PR en el navegador): Azure DevOps, GitHub y GitHub Enterprise, GitLab (incluido autoalojado), Bitbucket. Cualquier otro host funciona igual como `other`.

**Fuera de v0.1:** listado de repos del proveedor para elegir qué clonar, lanzar o ver pipelines, issues y tableros, inicio de sesión con Microsoft (v0.2, D22), PR de GitLab y Bitbucket dentro de la app.

## 4.4 Pull requests (D21, D22)

Alcance: **Azure DevOps y GitHub** (incluido GitHub Enterprise con su host). Para el resto, solo abrir el navegador.

| ID | Requisito [N] |
|---|---|
| PR-01 | Detectar proveedor, organización/proyecto y repo desde el remoto (§4.3). Sin proveedor soportado, el panel de PR ofrece solo «Abrir en el navegador». |
| PR-02 | **Listar** PR: abiertas, creadas por mí, donde soy revisor, y completadas/abandonadas con filtro. Cada fila: título, autor, ramas, estado (borrador, activa, completada, abandonada), votos de revisores, resumen de políticas/comprobaciones y si hay conflictos de merge. |
| PR-03 | **Detalle**: descripción (Markdown **sin `{@html}`**, renderizado desde el AST), autor, ramas origen/destino, revisores y votos, políticas y estado de build, work items vinculados (Azure DevOps), commits y archivos cambiados. |
| PR-04 | **Crear** desde la rama actual: destino por defecto = rama por defecto del remoto; título y descripción rellenados con la plantilla del repo (`.azuredevops/pull_request_template.md` o `pull_request_template.md`; `.github/pull_request_template.md`), revisores, borrador y work items (Azure DevOps). Si la rama no está publicada, ofrece hacer push antes. «Generar título y descripción con IA» como propuesta (M6, `SummaryStyle::PullRequest`). |
| PR-05 | **Bajar la rama** de una PR (fetch + checkout o crear rama de seguimiento), con las reglas de checkout seguro ya existentes. Soporta PR de forks de GitHub (`refs/pull/N/head`). |
| PR-06 | **Revisar**: diff de la PR (merge-base…origen) archivo a archivo con el mismo visor de diff; hilos de comentarios en línea y generales: ver, **crear, responder y resolver**. |
| PR-07 | **Votar**: Azure DevOps (aprobar, aprobar con sugerencias, sin voto, esperar al autor, rechazar). GitHub (aprobar, solicitar cambios, comentar). |
| PR-08 | **Completar / abandonar / reabrir**. Azure DevOps: estrategia (merge sin fast-forward, squash, rebase, rebase + merge), borrar rama origen, completar work items y *auto-complete*. GitHub: merge, squash o rebase. Acciones destructivas con **confirmación nativa** que nombra repo, PR y rama destino, y con las políticas del proveedor siempre por encima (Ariadna no las salta). |
| PR-09 | **Refresco** manual y opcional a intervalo (apagado por defecto, mínimo 60 s) **solo con el panel abierto**. Sin sondeo en segundo plano. |
| PR-10 | **Errores** tipados: `AUTH` con el permiso que falta, `NOT_FOUND`, `POLICY` (conflicto 409 o política incumplida, con el motivo del proveedor), `RATE_LIMITED`, `NETWORK`. |

**Arquitectura.** Crate nuevo `ariadna-hosting`: `RemoteInfo` (§4.3) + `trait PullRequestProvider` con `AzureDevOps` y `GitHub`; cliente HTTP `reqwest` con `rustls`; el trait no conoce Tauri y se prueba con un servidor HTTP simulado (`wiremock`) y respuestas JSON de ejemplo de cada API.

**Autenticación sin guardar secretos (D22).**
- Cada llamada obtiene el secreto con `git credential fill` (stdin/stdout, no interactivo: `GIT_TERMINAL_PROMPT=0` y `GCM_INTERACTIVE=never`). Si el helper no lo tiene, el error es `AUTH` y la UI abre el formulario de credenciales de §4.3.
- El secreto vive solo en memoria durante la petición y se borra después (`zeroize`). No se cachea, no se registra y no aparece en errores ni fixtures.
- `AuthMethod::Pat { username, secret }` envía HTTP Basic (Azure DevOps y GitHub aceptan PAT); `AuthMethod::Bearer(token)` queda definido para el inicio de sesión con Microsoft de la v0.2.
- Permisos mínimos del PAT por operación, mostrados en el formulario (lectura de código para listar y ver; lectura y escritura de código para crear, comentar, votar y completar; lectura de work items si se vinculan). **A verificar en el spike de M0 con un PAT real.**

**Seguridad de red.** TLS siempre verificado; solo se contacta el host del remoto (más `api.github.com` para github.com); redirecciones solo al mismo host; timeout de 30 s; tope de tamaño de respuesta; sin cabeceras en logs. Es la **única** comunicación de red propia de Ariadna y solo ocurre por acción del usuario o con el panel abierto.

**Pruebas.** Servidor simulado con respuestas de ejemplo (listar, detalle, crear, hilos, voto, completar, errores 401/403/404/409/429); prueba de humo **manual** con cuentas reales (no es criterio automático). Los endpoints exactos de Azure DevOps REST 7.x y GitHub REST se confirman en un spike antes de implementar.

## 5. IA

Principio: **la IA solo propone**. Ninguna acción escribe en worktree, índice ni refs. Toda respuesta es una `AiSuggestion` (`pending | accepted | rejected`) ligada a la `revision` del repo y marcada **obsoleta** si el repo cambia.

### 5.1 Consentimiento y privacidad
- Primer uso por **perfil y proveedor**: diálogo que explica qué se envía (diff, mensajes, tamaño) y a quién. Se guarda por perfil y proveedor. Interruptor global para desactivar la IA.
- Vista previa exacta de lo que se enviará antes de enviarlo.
- Se excluyen del contexto `.env*`, `*.pem`, `id_*`, `*.pfx`, `*.key` y se pasa un redactor de secretos con las reglas de `.gitleaks.toml`.
- El contenido del repo se delimita como **dato no fiable** con marcadores y una instrucción de sistema propia. La propuesta de conflicto se rechaza si introduce texto fuera de los hunks originales.
- Tope de contexto 100 000 bytes (configurable); si se supera, no se llama al CLI.
- Registro local opcional de lo enviado.

### 5.1.1 Perfiles Personal y Empresa (D16)

El riesgo a evitar: mandar código de la empresa con una cuenta personal, o código personal por la cuenta de la empresa.

- Un **perfil** tiene: nombre, tipo (`personal` | `empresa`), proveedores habilitados, consentimiento por proveedor, tope de contexto y, si el proveedor lo permite, un **directorio de configuración aislado** para que cada perfil conserve su propio login.
- Cada repo se asocia a un perfil (guardado en Ariadna, nunca en el repo). Reglas sugeridas por patrón del remoto (`github.com/<org-empresa>/*` → Empresa). Un repo sin perfil pregunta en el primer uso de IA. Cambiar el perfil de un repo pide confirmación.
- La pestaña y la hoja de propuesta muestran el **perfil activo** y la **cuenta** que usará cada proveedor, cuando el CLI lo expone (RF-IA12).
- Valores por defecto: **Personal** = Claude, Codex y Copilot habilitados tras consentimiento. **Empresa** = Codex y Copilot habilitados tras consentimiento; Claude **desactivado** (la empresa no lo tiene). El modo Auto del perfil recorre Claude → Codex → Copilot solo entre los habilitados.
- Aislamiento de login por perfil (SPIKE, ver M0): variables como `CLAUDE_CONFIG_DIR` y `CODEX_HOME` podrían permitir dos sesiones en paralelo; para Copilot no está verificado. Si un proveedor no permite aislar, ese proveedor **comparte login entre perfiles**: Ariadna lo avisa y solo controla si se permite, no con qué cuenta. **Resultado del spike (alpha.2):** `CLAUDE_CONFIG_DIR` verificado con `claude 2.1.319` (un directorio vacío da `loggedIn: false` y `claude auth status` informa del directorio usado); `CODEX_HOME` según la documentación, **no verificado**; Copilot sin mecanismo confirmado (`COPILOT_HOME` es solo una candidata) → comparte login. Por defecto **Empresa aísla y Personal usa la sesión normal** del CLI (para no cerrar sesiones existentes); ambos pueden cambiarlo en Ajustes.
- El inicio de sesión de cada perfil lo hace el CLI (botón «Iniciar sesión» que abre una terminal con el entorno del perfil en Windows y macOS, o muestra el comando si no es seguro o posible abrirla; `ai_login`, `ipc.md`). Ariadna no ve credenciales.
- Fuera de v0.1: identidad de commit (autor/email) por perfil.

### 5.2 Proveedores

| Proveedor | Estado de verificación | Invocación |
|---|---|---|
| Claude | **Flags verificados** contra `claude 2.1.319` | `claude -p --output-format json --tools "" --strict-mcp-config --disable-slash-commands --no-session-persistence --permission-mode plan`, prompt por stdin. Probar `--setting-sources ""`. **Prohibido `--bare`**: solo lee `ANTHROPIC_API_KEY` y rompería el login de suscripción |
| Codex | **NO verificado** (CLI no instalado en la máquina de revisión) | Candidato: `codex exec --json --sandbox read-only --skip-git-repo-check -`. Spike en M0 |
| Copilot | **NO verificado** | Candidato: `copilot -p … -s --no-ask-user` con lista blanca de herramientas. Comprobar stdin. Spike en M0 |

Un proveedor sin flags verificados no bloquea `alpha.2`; se publica marcado experimental.

### 5.3 Ejecución
- **RF-IA1:** el prompt va por **stdin**, nunca por argv. Límites: ~32 K en CreateProcess, **8 191 en `.cmd`**. Si un CLI no lee stdin, se reduce el contexto (tope propio) en lugar de usar un fichero que el CLI leería.
- **RF-IA2:** en Windows, `codex` y `copilot` pueden ser shims `.cmd`. Orden: (1) binario nativo si existe (`~/.local/bin/claude.exe`); (2) `node.exe <entrypoint.js>` con parseo tolerante del shim; (3) `.cmd` solo con argumentos constantes de lista blanca y prompt por stdin, con aviso. Rust ≥ 1.77.2. Nada derivado del repo (rama, ruta, mensaje) va como argumento. Test que falla si un `.cmd` recibe un argumento fuera de la lista.
- **RF-IA3:** entorno por **lista blanca** por proveedor. Común: `PATH`, `SystemRoot`, `windir`, `TEMP`, `TMP`, `USERPROFILE`, `HOME`, `APPDATA`, `LOCALAPPDATA`, `ProgramData`, `ComSpec`, `PATHEXT`, `LANG`, `LC_*`, proxies y certificados (`HTTP(S)_PROXY`, `NO_PROXY`, `SSL_CERT_FILE`, `NODE_EXTRA_CA_CERTS`), `CLAUDE_CONFIG_DIR`, `CODEX_HOME`. Se eliminan siempre `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`, `OPENAI_API_KEY`, `OPENAI_BASE_URL`. Comparación sin distinguir mayúsculas. Copilot recibe un token de GitHub solo si el usuario lo declara.
- **RF-IA4:** disponibilidad, versión y sesión se detectan **ejecutando el propio CLI** (`--version`, subcomando de estado si existe, o botón «Probar» con un prompt trivial). No se leen `.credentials.json` ni el llavero.
- **RF-IA5:** directorio de trabajo = carpeta vacía dedicada (`…/Ariadna/ai-sandbox`), sin ficheros de instrucciones, limpiada tras cada llamada.
- **RF-IA6:** Escribir stdin en una tarea aparte y cerrarlo; drenar stdout/stderr en paralelo; tope de salida; sin TTY; un prompt interactivo se trata como error `AiNotLoggedIn`. Timeout duro 120 s con cancelación.
- **RF-IA7:** esquema de parseo por proveedor y versión mínima, con golden files de salida real. Formato desconocido → «formato no reconocido», nunca texto crudo a la UI.
- **RF-IA8:** Cola de IA FIFO global con **2 llamadas simultáneas** y límite por minuto. Estado visible (en cola, generando, cancelar). Se cancela al cerrar la pestaña.
- **RF-IA9:** Auto salta de proveedor solo si el anterior **no está instalado**, y solo entre proveedores con consentimiento. No hay fallback por error de contenido, sesión o cuota. La hoja de propuesta muestra qué proveedor respondió.
- **RF-IA10:** términos de uso. Ariadna solo lanza el CLI oficial con la sesión del propio usuario y no toca sus credenciales. Antes de `alpha.2` se leen y citan con fecha los términos vigentes de Anthropic, OpenAI y GitHub como tarea de cumplimiento; **no bloquea el desarrollo**. El README avisa de que el usuario es responsable de cumplir las condiciones de su cuenta y de las reglas de su empresa.
- **RF-IA11 (política administrable, POSTERIOR a v0.1):** un archivo de política opcional con campo `appliesTo` (`empresa` | `todos`), de solo lectura para el usuario (`/Library/Application Support/Ariadna/policy.json` en macOS, `%ProgramData%\Ariadna\policy.json` en Windows), que permite: desactivar la IA, restringir los proveedores permitidos, fijar el tope de contexto y obligar a la vista previa. Si existe, manda sobre los ajustes del usuario y la UI lo indica. Es desplegable con MDM/Intune/GPO.
- **RF-IA12 (cuentas de organización):** el login lo hace siempre el CLI oficial (SSO incluido). Ajustes y la hoja de propuesta muestran qué cuenta/organización usa cada proveedor cuando el CLI lo expone (`ai_accounts`; hoy solo `claude auth status --json`, verificado; Codex y Copilot: «no disponible»), para evitar mandar código de empresa con una cuenta personal. Las pruebas de humo se hacen con cuentas de organización.

## 6. Seguridad de la aplicación

- **Confirmaciones:** el diálogo lo abre **Rust** (plugin dialog nativo). Los comandos destructivos no reciben ninguna prueba de confirmación desde la UI. Internamente `Confirmed` es un token de un solo uso ligado a `(session_id, operación, revision, hash de parámetros)` con caducidad; el backend comprueba la revisión antes de ejecutar. El diálogo muestra repo, rama y ruta, y bloquea el cambio de pestaña.
- **WebView:** CSP estricta sin `unsafe-eval`, capabilities mínimas por ventana, sin plugin shell ni fs, sin `{@html}` salvo sanitizado. Todo texto de git o IA se escapa. Test con repo malicioso (nombres de rama con HTML, secuencias de control, `.git/config` con `core.fsmonitor`/`core.sshCommand`).
- Entorno de `git`: igual que el Swift (`ProcessEnvironment.sanitized`), **lista blanca** de nombres (`PATH`, `HOME`, `LANG`, `LC_*`, `TMP*`, `SSH_*`, `GIT_*`…) y, de las que pasan, se quitan las que contengan `TOKEN`, `SECRET`, `PASSWORD`, `PASSWD` o `API_KEY`. En Windows se añaden las que exige el sistema (`SystemRoot`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `ComSpec`, `PATHEXT`…) y `GCM_*` para el Git Credential Manager. Implementado en `EnvPolicy::git_default`. Los hijos de **IA** usan su propia lista blanca (§5.3).
- No se escribe `core.hooksPath` ni ninguna configuración en repos ajenos.
- Repo público: `.gitignore` ampliado (`target/`, `node_modules/`), gitleaks en pre-commit y CI.

## 7. Diseño visual

«Moderno» se define con propiedades verificables:

1. Tokens de diseño en un único archivo: escala de espaciado 4/8 px, 3 radios, 2 elevaciones, paleta con ratios de contraste calculados (≥ 4,5:1 texto, ≥ 3:1 UI).
2. Tema claro, oscuro y sistema, con transición de 150 ms o menos; respeta `prefers-reduced-motion`.
3. Tipografía Inter (UI) y JetBrains Mono (código), incluidas en la app.
4. Barra de título integrada; fondo sólido en v0.1 (D11).
5. Densidad cómoda y compacta, configurable.
6. Iconos de un único conjunto vectorial; estados vacío, cargando y error diseñados en cada pantalla.
7. Paleta de comandos (⌘K / Ctrl+K).
8. Mockups aprobados de las 5 pantallas clave (Bienvenida, Cambios+Diff, Historial, Conflictos, Propuesta de IA) **antes de M5**.

Compatibilidad: matriz con versión mínima de WebView2 (Evergreen) y WebKit de macOS 13; `browserslist` y lint de CSS que prohíben APIs fuera de ella. Se decide Ctrl+Tab frente a Ctrl+PgDn tras probar WebView2.

Accesibilidad: roles `tablist/tab/tabpanel` con reorden por teclado, listas virtualizadas con `grid/listbox`, `aria-live` para operaciones, trampa de foco en diálogos, `forced-colors` y `prefers-contrast`, zoom de texto al 200 %, objetivos ≥ 24 px. Revisión manual con NVDA y VoiceOver y chequeo automático axe en E2E.

Atajos: registro único con ámbito (global/pestaña/editor), detección de conflictos en tests, menú nativo de Tauri donde se pueda, teclados ES/US/DE.

## 8. Pestañas por repo

- Una ventana, N pestañas, una `RepositorySession` por repo (no por pestaña): si el mismo repo (según §3.2) se abre otra vez, se enfoca la pestaña existente. Worktrees comparten clave; se avisa.
- Estado por pestaña: panel, selección, borrador de commit (autoguardado), filtros y propuestas de IA.
- **Pestañas ocultas hibernan:** la UI pesada (diff, historial) se desmonta y se rehidrata al activar. En Rust queda estado ligero. Hibernación completa tras 10 min de inactividad.
- `session.json`: versionado, escritura atómica (tmp + rename), copia de respaldo, recuperación si está corrupto. Aviso si una ruta ya no existe.
- Vigilancia de archivos (`notify`): debounce 300 ms con tope de 2 s; worktree con filtro de ignorados; `.git/{HEAD,index,refs}` aparte; se ignoran los eventos de nuestras propias mutaciones; ante overflow o error se hace refresco completo y se degrada a sondeo (también en UNC/red). **Solo la pestaña visible** vigila a fondo; las demás se marcan «sucias» y refrescan al activarse. Estado ligero: `status --porcelain=v2 -z --no-renames -uno` con límite de 5 s, cada 60 s como máximo.
- Atajos: Ctrl/⌘+T, Ctrl/⌘+W, Ctrl/⌘+1…9, siguiente/anterior (ver §7).
- Cerrar con propuesta de IA sin descartar o borrador sin commitear pide confirmación.
- Tope blando de 20 pestañas con aviso.

## 9. Requisitos no funcionales (con método de medida)

Entorno de referencia: runners `macos-15` y `windows-2025` más un portátil de gama media para medición manual. Fixtures sintéticos de repos grandes en CI con umbral de regresión.

| ID | Métrica | Objetivo |
|---|---|---|
| NF-1 | Arranque en frío hasta ventana interactiva | ≤ 1,5 s |
| NF-2 | Primer pintado de `status` con datos parciales | ≤ 300 ms |
| NF-3 | `status` completo, 50 000 archivos | ≤ 2 s, sin bloquear UI |
| NF-4 | Historial: 100 000 commits | Paginado por 200, scroll a 60 fps |
| NF-5 | Diff grande | > 1 MB o > 5 000 líneas: colapsado y carga bajo demanda; binarios detectados; líneas largas truncadas |
| NF-6 | 20 pestañas | Memoria acotada por hibernación; sin agotar manejadores de watcher |
| NF-7 | Cancelación | Tiempo hasta terminar el árbol de procesos ≤ 5 s |
| NF-8 | Sin telemetría. La única red propia es la API del proveedor del repo (PR), por acción del usuario | |

Procesos: Job Object de Windows con `KILL_ON_JOB_CLOSE`, proceso creado suspendido y asignado antes de reanudar, `CREATE_NO_WINDOW`; en macOS, grupo de procesos con `SIGTERM`, espera y `SIGKILL`. Tope de bytes al capturar salida y *streaming* de diffs.

## 10. Distribución y CI

- CI: `macos-15`, `windows-2025`, `windows-11-arm`, `ubuntu` (núcleo). `cargo fmt --check`, `clippy -D warnings`, `cargo deny`, `cargo test`, gitleaks, build de UI, `vitest`, `axe` en E2E, regresión visual con baseline por SO (tolerancia 0,1 %). Un **job agregador** siempre corre. Caché de cargo.
- Git mínimo (2.39) y último, ambos en CI.
- macOS: `.dmg` universal, firma ad-hoc, notarización opcional con los secrets actuales. Se documenta cómo abrir una app no notarizada y se acepta ese límite en v0.1.
- Windows: instalador **NSIS** por usuario (sin administrador), x64 y ARM64 (mejor esfuerzo). WebView2: presente en Windows 11; el instalador lo comprueba y lo instala si falta. Prueba de instalación, actualización y desinstalación limpias en VM. MSI para despliegue gestionado: posterior a v0.1.
- **Firma de código (D17).** No hay certificado. Sin firma, SmartScreen avisa y los equipos de empresa pueden bloquear el binario. Las alfas salen sin firmar con aviso. Para `v0.1.0` se elige entre (datos de Microsoft Learn consultados al redactar; revisar antes de decidir):

| Opción | Coste aprox. | Condiciones | Encaja |
|---|---|---|---|
| **SignPath Foundation** | Gratis para código abierto | Requiere proyecto OSS: el repo es público y desde D18 tiene licencia MIT. Otras condiciones por leer en signpath.org | **Elegida (D17)** |
| **Azure Artifact Signing** | ~10 USD/mes | Organizaciones en UE/EE. UU./Canadá/Reino Unido con 3+ años de historial fiscal; **particulares solo EE. UU. y Canadá** (no aplica a un particular en España). Suscripción de Azure de pago | Solo si el titular es una empresa elegible |
| **Certificado OV** de una CA | 150–300 USD/año | Token HSM o HSM en la nube; validación de identidad | Particulares fuera de EE. UU./Canadá |
| Sin firmar | 0 | SmartScreen avisa; empresas pueden bloquear | Solo alfas |

macOS: Developer ID y notarización requieren Apple Developer Program (de pago). Sin él, firma ad-hoc con el procedimiento de «Abrir igualmente»; los Mac gestionados pueden bloquearlo.
- Propuesta (D17): alfas sin firmar; `v0.1.0` con SignPath Foundation; si no se concede, un OV o sin firmar con aviso.
- Sin telemetría. Lo único que sale del equipo es lo que hacen `git`, los CLI de IA y las llamadas a la API del proveedor del repo para las pull requests (§4.4), siempre por acción del usuario.
- Política de ramas: fusionar `main` en `rust-migration` periódicamente.
- Al lograr la paridad: tag `swift-final`, rama `legacy-swift`, borrar Swift de `main`, README nuevo.

## 11. Pruebas

- Inventario de los **58** tests de `GitCoreTests` y los 8 de `AriadnaAITests` (66 en total; el «36 + 8» del README estaba desactualizado) en [trazabilidad.md](trazabilidad.md). Criterio de salida de M2 y M3: cada test Swift del área tiene su equivalente Rust o una justificación escrita.
- **La fuente de verdad es git.** Fixtures de parsers generados con git real y contrastados con el Swift; versionados con metadatos (versión de git, SO) y regenerables con un script Rust.
- Tests de sesiones: cola, cancelación, cierre durante push/rebase/IA, refresco concurrente, confirmar tras cambiar de pestaña, repo borrado, 20 sesiones.
- Fuzz/propiedades de los parsers.
- IA en CI con un **CLI falso** que vuelca entorno, argumentos y stdin: comprueba lista blanca de entorno, orden de flags, nada de datos del usuario por argv, y que un secreto de prueba no llega al hijo.
- Humo con CLI reales: job manual con checklist; **no** es criterio de salida automático.
- Frontend: Vitest para componentes; E2E con `tauri-driver`.

## 12. Riesgos

| Riesgo | Mitigación |
|---|---|
| Alcance grande + aprender Rust, Tauri y Svelte | Hitos alfa (D13), núcleo primero, UI delgada |
| Flags de Codex/Copilot sin verificar | Spike M0, experimental si no se verifica |
| Términos de uso de suscripciones en la empresa | RF-IA10 antes de `alpha.2`; IA opt-in y desactivable por política (RF-IA11) |
| Shims `.cmd` y límites de línea | RF-IA1, RF-IA2 |
| Repos hostiles ejecutan código vía git | §4.1 |
| WebView2/WKWebView difieren | Matriz, lint CSS, regresión visual |
| Alcance: clonado, credenciales y PR completas (dos proveedores) en la misma v0.1 | Hitos alfa: `alpha.3` tras M8. **Regla de corte (PROPUESTA):** si el calendario se rompe, revisar/comentar/votar/completar pasan a v0.2 antes que retrasar `v0.1.0`; crear, listar, detalle y bajar rama no se recortan |
| APIs de Azure DevOps y GitHub cambian o requieren más permisos de PAT | Spike con PAT real en M0, versión de API fijada, pruebas contra servidor simulado |
| Calidad de ARM64 sin hardware real | Mejor esfuerzo (D7) |
| Binarios sin firmar bloqueados en equipos gestionados | Firma por etapas (D17); documentar el aviso |
| Código de empresa enviado con cuenta personal | RF-IA12 y RF-IA11 |
| Sandbox de Codex en Windows distinto al de Unix | Humo manual de escritura/ejecución; avisar si no hay garantía |

## 13. Preguntas abiertas para Javier
Ninguna bloquea M0. Quedan por confirmar las **PROPUESTAS**: D7 (ARM64 mejor esfuerzo), D10 (IA siempre en el binario, apagada hasta consentir), D11 (sin Mica en 0.1), D13 (alfas), D16 (perfiles Personal/Empresa) y D17 (SignPath). Si no se vetan en M0, pasan a cerradas.
