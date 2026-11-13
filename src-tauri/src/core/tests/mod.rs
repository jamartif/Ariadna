//! Pruebas del núcleo sin ventana: git real en carpetas temporales y un `Prompter` falso.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod ai;
mod clone_history;
mod destructive;
mod errors;
mod fixtures;
mod merge;
mod pull_requests;
mod remotes;
mod safe_directory;
mod session;
mod settings;
mod stash;
mod tabs;
mod tags;
mod watch;
mod window_config;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ariadna_git::{find_git, GitInstall, LocatorOptions};
use async_trait::async_trait;
use tempfile::TempDir;

use super::{AppConfig, AppCore, AppEvent, EventSink, Prompter};

/// Pregunta registrada por el `Prompter` falso.
#[derive(Debug, Clone)]
pub struct Asked {
    pub title: String,
    pub message: String,
    pub detail: String,
}

/// Doble del diálogo nativo: contesta lo que se le diga (por turnos o por defecto).
pub struct FakePrompter {
    pub answers: Mutex<VecDeque<bool>>,
    default: AtomicBool,
    pub asked: Mutex<Vec<Asked>>,
    pub folder: Mutex<Option<PathBuf>>,
}

impl FakePrompter {
    pub fn new(default: bool) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(VecDeque::new()),
            default: AtomicBool::new(default),
            asked: Mutex::new(Vec::new()),
            folder: Mutex::new(None),
        })
    }
    pub fn accept_all(&self) {
        self.default.store(true, Ordering::SeqCst);
    }
    pub fn reject_all(&self) {
        self.default.store(false, Ordering::SeqCst);
    }
    pub fn count(&self) -> usize {
        self.asked.lock().unwrap().len()
    }
    pub fn last(&self) -> Asked {
        self.asked
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("ninguna pregunta")
    }
}

#[async_trait]
impl Prompter for FakePrompter {
    async fn confirm(&self, title: &str, message: &str, detail: &str) -> bool {
        self.asked.lock().unwrap().push(Asked {
            title: title.to_owned(),
            message: message.to_owned(),
            detail: detail.to_owned(),
        });
        let next = self.answers.lock().unwrap().pop_front();
        next.unwrap_or_else(|| self.default.load(Ordering::SeqCst))
    }
    async fn pick_folder(&self) -> Option<PathBuf> {
        self.folder.lock().unwrap().clone()
    }
}

/// Colector de eventos.
#[derive(Default)]
pub struct CollectSink {
    pub events: Mutex<Vec<AppEvent>>,
}

impl CollectSink {
    pub fn all(&self) -> Vec<AppEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl EventSink for CollectSink {
    fn emit(&self, event: AppEvent) {
        self.events.lock().unwrap().push(event);
    }
}

pub fn git_install() -> GitInstall {
    find_git(&LocatorOptions::default()).expect("git >= 2.39 instalado")
}

/// Entorno de prueba: carpeta temporal con datos de la app y repos.
pub struct Env {
    pub _tmp: TempDir,
    pub root: PathBuf,
    pub data: PathBuf,
    pub core: AppCore,
    pub prompter: Arc<FakePrompter>,
    pub sink: Arc<CollectSink>,
    pub git: PathBuf,
}

impl Env {
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(tmp.path()).unwrap();
        let data = root.join("data");
        std::fs::create_dir_all(root.join("home")).unwrap();
        let install = git_install();
        let git = install.path.clone();
        let prompter = FakePrompter::new(true);
        let sink = Arc::new(CollectSink::default());
        let core = AppCore::new(
            AppConfig {
                data_dir: data.clone(),
                git: Ok(install),
                legacy_recents: None,
            },
            prompter.clone(),
            sink.clone(),
        );
        Self {
            _tmp: tmp,
            root,
            data,
            core,
            prompter,
            sink,
            git,
        }
    }

    /// Nuevo `AppCore` sobre los mismos datos (como reiniciar la app).
    pub fn restart(&self) -> (AppCore, Arc<CollectSink>) {
        let sink = Arc::new(CollectSink::default());
        let core = AppCore::new(
            AppConfig {
                data_dir: self.data.clone(),
                git: Ok(git_install()),
                legacy_recents: None,
            },
            self.prompter.clone(),
            sink.clone(),
        );
        (core, sink)
    }

    pub fn command(&self, dir: &Path) -> Command {
        let mut c = Command::new(&self.git);
        let home = self.root.join("home");
        c.current_dir(dir)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("XDG_CONFIG_HOME", &home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .args([
                "-c",
                "core.autocrlf=false",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "protocol.file.allow=always",
            ]);
        c
    }

    /// Ejecuta git de verdad; falla el test si git falla.
    pub fn git(&self, dir: &Path, args: &[&str]) -> String {
        let out = self.command(dir).args(args).output().unwrap();
        assert!(
            out.status.success(),
            "git {args:?} falló: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    pub fn write(&self, dir: &Path, rel: &str, bytes: &[u8]) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, bytes).unwrap();
    }

    /// Repo sin commits.
    pub fn empty_repo(&self, name: &str) -> PathBuf {
        let dir = self.root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        self.git(&dir, &["init", "-q", "-b", "main"]);
        self.git(&dir, &["config", "user.name", "Test"]);
        self.git(&dir, &["config", "user.email", "test@example.invalid"]);
        // El motor lanza git con el entorno normal (en Windows, `core.autocrlf=true` de sistema):
        // se fija en el propio repo para que los saltos de línea sean deterministas.
        self.git(&dir, &["config", "core.autocrlf", "false"]);
        dir
    }

    /// Repo con un primer commit (`README.md`).
    pub fn repo(&self, name: &str) -> PathBuf {
        let dir = self.empty_repo(name);
        self.write(&dir, "README.md", b"hola\n");
        self.git(&dir, &["add", "-A"]);
        self.git(&dir, &["commit", "-q", "-m", "inicial"]);
        dir
    }

    pub fn bare(&self, name: &str) -> PathBuf {
        let dir = self.root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        self.git(&dir, &["init", "-q", "--bare", "-b", "main"]);
        dir
    }

    pub fn s(p: &Path) -> String {
        p.to_string_lossy().into_owned()
    }

    /// Abre un repo y devuelve el id de su pestaña.
    pub async fn open(&self, dir: &Path) -> String {
        self.core
            .open_repository(&Self::s(dir))
            .await
            .unwrap()
            .tab
            .tab_id
    }
}

/// Espera (hasta 15 s) a que se cumpla la condición.
pub async fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("tiempo agotado esperando: {what}");
}

/// Instala un hook `pre-commit` que tarda (para tener una mutación en curso).
pub fn slow_pre_commit_hook(repo: &Path, seconds: u32) {
    let hook = repo.join(".git/hooks/pre-commit");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(&hook, format!("#!/bin/sh\nsleep {seconds}\nexit 0\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
