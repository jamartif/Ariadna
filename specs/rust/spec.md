# Ariadna en Rust — especificación del producto 0.1

Ariadna es un cliente Git de escritorio para **macOS y Windows**, con sugerencias de IA obtenidas ejecutando los CLI oficiales (Claude, Codex, Copilot). Sustituye a la aplicación Swift (`Sources/`, `Tests/`, `Package.swift`), que se conserva como **oráculo de comportamiento** (ver glosario) hasta alcanzar la paridad.

Uso previsto: **particular y profesional** en la misma instalación (D14, D16). Este documento describe la **versión 0.1** del producto; las versiones `v0.1.0-alpha.1`, `v0.1.0-alpha.2`, `v0.1.0-alpha.3` y `v0.1.0` existen como etiquetas del repositorio y su contenido real figura en §1.2 y en [CHANGELOG.md](../../CHANGELOG.md).

**Revisiones del documento.** La revisión 1 salió de un proceso EGM (Elephant-Goldfish Model) con tres revisores sin contexto ([revision-egm.md](revision-egm.md)). La revisión 2 incorporó el estado real del código tras `alpha.2`. La **revisión 3** aplica una revisión formal con speckfish (Spec Kit + Elephant-Goldfish, siete subagentes) cuyos hallazgos y su resolución están en [revision-speckfish.md](revision-speckfish.md). En este documento, «decidido» significa que hay una decisión registrada; «implementado» solo se afirma cuando el código de `crates/`, `src-tauri/` o `ui/` lo hace hoy.

## 0. Convenciones

### 0.1 Glosario

El documento usa siglas y jerga propia. Esta tabla las define una vez; el resto del texto las usa sin volver a explicarlas.

| Término | Significado |
|---|---|
| **[P] / [N]** | Requisito de **paridad** con la aplicación Swift / **novedad** que el Swift no tenía. |
| **ADO** | Azure DevOps (Azure DevOps Services, `dev.azure.com`). |
| **GCM** | Git Credential Manager: el ayudante de credenciales de git que Git for Windows instala y que en macOS usa el llavero. Guarda y devuelve credenciales, y puede abrir una ventana de inicio de sesión (Microsoft/Entra ID, GitHub). |
| **PAT** | *Personal Access Token*: token de larga duración que el usuario crea en el servidor con permisos concretos. Se usa como contraseña HTTP Basic. |
| **Entra ID** | El servicio de identidad de Microsoft (antes Azure AD). Su token OAuth **no** es un PAT. |
| **GHE / GHES** | GitHub Enterprise Cloud (`*.ghe.com`) / GitHub Enterprise Server (host propio). |
| **OV** | Certificado de firma de código con validación de organización, emitido por una autoridad certificadora comercial. |
| **Oráculo** | La aplicación Swift: sus pruebas y su comportamiento observable son la referencia de qué debe hacer Rust. No es la fuente de verdad de los datos (esa es `git`). |
| **Paridad** | Que cada capacidad y cada prueba del Swift tenga equivalente en Rust, o una justificación escrita de por qué no. |
| **Hunk** | Bloque contiguo de líneas cambiadas dentro de un diff. |
| **Preparar / quitar (stage / unstage)** | Mover cambios al índice de git o sacarlos de él. |
| **Stash** | Guardado aparte de cambios sin confirmar (`git stash`), para recuperarlos después. |
| **Worktree** | Árbol de trabajo adicional de un mismo repositorio (`git worktree`). Comparte el directorio git y tiene su propia ruta. |
| **Snapshot** | Copia inmutable del estado de un repo (cambios, ramas, historial…) que Rust envía a la interfaz. |
| **`revision`** | Contador monótono que sube con cada snapshot. La interfaz descarta revisiones antiguas. |
| **Coalescer** | Agrupar varias peticiones de refresco seguidas en una sola ejecución. |
| **Hibernar (pestaña)** | Desmontar la parte pesada de la interfaz de una pestaña oculta y reconstruirla al activarla. |
| **Delta por sección** | Evento que envía solo la parte del snapshot que cambió (cambios, ramas o una página de historial). |
| **Spike** | Experimento acotado para verificar una hipótesis antes de implementarla (por ejemplo, los flags reales de un CLI). |
| **Fixture** | Entrada de prueba guardada en el repositorio (salida de `git`, respuesta JSON de una API). |
| **Golden file** | Fixture que se compara byte a byte con la salida actual; si cambia, la prueba falla. |
| **Shim** | En Windows, fichero `.cmd`/`.ps1` que los envoltorios de npm dejan en lugar del ejecutable real. |
| **RF-IA / NF** | Requisito funcional del bloque de IA / requisito no funcional (rendimiento, cancelación). |
| **M0…M8, `alpha.N`** | Hitos de desarrollo y versiones alfa publicables (§1.2). |
| **EGM / speckfish** | Métodos de revisión de esta especificación: revisores con contexto limpio sobre un borrador. |

### 0.2 Documentos citados y qué se toma de cada uno

Ningún requisito de esta especificación depende de leer otro documento: lo externo se resume aquí y la ruta exacta se da para ampliar. Si un documento citado desapareciera, la especificación seguiría siendo implementable.

| Documento | Ruta | Qué se toma |
|---|---|---|
| Plan de migración | [plan.md](plan.md) | Estructura del repositorio, orden de los hitos y dependencias entre ellos. |
| Tareas | [tasks.md](tasks.md) | Estado de ejecución de cada tarea. No es normativo. |
| Contrato IPC | [ipc.md](ipc.md) | Nombres y formas de comandos y eventos. Si discrepa de §3.4, manda §3.4 y se corrige `ipc.md`. |
| Trazabilidad de pruebas | [trazabilidad.md](trazabilidad.md) | Inventario de las 58 pruebas de `GitCoreTests` y las 8 de `AriadnaAITests`. |
| Revisión EGM | [revision-egm.md](revision-egm.md) | Historial de la revisión 1. |
| Revisión speckfish | [revision-speckfish.md](revision-speckfish.md) | Hallazgos de la revisión 3 y su resolución, con las decisiones de diseño que se citan aquí. |
| Spike de PR | [spike-pr.md](spike-pr.md) | Qué está verificado contra GitHub real y qué sigue sin verificar (todo Azure DevOps). |
| Términos de uso de los CLI | [terminos-ia.md](terminos-ia.md) | Extractos y fecha de consulta de las condiciones de Anthropic, OpenAI y GitHub. |
| Medidas de rendimiento | [rendimiento.md](rendimiento.md) | Números medidos frente a NF-3, NF-4 y NF-5. |
| Contraste y accesibilidad | [contraste.md](contraste.md) | Pares de color medidos y su ratio. |
| Especificación del Swift | [../spec.md](../spec.md) | Comportamiento del oráculo (requisitos `G-*` y pruebas). |
| Firma de código | [../../docs/firma-de-codigo.md](../../docs/firma-de-codigo.md) | Detalle de las opciones de firma de §10. |
| Privacidad | [../../docs/privacidad.md](../../docs/privacidad.md) | Qué se envía a cada proveedor de IA. |

### 0.3 Marcas de estado

- **Implementado**: el código actual lo hace; hay prueba o medida que lo respalda.
- **Decidido, no implementado**: hay decisión cerrada y el código todavía no la cumple; la tarea figura en `tasks.md`.
- **Pendiente**: sin decisión; solo se admite en §13 (preguntas abiertas) o en los riesgos.

## 1. Objetivo y alcance

**Objetivo.** Un cliente Git de escritorio que haga el trabajo diario —ver cambios, preparar, confirmar, ramas, remotos, conflictos— sin depender de la terminal, con la misma funcionalidad que la aplicación Swift más Windows, pestañas por repositorio y sugerencias de IA que nunca escriben por su cuenta.

**Criterio de éxito de la versión 0.1:** cada prueba del Swift tiene equivalente en Rust ([trazabilidad.md](trazabilidad.md)), la aplicación se instala y funciona en macOS 15+ y Windows 10/11 x64, y ninguna operación destructiva ocurre sin confirmación nativa.

**Plataformas:** macOS 15+ (universal arm64 y x86_64), Windows 10 22H2 y Windows 11 x64 (oficiales) y Windows 11 ARM64 (compila, núcleo probado en CI y publicado, sin garantía de IA ni de E2E, D7).

### 1.1 Alcance real de v0.1

Lo que entra, con la versión en la que ya está y la marca de paridad. Esta tabla **sustituye** a la regla anterior «paridad y nada más»: la versión 0.1 incluye tres novedades grandes (Windows, pestañas y PR) y lo declara de forma explícita, en lugar de contradecirse.

| Área | Marca | Estado en v0.1 | Detalle |
|---|---|---|---|
| Abrir repo, estado, preparar/quitar por archivo y por hunk, descartar, commit, amend seguro, log, búsqueda en todo el historial, diff | [P] | Publicado en `alpha.1` | §4, G-01…G-09 |
| Ramas, fetch/pull/push, merge ff/no-ff/ya al día, conflictos a tres vías, continuar/abortar | [P] | `alpha.1` | G-10…G-14 |
| Recientes y favoritos | [P] | `alpha.1`; mismo formato y ruta que el `recents.json` del Swift | G-15 |
| IA: mensaje de commit, resumen, explicar commit, propuesta de conflicto | [P] | `alpha.2`, con Copilot y Codex tras consentimiento | §5 |
| IA con Claude | [N] | `alpha.2`; flags verificados contra `claude 2.1.319` | §5.2 |
| Modo Auto (recorre proveedores habilitados y consentidos) | [N] | `alpha.2`. **Cambia el comportamiento del Swift**: salta solo si el CLI no está instalado (RF-IA9) | §5.3 |
| Consentimiento por perfil y proveedor; ajustes por proveedor | [N] | `alpha.2` | §5.1 |
| Pestañas por repo, hibernación, vigilancia de archivos | [N] | `alpha.1` | §8 |
| Windows 10/11 x64 | [N] | `alpha.1` | §4.2, §10 |
| Windows 11 ARM64 | [N] | `v0.1.0`: compila, núcleo probado, publicado; sin garantía de IA ni E2E | D7 |
| Tema, paleta de comandos, accesibilidad | [N] | `alpha.1` y `v0.1.0` | §7 |
| Clonar desde URL con PAT de un solo uso, o con la sesión del GCM | [N] | `alpha.1` | §4.3.1, D20 |
| Preparar/quitar líneas sueltas dentro de un hunk; stash (guardar, listar, aplicar, sacar, borrar, ver diff) | [N] | `alpha.3` | G-17, G-18, [ipc.md](ipc.md) «Líneas sueltas y stash» |
| Pull requests de Azure DevOps y GitHub: crear, listar, detalle, bajar rama, revisar y comentar, votar, completar/abandonar | [N] | `alpha.3` (GitHub verificado contra servidor real; Azure DevOps solo contra simulación) | §4.3.2, D21 |
| Etiquetas (listar, crear, borrar, publicar) | [N] | `alpha.3` | `crates/ariadna-git/src/tag.rs` |

**Fuera de v0.1** (y en su mayoría también fuera del Swift): `init`, submódulos, LFS, rebase interactivo, cherry-pick visual, grafo de commits, diff lado a lado, actualizador automático, tiendas de aplicaciones, listado de repos del proveedor, pipelines, issues y tableros. **Worktrees:** solo se detectan y se avisa (§8). **PR de GitLab y Bitbucket:** solo «abrir en el navegador».

**Regla de corte, con prioridad explícita.** Si el calendario aprieta, se recorta primero una novedad, nunca una paridad. Las PR ya están publicadas en `alpha.3`, así que la regla se aplica a lo que queda por delante: revisar, comentar, votar y completar podrían retirarse a `v0.2` sin afectar a la paridad, y `alpha.1` y `alpha.2` ya demostraron que la paridad y la IA no dependen de ellas.

### 1.2 Hitos y alfas

| Hito | Contenido | Criterio de salida | Estado |
|---|---|---|---|
| **M0** | Licencia MIT, solicitud de SignPath, spikes (flags de IA reales, Mica descartado, WebView2, PR), matriz de trazabilidad, inventario de pruebas, términos de uso, mockups base | Cero hallazgos altos abiertos; spikes documentados | Cerrado |
| **M1** | Espacio de trabajo Cargo, CI en cuatro sistemas, `ProcessRunner`, localización de git, `PathSafety`, parsers y fixtures | Pruebas de parser y de proceso en verde en macOS, Windows y Linux | Cerrado |
| **M2** | `GitEngine`: estado, preparar, commit, amend, log, búsqueda, diff; contrato de comandos, errores y eventos | Pruebas portadas = pruebas del Swift del área | Cerrado |
| **M3** | Ramas, remotos, merge, conflictos a tres vías, continuar y abortar | Ídem | Cerrado |
| **M4** | Shell Tauri sobre `GitServing`: ventana, confirmaciones, bienvenida, pestañas, sesión persistente, vigilante | Varios repos en pestañas, restaurados al reabrir | Cerrado |
| **M5** | Pantallas: cambios y diff por hunk, commit, historial, ramas, conflictos | **`v0.1.0-alpha.1`**: Git completo sin IA, macOS y Windows | Cerrado (2026-11-10) |
| **M6** | IA: trait, Claude, Codex, Copilot, consentimiento, entrada estándar, shims, perfiles aislados, ajustes, hoja de propuesta | **`v0.1.0-alpha.2`**: IA con CLI falso en CI y humo manual | Cerrado (2026-11-11) |
| **M8** | Pull requests de Azure DevOps y GitHub | **`v0.1.0-alpha.3`** | Cerrado; los endpoints de Azure DevOps no están verificados contra servidor real |
| **M7** | Pulido, accesibilidad, rendimiento, instaladores, publicación | **`v0.1.0`** | Publicado |

## 2. Decisiones

Estados posibles, sin ambigüedad: **Cerrada** (vigente), **Propuesta** (decidida por el equipo de producto, pendiente de confirmación de la dirección; si no hay objeción antes del cierre del hito en curso, pasa a cerrada) y **Sustituida** (reemplazada por otra; se conserva el rastro). El orden de la tabla es el del identificador, no el de adopción.

| # | Decisión | Estado |
|---|---|---|
| D1 | Alcance de v0.1 = tabla de §1.1: paridad con el Swift, más Windows, pestañas, IA con perfiles, clonado con PAT, stash y PR de Azure DevOps y GitHub, con la regla de corte | **Cerrada** (revisión 3) |
| D2 | IA ejecutando los CLI oficiales `claude`, `codex` y `copilot`. Sin claves de API ni proxies | Cerrada |
| D3 | Tauri 2 + Svelte 5 + TypeScript. Núcleo en Rust. Editor de diff CodeMirror 6. Tipografías Inter y JetBrains Mono | Cerrada |
| D4 | Mismo repositorio `jamartif/Ariadna`, rama `rust-migration`. Se fusiona a `main` al alcanzar la paridad | Cerrada |
| D5 | El `git` del sistema, versión mínima **2.39** | Cerrada |
| D6 | Fuera de las tiendas de aplicaciones | Cerrada |
| D7 | Windows en v0.1. ARM64 = mejor esfuerzo (compila, núcleo probado en CI, publicado) | Windows: cerrada. ARM64: **Propuesta** |
| D8 | Modo Auto: recorre Claude → Codex → Copilot entre los proveedores habilitados y consentidos, y **solo salta al siguiente cuando el CLI no está instalado** (RF-IA9) | Cerrada |
| D9 | Pestañas por repositorio | Cerrada |
| D10 | La IA va siempre en el binario, apagada hasta el consentimiento. Un único interruptor global la desactiva del todo y manda sobre cualquier otro ajuste | **Propuesta** (sustituye al flag de entorno `ARIADNA_AI`) |
| D11 | Sin Mica ni vibrancy en v0.1: fondo sólido con diseño propio. Mica y vibrancy en v0.2 | **Propuesta** |
| D12 | Instalador de Windows: NSIS por usuario, con arranque que comprueba e instala WebView2 | Cerrada |
| D13 | Hitos y alfas publicables: `alpha.1` tras M5 (Git sin IA), `alpha.2` tras M6 (IA), `alpha.3` tras M8 (PR) y `v0.1.0` tras M7 | **Propuesta** (las tres alfas y `v0.1.0` ya existen) |
| D14 | Doble uso, particular y profesional. Ariadna usa los CLI con la sesión que el usuario ya tiene y **no gestiona planes ni condiciones de cuenta**. Sin proxy ni certificado de firma corporativos. **No se diseña para despliegue gestionado por TI** | Cerrada |
| D15 | macOS mínimo **15** (igual que el Swift) | Cerrada |
| D16 | Perfiles **Personal** y **Empresa**; cada repo se asocia a uno y el perfil decide qué proveedores se permiten. Personal: los tres. **Empresa: IA desactivada por defecto**, porque el único proveedor con aislamiento verificado (Claude) no está disponible en la empresa y Codex/Copilot comparten sesión con Personal; se pueden habilitar aceptando por escrito el aviso de sesión compartida | **Propuesta** (revisión 3; antes «Empresa = Codex y Copilot») |
| D17 | Firma de código por etapas: las alfas salen sin firmar con aviso; para `v0.1.0`, solicitar **SignPath Foundation** (gratis para código abierto con licencia OSI). Si no se concede, se publica sin firmar con la guía de «Abrir igualmente» o con un certificado OV | **Propuesta** |
| D18 | Licencia **MIT** (`LICENSE` en la raíz) | Cerrada |
| D19 | **Alojamiento remoto agnóstico.** Todo el control de versiones va por `git`, así que sirve Azure DevOps, GitHub, GitLab, Bitbucket o un servidor propio. Ariadna reconoce el proveedor por la URL para sugerir perfil, mostrar su icono y abrir el navegador | **Propuesta** |
| D20 | **Clonar y autenticar desde la app.** El PAT se introduce una vez y **no se guarda**: se entrega a git por un `GIT_ASKPASS` temporal, sin `credential approve`. Guardarlo en el gestor del sistema es una acción aparte y explícita. También se puede clonar sin PAT usando la sesión del GCM (ventana del sistema) | **Propuesta** (sustituye a «clonar fuera de v0.1») |
| D21 | **Pull requests completas en v0.1** para Azure DevOps y GitHub: crear, listar, detalle, bajar rama, revisar (diff e hilos), votar y completar/abandonar. GitLab y Bitbucket: solo «abrir en el navegador» | Cerrada; el alcance completo es decisión de producto y figura como riesgo en §12 |
| D22 | El cliente de proveedor abstrae la autenticación (`AuthMethod`) para poder añadir el **inicio de sesión con Microsoft (Entra ID)** en `v0.2` sin reescribir el cliente. **En v0.1 solo se usa `AuthMethod::Pat`**: `AuthMethod::Bearer` existe en el tipo y no se usa | Cerrada |
| D23 | **Clonado atómico**: se clona en un directorio temporal hermano del destino y se renombra al terminar; un registro de clones en curso permite limpiar restos al arrancar | **Propuesta** (revisión 3) |
| D24 | **Las PR se autentican solo con PAT**, nunca con el token OAuth del GCM (§4.3.2). En Azure DevOps la interfaz lo dice antes de la primera llamada | **Cerrada** (revisión 3, resuelve el bloqueo B2) |
| D25 | Estado ligero y bloqueos: las lecturas usan `--no-optional-locks` y `GIT_OPTIONAL_LOCKS=0`; un `index.lock` ajeno se reintenta y solo se ofrece borrarlo con confirmación cuando está demostrado que está huérfano | **Propuesta** (revisión 3) |
| D26 | Cola de IA: 2 llamadas simultáneas, 20 por minuto, **una por repositorio**, sin reservas ni prioridad especial; sin reintento ni cambio de proveedor por cuota agotada | **Propuesta** (revisión 3) |

## 3. Arquitectura

```
UI Svelte ─IPC tipado─▶ comando Tauri ─▶ AppCore ─▶ RepositorySession ─▶ GitEngine ─▶ ProcessRunner ─▶ git
                                             └────▶ AiService ─▶ AiProvider ─▶ ProcessRunner ─▶ claude | codex | copilot
                                             └────▶ PullRequestProvider (ariadna-hosting) ─▶ API REST del proveedor
```

Espacio de trabajo Cargo: `crates/ariadna-process` (lanzador de procesos), `crates/ariadna-git` (motor Git y sesión), `crates/ariadna-ai` (CLI de IA), `crates/ariadna-hosting` (reconocimiento de remotos y PR), `src-tauri` (aplicación Tauri) y `ui` (Svelte).

### 3.1 Componentes

Cada componente tiene una responsabilidad y un dueño claro del estado. Los nombres son los del código.

| Componente | Responsabilidad (una frase) | Dónde vive |
|---|---|---|
| **`ProcessRunner`** | Lanza un proceso hijo con entorno saneado, límite de tiempo opcional, cancelación y muerte del árbol completo, y devuelve los bytes de salida sin interpretarlos. | `crates/ariadna-process` (`runner.rs`) |
| **`GitEngine`** | Única puerta a `git`: compone los argumentos endurecidos de §4.1, ejecuta y convierte porcelain, diffs y salida en tipos; no conoce Tauri ni la interfaz. | `crates/ariadna-git` (`GitCliEngine`) |
| **`GitServing`** | Trait con las operaciones que consume la aplicación; permite sustituir el motor por un doble en las pruebas de sesiones e interfaz. | `crates/ariadna-git/src/serving.rs` |
| **`RepositorySession`** | Estado de un repositorio abierto: snapshot, `revision`, cola de mutaciones en serie, refresco coalescido, ciclo de vida y `session_id`. | `crates/ariadna-git/src/session.rs` |
| **`AppCore`** | Registro de pestañas y sesiones, ciclo de vida de la ventana, activación, vigilantes y despacho de comandos IPC; hace el papel que el borrador llamaba «SessionManager». | `src-tauri/src/core/app.rs` |
| **`AiService`** | Consentimiento, perfiles, construcción del prompt, invocación del proveedor, interpretación de la respuesta y cola de llamadas; nunca escribe en el repositorio. | `crates/ariadna-ai/src/orchestrator.rs` |
| **`AiProvider`** | Trait de un CLI concreto: flags, entorno permitido, cómo se lanza y cómo se interpreta su salida. | `crates/ariadna-ai/src/provider.rs` |
| **`PullRequestProvider`** | Trait de la API REST de un proveedor: listar, detalle, crear, comentar, votar y completar; pide la credencial en cada operación y no la guarda. | `crates/ariadna-hosting` |
| **`PathSafety`** | Valida rutas y refs antes de tocar el sistema de archivos o de pasarlas a `git` (§4.2). | `crates/ariadna-git/src/path_safety.rs` |
| **`EnvPolicy`** | Lista blanca de entorno para los hijos: `git_default` para git y una por proveedor para la IA (§5.3). | `crates/ariadna-process`, `crates/ariadna-ai/src/env.rs` |

**Regla de oro: el estado vive en Rust.** La interfaz es una vista, sin lógica de git ni de IA. Equivalencias con el Swift: `protocol` → `trait`; `actor` → tarea de tokio con canal; `@MainActor` → no aplica (la interfaz no guarda estado propio); `UserConfirmation.require` → §6.

### 3.2 Ciclo de vida de `RepositorySession`

`Abriendo → Activa → Cerrando → Cerrada`.

- Cerrar una pestaña, cerrar la ventana y salir de la aplicación siguen el mismo camino.
- Si hay una mutación en curso o en cola, **pide confirmación** nombrando repo y operación. Tras confirmar: cancelación cooperativa, plazo de gracia de 5 s y terminación del árbol de procesos. La cola se vacía. Los eventos tardíos se descartan por `session_id`.
- `index.lock` solo se elimina si lo creó un proceso git de Ariadna que acaba de terminar. Si no, se explica el bloqueo y no se borra nada (criterio completo en §4.1).
- Repo borrado, movido o en una unidad desconectada: aviso en la pestaña con **Reintentar / Localizar / Cerrar**.

### 3.3 Concurrencia

- Mutaciones: serie estricta por repo, con **prioridad** sobre las lecturas.
- Lecturas (`status`, `diff`, `log`): con `--no-optional-locks` y `GIT_OPTIONAL_LOCKS=0`; pueden solaparse entre sí.
- Refrescos coalescidos: como mucho uno en curso y uno pendiente por sesión; se descartan si ya hay una mutación en cola.
- Cada snapshot lleva `revision` monótona; la interfaz ignora revisiones anteriores.
- Ante `index.lock` ajeno: reintento acotado (3 intentos, 200/400/800 ms) y mensaje accionable.
- La cola es por **clave de escritura = ruta del worktree** (la pestaña), no por directorio git común. La deduplicación de pestañas usa el directorio git común para avisar, pero dos worktrees del mismo repo son dos sesiones distintas: nunca se escribe sobre el árbol equivocado (decisión de la revisión 3; **decidido, no implementado**: hoy se enfoca la pestaña existente con un aviso).

### 3.4 Contrato IPC

- Todo comando recibe `session_id`. Los tipos se generan con `tauri-specta`; la CI falla si el binding generado difiere del versionado.
- Operaciones largas devuelven `operation_id` y emiten progreso, fin, error y cancelación por `tauri::ipc::Channel`; existe `cancel_operation(operation_id)`. En el clonado el evento es `clone-progress`; en el resto, `op:progress`, `op:done`, `op:error` y `op:cancelled`.
- Los eventos de estado llevan `session_id` y `revision`, y se envían como deltas por sección.
- Rutas: `PathBuf` y bytes en todo el IPC, nunca `String` solo, para no perder nombres no UTF-8.
- Errores: `ApiError { code, message, detail?, action? }`, con código estable y texto localizable. Códigos: `GIT_NOT_FOUND`, `NOT_A_REPO`, `LOCKED`, `CONFLICT`, `AUTH`, `NETWORK`, `TIMEOUT`, `CANCELLED`, `DUBIOUS_OWNERSHIP`, `REPO_MISSING`, `NOT_FOUND`, `POLICY`, `RATE_LIMITED`, `INVALID_REFERENCE`, `PARSE`, `IO`, `AI_NOT_INSTALLED`, `AI_NOT_LOGGED_IN`, `AI_QUOTA`, `AI_BAD_OUTPUT`. Presentación: error de operación = aviso con detalle; error de sesión = banda en la pestaña; fatal = pantalla de recuperación. El `stderr` se muestra tras redactar secretos.

## 4. Git

Requisitos por identificador para poder trazar tareas y pruebas. El comportamiento de referencia es el del Swift ([../spec.md](../spec.md) §3).

| ID | Requisito |
|---|---|
| G-01 | Abrir carpeta dentro de un repo (sube a la raíz); error claro si no lo es [P] |
| G-02 | Estado con `status --porcelain=v2 -z`: preparados, sin preparar, sin seguimiento, ignorados, conflictos [P] |
| G-03 | Preparar y quitar por archivo [P] |
| G-04 | Preparar y quitar por hunk (nuevos, CRLF, sin salto final, rutas con espacios y Unicode) [P] |
| G-05 | Descartar cambios, siempre con confirmación [P] |
| G-06 | Commit con mensaje obligatorio; aviso en HEAD separado [P] |
| G-07 | Amend seguro: sin commits, fuera de rama, merge o rebase en curso o commit ya publicado → se rechaza [P] |
| G-08 | Log con autor, fecha, refs y cuerpo; diff del commit [P] |
| G-09 | Filtro local y búsqueda en todo el historial (mensaje, autor, hash) [P] |
| G-10 | Ramas: listar, crear, cambiar, renombrar, borrar; remotas con seguimiento [P] |
| G-11 | Fetch, pull (merge o rebase) y push con `--set-upstream` [P] |
| G-12 | Merge ff / no-ff / ff-only / ya al día [P] |
| G-13 | Conflictos a tres vías (ours/base/theirs), diff3, elección por hunk o archivo, escribir solo con confirmación [P] |
| G-14 | Marcar resuelto; continuar o abortar merge, rebase y cherry-pick [P] |
| G-15 | Recientes y favoritos, con importación del `recents.json` del Swift [P] |
| G-16 | Clonar desde URL (https, ssh, local) con credenciales de un solo uso vía `GIT_ASKPASS`; probar y olvidar credenciales [N] |
| G-17 | Preparar o quitar líneas sueltas dentro de un hunk [N] |
| G-18 | Stash: guardar, listar, aplicar, sacar (*pop*), borrar con confirmación y ver su diff; los conflictos al aplicar van al panel de Conflictos [N] |

### 4.1 Endurecimiento de git (todas las llamadas)

- `git` se resuelve **una vez al arrancar** a ruta absoluta (nunca con el directorio actual): en Windows `HKLM\SOFTWARE\GitForWindows\InstallPath`, `Program Files\Git\cmd\git.exe` y luego `PATH`; en macOS `PATH` ampliado (`/usr/bin`, Homebrew, Command Line Tools). Se valida `git --version ≥ 2.39`.
- `--literal-pathspecs` (`GIT_LITERAL_PATHSPECS=1`), `--` antes de rutas, refs validadas con `git check-ref-format` y rechazo de nombres que empiecen por `-`.
- Rutas a git por entrada estándar cuando son muchas, para no chocar con el límite de línea de comandos de Windows: `--pathspec-from-file=- --pathspec-file-nul` o `update-index -z --stdin`.
- Lecturas con `-c core.fsmonitor=false -c core.hooksPath=<vacío> --no-ext-diff --no-textconv`, `--no-optional-locks` y `GIT_OPTIONAL_LOCKS=0`; `GIT_TERMINAL_PROMPT=0`, `GIT_PAGER=cat`, `GIT_EDITOR=true`. Los hooks se ejecutan solo en mutaciones, con aviso de confianza del repo la primera vez.
- Codificación: siempre `-z`, `-c core.quotepath=false`, `i18n.logOutputEncoding=UTF-8`. No se interpretan mensajes humanos de error: se usan códigos de salida y porcelain, con `LC_ALL=C` como apoyo y no como garantía. Las dos excepciones documentadas son `dubious ownership` (git solo lo comunica por texto, con código 128) y la clasificación de `AUTH` de §4.3; ambas viven en una función con pruebas y solo se aplican a lo que declaran.
- `core.longpaths=true` por proceso y manifiesto `longPathAware`.
- `safe.directory`: error accionable `DUBIOUS_OWNERSHIP`. Se añade solo con confirmación y nunca con `*`.
- Parches: se construyen **desde bytes de `git diff`** y se aplican con `git apply --cached` por entrada estándar binaria, sin tocar `\r` ni leer el árbol de trabajo. Las resoluciones de conflicto se escriben en binario conservando fin de línea y BOM.
- **Tiempos, con una sola regla** (resuelve la contradicción entre «timeout propio» y «clonar sin timeout»):

  | Tipo de llamada | Límite | Cancelación |
  |---|---|---|
  | Lectura local (`status`, `diff`, `log`, `for-each-ref`) | 60 s por comando (ajustable por operación) | Sí, por `operation_id` |
  | Mutación local (commit, merge, checkout…) | 60 s por comando | Sí |
  | Red por git (clone, fetch, pull, push, `ls-remote`) | **Sin límite propio**: puede tardar lo que tarde el servidor o una ventana del GCM o de SSH | Solo manual, con progreso cuando git lo emite |
  | API REST de PR | 30 s en total, 8 MiB de respuesta | Al cerrar el panel o cambiar de repo |

  Con PAT de un solo uso no hay interacción posible: `GIT_TERMINAL_PROMPT=0` y `GCM_INTERACTIVE=never`. Sin PAT, la ventana la abre el GCM como proceso aparte, y por eso la operación de red no lleva temporizador. Nunca se registran URL con credenciales.
- **Bloqueos (`index.lock`).** Criterio para no confundir un bloqueo ajeno con un error del usuario:
  1. Las lecturas no crean bloqueo (`--no-optional-locks`), así que un `index.lock` presente durante una lectura es de otro proceso.
  2. Ante un bloqueo se reintenta 3 veces (200/400/800 ms). Si sigue, se comprueba si hay un proceso `git` vivo cuyo directorio de trabajo o línea de comandos apunte a ese repo y cuya hora de inicio sea anterior a la del `.git/index.lock`.
  3. Si no hay proceso vivo y el bloqueo tiene más de 10 minutos, la interfaz ofrece «Eliminar bloqueo» con confirmación nativa que nombra repo, ruta y antigüedad. **Nunca se borra automáticamente.**
  4. Si hay proceso vivo, el error `LOCKED` nombra el programa cuando se conoce («lo tiene un `git` en otra terminal»).

  Hoy están implementados (1) y (2); (3) y (4) están **decididos, no implementados**.

### 4.2 Rutas y plataforma

- `PathSafety` con lista cerrada de casos: `\`, unidades, UNC, `\\?\`, nombres reservados (CON, NUL, AUX…), `.git` y sus variantes NTFS (`.git.`, `git~1`, `.git `), flujos alternativos `fichero:stream`, final con punto o espacio, enlaces simbólicos y *junctions* que escapan, 8.3, mayúsculas, NFD/NFC en macOS, nombres `-rf`, `:(glob)*`, `*.c`.
- Al proceso hijo se le pasa la ruta con `dunce::simplified()`.
- Persistencia: `~/Library/Application Support/Ariadna` y `%APPDATA%\Ariadna` (crate `directories`).
- Atajos: ⌘ en macOS, Ctrl en Windows.
- WSL (`\\wsl$`), OneDrive y unidades de red: soportados en lectura y sondeo; las limitaciones se documentan en la interfaz.
- Matriz de `core.autocrlf` (true, false, input) más `.gitattributes` con `eol=`, ficheros mixtos, UTF-16 y BOM.

### 4.3 Alojamiento remoto: GitHub, Azure DevOps y otros (D19)

**Principio:** el control de versiones va **siempre por `git`** y con la autenticación que el usuario ya tiene (GCM, agente SSH, `credential.helper`), así que clonar, fetch, pull y push funcionan igual con `github.com`, GitHub Enterprise, Azure DevOps, GitLab, Bitbucket o un servidor propio. Solo las **pull requests** (§4.3.2) hablan con la API REST del proveedor, y solo para Azure DevOps y GitHub.

**Reconocimiento del proveedor.** Función pura y testeable `RemoteInfo::parse(url)` que devuelve `{ provider, host, owner, project?, repo, webUrl }`:

| Proveedor | Formas de URL |
|---|---|
| GitHub / GitHub Enterprise | `https://github.com/o/r(.git)`, `git@github.com:o/r.git`, `ssh://git@host/o/r`, `https://{host}/*.ghe.com/o/r` |
| Azure DevOps | `https://dev.azure.com/{org}/{project}/_git/{repo}` (también con `{org}@` delante), `git@ssh.dev.azure.com:v3/{org}/{project}/{repo}`, `https://{org}.visualstudio.com/[DefaultCollection/]{project}/_git/{repo}` |
| GitLab, Bitbucket, otros | host y ruta; proveedor `otro` si no se reconoce |

Debe tolerar `%20` y espacios en nombres de proyecto, descartar credenciales embebidas (`usuario:contraseña@`) y no fallar nunca con una URL mal formada.

**Para qué se usa (y solo para eso):**
1. **Sugerir perfil** (§5.1.1) con reglas del tipo `host/propietario` (`github.com/mi-org`, `dev.azure.com/mi-org`), no solo GitHub.
2. **Icono y etiqueta del proveedor** en la pestaña y en la lista de remotos.
3. **Abrir en el navegador** (repo, PR, página de creación de PR). GitHub: `/pull/new/{rama}`; Azure DevOps: `/pullrequestcreate?sourceRef={rama}`. Es la única vía de PR para GitLab y Bitbucket. Solo `https`, solo el host del remoto, mediante el plugin `opener` con una única función expuesta desde Rust.

**Autenticación de git (no de la API).**
- `GIT_TERMINAL_PROMPT=0` evita que git se cuelgue esperando un teclado inexistente; el GCM abre su propia ventana cuando hace falta y en macOS usa el llavero.
- Error `AUTH` (código `AUTH` en el contrato IPC) para fetch, pull y push, con texto accionable: «inicia sesión con el Git Credential Manager», «comprueba `ssh-add`», «tu PAT ha caducado». Detectarlo exige reconocer unos pocos textos de git (`Authentication failed`, `could not read Username`, `Permission denied (publickey)`); es una de las dos excepciones de §4.1, aislada en una función con pruebas y aplicada solo a operaciones de red con código de salida distinto de cero. Si el texto no coincide, el error es `NETWORK` o de comando fallido **con el `stderr` redactado a la vista**, nunca un `AUTH` inventado.
- El filtro de entorno de git quita las variables con forma de secreto; `GIT_*` y `GCM_*` pasan para que `GIT_ASKPASS` y `credential.helper` sigan funcionando.

**Particularidades de Azure DevOps:**
- Las **políticas de rama** rechazan el push a ramas protegidas: se muestra el `remote:` tal cual llega, sin interpretarlo.
- Ariadna **nunca hace force push**, así que no choca con las políticas que lo prohíben.
- La URL que da el botón *Clone* de Azure DevOps lleva `{org}@` delante: es válida y se conserva.
- SSH exige claves con algoritmos modernos: se documenta en la ayuda del error `AUTH`.

#### 4.3.1 Clonar y credenciales (D20, D23)

**Clonar** (`clone_repository`): URL, carpeta destino y rama opcional. Reglas:
- Esquemas permitidos: `https`, `http`, `ssh`, `git@host:ruta`, `file://` y rutas locales absolutas. **Se rechaza `ext::`** y cualquier transporte con ayudante, las URL que empiezan por `-`, los caracteres de control y las contraseñas incrustadas; se pasa `-c protocol.ext.allow=never` en todas las llamadas y `--` antes de la URL.
- El destino debe no existir o estar vacío, ser absoluto y no contener `..` ni `.git`.
- **Atomicidad (D23).** El clonado se hace en un directorio temporal **hermano** del destino (`.{nombre}.ariadna-clone-{aleatorio}`), de modo que el renombrado final sea atómico en el mismo volumen. Al terminar bien: renombrar al destino. Al fallar o cancelarse: se borra solo el temporal. Cada clonado en curso se anota en `pending-clones.json` (ruta temporal, destino, identificador de instancia y hora); al arrancar se barren los temporales anotados cuya instancia ya no exista o que tengan más de 24 h, y nunca se toca un directorio que no esté anotado. Ventaja frente a «borrar lo que creé»: un cierre forzado no deja un repo a medias en la ruta del usuario. **Decidido, no implementado**: hoy se borra lo creado y se toleran los fallos de borrado por ficheros bloqueados o de solo lectura (se informa y se deja la ruta para borrar a mano).
- Es una operación de red: sin límite propio de tiempo, cancelable y con progreso (`clone-progress`, fases en español y porcentaje cuando git lo da). Los submódulos no se descargan (`--no-recurse-submodules`).
- Al terminar, abre el repo en una pestaña nueva. Un proveedor reconocido rellena el nombre de carpeta y avisa de qué credencial hace falta.

**Credenciales: dos caminos, ninguno de los cuales deja el secreto en Ariadna.**

1. **PAT de un solo uso (la vía recomendada en Azure DevOps).** Formulario con URL, usuario y token. El token viaja una vez de la interfaz a Rust como `Secret` (sin `Clone` ni `Serialize`, `Debug` redactado, `zeroize` al soltarlo), se entrega a git **solo** por un `GIT_ASKPASS` temporal en una carpeta privada (`0700`, sin secretos dentro) más dos variables que existen únicamente en el proceso hijo, y se ejecuta con `-c credential.helper=` vacío: **ni el GCM ni el llavero reciben nada**. La carpeta temporal se borra al terminar, también al fallar o cancelar. Todo texto de error se limpia antes de salir del motor.
2. **Sesión del GCM (sin PAT).** Si el usuario no introduce token, git usa su `credential.helper` normal y el GCM abre su ventana (cuenta Microsoft para Azure DevOps, OAuth para GitHub). Sirve para clonar, fetch, pull y push. **No sirve para las PR** (§4.3.2, D24).

Acciones separadas y explícitas: **Guardar en el gestor** (`git credential approve` con `protocol`, `host`, `path` si `credential.useHttpPath`, `username` y `password` por entrada estándar, nunca en `argv` ni en variables de entorno), **Probar** (`git ls-remote --exit-code`, sin interacción) y **Olvidar** (`git credential reject`). Si no hay ningún `credential.helper` configurado, se avisa y se ofrece «usar una vez». En Azure DevOps el nombre de usuario puede ser cualquier valor no vacío; la ayuda por proveedor enlaza a la página de creación de tokens con los permisos mínimos: Azure DevOps *Code → Read & write*; GitHub *repo* o token de grano fino con *Contents*; GitLab `read_repository`/`write_repository`; Bitbucket token de aplicación con *Repositories → Write*.

**Dos cuentas en el mismo host** (GitHub personal y de empresa): v0.1 usa el usuario del remoto como discriminador (`https://usuario@github.com/...`) y permite recordar por host qué usuario corresponde; el soporte completo de varias cuentas simultáneas es posterior.

**Proveedores con reconocimiento de serie:** Azure DevOps, GitHub, GitHub Enterprise, GitLab (incluido autoalojado) y Bitbucket. Cualquier otro host funciona igual como `otro`.

**Fuera de v0.1:** listar repos del proveedor, pipelines, issues y tableros; inicio de sesión con Microsoft (v0.2, D22); PR de GitLab y Bitbucket dentro de la aplicación.

#### 4.3.2 Pull requests (D21, D22, D24)

Alcance: **Azure DevOps y GitHub** (incluido GitHub Enterprise por host). Para el resto, solo abrir el navegador.

| ID | Requisito [N] |
|---|---|
| PR-01 | Detectar proveedor, organización/proyecto y repo desde el remoto (§4.3). Sin proveedor soportado, el panel ofrece solo «Abrir en el navegador». |
| PR-02 | **Listar** PR: abiertas, creadas por mí, donde soy revisor y completadas/abandonadas, con filtro. Cada fila: título, autor, ramas, estado (borrador, activa, completada, abandonada), votos, resumen de políticas y comprobaciones, y conflictos de merge. |
| PR-03 | **Detalle**: descripción (Markdown renderizado desde el AST, **nunca con `{@html}`**), autor, ramas origen y destino, revisores y votos, políticas y estado de compilación, work items vinculados (Azure DevOps), commits y archivos cambiados. |
| PR-04 | **Crear** desde la rama actual: destino por defecto = rama por defecto del remoto; título y descripción desde la plantilla del repo (`pull_request_template.md` en `.azuredevops/`, la raíz o `docs/`, o en `.github/`), revisores, borrador y work items. Si la rama no está publicada, ofrece publicarla antes. «Generar título y descripción con IA» como propuesta. |
| PR-05 | **Bajar la rama** de una PR (fetch y checkout, o rama de seguimiento) con las reglas de checkout seguro. Soporta PR de forks de GitHub vía `refs/pull/N/head`. |
| PR-06 | **Revisar**: diff de la PR (merge-base…origen) archivo a archivo con el mismo visor; hilos de comentarios en línea y generales: ver, crear, responder y resolver. |
| PR-07 | **Votar**: Azure DevOps (aprobar, aprobar con sugerencias, sin voto, esperar al autor, rechazar). GitHub (aprobar, solicitar cambios, comentar). |
| PR-08 | **Completar / abandonar / reabrir**. Azure DevOps: estrategia (merge sin fast-forward, squash, rebase, rebase + merge), borrar rama origen, completar work items y *auto-complete*. GitHub: merge, squash o rebase. Acciones destructivas con **confirmación nativa** que nombra repo, PR y rama destino; las políticas del proveedor siempre por encima: Ariadna no las salta. |
| PR-09 | **Refresco** manual y opcional a intervalo (apagado por defecto, mínimo 60 s) **solo con el panel abierto**. Sin sondeo en segundo plano. |
| PR-10 | **Errores** tipados: `AUTH` con el permiso que falta, `NOT_FOUND`, `POLICY` (409 o política incumplida, con el motivo del proveedor), `RATE_LIMITED`, `NETWORK`. |

**Arquitectura.** El crate `ariadna-hosting` contiene `RemoteInfo` (§4.3), el trait `PullRequestProvider` con las implementaciones `AzureDevOps` y `GitHub`, y un cliente HTTP `reqwest` con `rustls` que no conoce Tauri. El núcleo (`src-tauri/src/core/pr_ops.rs`) solo elige el remoto, valida y pregunta.

**Autenticación: solo PAT (D24).**
- Cada llamada obtiene el secreto con `git credential fill` no interactivo (`GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never`, entorno saneado), lo usa en memoria y lo suelta (`zeroize`). Si el helper no tiene nada, el error es `AUTH` y la interfaz abre el formulario de credenciales de §4.3.1.
- Se envía **siempre** `AuthMethod::Pat` (HTTP Basic). `AuthMethod::Bearer` existe en el tipo y **no se usa en v0.1**: entra con el inicio de sesión de Microsoft de `v0.2`.
- **Azure DevOps no acepta el token OAuth del GCM en su API REST.** Quien clonó con «iniciar sesión con el navegador» recibe un token OAuth/Entra que git acepta y la API rechaza; Azure DevOps responde `401` o `203` con una página de inicio de sesión en HTML. Ambos se traducen a `AUTH` con el texto «Azure DevOps necesita un PAT para las pull requests; el inicio de sesión del navegador sirve para git, no para la API» y un botón que abre el formulario de PAT. El panel de PR de Azure DevOps muestra ese aviso **antes** de la primera llamada si `current_user` falla con `AUTH`. El comportamiento del `203` está **sin verificar contra un servidor real** y se confirma en el spike pendiente de Azure DevOps ([spike-pr.md](spike-pr.md)).
- **No hay caché de credenciales.** Motivos: (a) el invariante de producto es que Ariadna no guarda secretos en memoria más allá de la petición; (b) un PAT revocado o rotado deja de valer de inmediato; (c) `fill` solo se ejecuta por acción del usuario —no hay sondeo, y el refresco es manual o de 60 s con el panel abierto—, así que no existe una ráfaga que amortiguar; (d) cualquier caché habría que invalidarla en el primer 401, con lo que el ahorro sería marginal. Si la medición muestra latencia perceptible, se revisará con un TTL de 30 s como máximo, nunca persistente.
- **Usuario explícito en `fill`.** Con dos cuentas en el mismo host, `fill` puede devolver la equivocada. Se pasa el usuario que lleve la URL del remoto; si no lleva, el que el usuario haya guardado para ese host en Ajustes (no es un secreto) y, si no hay ninguno, se deja vacío para que decida el helper. La respuesta se valida con `current_user` antes de usarla en una escritura.
- Permisos mínimos del PAT por operación, mostrados en el formulario: lectura de código para listar y ver; lectura y escritura para crear, comentar, votar y completar; lectura de work items si se vinculan. En Azure DevOps están verificados solo contra simulación.

**Seguridad de red.** TLS siempre verificado; solo se contacta el origen del remoto (más `api.github.com` para `github.com`); las redirecciones y los enlaces `next` a otro origen **no se siguen**; timeout de 30 s; tope de respuesta de 8 MiB; sin cabeceras en registros. Es la única comunicación de red propia de Ariadna y solo ocurre por acción del usuario o con el panel abierto.

**Diff de la PR.** Se construye con los parches que da el proveedor: GitHub usa el `patch` de `pulls/{n}/files`; Azure DevOps pide las dos versiones de cada archivo afectado y calcula el diff. No se hace `git diff` tras un fetch, porque exigiría bajar la rama y no siempre es posible. Paginación por `Link` (GitHub) o por continuación (Azure DevOps); topes de 8 MiB y 3 000 archivos por PR; los binarios, los enormes y los solo renombrados aparecen como omitidos con el motivo, sin romper el panel.

**Pruebas.** Servidor simulado `wiremock` con respuestas de ejemplo (listar, detalle, crear, hilos, voto, completar y errores 401/403/404/409/429), paginación de dos páginas, límite de tasa con `Retry-After` y redirección a otro host que **no** debe seguirse. Los fixtures de GitHub se generan a partir del spike real ([spike-pr.md](spike-pr.md), 72 comprobaciones) y se revisan para que no lleven datos de la cuenta; los de Azure DevOps salen de la documentación y están marcados como no verificados. Prueba de humo **manual** con cuentas reales; no es criterio automático.

## 5. IA

Principio: **la IA solo propone**. Ninguna acción escribe en el árbol de trabajo, el índice ni las referencias. Toda respuesta es una `AiSuggestion` (`pending | accepted | rejected`) ligada a la `revision` del repo y marcada **obsoleta** si el repo cambia.

### 5.1 Consentimiento y privacidad

- Primer uso por **perfil y proveedor**: diálogo que explica qué se envía (diff, mensajes, tamaño) y a quién. Se guarda por perfil y proveedor.
- **Tres niveles de apagado, con precedencia clara:** (1) el interruptor global (D10) apaga la IA entera y manda sobre todo lo demás; (2) el consentimiento por perfil y proveedor decide qué se puede usar; (3) el archivo de política administrable (RF-IA11) solo existe a partir de `v0.2` y, cuando exista, mandará sobre los ajustes del usuario. En v0.1 no hay política administrable.
- Vista previa exacta de lo que se enviará antes de enviarlo.
- Se excluyen del contexto `.env*`, `*.pem`, `id_*`, `*.pfx`, `*.key` y se aplica un redactor con las reglas de `.gitleaks.toml`.
- El contenido del repo se delimita como **dato no fiable** con marcadores y una instrucción de sistema propia. La propuesta de conflicto se rechaza si introduce texto fuera de los hunks originales.
- Tope de contexto de 100 000 bytes (configurable); si se supera, no se llama al CLI.
- Registro local opcional de lo enviado.

### 5.1.1 Perfiles Personal y Empresa (D16)

El riesgo que se evita: mandar código de la empresa con una cuenta personal, o código personal por la cuenta de la empresa.

- Un **perfil** tiene nombre, tipo (`personal` | `empresa`), proveedores habilitados, consentimiento por proveedor, tope de contexto y, si el proveedor lo permite, un **directorio de configuración aislado** para conservar su propio inicio de sesión.
- Cada repo se asocia a un perfil, guardado en Ariadna y nunca en el repo. Las reglas por patrón del remoto (`github.com/<org-empresa>/*` → Empresa) se aplican al abrir. Un repo sin perfil pregunta en el primer uso de IA; cambiar el perfil de un repo pide confirmación y **cancela las llamadas de IA en vuelo y en cola** de ese repo, porque el consentimiento y el entorno eran del perfil anterior.
- La pestaña y la hoja de propuesta muestran el **perfil activo** y la **cuenta** que usará cada proveedor cuando el CLI lo expone (RF-IA12).
- **Valores por defecto:** Personal = Claude, Codex y Copilot, habilitados tras consentimiento. **Empresa = IA desactivada**, con Claude desactivado (la empresa no lo tiene) y Codex/Copilot habilitables solo aceptando un aviso explícito de que **comparten sesión con Personal**, porque su inicio de sesión no se puede aislar. El modo Auto recorre Claude → Codex → Copilot solo entre los habilitados y consentidos del perfil. **Decidido, no implementado**: hoy Empresa trae Codex y Copilot habilitados por defecto.
- **Aislamiento por perfil.** Cada perfil aislado usa su propio directorio de configuración (`…/Ariadna/profiles/<perfil>/<proveedor>`, permisos `0700`, borrado con el perfil). La variable del proveedor (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`) se **inyecta después** de aplicar la lista blanca, de modo que **sobrescribe la que el usuario tuviera en el entorno**; sin eso, un `CLAUDE_CONFIG_DIR` heredado haría que el perfil Empresa escribiera en la configuración personal. Verificado: Claude (`claude 2.1.319`: un directorio vacío da `loggedIn: false` y `claude auth status` informa del directorio usado). No verificado: `CODEX_HOME` (según documentación). Copilot: **verificado que no se puede aislar** (`copilot 1.0.112`: `COPILOT_HOME` mueve configuración y estado, pero el token OAuth vive en el almacén de credenciales del sistema y la sesión sigue activa con un `COPILOT_HOME` vacío), así que comparte login entre perfiles; la interfaz lo dice en Ajustes y en el aviso de habilitación.
- Por defecto, **Personal usa la sesión normal** del CLI (para no cerrar sesiones existentes) y **Empresa aísla**; ambos se pueden cambiar en Ajustes.
- El inicio de sesión de cada perfil lo hace el CLI: botón «Iniciar sesión» que abre una terminal con el entorno del perfil en Windows y macOS, o muestra el comando si no es posible o no es seguro. Ariadna no ve credenciales.
- Fuera de v0.1: identidad de commit (autor y correo) por perfil.

### 5.2 Proveedores

| Proveedor | Estado de verificación | Invocación |
|---|---|---|
| Claude | **Flags verificados** contra `claude 2.1.319` | `claude -p --output-format json --tools "" --strict-mcp-config --disable-slash-commands --no-session-persistence --permission-mode plan`, prompt por entrada estándar. Queda por probar `--setting-sources ""`. **Prohibido `--bare`**: solo lee `ANTHROPIC_API_KEY` y rompería el inicio de sesión por suscripción |
| Codex | **No verificado** (CLI no instalado en la máquina de verificación) | Candidato: `codex exec --json --sandbox read-only --skip-git-repo-check -`. Spike pendiente |
| Copilot | **Flags verificados** contra `GitHub Copilot CLI 1.0.112` (2026-11-14) | `copilot --output-format json --no-ask-user --disable-builtin-mcps --disable-client-telemetry --no-auto-update --deny-tool shell write read url memory task --available-tools ariadna-no-tools`, prompt por entrada estándar **sin `-p`**. Detalle en [`docs/ia-copilot.md`](../../docs/ia-copilot.md) |

Un proveedor sin flags verificados no bloquea `alpha.2`: se publica marcado como experimental
(hoy solo Codex).

### 5.3 Ejecución

- **RF-IA1:** el prompt va por **entrada estándar**, nunca por `argv`. Límites: unos 32 K en `CreateProcess` y **8 191 en un `.cmd`**. Si un CLI no lee la entrada estándar, se reduce el contexto en lugar de usar un fichero que el CLI leería.
- **RF-IA2:** en Windows, `codex` y `copilot` pueden ser shims `.cmd`. Orden: (1) binario nativo si existe (`~/.local/bin/claude.exe`); (2) `node.exe <entrypoint.js>` con interpretación tolerante del shim; (3) `.cmd` solo con argumentos de una lista blanca constante y prompt por entrada estándar, con aviso. Rust ≥ 1.77.2. Nada derivado del repo (rama, ruta, mensaje) va como argumento. Prueba que falla si un `.cmd` recibe un argumento fuera de la lista.
- **RF-IA3:** entorno por **lista blanca** por proveedor. Común: `PATH`, `SystemRoot`, `windir`, `TEMP`, `TMP`, `USERPROFILE`, `HOME`, `APPDATA`, `LOCALAPPDATA`, `ProgramData`, `ComSpec`, `PATHEXT`, `LANG`, `LC_*`, proxies y certificados (`HTTP(S)_PROXY`, `NO_PROXY`, `SSL_CERT_FILE`, `NODE_EXTRA_CA_CERTS`) y `CLAUDE_CONFIG_DIR`/`CODEX_HOME` (con el valor inyectado por el perfil, §5.1.1). Se eliminan siempre `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`, `OPENAI_API_KEY` y `OPENAI_BASE_URL`, aunque alguien las añada a la lista. Comparación sin distinguir mayúsculas. Copilot recibe un token de GitHub solo si el usuario lo declara.
- **RF-IA4:** disponibilidad, versión y sesión se detectan **ejecutando el propio CLI** (`--version`, subcomando de estado si existe, o botón «Probar» con un prompt trivial). No se leen `.credentials.json` ni el llavero.
- **RF-IA5:** directorio de trabajo = carpeta vacía dedicada (`…/Ariadna/ai-sandbox`), sin ficheros de instrucciones, limpiada tras cada llamada.
- **RF-IA6:** escribir la entrada estándar en una tarea aparte y cerrarla; drenar salida y errores en paralelo; tope de salida; sin terminal interactiva; un prompt interactivo se trata como `AI_NOT_LOGGED_IN`. Tiempo límite duro de 120 s con cancelación.
- **RF-IA7:** esquema de interpretación por proveedor y versión mínima, con golden files de salida real. Formato desconocido → «formato no reconocido», nunca texto crudo a la interfaz.
- **RF-IA8:** cola de IA global FIFO con **2 llamadas simultáneas**, **20 por minuto** y **como máximo una por repositorio** (así dos llamadas lentas de un repo no ocupan las dos plazas). Estado visible (en cola, generando, cancelar); la cancelación funciona también mientras se espera turno. Se cancela al cerrar la pestaña o al cambiar de perfil.
- **RF-IA9:** Auto salta de proveedor **solo si el anterior no está instalado**, y solo entre proveedores habilitados y consentidos. **No hay cambio de proveedor por sesión caducada, cuota agotada ni salida mal formada**: `AI_QUOTA` se muestra con la espera si el CLI la da y Auto se detiene en ese proveedor. La hoja de propuesta indica qué proveedor respondió.
- **RF-IA10:** términos de uso. Ariadna solo lanza el CLI oficial con la sesión del usuario y no toca sus credenciales. Antes de `alpha.2` se leyeron y citaron con fecha los términos vigentes de Anthropic, OpenAI y GitHub ([terminos-ia.md](terminos-ia.md)); el README avisa de que el usuario es responsable de cumplir las condiciones de su cuenta y las reglas de su empresa.
- **RF-IA11 (política administrable, a partir de v0.2):** archivo de política opcional con campo `appliesTo` (`empresa` | `todos`), de solo lectura para el usuario (`/Library/Application Support/Ariadna/policy.json` en macOS, `%ProgramData%\Ariadna\policy.json` en Windows), que permite desactivar la IA, restringir los proveedores, fijar el tope de contexto y obligar a la vista previa. Si existe, manda sobre los ajustes del usuario y la interfaz lo indica; es desplegable con MDM, Intune o GPO. **No forma parte de v0.1 y no se cita como mitigación de ningún riesgo de v0.1.**
- **RF-IA12 (cuentas de organización):** el inicio de sesión lo hace siempre el CLI oficial, SSO incluido. Ajustes y la hoja de propuesta muestran qué cuenta u organización usa cada proveedor cuando el CLI lo expone (`ai_accounts`; hoy solo `claude auth status --json`, verificado; Codex y Copilot: «no disponible»). Las pruebas de humo se hacen con cuentas de organización.

## 6. Seguridad de la aplicación

- **Confirmaciones:** el diálogo lo abre **Rust** con el plugin nativo. Ningún comando destructivo acepta una prueba de confirmación venida de la interfaz. Internamente, `Confirmed` es un token de un solo uso ligado a `(session_id, operación, revision, hash de parámetros)`, con caducidad de 120 s.
  - Un cambio de `revision` entre abrir el diálogo y ejecutar **invalida** el token: la operación no se ejecuta y la interfaz muestra «el repositorio cambió; revisa y vuelve a intentarlo» con el botón de repetir. No se reabre el diálogo automáticamente.
  - El diálogo es modal por ventana: mientras está abierto no se pueden lanzar otras mutaciones ni cambiar de pestaña, así que la cola no avanza y no hay carrera con ella.
  - Si el token caduca por tiempo, se ofrece reabrir el diálogo; no se reintenta en silencio.
- **WebView:** CSP estricta sin `unsafe-eval`, capacidades mínimas por ventana, sin plugin `shell` ni `fs`, sin `{@html}` salvo contenido sanitizado desde AST. Todo texto de git o IA se escapa. Prueba con repo malicioso (nombres de rama con HTML, secuencias de control, `.git/config` con `core.fsmonitor` o `core.sshCommand`).
- **Entorno de `git`:** como el del Swift, **lista blanca** de nombres (`PATH`, `HOME`, `LANG`, `LC_*`, `TMP*`, `SSH_*`, `GIT_*`…) y, de las que pasan, se quitan las que contengan `TOKEN`, `SECRET`, `PASSWORD`, `PASSWD` o `API_KEY`. En Windows se añaden las del sistema (`SystemRoot`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `ComSpec`, `PATHEXT`…) y `GCM_*`. Implementado en `EnvPolicy::git_default`. Los hijos de **IA** usan su propia lista blanca (§5.3).
- No se escribe `core.hooksPath` ni ninguna configuración en repos ajenos.
- Repo público: `.gitignore` ampliado (`target/`, `node_modules/`), gitleaks en pre-commit y en CI.

## 7. Diseño visual

«Moderno» se define con propiedades verificables:

1. Tokens de diseño en un único archivo: escala de espaciado de 4 y 8 px, tres radios, dos elevaciones y paleta con ratios de contraste calculados (≥ 4,5:1 en texto, ≥ 3:1 en interfaz).
2. Tema claro, oscuro y sistema, con transición de 150 ms o menos; respeta `prefers-reduced-motion`.
3. Tipografías Inter (interfaz) y JetBrains Mono (código), incluidas en la aplicación.
4. Barra de título integrada; fondo sólido en v0.1 (D11).
5. Densidad cómoda y compacta, configurable.
6. Iconos de un único conjunto vectorial; estados vacío, cargando y error resueltos en cada pantalla.
7. Paleta de comandos (⌘K / Ctrl+K).
8. Mockups aprobados de las cinco pantallas clave (Bienvenida, Cambios y diff, Historial, Conflictos, Propuesta de IA) antes de M5.

Compatibilidad: matriz con versión mínima de WebView2 (Evergreen) y WebKit de macOS 13; `browserslist` y lint de CSS que prohíben APIs fuera de ella. Ctrl+Tab frente a Ctrl+PgDn se decide tras probar en WebView2.

Accesibilidad: roles `tablist`, `tab` y `tabpanel` con reorden por teclado, listas virtualizadas con `grid` o `listbox`, `aria-live` para operaciones, foco atrapado en diálogos, `forced-colors` y `prefers-contrast`, zoom de texto al 200 % y objetivos ≥ 24 px. Comprobación automática con axe en pruebas de componentes y en E2E; la auditoría con lector de pantalla es manual y queda pendiente.

Atajos: registro único con ámbito (global, pestaña, editor), detección de conflictos en pruebas, menú nativo de Tauri donde se pueda y teclados ES/US/DE.

## 8. Pestañas por repositorio

- Una ventana, N pestañas, una `RepositorySession` por **worktree** abierto. Si se abre otra vez la **misma** ruta, se enfoca la pestaña existente. Si se abre un **worktree distinto** del mismo repositorio, se abre su propia pestaña con un aviso, porque la sesión y la cola son por árbol de trabajo (§3.3; decidido, no implementado).
- Estado por pestaña: panel, selección, borrador de commit (autoguardado), filtros y propuestas de IA.
- **Pestañas ocultas hibernan:** la interfaz pesada (diff, historial) se desmonta y se reconstruye al activar. En Rust queda estado ligero. Hibernación completa tras 10 minutos de inactividad.
- `session.json`: versionado, escritura atómica (temporal y renombrado), copia de respaldo y recuperación si está corrupto. Aviso si una ruta ya no existe.
- **Vigilancia de archivos** (`notify`): debounce de 300 ms con espera máxima de 2 s; se filtra lo ignorado; `.git/{HEAD,index,refs}` se vigila aparte; se ignoran los eventos de nuestras propias mutaciones. **Solo la pestaña activa tiene vigilante**; al activar otra, el anterior se detiene y la nueva se refresca. Ante desbordamiento o error del vigilante, esa sesión pasa a **sondeo cada 2,5 s** (también en UNC y unidades de red) y se avisa en la pestaña. No hay sondeo periódico de `status` en pestañas ocultas.
- Atajos: Ctrl/⌘+T, Ctrl/⌘+W, Ctrl/⌘+1…9, siguiente y anterior.
- Cerrar con propuesta de IA sin descartar o con borrador sin confirmar pide confirmación.
- Tope blando de 20 pestañas con aviso.

## 9. Requisitos no funcionales (con método de medida)

Entorno de referencia: ejecutores `macos-15` y `windows-2025` más un portátil de gama media para medición manual. Fixtures sintéticos de repos grandes en CI con umbral de regresión ([rendimiento.md](rendimiento.md)).

| ID | Métrica | Objetivo |
|---|---|---|
| NF-1 | Arranque en frío hasta ventana interactiva | ≤ 1,5 s |
| NF-2 | Primer pintado de `status` con datos parciales | ≤ 300 ms |
| NF-3 | `status` completo, 50 000 archivos | ≤ 2 s, sin bloquear la interfaz |
| NF-4 | Historial de 100 000 commits | Página de 200, desplazamiento a 60 fps |
| NF-5 | Diff grande | > 1 MB o > 5 000 líneas: colapsado y carga bajo demanda; binarios detectados; líneas largas truncadas. En PR, topes de 8 MiB y 3 000 archivos |
| NF-6 | 20 pestañas | Memoria acotada por hibernación; sin agotar manejadores de vigilante |
| NF-7 | Cancelación | Tiempo hasta terminar el árbol de procesos ≤ 5 s |
| NF-8 | Red | Sin telemetría. La única red propia es la API del proveedor del repo para las PR, siempre por acción del usuario; el navegador lo abre el sistema operativo |

Los umbrales son absolutos y se miden en ejecutores compartidos, así que la CI usa margen (2×) para marcar regresión y el número exacto se comprueba en la medición manual.

Procesos: Job Object de Windows con `KILL_ON_JOB_CLOSE`, proceso creado suspendido y asignado antes de reanudar, `CREATE_NO_WINDOW`; en macOS, grupo de procesos con `SIGTERM`, espera y `SIGKILL`. Tope de bytes al capturar salida y lectura por streaming de los diffs.

## 10. Distribución y CI

- CI: `macos-15`, `windows-2025`, `windows-11-arm` y `ubuntu-24.04` (núcleo). `cargo fmt --check`, `clippy -D warnings`, `cargo deny`, `cargo test`, gitleaks, compilación de la interfaz y `vitest`. Un **job agregador** corre siempre. Caché de cargo.
- Git mínimo (2.39) y última versión, ambos en CI.
- macOS: `.dmg` universal, firma ad-hoc y notarización opcional con los secretos actuales. Se documenta cómo abrir una aplicación no notarizada y se acepta ese límite en v0.1.
- Windows: instalador **NSIS** por usuario (sin administrador), x64 y ARM64 (mejor esfuerzo). WebView2 viene en Windows 11; el instalador lo comprueba e instala si falta. Prueba de instalación, actualización y desinstalación limpias en máquina virtual. MSI para despliegue gestionado: posterior a v0.1, coherente con D14.
- **Firma de código (D17).** No hay certificado. Sin firma, SmartScreen avisa y los equipos de empresa pueden bloquear el binario. Las alfas salen sin firmar con aviso. Para `v0.1.0`, con precios y condiciones consultados en noviembre de 2026 (detalle y fecha en [../../docs/firma-de-codigo.md](../../docs/firma-de-codigo.md); revisar antes de decidir):

| Opción | Coste aproximado | Condiciones | Encaje |
|---|---|---|---|
| **SignPath Foundation** | Gratis para código abierto | Proyecto OSS con licencia aprobada por la OSI: el repo es público y tiene licencia MIT (D18). Resto de condiciones, en signpath.org | **Elegida (D17)** |
| **Azure Artifact Signing** | ~10 USD/mes | Organizaciones de UE, EE. UU., Canadá o Reino Unido con 3+ años de historial fiscal; particulares solo de EE. UU. y Canadá | Solo si el titular es una empresa elegible |
| **Certificado OV** de una CA | 150–300 USD/año | Token HSM o HSM en la nube; validación de identidad | Particulares fuera de EE. UU. y Canadá |
| Sin firmar | 0 | SmartScreen avisa; las empresas pueden bloquearlo | Solo alfas |

macOS: Developer ID y notarización exigen Apple Developer Program (de pago). Sin él, firma ad-hoc con el procedimiento de «Abrir igualmente»; los Mac gestionados pueden bloquearlo.
- Sin telemetría. Lo único que sale del equipo es lo que hacen `git`, los CLI de IA y las llamadas a la API del proveedor para las pull requests (§4.3.2), siempre por acción del usuario. El navegador lo abre el sistema operativo.
- Política de ramas: fusionar `main` en `rust-migration` periódicamente.
- La etiqueta `swift-final` y la rama `legacy-swift` ya existen. Al fusionar `rust-migration` en `main` se retiran `Sources/`, `Tests/` y `Package.swift` y se sustituye el README.

## 11. Pruebas

- **La fuente de verdad es git.** Los fixtures de los parsers se generan con git real y se contrastan con el Swift; se versionan con metadatos (versión de git, sistema) y se regeneran con un script Rust.
- **E2E con `tauri-driver`: solo Linux en CI** (`.github/workflows/e2e.yml`, `ubuntu-24.04`), con baselines visuales en `e2e/baselines/linux`. En macOS y Windows el E2E es **manual**, con lista de comprobación: `tauri-driver` solo controla WebKitGTK en Linux, y el resto de plataformas queda cubierto por pruebas de componentes, pruebas de integración de Rust y humo manual. Los umbrales de regresión visual son por plataforma.
- Pruebas de sesiones: cola, cancelación, cierre durante push, rebase o IA, refresco concurrente, confirmar tras cambiar de pestaña, repo borrado, 20 sesiones.
- Pruebas de propiedades de los parsers (estado, diff, log, ramas, remotos, conflictos, rutas): ninguna entra en pánico con bytes arbitrarios, UTF-8 inválido o entrada truncada.
- IA en CI con un **CLI falso** que vuelca entorno, argumentos y entrada estándar: comprueba la lista blanca, el orden de los flags, que no viajan datos del usuario por `argv` y que un secreto de prueba no llega al hijo.
- Humo con CLI reales: job manual con lista de comprobación; **no** es criterio de salida automático.
- Interfaz: Vitest para componentes y `vitest-axe` para accesibilidad.

## 12. Riesgos

| Riesgo | Mitigación vigente en v0.1 |
|---|---|
| Alcance grande y tres tecnologías nuevas (Rust, Tauri, Svelte) | Hitos alfa (D13), núcleo primero, interfaz delgada y regla de corte de §1.1 |
| Flags de Codex sin verificar (Copilot **ya verificado** con 1.0.112) | Spike de Codex pendiente; se publica como experimental si no se verifica |
| Términos de uso de suscripciones en la empresa | RF-IA10 con extractos fechados; IA opt-in por perfil y proveedor, apagable con el interruptor global y, en Empresa, apagada por defecto |
| Código de empresa enviado con una cuenta personal | Perfil Empresa con IA apagada por defecto (D16), aviso de sesión compartida y cuenta visible cuando el CLI la expone (RF-IA12). Sin mecanismo administrable en v0.1 |
| Shims `.cmd` y límites de línea de comandos | RF-IA1, RF-IA2 |
| Repos hostiles ejecutan código vía git | §4.1: sin `fsmonitor`, sin `ext::`, hooks solo en mutaciones y con aviso |
| WebView2 y WKWebView se comportan distinto | Matriz de versiones, lint de CSS y regresión visual por plataforma |
| Clonado, credenciales y PR completas en la misma versión | Ya publicado en alfas; la regla de corte sigue vigente y las PR son lo primero recortable |
| Las APIs de Azure DevOps y GitHub cambian o piden más permisos | GitHub verificado con servidor real; Azure DevOps limitado a simulación, con spike y lista de comprobación en [spike-pr.md](spike-pr.md) |
| Calidad de ARM64 sin hardware real | Mejor esfuerzo (D7) |
| Binarios sin firmar bloqueados en equipos gestionados | Firma por etapas (D17) y guía de apertura |
| Worktrees: escribir sobre el árbol equivocado | Sesión y cola por worktree (§3.3); mientras no esté implementado, se enfoca la pestaña existente con aviso |
| `git credential fill` devuelve la cuenta equivocada con dos cuentas en el mismo host | Usuario explícito en `fill` y validación con `current_user` antes de escribir |
| El sandbox de Codex en Windows se comporta distinto al de Unix | Humo manual de escritura y ejecución; se avisa si no hay garantía |

## 13. Preguntas abiertas

Ninguna bloquea el hito en curso. Quedan por confirmar las **Propuestas** de §2: D7 (ARM64 mejor esfuerzo), D10 (IA en el binario, apagada hasta consentir), D11 (sin Mica en 0.1), D13 (alfas), D16 (**Empresa con IA apagada**), D17 (SignPath), D19 (alojamiento agnóstico), D20 (clonar con PAT de un solo uso), D23 (clonado atómico), D25 (criterio de bloqueos) y D26 (cola de IA). Si no hay objeción antes del cierre de M7, pasan a cerradas.

## 14. Historial de revisiones

| Revisión | Fecha | Cambio |
|---|---|---|
| 1 | — | Borrador inicial y revisión EGM con tres revisores sin contexto; resoluciones en [revision-egm.md](revision-egm.md) |
| 2 | 2026-11 | Se incorpora el estado real tras `alpha.2`: clonado, líneas sueltas, stash, perfiles aislados y backend de PR |
| 3 | 2026-11-13 | Revisión formal con speckfish (siete subagentes): 4 bloqueos y 23 hallazgos mayores. Se reescribe el alcance (§1.1), se define la arquitectura (§3.1), se resuelve la autenticación de PR (D24, §4.3.2), se unifica el criterio de tiempos (§4.1), se añade glosario (§0.1) y marcas de estado (§0.3). Detalle en [revision-speckfish.md](revision-speckfish.md) |
