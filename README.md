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

El script ejecuta `git config --local core.hooksPath .githooks` y comprueba que quedó así. Solo afecta al clon de Ariadna, no a los repos que abras con la app.

Ariadna no escribe `core.hooksPath` ni ningún otro ajuste de git en los repositorios que abres. Si omites el script, el pre-commit no corre en local.

## Probarlo en un Mac

Hace falta macOS 15 o posterior y Xcode 16 o posterior (Swift 6).

```bash
git clone https://github.com/jamartif/ariadna.git
cd ariadna
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

El identificador del bundle está en `Sources/App/Info.plist` y en `AriadnaIdentity.bundleIdentifier`.

## Publicar una versión

El remoto público previsto es [github.com/jamartif/ariadna](https://github.com/jamartif/ariadna). Cada push a `main` y cada ejecución manual del workflow **Release** generan `Ariadna.app`, un `.dmg` y un `.zip` como artefactos. Para publicar una release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

El tag tiene que coincidir con `vX.Y.Z` (la `v` es obligatoria). El workflow, en un runner `macos-15`, compila en Release un binario universal (arm64 y x86_64), arma `Ariadna.app` con el bundle `es.javiermartinez.ariadna` y la versión del tag, firma ad-hoc (`codesign -s -`) y adjunta `Ariadna-X.Y.Z.dmg` y `.zip` a la GitHub Release.

En local, en un Mac con Xcode:

```bash
sh scripts/build-app.sh
```

Los ficheros quedan en `dist/`. Si el commit no tiene tag `v*`, la versión es `0.0.0-` más el SHA corto.

Javier no tiene cuenta de Apple Developer. La firma Developer ID y la notarización están en el script, pero **no se ejecutan** salvo que existan a la vez estas variables (en GitHub, como secrets del repo, nunca en el código): `APPLE_CERT_P12` (el `.p12` en base64), `APPLE_CERT_PASSWORD`, `APPLE_ID`, `APPLE_TEAM_ID` y `APPLE_APP_PASSWORD`.

### Abrir la app sin notarizar en macOS 15

macOS marca lo descargado con cuarentena. Sin notarización, la primera apertura puede bloquearse.

1. Mueve `Ariadna.app` a `/Applications`.
2. Ábrela. Si macOS la rechaza: **Ajustes del Sistema > Privacidad y seguridad** y pulsa **Abrir igualmente**.
3. O, en Terminal:

```bash
xattr -dr com.apple.quarantine /Applications/Ariadna.app
```

En Apple Silicon un binario sin firmar no arranca. El `.dmg` de la release ya lleva firma ad-hoc; el paso de arriba solo quita la cuarentena.

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

Sin ese script el pre-commit no corre. La app no lo activa por ti. CI (`.github/workflows/ci.yml`) sí pasa gitleaks sobre todo el historial y, en un runner `macos-15`, hace `swift build` y `swift test` de la Fase 1 y otra vez con `ARIADNA_AI=1`.

## Qué no se pudo comprobar aquí

El entorno de esta entrega es Linux, sin SDK de macOS.

- `swift test` del núcleo sí corre aquí.
- La app SwiftUI no se typechequea en Linux: el código está dentro de `#if os(macOS)`. El workflow de CI en `macos-15` es el que la compila.
- Copilot y Codex no están instalados. Con el flag apagado ni siquiera se compilan. Con `ARIADNA_AI=1` los tests usan un doble, no los CLI reales.
