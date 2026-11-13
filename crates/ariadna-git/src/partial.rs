//! Parches **parciales**: preparar o quitar de preparados solo algunas líneas de un hunk.
//!
//! Regla (la misma que `git add -p` al editar un hunk, y que `git apply --recount` acepta):
//!
//! | línea sin seleccionar | preparar (`apply --cached`) | quitar (`apply --cached --reverse`) |
//! |---|---|---|
//! | `-` borrada | pasa a **contexto** (sigue en el índice) | se **omite** (sigue borrada en el índice) |
//! | `+` añadida | se **omite** | pasa a **contexto** (sigue en el índice) |
//!
//! Las líneas seleccionadas se copian tal cual, con sus bytes (incluido `\r`). Un marcador
//! `\ No newline at end of file` se conserva solo si la línea a la que acompaña se conserva.
//! La cabecera de archivo se reconstruye con rutas entrecomilladas al estilo de git cuando
//! hace falta (Unicode, espacios con comillas, control), y se evita copiar `rename`, `index`
//! o cambios de modo: un parche parcial solo toca contenido.

use crate::models::{DiffLineKind, FileDiff};

/// Por qué no se puede construir el parche de líneas.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LinePatchError {
    #[error("no se ha seleccionado ninguna línea que cambie")]
    NoLinesSelected,
    #[error("la línea {0} no existe en este hunk")]
    LineOutOfRange(usize),
    #[error("el hunk {0} no existe en este archivo")]
    HunkOutOfRange(usize),
    #[error("un archivo binario solo se prepara entero")]
    Binary,
    #[error("los bytes del hunk no coinciden con sus líneas")]
    Inconsistent,
    #[error(
        "no se pueden quitar líneas sueltas de un archivo borrado; usa el hunk o el archivo entero"
    )]
    DeletedFileReverse,
}

/// Divide en líneas conservando el `\n` final de cada una.
fn split_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            lines.push(&bytes[start..=i]);
            start = i + 1;
        }
    }
    if start < bytes.len() {
        lines.push(&bytes[start..]);
    }
    lines
}

/// `a/ruta` o `b/ruta`, entrecomillada como hace git si lleva bytes especiales.
fn quoted_with_prefix(prefix: &str, path: &[u8]) -> Vec<u8> {
    let needs_quote = path
        .iter()
        .any(|b| *b < 0x20 || *b == 0x7f || *b >= 0x80 || *b == b'"' || *b == b'\\');
    let mut full = prefix.as_bytes().to_vec();
    full.extend_from_slice(path);
    if !needs_quote {
        return full;
    }
    let mut out = vec![b'"'];
    for b in full {
        match b {
            b'"' => out.extend_from_slice(b"\\\""),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\t' => out.extend_from_slice(b"\\t"),
            b'\r' => out.extend_from_slice(b"\\r"),
            0x20..=0x7e => out.push(b),
            _ => out.extend_from_slice(format!("\\{b:03o}").as_bytes()),
        }
    }
    out.push(b'"');
    out
}

fn modification_header(path: &[u8]) -> Vec<u8> {
    let a = quoted_with_prefix("a/", path);
    let b = quoted_with_prefix("b/", path);
    let mut out = b"diff --git ".to_vec();
    out.extend_from_slice(&a);
    out.push(b' ');
    out.extend_from_slice(&b);
    out.extend_from_slice(b"\n--- ");
    out.extend_from_slice(&a);
    out.extend_from_slice(b"\n+++ ");
    out.extend_from_slice(&b);
    out.push(b'\n');
    out
}

impl FileDiff {
    /// Parche con solo las líneas `selected` (índices de `hunks[hunk_index].lines`) para
    /// `git apply --cached --recount` (`reverse = false`, preparar) o con `--reverse`
    /// (`reverse = true`, quitar de preparados). Los índices que apuntan a contexto o a
    /// marcadores se ignoran; si no queda ninguna línea que cambie, es un error.
    ///
    /// Si la selección cubre **todas** las líneas que cambian, devuelve el parche original
    /// del hunk (byte a byte).
    pub fn patch_for_lines(
        &self,
        hunk_index: usize,
        selected: &[usize],
        reverse: bool,
    ) -> Result<Vec<u8>, LinePatchError> {
        if self.is_binary {
            return Err(LinePatchError::Binary);
        }
        let hunk = self
            .hunks
            .get(hunk_index)
            .ok_or(LinePatchError::HunkOutOfRange(hunk_index))?;
        let raw = split_lines(&hunk.raw);
        let body = raw.get(1..).unwrap_or(&[]);
        if body.len() != hunk.lines.len() {
            return Err(LinePatchError::Inconsistent);
        }
        let mut picked = vec![false; hunk.lines.len()];
        for &i in selected {
            let line = hunk.lines.get(i).ok_or(LinePatchError::LineOutOfRange(i))?;
            if matches!(line.kind, DiffLineKind::Addition | DiffLineKind::Deletion) {
                picked[i] = true;
            }
        }
        let changing = hunk
            .lines
            .iter()
            .filter(|l| matches!(l.kind, DiffLineKind::Addition | DiffLineKind::Deletion))
            .count();
        let chosen = picked.iter().filter(|p| **p).count();
        if chosen == 0 {
            return Err(LinePatchError::NoLinesSelected);
        }
        if chosen == changing {
            return self
                .patch_for_hunks(&[hunk_index])
                .map_err(|_| LinePatchError::HunkOutOfRange(hunk_index));
        }
        if reverse && self.is_deleted_file {
            return Err(LinePatchError::DeletedFileReverse);
        }

        let mut text: Vec<u8> = Vec::new();
        let (mut old_count, mut new_count) = (0u32, 0u32);
        let mut previous_kept = false;
        let push = |text: &mut Vec<u8>, line: &[u8], prefix: Option<u8>| {
            if !text.is_empty() && text.last() != Some(&b'\n') {
                text.push(b'\n');
            }
            match prefix {
                Some(p) if !line.is_empty() => {
                    text.push(p);
                    text.extend_from_slice(&line[1..]);
                }
                _ => text.extend_from_slice(line),
            }
        };
        for (i, line) in body.iter().enumerate() {
            match hunk.lines[i].kind {
                DiffLineKind::Context => {
                    push(&mut text, line, None);
                    old_count += 1;
                    new_count += 1;
                    previous_kept = true;
                }
                DiffLineKind::Addition if picked[i] => {
                    push(&mut text, line, None);
                    new_count += 1;
                    previous_kept = true;
                }
                DiffLineKind::Addition if reverse => {
                    push(&mut text, line, Some(b' '));
                    old_count += 1;
                    new_count += 1;
                    previous_kept = true;
                }
                DiffLineKind::Deletion if picked[i] => {
                    push(&mut text, line, None);
                    old_count += 1;
                    previous_kept = true;
                }
                DiffLineKind::Deletion if !reverse => {
                    push(&mut text, line, Some(b' '));
                    old_count += 1;
                    new_count += 1;
                    previous_kept = true;
                }
                DiffLineKind::Meta => {
                    if previous_kept {
                        push(&mut text, line, None);
                    }
                }
                // Omitidas: `+` sin elegir al preparar, `-` sin elegir al quitar.
                DiffLineKind::Addition | DiffLineKind::Deletion => previous_kept = false,
            }
        }
        if !text.is_empty() && text.last() != Some(&b'\n') {
            text.push(b'\n');
        }

        // Un archivo nuevo se crea solo al preparar; al quitar líneas, el índice ya lo tiene
        // y el parche es una modificación. Un borrado parcial o un renombrado también lo es.
        let creates = self.is_new_file && !reverse;
        let converted = (self.is_new_file && reverse) || self.is_deleted_file;
        let mut out = if creates {
            let mut h = self.header.clone();
            if !h.is_empty() && h.last() != Some(&b'\n') {
                h.push(b'\n');
            }
            h
        } else {
            modification_header(self.path.as_bytes())
        };
        let (mut old_start, mut new_start) = (hunk.old_start, hunk.new_start);
        if converted {
            // `-0,0` / `+0,0` solo valen para archivos que no existen en ese lado.
            if old_count > 0 && old_start == 0 {
                old_start = 1;
            }
            if new_count > 0 && new_start == 0 {
                new_start = 1;
            }
        }
        out.extend_from_slice(
            format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@").as_bytes(),
        );
        if !hunk.heading.is_empty() {
            out.push(b' ');
            out.extend_from_slice(hunk.heading.as_bytes());
        }
        out.push(b'\n');
        out.extend_from_slice(&text);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::diff::parse_diff;

    fn file(diff: &[u8]) -> FileDiff {
        parse_diff(diff).remove(0)
    }

    const TWO_CHANGES: &[u8] = b"diff --git a/f.txt b/f.txt\nindex 1..2 100644\n--- a/f.txt\n+++ b/f.txt\n@@ -1,5 +1,5 @@ fn x()\n a\n-b\n+B\n c\n-d\n+D\n e\n";

    #[test]
    fn stage_one_line_turns_other_deletions_into_context_and_drops_other_additions() {
        let f = file(TWO_CHANGES);
        // Líneas: 0 ` a`, 1 `-b`, 2 `+B`, 3 ` c`, 4 `-d`, 5 `+D`, 6 ` e`. Elegimos `+B` y `-b`.
        let patch = f.patch_for_lines(0, &[1, 2], false).unwrap();
        assert_eq!(
            String::from_utf8(patch).unwrap(),
            "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1,5 +1,5 @@ fn x()\n a\n-b\n+B\n c\n d\n e\n"
        );
        // Solo `+D` al preparar: `-d` queda como contexto.
        let patch = f.patch_for_lines(0, &[5], false).unwrap();
        let text = String::from_utf8(patch).unwrap();
        assert!(
            text.contains("@@ -1,5 +1,6 @@ fn x()\n a\n b\n c\n d\n+D\n e\n"),
            "{text}"
        );
    }

    #[test]
    fn unstage_swaps_the_rules() {
        let f = file(TWO_CHANGES);
        // Quitar solo `-d`: `+D` y `-b`/`+B` no elegidas → `+` a contexto, `-` omitido.
        let text = String::from_utf8(f.patch_for_lines(0, &[4], true).unwrap()).unwrap();
        assert!(
            text.contains("@@ -1,6 +1,5 @@ fn x()\n a\n B\n c\n-d\n D\n e\n"),
            "{text}"
        );
        // Quitar solo `+B`.
        let text = String::from_utf8(f.patch_for_lines(0, &[2], true).unwrap()).unwrap();
        assert!(
            text.contains("@@ -1,4 +1,5 @@ fn x()\n a\n+B\n c\n D\n e\n"),
            "{text}"
        );
    }

    #[test]
    fn keeps_cr_and_drops_orphan_no_newline_marker() {
        let raw = b"diff --git a/c.txt b/c.txt\n--- a/c.txt\n+++ b/c.txt\n@@ -1,3 +1,3 @@\n a\r\n-b\r\n+B\r\n+X\r\n z\n\\ No newline at end of file\n";
        let f = file(raw);
        let text = f.patch_for_lines(0, &[1, 2], false).unwrap();
        // `+X` omitida; la marca sigue a ` z` (conservada).
        assert!(text.ends_with(b" a\r\n-b\r\n+B\r\n z\n\\ No newline at end of file\n"));
        // Una marca tras una línea omitida desaparece.
        let raw = b"diff --git a/c.txt b/c.txt\n--- a/c.txt\n+++ b/c.txt\n@@ -1,2 +1,3 @@\n a\n+y\n+z\n\\ No newline at end of file\n";
        let f = file(raw);
        let text = f.patch_for_lines(0, &[1], false).unwrap();
        assert!(
            text.ends_with(b"@@ -1,1 +1,2 @@\n a\n+y\n"),
            "{:?}",
            String::from_utf8_lossy(&text)
        );
    }

    #[test]
    fn selecting_everything_returns_the_original_hunk_patch() {
        let f = file(TWO_CHANGES);
        let all = f.patch_for_lines(0, &[1, 2, 4, 5], false).unwrap();
        assert_eq!(all, f.patch_for_hunks(&[0]).unwrap());
    }

    #[test]
    fn errors_for_empty_out_of_range_binary_and_deleted_reverse() {
        let f = file(TWO_CHANGES);
        assert_eq!(
            f.patch_for_lines(0, &[], false),
            Err(LinePatchError::NoLinesSelected)
        );
        // Contexto solo: nada que cambie.
        assert_eq!(
            f.patch_for_lines(0, &[0, 3], false),
            Err(LinePatchError::NoLinesSelected)
        );
        assert_eq!(
            f.patch_for_lines(0, &[99], false),
            Err(LinePatchError::LineOutOfRange(99))
        );
        assert_eq!(
            f.patch_for_lines(3, &[1], false),
            Err(LinePatchError::HunkOutOfRange(3))
        );
        let bin = file(b"diff --git a/x b/x\nBinary files a/x and b/x differ\n");
        assert_eq!(
            bin.patch_for_lines(0, &[0], false),
            Err(LinePatchError::Binary)
        );
        let del = file(b"diff --git a/d b/d\ndeleted file mode 100644\nindex 1..0\n--- a/d\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-a\n-b\n");
        assert_eq!(
            del.patch_for_lines(0, &[0], true),
            Err(LinePatchError::DeletedFileReverse)
        );
        let text = String::from_utf8(del.patch_for_lines(0, &[0], false).unwrap()).unwrap();
        assert!(
            text.contains("--- a/d\n+++ b/d\n@@ -1,2 +1,1 @@\n-a\n b\n"),
            "{text}"
        );
    }

    #[test]
    fn new_file_creates_when_staging_and_modifies_when_unstaging() {
        let f = file(b"diff --git a/n.txt b/n.txt\nnew file mode 100644\nindex 0000000..1\n--- /dev/null\n+++ b/n.txt\n@@ -0,0 +1,3 @@\n+uno\n+dos\n+tres\n");
        let create = String::from_utf8(f.patch_for_lines(0, &[1], false).unwrap()).unwrap();
        assert!(create.contains("new file mode 100644"));
        assert!(create.ends_with("@@ -0,0 +1,1 @@\n+dos\n"), "{create}");
        let modify = String::from_utf8(f.patch_for_lines(0, &[1], true).unwrap()).unwrap();
        assert!(!modify.contains("new file mode"));
        assert!(
            modify.ends_with("@@ -1,2 +1,3 @@\n uno\n+dos\n tres\n"),
            "{modify}"
        );
    }

    #[test]
    fn header_quotes_unicode_and_special_paths() {
        assert_eq!(quoted_with_prefix("a/", b"x y.txt"), b"a/x y.txt");
        assert_eq!(
            quoted_with_prefix("b/", "cañón.txt".as_bytes()),
            b"\"b/ca\\303\\261\\303\\263n.txt\"".to_vec()
        );
        assert_eq!(quoted_with_prefix("a/", b"q\"t"), b"\"a/q\\\"t\"".to_vec());
    }
}
