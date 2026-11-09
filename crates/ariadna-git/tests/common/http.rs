//! Servidor HTTP mínimo para tests (hilos estándar, sin dependencias).
//!
//! * `Mode::Status(n)`: responde siempre con ese código (401 con `WWW-Authenticate`).
//! * `Mode::Static`: sirve ficheros de una carpeta (protocolo «tonto» de git: un bare repo con
//!   `git update-server-info`), opcionalmente exigiendo `Authorization: Basic <b64>`.
#![allow(dead_code, clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub enum Mode {
    Status(u16),
    Static {
        root: PathBuf,
        basic: Option<String>,
    },
}

pub struct HttpServer {
    pub port: u16,
    stop: Arc<AtomicBool>,
    /// Líneas de petición recibidas (`GET /x HTTP/1.1`).
    pub requests: Arc<Mutex<Vec<String>>>,
}

pub fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in input.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Puerto en el que no escucha nadie (conexión rehusada).
pub fn closed_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

impl HttpServer {
    pub fn start(mode: Mode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mode = Arc::new(mode);
        let (s2, r2) = (stop.clone(), requests.clone());
        std::thread::spawn(move || {
            while !s2.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((conn, _)) => {
                        let (m, r) = (mode.clone(), r2.clone());
                        std::thread::spawn(move || handle(conn, &m, &r));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        Self {
            port,
            stop,
            requests,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!(
            "http://127.0.0.1:{}/{}",
            self.port,
            path.trim_start_matches('/')
        )
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn respond(conn: &mut TcpStream, status: u16, extra: &str, body: &[u8]) {
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Status",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = conn.write_all(head.as_bytes());
    let _ = conn.write_all(body);
    let _ = conn.flush();
}

fn handle(mut conn: TcpStream, mode: &Mode, log: &Mutex<Vec<String>>) {
    let _ = conn.set_nonblocking(false);
    let _ = conn.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        match conn.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    let mut lines = text.lines();
    let request = lines.next().unwrap_or("").to_string();
    log.lock().unwrap().push(request.clone());
    let auth = lines
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
        .map(|(_, v)| v.trim().to_string());
    let challenge = "WWW-Authenticate: Basic realm=\"Ariadna\"\r\n";
    match mode {
        Mode::Status(401) => respond(&mut conn, 401, challenge, b"no"),
        Mode::Status(n) => respond(&mut conn, *n, "", b"no"),
        Mode::Static { root, basic } => {
            if let Some(b64) = basic {
                if auth.as_deref() != Some(&format!("Basic {b64}")) {
                    return respond(&mut conn, 401, challenge, b"no");
                }
            }
            let path = request.split_whitespace().nth(1).unwrap_or("/");
            let path = path.split('?').next().unwrap_or("/");
            let rel = path.trim_start_matches('/');
            if rel.contains("..") {
                return respond(&mut conn, 404, "", b"");
            }
            match std::fs::read(root.join(rel)) {
                Ok(body) => respond(&mut conn, 200, "", &body),
                Err(_) => respond(&mut conn, 404, "", b""),
            }
        }
    }
}
