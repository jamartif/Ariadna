//! CLI falso que imita a `claude`, `codex` y `copilot` para los tests de integración.
//!
//! Modo: variable `FAKE_MODE` (`claude|codex|copilot`) o, si falta, el nombre del ejecutable.
//! Comportamiento: `FAKE_BEHAVIOR` = `ok` (por defecto) | `login` | `quota` | `sleep` |
//! `garbage` | `empty` | `flood` | `exit:<n>` | `version-fail`.
//! Respuesta: `FAKE_REPLY` (si no, una por defecto).
//! Volcado: si `FAKE_DUMP` apunta a un fichero, añade una línea JSON por ejecución con
//! `argv`, `env`, `stdin`, `cwd` y `cwd_entries`, para que los tests comprueben qué recibió.
//! No hace ninguna llamada de red ni lee credenciales.

use std::io::{Read, Write};

fn mode() -> String {
    if let Ok(m) = std::env::var("FAKE_MODE") {
        return m;
    }
    let exe = std::env::args().next().unwrap_or_default().to_lowercase();
    for m in ["claude", "codex", "copilot"] {
        if exe.contains(m) {
            return m.to_string();
        }
    }
    "claude".to_string()
}

fn dump(args: &[String], stdin: &str) {
    let Ok(path) = std::env::var("FAKE_DUMP") else {
        return;
    };
    let env: serde_json::Map<String, serde_json::Value> = std::env::vars()
        .map(|(k, v)| (k, serde_json::Value::String(v)))
        .collect();
    let cwd = std::env::current_dir().unwrap_or_default();
    let entries = std::fs::read_dir(&cwd).map(|d| d.count()).unwrap_or(0);
    let line = serde_json::json!({
        "argv": args,
        "env": env,
        "stdin": stdin,
        "cwd": cwd.to_string_lossy(),
        "cwd_entries": entries,
    });
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{line}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = mode();
    let behavior = std::env::var("FAKE_BEHAVIOR").unwrap_or_else(|_| "ok".into());

    if args == ["--version"] {
        dump(&args, "");
        if behavior == "version-fail" {
            eprintln!("version rota");
            std::process::exit(5);
        }
        println!("{mode} 9.9.9 (fake)");
        return;
    }

    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    dump(&args, &stdin);

    let reply = std::env::var("FAKE_REPLY")
        .unwrap_or_else(|_| "feat: mensaje falso\n\nCuerpo falso.".to_string());

    match behavior.as_str() {
        "sleep" => {
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
        "garbage" => println!("esto no es el formato esperado"),
        "empty" => {}
        "flood" => {
            let chunk = "x".repeat(8192);
            let mut out = std::io::stdout();
            for _ in 0..512 {
                if out.write_all(chunk.as_bytes()).is_err() {
                    break;
                }
            }
        }
        "login" => match mode.as_str() {
            "claude" => {
                println!(
                    "{}",
                    serde_json::json!({"type":"result","is_error":true,"result":"Invalid API key · Please run /login"})
                );
                std::process::exit(1);
            }
            "codex" => {
                eprintln!("Not logged in. Please run `codex login` to continue");
                std::process::exit(1);
            }
            _ => {
                eprintln!("Error: you are not logged in. Run `copilot login`");
                std::process::exit(1);
            }
        },
        "quota" => match mode.as_str() {
            "claude" => {
                println!(
                    "{}",
                    serde_json::json!({"type":"result","is_error":true,"result":"Claude AI usage limit reached"})
                );
                std::process::exit(1);
            }
            "codex" => {
                println!(
                    "{}",
                    serde_json::json!({"type":"turn.failed","error":{"message":"You exceeded your current quota"}})
                );
                std::process::exit(1);
            }
            _ => {
                eprintln!("HTTP 429 Too Many Requests");
                std::process::exit(1);
            }
        },
        b if b.starts_with("exit:") => {
            let code: i32 = b[5..].parse().unwrap_or(1);
            eprintln!("fallo simulado");
            std::process::exit(code);
        }
        _ => match mode.as_str() {
            "claude" => println!(
                "{}",
                serde_json::json!({
                    "type": "result", "subtype": "success", "is_error": false,
                    "result": reply, "usage": {"input_tokens": 3, "output_tokens": 2}
                })
            ),
            "codex" => {
                println!(
                    "{}",
                    serde_json::json!({"type":"thread.started","thread_id":"t"})
                );
                println!(
                    "{}",
                    serde_json::json!({"type":"item.completed","item":{"id":"i","type":"agent_message","text":reply}})
                );
                println!(
                    "{}",
                    serde_json::json!({"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":4}})
                );
            }
            _ => println!("{reply}"),
        },
    }
}
