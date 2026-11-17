# Codex CLI en Ariadna

Verificado el **2026-11-17** con `codex-cli 0.169.0` en Windows 11 (sesión `Logged in using ChatGPT`).
Este documento recoge lo que se comprobó ejecutando el CLI real y cómo se invoca desde Ariadna.

## Cómo lo llama Ariadna

```text
codex exec --json --sandbox read-only --skip-git-repo-check -
```

- El prompt va por **stdin**; el `-` final es el marcador de entrada estándar (no es el prompt).
- `--json` devuelve JSONL: `thread.started`, `turn.started`, `item.completed` (donde
  `item.type = agent_message` trae el texto) y `turn.completed` con el consumo de tokens.
- `--sandbox read-only` deja al modelo sin permiso de escritura y `--skip-git-repo-check`
  permite usarlo en carpetas que no son un repo.
- Tiempo máximo por petición: 120 s (`AI_TIMEOUT`). Si el CLI se cuelga, Ariadna cancela y
  avisa; nunca aplica nada por su cuenta.

## Sesión y aislamiento por perfil

| Comprobación | Resultado |
| --- | --- |
| `codex login status` | `Logged in using ChatGPT` (método, **no** correo ni organización) |
| `codex login` | Abre el flujo oficial; Ariadna no pide ni ve credenciales |
| `CODEX_HOME` con un directorio vacío | `Not logged in` → **sí aísla la sesión** |

Por eso, en el perfil **Empresa**, Codex usa su propio `ai-profiles/<perfil>/codex` y avisa de
que la sesión es independiente. Copilot es el único proveedor que **no** se puede aislar: su
token vive en el llavero del sistema y comparte login entre perfiles.

Nota: si `CODEX_HOME` apunta dentro de `%TEMP%`, el CLI avisa de que no puede crear alias de
PATH. Ariadna usa siempre un directorio estable bajo sus datos.

## Si pruebas desde WSL

El `codex` que instala npm en Windows (`codex.cmd` → `bin/codex.js`) **no funciona dentro de
Linux**: al ejecutarlo desde WSL falla con

```text
Error: Missing optional dependency @openai/codex-linux-x64. Reinstall Codex: npm install -g @openai/codex@latest
```

Para usarlo desde WSL hay que llamar al de Windows con un lanzador que pase la entrada estándar
por un fichero temporal visible desde Windows:

```sh
#!/bin/sh
tmpwsl="/mnt/c/Users/<usuario>/AppData/Local/Temp/codex-stdin-$$.txt"
tmpwin="C:\\Users\\<usuario>\\AppData\\Local\\Temp\\codex-stdin-$$.txt"
cat > "$tmpwsl" || exit 1
trap 'rm -f "$tmpwsl"' EXIT
cmd.exe /c "codex.cmd $* < $tmpwin"
```

Alternativa: instalar Codex dentro de WSL (`npm i -g @openai/codex`) e iniciar sesión allí; es
una sesión distinta de la de Windows.

## Qué falta por comprobar

- Cuenta y organización: el CLI solo dice el método de login, así que la UI muestra «Cuenta no
  disponible» en vez de inventarla.
- Política de empresa que bloquee el CLI, cuota agotada y sesión caducada con textos reales
  (los patrones de error están implementados, pero sin captura real).
- macOS.
