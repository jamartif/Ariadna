# Plan del MVP (fundación)

Derivado de [spec.md](spec.md). Esta entrega construye la fundación que ya se puede compilar y testear, más la UI SwiftUI lista para abrir en Xcode en un Mac. No es todavía el pulido de la fase 5.

## Decisiones de diseño

- **Git por CLI.** `GitCLIEngine` implementa `GitServing` lanzando `git`. Misma semántica que la terminal, hooks incluidos. libgit2 queda descartado hasta que un perfil de `status` en repos enormes lo pida.
- **Procesos en serie.** `SerialExecutor` encadena las llamadas a `git` de un motor. Así no se corrompe el índice. La IA no usa esa cola.
- **IA solo propone.** `AIProviding` devuelve `AISuggestion` en `pending`. `AIOrchestrator.accept` cambia el estado en memoria. Escribir el archivo exige `confirmed: true` en el motor, y el commit es otro método. No existe un `apply` que junte las dos cosas.
- **CLIs del usuario.** Copilot: `copilot -p … -s --no-ask-user` y denegación de `shell` y `write`. Codex: `codex exec --json --sandbox read-only`. Sin API keys. El cwd de la IA no es el repo.
- **Linux puede testear el núcleo.** SwiftUI y AppKit van en `#if os(macOS)`. En Linux el ejecutable `Ariadna` solo explica que la UI es de Mac.
- **Swift tools 6.0, lenguaje Swift 6, macOS 15.** En Mac el manifiesto fija `.macOS(.v15)`. En Linux no se lista plataforma porque SwiftPM no acepta la lista vacía.
- **IA detrás de un flag.** `ARIADNA_AI=1` incluye el target `AriadnaAI`. Por defecto el build de Fase 1 no lo enlaza. El código no se borra. Merge y conflictos siguen en `GitCore` y en la app.

## Módulos

| Módulo | Rol | Dónde corre |
| --- | --- | --- |
| `GitCore` | Modelos, `ProcessRunner`, parsers, `GitCLIEngine`, `RepositorySession`, recientes | Siempre |
| `AriadnaAI` | Protocolo, Copilot, Codex, orquestador | Solo con `ARIADNA_AI=1` |
| `App` | SwiftUI. El producto ejecutable se llama `Ariadna` | UI solo macOS |

## Qué entra en esta fundación

Abrir repo, status, stage/unstage de archivos y hunks, commit (con aviso si HEAD está detached), log, diff, ramas, merge, conflictos a tres vías, fetch/pull/push. La UI de Fase 1 no muestra la IA. Con `ARIADNA_AI=1` vuelven el mensaje de commit, el resumen, la explicación y la propuesta de conflicto.

## Qué no entra

Clone, init, amend, rebase interactivo, cherry-pick visual, stash, tags, submódulos, LFS, worktrees, diff lado a lado, grafo de historia, stage por líneas sueltas, Copilot SDK, Codex app-server, notarización, icono, empaquetar un `.app`.

## Cómo se comprueba

- `swift test` en Linux con repos temporales reales y CLIs simulados.
- En Mac: abrir `Package.swift` en Xcode 16 o posterior y ejecutar el scheme `Ariadna`. La UI no se ha podido ejecutar en el entorno de esta entrega (no hay SDK de macOS). El workflow `.github/workflows/ci.yml` la compila en un runner `macos-15`.
