//! Utilidades de test: repos temporales reales con `git` de verdad (solo `cfg(test)`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use std::io::Write;

use tempfile::TempDir;

/// Repo temporal aislado de la configuración global del usuario.
pub struct TestRepo {
    _home: TempDir,
    root: PathBuf,
}

/// ¿Hay un `git` ejecutable en el PATH?
pub fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn base_command(home: &Path, cwd: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .env("LC_ALL", "C")
        .args([
            "-c",
            "core.autocrlf=false",
            "-c",
            "core.quotepath=false",
            "-c",
            "commit.gpgsign=false",
        ]);
    cmd
}

impl TestRepo {
    /// Crea un repo vacío en rama `main`. `None` si no hay git (el test se omite).
    pub fn init() -> Option<Self> {
        if !git_available() {
            eprintln!("git no disponible: test omitido");
            return None;
        }
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let repo = Self { _home: home, root };
        repo.git(&["init", "-q", "-b", "main"]);
        Some(repo)
    }

    /// Carpeta raíz del repo.
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Escribe un archivo (crea carpetas intermedias).
    pub fn write(&self, name: &str, bytes: &[u8]) {
        let path = self.root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, bytes).unwrap();
    }

    /// Ejecuta git y devuelve stdout; falla el test si git falla.
    pub fn git(&self, args: &[&str]) -> Vec<u8> {
        let out = base_command(self._home.path(), &self.root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} falló: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out.stdout
    }

    /// Ejecuta git con datos por stdin; falla el test si git falla.
    pub fn git_stdin(&self, args: &[&str], stdin: &[u8]) -> Vec<u8> {
        let mut child = base_command(self._home.path(), &self.root)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(stdin).unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "git {args:?} falló: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out.stdout
    }
}
