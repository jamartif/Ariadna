# Tareas

Estado después de alinear la Fase 1 con el PM. `[x]` significa hecho en el código. Los tests del núcleo se ejecutaron en Linux. La app SwiftUI no se ha ejecutado aquí.

## Fase 0 — Spec

- [x] `specs/spec.md`, `specs/plan.md` y esta lista
- [x] Nombre **Ariadna**, bundle id `es.javiermartinez.ariadna`
- [x] macOS mínimo 15.0
- [ ] Flujo completo de github/spec-kit (`constitution` / `specify`) si se quiere el CLI de Spec Kit además de estos archivos

## Fase 1 — Núcleo Git

- [x] Paquete: `GitCore`, `GitCoreTests`, `App` (ejecutable `Ariadna`)
- [x] `ProcessRunner`: timeout, cancelación, entorno sin variables que parezcan tokens, PATH del login shell
- [x] Abrir repo y error si no lo es
- [x] `git status --porcelain=v2 -z`
- [x] Stage y unstage por archivo
- [x] Stage y unstage de hunk: aislado, contiguo, CRLF, sin salto final, ruta con espacios y Unicode
- [x] Commit. Mensaje vacío rechazado. HEAD detached visible y aviso antes de commitear fuera de rama
- [x] Log y diff unificado
- [x] Ramas: listar, crear, checkout, borrar (también renombrar)
- [x] Fetch, pull merge, pull rebase y push con upstream
- [x] Cola serial de mutaciones git y refresco cancelable (no publica estado si se cancela)
- [x] Recientes y favoritos
- [x] `.gitignore` estricto y gitleaks en `.githooks/pre-commit` y en CI
- [x] Workflow de CI en `macos-15`: `swift build` + `swift test`, y otra pasada con `ARIADNA_AI=1`
- [ ] Stage por líneas sueltas
- [ ] Amend seguro
- [ ] `git clean` de no rastreados (a propósito no está)
- [ ] Búsqueda en todo el historial (la UI filtra lo ya cargado)
- [ ] Grafo de commits
- [ ] Compilar la app en el Mac de Javier (CI lo intenta; este entorno no tiene SDK de Apple)

## Merge y conflictos

Se mantienen en el núcleo, no se mueven a otra fase.

- [x] Merge ff, no-ff y “already up to date”
- [x] Modelo ours / base / theirs
- [x] Marcadores, incluido diff3, y elección por hunk sin escribir disco
- [x] Escribir el archivo solo con confirmación
- [x] Marcar resuelto, continuar y abortar
- [x] Editor de tres vías en la UI
- [ ] Tests de rebase y cherry-pick con conflicto (el código de abort/continue está; el fixture no)

## Fase 3 — IA (código presente, build apagado)

El target `AriadnaAI` no se compila salvo `ARIADNA_AI=1`.

- [x] Protocolo, Copilot CLI, Codex CLI, orquestador, tope de contexto, hoja de propuesta
- [x] Tests del doble de proceso cuando el flag está activo
- [ ] Probar contra los CLI reales de Javier
- [ ] Copilot SDK, Codex app-server, streaming

## Comprobación de esta entrega

- [x] `swift test` en Linux: 36 tests de `GitCoreTests`
- [x] `ARIADNA_AI=1 swift test` en Linux: esos 36 más 8 de `AriadnaAITests`
- [ ] Run de la app en macOS 15 (lo hace el workflow de CI y, en local, Javier)
