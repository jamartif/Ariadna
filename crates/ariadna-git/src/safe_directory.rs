//! `safe.directory`: aceptar **una** carpeta concreta cuyo propietario no es el usuario.
//!
//! Git se niega a operar en un repositorio cuyo dueño no coincide con quien ejecuta git
//! (discos de red, carpetas de un contenedor/WSL, un `sudo` de por medio) y el error
//! [`GitError::DubiousOwnership`] lleva la ruta. La única salida sin cambiar propietarios es
//! `safe.directory`, y aquí **nunca** se usa `*`: se añade la ruta exacta que git rechazó.
//!
//! Reglas: la ruta tiene que ser absoluta, no puede llevar caracteres de control ni ser `*`, y
//! se escribe con `git config --global --add` (argv, sin secretos). `config_file` solo existe
//! para las pruebas: apunta `GIT_CONFIG_GLOBAL` a un fichero temporal para no tocar la
//! configuración real del usuario que ejecuta los tests.

use std::path::{Path, PathBuf};

use crate::engine::GitCliEngine;
use crate::error::{GitError, GitResult};
use crate::runner::CommandKind;

/// Directorio neutro para un comando que no pertenece a ningún repositorio.
fn neutral_cwd() -> PathBuf {
    std::env::temp_dir()
}

impl GitCliEngine {
    /// Añade `path` a `safe.directory` en la configuración global de git.
    ///
    /// Solo esa ruta (nunca `*`); si ya estaba, `git config --add` deja una entrada repetida,
    /// que es inocua. Falla con [`GitError::SafeDirectoryRejected`] si la ruta no sirve.
    pub async fn allow_safe_directory(
        &self,
        path: &str,
        config_file: Option<&Path>,
    ) -> GitResult<()> {
        let value = path.trim();
        let reason = if value.is_empty() {
            Some("la ruta está vacía")
        } else if value == "*" {
            Some("no se admite «*»: solo la carpeta concreta")
        } else if value.chars().any(char::is_control) {
            Some("lleva caracteres de control")
        } else if !Path::new(value).is_absolute() {
            Some("no es una ruta absoluta")
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(GitError::SafeDirectoryRejected {
                path: value.to_owned(),
                reason: reason.to_owned(),
            });
        }
        let cfg = config_file.map(|p| p.to_string_lossy().into_owned());
        let env: Vec<(&str, &str)> = cfg
            .iter()
            .map(|c| ("GIT_CONFIG_GLOBAL", c.as_str()))
            .collect();
        let args = [
            std::ffi::OsString::from("config"),
            std::ffi::OsString::from("--global"),
            std::ffi::OsString::from("--add"),
            std::ffi::OsString::from("safe.directory"),
            std::ffi::OsString::from(value),
        ];
        self.runner
            .run_with_env(&neutral_cwd(), CommandKind::Mutate, &args, None, &env)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::git_locator::{find_git, LocatorOptions};
    use crate::runner::GitRunner;

    fn engine() -> GitCliEngine {
        GitCliEngine::new(GitRunner::new(
            &find_git(&LocatorOptions::default()).unwrap(),
        ))
    }

    async fn entries(e: &GitCliEngine, cfg: &Path) -> Vec<String> {
        let args = [
            std::ffi::OsString::from("config"),
            std::ffi::OsString::from("--global"),
            std::ffi::OsString::from("--get-all"),
            std::ffi::OsString::from("safe.directory"),
        ];
        let cfg = cfg.to_string_lossy().into_owned();
        let out = e
            .runner
            .run_with_env(
                &neutral_cwd(),
                CommandKind::Read,
                &args,
                None,
                &[("GIT_CONFIG_GLOBAL", cfg.as_str())],
            )
            .await
            .unwrap();
        out.stdout_lossy()
            .lines()
            .map(|l| l.trim().to_owned())
            .filter(|l| !l.is_empty())
            .collect()
    }

    #[tokio::test]
    async fn anade_solo_la_ruta_concreta_y_sobrevive_a_la_segunda_llamada() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("gitconfig");
        let e = engine();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let repo_s = repo.to_string_lossy().into_owned();
        e.allow_safe_directory(&repo_s, Some(&cfg)).await.unwrap();
        assert_eq!(entries(&e, &cfg).await, vec![repo_s.clone()]);
        // Idempotente en el sentido de que no rompe: la entrada repetida es inocua.
        e.allow_safe_directory(&repo_s, Some(&cfg)).await.unwrap();
        assert!(entries(&e, &cfg).await.iter().all(|x| *x == repo_s));
    }

    #[tokio::test]
    async fn nunca_admite_asterisco_ni_rutas_que_no_sirven() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("gitconfig");
        let e = engine();
        for bad in ["", "*", "  *  ", "relativa/repo", "/tmp/a\nb"] {
            let err = e.allow_safe_directory(bad, Some(&cfg)).await.unwrap_err();
            assert!(
                matches!(err, GitError::SafeDirectoryRejected { .. }),
                "{bad:?} → {err:?}"
            );
            let text = format!("{err}");
            assert!(!text.contains("safe.directory = *"), "{text}");
        }
        assert!(entries(&e, &cfg).await.is_empty(), "no se escribió nada");
    }
}
