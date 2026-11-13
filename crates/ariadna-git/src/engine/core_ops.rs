//! Operaciones núcleo del motor: abrir repo, status, stage/unstage (archivo y hunk), commit,
//! amend, log, búsqueda y diff.
//!
//! Reglas (spec §4/§4.1):
//! * Todo git pasa por [`GitRunner`](crate::GitRunner); nunca se lanza un proceso aquí.
//! * Las decisiones se toman con **códigos de salida** y salidas estructuradas (`-z`,
//!   `--format`), no con mensajes humanos de git. No hay ninguna excepción en este archivo.
//! * Las rutas se validan con `path_safety` antes de llegar a git y se pasan por stdin
//!   (`--pathspec-from-file=- --pathspec-file-nul`) o tras `--`, siempre literales.
//! * Los diffs se leen como bytes con `--no-ext-diff --no-textconv` y prefijos fijos.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use ariadna_process::ProcessOutput;

use super::GitCliEngine;
use crate::diff::parse_diff;
use crate::error::{truncate, GitError, GitResult};
use crate::listings::parse_commit_log;
use crate::models::{
    bytes_to_os_string, Commit, DiffHunk, DiffScope, FileDiff, GitPath, GitStatus, Repository,
};
use crate::path_safety::{
    git_pathspec_args, pathspec_stdin, validate_repo_path_components, validate_revision_arg,
    PathSafetyError,
};
use crate::porcelain::parse_status;
use crate::runner::{redact_credentials, CommandKind};
use crate::serving::Confirmation;

/// A partir de este número de rutas, `stage`/`unstage` dejan de usar pathspecs: la
/// coincidencia de pathspecs de git es cuadrática (≈ 35 s con 50 000 rutas) y las órdenes
/// de índice (`update-index`) son lineales. Ver `specs/rust/rendimiento.md`.
const BULK_PATHS: usize = 2_000;

/// Máximo de commits que devuelven `log` y `search_log`.
pub(super) const MAX_LOG: usize = 2_000;

/// Campos separados por `0x1f`, commits terminados en `0x1e` (ver `parse_commit_log`).
pub(super) const LOG_FORMAT: &str =
    "--pretty=format:%H%x1f%P%x1f%an%x1f%ae%x1f%aI%x1f%s%x1f%b%x1f%D%x1e";

/// Opciones comunes de diff: salida estable, sin programas externos del repo.
const DIFF_COMMON: [&str; 6] = [
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--unified=3",
];

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

fn unsafe_path(path: &[u8], _why: PathSafetyError) -> GitError {
    GitError::PathEscapesRepository {
        path: String::from_utf8_lossy(path).into_owned(),
    }
}

/// Valida todas las rutas (antes de pasar nada a git).
fn validate_all(paths: &[GitPath]) -> GitResult<()> {
    for p in paths {
        validate_repo_path_components(p.as_bytes()).map_err(|e| unsafe_path(p.as_bytes(), e))?;
    }
    Ok(())
}

/// Cuerpo NUL-separado para `--pathspec-from-file=-`, con las rutas ya validadas.
fn validated_stdin(paths: &[GitPath]) -> GitResult<Vec<u8>> {
    validate_all(paths)?;
    pathspec_stdin(paths.iter().map(GitPath::as_bytes)).map_err(|e| unsafe_path(b"", e))
}

/// Rutas validadas, literales, separadas por NUL (sin `./`: no son pathspecs).
fn validated_nul_list(paths: &[GitPath]) -> GitResult<Vec<u8>> {
    validate_all(paths)?;
    let mut out = Vec::with_capacity(paths.iter().map(|p| p.as_bytes().len() + 1).sum());
    for p in paths {
        out.extend_from_slice(p.as_bytes());
        out.push(0);
    }
    Ok(out)
}

fn validated_pathspec_args(paths: &[GitPath]) -> GitResult<Vec<OsString>> {
    validate_all(paths)?;
    git_pathspec_args(paths.iter().map(GitPath::as_bytes)).map_err(|e| unsafe_path(b"", e))
}

fn revision(rev: &str) -> GitResult<()> {
    validate_revision_arg(rev).map_err(|_| GitError::InvalidReference(rev.to_owned()))
}

/// Error para un código de salida inesperado en una llamada hecha con `run` (no `run_ok`).
fn unexpected(args: &[OsString], out: &ProcessOutput) -> GitError {
    GitError::CommandFailed {
        command: args.first().map_or_else(
            || "git".to_string(),
            |a| format!("git {}", a.to_string_lossy()),
        ),
        exit_code: out.code.unwrap_or(-1),
        stderr: truncate(&redact_credentials(&out.stderr_lossy()), 8_000),
    }
}

fn trim_eol(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && matches!(bytes[end - 1], b'\n' | b'\r') {
        end -= 1;
    }
    &bytes[..end]
}

fn canonical_or(path: PathBuf) -> PathBuf {
    dunce::canonicalize(&path).unwrap_or(path)
}

fn is_hex_hash(text: &str) -> bool {
    (4..=64).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Líneas `<sha> <epoch>`.
fn parse_stamps(bytes: &[u8]) -> Vec<(String, i64)> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let sha = it.next()?;
            let time = it.next()?.parse::<i64>().ok()?;
            it.next().is_none().then(|| (sha.to_owned(), time))
        })
        .collect()
}

impl GitCliEngine {
    // ---- helpers privados -------------------------------------------------------------

    async fn read(&self, repo: &Repository, args: &[OsString]) -> GitResult<ProcessOutput> {
        let out = self
            .runner
            .run_ok(&repo.path, CommandKind::Read, args, None)
            .await?;
        // Si la salida superó el tope del runner (64 MiB) está cortada a media línea: un
        // diff o un status incompletos serían peor que un error claro.
        if out.truncated {
            return Err(GitError::Io(format!(
                "La salida de {} es demasiado grande (más de 64 MiB). Pide una parte más pequeña (una ruta concreta).",
                args.first().map_or_else(|| "git".into(), |a| format!("git {}", a.to_string_lossy()))
            )));
        }
        Ok(out)
    }

    async fn read_raw(&self, repo: &Repository, args: &[OsString]) -> GitResult<ProcessOutput> {
        self.runner
            .run(&repo.path, CommandKind::Read, args, None)
            .await
    }

    async fn mutate(
        &self,
        repo: &Repository,
        args: &[OsString],
        stdin: Option<Vec<u8>>,
    ) -> GitResult<ProcessOutput> {
        self.runner
            .run_ok(&repo.path, CommandKind::Mutate, args, stdin)
            .await
    }

    /// `true` si `HEAD` apunta a un commit (código de salida de `rev-parse --verify`).
    async fn has_head(&self, repo: &Repository) -> GitResult<bool> {
        let a = os(&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]);
        Ok(self.read_raw(repo, &a).await?.success())
    }

    /// `true` si HEAD es una rama; `false` si está detached.
    /// `symbolic-ref -q`: 0 = rama, 1 = detached, otro = fallo.
    async fn head_is_branch(&self, repo: &Repository) -> GitResult<bool> {
        let a = os(&["symbolic-ref", "-q", "HEAD"]);
        let out = self.read_raw(repo, &a).await?;
        match out.code {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(unexpected(&a, &out)),
        }
    }

    async fn head_sha(&self, repo: &Repository) -> GitResult<String> {
        let out = self
            .read(repo, &os(&["rev-parse", "--verify", "HEAD"]))
            .await?;
        Ok(out.stdout_lossy().trim().to_owned())
    }

    // ---- API --------------------------------------------------------------------------

    /// Sube desde `path` (archivo o carpeta) hasta la raíz del repo.
    pub async fn open_repository(&self, path: &Path) -> GitResult<Repository> {
        let shown = path.display().to_string();
        if !path.exists() {
            return Err(GitError::NotARepository { path: shown });
        }
        let dir: PathBuf = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent()
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
        };
        let top = match self
            .runner
            .run_ok(
                &dir,
                CommandKind::Read,
                &os(&["rev-parse", "--show-toplevel"]),
                None,
            )
            .await
        {
            Ok(out) => out,
            // Fuera de un repo (o repo bare): git sale con error; no se lee su texto.
            Err(GitError::CommandFailed { .. }) => {
                return Err(GitError::NotARepository { path: shown })
            }
            Err(e) => return Err(e),
        };
        let root = canonical_or(PathBuf::from(bytes_to_os_string(trim_eol(&top.stdout))));
        let git_dir = self
            .runner
            .run_ok(
                &root,
                CommandKind::Read,
                &os(&["rev-parse", "--absolute-git-dir"]),
                None,
            )
            .await?;
        let git_dir = canonical_or(PathBuf::from(bytes_to_os_string(trim_eol(&git_dir.stdout))));
        Ok(Repository {
            path: root,
            git_dir,
        })
    }

    pub async fn status(&self, repo: &Repository, include_ignored: bool) -> GitResult<GitStatus> {
        let mut args = os(&["status", "--porcelain=v2", "-z", "--branch", "-uall"]);
        if include_ignored {
            args.push(OsString::from("--ignored=matching"));
        }
        let out = self.read(repo, &args).await?;
        Ok(parse_status(&out.stdout))
    }

    pub async fn stage(&self, repo: &Repository, paths: &[GitPath]) -> GitResult<()> {
        if paths.is_empty() {
            return Ok(());
        }
        if paths.len() > BULK_PATHS {
            // Lineal; trata las rutas como archivos literales (ni globs ni carpetas) y
            // registra los borrados igual que `git add`.
            let stdin = validated_nul_list(paths)?;
            let args = os(&["update-index", "--add", "--remove", "-z", "--stdin"]);
            self.mutate(repo, &args, Some(stdin)).await?;
            return Ok(());
        }
        let stdin = validated_stdin(paths)?;
        let args = os(&["add", "--pathspec-from-file=-", "--pathspec-file-nul"]);
        self.mutate(repo, &args, Some(stdin)).await?;
        Ok(())
    }

    pub async fn unstage(&self, repo: &Repository, paths: &[GitPath]) -> GitResult<()> {
        if paths.is_empty() {
            return Ok(());
        }
        if paths.len() > BULK_PATHS && self.has_head(repo).await? {
            return self.unstage_bulk(repo, paths).await;
        }
        let stdin = validated_stdin(paths)?;
        // Sin commits `restore --staged` no tiene de dónde restaurar; `reset` sí funciona.
        let args = if self.has_head(repo).await? {
            os(&[
                "restore",
                "--staged",
                "--pathspec-from-file=-",
                "--pathspec-file-nul",
            ])
        } else {
            os(&[
                "reset",
                "-q",
                "--pathspec-from-file=-",
                "--pathspec-file-nul",
            ])
        };
        self.mutate(repo, &args, Some(stdin)).await?;
        Ok(())
    }

    /// `unstage` de muchas rutas en tiempo lineal: copia al índice la entrada de `HEAD`
    /// de cada ruta (`ls-tree -r -z` + `update-index --index-info`) y quita del índice las
    /// que `HEAD` no tiene (modo 0). Las rutas son archivos literales.
    async fn unstage_bulk(&self, repo: &Repository, paths: &[GitPath]) -> GitResult<()> {
        validate_all(paths)?;
        let wanted: HashSet<&[u8]> = paths.iter().map(GitPath::as_bytes).collect();
        let mut found: HashSet<&[u8]> = HashSet::new();
        let tree = self
            .read(repo, &os(&["ls-tree", "-r", "-z", "--full-tree", "HEAD"]))
            .await?;
        let zeros = "0".repeat(self.head_sha(repo).await?.len());
        let mut body = Vec::new();
        for record in tree.stdout.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            // `<modo> <tipo> <sha>\t<ruta>`: es justo el formato de `--index-info`.
            let Some(tab) = record.iter().position(|b| *b == b'\t') else {
                continue;
            };
            if let Some(path) = wanted.get(&record[tab + 1..]) {
                found.insert(*path);
                body.extend_from_slice(record);
                body.push(0);
            }
        }
        for path in paths.iter().map(GitPath::as_bytes) {
            if !found.contains(path) {
                body.extend_from_slice(format!("0 {zeros}\t").as_bytes());
                body.extend_from_slice(path);
                body.push(0);
            }
        }
        self.mutate(
            repo,
            &os(&["update-index", "-z", "--index-info"]),
            Some(body),
        )
        .await?;
        Ok(())
    }

    pub async fn stage_hunk(
        &self,
        repo: &Repository,
        hunk: &DiffHunk,
        path: &GitPath,
    ) -> GitResult<()> {
        self.apply_hunk(repo, hunk, path, false).await
    }

    pub async fn unstage_hunk(
        &self,
        repo: &Repository,
        hunk: &DiffHunk,
        path: &GitPath,
    ) -> GitResult<()> {
        self.apply_hunk(repo, hunk, path, true).await
    }

    /// Localiza el hunk en el diff ACTUAL y aplica un parche de sus bytes originales al
    /// índice. Para archivos sin seguimiento el diff viene de `--no-index` (parche de
    /// «archivo nuevo»), así que no hace falta `add -N` ni deshacerlo si falla.
    async fn apply_hunk(
        &self,
        repo: &Repository,
        hunk: &DiffHunk,
        path: &GitPath,
        reverse: bool,
    ) -> GitResult<()> {
        validate_all(std::slice::from_ref(path))?;
        let scope = if reverse {
            DiffScope::Staged
        } else {
            DiffScope::Unstaged
        };
        let files = self.diff(repo, &scope, std::slice::from_ref(path)).await?;
        let not_found = || GitError::HunkNotFound {
            path: path.to_string_lossy().into_owned(),
        };
        let file = files
            .iter()
            .find(|f| f.path == *path || f.old_path.as_ref() == Some(path))
            .ok_or_else(not_found)?;
        let index = file
            .hunks
            .iter()
            .position(|h| h.matches_range(hunk))
            .ok_or_else(not_found)?;
        let patch = file.patch_for_hunks(&[index]).map_err(|_| not_found())?;
        let mut args = os(&["apply", "--whitespace=nowarn", "--cached"]);
        if reverse {
            args.push(OsString::from("--reverse"));
        }
        args.push(OsString::from("-"));
        self.mutate(repo, &args, Some(patch)).await?;
        Ok(())
    }

    pub async fn stage_lines(
        &self,
        repo: &Repository,
        hunk: &DiffHunk,
        lines: &[u32],
        path: &GitPath,
    ) -> GitResult<()> {
        self.apply_lines(repo, hunk, lines, path, false).await
    }

    pub async fn unstage_lines(
        &self,
        repo: &Repository,
        hunk: &DiffHunk,
        lines: &[u32],
        path: &GitPath,
    ) -> GitResult<()> {
        self.apply_lines(repo, hunk, lines, path, true).await
    }

    /// Como [`apply_hunk`](Self::apply_hunk) pero con un parche de líneas sueltas
    /// (`FileDiff::patch_for_lines`). El hunk recibido se compara con el actual, línea a
    /// línea, para no aplicar a ciegas una selección hecha sobre un diff ya viejo.
    async fn apply_lines(
        &self,
        repo: &Repository,
        hunk: &DiffHunk,
        lines: &[u32],
        path: &GitPath,
        reverse: bool,
    ) -> GitResult<()> {
        validate_all(std::slice::from_ref(path))?;
        let shown = || path.to_string_lossy().into_owned();
        let rejected = |reason: String| GitError::PatchRejected {
            path: shown(),
            reason,
        };
        let scope = if reverse {
            DiffScope::Staged
        } else {
            DiffScope::Unstaged
        };
        let files = self.diff(repo, &scope, std::slice::from_ref(path)).await?;
        let not_found = || GitError::HunkNotFound { path: shown() };
        let file = files
            .iter()
            .find(|f| f.path == *path || f.old_path.as_ref() == Some(path))
            .ok_or_else(not_found)?;
        let index = file
            .hunks
            .iter()
            .position(|h| h.matches_range(hunk))
            .ok_or_else(not_found)?;
        let current = &file.hunks[index];
        if hunk.lines.len() != current.lines.len() {
            return Err(not_found());
        }
        let mut selected = Vec::with_capacity(lines.len());
        for &i in lines {
            let i = i as usize;
            let (Some(mine), Some(now)) = (hunk.lines.get(i), current.lines.get(i)) else {
                return Err(rejected(format!("la línea {i} no existe en el hunk")));
            };
            if mine.kind != now.kind || mine.text != now.text {
                return Err(not_found());
            }
            selected.push(i);
        }
        let patch = file
            .patch_for_lines(index, &selected, reverse)
            .map_err(|e| rejected(e.to_string()))?;
        let mut args = os(&["apply", "--whitespace=nowarn", "--cached", "--recount"]);
        if reverse {
            args.push(OsString::from("--reverse"));
        }
        args.push(OsString::from("-"));
        match self.mutate(repo, &args, Some(patch)).await {
            Ok(_) => Ok(()),
            Err(GitError::CommandFailed {
                command, stderr, ..
            }) if command == "git apply" => Err(rejected(stderr.trim().to_owned())),
            Err(e) => Err(e),
        }
    }

    /// Hace commit de lo preparado y devuelve el SHA. Ejecuta los hooks del repo.
    pub async fn commit(
        &self,
        repo: &Repository,
        message: &str,
        allow_detached: bool,
    ) -> GitResult<String> {
        if message.trim().is_empty() {
            return Err(GitError::EmptyCommitMessage);
        }
        if !allow_detached && !self.head_is_branch(repo).await? {
            return Err(GitError::DetachedHeadCommit);
        }
        // Un merge resuelto sin diferencias contra HEAD es un commit válido.
        let merging = repo.git_dir.join("MERGE_HEAD").exists();
        if !merging {
            // `--quiet`: 0 = índice igual a HEAD (nada que commitear), 1 = hay cambios.
            let a = os(&["diff", "--cached", "--quiet", "--no-ext-diff"]);
            let out = self.read_raw(repo, &a).await?;
            match out.code {
                Some(0) => return Err(GitError::NothingToCommit),
                Some(1) => {}
                _ => return Err(unexpected(&a, &out)),
            }
        }
        let args = os(&["commit", "-F", "-"]);
        self.mutate(repo, &args, Some(message.as_bytes().to_vec()))
            .await?;
        self.head_sha(repo).await
    }

    /// `commit --amend` solo si el commit no está publicado ni hay nada en curso.
    pub async fn amend_last_commit(&self, repo: &Repository, message: &str) -> GitResult<String> {
        if message.trim().is_empty() {
            return Err(GitError::EmptyCommitMessage);
        }
        let cannot = |reason: &str| GitError::CannotAmend {
            reason: reason.to_owned(),
        };
        if !self.has_head(repo).await? {
            return Err(cannot("Todavía no hay ningún commit que corregir."));
        }
        if !self.head_is_branch(repo).await? {
            return Err(cannot(
                "HEAD no está en una rama. Crea una rama antes de corregir el commit.",
            ));
        }
        let in_progress = [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-merge",
            "rebase-apply",
        ]
        .iter()
        .any(|f| repo.git_dir.join(f).exists());
        if in_progress {
            return Err(cannot(
                "Hay un merge, rebase o cherry-pick en curso. Termínalo o abórtalo antes.",
            ));
        }
        // Nombre completo (`refs/remotes/…`): `refname:short` de `origin/HEAD` es ambiguo.
        let published = self
            .read(
                repo,
                &os(&["branch", "-r", "--contains", "HEAD", "--format=%(refname)"]),
            )
            .await?;
        let first = published
            .stdout_lossy()
            .lines()
            .map(str::trim)
            .find(|r| !r.is_empty() && !r.ends_with("/HEAD"))
            .map(|r| r.strip_prefix("refs/remotes/").unwrap_or(r).to_owned());
        if let Some(remote_ref) = first {
            return Err(GitError::CannotAmend {
                reason: format!(
                    "El último commit ya está en {remote_ref}. Corregirlo obligaría a un push forzado."
                ),
            });
        }
        self.mutate(
            repo,
            &os(&["commit", "--amend", "-F", "-"]),
            Some(message.as_bytes().to_vec()),
        )
        .await?;
        self.head_sha(repo).await
    }

    pub async fn log(&self, repo: &Repository, limit: usize) -> GitResult<Vec<Commit>> {
        if !self.has_head(repo).await? {
            return Ok(Vec::new());
        }
        let capped = limit.clamp(1, MAX_LOG);
        let mut args = os(&["log", "-n"]);
        args.push(OsString::from(capped.to_string()));
        args.extend(os(&["--no-color", "--no-show-signature", LOG_FORMAT]));
        let out = self.read(repo, &args).await?;
        Ok(parse_commit_log(&out.stdout))
    }

    /// Página del historial de HEAD: `limit` commits tras saltar `skip` (`git log --skip`).
    pub async fn log_page(
        &self,
        repo: &Repository,
        skip: usize,
        limit: usize,
    ) -> GitResult<Vec<Commit>> {
        if !self.has_head(repo).await? {
            return Ok(Vec::new());
        }
        let capped = limit.clamp(1, MAX_LOG);
        let mut args = os(&["log", "-n"]);
        args.push(OsString::from(capped.to_string()));
        args.push(OsString::from(format!("--skip={}", skip.min(10_000_000))));
        args.extend(os(&["--no-color", "--no-show-signature", LOG_FORMAT]));
        let out = self.read(repo, &args).await?;
        Ok(parse_commit_log(&out.stdout))
    }

    /// Busca (sin distinguir mayúsculas, texto literal) en mensaje, autor y prefijo de hash
    /// dentro del historial de HEAD. `--grep` y `--author` se combinan con AND en git; aquí
    /// se quiere OR, por eso son dos pasadas. De más nuevo a más viejo, sin duplicados.
    pub async fn search_log(
        &self,
        repo: &Repository,
        query: &str,
        limit: usize,
    ) -> GitResult<Vec<Commit>> {
        let needle = query.trim();
        if needle.is_empty() {
            return self.log(repo, limit).await;
        }
        if needle.contains('\0') || needle.contains('\n') {
            return Err(GitError::InvalidReference(needle.to_owned()));
        }
        if !self.has_head(repo).await? {
            return Ok(Vec::new());
        }
        let capped = limit.clamp(1, MAX_LOG);
        let stamp_args = |filter: String| {
            let mut a = os(&["log", "-n"]);
            a.push(OsString::from(capped.to_string()));
            a.extend(os(&[
                "--no-color",
                "--no-show-signature",
                "--format=%H %ct",
                "--regexp-ignore-case",
                "--fixed-strings",
            ]));
            a.push(OsString::from(filter));
            a
        };
        let mut candidates = self
            .stamps(repo, &stamp_args(format!("--grep={needle}")))
            .await?;
        candidates.extend(
            self.stamps(repo, &stamp_args(format!("--author={needle}")))
                .await?,
        );
        if is_hex_hash(needle) {
            // Un hash que no resuelve (o es ambiguo) simplemente no aporta resultados.
            let mut a = os(&["log", "-n", "1", "--no-show-signature", "--format=%H %ct"]);
            a.push(OsString::from(format!("{needle}^{{commit}}")));
            a.push(OsString::from("--"));
            let out = self.read_raw(repo, &a).await?;
            if out.success() {
                for (sha, time) in parse_stamps(&out.stdout) {
                    // Solo cuenta si está en el historial de HEAD (incluido HEAD).
                    let anc = os(&["merge-base", "--is-ancestor", &sha, "HEAD"]);
                    if self.read_raw(repo, &anc).await?.success() {
                        candidates.push((sha, time));
                    }
                }
            }
        }
        let mut seen = HashSet::new();
        let mut unique: Vec<(String, i64)> = candidates
            .into_iter()
            .filter(|(sha, _)| seen.insert(sha.clone()))
            .collect();
        // Estable: con la misma fecha se respeta el orden en que git los recorrió.
        unique.sort_by_key(|c| std::cmp::Reverse(c.1));
        unique.truncate(capped);
        if unique.is_empty() {
            return Ok(Vec::new());
        }
        let mut args = os(&[
            "log",
            "--no-walk=unsorted",
            "--no-color",
            "--no-show-signature",
        ]);
        args.push(OsString::from(LOG_FORMAT));
        args.extend(unique.iter().map(|(sha, _)| OsString::from(sha)));
        args.push(OsString::from("--"));
        let out = self.read(repo, &args).await?;
        let mut commits = parse_commit_log(&out.stdout);
        commits.truncate(capped);
        Ok(commits)
    }

    async fn stamps(&self, repo: &Repository, args: &[OsString]) -> GitResult<Vec<(String, i64)>> {
        let out = self.read(repo, args).await?;
        Ok(parse_stamps(&out.stdout))
    }

    /// Diff como bytes → [`FileDiff`]. En `Unstaged`, con rutas explícitas, añade los
    /// archivos sin seguimiento como «archivo nuevo» (`diff --no-index` contra `/dev/null`),
    /// igual que el Swift.
    pub async fn diff(
        &self,
        repo: &Repository,
        scope: &DiffScope,
        paths: &[GitPath],
    ) -> GitResult<Vec<FileDiff>> {
        // Con miles de rutas el pathspec no cabe en argv (E2BIG) y `git diff` no lee rutas
        // de stdin: se pide el diff completo y se filtra aquí (lineal).
        let bulk = paths.len() > BULK_PATHS;
        let tail = if paths.is_empty() || bulk {
            if bulk {
                validate_all(paths)?;
            }
            Vec::new()
        } else {
            validated_pathspec_args(paths)?
        };
        let mut args = os(&["diff"]);
        let mut revs: Vec<OsString> = Vec::new();
        match scope {
            DiffScope::Unstaged => {}
            DiffScope::Staged => args.push(OsString::from("--cached")),
            DiffScope::Commit(rev) => {
                revision(rev)?;
                args = os(&[
                    "show",
                    "-m",
                    "--first-parent",
                    "--format=",
                    "--patch",
                    "--root",
                ]);
                revs.push(OsString::from(rev));
            }
            DiffScope::Between { base, head } => {
                revision(base)?;
                revision(head)?;
                revs.push(OsString::from(base));
                revs.push(OsString::from(head));
            }
        }
        args.extend(DIFF_COMMON.iter().map(OsString::from));
        if !revs.is_empty() {
            args.push(OsString::from("--end-of-options"));
            args.extend(revs);
            if tail.is_empty() {
                args.push(OsString::from("--"));
            }
        }
        args.extend(tail);
        let out = self.read(repo, &args).await?;
        let mut parsed = parse_diff(&out.stdout);
        if bulk {
            let wanted: HashSet<&[u8]> = paths.iter().map(GitPath::as_bytes).collect();
            parsed.retain(|f| {
                wanted.contains(f.path.as_bytes())
                    || f.old_path
                        .as_ref()
                        .is_some_and(|o| wanted.contains(o.as_bytes()))
            });
        }
        if matches!(scope, DiffScope::Unstaged) && !paths.is_empty() {
            parsed.extend(self.untracked_diffs(repo, paths).await?);
        }
        Ok(parsed)
    }

    async fn untracked_diffs(
        &self,
        repo: &Repository,
        paths: &[GitPath],
    ) -> GitResult<Vec<FileDiff>> {
        let mut ls = os(&["ls-files", "--others", "--exclude-standard", "-z"]);
        let bulk = paths.len() > BULK_PATHS;
        let wanted: HashSet<&[u8]> = if bulk {
            validate_all(paths)?;
            paths.iter().map(GitPath::as_bytes).collect()
        } else {
            ls.extend(validated_pathspec_args(paths)?);
            HashSet::new()
        };
        let listed = self.read(repo, &ls).await?;
        let mut extra = Vec::new();
        for raw in listed
            .stdout
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty() && (!bulk || wanted.contains(p)))
        {
            let mut a = os(&["diff", "--no-index"]);
            a.extend(DIFF_COMMON.iter().map(OsString::from));
            a.push(OsString::from("--"));
            a.push(OsString::from("/dev/null"));
            // Ruta que git acaba de listar; tras `--` un `-` inicial no es una opción, y sin
            // prefijo `./` las cabeceras del diff salen como `a/-rf`, igual que en un diff normal.
            a.push(bytes_to_os_string(raw));
            let out = self.read_raw(repo, &a).await?;
            // 1 = hay diferencias contra /dev/null (lo normal).
            if !matches!(out.code, Some(0 | 1)) {
                return Err(unexpected(&a, &out));
            }
            extra.extend(parse_diff(&out.stdout));
        }
        Ok(extra)
    }

    /// Descarta los cambios del árbol de trabajo de `paths` (`restore --worktree`, desde el
    /// índice). La [`Confirmation`] es la puerta: sin ella no se puede llamar.
    ///
    /// Igual que el Swift, **no** toca archivos sin seguimiento: git rechaza una ruta que no
    /// conoce y la operación entera falla sin cambiar nada.
    pub async fn restore_worktree(
        &self,
        repo: &Repository,
        paths: &[GitPath],
        _confirmation: &Confirmation,
    ) -> GitResult<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let stdin = validated_stdin(paths)?;
        let args = os(&[
            "restore",
            "--worktree",
            "--pathspec-from-file=-",
            "--pathspec-file-nul",
        ]);
        self.mutate(repo, &args, Some(stdin)).await?;
        Ok(())
    }
}
