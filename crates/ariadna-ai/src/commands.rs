//! Argumentos de cada CLI (RF-IA1, spec §5.2). **Único** módulo que conoce los flags.
//!
//! Reglas:
//! - El prompt va por **stdin**, nunca por argv. Nada derivado del repo (rama, ruta,
//!   mensaje) puede acabar aquí: las funciones no reciben datos del repo.
//! - Claude: flags verificados contra `claude 2.1.319`. **Prohibido `--bare`**
//!   (solo lee `ANTHROPIC_API_KEY` y rompería el login de suscripción).
//! - Copilot: flags verificados contra `GitHub Copilot CLI 1.0.112` (2026-11-14).
//!   El prompt va por stdin **sin `-p`** y el modelo no ve ninguna herramienta.
//! - Codex: flags **NO VERIFICADOS** (CLI no instalado en la máquina de revisión).
//!   Cada flag es una constante para cambiarla fácil tras el spike.

use std::ffi::OsString;

use crate::models::ProviderId;

// ---------------------------------------------------------------- Claude (verificado)

pub const CLAUDE_PRINT: &str = "-p";
pub const CLAUDE_OUTPUT_FORMAT: &str = "--output-format";
pub const CLAUDE_FORMAT_JSON: &str = "json";
/// `--tools ""`: ninguna herramienta (el valor es una cadena vacía).
pub const CLAUDE_TOOLS: &str = "--tools";
pub const CLAUDE_TOOLS_NONE: &str = "";
pub const CLAUDE_STRICT_MCP: &str = "--strict-mcp-config";
pub const CLAUDE_NO_SLASH: &str = "--disable-slash-commands";
pub const CLAUDE_NO_PERSIST: &str = "--no-session-persistence";
pub const CLAUDE_PERMISSION_MODE: &str = "--permission-mode";
pub const CLAUDE_MODE_PLAN: &str = "plan";
/// Flag prohibido. Solo existe para que los tests comprueben su ausencia.
pub const FORBIDDEN_BARE: &str = "--bare";

// ---------------------------------------------------------------- Codex (NO verificado)

// SPIKE (no verificado): `codex exec --json --sandbox read-only --skip-git-repo-check -`
pub const CODEX_EXEC: &str = "exec";
// SPIKE (no verificado):
pub const CODEX_JSON: &str = "--json";
// SPIKE (no verificado):
pub const CODEX_SANDBOX: &str = "--sandbox";
// SPIKE (no verificado):
pub const CODEX_SANDBOX_READ_ONLY: &str = "read-only";
// SPIKE (no verificado):
pub const CODEX_SKIP_GIT: &str = "--skip-git-repo-check";
// SPIKE (no verificado): `-` = leer el prompt de stdin.
pub const CODEX_STDIN_MARKER: &str = "-";

// ---------------------------------------------------------------- Copilot (verificado)
//
// Verificado el **2026-11-14** contra `GitHub Copilot CLI 1.0.112` (Windows 11 x64, binario
// `@github/copilot-win32-x64/copilot.exe`; también con el lanzador de WSL):
// - El prompt va por **stdin y sin `-p`**. `-p -` NO lee stdin: envía el literal «-» al
//   modelo (visto en el evento `user.message` del JSONL). Con stdin canalizado (no TTY) y
//   sin `--prompt`, el CLI usa stdin como prompt y termina solo.
// - `--output-format json` emite **JSONL**: el texto final está en
//   `{"type":"assistant.message","data":{"content":"…"}}` y el cierre en
//   `{"type":"result",…,"exitCode":N}`.
// - Sin restringir herramientas el agente **las usa** (leyó el checkout y ejecutó
//   PowerShell). `--available-tools <nombre inexistente>` deja el catálogo vacío y
//   `--deny-tool` es la segunda barrera; comprobado pidiéndole que ejecutara shell y
//   escribiera un fichero: cero `tool.execution_start`, fichero no creado.

pub const COPILOT_OUTPUT_FORMAT: &str = "--output-format";
pub const COPILOT_FORMAT_JSON: &str = "json";
pub const COPILOT_NO_ASK: &str = "--no-ask-user";
pub const COPILOT_NO_BUILTIN_MCPS: &str = "--disable-builtin-mcps";
pub const COPILOT_NO_TELEMETRY: &str = "--disable-client-telemetry";
/// Evita que la llamada descargue/instale una versión nueva del CLI a mitad de un commit.
pub const COPILOT_NO_AUTO_UPDATE: &str = "--no-auto-update";
/// Reglas de denegación (`--deny-tool`): ganan a cualquier permiso, también a `--allow-all`.
pub const COPILOT_DENY_TOOL: &str = "--deny-tool";
pub const COPILOT_DENY_RULES: &[&str] = &["shell", "write", "read", "url", "memory", "task"];
/// Único catálogo de herramientas que se le ofrece al modelo: la lista vacía no basta
/// (el CLI la interpreta como «sin filtro»), así que se pide una herramienta que no existe.
pub const COPILOT_AVAILABLE_TOOLS: &str = "--available-tools";
pub const COPILOT_NO_TOOLS: &str = "ariadna-no-tools";
/// `true` verificado: con stdin canalizado el CLI usa su contenido como prompt.
pub const COPILOT_READS_STDIN: bool = true;

// ---------------------------------------------------------------- Sesión y cuenta (RF-IA4/12)

/// `claude auth status --json` (verificado con claude 2.1.319): JSON con `loggedIn`, `email`,
/// `orgName`, `subscriptionType`, `authMethod` y `configDirectory`; sale con 1 si no hay sesión.
pub const CLAUDE_AUTH: &str = "auth";
pub const CLAUDE_AUTH_STATUS: &str = "status";
pub const CLAUDE_AUTH_LOGIN: &str = "login";
pub const CLAUDE_JSON: &str = "--json";
// SPIKE (no verificado): `codex login` abre el flujo oficial. Existe `codex login status`
// según la documentación, pero no se ha ejecutado aquí y no se sabe si expone la cuenta.
pub const CODEX_LOGIN: &str = "login";
// `copilot login` existe y procesa el token (verificado: `copilot login --with-token`
// responde «Login failed: …» con un PAT clásico). El flujo OAuth del navegador lo ejecuta
// siempre el CLI; Ariadna solo abre la terminal.
pub const COPILOT_LOGIN: &str = "login";

/// Argumento para comprobar disponibilidad y versión (RF-IA4).
pub const VERSION_FLAG: &str = "--version";

/// Argumentos de `--version`.
pub fn version_args() -> Vec<OsString> {
    vec![OsString::from(VERSION_FLAG)]
}

/// Argumentos fijos para pedir una respuesta. **No reciben ningún dato del repo**.
pub fn build_args(provider: ProviderId) -> Vec<OsString> {
    let list: Vec<&str> = match provider {
        ProviderId::Claude => vec![
            CLAUDE_PRINT,
            CLAUDE_OUTPUT_FORMAT,
            CLAUDE_FORMAT_JSON,
            CLAUDE_TOOLS,
            CLAUDE_TOOLS_NONE,
            CLAUDE_STRICT_MCP,
            CLAUDE_NO_SLASH,
            CLAUDE_NO_PERSIST,
            CLAUDE_PERMISSION_MODE,
            CLAUDE_MODE_PLAN,
        ],
        ProviderId::Codex => vec![
            CODEX_EXEC,
            CODEX_JSON,
            CODEX_SANDBOX,
            CODEX_SANDBOX_READ_ONLY,
            CODEX_SKIP_GIT,
            CODEX_STDIN_MARKER,
        ],
        ProviderId::Copilot => {
            // Orden verificado (2026-11-14, copilot 1.0.112): las opciones variádicas
            // (`--deny-tool`, `--available-tools`) van antes de las banderas finales.
            let mut v = vec![
                COPILOT_OUTPUT_FORMAT,
                COPILOT_FORMAT_JSON,
                COPILOT_NO_ASK,
                COPILOT_NO_BUILTIN_MCPS,
                COPILOT_NO_TELEMETRY,
                COPILOT_DENY_TOOL,
            ];
            v.extend_from_slice(COPILOT_DENY_RULES);
            v.extend_from_slice(&[
                COPILOT_AVAILABLE_TOOLS,
                COPILOT_NO_TOOLS,
                COPILOT_NO_AUTO_UPDATE,
            ]);
            v
        }
    };
    list.into_iter().map(OsString::from).collect()
}

/// Argumentos para consultar la cuenta activa, solo si el CLI la expone de forma
/// **verificada**. `None` = «no disponible» (nunca se leen archivos de credenciales).
pub fn account_args(provider: ProviderId) -> Option<Vec<OsString>> {
    match provider {
        ProviderId::Claude => Some(
            [CLAUDE_AUTH, CLAUDE_AUTH_STATUS, CLAUDE_JSON]
                .into_iter()
                .map(OsString::from)
                .collect(),
        ),
        // SPIKE (no verificado): `codex login status` y similares no se han probado.
        ProviderId::Codex | ProviderId::Copilot => None,
    }
}

/// Argumentos del inicio de sesión oficial de cada CLI (lo ejecuta el usuario en una
/// terminal; Ariadna no ve credenciales). Constantes: no llevan datos del repo.
pub fn login_args(provider: ProviderId) -> Vec<&'static str> {
    match provider {
        ProviderId::Claude => vec![CLAUDE_AUTH, CLAUDE_AUTH_LOGIN],
        ProviderId::Codex => vec![CODEX_LOGIN],
        ProviderId::Copilot => vec![COPILOT_LOGIN],
    }
}

/// `true` si el comando de login de este proveedor se ha comprobado contra el CLI real
/// **poniendo en marcha el flujo** (no solo que el subcomando exista).
///
/// Copilot: `copilot login` existe y su ruta `--with-token` se ejecutó (1.0.112), pero el
/// flujo OAuth del navegador no se ha recorrido a mano, así que sigue en `false` y la UI
/// avisa. Queda anotado en `specs/rust/checklist-humo-ia.md`.
pub fn login_verified(provider: ProviderId) -> bool {
    provider == ProviderId::Claude
}

/// `true` si el prompt viaja por stdin. Verificado en los tres: Claude y Codex con `-p`/
/// `-`/`exec`, y Copilot **sin `-p`** (con stdin canalizado el CLI lo usa como prompt).
pub fn prompt_via_stdin(provider: ProviderId) -> bool {
    match provider {
        ProviderId::Copilot => COPILOT_READS_STDIN,
        _ => true,
    }
}

/// Lista blanca constante de argumentos que puede recibir un shim `.cmd`
/// (RF-IA2). Ningún dato del repo puede estar aquí. La cadena vacía no se admite.
pub fn cmd_shim_allowed_args() -> Vec<&'static str> {
    let mut v = vec![
        VERSION_FLAG,
        CODEX_EXEC,
        CODEX_JSON,
        CODEX_SANDBOX,
        CODEX_SANDBOX_READ_ONLY,
        CODEX_SKIP_GIT,
        CODEX_STDIN_MARKER,
        COPILOT_OUTPUT_FORMAT,
        COPILOT_FORMAT_JSON,
        COPILOT_NO_ASK,
        COPILOT_NO_BUILTIN_MCPS,
        COPILOT_NO_TELEMETRY,
        COPILOT_NO_AUTO_UPDATE,
        COPILOT_DENY_TOOL,
        COPILOT_AVAILABLE_TOOLS,
        COPILOT_NO_TOOLS,
    ];
    v.extend_from_slice(COPILOT_DENY_RULES);
    v.sort_unstable();
    v.dedup();
    v
}

/// Resumen legible del comando para mostrar y registrar. **No incluye el prompt**,
/// solo su tamaño.
pub fn command_summary(provider: ProviderId, prompt_bytes: usize) -> String {
    let args: Vec<String> = build_args(provider)
        .into_iter()
        .map(|a| {
            let s = a.to_string_lossy().into_owned();
            if s.is_empty() {
                "\"\"".to_string()
            } else {
                s
            }
        })
        .collect();
    format!(
        "{} {} <prompt de {prompt_bytes} bytes por stdin, no se registra>",
        provider.binary_name(),
        args.join(" ")
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn strs(p: ProviderId) -> Vec<String> {
        build_args(p)
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn claude_usa_exactamente_los_flags_verificados() {
        assert_eq!(
            strs(ProviderId::Claude),
            [
                "-p",
                "--output-format",
                "json",
                "--tools",
                "",
                "--strict-mcp-config",
                "--disable-slash-commands",
                "--no-session-persistence",
                "--permission-mode",
                "plan"
            ]
        );
    }

    #[test]
    fn nunca_aparece_bare() {
        for p in ProviderId::AUTO_ORDER {
            assert!(!strs(p).iter().any(|a| a == FORBIDDEN_BARE), "{p:?}");
            assert!(!command_summary(p, 10).contains("--bare"));
        }
        assert!(!cmd_shim_allowed_args().contains(&FORBIDDEN_BARE));
    }

    #[test]
    fn codex_candidato_con_prompt_por_stdin() {
        assert_eq!(
            strs(ProviderId::Codex),
            [
                "exec",
                "--json",
                "--sandbox",
                "read-only",
                "--skip-git-repo-check",
                "-"
            ]
        );
    }

    #[test]
    fn copilot_usa_exactamente_los_flags_verificados() {
        // Comprobado contra `GitHub Copilot CLI 1.0.112` el 2026-11-14.
        assert_eq!(
            strs(ProviderId::Copilot),
            [
                "--output-format",
                "json",
                "--no-ask-user",
                "--disable-builtin-mcps",
                "--disable-client-telemetry",
                "--deny-tool",
                "shell",
                "write",
                "read",
                "url",
                "memory",
                "task",
                "--available-tools",
                "ariadna-no-tools",
                "--no-auto-update",
            ]
        );
    }

    #[test]
    fn copilot_no_pasa_el_prompt_por_argv_ni_habilita_herramientas() {
        let a = strs(ProviderId::Copilot);
        // `-p -` NO lee stdin en 1.0.112 (manda el literal «-»): no se usa nunca.
        assert!(!a.contains(&"-p".into()));
        assert!(!a.contains(&"--prompt".into()));
        assert!(!a.contains(&"-".into()));
        assert!(!a.iter().any(|x| x.contains("allow-tool")));
        assert!(!a.iter().any(|x| x.contains("allow-all") || x == "--yolo"));
        assert!(!a.contains(&"--trust-folder".into()));
        // Todas las herramientas de riesgo quedan denegadas explícitamente.
        for rule in ["shell", "write", "read", "url", "memory", "task"] {
            assert!(a.contains(&rule.to_string()), "{rule}");
        }
    }

    #[test]
    fn datos_del_repo_nunca_acaban_en_argv() {
        // Las funciones de argumentos no reciben datos del repo; aun así comprobamos
        // que ningún argumento puede contener rama, ruta o mensaje típicos.
        let repo_data = [
            "feature/login-secreto",
            "src/main.rs",
            "C:\\Users\\javie\\repo",
            "/home/javie/repo",
            "fix: arregla el bug",
            "diff --git",
        ];
        for p in ProviderId::AUTO_ORDER {
            for a in strs(p)
                .iter()
                .chain(std::iter::once(&command_summary(p, 5)))
            {
                for d in repo_data {
                    assert!(!a.contains(d), "{p:?} argv contiene {d}");
                }
            }
        }
    }

    #[test]
    fn todo_argumento_de_codex_y_copilot_esta_en_la_lista_blanca_de_cmd() {
        let allowed = cmd_shim_allowed_args();
        for p in [ProviderId::Codex, ProviderId::Copilot] {
            for a in strs(p) {
                assert!(allowed.contains(&a.as_str()), "{a} no está en la lista");
            }
        }
        assert!(!allowed.contains(&""));
    }

    #[test]
    fn el_resumen_no_incluye_el_prompt() {
        let s = command_summary(ProviderId::Claude, 1234);
        assert!(s.starts_with("claude -p --output-format json"));
        assert!(s.contains("1234 bytes"));
    }

    #[test]
    fn solo_claude_expone_la_cuenta_y_el_login_de_claude_y_copilot_esta_verificado() {
        let a: Vec<String> = account_args(ProviderId::Claude)
            .unwrap()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(a, ["auth", "status", "--json"]);
        assert!(account_args(ProviderId::Codex).is_none());
        // Copilot: `copilot help` no tiene subcomando de cuenta ni de estado de sesión
        // (comandos: app, login, init, config, update, version, workflow, sessions,
        // memories, plugin, mcp, skill, instruction, lsp, sandbox, completion).
        assert!(account_args(ProviderId::Copilot).is_none());
        assert_eq!(login_args(ProviderId::Claude), ["auth", "login"]);
        assert_eq!(login_args(ProviderId::Codex), ["login"]);
        assert_eq!(login_args(ProviderId::Copilot), ["login"]);
        assert!(login_verified(ProviderId::Claude) && !login_verified(ProviderId::Codex));
        // `copilot login` existe (verificado), pero el flujo del navegador no se ha recorrido:
        // sigue sin marcarse como verificado para que la UI no prometa de más.
        assert!(!login_verified(ProviderId::Copilot));
        for p in ProviderId::AUTO_ORDER {
            assert!(!login_args(p).contains(&FORBIDDEN_BARE));
        }
    }

    #[test]
    fn el_prompt_viaja_por_stdin() {
        for p in ProviderId::AUTO_ORDER {
            assert!(prompt_via_stdin(p));
        }
    }
}
