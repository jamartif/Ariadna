# Ariadna

[![CI](https://github.com/jamartif/ariadna/actions/workflows/ci.yml/badge.svg)](https://github.com/jamartif/ariadna/actions/workflows/ci.yml)
[![Release](https://github.com/jamartif/ariadna/actions/workflows/release.yml/badge.svg)](https://github.com/jamartif/ariadna/actions/workflows/release.yml)
![macOS 15+](https://img.shields.io/badge/macOS-15%2B-blue)
![Swift 6](https://img.shields.io/badge/Swift-6-orange)

**Ariadna es un cliente Git nativo para macOS**, escrito en Swift y SwiftUI, en español. Por debajo usa el `git` de tu sistema, así que hooks, credenciales, configuración y resultados son los mismos que en la terminal. Lo que añade es una interfaz clara para preparar cambios por hunk, revisar el historial, gestionar ramas y resolver conflictos a tres vías, con una regla fija: **nada que reescriba tu repositorio ocurre sin que lo confirmes**.

Opcionalmente, y detrás de un flag de compilación, puede pedir ayuda a GitHub Copilot CLI o a Codex CLI para redactar mensajes de commit, resumir cambios, explicar commits o proponer resoluciones de conflictos. La IA solo propone; tú decides.

![Ariadna: cambios preparados, sin preparar y diff por hunk](docs/screenshots/staging.png)

## Índice

- [Funcionalidades](#funcionalidades)
- [Capturas](#capturas)
- [Instalación](#instalación)
- [Compilar desde el código](#compilar-desde-el-código)
- [Arquitectura](#arquitectura)
- [Principios de seguridad](#principios-de-seguridad)
- [Capa de IA (opcional)](#capa-de-ia-opcional)
- [Desarrollo](#desarrollo)
- [Publicar una versión](#publicar-una-versión)
- [Hoja de ruta](#hoja-de-ruta)
- [Limitaciones conocidas](#limitaciones-conocidas)

## Funcionalidades

**Repositorios**
- Abrir cualquier carpeta dentro de un repo (sube hasta la raíz) con error claro si no lo es.
- Recientes y favoritos, guardados en `~/Library/Application Support/Ariadna/recents.json`.

**Cambios y commits**
- Estado con `git status --porcelain=v2`: preparados, sin preparar, sin seguimiento, ignorados y conflictos.
- Preparar y quitar **por archivo o por hunk**, también en archivos nuevos, con CRLF, sin salto final o con rutas con espacios y Unicode.
- Descartar cambios del árbol de trabajo (siempre con confirmación).
- Commit con mensaje obligatorio. Si HEAD está *detached*, Ariadna avisa y no crea el commit hasta que lo confirmes.
- **Amend seguro**: corrige el último commit (mensaje y/o contenido) solo si aún no está en ningún remoto, no hay un merge/rebase en curso y estás en una rama. Así nunca te obliga a un push forzado.

**Historial**
- Log con autor, fecha, refs y cuerpo del mensaje, y diff del commit seleccionado.
- Filtro instantáneo sobre lo cargado y **búsqueda en todo el historial** por mensaje, autor o hash (pulsa Intro o *Buscar en todo*).

**Ramas y remotos**
- Listar, crear, cambiar, renombrar y borrar ramas. Las ramas remotas (`origin/x`) crean la local con seguimiento; si ya existe y no coincide, Ariadna pregunta.
- Fetch, pull (merge o rebase, configurable) y push, con `--set-upstream` automático la primera vez.

**Merge y conflictos**
- Merge con fast-forward, sin fast-forward o “ya al día”.
- Editor de conflictos a tres vías (ours / base / theirs), elección por hunk o por archivo completo, sin tocar el disco hasta que confirmas.
- Marcar como resuelto, continuar o abortar merge, rebase y cherry-pick.

**Robustez**
- Las mutaciones de git van en cola, de una en una; las lecturas pueden solaparse. Un refresco cancelado no publica un estado a medias.
- Timeout y cancelación de procesos, y `PATH` del login shell para encontrar el mismo `git` que en tu terminal.
- Los mensajes de git se piden en inglés internamente para interpretarlos igual con el Mac en cualquier idioma; la interfaz y los errores se muestran en español.

## Capturas

| Ramas | Historial | Ajustes |
| --- | --- | --- |
| ![Ramas](docs/screenshots/ramas.png) | ![Historial](docs/screenshots/historial.png) | ![Ajustes](docs/screenshots/ajustes.png) |

| Diff por hunk | Propuesta de la IA |
| --- | --- |
| ![Diff](docs/screenshots/diff.png) | ![Panel de IA](docs/screenshots/panel-ia.png) |

Las capturas se generan solas en CI (workflow **Capturas**, `scripts/render-snapshots.sh`) con un repositorio de ejemplo, así que reflejan la interfaz real de `main`.

## Instalación

Requisitos: **macOS 15 (Sequoia) o posterior** y `git` instalado (el de las Command Line Tools de Xcode o el de Homebrew).

1. Descarga `Ariadna-X.Y.Z.dmg` (o `.zip`) de la página de [Releases](https://github.com/jamartif/ariadna/releases). El binario es universal (Apple Silicon e Intel).
2. Arrastra `Ariadna.app` a `/Applications`.
3. La app tiene firma ad-hoc pero **no está notarizada**. Si macOS la bloquea la primera vez, ve a **Ajustes del Sistema › Privacidad y seguridad** y pulsa **Abrir igualmente**, o quita la cuarentena desde Terminal:

```bash
xattr -dr com.apple.quarantine /Applications/Ariadna.app
```

## Compilar desde el código

Necesitas macOS 15+ y Xcode 16+ (Swift 6).

```bash
git clone https://github.com/jamartif/ariadna.git
```

```bash
cd ariadna && sh scripts/install-git-hooks.sh && brew install gitleaks
```

```bash
swift test
```

```bash
swift run Ariadna
```

También puedes abrir `Package.swift` en Xcode y ejecutar el scheme **Ariadna**.

Para generar `Ariadna.app`, `.dmg` y `.zip` en `dist/`:

```bash
sh scripts/build-app.sh
```

> `swift test` del núcleo (`GitCore`) también funciona en Linux. Allí `swift run Ariadna` solo avisa de que la interfaz es de macOS y sale con código 64.

## Arquitectura

Es un paquete de Swift Package Manager con tres módulos:

```
Sources/
├── GitCore/      Núcleo sin UI: modelos, procesos, parsers y motor Git
├── App/          Interfaz SwiftUI (ejecutable «Ariadna»)
└── AriadnaAI/    Copilot CLI y Codex CLI (solo con ARIADNA_AI=1)
Tests/
├── GitCoreTests/     Tests contra repos git reales en carpetas temporales
└── AriadnaAITests/   Tests de la IA con un doble de proceso (sin CLI reales)
specs/            Especificación, plan y lista de tareas
scripts/          Hooks, empaquetado y capturas
```

El flujo de una acción es siempre el mismo:

```
Vista SwiftUI ─▶ AppModel (@MainActor) ─▶ RepositorySession (actor) ─▶ GitServing ─▶ GitCLIEngine ─▶ ProcessRunner ─▶ git
```

| Pieza | Responsabilidad |
| --- | --- |
| `GitServing` | Protocolo con todas las operaciones Git. La app y los tests dependen de él, no de `Process`. |
| `GitCLIEngine` | Implementación que lanza el `git` del sistema. Valida rutas y refs, serializa mutaciones (`GitOperationGate`) y traduce fallos a `GitError` con texto en español. |
| `RepositorySession` | Estado de un repo abierto (status, ramas, remotos, conflictos). Refresca tras cada mutación y descarta refrescos obsoletos. |
| `PorcelainParser`, `DiffParser`, `ConflictParser` | Interpretan `status --porcelain=v2 -z`, diffs unificados y marcadores de conflicto (incluido diff3). |
| `ProcessRunner` | Subprocesos con timeout, cancelación, stdin y un entorno saneado (sin variables con pinta de token). |
| `PathSafety` | Impide que una ruta o ref salga del repo o se interprete como opción de git. |
| `AppModel` | Estado de la ventana y acciones de la UI; se comunica con la sesión con `async/await`. |

Se usa el `git` de la terminal en lugar de libgit2 a propósito: el comportamiento es idéntico al que ya conoces (hooks, credenciales, `includeIf`, LFS…).

*Si vienes de C#:* un `protocol` es una `interface`, un `actor` es una clase cuyo estado solo se toca de forma serializada, y `@MainActor` es el hilo de la UI.

## Principios de seguridad

- **Confirmación explícita** para todo lo que pisa trabajo: escribir un archivo de conflicto, descartar cambios, abortar una integración, commitear con HEAD detached o cambiar a una rama local que no coincide con la remota. En el núcleo esto se fuerza con `UserConfirmation.require(confirmed:)`, sin valor por defecto.
- **Nunca reescribe historia publicada**: el amend se niega si el commit ya está en alguna rama remota. No hay push forzado.
- **No toca tu configuración**: Ariadna no escribe `core.hooksPath` ni ningún otro ajuste en los repos que abres.
- **Sin secretos en los procesos hijos**: el entorno que recibe `git` (y los CLI de IA) se filtra y deja fuera variables cuyo nombre contiene `TOKEN`, `SECRET`, `PASSWORD` o `API_KEY`.
- **El repo de Ariadna es público**: `.gitignore` excluye `.env*`, certificados y claves; el hook `pre-commit` pasa `gitleaks protect --staged` y CI pasa gitleaks sobre todo el historial.

## Capa de IA (opcional)

El código está en `Sources/AriadnaAI`, pero **no entra en el build por defecto**. Para activarla:

```bash
ARIADNA_AI=1 swift run Ariadna
```

Con el flag aparecen en la interfaz: *Generar mensaje* y *Resumir* en la barra de commit, *Explicar con IA* en el historial y *Pedir propuesta* en el editor de conflictos. En Ajustes eliges proveedor y rutas de los binarios.

La app no pide API keys: reutiliza el login de cada CLI.

```bash
copilot login
```

```bash
codex login
```

- Copilot se lanza como `copilot -p … -s`, sin permiso de shell ni de escritura.
- Codex se lanza como `codex exec --json --sandbox read-only`.
- El contexto que se envía tiene un tope de tamaño configurable.
- Toda respuesta llega como **propuesta** en una hoja aparte: puedes copiarla, usarla o descartarla. Ni un mensaje de commit ni una resolución se aplican sin tu acción.

## Desarrollo

**Justo después de clonar**, activa el hook de secretos (git no lo hace solo):

```bash
sh scripts/install-git-hooks.sh
```

El script ejecuta `git config --local core.hooksPath .githooks` solo en el clon de Ariadna.

Comandos habituales:

| Comando | Qué hace |
| --- | --- |
| `swift build` | Compila núcleo y app (sin IA). |
| `swift test` | Tests de `GitCoreTests`, contra repos git reales en `/tmp`. |
| `ARIADNA_AI=1 swift test` | Lo anterior más `AriadnaAITests`. |
| `swift run Ariadna` | Abre la app (solo macOS). |
| `bash scripts/render-snapshots.sh` | Regenera las capturas de `docs/screenshots` (solo macOS). |
| `sh scripts/build-app.sh` | Empaqueta `.app`, `.dmg` y `.zip` en `dist/`. |

**CI** (`.github/workflows/ci.yml`) corre en cada push y PR: gitleaks sobre todo el historial en Ubuntu y, en un runner `macos-15`, `swift build` + `swift test` con y sin `ARIADNA_AI=1`.

Documentación de diseño: [`specs/spec.md`](specs/spec.md) (qué y por qué), [`specs/plan.md`](specs/plan.md) (cómo) y [`specs/tasks.md`](specs/tasks.md) (estado de cada tarea).

Datos útiles:
- Bundle id: `es.javiermartinez.ariadna` (en `Sources/App/Info.plist` y `AriadnaIdentity.bundleIdentifier`).
- Ajustes de la app y recientes: `~/Library/Application Support/Ariadna/`.

## Publicar una versión

Cada push a `main` y cada ejecución manual del workflow **Release** generan `Ariadna.app`, un `.dmg` y un `.zip` como artefactos. Para publicar una release con descarga:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

El tag debe seguir el formato `vX.Y.Z`. El workflow, en `macos-15`, compila en Release un binario universal (arm64 + x86_64), arma `Ariadna.app` con la versión del tag, firma ad-hoc (`codesign -s -`) y adjunta `Ariadna-X.Y.Z.dmg` y `.zip` a la GitHub Release. Sin tag `v*`, la versión es `0.0.0-<sha corto>`.

La firma Developer ID y la notarización están preparadas en `scripts/build-app.sh`, pero **solo se ejecutan** si existen a la vez estos secrets del repositorio: `APPLE_CERT_P12` (el `.p12` en base64), `APPLE_CERT_PASSWORD`, `APPLE_ID`, `APPLE_TEAM_ID` y `APPLE_APP_PASSWORD`. Nunca van en el código.

## Hoja de ruta

Pendiente según [`specs/tasks.md`](specs/tasks.md):

- [ ] Preparar líneas sueltas (no solo hunks completos)
- [ ] Grafo de commits
- [ ] Stash
- [ ] Test de cherry-pick con conflicto
- [ ] Probar la IA contra los CLI reales; streaming y Copilot SDK / Codex app-server
- [ ] Firma Developer ID y notarización

`git clean` de archivos no rastreados queda fuera a propósito: es irreversible.

## Limitaciones conocidas

- Solo macOS 15 o posterior; la interfaz no existe en otras plataformas.
- La app no está notarizada (ver [Instalación](#instalación)).
- Las acciones de remoto usan el primer remoto configurado (normalmente `origin`).
- El historial y la búsqueda recorren la rama actual (HEAD), no todas las ramas.
