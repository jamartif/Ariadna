# GitForge — cliente Git nativo para macOS con IA

Especificación de producto. Pensada para alimentar Spec-Driven Development y para que Javier (desarrollador C#, macOS, uso personal) pueda iterar sin redescubrir decisiones.

Nombre provisional: **GitForge**. Bundle id provisional: `app.gitforge.macos` (todavía no hay `.app` empaquetada).

## 1. Visión y usuario

Cliente Git nativo en Swift y SwiftUI para el flujo diario: estado, staging por archivo y hunk, ramas, merge, conflictos y remotos. Añade asistencia de IA que **nunca aplica nada sin confirmación**: mensajes de commit, resúmenes, explicación de commits y propuestas de resolución.

- Usuario del MVP: Javier. Ya tiene GitHub Copilot (IDE + CLI) y Codex vía suscripción ChatGPT.
- Propuesta de valor: un solo sitio para Git visual y la IA de esas suscripciones, sin API keys y sin mandar el código a un proveedor nuevo.
- No objetivos: SaaS multiusuario, hosting de modelos, reemplazar Xcode o VS Code, ni reimplementar el agente completo de Copilot o Codex.

## 2. Alcance

| Ámbito | MVP (v0.1) | Después |
| --- | --- | --- |
| Repos | Abrir repo local, estado, stage por archivo/hunk, commit, log, diff, ramas, fetch/pull/push | Rebase interactivo, cherry-pick UI, stash UI, tags, submódulos, LFS, worktrees |
| Conflictos | Detectar, modelo a tres vías, editor, resolución manual, continuar/abortar | La IA propone; Apply sigue siendo explícito |
| IA | Mensaje de commit, resumen, explicar commit, propuesta de conflicto. Selector Copilot / Codex / Auto | Copilot SDK, Codex app-server, chat del repo |
| Auth IA | Reutilizar `copilot login` y `codex login`. Sin API keys | Onboarding in-app, OAuth si se distribuye |
| Distribución | Build local con SwiftPM / Xcode | Notarización, Sparkle, icono |

Restricción dura: nada se escribe en el working tree, el índice o las refs por iniciativa de la IA. Commit, merge, push y descartar cambios los dispara el usuario.

## 3. Git

Prioridad: paridad con `git(1)` y cero corrupción del índice.

- Abrir carpeta; detectar `.git`; recientes y favoritos.
- Estados: modificado, sin seguimiento, preparado, en conflicto, ignorado (con toggle).
- Stage / unstage por archivo y por hunk. Descartar cambios del worktree con confirmación.
- Commit del índice. Mensaje vacío rechazado. Amend queda fuera de esta entrega.
- Log con padres, autor, asunto, cuerpo y decoraciones. Diff unificado de worktree, índice y commit.
- Ramas: listar, crear, renombrar, borrar, checkout. HEAD detached visible.
- Merge `--ff`, `--no-ff` y `--ff-only`. Abort y continue de merge, rebase y cherry-pick.
- Remotos: listar, añadir, fetch, pull (merge o rebase), push con `--set-upstream` si falta upstream. La auth remota es la del sistema (SSH agent / credential helper). No se reimplementa.
- Conflictos: etapas del índice (`:1:` base, `:2:` ours, `:3:` theirs) más marcadores del worktree (también estilo diff3). Tomar un lado por archivo o por hunk. Escribir el resultado solo con confirmación. Marcar resuelto es `git add` aparte.

## 4. IA

Toda acción produce una `AISuggestion` en estado `pending`. El usuario acepta, copia o descarta. Aceptar no llama a git.

| Capacidad | Entrada | Salida |
| --- | --- | --- |
| Mensaje de commit | Diff preparado y plantilla opcional | Asunto + cuerpo editables |
| Resumen | Diff | Viñetas o estilo PR |
| Explicar commit | SHA + patch | Texto |
| Resolver conflicto | Ours / base / theirs + ruta | Archivo propuesto + razón |

Reglas:

1. Ningún write al worktree, índice o refs sin botón explícito.
2. El contexto tiene un tope de bytes (por defecto 100_000). Si se pasa, no se llama al CLI.
3. Conflictos en seco: la propuesta se ve en el editor antes de escribir el archivo.
4. La UI muestra proveedor y un resumen del comando, sin volcar el prompt (puede contener código y acaba en `argv`).
5. Si el CLI no está o no hay login, el error dice qué ejecutar: `copilot login` o `codex login`.

Invocación real (sin darles permiso de escribir):

- `copilot -p <prompt> -s --no-ask-user --deny-tool shell --deny-tool write …`
- `codex exec --json --sandbox read-only --skip-git-repo-check <prompt>`

El diff va dentro del prompt. No se usa `--allow-tool='shell(git:*)'`: eso dejaría a Copilot ejecutar git. El proceso de IA arranca fuera del repo. Codex va en sandbox de solo lectura.

No hay API keys. La sesión es la que el usuario ya creó con el CLI. El entorno del subproceso es reducido: PATH, HOME, locale, agente SSH y variables `GIT_*` / `SSH_*` que no parezcan tokens. La app no copia `~/.codex/auth.json` ni tokens de Copilot.

## 5. Arquitectura

```
SwiftUI  →  AppModel / RepositorySession
                 ├─ GitCLIEngine : GitServing  →  ProcessRunner  →  git
                 └─ AIOrchestrator : AIProviding
                       ├─ CopilotCLIProvider  →  copilot
                       └─ CodexExecProvider   →  codex
```

- UI: SwiftUI, Observation, async/await. macOS 14+. AppKit solo para el panel de abrir carpeta.
- Persistencia: JSON en Application Support (macOS) o `~/.config/gitforge` (Linux). Sin cuenta propia.
- Git por CLI, no libgit2. Porcelain v2 (`git status --porcelain=v2 -z`), diff unificado, `git log` con separadores ASCII.
- Las llamadas a `git` van en serie por motor para no pisar el índice. La IA no entra en esa cola.
- Tests: XCTest, repos temporales de verdad para git, y un doble de `ProcessRunning` para los CLI.
- Distribución del MVP: fuera de Mac App Store. El sandbox de la tienda impide lanzar `git`, `copilot` y `codex` del sistema con el llavero y el agente SSH del usuario. Hardened Runtime y notarización quedan para la fase de distribución.

## 6. Modelo

`Repository`, `GitStatus`, `StatusEntry`, `Commit`, `Branch`, `Remote`, `FileDiff`, `DiffHunk`, `ConflictFile` (ours / base / theirs), `ConflictHunk`, `AISuggestion` (`pending | accepted | rejected`), `AISettings`, `RecentRepository`.

## 7. Pantallas

- Bienvenida: ruta, Examinar, recientes, favoritos.
- Workspace en tres columnas: ramas y remotos | cambios, historial o conflictos | diff o estudio de conflicto.
- Barra de commit: editor, Generar, Resumir, Crear commit (⌘↩).
- Estudio de conflicto: tres paneles + resultado. Usar lado, hunk, pedir propuesta, escribir (confirmación), marcar resuelto.
- Historial: filtro local sobre los commits cargados, explicación bajo demanda.
- Ajustes: proveedor, rutas de los binarios, tope de contexto, pull con rebase.
- Hoja de propuesta: texto editable, copiar, regenerar, descartar. El aviso deja claro que el repo no ha cambiado.

Atajos: ⌘O volver a la bienvenida, ⌘↩ commit. La UI está en español. Los mensajes de `git` se muestran como los emite el CLI.

## 8. Requisitos no funcionales

- Operaciones largas con timeout y cancelación.
- Sin telemetría de código. Copilot se lanza con `--disable-client-telemetry` (no afecta a la telemetría del servidor de GitHub).
- Apple Silicon como objetivo de distribución. El núcleo también compila en Linux para poder testearlo sin Mac.
- Accesibilidad: etiquetas en la barra, en los paneles del conflicto y en la propuesta.

## 9. Riesgos

| Riesgo | Mitigación en el MVP |
| --- | --- |
| App Sandbox vs CLI externos | Fuera de la Mac App Store. Documentado. |
| Cuota de Copilot o ChatGPT | El error se muestra. No se reintenta solo. |
| Licencia del CLI de Copilot si se embebe | No se redistribuye. El usuario lo instala. |
| Proxies no oficiales | Prohibidos. Solo CLI oficiales. |
| Hooks y credential helpers raros | Los ejecuta `git`. El stderr se enseña. |

## 10. Fases

0. Spec en el repo, plan y tareas.
1. Núcleo Git usable.
2. Conflictos manuales con modelo a tres vías.
3. IA de commit y resumen, con hoja de confirmación.
4. Propuesta de conflicto y explicación de commit.
5. Pulido y distribución notarizada.

## 11. Decisiones cerradas para esta entrega

1. Nombre GitForge, hasta que se elija otro.
2. macOS mínimo 14. Swift tools 6.0.
3. Una ventana, un repo. Los recientes se guardan.
4. Sin clone ni init.
5. No se embebe el binario de Copilot ni el de Codex.
6. Solo CLI. SDK y `codex app-server` más adelante.
7. Pull requests en el navegador, no en la app.
8. Tema del sistema.
9. Plantilla de commit opcional, en el cuadro de la barra.
10. Worktrees fuera.

## 12. Fuentes

Rutas oficiales, sin API keys: Copilot CLI programático, autenticación de Copilot CLI, `codex exec` no interactivo, login de Codex con ChatGPT, y github/spec-kit. No usar proxies no oficiales ni GitHub Models (retirado).
