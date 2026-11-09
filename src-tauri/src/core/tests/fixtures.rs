//! Fixtures JSON del contrato IPC (`ui/src/lib/api/fixtures/*.json`).
//!
//! Se generan con el motor real sobre repos temporales y se comparan con los ficheros
//! versionados: el test **falla** si difieren. Para regenerarlos de forma consciente:
//! `ARIADNA_UPDATE_FIXTURES=1 cargo test -p ariadna-app fixtures`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use ariadna_git::{GitError, SessionEvent, SessionEventKind, SessionState};
use serde::Serialize;
use serde_json::{Map, Value};

use super::errors::every_git_error;
use super::*;
use crate::core::{ApiError, ApiSessionEvent, ApiSessionEventKind, AppState, TabInfo};
use ariadna_git::{DiffScope, MergeFastForward, MergeResult};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("ui")
        .join("src")
        .join("lib")
        .join("api")
        .join("fixtures")
}

/// Normaliza lo que cambia entre ejecuciones y máquinas: rutas temporales, SHAs, ids de
/// operación y de pestaña, fecha de «recientes» y datos del `git` instalado.
struct Norm {
    root: String,
    shas: HashMap<String, String>,
    ops: HashMap<u64, u64>,
    tabs: HashMap<String, String>,
}

impl Norm {
    fn new(root: &str) -> Self {
        Self {
            root: root.to_owned(),
            shas: HashMap::new(),
            ops: HashMap::new(),
            tabs: HashMap::new(),
        }
    }

    fn is_sha(s: &str) -> bool {
        s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
    }

    fn walk(&mut self, key: Option<&str>, v: &mut Value) {
        match v {
            Value::String(s) => {
                if Self::is_sha(s) {
                    let n = self.shas.len() + 1;
                    *s = self
                        .shas
                        .entry(s.clone())
                        .or_insert_with(|| format!("{n:040x}"))
                        .clone();
                } else if s
                    .strip_prefix("tab-")
                    .is_some_and(|d| d.chars().all(|c| c.is_ascii_digit()))
                    && s.len() > 4
                {
                    let n = self.tabs.len() + 1;
                    *s = self
                        .tabs
                        .entry(s.clone())
                        .or_insert_with(|| format!("tab-{n}"))
                        .clone();
                } else if key == Some("lastOpened") {
                    *s = "2026-01-01T00:00:00Z".to_owned();
                } else if s.contains(&self.root) {
                    // En Windows las rutas de los mensajes llevan `\` (`C:\...\suelta`): tras
                    // sustituir la raíz se unifican los separadores en todo el texto, no solo
                    // cuando el valor empieza por la ruta.
                    let mut t = s.replace(&self.root, "/work");
                    if t.contains("/work") {
                        t = t.replace('\\', "/");
                    }
                    *s = t;
                }
            }
            Value::Number(n) if key == Some("operationId") => {
                let id = n.as_u64().unwrap_or(0);
                let next = u64::try_from(self.ops.len()).unwrap_or(0) + 1;
                let mapped = *self.ops.entry(id).or_insert(next);
                *v = Value::from(mapped);
            }
            Value::Array(items) => {
                for it in items {
                    self.walk(key, it);
                }
            }
            Value::Object(map) => {
                if key == Some("git") {
                    if let (Some(Value::String(_)), Some(Value::String(_))) =
                        (map.get("path"), map.get("version"))
                    {
                        map.insert("path".into(), Value::from("/usr/bin/git"));
                        map.insert("version".into(), Value::from("2.50.0"));
                        return;
                    }
                }
                let keys: Vec<String> = map.keys().cloned().collect();
                for k in keys {
                    if let Some(child) = map.get_mut(&k) {
                        self.walk(Some(&k), child);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Claves ordenadas en todos los niveles, para que el texto no dependa de `preserve_order`.
fn sorted(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<String> = m.keys().cloned().collect();
            keys.sort();
            let mut out = Map::new();
            let mut m = m;
            for k in keys {
                if let Some(child) = m.remove(&k) {
                    out.insert(k, sorted(child));
                }
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sorted).collect()),
        other => other,
    }
}

/// Acumula fixtures: compara con el disco o, en modo actualización, los escribe.
struct Fixtures {
    update: bool,
    norm_root: String,
    problems: Vec<String>,
    written: Vec<String>,
}

impl Fixtures {
    fn new(root: &str) -> Self {
        Self {
            update: std::env::var("ARIADNA_UPDATE_FIXTURES").is_ok_and(|v| v == "1"),
            norm_root: root.to_owned(),
            problems: Vec::new(),
            written: Vec::new(),
        }
    }

    fn check<T: Serialize>(&mut self, name: &str, value: &T) {
        let mut v = serde_json::to_value(value).unwrap();
        Norm::new(&self.norm_root).walk(None, &mut v);
        let mut text = serde_json::to_string_pretty(&sorted(v)).unwrap();
        text.push('\n');
        let path = fixtures_dir().join(name);
        if self.update {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            if std::fs::read_to_string(&path)
                .map(|t| t.replace("\r\n", "\n"))
                .ok()
                .as_deref()
                != Some(text.as_str())
            {
                std::fs::write(&path, &text).unwrap();
                self.written.push(name.to_owned());
            }
            return;
        }
        match std::fs::read_to_string(&path) {
            // Un checkout con `autocrlf` puede haber convertido los saltos de línea.
            Ok(existing) if existing.replace("\r\n", "\n") == text => {}
            Ok(existing) => {
                let existing = existing.replace("\r\n", "\n");
                let corto = |s: Option<&str>| -> String {
                    s.unwrap_or("<fin>").chars().take(160).collect()
                };
                let mut a = existing.lines();
                let mut b = text.lines();
                let mut linea = 0usize;
                let detalle = loop {
                    linea += 1;
                    match (a.next(), b.next()) {
                        (Some(x), Some(y)) if x == y => continue,
                        (x, y) => {
                            break format!("fichero: {}\n    motor:   {}", corto(x), corto(y))
                        }
                    }
                };
                self.problems.push(format!(
                    "{name}: el contenido difiere en la línea {linea}\n    {detalle}"
                ));
            }
            Err(_) => self.problems.push(format!("{name}: no existe")),
        }
    }

    fn finish(self) {
        if self.update {
            eprintln!("fixtures actualizados: {:?}", self.written);
            return;
        }
        assert!(
            self.problems.is_empty(),
            "Los fixtures del contrato IPC no coinciden con la salida del motor: {:#?}\n\
             Si el cambio es intencionado, regenera con \
             `ARIADNA_UPDATE_FIXTURES=1 cargo test -p ariadna-app fixtures` \
             y revisa `ui/src/lib/api/fixtures/` (los tipos de la UI deben seguirlos).",
            self.problems
        );
    }
}

fn commit_on(env: &Env, repo: &std::path::Path, msg: &str, day: u32) {
    let date = format!("2026-01-{day:02}T12:00:00+00:00");
    let out = env
        .command(repo)
        .args(["commit", "-q", "-a", "-m", msg])
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", &date)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn fixtures_del_contrato_coinciden_con_el_motor() {
    let env = Env::new();
    let mut fx = Fixtures::new(&Env::s(&env.root));

    // --- historia determinista ----------------------------------------------------------
    let repo = env.empty_repo("demo");
    let lib: String = (1..=30).map(|i| format!("fn linea_{i}() {{}}\n")).collect();
    env.write(&repo, "README.md", b"# Demo\n");
    env.write(&repo, "src/lib.rs", lib.as_bytes());
    env.write(&repo, "crlf.txt", b"uno\r\ndos\r\ntres\r\n");
    env.write(&repo, "bin.dat", &[0, 1, 2, 0xff, 0, b'A']);
    env.write(&repo, "conflict.txt", b"a\nb\nc\n");
    env.write(&repo, "docs/guia.md", b"# Guia\n");
    env.git(&repo, &["add", "-A"]);
    commit_on(&env, &repo, "inicial", 1);
    env.git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/demo.git",
        ],
    );

    // Cambios previos: varios hunks, CRLF, binario y archivo nuevo.
    let changed = lib
        .replace("fn linea_3() {}\n", "fn linea_3() { 1 }\n")
        .replace("fn linea_27() {}\n", "fn linea_27() { 2 }\n");
    env.write(&repo, "src/lib.rs", changed.as_bytes());
    env.write(&repo, "crlf.txt", b"uno\r\nDOS\r\ntres\r\ncuatro\r\n");
    env.write(&repo, "bin.dat", &[0, 9, 9, 0xfe, 0, b'B']);
    env.write(&repo, "nuevo.txt", b"archivo nuevo\n");
    env.git(&repo, &["add", "nuevo.txt"]);

    let id = env.open(&repo).await;
    let mut diffs = env
        .core
        .diff(&id, DiffScope::Unstaged, vec![])
        .await
        .unwrap();
    diffs.extend(env.core.diff(&id, DiffScope::Staged, vec![]).await.unwrap());
    assert!(
        diffs.iter().any(|d| d.hunks.len() >= 2),
        "algún archivo con varios hunks"
    );
    assert!(diffs.iter().any(|d| d.is_binary));
    assert!(diffs.iter().any(|d| d.is_new_file));
    assert!(diffs.iter().any(|d| d
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .any(|l| l.text.ends_with('\r'))));
    fx.check("file-diffs.json", &diffs);

    env.git(&repo, &["add", "-A"]);
    commit_on(&env, &repo, "cambios previos", 2);

    // Rama con conflicto.
    env.git(&repo, &["checkout", "-q", "-b", "feature"]);
    env.write(&repo, "conflict.txt", b"a\nb-feature\nc\n");
    commit_on(&env, &repo, "feature: cambia b", 3);
    env.git(&repo, &["checkout", "-q", "main"]);
    env.write(&repo, "conflict.txt", b"a\nb-main\nc\n");
    commit_on(&env, &repo, "main: cambia b", 4);

    // La pestaña ya estaba abierta; el merge lo hace el comando real.
    env.core.refresh(&id).await.unwrap();
    let r = env
        .core
        .merge(&id, "feature".into(), MergeFastForward::Allow)
        .await
        .unwrap();
    assert!(matches!(r, MergeResult::Conflicts { .. }));

    // Estado mixto: preparado, sin preparar, MM, sin seguimiento y conflicto.
    env.write(&repo, "README.md", b"# Demo\n\nmas texto\n");
    env.write(&repo, "preparado.txt", b"p\n");
    env.git(&repo, &["add", "preparado.txt"]);
    env.write(&repo, "docs/guia.md", b"# Guia\n\npaso 1\n");
    env.git(&repo, &["add", "docs/guia.md"]);
    env.write(&repo, "docs/guia.md", b"# Guia\n\npaso 1\npaso 2\n");
    env.write(&repo, "notas.md", b"sin seguimiento\n");
    env.core.refresh(&id).await.unwrap();

    let snapshot = env.core.get_snapshot(&id).await.unwrap();
    let st = &snapshot.0.status;
    assert!(st.entries.iter().any(|e| e.is_conflicted));
    assert!(st.entries.iter().any(|e| e.is_untracked));
    assert!(st.entries.iter().any(|e| e.is_staged()));
    assert!(st
        .entries
        .iter()
        .any(|e| !e.is_untracked && e.has_unstaged_change()));
    assert_eq!(
        snapshot.0.integration,
        ariadna_git::IntegrationState::Merging
    );
    fx.check("snapshot.json", &snapshot);
    fx.check("git-status.json", &snapshot.0.status);
    fx.check("branches.json", &snapshot.0.branches);
    fx.check(
        "conflicts.json",
        &env.core.get_conflicts(&id).await.unwrap(),
    );
    fx.check("commits.json", &env.core.log(&id, 20, None).await.unwrap());

    let state: AppState = env.core.app_state();
    assert_eq!(state.tabs.len(), 1);
    let tab: TabInfo = state.tabs[0].clone();
    assert_eq!(tab.conflict_count, 1);
    fx.check("app-state.json", &state);
    fx.check("tab-info.json", &tab);

    // --- errores ---------------------------------------------------------------------------
    let suelta = env.root.join("suelta");
    std::fs::create_dir_all(&suelta).unwrap();
    let real = env
        .core
        .open_repository(&Env::s(&suelta))
        .await
        .unwrap_err();
    assert_eq!(real.code, "NOT_A_REPOSITORY");
    fx.check("api-error.json", &real);
    let mut all: Vec<ApiError> = every_git_error()
        .into_iter()
        .map(|(e, _)| ApiError::from(e))
        .collect();
    all.push(ApiError::repo_missing("/work/demo"));
    fx.check("api-error-codes.json", &all);

    // --- eventos de sesión reales -------------------------------------------------------------
    let (core2, sink2) = env.restart();
    let ev_repo = env.repo("eventos");
    env.write(&ev_repo, "a.txt", b"a\n");
    let ev_id = core2
        .open_repository(&Env::s(&ev_repo))
        .await
        .unwrap()
        .tab
        .tab_id;
    core2.stage(&ev_id, vec!["a.txt".into()]).await.unwrap();
    wait_until("snapshot publicado", || {
        sink2.all().iter().any(|e| {
            matches!(e, AppEvent::Session(s) if matches!(s.kind, ApiSessionEventKind::SnapshotPublished { .. }))
        })
    })
    .await;
    // Con un `index.lock` ajeno: reintentos y fallo LOCKED.
    std::fs::write(ev_repo.join(".git/index.lock"), b"").unwrap();
    env.write(&ev_repo, "b.txt", b"b\n");
    let locked = core2.stage(&ev_id, vec!["b.txt".into()]).await;
    std::fs::remove_file(ev_repo.join(".git/index.lock")).unwrap();
    assert_eq!(locked.unwrap_err().code, "LOCKED");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut events: Vec<ApiSessionEvent> = sink2
        .all()
        .into_iter()
        .filter_map(|e| match e {
            AppEvent::Session(s) => Some(s),
            _ => None,
        })
        .collect();
    let mut picked: Vec<ApiSessionEvent> = Vec::new();
    let mut pick = |f: &dyn Fn(&ApiSessionEventKind) -> bool| {
        let found = events
            .iter()
            .position(|e| f(&e.kind))
            .expect("evento esperado");
        picked.push(events.remove(found));
    };
    pick(&|k| matches!(k, ApiSessionEventKind::OperationStarted { .. }));
    pick(&|k| matches!(k, ApiSessionEventKind::OperationRetrying { .. }));
    pick(&|k| matches!(k, ApiSessionEventKind::OperationFinished { ok: true, .. }));
    pick(&|k| matches!(k, ApiSessionEventKind::OperationFinished { ok: false, .. }));
    pick(&|k| matches!(k, ApiSessionEventKind::SnapshotPublished { .. }));
    // Los que no se pueden provocar de forma estable se construyen con la conversión real.
    let synthetic = |kind| {
        ApiSessionEvent::from(&SessionEvent {
            session_id: ev_id.clone(),
            revision: 2,
            kind,
        })
    };
    picked.push(synthetic(SessionEventKind::StateChanged {
        state: SessionState::Closing,
    }));
    picked.push(synthetic(SessionEventKind::RefreshFailed {
        error: GitError::Timeout {
            command: "git status".into(),
        },
    }));
    fx.check("session-event.json", &picked);

    fx.finish();
}

#[test]
fn normalizador_estabiliza_rutas_shas_y_ids() {
    let mut n = Norm::new("/tmp/xyz");
    let mut v = serde_json::json!({
        "path": "/tmp/xyz/demo",
        "sha": "a".repeat(40),
        "otra": "b".repeat(40),
        "again": "a".repeat(40),
        "operationId": 77,
        "tabId": "tab-42",
        "lastOpened": "2020-05-05T00:00:00Z",
        "git": {"path": "/opt/git", "version": "2.43.0"},
    });
    n.walk(None, &mut v);
    assert_eq!(v["path"], "/work/demo");
    assert_eq!(v["sha"], format!("{:040x}", 1));
    assert_eq!(v["otra"], format!("{:040x}", 2));
    assert_eq!(v["again"], v["sha"]);
    assert_eq!(v["operationId"], 1);
    assert_eq!(v["tabId"], "tab-1");
    assert_eq!(v["lastOpened"], "2026-01-01T00:00:00Z");
    assert_eq!(v["git"]["path"], "/usr/bin/git");
}

#[test]
fn un_fixture_distinto_hace_fallar_la_comprobacion() {
    // Con el modo de actualización apagado, un valor que no coincide queda como problema.
    let mut fx = Fixtures {
        update: false,
        norm_root: "/nada".into(),
        problems: Vec::new(),
        written: Vec::new(),
    };
    fx.check("api-error.json", &serde_json::json!({"code": "OTRO"}));
    fx.check("no-existe-jamas.json", &serde_json::json!(1));
    assert_eq!(fx.problems.len(), 2, "{:?}", fx.problems);
}
