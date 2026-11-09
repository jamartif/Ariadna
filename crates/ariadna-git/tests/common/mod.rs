//! Helpers compartidos por los tests de integración del motor: repos git temporales de verdad.
#![allow(dead_code, clippy::unwrap_used)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

pub mod http;

use ariadna_git::{find_git, GitCliEngine, GitRunner, LocatorOptions, Repository};
use tempfile::TempDir;

/// Repo temporal con identidad fija y rama `main`. Se borra al soltarlo.
pub struct TestRepo {
    pub dir: TempDir,
    pub repo: Repository,
    pub engine: GitCliEngine,
    git: PathBuf,
}

pub fn git_path() -> PathBuf {
    find_git(&LocatorOptions::default())
        .expect("git >= 2.39 instalado")
        .path
}

impl TestRepo {
    /// `git init -b main` con usuario de prueba, sin commits.
    pub fn init() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let git = git_path();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let r = Self {
            repo: Repository {
                path: root.clone(),
                git_dir: root.join(".git"),
            },
            engine: GitCliEngine::new(GitRunner::with_path(git.clone())),
            git,
            dir,
        };
        r.git(&["init", "-q", "-b", "main"]);
        r.git(&["config", "user.name", "Test"]);
        r.git(&["config", "user.email", "test@example.invalid"]);
        r.git(&["config", "commit.gpgsign", "false"]);
        r.git(&["config", "core.autocrlf", "false"]);
        r
    }

    /// `init` + un primer commit con `README.md`.
    pub fn with_initial_commit() -> Self {
        let r = Self::init();
        r.write("README.md", b"hola\n");
        r.git(&["add", "-A"]);
        r.git(&["commit", "-q", "-m", "inicial"]);
        r
    }

    pub fn path(&self) -> &Path {
        &self.repo.path
    }

    /// Ejecuta git de verdad (sin pasar por el motor) y devuelve stdout. Falla el test si git falla.
    pub fn git(&self, args: &[&str]) -> String {
        let out = Command::new(&self.git)
            .args(args)
            .current_dir(self.path())
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} falló: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Como `git` pero devuelve (código, stdout, stderr) sin hacer fallar el test.
    pub fn git_raw(&self, args: &[&str]) -> (i32, String, String) {
        let out = Command::new(&self.git)
            .args(args)
            .current_dir(self.path())
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .output()
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    pub fn write(&self, rel: &str, contents: &[u8]) {
        let p = self.path().join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, contents).unwrap();
    }

    pub fn read(&self, rel: &str) -> Vec<u8> {
        std::fs::read(self.path().join(rel)).unwrap()
    }

    /// `git add -A && git commit -m` y devuelve el SHA.
    pub fn commit_all(&self, msg: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", msg]);
        self.git(&["rev-parse", "HEAD"]).trim().to_string()
    }

    /// Crea un remoto bare en otra carpeta temporal y lo registra como `origin`.
    pub fn with_bare_origin(&self) -> TempDir {
        let bare = tempfile::tempdir().unwrap();
        let st = Command::new(&self.git)
            .args(["init", "-q", "--bare", "-b", "main"])
            .arg(bare.path())
            .status()
            .unwrap();
        assert!(st.success());
        let url = bare.path().to_string_lossy().into_owned();
        self.git(&["remote", "add", "origin", &url]);
        bare
    }
}

pub fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

/// Motor sin ningún `credential.helper` heredado del sistema o del usuario.
pub fn plain_engine() -> GitCliEngine {
    GitCliEngine::new(GitRunner::with_path(git_path()).with_config("credential.helper", ""))
}

/// Carpeta con `repo.git` (bare, rama `main` con un commit) lista para servirse por HTTP «tonto».
pub fn served_bare_repo() -> TempDir {
    let work = TestRepo::with_initial_commit();
    let root = tempfile::tempdir().unwrap();
    let bare = root.path().join("repo.git");
    let st = Command::new(git_path())
        .args(["clone", "-q", "--bare"])
        .arg(work.path())
        .arg(&bare)
        .status()
        .unwrap();
    assert!(st.success());
    let st = Command::new(git_path())
        .arg("update-server-info")
        .current_dir(&bare)
        .status()
        .unwrap();
    assert!(st.success());
    root
}
