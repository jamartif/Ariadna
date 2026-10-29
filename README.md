# Ariadna

Cliente Git nativo para macOS, en Swift y SwiftUI. El núcleo llama al `git` del sistema. La ayuda de Copilot y Codex existe en el código, pero **no entra en el build de la Fase 1** salvo que pidas el flag.

Bundle id: `es.javiermartinez.ariadna`. macOS mínimo: **15.0**.

La especificación está en [`specs/spec.md`](specs/spec.md). El plan y las tareas, en [`specs/plan.md`](specs/plan.md) y [`specs/tasks.md`](specs/tasks.md).

## Qué cubre la Fase 1

- Abrir un repo (error claro si la carpeta no lo es), status porcelain v2, stage y unstage por archivo y por hunk.
- Commit. Un mensaje vacío se rechaza. Si HEAD no está en una rama, la app avisa y no crea el commit hasta que lo confirmes.
- Log y diff unificado.
- Ramas: listar, crear, checkout y borrar. HEAD detached se ve en el modelo y en la barra de commit.
- Fetch, pull (merge o rebase) y push con upstream.
- Merge y editor de conflictos a tres vías (ours / base / theirs). Eso se queda: no es Fase 3.
- Cola: las mutaciones de git van de una en una; un refresco de estado se puede cancelar y no publica un estado a medias.
- `ProcessRunner` con timeout, cancelación y PATH del login shell (`$SHELL -l`).

La capa de IA (protocolo, Copilot CLI, Codex CLI, hoja de propuesta) sigue en `Sources/AriadnaAI`. Para compilarla:

```bash
ARIADNA_AI=1 swift test
```

Sin esa variable, `swift build` y `swift test` no la enlazan.

## Obligatorio justo después de clonar

Git no usa `.githooks` hasta que `core.hooksPath` apunta ahí. Un `git clone` **no** lo configura solo: sin este paso, un commit local no pasa por gitleaks (CI sí lo haría después).

```bash
cd <carpeta-del-clon>
sh scripts/install-git-hooks.sh
brew install gitleaks
```

El script ejecuta `git config --local core.hooksPath .githooks` y comprueba que quedó así. Solo afecta a este clon.

Si abres el repo con Ariadna y existe `.githooks/pre-commit`, la app hace esa misma config local cuando `core.hooksPath` aún no está definido. No pisa un valor distinto que ya hubieras puesto. Aun así, instala el hook **antes del primer commit**, no esperes a abrir la app.

## Probarlo en un Mac

Hace falta macOS 15 o posterior y Xcode 16 o posterior (Swift 6).

```bash
git clone <url-del-repo>
cd <carpeta>
sh scripts/install-git-hooks.sh
brew install gitleaks
swift test
open Package.swift
```

En Xcode, scheme **Ariadna**, Run. También:

```bash
swift run Ariadna
```

`swift test` no abre la ventana. `swift run Ariadna` sí, en tu Mac. En Linux el mismo ejecutable solo escribe que la interfaz es de macOS 15 y sale con código 64.

Recientes: `~/Library/Application Support/Ariadna/recents.json`.

El identificador del bundle está en `Sources/App/Info.plist` y en `AriadnaIdentity.bundleIdentifier`. SwiftPM no empaqueta un `.app` firmado; el plist queda listo para cuando se arme el bundle.

## Login de los CLI (solo con el flag de IA)

La app no pide API keys. Usa el login que ya tengas.

```bash
copilot login
codex login
```

Con `ARIADNA_AI=1`, en Ajustes aparecen el proveedor y las rutas de `copilot` y `codex`. Copilot se lanza como `copilot -p … -s` sin permiso de shell ni de escritura. Codex, como `codex exec --json --sandbox read-only`. Nada de eso se aplica al repo hasta que tú confirmas.

## Módulos

| Target | Qué es | Build por defecto |
| --- | --- | --- |
| `GitCore` | Modelos, procesos, parsers, `GitCLIEngine` | Sí |
| `GitCoreTests` | Tests del núcleo | Sí |
| `App` | SwiftUI. Producto ejecutable `Ariadna` | Sí (la UI solo en macOS) |
| `AriadnaAI` | Copilot y Codex | No. Hace falta `ARIADNA_AI=1` |

Si vienes de C#: un `protocol` es una `interface`, y `@MainActor` es el hilo de la UI.

## Secretos

El repo va a ser público. `.gitignore` excluye `.env*`, certificados y claves. El hook `.githooks/pre-commit` ejecuta `gitleaks protect --staged`. Actívalo con:

```bash
sh scripts/install-git-hooks.sh
```

Sin ese script, o sin abrir el repo en Ariadna, el pre-commit no corre. CI (`.github/workflows/ci.yml`) sí pasa gitleaks sobre todo el historial y, en un runner `macos-15`, hace `swift build` y `swift test` de la Fase 1 y otra vez con `ARIADNA_AI=1`.

## Qué no se pudo comprobar aquí

El entorno de esta entrega es Linux, sin SDK de macOS.

- `swift test` del núcleo sí corre aquí.
- La app SwiftUI no se typechequea en Linux: el código está dentro de `#if os(macOS)`. El workflow de CI en `macos-15` es el que la compila.
- Copilot y Codex no están instalados. Con el flag apagado ni siquiera se compilan. Con `ARIADNA_AI=1` los tests usan un doble, no los CLI reales.
