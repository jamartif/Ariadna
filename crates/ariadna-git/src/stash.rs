//! Stash: tipos y parser de `git stash list`. La E/S vive en `engine::stash_ops`.
//!
//! Un stash se identifica por su **SHA** (`oid`), no por `stash@{n}`: el índice cambia en cuanto
//! se guarda o se borra otro, y una interfaz con la lista vieja borraría el equivocado.

use serde::{Deserialize, Serialize};

use crate::models::GitPath;

/// Entrada de la lista de stashes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    /// Posición actual (`stash@{index}`); solo informativa.
    pub index: u32,
    /// SHA completo del commit del stash: su identidad.
    pub oid: String,
    /// Mensaje del usuario, o el automático de git (`<sha corto> <asunto>`).
    pub message: String,
    /// Rama en la que se guardó; `None` en HEAD separado.
    pub branch: Option<String>,
    /// `true` si el mensaje lo generó git (`WIP on …`) y no el usuario.
    pub auto_message: bool,
    /// Fecha ISO 8601 del stash.
    pub created_at_iso: String,
}

/// Qué guardar al hacer `stash push`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StashPushOptions {
    /// Mensaje opcional (una línea; se recorta).
    pub message: Option<String>,
    /// Incluir archivos sin seguimiento (`-u`). Se ignora con `staged_only`.
    pub include_untracked: bool,
    /// Guardar solo lo preparado (`--staged`).
    pub staged_only: bool,
}

/// Resultado de `apply`/`pop`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashApplyResult {
    /// Archivos que han quedado en conflicto (el repo conserva el estado de conflicto).
    pub conflicts: Vec<GitPath>,
    /// `true` si el stash se ha borrado (solo `pop` sin conflictos).
    pub dropped: bool,
}

/// Separa `WIP on <rama>: <resto>` / `On <rama>: <mensaje>`.
fn split_subject(subject: &str) -> (Option<String>, String, bool) {
    let parsed = subject
        .strip_prefix("WIP on ")
        .map(|r| (r, true))
        .or_else(|| subject.strip_prefix("On ").map(|r| (r, false)));
    if let Some((rest, auto)) = parsed {
        if let Some((branch, message)) = rest.split_once(": ") {
            let branch = (branch != "(no branch)" && !branch.is_empty()).then(|| branch.to_owned());
            return (branch, message.to_owned(), auto);
        }
    }
    (None, subject.to_owned(), false)
}

/// Interpreta `git stash list --format=%H%x1f%gs%x1f%cI%x1e`.
pub fn parse_stash_list(bytes: &[u8]) -> Vec<StashEntry> {
    String::from_utf8_lossy(bytes)
        .split('\x1e')
        .map(|r| r.trim_matches(['\n', '\r']))
        .filter(|r| !r.is_empty())
        .enumerate()
        .filter_map(|(index, record)| {
            let mut f = record.split('\x1f');
            let (oid, subject, date) = (f.next()?, f.next()?, f.next()?);
            let (branch, message, auto_message) = split_subject(subject);
            Some(StashEntry {
                index: u32::try_from(index).ok()?,
                oid: oid.trim().to_owned(),
                message,
                branch,
                auto_message,
                created_at_iso: date.trim().to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn parses_user_auto_and_detached_subjects() {
        let raw = "aaa\x1fOn main: arreglo a medias\x1f2026-01-02T03:04:05+01:00\x1e\nbbb\x1fWIP on feature/x: 1234567 asunto\x1f2026-01-01T00:00:00+00:00\x1e\nccc\x1fWIP on (no branch): 89abcde otro\x1f2025-12-31T00:00:00Z\x1e\nddd\x1fautostash\x1f2025-12-30T00:00:00Z\x1e";
        let list = parse_stash_list(raw.as_bytes());
        assert_eq!(list.len(), 4);
        assert_eq!((list[0].index, list[0].oid.as_str()), (0, "aaa"));
        assert_eq!(list[0].branch.as_deref(), Some("main"));
        assert_eq!(list[0].message, "arreglo a medias");
        assert!(!list[0].auto_message);
        assert_eq!(list[1].branch.as_deref(), Some("feature/x"));
        assert!(list[1].auto_message);
        assert_eq!(list[1].message, "1234567 asunto");
        assert_eq!(list[2].branch, None);
        assert_eq!(list[3].message, "autostash");
        assert_eq!(list[3].index, 3);
    }

    #[test]
    fn empty_and_garbage_input_give_no_entries() {
        assert!(parse_stash_list(b"").is_empty());
        assert!(parse_stash_list(b"\n\x1e\x1e").is_empty());
        assert!(parse_stash_list(b"solo-un-campo").is_empty());
    }
}
