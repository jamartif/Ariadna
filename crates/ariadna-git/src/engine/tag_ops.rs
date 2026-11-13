//! Etiquetas: listar, crear (ligeras y anotadas), borrar en local, publicar y borrar en remoto.
//!
//! Se decide por códigos de salida y por salidas estructuradas (`for-each-ref --format`); no
//! se interpreta texto humano de git. Los nombres se validan antes con
//! [`tag_name_problem`](crate::tag::tag_name_problem) y, como última palabra, con
//! `git check-ref-format`.

use std::ffi::OsString;

use ariadna_process::ProcessOutput;

use super::GitCliEngine;
use crate::error::{truncate, GitError, GitResult};
use crate::models::Repository;
use crate::path_safety::{validate_ref, validate_revision_arg};
use crate::runner::{redact_credentials, CommandKind};
use crate::serving::Confirmation;
use crate::tag::{parse_tag_list, tag_name_problem, Tag, TAG_LIST_FORMAT};

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

fn failed(repo: &Repository, subcommand: &str, out: &ProcessOutput) -> GitError {
    let lock = repo.git_dir.join("index.lock");
    if lock.exists() {
        return GitError::Locked {
            lock: lock.display().to_string(),
        };
    }
    GitError::CommandFailed {
        command: format!("git {subcommand}"),
        exit_code: out.code.unwrap_or(-1),
        stderr: truncate(&redact_credentials(&out.stderr_lossy()), 8_000),
    }
}

fn invalid_name(name: &str, reason: &str) -> GitError {
    GitError::InvalidTagName {
        name: name.to_owned(),
        reason: reason.to_owned(),
    }
}

fn check_name(name: &str) -> GitResult<()> {
    match tag_name_problem(name) {
        None => Ok(()),
        Some(reason) => Err(invalid_name(name, reason)),
    }
}

impl GitCliEngine {
    async fn to_run(
        &self,
        repo: &Repository,
        kind: CommandKind,
        args: &[OsString],
        stdin: Option<Vec<u8>>,
    ) -> GitResult<ProcessOutput> {
        self.runner.run(&repo.path, kind, args, stdin).await
    }

    async fn to_ref_exists(&self, repo: &Repository, name: &str) -> GitResult<bool> {
        let full = format!("refs/tags/{name}");
        let out = self
            .to_run(
                repo,
                CommandKind::Read,
                &os(&["show-ref", "--verify", "--quiet", &full]),
                None,
            )
            .await?;
        match out.code {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            // `show-ref --verify` sale con 128 si la referencia no existe o no es válida.
            Some(128) => Ok(false),
            _ => Err(failed(repo, "show-ref", &out)),
        }
    }

    /// Todas las etiquetas, de la más reciente a la más antigua.
    pub async fn tags(&self, repo: &Repository) -> GitResult<Vec<Tag>> {
        let args = os(&[
            "for-each-ref",
            // El último `--sort` es la clave principal: fecha, y a igualdad el nombre.
            "--sort=-refname",
            "--sort=-creatordate",
            TAG_LIST_FORMAT,
            "refs/tags",
        ]);
        let out = self
            .runner
            .run_ok(&repo.path, CommandKind::Read, &args, None)
            .await?;
        Ok(parse_tag_list(&out.stdout))
    }

    /// Crea una etiqueta en `target` (por defecto HEAD). Con `message` no vacío es anotada.
    /// Nunca pisa una existente ([`GitError::TagExists`]).
    pub async fn create_tag(
        &self,
        repo: &Repository,
        name: &str,
        target: Option<&str>,
        message: Option<&str>,
    ) -> GitResult<Tag> {
        check_name(name)?;
        let full = format!("refs/tags/{name}");
        let fmt = self
            .to_run(
                repo,
                CommandKind::Read,
                &os(&["check-ref-format", &full]),
                None,
            )
            .await?;
        if !fmt.success() {
            return Err(invalid_name(
                name,
                "git lo rechaza como nombre de referencia",
            ));
        }
        if self.to_ref_exists(repo, name).await? {
            return Err(GitError::TagExists {
                name: name.to_owned(),
            });
        }

        let wanted = target.map(str::trim).filter(|t| !t.is_empty());
        if let Some(t) = wanted {
            validate_revision_arg(t).map_err(|_| GitError::InvalidReference(t.to_owned()))?;
        }
        let rev = wanted.unwrap_or("HEAD");
        let resolved = self
            .to_run(
                repo,
                CommandKind::Read,
                &os(&[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{rev}^{{commit}}"),
                ]),
                None,
            )
            .await?;
        let sha = resolved.stdout_lossy().trim().to_owned();
        if !resolved.success() || sha.is_empty() {
            return Err(GitError::InvalidReference(rev.to_owned()));
        }

        let note = message.map(str::trim).filter(|m| !m.is_empty());
        let out = match note {
            Some(m) => {
                self.to_run(
                    repo,
                    CommandKind::Mutate,
                    &os(&["tag", "--no-sign", "-a", "-F", "-", name, &sha]),
                    Some(m.as_bytes().to_vec()),
                )
                .await?
            }
            None => {
                self.to_run(
                    repo,
                    CommandKind::Mutate,
                    &os(&["tag", "--no-sign", name, &sha]),
                    None,
                )
                .await?
            }
        };
        if !out.success() {
            return Err(failed(repo, "tag", &out));
        }
        self.tags(repo)
            .await?
            .into_iter()
            .find(|t| t.name == name)
            .ok_or_else(|| GitError::Parse(format!("no encuentro la etiqueta creada «{name}»")))
    }

    /// Borra una etiqueta **local**. Destructivo (una anotada pierde su mensaje): exige
    /// [`Confirmation`].
    pub async fn delete_tag(
        &self,
        repo: &Repository,
        name: &str,
        _confirmation: &Confirmation,
    ) -> GitResult<()> {
        check_name(name)?;
        if !self.to_ref_exists(repo, name).await? {
            return Err(GitError::TagNotFound {
                name: name.to_owned(),
            });
        }
        let out = self
            .to_run(repo, CommandKind::Mutate, &os(&["tag", "-d", name]), None)
            .await?;
        if out.success() {
            Ok(())
        } else {
            Err(failed(repo, "tag -d", &out))
        }
    }

    /// Publica la etiqueta en `remote` (`git push <remote> refs/tags/<name>`). Git rechaza pisar
    /// una etiqueta remota que apunta a otro objeto.
    pub async fn push_tag(&self, repo: &Repository, remote: &str, name: &str) -> GitResult<()> {
        check_name(name)?;
        validate_ref(remote).map_err(|_| GitError::InvalidReference(remote.to_owned()))?;
        if !self.to_ref_exists(repo, name).await? {
            return Err(GitError::TagNotFound {
                name: name.to_owned(),
            });
        }
        let full = format!("refs/tags/{name}");
        self.runner
            .run_ok(
                &repo.path,
                CommandKind::Network,
                &os(&["push", remote, &format!("{full}:{full}")]),
                None,
            )
            .await?;
        Ok(())
    }

    /// Borra la etiqueta en el **remoto** (`git push <remote> --delete refs/tags/<name>`).
    /// Destructivo para quien la use: exige [`Confirmation`]. No toca la etiqueta local.
    pub async fn delete_remote_tag(
        &self,
        repo: &Repository,
        remote: &str,
        name: &str,
        _confirmation: &Confirmation,
    ) -> GitResult<()> {
        check_name(name)?;
        validate_ref(remote).map_err(|_| GitError::InvalidReference(remote.to_owned()))?;
        self.runner
            .run_ok(
                &repo.path,
                CommandKind::Network,
                &os(&["push", remote, "--delete", &format!("refs/tags/{name}")]),
                None,
            )
            .await?;
        Ok(())
    }
}
