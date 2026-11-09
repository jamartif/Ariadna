//! Argumentos de cada CLI (RF-IA1, spec §5.2). **Único** módulo que conoce los flags.
//!
//! Reglas:
//! - El prompt va por **stdin**, nunca por argv. Nada derivado del repo (rama, ruta,
//!   mensaje) puede acabar aquí: las funciones no reciben datos del repo.
//! - Claude: flags verificados contra `claude 2.1.319`. **Prohibido `--bare`**
//!   (solo lee `ANTHROPIC_API_KEY` y rompería el login de suscripción).
//! - Codex y Copilot: flags **NO VERIFICADOS** (CLI no instalado en la máquina de
//!   revisión). Cada flag es una constante para cambiarla fácil tras el spike M0.

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

// ---------------------------------------------------------------- Copilot (NO verificado)

// SPIKE (no verificado): `-p` con valor `-` para leer de stdin; hay que comprobar que
// Copilot lo acepta. Si no lee stdin, hay que reducir el contexto con un tope propio
// (RF-IA1) y NUNCA pasar un fichero que el CLI lea.
pub const COPILOT_PROMPT: &str = "-p";
// SPIKE (no verificado):
pub const COPILOT_STDIN_MARKER: &str = "-";
/// SPIKE (no verificado): hipótesis de que Copilot lee el prompt de stdin.
pub const COPILOT_READS_STDIN: bool = true;
// SPIKE (no verificado):
pub const COPILOT_SILENT: &str = "-s";
// SPIKE (no verificado):
pub const COPILOT_NO_ASK: &str = "--no-ask-user";
// SPIKE (no verificado):
pub const COPILOT_NO_BUILTIN_MCPS: &str = "--disable-builtin-mcps";
// SPIKE (no verificado):
pub const COPILOT_NO_TELEMETRY: &str = "--disable-client-telemetry";
// SPIKE (no verificado): lista negra explícita; no hay `--allow-tool` (el modelo no ejecuta nada).
pub const COPILOT_DENY_TOOL: &str = "--deny-tool";
pub const COPILOT_DENY_SHELL: &str = "shell";
pub const COPILOT_DENY_WRITE: &str = "write";
// SPIKE (no verificado):
pub const COPILOT_EXCLUDED_TOOLS: &str = "--excluded-tools";
pub const COPILOT_EXCLUDED_TOOLS_LIST: &str =
    "bash,powershell,create,edit,apply_patch,task,write_bash,write_powershell";

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
        ProviderId::Copilot => vec![
            COPILOT_PROMPT,
            COPILOT_STDIN_MARKER,
            COPILOT_SILENT,
            COPILOT_NO_ASK,
            COPILOT_NO_BUILTIN_MCPS,
            COPILOT_NO_TELEMETRY,
            COPILOT_DENY_TOOL,
            COPILOT_DENY_SHELL,
            COPILOT_DENY_TOOL,
            COPILOT_DENY_WRITE,
            COPILOT_EXCLUDED_TOOLS,
            COPILOT_EXCLUDED_TOOLS_LIST,
        ],
    };
    list.into_iter().map(OsString::from).collect()
}

/// `true` si el prompt viaja por stdin (siempre, hoy). Ver [`COPILOT_READS_STDIN`].
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
        COPILOT_PROMPT,
        COPILOT_STDIN_MARKER,
        COPILOT_SILENT,
        COPILOT_NO_ASK,
        COPILOT_NO_BUILTIN_MCPS,
        COPILOT_NO_TELEMETRY,
        COPILOT_DENY_TOOL,
        COPILOT_DENY_SHELL,
        COPILOT_DENY_WRITE,
        COPILOT_EXCLUDED_TOOLS,
        COPILOT_EXCLUDED_TOOLS_LIST,
    ];
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
    fn copilot_candidato_sin_allow_tool() {
        let a = strs(ProviderId::Copilot);
        assert!(a.contains(&"-p".into()));
        assert!(a.contains(&"-s".into()));
        assert!(a.contains(&"--no-ask-user".into()));
        assert!(a.contains(&"shell".into()) && a.contains(&"write".into()));
        assert!(!a.iter().any(|x| x.contains("allow-tool")));
        assert!(!a.contains(&"git".into()));
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
    fn el_prompt_viaja_por_stdin() {
        for p in ProviderId::AUTO_ORDER {
            assert!(prompt_via_stdin(p));
        }
    }
}
