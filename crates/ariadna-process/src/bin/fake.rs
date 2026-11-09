//! Proceso de prueba: se comporta según su primer argumento.
//!   echo-stdin | args | env | sleep <ms> | flood <bytes> | exit <code> | stderr <texto>
//!   spawn-child  (lanza un nieto que duerme y luego duerme él también)
use std::io::{Read, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("echo-stdin") => {
            let mut s = Vec::new();
            std::io::stdin().read_to_end(&mut s).ok();
            std::io::stdout().write_all(&s).ok();
        }
        Some("args") => {
            for a in &args[1..] {
                println!("{a}");
            }
        }
        Some("env") => {
            let mut v: Vec<_> = std::env::vars().map(|(k, v)| format!("{k}={v}")).collect();
            v.sort();
            for l in v {
                println!("{l}");
            }
        }
        Some("sleep") => {
            let ms: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1000);
            std::thread::sleep(std::time::Duration::from_millis(ms));
        }
        Some("flood") => {
            let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1024);
            let chunk = vec![b'x'; 8192];
            let mut left = n;
            let mut out = std::io::stdout();
            while left > 0 {
                let k = left.min(chunk.len());
                if out.write_all(&chunk[..k]).is_err() {
                    break;
                }
                left -= k;
            }
        }
        Some("exit") => {
            let c: i32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            std::process::exit(c);
        }
        Some("stderr") => {
            eprint!("{}", args.get(1).cloned().unwrap_or_default());
            std::process::exit(3);
        }
        Some("spawn-child") => {
            let exe = std::env::current_exe().expect("exe");
            let _ = std::process::Command::new(exe)
                .args(["sleep", "30000"])
                .spawn();
            std::thread::sleep(std::time::Duration::from_millis(30000));
        }
        _ => {
            eprintln!("uso: ariadna-proc-fake <modo>");
            std::process::exit(64);
        }
    }
}
