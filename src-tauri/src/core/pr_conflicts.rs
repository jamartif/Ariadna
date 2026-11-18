//! Conflictos de una pull request resueltos con IA.
//!
//! Flujo, en tres pasos separados para que nada se escriba sin permiso:
//!
//! 1. [`AppCore::pr_conflicts`]: baja las referencias que necesita de la PR (`git fetch` de
//!    `refs/pull/N/head` y de la base, a `refs/ariadna/…`) y **lee** el merge que git calcularía
//!    (`git merge-tree --write-tree`), devolviendo los ficheros en conflicto con sus tres
//!    versiones. No toca el árbol de trabajo ni el índice.
//! 2. [`AppCore::ai_pr_conflict_options`]: pide a la IA hasta tres alternativas de resolución de
//!    un fichero. Solo propone.
//! 3. [`AppCore::pr_apply_conflicts`]: con **confirmación nativa**, deja la PR preparada en local
//!    (checkout de la rama, merge de la base, ficheros escritos y preparados). El commit y el
//!    push los hace el usuario desde la interfaz normal.
//!
//! Nada se adivina: los conflictos salen del **código de salida** de `merge-tree` (0 limpio,
//! 1 conflictos, otro error) y las rutas sin resolver del índice (`--diff-filter=U`).

use std::collections::HashMap;
use std::sync::Mutex;

use ariadna_git::{GitError, GitPath, MergeFastForward, MergeResult};

use super::api::{PrConflictApplied, PrConflictFile, PrConflictResolution, PrConflicts};
use super::app::AppCore;
use super::error::{ApiError, ApiResult};

/// Tope de ficheros en conflicto que se leen de una vez (el resto se avisa, no se oculta).
const MAX_CONFLICT_FILES: usize = 40;
/// Tope de bytes de cada versión que se manda a la IA.
const MAX_SIDE_BYTES: usize = 60_000;

/// Caché de la última lectura por pestaña y PR. La clave lleva los SHA anunciados por la API, así
/// que cuando la PR cambia se invalida sola; sin ella, cada petición repetía un `fetch`, un
/// `merge-tree` y tres `git show` por fichero.
static CACHE: Mutex<Option<HashMap<String, PrConflicts>>> = Mutex::new(None);

fn cache_get(key: &str) -> Option<PrConflicts> {
    CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(key))
        .cloned()
}

fn cache_put(key: String, value: PrConflicts) {
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    // Tope simple: la caché es de trabajo, no histórica.
    if map.len() > 16 {
        map.clear();
    }
    map.insert(key, value);
}

/// Vacía la caché (la PR ha cambiado de estado en local).
pub fn clear_conflict_cache() {
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

impl AppCore {
    /// Ficheros en conflicto de una PR, con sus tres versiones. **Solo lectura** (salvo el `fetch`
    /// de las dos referencias, que se hace por la cola del repositorio).
    pub async fn pr_conflicts(&self, tab_id: &str, id: u64) -> ApiResult<PrConflicts> {
        let c = self.pr_ctx(tab_id).await?;
        let detail = c.provider.detail(id).await?;
        let summary = detail.summary.clone();
        let head_ref = pr_head_ref(
            &detail.checkout_ref,
            summary.id,
            &summary.source_branch,
            c.info.provider,
        );
        let base_ref = format!("refs/heads/{}", summary.target_branch);
        let remote = c.remote_name.clone();
        let tab_label = c.tab_label.clone();

        let cache_key = format!(
            "{tab_id}|{id}|{}|{}|{head_ref}|{}",
            detail.source_commit.as_deref().unwrap_or(""),
            detail.target_commit.as_deref().unwrap_or(""),
            summary.target_branch
        );
        if let Some(hit) = cache_get(&cache_key) {
            return Ok(hit);
        }

        // 1) Bajar **lo que hace falta**: la cabeza de la PR (en GitHub vive en
        // `refs/pull/N/head`, que el refspec por defecto no trae, y puede venir de un fork) y la
        // base. Se guardan en `refs/ariadna/…`, que es espacio nuestro y no ensucia el repo.
        let head_target = format!("refs/ariadna/pr/{id}/head");
        let base_target = format!("refs/ariadna/pr/{id}/base");
        let (head, base) = self
            .mutate_op(
                tab_id,
                "bajar las referencias de la pull request",
                (
                    remote.clone(),
                    head_ref.clone(),
                    base_ref,
                    head_target,
                    base_target,
                ),
                |e, r, (remote, head_ref, base_ref, head_target, base_target)| {
                    Box::pin(async move {
                        let head = e.fetch_to_ref(&r, &remote, &head_ref, &head_target).await?;
                        let base = e.fetch_to_ref(&r, &remote, &base_ref, &base_target).await?;
                        Ok((head, base))
                    })
                },
            )
            .await?;

        // 2) Merge de prueba. `merge-tree` sale con 1 cuando hay conflictos; eso **no** es un
        // error y el motor lo interpreta por su código de salida.
        let scan = self
            .read_op(tab_id, "calcular el merge de la pull request", {
                let (base, head) = (base.clone(), head.clone());
                move |e, r| Box::pin(async move { e.merge_tree(&r, &base, &head).await })
            })
            .await?;

        let mut paths = scan.conflicts.clone();
        let truncated = paths.len() > MAX_CONFLICT_FILES;
        paths.truncate(MAX_CONFLICT_FILES);

        // 3) Tres versiones de cada fichero (ancestro común + cada lado).
        let files = if paths.is_empty() {
            Vec::new()
        } else {
            let (base2, head2) = (base.clone(), head.clone());
            self.read_op(tab_id, "leer los ficheros en conflicto", move |e, r| {
                Box::pin(async move {
                    let merge_base = e.merge_base(&r, &base2, &head2).await.ok().flatten();
                    let mut out = Vec::with_capacity(paths.len());
                    for path in paths {
                        out.push(
                            read_sides(
                                e.as_ref(),
                                &r,
                                &base2,
                                &head2,
                                merge_base.as_deref(),
                                &path,
                            )
                            .await?,
                        );
                    }
                    Ok(out)
                })
            })
            .await?
        };

        let result = PrConflicts {
            number: summary.id,
            base: summary.target_branch.clone(),
            head: summary.source_branch.clone(),
            base_sha: base,
            head_sha: head,
            files,
            truncated,
            tab: tab_label,
        };
        cache_put(cache_key, result.clone());
        Ok(result)
    }

    /// Deja la PR preparada en local: checkout de la rama, merge de la base y ficheros resueltos
    /// escritos y preparados. **Pide confirmación nativa** y no hace commit ni push.
    ///
    /// Las rutas se validan contra los conflictos reales del merge (no por su nombre) y la rama
    /// local **no se fuerza** nunca: si tiene commits propios, se para y se avisa.
    pub async fn pr_apply_conflicts(
        &self,
        tab_id: &str,
        id: u64,
        resolutions: Vec<PrConflictResolution>,
    ) -> ApiResult<PrConflictApplied> {
        if resolutions.is_empty() {
            return Err(ApiError::new(
                "INVALID_REFERENCE",
                "No hay ninguna resolución que aplicar.",
                None,
            ));
        }
        let c = self.pr_ctx(tab_id).await?;
        let detail = c.provider.detail(id).await?;
        let summary = detail.summary.clone();
        let head_ref = pr_head_ref(
            &detail.checkout_ref,
            summary.id,
            &summary.source_branch,
            c.info.provider,
        );
        let branch = summary.source_branch.clone();
        // La base se toma de la referencia que acaba de bajar `pr_conflicts` (ya en local).
        let base_sha = format!("refs/ariadna/pr/{id}/base");
        let files: Vec<String> = resolutions.iter().map(|r| r.path.clone()).collect();
        for r in &resolutions {
            if !valid_relative_path(&r.path) || r.content.trim().is_empty() {
                return Err(ApiError::new(
                    "INVALID_REFERENCE",
                    "La resolución de un fichero no es válida.",
                    Some(r.path.clone()),
                ));
            }
        }

        // Cada ruta tiene que ser un conflicto real de **esta** PR, y ninguna puede venir de una
        // versión recortada: se escribiría un texto incompleto encima del fichero de verdad.
        let conflicts = self.pr_conflicts(tab_id, id).await?;
        let by_path: HashMap<&str, &PrConflictFile> = conflicts
            .files
            .iter()
            .map(|f| (f.path.as_str(), f))
            .collect();
        for path in &files {
            let Some(file) = by_path.get(path.as_str()) else {
                return Err(ApiError::new(
                    "INVALID_REFERENCE",
                    format!("«{path}» no está en conflicto en esta pull request."),
                    Some(path.clone()),
                ));
            };
            if file.truncated || file.binary {
                return Err(ApiError::new(
                    "INVALID_REFERENCE",
                    format!(
                        "«{path}» es demasiado grande o binario: no se resuelve con IA para no \
                         escribir una versión incompleta."
                    ),
                    Some(path.clone()),
                ));
            }
        }

        let ok = self
            .inner
            .prompter
            .confirm(
                "Preparar la resolución de los conflictos de la pull request",
                &format!(
                    "En {}: se bajará la rama «{branch}», se fusionará la base y se escribirán {} fichero(s) resueltos, ya preparados. No se hace commit ni se sube nada: eso lo decides tú después.",
                    c.where_(),
                    resolutions.len()
                ),
                &files.join("\n"),
            )
            .await;
        if !ok {
            return Ok(PrConflictApplied {
                done: false,
                branch,
                files: Vec::new(),
                merge_ready: false,
            });
        }

        let remote = c.remote_name.clone();
        let payload: Vec<(GitPath, Vec<u8>)> = resolutions
            .iter()
            .map(|r| {
                (
                    GitPath::new(r.path.clone().into_bytes()),
                    r.content.clone().into_bytes(),
                )
            })
            .collect();
        let files_out = files.clone();
        let (branch_out, merge_ready) = self
            .mutate_op(
                tab_id,
                "preparar los conflictos de la pull request",
                (remote, head_ref, branch, base_sha, payload),
                |e, r, (remote, head_ref, branch, base, payload)| {
                    Box::pin(async move {
                        // 1) Rama de la PR. **Sin forzar**: si la rama local tiene commits que no
                        // están en la PR, se para (`DivergedLocalBranch`) en vez de pisarlos.
                        e.checkout_remote_ref(&r, &remote, &head_ref, &branch, false)
                            .await?;
                        // 2) Merge de la base: aquí sí se esperan conflictos.
                        let merged = e.merge(&r, &base, MergeFastForward::Never).await?;
                        match merged {
                            MergeResult::Conflicts { .. } => {}
                            MergeResult::UpToDate
                            | MergeResult::FastForward { .. }
                            | MergeResult::Merged { .. } => {
                                return Err(GitError::InvalidReference(format!(
                                    "la rama «{branch}» se fusiona sin conflictos; no hay nada que resolver"
                                )));
                            }
                        }
                        // 3) Escribir las resoluciones elegidas y prepararlas.
                        let confirmation = ariadna_git::Confirmation::granted(
                            "resolver conflictos de la pull request",
                        );
                        let mut paths = Vec::new();
                        for (path, contents) in payload {
                            e.write_working_tree_file(&r, &path, &contents, &confirmation)
                                .await?;
                            paths.push(path);
                        }
                        e.stage(&r, &paths).await?;
                        // 4) «Listo para commitear» solo si de verdad no queda nada sin resolver.
                        let pending = e.unmerged_paths(&r).await?;
                        Ok((branch, pending.is_empty()))
                    })
                },
            )
            .await?;

        // La rama local ha cambiado de estado: la caché de esta PR ya no vale.
        clear_conflict_cache();
        Ok(PrConflictApplied {
            done: true,
            branch: branch_out,
            files: files_out,
            merge_ready,
        })
    }
}

/// Referencia de la cabeza de la PR: la que diga el proveedor o la convención de cada uno.
fn pr_head_ref(
    checkout_ref: &str,
    number: u64,
    source_branch: &str,
    provider: ariadna_hosting::Provider,
) -> String {
    if !checkout_ref.trim().is_empty() {
        return checkout_ref.to_owned();
    }
    match provider {
        ariadna_hosting::Provider::AzureDevOps => format!("refs/heads/{source_branch}"),
        _ => format!("refs/pull/{number}/head"),
    }
}

/// Ruta relativa aceptable: sin vacío, sin absoluta, sin `..` como **componente** (un fichero
/// llamado `a..b.txt` es válido) y sin NUL.
fn valid_relative_path(path: &str) -> bool {
    let p = path.trim();
    if p.is_empty() || p.contains('\u{0}') || p.starts_with('/') || p.starts_with('\\') {
        return false;
    }
    if p.len() >= 2 && p.as_bytes()[1] == b':' {
        return false; // C:\… o C:/…
    }
    !p.split(['/', '\\']).any(|c| c == "..")
}

/// Tres versiones de un fichero en conflicto, con aviso de binario y de recorte.
async fn read_sides(
    e: &dyn ariadna_git::GitServing,
    r: &ariadna_git::Repository,
    base: &str,
    head: &str,
    merge_base: Option<&str>,
    path: &str,
) -> Result<PrConflictFile, GitError> {
    let ours = e.file_at(r, base, path).await.unwrap_or_default();
    let theirs = e.file_at(r, head, path).await.unwrap_or_default();
    let ancestor = match merge_base {
        Some(mb) => e.file_at(r, mb, path).await.unwrap_or_default(),
        None => String::new(),
    };
    let (ours, cut_ours, binary_ours) = split_text(ours);
    let (theirs, cut_theirs, binary_theirs) = split_text(theirs);
    let (ancestor, cut_base, _) = split_text(ancestor);
    Ok(PrConflictFile {
        path: path.to_owned(),
        ours,
        theirs,
        base: ancestor,
        binary: binary_ours || binary_theirs,
        truncated: cut_ours || cut_theirs || cut_base,
    })
}

/// Texto recortado al tope, aviso de binario (contiene un byte nulo) y si se recortó.
fn split_text(raw: String) -> (String, bool, bool) {
    let binary = raw.contains('\u{0}');
    if raw.len() <= MAX_SIDE_BYTES {
        return (raw, false, binary);
    }
    let mut end = 0;
    for (i, ch) in raw.char_indices() {
        if i >= MAX_SIDE_BYTES {
            break;
        }
        end = i + ch.len_utf8();
    }
    (format!("{}\n…(recortado)\n", &raw[..end]), true, binary)
}
