# Tareas del MVP

Estado real después de la fundación. `[x]` significa implementado y cubierto por tests en Linux, o presente en la UI SwiftUI (esa UI no se ha compilado en Mac).

## Fase 0 — Spec

- [x] `specs/spec.md` como fuente de verdad
- [x] `specs/plan.md` y esta lista
- [x] Nombre provisional GitForge y macOS 14 documentados
- [ ] Elegir nombre definitivo y bundle id
- [ ] Inicializar github/spec-kit (`constitution` / `specify`) si se quiere el flujo completo

## Fase 1 — Núcleo Git

- [x] Paquete Swift con módulos separados
- [x] `ProcessRunner` con timeout, cancelación y entorno sin variables que parezcan tokens
- [x] Abrir repo (también desde un archivo dentro del repo) y rechazar carpetas que no lo son
- [x] `git status --porcelain=v2 -z` (preparado, sin preparar, sin seguimiento, ignorados, renombres, conflictos, detached, repo sin commits)
- [x] Stage y unstage por archivo
- [x] Stage y unstage de un hunk (`git apply --cached`)
- [x] Commit con mensaje multilínea; vacío y “nothing to commit” traducidos
- [x] Log y diff unificado (worktree, índice, commit)
- [x] Ramas: crear, renombrar, checkout, borrar, detached HEAD
- [x] Fetch, pull y push (probado contra un remoto bare local)
- [x] Recientes y favoritos en JSON
- [ ] Stage por líneas sueltas
- [ ] Amend seguro (solo HEAD propio y no pusheado)
- [ ] Descartar archivos sin seguimiento (`git clean`) — a propósito no está, borra datos
- [ ] Búsqueda de commits en todo el historial (la UI filtra solo los que ya cargó)
- [ ] Grafo de commits

## Fase 2 — Conflictos manuales

- [x] Merge ff, no-ff y “already up to date”
- [x] Detección de conflictos y modelo ours / base / theirs desde el índice
- [x] Marcadores, incluido diff3, y elección por hunk sin escribir disco
- [x] Escribir el archivo solo con `confirmed: true` (la sesión también lo exige)
- [x] Marcar resuelto, continuar y abortar
- [x] Rechazar rutas que se salen del repo
- [x] Estado de rebase / cherry-pick reconocido para abort/continue (sin UI dedicada de rebase)
- [ ] Tests de rebase y cherry-pick con conflicto (el código está; el fixture no)

## Fase 3 y 4 — IA

- [x] Protocolo `AIProviding`
- [x] `CopilotCLIProvider` (`copilot -p … -s`) sin herramientas de shell ni escritura
- [x] `CodexExecProvider` (`codex exec --json`, sandbox read-only)
- [x] Parser del JSONL de Codex (`agent_message` y `assistant_message`)
- [x] Tope de contexto: no se lanza el proceso si el diff no cabe
- [x] Orquestador Auto con fallback; un proveedor elegido a mano no cambia en silencio
- [x] Error de login con la orden concreta
- [x] Aceptar o rechazar no vuelve a llamar al CLI ni a git
- [x] Hoja de la UI: usar mensaje, aplicar al editor, copiar, regenerar, descartar
- [ ] Probar contra los CLI reales de Javier (aquí no están instalados; los tests usan un doble)
- [ ] Copilot SDK y Codex app-server
- [ ] Streaming de la respuesta

## App macOS

- [x] Target ejecutable `GitForge` con SwiftUI en `#if os(macOS)`
- [x] Bienvenida, workspace, commit, ramas, historial, conflictos, ajustes
- [x] Confirmaciones de abortar, borrar rama, escribir resolución y descartar cambios
- [ ] Compilar y ejecutar en macOS (no hay SDK de Apple en el entorno de build)
- [ ] Atajos ⌘⇧S (stage) y espacio para hunk
- [ ] Diff lado a lado
- [ ] Icono, `.app`, Hardened Runtime, notarización
- [ ] Onboarding si falta `copilot` o `codex` en el PATH (hoy el error aparece al usarlos)

## Comprobación de esta entrega

- [x] `swift build` en Linux (Swift 6.4.1)
- [x] `swift test`: 24 tests de `GitForgeCore` y 8 de `GitForgeAI`, todos en verde
- [ ] `swift build` / Run en Xcode sobre macOS
