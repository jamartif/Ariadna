//! Reglas del cuadro de mensaje al activar o desactivar el amend (port de `AmendDraft.swift`).

use crate::models::Commit;

/// Mensaje completo del commit para editarlo: asunto y, si hay, cuerpo.
pub fn message_for(commit: &Commit) -> String {
    if commit.body.is_empty() {
        commit.subject.clone()
    } else {
        format!("{}\n\n{}", commit.subject, commit.body)
    }
}

/// Texto que queda en el cuadro al desactivar el amend.
///
/// Si sigue con el mensaje que Ariadna puso al activarlo, se borra (si no, el siguiente commit
/// normal saldría con el mensaje del anterior). Si el usuario lo editó, se respeta.
pub fn message_after_disabling(current: &str, prefilled: Option<&str>) -> String {
    match prefilled {
        Some(p) if p == current => String::new(),
        _ => current.to_owned(),
    }
}

/// Aviso si el commit a corregir es un merge.
pub fn merge_warning(commit: &Commit) -> Option<String> {
    (commit.parents.len() > 1).then(|| {
        format!(
            "El último commit es un merge ({} padres). Corregirlo lo rehace con los mismos padres: revisa el mensaje y lo preparado.",
            commit.parents.len()
        )
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn commit(parents: &[&str], subject: &str, body: &str) -> Commit {
        Commit {
            sha: "a".into(),
            parents: parents.iter().map(|p| (*p).to_owned()).collect(),
            subject: subject.into(),
            body: body.into(),
            author_name: "Ada".into(),
            author_email: "a@b".into(),
            author_date_iso: String::new(),
            refs: vec![],
        }
    }

    #[test]
    fn prefills_and_clears_untouched_message() {
        assert_eq!(message_for(&commit(&["p"], "Asunto", "")), "Asunto");
        assert_eq!(
            message_for(&commit(&["p"], "Asunto", "Cuerpo")),
            "Asunto\n\nCuerpo"
        );
        assert_eq!(message_after_disabling("Asunto", Some("Asunto")), "");
        assert_eq!(
            message_after_disabling("Asunto corregido", Some("Asunto")),
            "Asunto corregido"
        );
        assert_eq!(message_after_disabling("Mío", None), "Mío");
    }

    #[test]
    fn warns_on_merge_commit() {
        assert!(merge_warning(&commit(&["p"], "s", "")).is_none());
        assert!(merge_warning(&commit(&["p1", "p2"], "Merge", ""))
            .unwrap()
            .contains("2 padres"));
    }
}
