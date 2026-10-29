# Plan del MVP (fundación)

Derivado de [spec.md](spec.md). Esta entrega construye la fundación que ya se puede compilar y testear, más la UI SwiftUI lista para abrir en Xcode en un Mac. No es todavía el pulido de la fase 5.

## Decisiones de diseño

- **Git por CLI.** `GitCLIEngine` implementa `GitServing` lanzando `git`. Misma semántica que la terminal, hooks incluidos. libgit2 queda descartado hasta que un perfil de `status` en repos enormes lo pida.
- **Procesos en serie.** `SerialExecutor` encadena las llamadas a `git` de un motor. Así no se corrompe el índice. La IA no usa esa cola.
- **IA solo propone.** `AIProviding` devuelve `AISuggestion` en `pending`. `AIOrchestrator.accept` cambia el estado en memoria. Escribir el archivo exige `confirmed: true` en el motor, y el commit es otro método. No existe un `apply` que junte las dos cosas.
- **CLIs del usuario.** Copilot: `copilot -p … -s --no-ask-user` y denegación de `shell` y `write`. Codex: `codex exec --json --sandbox read-only`. Sin API keys. El cwd de la IA no es el repo.
- **Linux puede testear el núcleo.** SwiftUI y AppKit van en `#if os(macOS)`. En Linux el ejecutable `GitForge` solo explica que la UI es de Mac.
- **Swift tools 6.0, lenguaje Swift 6, macOS 14.** En Mac el manifiesto fija la plataforma. En Linux el manifiesto no lista plataformas (SwiftPM no acepta la lista vacía).

## Módulos

| Módulo | Rol | Dónde corre |
| --- | --- | --- |
| `GitForgeCore` | Modelos, `ProcessRunner`, parsers, `GitCLIEngine`, `RepositorySession`, recientes | macOS y Linux |
| `GitForgeAI` | Protocolo, Copilot, Codex, orquestador, ajustes | macOS y Linux |
| `GitForgeApp` | SwiftUI (`AppModel` y vistas). En Linux, un `main` que sale | UI solo macOS |

## Qué entra en esta fundación

Abrir repo, status (incluido ignorados), stage/unstage de archivos y hunks, commit, log, diff, ramas (crear, checkout, renombrar, borrar, detached), merge ff / no-ff, conflictos a tres vías, escribir resolución con confirmación, marcar resuelto, continuar y abortar, fetch/pull/push contra un remoto, mensaje de commit, resumen, explicación y propuesta de conflicto. La UI cubre ese flujo con hojas y diálogos de confirmación.

## Qué no entra

Clone, init, amend, rebase interactivo, cherry-pick visual, stash, tags, submódulos, LFS, worktrees, diff lado a lado, grafo de historia, stage por líneas sueltas, Copilot SDK, Codex app-server, notarización, icono, empaquetar un `.app`.

## Cómo se comprueba

- `swift test` en Linux con repos temporales reales y CLIs simulados.
- En Mac: abrir `Package.swift` en Xcode 16 o posterior y ejecutar el scheme `GitForge`. La UI no se ha podido ejecutar en el entorno de esta entrega (no hay SDK de macOS).
