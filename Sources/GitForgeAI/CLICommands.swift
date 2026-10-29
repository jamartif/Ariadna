import Foundation

/// Argumentos fijos de los CLI.
///
/// Copilot podría recibir `--allow-tool='shell(git:*)'` y entonces el modelo
/// ejecutaría git él solo. No lo hacemos: el diff va dentro del prompt y
/// denegamos shell y escritura. La confirmación del usuario queda en la app.
enum CopilotCommand {
    static func arguments(prompt: String) -> [String] {
        [
            "-p", prompt,
            "-s",
            "--no-ask-user",
            "--disable-builtin-mcps",
            "--disable-client-telemetry",
            "--deny-tool", "shell",
            "--deny-tool", "write",
            "--excluded-tools", "bash,powershell,create,edit,apply_patch,task,write_bash,write_powershell",
        ]
    }

    static func summary(promptBytes: Int) -> String {
        "copilot -p <prompt de \(promptBytes) bytes, no se registra> -s --no-ask-user --deny-tool shell --deny-tool write"
    }
}

enum CodexCommand {
    static func arguments(prompt: String) -> [String] {
        [
            "exec",
            "--json",
            "--sandbox", "read-only",
            "--skip-git-repo-check",
            prompt,
        ]
    }

    static func summary(promptBytes: Int) -> String {
        "codex exec --json --sandbox read-only --skip-git-repo-check <prompt de \(promptBytes) bytes, no se registra>"
    }
}
