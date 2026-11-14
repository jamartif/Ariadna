# IA con GitHub Copilot CLI: qué está verificado

Estado: **verificado** como proveedor (ya **no** es experimental). Verificación hecha el
**2026-11-14** en Windows 11 x64 con **`GitHub Copilot CLI 1.0.112`** (`@github/copilot`
1.0.110 de npm con su paquete de plataforma `@github/copilot-win32-x64`; también con el
lanzador de WSL). Lo que sigue sin verificar se lista aparte, con su motivo: aquí no se
rellena a ojo.

## Argumentos finales

`ariadna-ai` lanza el CLI **sin shell**, con ruta absoluta, entorno por lista blanca y el
prompt por **stdin**. Argumentos exactos (`crates/ariadna-ai/src/commands.rs`):

```
copilot --output-format json --no-ask-user --disable-builtin-mcps \
        --disable-client-telemetry --no-auto-update \
        --deny-tool shell --deny-tool write --deny-tool read \
        --deny-tool url --deny-tool memory --deny-tool task \
        --available-tools ariadna-no-tools
```

| Flag | Por qué está |
|---|---|
| `--output-format json` | Salida **JSONL** legible: el texto final va en `assistant.message.data.content` y el cierre en `result.exitCode` |
| `--no-ask-user` | Sin preguntas interactivas: el proceso no puede quedarse esperando una respuesta |
| `--disable-builtin-mcps` | Ni los MCP integrados están disponibles (`session.info` los marca como `disabled`) |
| `--disable-client-telemetry` | Sin telemetría del cliente |
| `--no-auto-update` | Una generación no puede descargar otra versión del CLI a mitad de un commit |
| `--deny-tool shell write read url memory task` | Segunda barrera: la denegación gana a cualquier permiso, también a un `--allow-all` |
| `--available-tools ariadna-no-tools` | Deja el **catálogo vacío**: se pide una herramienta que no existe porque `--available-tools` sin valores se interpreta como «sin filtro» |

**Nunca se pasa `-p`.** Con `-p -` el CLI manda el literal «-» al modelo (se ve en el evento
`user.message` del JSONL) y el modelo se pone a buscar el texto por su cuenta con
herramientas. Con stdin canalizado (no TTY) y sin `--prompt`, el CLI usa stdin como prompt y
termina solo: es la única forma verificada.

## Lo que se comprobó ejecutando el CLI

| Comprobación | Resultado |
|---|---|
| `copilot --version` | `GitHub Copilot CLI 1.0.112.` (código 0). Se detecta el `.exe` del paquete de plataforma; si falta, el cargador `npm-loader.js` con `node.exe` |
| Prompt grande por stdin, sin `-p` | `user.message.data.content` lleva el prompt exacto; respuesta correcta en unos 5 s |
| `-p -` como marcador de stdin | **No funciona** (ver arriba): descartado |
| Petición explícita de ejecutar `echo PWNED > marker.txt` | Cero `tool.execution_start`, `marker.txt` no existe, respuesta «NO-TOOLS» |
| `session.info` | Lista las herramientas deshabilitadas (`create`, `edit`, `glob`, `grep`, `powershell`, `task`, `view`, `web_fetch`…) y los MCP integrados en `disabled` |
| `--model modelo-que-no-existe` | `session.error` + `result` con `exitCode: 1`: el parser devuelve error tipado (`copilot_error.jsonl`) |
| Formato de salida | JSONL real capturado en `crates/ariadna-ai/tests/fixtures/copilot_ok.jsonl` y `copilot_error.jsonl` |
| Lanzador de WSL (`/usr/local/bin/copilot` → `node.exe npm-loader.js`) | Se resuelve y responde, pero una tubería de Linux puede no llegar bien por stdin al `node.exe` de Windows: Ariadna no depende de eso (lanza el CLI con el stdin de su propio hijo) |

## Límites conocidos

- **No se puede aislar el inicio de sesión por perfil.** El token OAuth vive en el
  almacén de credenciales del sistema, no en `~/.copilot`: con `COPILOT_HOME` apuntando a un
  directorio vacío el CLI **sigue autenticado** (se crean allí `settings.json`, sesiones,
  logs y plugins). Por eso no se inyecta `COPILOT_HOME` —daría a entender un aislamiento que
  no existe— y Ariadna lo dice claro: `sharesLogin = true`, aviso en Ajustes y en el login.
  Entre perfiles Personal y Empresa se comparte la cuenta; Ariadna solo controla **si** se
  permite el proveedor, no **con qué cuenta** se usa.
- **No expone la cuenta.** `copilot help` no tiene subcomando de cuenta ni de estado de
  sesión (`app`, `login`, `init`, `config`, `update`, `version`, `workflow`, `sessions`,
  `memories`, `plugin`, `mcp`, `skill`, `instruction`, `lsp`, `sandbox`, `completion`). La UI
  dice «Cuenta no disponible» y **no** se lee `~/.copilot`, el llavero ni variables con
  tokens.
- **El prompt viaja solo por stdin**, nunca por `argv`: la línea de comandos del proceso no
  contiene el diff, la rama ni la ruta del repo. Tampoco se registra el prompt ni el diff.
- **Ninguna herramienta está disponible**: aunque el modelo intente usarlas, el catálogo está
  vacío y las reglas de denegación lo impiden. La respuesta se usa como texto, nada más.
- **Sin `-p`**, el CLI termina cuando acaba el stream de stdin; el tope de contexto de
  Ariadna (100 000 bytes) corta antes de lanzarlo, así que no hace falta un tope propio de
  argumentos.

## Lo que **no** se ha podido probar

| Tema | Por qué sigue abierto |
|---|---|
| Flujo **OAuth del navegador** de `copilot login` desde «Abrir terminal» | `copilot login` existe y `copilot login --with-token` procesa el token (con un PAT clásico responde «Login failed: … not supported»), pero el recorrido completo en el navegador no se ha hecho a mano. Por eso `ai_login` sigue devolviendo `verified = false` para Copilot y la UI avisa: no se promete más de lo probado |
| Política de una **organización** que bloquee el CLI o el modelo | No hay cuenta de organización de Copilot disponible |
| **Cuota agotada** y **sesión caducada** con textos reales | `classify_failure` los reconoce por patrón, sin captura real |
| **macOS** | La verificación se hizo en Windows 11 x64 (y con el lanzador de WSL) |
| Ajustes por repositorio (`copilot-instructions.md`, plugins, MCP propios) | No aplican: el catálogo de herramientas está vacío y la carpeta de trabajo es la vacía de Ariadna |

Checklist completa, con el resto de proveedores:
[`specs/rust/checklist-humo-ia.md`](../specs/rust/checklist-humo-ia.md) (§0 y §2.1 son las
mediciones de Copilot).

## Cómo volver a verificar tras actualizar el CLI

1. `copilot --version` y comparar con la versión de este documento.
2. `printf 'responde solo OK' | copilot --output-format json --no-ask-user …` (los flags de
   arriba, **sin `-p`**): el evento `user.message` debe llevar el texto exacto.
3. Pedirle que ejecute un comando y escriba un fichero: no debe haber ningún
   `tool.execution_start` ni fichero nuevo.
4. Si cambia algo, actualizar `commands.rs`, `parse.rs`, los fixtures de
   `crates/ariadna-ai/tests/fixtures/`, este documento, la tabla de §0 del checklist y la
   columna de Copilot en el README.
