//! Etiquetas: tipo, validación del nombre y parser de `git for-each-ref refs/tags`.
//! La E/S vive en `engine::tag_ops`.

use serde::{Deserialize, Serialize};

use crate::path_safety::{validate_ref, PathSafetyError};

/// Una etiqueta del repositorio (ligera o anotada).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    /// Nombre sin el prefijo `refs/tags/` (puede llevar `/`: `release/1.0`).
    pub name: String,
    /// `true` si es anotada (tiene objeto propio con mensaje y autor).
    pub annotated: bool,
    /// SHA completo del objeto al que apunta (el commit, ya «pelado» en las anotadas).
    pub commit: String,
    /// Mensaje de la etiqueta anotada (asunto + cuerpo, sin firma PGP). `None` si es ligera.
    pub message: Option<String>,
    /// Quien creó la etiqueta anotada. `None` si es ligera.
    pub tagger: Option<String>,
    /// Fecha ISO 8601: la de la etiqueta anotada o, en una ligera, la del commit.
    pub date_iso: String,
}

/// Longitud máxima razonable de un nombre (límite práctico de muchos sistemas de archivos).
pub const MAX_TAG_NAME: usize = 255;

/// Por qué un nombre de etiqueta no vale (reglas de `git check-ref-format` más las propias de
/// Ariadna: no `HEAD`, no demasiado largo). `None` si es válido. El texto es para la UI.
pub fn tag_name_problem(name: &str) -> Option<&'static str> {
    if name.chars().count() > MAX_TAG_NAME {
        return Some("es demasiado largo");
    }
    if name == "HEAD" {
        return Some("no puede llamarse HEAD");
    }
    match validate_ref(name) {
        Ok(()) => None,
        Err(PathSafetyError::InvalidRef { reason, .. }) => Some(reason),
        Err(_) => Some("no es válido"),
    }
}

/// Formato de `for-each-ref`: campos separados por `0x1f`, registros terminados en `0x1e`.
pub const TAG_LIST_FORMAT: &str = "--format=%(refname)%1f%(objecttype)%1f%(objectname)%1f%(*objectname)%1f%(creatordate:iso-strict)%1f%(taggername)%1f%(contents:subject)%1f%(contents:body)%1e";

/// Interpreta la salida de `for-each-ref refs/tags` con [`TAG_LIST_FORMAT`]. Los registros mal
/// formados se descartan (nunca se inventan etiquetas).
pub fn parse_tag_list(bytes: &[u8]) -> Vec<Tag> {
    String::from_utf8_lossy(bytes)
        .split('\x1e')
        .map(|r| r.trim_matches(['\n', '\r']))
        .filter(|r| !r.is_empty())
        .filter_map(|record| {
            let f: Vec<&str> = record.split('\x1f').collect();
            if f.len() != 8 {
                return None;
            }
            let name = f[0].strip_prefix("refs/tags/")?;
            if name.is_empty() {
                return None;
            }
            let annotated = f[1] == "tag";
            // En una anotada `*objectname` es el objeto apuntado; en una ligera, `objectname`.
            let commit = if annotated && !f[3].is_empty() {
                f[3]
            } else {
                f[2]
            };
            let message = annotated.then(|| {
                let (subject, body) = (f[6].trim(), f[7].trim());
                if body.is_empty() {
                    subject.to_owned()
                } else {
                    format!("{subject}\n\n{body}")
                }
            });
            let tagger = (annotated && !f[5].trim().is_empty()).then(|| f[5].trim().to_owned());
            Some(Tag {
                name: name.to_owned(),
                annotated,
                commit: commit.to_owned(),
                message,
                tagger,
                date_iso: f[4].to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn valid_and_invalid_names() {
        for ok in [
            "v1.0.0",
            "release/1.0",
            "1.2.3-rc.1",
            "ñandú",
            "a@b",
            "v1_2",
        ] {
            assert_eq!(tag_name_problem(ok), None, "{ok}");
        }
        for bad in [
            "",
            "-v1",
            "con espacio",
            "a..b",
            "a~1",
            "a^b",
            "a:b",
            "a?b",
            "a*b",
            "a[b",
            "a\\b",
            "/v1",
            "v1/",
            "v1.",
            "a//b",
            "a@{b",
            "@",
            ".oculto",
            "x/.y",
            "v1.lock",
            "HEAD",
            "tab\tchar",
        ] {
            assert!(tag_name_problem(bad).is_some(), "debía rechazarse: {bad:?}");
        }
        assert!(tag_name_problem(&"a".repeat(MAX_TAG_NAME + 1)).is_some());
        assert_eq!(tag_name_problem(&"a".repeat(MAX_TAG_NAME)), None);
    }

    #[test]
    fn parses_lightweight_and_annotated() {
        let raw = "refs/tags/v1\x1fcommit\x1faaaa\x1f\x1f2026-01-02T03:04:05+01:00\x1f\x1fsubj\x1fbody\x1e\n\
                   refs/tags/rel/2\x1ftag\x1ftttt\x1fcccc\x1f2026-02-03T00:00:00+00:00\x1fAna <a@x>\x1fAsunto\x1fCuerpo\nmás\x1e";
        let tags = parse_tag_list(raw.as_bytes());
        assert_eq!(tags.len(), 2);
        assert!(!tags[0].annotated);
        assert_eq!(tags[0].commit, "aaaa");
        assert_eq!(tags[0].message, None);
        assert_eq!(tags[0].tagger, None);
        assert!(tags[1].annotated);
        assert_eq!(tags[1].name, "rel/2");
        assert_eq!(tags[1].commit, "cccc");
        assert_eq!(tags[1].message.as_deref(), Some("Asunto\n\nCuerpo\nmás"));
        assert_eq!(tags[1].tagger.as_deref(), Some("Ana <a@x>"));
    }

    #[test]
    fn malformed_records_are_dropped() {
        assert!(parse_tag_list(b"basura\x1e").is_empty());
        assert!(parse_tag_list(b"refs/heads/x\x1f\x1f\x1f\x1f\x1f\x1f\x1f\x1e").is_empty());
        assert!(parse_tag_list(b"").is_empty());
    }
}
