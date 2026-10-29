# GitForge

Cliente Git nativo para macOS, en Swift y SwiftUI, con propuestas de IA. La IA no escribe en el repositorio: propone un mensaje, un resumen o una resolución, y el usuario confirma.

El núcleo llama al `git` del sistema (no a libgit2). Copilot y Codex se usan por sus CLI, con el login que ya tengas. No hay API keys.

La especificación vive en [`specs/spec.md`](specs/spec.md). El plan y el estado de las tareas están en [`specs/plan.md`](specs/plan.md) y [`specs/tasks.md`](specs/tasks.md).

## Qué hay hoy

Hecho y probado en Linux (el núcleo no depende de AppKit):

- Abrir un repo, status porcelain v2, stage y unstage por archivo y por hunk, commit, log y diff.
- Ramas: crear, renombrar, checkout, borrar, HEAD detached.
- Merge (fast-forward y `--no-ff`), conflictos a tres vías (ours / base / theirs), escribir la resolución solo con confirmación, marcar resuelto, continuar y abortar.
- Fetch, pull y push.
- Proveedores `copilot -p … -s` y `codex exec --json`, con un protocolo común. Las sugerencias nacen en `pending`. Aceptarlas no llama a git.
- Tests: 24 de `GitForgeCore` y 8 de `GitForgeAI`.

En la app SwiftUI (código listo, no ejecutado en esta máquina): bienvenida, cambios, historial, estudio de conflicto, ajustes y la hoja de propuesta.

Pendiente: clone/init, amend, stash, tags, rebase interactivo, diff lado a lado, grafo, Copilot SDK, Codex app-server, icono y notarización. El detalle está en `specs/tasks.md`.

## Compilar y ejecutar en macOS

Hace falta macOS 14 o posterior y Xcode 16 o posterior (Swift 6).

```bash
git clone <este-repo>
cd GitForge
open Package.swift
```

En Xcode, elige el scheme **GitForge** y pulsa Run. También vale por terminal, con el toolchain de Xcode:

```bash
swift test
swift run GitForge
```

`swift test` no necesita la UI. `swift run` abre la ventana.

La app no va a la Mac App Store. El sandbox de la tienda impide lanzar el `git`, `copilot` y `codex` del sistema y usar el agente SSH. La distribución prevista es fuera de la tienda (Developer ID), más adelante.

Al abrir un repo, la app guarda recientes en `~/Library/Application Support/GitForge/`.

## Linux (solo núcleo)

En Linux se compilan `GitForgeCore` y `GitForgeAI`, y pasa `swift test`. El ejecutable existe, pero la interfaz SwiftUI no: al lanzarlo sale un aviso y el código 64. Hace falta Swift 6.

```bash
swift test
swift build
```

## Login de los CLI

La app no pide claves. Usa la sesión que ya tengas.

Copilot (suscripción GitHub Copilot):

```bash
# instalación: https://docs.github.com/en/copilot/how-tos/set-up/install-copilot-cli
copilot login
copilot --version
```

Codex (cuenta ChatGPT, no una API key de OpenAI):

```bash
# instalación: https://developers.openai.com/codex/cli
codex login
codex --version
```

En Ajustes se elige Automático, Copilot o Codex, y la ruta del binario si no está en el `PATH`. Si falta el login, el error dice qué comando ejecutar.

Copilot se lanza como `copilot -p <prompt> -s --no-ask-user`, con `shell` y `write` denegados. Codex se lanza como `codex exec --json --sandbox read-only`. El diff viaja dentro del prompt. El proceso no usa el directorio del repo como cwd, para que un descuido del agente no edite archivos. Aun así, `ps` puede mostrar el prompt: es una limitación de `-p` y de `codex exec`, y la app no lo escribe en ningún log.

## Módulos

| Target | Qué es |
| --- | --- |
| `GitForgeCore` | Modelos, procesos, parsers y `GitCLIEngine` |
| `GitForgeAI` | `AIProviding`, Copilot, Codex, orquestador |
| `GitForgeApp` | App SwiftUI. En Linux solo el aviso |

Si vienes de C#: un `protocol` es una `interface`, un `actor` se parece a un objeto al que solo se entra con `await`, y `@MainActor` es el hilo de la UI.

## Qué no se pudo comprobar aquí

El entorno de esta fundación es Linux, sin SDK de macOS y sin los CLI de Copilot y Codex.

- `swift build` y `swift test` sí se ejecutaron. Los 32 tests pasaron.
- La app SwiftUI está en el target, pero el compilador de Linux no typechequea el código dentro de `#if os(macOS)`. Hay que abrir el paquete en Xcode y darle a Run.
- Los proveedores de IA se probaron con un doble que graba los argumentos. No hubo una llamada real a `copilot` ni a `codex`.
