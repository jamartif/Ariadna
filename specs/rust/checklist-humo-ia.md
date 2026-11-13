# Checklist de humo manual de la IA con CLI reales (M6 → `0.1.0-alpha.2`)

Objetivo: comprobar, con los CLI **oficiales** y **cuentas de organización**, lo que los tests con CLI falso no pueden probar. Plataformas: **Windows 11 x64** (ARM64 a mejor esfuerzo) y **macOS 15**. Alcance de la spec: §5, RF-IA1…IA12, D14, D16. Contrato: [ipc.md](ipc.md) («Comandos de IA», «Perfiles con sesión aislada…»).

Regla del checklist: lo que no se pueda comprobar se marca **NO VERIFICADO** con el motivo; no se rellena a ojo. Los resultados se anotan en la tabla final y, si cambian un flag o un formato, se actualizan `commands.rs`, `parse.rs` y los fixtures de `crates/ariadna-ai/tests/fixtures`.

## 0. Qué está verificado y qué no

| Tema | Claude Code | Codex CLI | GitHub Copilot CLI |
|---|---|---|---|
| Binario y `--version` | ✅ `claude 2.1.319` (Windows) | ❌ NO VERIFICADO: CLI no instalado en la máquina de desarrollo | ❌ NO VERIFICADO: ídem |
| Flags de `build_args` | ✅ `-p --output-format json --tools "" --strict-mcp-config --disable-slash-commands --no-session-persistence --permission-mode plan` | ❌ NO VERIFICADO (`exec --json --sandbox read-only --skip-git-repo-check -`) | ❌ NO VERIFICADO (`-p - -s --no-ask-user --disable-builtin-mcps --disable-client-telemetry --deny-tool … --excluded-tools …`) |
| Prompt por stdin | ✅ | ❌ NO VERIFICADO (`-` como marcador de stdin) | ❌ NO VERIFICADO (`COPILOT_READS_STDIN` es una hipótesis) |
| Formato de salida / golden files | ⚠️ fixtures reconstruidos, no capturados | ❌ reconstruidos | ❌ reconstruidos |
| Cuenta activa (RF-IA12) | ✅ `claude auth status --json` (`loggedIn`, `email`, `orgName`, `subscriptionType`, `authMethod`, `configDirectory`; sale con 1 sin sesión) | ❌ NO VERIFICADO: no se ejecuta ningún subcomando de estado; la UI dice «Cuenta no disponible» | ❌ NO VERIFICADO: ídem |
| Aislamiento de sesión | ✅ `CLAUDE_CONFIG_DIR` (directorio vacío → `loggedIn: false`; `configDirectory` refleja el directorio) | ⚠️ `CODEX_HOME` según la documentación, **no probado** | ❌ sin mecanismo confirmado (`COPILOT_HOME`, `--config-dir` o `XDG_CONFIG_HOME` son candidatas sin usar) → **comparte login** y la UI lo avisa |
| Comando de login | ✅ `claude auth login` (opciones `--claudeai`, `--console`, `--sso`, `--email`) | ❌ NO VERIFICADO: `codex login` | ❌ NO VERIFICADO: `copilot login` |
| Apertura de terminal (`ai_login`) | Windows: `cmd /c start` y macOS: `open` — compilado, **no ejecutado a mano**; solo probado con una terminal falsa | | |

Pendiente de la propia spec: RF-IA10 (términos de uso de Anthropic, OpenAI y GitHub citados con fecha antes de `alpha.2`).

## 1. Preparación

- [ ] Versión instalada de cada CLI anotada (`claude --version`, `codex --version`, `copilot --version`) y fecha.
- [ ] Dos cuentas de **Claude**: una personal y una de **organización** (Team/Enterprise, idealmente con SSO). Una cuenta de **Codex/ChatGPT** y otra de **Copilot** de la organización (con la política de la organización que permita el CLI; anota si la bloquea).
- [ ] Un repo de prueba con remoto de la empresa (`dev.azure.com/<org>`: regla → Empresa) y otro con remoto `github.com/<usuario>` (regla → Personal). Con cambios preparados que incluyan: un archivo normal, un `.env`, un archivo con un secreto falso (`AKIA` + 16 letras) y un binario.
- [ ] Ariadna instalada desde el instalador de la alfa (no desde `cargo run`), con una carpeta de datos limpia. Variables `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`… definidas a propósito en el entorno de usuario para comprobar que **no** llegan al CLI.
- [ ] Anotado el entorno de red de la empresa (proxy, certificados propios `NODE_EXTRA_CA_CERTS`, VPN) por si afecta.

## 2. Detección de los CLI (RF-IA2, RF-IA4)

Para cada CLI, en cada plataforma:
- [ ] Ajustes → IA muestra «Instalado», versión y ruta correctas.
- [ ] **Windows**: instalado como shim `.cmd` (npm global), como `.exe` nativo y con ruta manual en Ajustes; rutas con espacios y con acentos en el nombre de usuario. Comprueba que se resuelve a `node.exe <entrypoint>` o al binario nativo y que un `.cmd` solo recibe argumentos de la lista blanca.
- [ ] **macOS**: instalado con Homebrew (`/opt/homebrew/bin`) y con npm; Ariadna se abre desde el Finder (el `PATH` de una app gráfica no es el de la terminal) y aun así lo encuentra, o lo indica con claridad.
- [ ] Desinstalado: «No encontrado» con el comando para instalarlo, sin error técnico.

## 3. Sesiones aisladas por perfil e inicio de sesión (D16, RF-IA12)

**Claude (verificable)**
- [ ] Perfil **Empresa** (aísla por defecto): «Iniciar sesión» muestra `CLAUDE_CONFIG_DIR='…/ai-profiles/empresa/claude' claude auth login` (macOS) o `set "CLAUDE_CONFIG_DIR=…" && claude auth login` (Windows) y el directorio existe tras usarlo. Habilita Claude en Empresa para probarlo.
- [ ] «Abrir terminal» abre una ventana (Windows Terminal/consola en Windows; Terminal.app en macOS) y ejecuta el login oficial; el navegador abre el flujo de la organización (SSO). **Ariadna no pide ni ve ninguna credencial.** Anota si macOS pide algún permiso o si Gatekeeper bloquea el `.command` generado.
- [ ] Al terminar, «Actualizar cuentas» muestra el correo y la organización **de la empresa** y el directorio aislado.
- [ ] Perfil **Personal** (sesión normal): la cuenta mostrada es la personal y **no** cambia tras iniciar sesión en Empresa. En una terminal normal, `claude auth status` sigue dando la cuenta personal.
- [ ] Activar «Sesión propia» en Personal: pide otro login y no afecta a la sesión normal ni a Empresa. Desactivarla devuelve la sesión normal.
- [ ] Cerrar sesión en un perfil (en su terminal, con su `CLAUDE_CONFIG_DIR`) no cierra la del otro.
- [ ] Un repo con regla → Empresa genera con la cuenta de Empresa aunque el perfil de Ajustes sea Personal, y la hoja de propuesta lo muestra (perfil y cuenta).

**Codex — NO VERIFICADO (comprobar y anotar)**
- [ ] `codex login --help`, `codex login status` (¿existe? ¿qué imprime? ¿expone cuenta/organización?). Si lo expone, abrir tarea para `account_args(Codex)` y su parser; mientras tanto la UI dice «Cuenta no disponible».
- [ ] ¿`CODEX_HOME` aísla de verdad la sesión? Con un directorio vacío, `codex login status` debe dar «sin sesión»; tras login en Empresa, la sesión normal no cambia. Si no, marcar `isolationVerified` y el aviso de «comparte sesión» para Codex.
- [ ] `codex login` abre el flujo oficial desde «Abrir terminal»; ¿pide algo por la propia terminal?

**Copilot — NO VERIFICADO (comprobar y anotar)**
- [ ] `copilot login` (¿existe con ese nombre?). ¿Dónde guarda la sesión? Probar `COPILOT_HOME`, `--config-dir` y `XDG_CONFIG_HOME`/`APPDATA` con un directorio vacío. Si alguno aísla, añadirlo a `isolation()` en `env.rs` y activar el aislamiento; si no, **confirmar** que comparte login y que el aviso de Ajustes y del login se ve en Personal y en Empresa.
- [ ] ¿Expone la cuenta (p. ej. `copilot` → estado, `gh auth status`)? Anotar; Ariadna no lee `~/.copilot` ni el llavero.
- [ ] Política de la organización: ¿el CLI de Copilot está permitido para la cuenta de empresa? ¿Hay que declarar un token (`GH_TOKEN`) en Ajustes? Anotar cómo falla si no.

## 4. Mensaje de commit (RF-IA1/3/5/6/7/8/9)

Por cada CLI y plataforma, con el repo de prueba:
- [ ] Primer uso por perfil y proveedor: aparece el diálogo nativo de consentimiento con archivos, tamaño, perfil y proveedor. Rechazarlo = no se envía nada (`AI_CONSENT_REQUIRED`). Aceptarlo se recuerda solo para ese perfil y proveedor.
- [ ] La propuesta llega con asunto ≤ 72 caracteres y descripción; no se crea ningún commit ni se cambia el índice (`git status` y `git log` iguales antes y después).
- [ ] **Qué sale del equipo**: ni `.env`, ni el binario, ni el secreto falso (aparece `[REDACTED]`); la vista previa del contexto coincide con lo enviado. Comprobar con un proxy de inspección o con el CLI en modo detallado si lo hay.
- [ ] **Por stdin, no por argv**: con Process Explorer (Windows) o `ps -ww` (macOS) mientras corre, la línea de comandos no contiene el diff, la rama ni la ruta del repo.
- [ ] **Entorno limpio**: con `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`/`GH_TOKEN` definidas, el CLI usa la sesión de suscripción (la cuenta mostrada) y no una API key. Anotar cualquier variable adicional que cada CLI necesite y que la lista blanca bloquee (proxies, certificados: probar tras el proxy de la empresa).
- [ ] La carpeta de trabajo es la vacía de Ariadna (`…/Ariadna/ai-sandbox`), sin ficheros de instrucciones (`CLAUDE.md`, `AGENTS.md`) del repo, y se limpia al terminar.
- [ ] Sin sesión → `AI_NOT_LOGGED_IN` con el comando de login correcto para ese CLI; sin cuota → `AI_QUOTA`; CLI colgado → `AI_TIMEOUT` a los 120 s; `Esc`/«Cancelar» mata el proceso hijo (no queda ningún `claude`/`codex`/`copilot` huérfano).
- [ ] Salida no reconocida → «formato no reconocido», nunca texto crudo. **Capturar la salida real** de cada CLI y guardarla como golden file (sustituye a los fixtures reconstruidos).
- [ ] Modo Auto: salta de proveedor solo si el anterior no está instalado, y solo entre habilitados y con consentimiento.
- [ ] Empresa: Claude desactivado por defecto; Auto recorre solo Codex → Copilot.
- [ ] Con el diff por encima del tope (100 000 bytes) no se lanza el CLI.
- [ ] Windows `.cmd`: un prompt grande (> 8 191 caracteres) llega íntegro por stdin; Copilot: si no lee stdin, documentar el tope propio (RF-IA1).

## 5. Hoja de propuesta

- [ ] «Ver propuesta» abre la hoja con proveedor, perfil y cuenta (o «Cuenta no disponible» para Codex/Copilot).
- [ ] **Copiar** pone en el portapapeles el texto visible (también editado) en Windows y macOS, y avisa si el portapapeles falla.
- [ ] **Usar** escribe en Asunto y Descripción; el commit sigue siendo manual.
- [ ] **Regenerar** pide otra propuesta con el mismo proveedor y perfil sin cerrar la hoja; con otra cuenta/perfil no cambia a escondidas. Cancelar la hoja durante la regeneración mata el proceso.
- [ ] **Descartar** y **Deshacer** devuelven el texto de antes de la IA. `Esc` cierra la hoja sin cambiar nada. Navegación por teclado y lector de pantalla (NVDA/VoiceOver): título, descripción y estado se anuncian.

## 6. Título y descripción de PR (`ai_generate_pull_request`, sin pantalla todavía)

No hay interfaz de PR (M8). Se prueba invocando el comando desde las herramientas de desarrollo de la ventana (`window.__TAURI_INTERNALS__.invoke('ai_generate_pull_request', { tabId, options: { base: 'main' } })`) o con el test de integración:
- [ ] Rama con varios commits sobre `main`, y `main` avanzado después: el diff enviado **no** incluye lo que añadió `main` (se compara contra el ancestro común).
- [ ] Sin `base`: detecta `origin/HEAD`/`origin/main`; en un repo de Azure DevOps con la rama por defecto `master` o `develop` explícita, indicar `base`.
- [ ] La respuesta tiene título ≤ 72 caracteres y descripción en Markdown **sin** números de incidencia, capturas ni resultados de pruebas inventados.
- [ ] Mismas garantías que el commit: `.env` y binarios fuera, secretos (también en los mensajes de commit) como `[REDACTED]`, consentimiento nombrando «pull request», cancelación.
- [ ] Rama sin cambios → `AI_NO_BRANCH_CHANGES`; base inexistente → `AI_BASE_NOT_FOUND`; base con forma sospechosa → `PARSE`.

## 7. Cuentas de organización: casos a mirar

- [ ] SSO/MFA con caducidad: tras expirar la sesión, el error de login indica el comando del CLI **y** del perfil afectado.
- [ ] Política de la organización que bloquee el CLI o el modelo: el mensaje del CLI llega a la UI sin texto crudo ni datos del repo.
- [ ] Proxy con inspección TLS: con `NODE_EXTRA_CA_CERTS`/`SSL_CERT_FILE` heredados funciona; sin ellos falla con un error comprensible.
- [ ] Antivirus/EDR: no bloquea el `.cmd`/`.command` de login ni el CLI lanzado por Ariadna (instalador sin firmar en las alfas, D17).
- [ ] Cuenta personal usada por error en un repo de empresa: la hoja y Ajustes muestran la cuenta antes de enviar (RF-IA12). Anotar si el flujo es suficientemente claro.

## 8. Registro de resultados

| Fecha | Plataforma | CLI y versión | Cuenta (tipo, sin datos personales) | Apartado | Resultado | Notas / cambios necesarios |
|---|---|---|---|---|---|---|
| | | | | | | |

Al terminar: actualizar la tabla de §0, `tasks.md` (M6) y los `SPIKE` de `commands.rs`; capturar los golden files reales; cerrar RF-IA10.
