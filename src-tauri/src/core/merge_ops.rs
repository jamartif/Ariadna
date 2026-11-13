//! Merge (o cherry-pick/rebase) en curso y resolución de conflictos (`specs/rust/ipc.md`).
//!
//! El estado de la integración sigue siendo el de git (MERGE_HEAD, índice con etapas 1/2/3);
//! aquí solo se resume para la UI y se aplican resoluciones **tras confirmación nativa**:
//! escribir el archivo + `git add` (o borrar + `git add` si el lado elegido es «borrado»).

use ariadna_git::{
    parse_conflicts, Confirmation, ConflictChoice, ConflictFile, ConflictKind, GitPath,
    IntegrationState, MergeFastForward, MergeResult,
};
use serde::{Deserialize, Serialize};

use super::api::Done;
use super::app::AppCore;
use super::error::{ApiError, ApiResult};

/// Resumen de un archivo en conflicto (sin contenidos: se piden con `get_conflicts`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictSummary {
    pub path: GitPath,
    pub kind: ConflictKind,
    /// Algún lado es binario: solo se puede elegir una versión entera.
    pub binary: bool,
    pub hunk_count: usize,
    /// Los lados y el archivo de trabajo son UTF-8: se puede editar el texto sin alterar bytes.
    pub text_safe: bool,
    pub ours_present: bool,
    pub base_present: bool,
    pub theirs_present: bool,
}

/// Estado de la integración en curso.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeState {
    pub integration: IntegrationState,
    /// Commit que se integra (`MERGE_HEAD` / `CHERRY_PICK_HEAD`).
    pub other_head: Option<String>,
    /// Nombre legible de lo que se integra (`feature`, `origin/main`…), si se deduce.
    pub source_label: Option<String>,
    /// Mensaje por defecto del commit de merge (`MERGE_MSG` sin comentarios).
    pub message: String,
    pub conflicts: Vec<ConflictSummary>,
    /// No quedan rutas sin resolver en el índice.
    pub can_continue: bool,
}

/// Resultado de `merge_start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeStartResult {
    pub result: MergeResult,
    /// Presente solo si el merge quedó en curso con conflictos.
    pub state: Option<MergeState>,
}

/// Resultado de `merge_continue`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinueResult {
    pub done: bool,
    /// Commit creado (merge con mensaje propio).
    pub sha: Option<String>,
    /// Estado tras continuar: en rebase/cherry-pick el siguiente paso puede volver a chocar.
    pub integration: IntegrationState,
}

/// Lado elegido para un bloque.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HunkSide {
    Ours,
    Theirs,
    Base,
    /// Nuestro y luego suyo.
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HunkChoice {
    pub id: usize,
    pub choice: HunkSide,
}

/// Cómo se resuelve un archivo.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictResolution {
    /// Versión entera del índice (`ours` = :2, `theirs` = :3, `base` = :1). Si ese lado no
    /// existe (borrado), el archivo se borra.
    Side(ConflictChoice),
    /// Elección por bloque; deben cubrir **todos** los bloques del archivo.
    Hunks(Vec<HunkChoice>),
    /// Texto final escrito a mano (bytes). No puede conservar marcadores.
    Manual(Vec<u8>),
}

enum Plan {
    Write(Vec<u8>),
    Delete,
}

fn summarize(f: &ConflictFile) -> ConflictSummary {
    let utf8 = |c: &Option<Vec<u8>>| {
        c.as_deref()
            .map(|b| std::str::from_utf8(b).is_ok())
            .unwrap_or(true)
    };
    let binary = f.ours.is_binary
        || f.base.is_binary
        || f.theirs.is_binary
        || f.working_tree.as_deref().is_some_and(|b| b.contains(&0));
    ConflictSummary {
        path: f.path.clone(),
        kind: f.kind,
        binary,
        hunk_count: f.hunks.len(),
        text_safe: !binary
            && utf8(&f.ours.content)
            && utf8(&f.base.content)
            && utf8(&f.theirs.content)
            && utf8(&f.working_tree),
        ours_present: f.ours.is_present(),
        base_present: f.base.is_present(),
        theirs_present: f.theirs.is_present(),
    }
}

/// `Merge branch 'x' into y` → `x`.
fn source_label(message: &str) -> Option<String> {
    let line = message.lines().next()?;
    let start = line.find('\'')? + 1;
    let len = line[start..].find('\'')?;
    Some(line[start..start + len].to_owned())
}

/// `# Conflicts:` con el carácter de comentario que use el repositorio (`core.commentChar`
/// puede ser `;`, o `auto` y git elige uno que no aparezca en el mensaje).
fn is_conflicts_header(line: &str) -> Option<char> {
    let mut chars = line.chars();
    let c = chars.next()?;
    (!c.is_alphanumeric() && !c.is_whitespace() && chars.as_str() == " Conflicts:").then_some(c)
}

/// Mensaje de git sin el bloque final de conflictos (`# Conflicts:` y las rutas comentadas
/// que lo siguen). El resto se respeta tal cual: una línea como `#123 arregla login` es
/// parte del mensaje.
pub(crate) fn clean_message(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    let mut end = lines.len();
    if let Some((start, c)) = lines
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, l)| is_conflicts_header(l).map(|c| (i, c)))
    {
        // Solo si es el final: tras la cabecera únicamente hay líneas comentadas o en blanco.
        if lines[start + 1..]
            .iter()
            .all(|l| l.starts_with(c) || l.trim().is_empty())
        {
            end = start;
        }
    }
    format!("{}\n", lines[..end].join("\n").trim_end())
}

/// Comprueba que las elecciones cubren exactamente los bloques del archivo: ni repetidas, ni
/// de bloques que no existen, ni falta ninguno (y en ese caso dice cuál).
fn check_hunk_choices(region_count: usize, choices: &[HunkChoice]) -> ApiResult<()> {
    let mut seen = vec![false; region_count];
    for c in choices {
        match seen.get_mut(c.id) {
            None => {
                return Err(ApiError::new(
                    "HUNK_NOT_FOUND",
                    format!(
                        "El archivo no tiene un bloque {} (tiene {region_count}). Vuelve a cargar el conflicto.",
                        c.id + 1
                    ),
                    None,
                ))
            }
            Some(true) => {
                return Err(ApiError::new(
                    "HUNK_NOT_FOUND",
                    format!("El bloque {} está elegido dos veces.", c.id + 1),
                    None,
                ))
            }
            Some(slot) => *slot = true,
        }
    }
    let missing: Vec<String> = seen
        .iter()
        .enumerate()
        .filter(|(_, s)| !**s)
        .map(|(i, _)| (i + 1).to_string())
        .collect();
    match missing.as_slice() {
        [] => Ok(()),
        [one] => Err(markers_remain(&format!(
            "Falta elegir qué hacer con el bloque {one}."
        ))),
        many => Err(markers_remain(&format!(
            "Falta elegir qué hacer con los bloques {}.",
            many.join(", ")
        ))),
    }
}

/// Aplica la elección por bloque sobre el archivo con marcadores.
fn apply_hunks(working: &[u8], choices: &[HunkChoice]) -> ApiResult<Vec<u8>> {
    let parsed = parse_conflicts(working);
    if parsed.unterminated {
        return Err(markers_remain("El archivo tiene marcadores sin cerrar."));
    }
    check_hunk_choices(parsed.regions.len(), choices)?;
    let mut out = Vec::with_capacity(working.len());
    let mut cursor = 0;
    for region in &parsed.regions {
        out.extend_from_slice(&working[cursor..region.start]);
        let choice = choices.iter().find(|c| c.id == region.hunk.id);
        match choice.map(|c| c.choice) {
            // `check_hunk_choices` garantiza que todos los bloques tienen elección.
            None => out.extend_from_slice(&working[region.start..region.end]),
            Some(HunkSide::Ours) => out.extend_from_slice(&region.hunk.ours),
            Some(HunkSide::Theirs) => out.extend_from_slice(&region.hunk.theirs),
            Some(HunkSide::Both) => {
                out.extend_from_slice(&region.hunk.ours);
                out.extend_from_slice(&region.hunk.theirs);
            }
            Some(HunkSide::Base) => match region.hunk.base.as_deref() {
                Some(b) => out.extend_from_slice(b),
                None => {
                    return Err(ApiError::new(
                        "CONFLICT_NOT_FOUND",
                        format!(
                        "El bloque {} no tiene versión base (el archivo no está en estilo diff3).",
                        region.hunk.id + 1
                    ),
                        None,
                    ))
                }
            },
        }
        cursor = region.end;
    }
    out.extend_from_slice(&working[cursor..]);
    Ok(out)
}

fn markers_remain(message: &str) -> ApiError {
    ApiError::new("CONFLICT_MARKERS_REMAIN", message, None)
}

fn not_found(path: &GitPath) -> ApiError {
    ApiError::new(
        "CONFLICT_NOT_FOUND",
        "Ese archivo ya no está en conflicto.",
        Some(path.to_string_lossy().into_owned()),
    )
}

fn plan_for(file: &ConflictFile, resolution: &ConflictResolution) -> ApiResult<Plan> {
    let summary = summarize(file);
    match resolution {
        ConflictResolution::Side(choice) => {
            let side = match choice {
                ConflictChoice::Ours => &file.ours,
                ConflictChoice::Base => &file.base,
                ConflictChoice::Theirs => &file.theirs,
            };
            if side.is_present() {
                Ok(Plan::Write(file.taking_whole_side(*choice)))
            } else if *choice == ConflictChoice::Base {
                Err(ApiError::new(
                    "CONFLICT_NOT_FOUND",
                    "Este conflicto no tiene versión base.",
                    Some(file.path.to_string_lossy().into_owned()),
                ))
            } else {
                Ok(Plan::Delete)
            }
        }
        ConflictResolution::Hunks(choices) => {
            if summary.binary {
                return Err(not_text(&file.path));
            }
            let working = file
                .working_tree
                .as_deref()
                .ok_or_else(|| not_text(&file.path))?;
            let out = apply_hunks(working, choices)?;
            check_no_markers(&out)?;
            Ok(Plan::Write(out))
        }
        ConflictResolution::Manual(bytes) => {
            if summary.binary {
                return Err(not_text(&file.path));
            }
            check_no_markers(bytes)?;
            Ok(Plan::Write(bytes.clone()))
        }
    }
}

fn not_text(path: &GitPath) -> ApiError {
    ApiError::new(
        "CONFLICT_NOT_TEXT",
        "Este archivo es binario o no es texto UTF-8: solo se puede elegir una versión entera.",
        Some(path.to_string_lossy().into_owned()),
    )
}

fn check_no_markers(bytes: &[u8]) -> ApiResult<()> {
    if ariadna_git::has_conflict_markers(bytes) {
        return Err(markers_remain(
            "Quedan marcadores de conflicto (<<<<<<<, =======, >>>>>>>) sin resolver en el resultado.",
        ));
    }
    Ok(())
}

impl AppCore {
    /// Estado de la integración en curso (vacío si no hay ninguna): mensaje por defecto,
    /// commit que se integra y resumen de los archivos en conflicto.
    pub async fn merge_state(&self, tab_id: &str) -> ApiResult<MergeState> {
        self.read_op(tab_id, "estado del merge", |e, r| {
            Box::pin(async move {
                let integration = e.integration_state(&r).await?;
                let conflicts: Vec<ConflictSummary> =
                    e.conflicts(&r).await?.iter().map(summarize).collect();
                let read = |name: &str| std::fs::read_to_string(r.git_dir.join(name)).ok();
                let (head_file, with_message) = match integration {
                    IntegrationState::Merging => (Some("MERGE_HEAD"), true),
                    IntegrationState::CherryPicking => (Some("CHERRY_PICK_HEAD"), true),
                    _ => (None, false),
                };
                let other_head = head_file
                    .and_then(read)
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty());
                let raw_message = if with_message {
                    read("MERGE_MSG").unwrap_or_default()
                } else {
                    String::new()
                };
                let source_label = match integration {
                    IntegrationState::Merging => source_label(&raw_message),
                    IntegrationState::Rebasing => read("rebase-merge/head-name")
                        .or_else(|| read("rebase-apply/head-name"))
                        .map(|s| s.trim().trim_start_matches("refs/heads/").to_owned()),
                    _ => None,
                };
                Ok(MergeState {
                    integration,
                    other_head,
                    source_label,
                    message: if raw_message.is_empty() {
                        String::new()
                    } else {
                        clean_message(&raw_message)
                    },
                    can_continue: integration != IntegrationState::None && conflicts.is_empty(),
                    conflicts,
                })
            })
        })
        .await
    }

    /// Empieza un merge y, si hay conflictos, **lo deja en curso** para resolverlos (a
    /// diferencia de `merge_clean`, que lo aborta).
    pub async fn merge_start(
        &self,
        tab_id: &str,
        branch: String,
        fast_forward: MergeFastForward,
    ) -> ApiResult<MergeStartResult> {
        let result = self.merge(tab_id, branch, fast_forward).await?;
        let state = if matches!(result, MergeResult::Conflicts { .. }) {
            Some(self.merge_state(tab_id).await?)
        } else {
            None
        };
        Ok(MergeStartResult { result, state })
    }

    /// Resuelve un archivo en conflicto: escribe el resultado (o lo borra) y hace `git add`.
    /// Pide confirmación nativa antes de tocar el archivo. `done: false` si se cancela.
    pub async fn resolve_conflict(
        &self,
        tab_id: &str,
        path: GitPath,
        resolution: ConflictResolution,
    ) -> ApiResult<Done> {
        let (s, meta) = self.live(tab_id)?;
        let files = self.get_conflicts(tab_id).await?;
        let file = files
            .iter()
            .find(|f| f.path == path)
            .ok_or_else(|| not_found(&path))?;
        let plan = plan_for(file, &resolution)?;
        let deleting = matches!(plan, Plan::Delete);
        let what = match &resolution {
            ConflictResolution::Side(ConflictChoice::Ours) => "nuestra versión",
            ConflictResolution::Side(ConflictChoice::Theirs) => "su versión",
            ConflictResolution::Side(ConflictChoice::Base) => "la versión base",
            ConflictResolution::Hunks(_) => "la elección por bloques",
            ConflictResolution::Manual(_) => "el texto editado",
        };
        let ok = self
            .inner
            .prompter
            .confirm(
                "Resolver el conflicto",
                &format!(
                    "En {}: {} y se marcará como resuelto ({what}).",
                    Self::context(&s, &meta),
                    if deleting {
                        "se borrará el archivo"
                    } else {
                        "se sobrescribirá el archivo"
                    }
                ),
                &path.to_string_lossy(),
            )
            .await;
        if !ok {
            return Ok(Done { done: false });
        }
        let confirmation = Confirmation::granted("resolver el conflicto");
        let plan = std::sync::Arc::new(plan);
        s.mutate("resolver conflicto", move |e, r| {
            let (p, c, plan) = (
                path.clone(),
                confirmation.clone(),
                std::sync::Arc::clone(&plan),
            );
            Box::pin(async move {
                match &*plan {
                    Plan::Write(bytes) => e.write_working_tree_file(&r, &p, bytes, &c).await?,
                    Plan::Delete => e.remove_working_tree_file(&r, &p, &c).await?,
                }
                e.mark_resolved(&r, &p).await
            })
        })
        .await?;
        Ok(Done { done: true })
    }

    /// Termina la integración en curso. Con merge y un mensaje, el commit lo usa; sin él, el
    /// mensaje por defecto de git. Falla con `CONFLICTS_PENDING` si quedan rutas sin resolver.
    pub async fn merge_continue(
        &self,
        tab_id: &str,
        message: Option<String>,
    ) -> ApiResult<ContinueResult> {
        let pending = self.get_conflicts(tab_id).await?;
        if !pending.is_empty() {
            let shown: Vec<String> = pending
                .iter()
                .take(20)
                .map(|f| f.path.to_string_lossy().into_owned())
                .collect();
            return Err(ApiError::new(
                "CONFLICTS_PENDING",
                format!(
                    "Quedan {} archivo(s) en conflicto sin resolver.",
                    pending.len()
                ),
                Some(shown.join("\n")),
            ));
        }
        let message = message.filter(|m| !m.trim().is_empty());
        let merging = self
            .read_op(tab_id, "estado de la integración", |e, r| {
                Box::pin(async move { e.integration_state(&r).await })
            })
            .await?
            == IntegrationState::Merging;
        let sha = match (merging, message) {
            (true, Some(m)) => Some(
                self.mutate_op(tab_id, "commit del merge", m, |e, r, m| {
                    Box::pin(async move { e.commit(&r, &m, true).await })
                })
                .await?,
            ),
            _ => {
                self.continue_integration(tab_id).await?;
                None
            }
        };
        let integration = self
            .read_op(tab_id, "estado de la integración", |e, r| {
                Box::pin(async move { e.integration_state(&r).await })
            })
            .await?;
        Ok(ContinueResult {
            done: true,
            sha,
            integration,
        })
    }

    /// Aborta el **merge** en curso (confirmación nativa). Para rebase o cherry-pick, usa
    /// `abort_integration`.
    pub async fn merge_abort(&self, tab_id: &str) -> ApiResult<Done> {
        let state = self
            .read_op(tab_id, "estado de la integración", |e, r| {
                Box::pin(async move { e.integration_state(&r).await })
            })
            .await?;
        if !matches!(state, IntegrationState::Merging | IntegrationState::None) {
            return Err(ApiError::new(
                "NO_INTEGRATION",
                "No hay ningún merge en curso (hay un rebase o cherry-pick: usa «abortar integración»).",
                None,
            ));
        }
        self.abort_integration(tab_id).await
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn clean_message_quita_solo_el_bloque_final_de_conflictos() {
        let raw = "Merge branch 'feature'\n\n#123 arregla login\n# no es comentario de git\n\n# Conflicts:\n#\tc.txt\n#\tsrc/a.rs\n";
        assert_eq!(
            clean_message(raw),
            "Merge branch 'feature'\n\n#123 arregla login\n# no es comentario de git\n"
        );
    }

    #[test]
    fn clean_message_respeta_core_comment_char() {
        let raw = "Merge branch 'x'\n\n#42 sigue\n\n; Conflicts:\n;\tc.txt\n";
        assert_eq!(clean_message(raw), "Merge branch 'x'\n\n#42 sigue\n");
    }

    #[test]
    fn clean_message_sin_bloque_o_con_texto_detras_no_quita_nada() {
        assert_eq!(clean_message("Merge\n\n#1 a\n"), "Merge\n\n#1 a\n");
        let raw = "Merge\n\n# Conflicts:\n#\ta\nTexto del usuario\n";
        assert_eq!(clean_message(raw), raw);
    }

    fn choice(id: usize) -> HunkChoice {
        HunkChoice {
            id,
            choice: HunkSide::Ours,
        }
    }

    #[test]
    fn las_elecciones_deben_cubrir_exactamente_los_bloques() {
        assert!(check_hunk_choices(2, &[choice(1), choice(0)]).is_ok());
        assert!(check_hunk_choices(0, &[]).is_ok());

        let e = check_hunk_choices(2, &[choice(0), choice(0), choice(1)]).unwrap_err();
        assert_eq!(e.code, "HUNK_NOT_FOUND");
        assert!(e.message.contains("bloque 1"), "{}", e.message);

        let e = check_hunk_choices(2, &[choice(0), choice(1), choice(2)]).unwrap_err();
        assert_eq!(e.code, "HUNK_NOT_FOUND");
        assert!(e.message.contains("bloque 3"), "{}", e.message);

        let e = check_hunk_choices(3, &[choice(1)]).unwrap_err();
        assert_eq!(e.code, "CONFLICT_MARKERS_REMAIN");
        assert!(e.message.contains("bloques 1, 3"), "{}", e.message);
    }

    #[test]
    fn el_texto_manual_admite_subrayados_y_separadores_sueltos() {
        // Títulos de Markdown/RST: `=======` o `>>>>>>>` sueltos no son un conflicto.
        let rst = b"Titulo\n=======\n\ntexto\n>>>>>>> cita\n";
        assert!(check_no_markers(rst).is_ok());
        // La secuencia completa sí, y un `<<<<<<<` sin cerrar también.
        assert!(check_no_markers(b"<<<<<<< a\n1\n=======\n2\n>>>>>>> b\n").is_err());
        assert!(check_no_markers(b"a\n<<<<<<< HEAD\nb\n").is_err());
    }
}
