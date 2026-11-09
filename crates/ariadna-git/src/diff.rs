//! Parser de diff unificado (`git diff`, `git show`) sobre bytes y constructor de parches.
//!
//! Principios (spec §4.1):
//! * El diff se trata como **bytes**: ni se normalizan `\r` ni se lee el worktree.
//! * Cada [`FileDiff`] conserva sus bytes exactos (`header` + hunks `raw` + `trailer`), de
//!   modo que `parse → rebuild` reproduce la entrada original byte a byte.
//! * El fin de un hunk se decide por los **contadores** de su cabecera `@@`, no por el
//!   contenido: una línea borrada que empieza por `-- ` (`--- x` en el diff) o una añadida
//!   que empieza por `++ ` no se confunden con cabeceras.
//! * El motor debe invocar git con `--src-prefix=a/ --dst-prefix=b/` (y `--no-ext-diff
//!   --no-textconv -c core.quotepath=false`) para no depender de `diff.noprefix` ni
//!   `diff.mnemonicPrefix` del usuario.
//!
//! No se interpretan diffs combinados (`diff --cc`); se ignoran.

use crate::models::{DiffHunk, DiffLine, DiffLineKind, FileDiff, GitPath};

/// Errores al construir parches.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiffError {
    #[error("no se ha seleccionado ningún hunk")]
    NoHunkSelected,
    #[error("el hunk {0} no existe en este archivo")]
    HunkOutOfRange(usize),
    #[error("el hunk no pertenece a este archivo")]
    HunkNotInFile,
}

/// Par (ruta antigua, ruta nueva) de la línea `diff --git`.
type PathPair = (Option<Vec<u8>>, Option<Vec<u8>>);

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

/// Línea sin el `\n` final (el `\r` se conserva).
fn content(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\n").unwrap_or(line)
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from > haystack.len() || haystack.len() - from < needle.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Interpreta un diff completo. Lo que haya antes del primer `diff --git` se descarta.
pub fn parse_diff(bytes: &[u8]) -> Vec<FileDiff> {
    let lines = split_lines(bytes);
    let mut files = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].starts_with(b"diff --git ") {
            let start = index;
            index += 1;
            while index < lines.len() && !lines[index].starts_with(b"diff --git ") {
                index += 1;
            }
            if let Some(file) = parse_file(&lines[start..index], files.len()) {
                files.push(file);
            }
        } else {
            index += 1;
        }
    }
    files
}

/// Reconstruye los bytes de un conjunto de [`FileDiff`] (inverso de [`parse_diff`]
/// cuando la entrada empieza en `diff --git`).
pub fn rebuild_diff(files: &[FileDiff]) -> Vec<u8> {
    files.iter().flat_map(FileDiff::raw_bytes).collect()
}

fn concat(lines: &[&[u8]]) -> Vec<u8> {
    lines.iter().flat_map(|l| l.iter().copied()).collect()
}

fn parse_file(lines: &[&[u8]], file_index: usize) -> Option<FileDiff> {
    let git_line = content(lines.first()?);
    let rest = git_line.strip_prefix(b"diff --git ")?;
    let (header_old, header_new) = paths_from_git_line(rest);
    let mut old_path = header_old;
    let mut new_path = header_new;
    let mut is_new = false;
    let mut is_deleted = false;
    let mut binary = false;
    let mut hunk_start = None;

    for (offset, raw_line) in lines.iter().enumerate().skip(1) {
        let line = content(raw_line);
        if line.starts_with(b"@@ ") {
            hunk_start = Some(offset);
            break;
        }
        if let Some(rest) = line.strip_prefix(b"--- ") {
            match decode_path(rest, Some(b"a/")) {
                Some(p) => old_path = Some(p),
                None => is_new = true,
            }
        } else if let Some(rest) = line.strip_prefix(b"+++ ") {
            match decode_path(rest, Some(b"b/")) {
                Some(p) => new_path = Some(p),
                None => is_deleted = true,
            }
        } else if line.starts_with(b"new file mode") {
            is_new = true;
        } else if line.starts_with(b"deleted file mode") {
            is_deleted = true;
        } else if let Some(rest) = line.strip_prefix(b"rename from ") {
            if let Some(p) = decode_path(rest, None) {
                old_path = Some(p);
            }
        } else if let Some(rest) = line.strip_prefix(b"rename to ") {
            if let Some(p) = decode_path(rest, None) {
                new_path = Some(p);
            }
        } else if let Some(rest) = line.strip_prefix(b"copy from ") {
            if let Some(p) = decode_path(rest, None) {
                old_path = Some(p);
            }
        } else if let Some(rest) = line.strip_prefix(b"copy to ") {
            if let Some(p) = decode_path(rest, None) {
                new_path = Some(p);
            }
        } else if line.starts_with(b"Binary files ") || line.starts_with(b"GIT binary patch") {
            binary = true;
            break;
        }
    }

    let path = new_path
        .or_else(|| old_path.clone())
        .unwrap_or_else(|| b"unknown".to_vec());
    let old = old_path.filter(|o| *o != path).map(GitPath::from);
    let path_lossy = String::from_utf8_lossy(&path).into_owned();

    let hunk_start = match (hunk_start, binary) {
        (Some(start), false) => start,
        _ => {
            return Some(FileDiff {
                path: GitPath::from(path),
                old_path: old,
                is_binary: binary,
                is_new_file: is_new,
                is_deleted_file: is_deleted,
                header: concat(lines),
                hunks: Vec::new(),
                trailer: Vec::new(),
            });
        }
    };

    let mut hunks = Vec::new();
    let mut cursor = hunk_start;
    while cursor < lines.len() && lines[cursor].starts_with(b"@@ ") {
        let Some(range) = parse_hunk_header(content(lines[cursor])) else {
            break;
        };
        let end = hunk_end(lines, cursor + 1, &range);
        let hunk_index = hunks.len();
        hunks.push(DiffHunk {
            id: format!(
                "{path_lossy}#{hunk_index}#{}#{}",
                range.old_start, range.new_start
            ),
            old_start: range.old_start,
            old_count: range.old_count,
            new_start: range.new_start,
            new_count: range.new_count,
            heading: range.heading.clone(),
            lines: classify(&lines[cursor + 1..end], file_index, hunk_index, &range),
            raw: concat(&lines[cursor..end]),
        });
        cursor = end;
    }

    Some(FileDiff {
        path: GitPath::from(path),
        old_path: old,
        is_binary: false,
        is_new_file: is_new,
        is_deleted_file: is_deleted,
        header: concat(&lines[..hunk_start]),
        hunks,
        trailer: concat(&lines[cursor..]),
    })
}

/// Índice (exclusivo) donde acaba el hunk que empieza en `from` (primera línea de cuerpo).
fn hunk_end(lines: &[&[u8]], from: usize, range: &HunkRange) -> usize {
    let mut old_left = range.old_count;
    let mut new_left = range.new_count;
    let mut next = from;
    while next < lines.len() {
        let line = lines[next];
        let done = old_left == 0 && new_left == 0;
        match line.first() {
            Some(b'\\') => {}
            _ if done => break,
            Some(b' ') | Some(b'\n') | None => {
                old_left = old_left.saturating_sub(1);
                new_left = new_left.saturating_sub(1);
            }
            Some(b'-') => old_left = old_left.saturating_sub(1),
            Some(b'+') => new_left = new_left.saturating_sub(1),
            Some(_) => break,
        }
        next += 1;
    }
    next
}

fn classify(
    body: &[&[u8]],
    file_index: usize,
    hunk_index: usize,
    range: &HunkRange,
) -> Vec<DiffLine> {
    let mut old_line = range.old_start;
    let mut new_line = range.new_start;
    let mut out = Vec::with_capacity(body.len());
    for (line_index, raw) in body.iter().enumerate() {
        let id = format!("{file_index}:{hunk_index}:{line_index}");
        let line = content(raw);
        let lossy = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
        match line.first() {
            Some(b'\\') => out.push(DiffLine {
                id,
                kind: DiffLineKind::Meta,
                text: lossy(line),
                old_line_number: None,
                new_line_number: None,
            }),
            Some(b'+') => {
                out.push(DiffLine {
                    id,
                    kind: DiffLineKind::Addition,
                    text: lossy(&line[1..]),
                    old_line_number: None,
                    new_line_number: Some(new_line),
                });
                new_line = new_line.saturating_add(1);
            }
            Some(b'-') => {
                out.push(DiffLine {
                    id,
                    kind: DiffLineKind::Deletion,
                    text: lossy(&line[1..]),
                    old_line_number: Some(old_line),
                    new_line_number: None,
                });
                old_line = old_line.saturating_add(1);
            }
            other => {
                let text = if other.is_some() {
                    lossy(&line[1..])
                } else {
                    String::new()
                };
                out.push(DiffLine {
                    id,
                    kind: DiffLineKind::Context,
                    text,
                    old_line_number: Some(old_line),
                    new_line_number: Some(new_line),
                });
                old_line = old_line.saturating_add(1);
                new_line = new_line.saturating_add(1);
            }
        }
    }
    out
}

#[derive(Debug, Clone)]
struct HunkRange {
    old_start: u32,
    old_count: u32,
    new_start: u32,
    new_count: u32,
    heading: String,
}

/// `@@ -l[,s] +l[,s] @@ encabezado`
fn parse_hunk_header(line: &[u8]) -> Option<HunkRange> {
    let payload = line.strip_prefix(b"@@ ")?;
    let closing = find(payload, b" @@", 0)?;
    let ranges = &payload[..closing];
    let heading = String::from_utf8_lossy(&payload[closing + 3..])
        .trim()
        .to_owned();
    let mut parts = ranges.split(|b| *b == b' ');
    let (old, new, none) = (parts.next()?, parts.next()?, parts.next());
    if none.is_some() {
        return None;
    }
    let (old_start, old_count) = parse_range(old, b'-')?;
    let (new_start, new_count) = parse_range(new, b'+')?;
    Some(HunkRange {
        old_start,
        old_count,
        new_start,
        new_count,
        heading,
    })
}

fn parse_range(token: &[u8], sign: u8) -> Option<(u32, u32)> {
    let rest = token.strip_prefix(&[sign])?;
    let text = std::str::from_utf8(rest).ok()?;
    match text.split_once(',') {
        None => Some((text.parse().ok()?, 1)),
        Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
    }
}

// ---------------------------------------------------------------------------
// Rutas
// ---------------------------------------------------------------------------

/// Decodifica la ruta de una línea `---`, `+++`, `rename from`… `None` = `/dev/null`
/// o vacía. `prefix` es el prefijo `a/`/`b/` que git antepone (no lo llevan `rename`/`copy`).
fn decode_path(raw: &[u8], prefix: Option<&[u8]>) -> Option<Vec<u8>> {
    let mut text = raw;
    while text.last() == Some(&b'\t') {
        text = &text[..text.len() - 1];
    }
    let quoted = text.len() >= 2 && text.starts_with(b"\"") && text.ends_with(b"\"");
    let mut path = if quoted {
        unquote_c_style(&text[1..text.len() - 1])
    } else {
        text.to_vec()
    };
    if path == b"/dev/null" {
        return None;
    }
    if let Some(prefix) = prefix {
        if path.starts_with(prefix) {
            path.drain(..prefix.len());
        }
    }
    (!path.is_empty()).then_some(path)
}

/// Rutas de la línea `diff --git a/<viejo> b/<nuevo>` (sin el prefijo `diff --git `).
///
/// Es solo un respaldo para diffs sin `---`/`+++` ni `rename` (binarios, cambios de modo):
/// con espacios en la ruta puede ser ambiguo, y entonces devuelve `(None, None)`.
fn paths_from_git_line(rest: &[u8]) -> PathPair {
    let strip = |mut p: Vec<u8>, prefix: &[u8]| -> Option<Vec<u8>> {
        if p.starts_with(prefix) {
            p.drain(..prefix.len());
        }
        (!p.is_empty()).then_some(p)
    };
    if rest.first() == Some(&b'"') {
        if let Some((first, end)) = scan_quoted(rest, 0) {
            let tail = rest.get(end..).unwrap_or(&[]);
            let tail = tail.strip_prefix(b" ").unwrap_or(tail);
            let second = if tail.first() == Some(&b'"') {
                scan_quoted(tail, 0).map(|(inner, _)| unquote_c_style(inner))
            } else {
                Some(tail.to_vec())
            };
            if let Some(second) = second {
                return (strip(unquote_c_style(first), b"a/"), strip(second, b"b/"));
            }
        }
    } else if rest.last() == Some(&b'"') {
        if let Some(pos) = find(rest, b" \"b/", 0) {
            let right = &rest[pos + 1..];
            if let Some((inner, _)) = scan_quoted(right, 0) {
                if rest[..pos].starts_with(b"a/") {
                    return (
                        strip(rest[..pos].to_vec(), b"a/"),
                        strip(unquote_c_style(inner), b"b/"),
                    );
                }
            }
        }
    }
    // Cada ` b/` es un corte posible. Si las dos mitades coinciden, el corte es el bueno.
    let mut candidates: Vec<PathPair> = Vec::new();
    let mut search = 0;
    while let Some(pos) = find(rest, b" b/", search) {
        search = pos + 1;
        let left = &rest[..pos];
        if !left.starts_with(b"a/") {
            continue;
        }
        let right = rest[pos + 1..].to_vec();
        candidates.push((strip(left.to_vec(), b"a/"), strip(right, b"b/")));
    }
    if let Some(same) = candidates.iter().find(|(o, n)| o == n) {
        return same.clone();
    }
    if candidates.len() == 1 {
        return candidates.remove(0);
    }
    (None, None)
}

/// Dado `input[start] == '"'`, devuelve el interior (con sus escapes) y la posición
/// siguiente a la comilla de cierre. Las comillas escapadas no cierran.
fn scan_quoted(input: &[u8], start: usize) -> Option<(&[u8], usize)> {
    if input.get(start) != Some(&b'"') {
        return None;
    }
    let mut i = start + 1;
    while i < input.len() {
        match input[i] {
            b'\\' => i += 2,
            b'"' => return Some((&input[start + 1..i], i + 1)),
            _ => i += 1,
        }
    }
    None
}

/// Decodifica los escapes de estilo C que git usa al citar rutas (`\t`, `\n`, `\\`, `\"`,
/// octales `\303`). Acepta el texto con o sin comillas exteriores.
pub fn unquote_c_style(raw: &[u8]) -> Vec<u8> {
    let inner = if raw.len() >= 2 && raw.starts_with(b"\"") && raw.ends_with(b"\"") {
        &raw[1..raw.len() - 1]
    } else {
        raw
    };
    let mut out = Vec::with_capacity(inner.len());
    let mut i = 0;
    while i < inner.len() {
        let b = inner[i];
        if b != b'\\' || i + 1 >= inner.len() {
            out.push(b);
            i += 1;
            continue;
        }
        let next = inner[i + 1];
        let simple = match next {
            b'n' => Some(0x0A),
            b't' => Some(0x09),
            b'r' => Some(0x0D),
            b'\\' => Some(b'\\'),
            b'"' => Some(b'"'),
            b'a' => Some(0x07),
            b'b' => Some(0x08),
            b'f' => Some(0x0C),
            b'v' => Some(0x0B),
            _ => None,
        };
        if let Some(byte) = simple {
            out.push(byte);
            i += 2;
        } else if (b'0'..=b'7').contains(&next) {
            let mut value = 0u32;
            let mut j = i + 1;
            while j < inner.len() && j < i + 4 && (b'0'..=b'7').contains(&inner[j]) {
                value = value * 8 + u32::from(inner[j] - b'0');
                j += 1;
            }
            out.push((value & 0xFF) as u8);
            i = j;
        } else {
            out.push(next);
            i += 2;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Parches
// ---------------------------------------------------------------------------

impl FileDiff {
    /// Bytes exactos del archivo en el diff original: cabecera + hunks + resto.
    pub fn raw_bytes(&self) -> Vec<u8> {
        let mut out = self.header.clone();
        for hunk in &self.hunks {
            out.extend_from_slice(&hunk.raw);
        }
        out.extend_from_slice(&self.trailer);
        out
    }

    /// Posición de un hunk dentro de este archivo.
    pub fn hunk_index(&self, hunk: &DiffHunk) -> Option<usize> {
        self.hunks
            .iter()
            .position(|h| h.id == hunk.id && h.raw == hunk.raw)
    }

    /// Parche de **un** hunk para `git apply --cached` (o con `-R` para deshacer).
    pub fn patch_for_hunk(&self, hunk: &DiffHunk) -> Result<Vec<u8>, DiffError> {
        let index = self.hunk_index(hunk).ok_or(DiffError::HunkNotInFile)?;
        self.patch_for_hunks(&[index])
    }

    /// Parche con los hunks indicados (en el orden del archivo, sin duplicados).
    ///
    /// La cabecera y los bytes de cada hunk se copian **tal cual** (incluidos `\r`): solo se
    /// añade un `\n` final si la entrada original no lo tenía. Las líneas `@@` no se
    /// renumeran; `git apply` tolera el desfase al aplicar un subconjunto de hunks.
    pub fn patch_for_hunks(&self, indices: &[usize]) -> Result<Vec<u8>, DiffError> {
        if indices.is_empty() {
            return Err(DiffError::NoHunkSelected);
        }
        let mut sorted = indices.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        if let Some(bad) = sorted.iter().find(|i| **i >= self.hunks.len()) {
            return Err(DiffError::HunkOutOfRange(*bad));
        }
        let mut out = self.header.clone();
        ensure_newline(&mut out);
        for i in sorted {
            out.extend_from_slice(&self.hunks[i].raw);
            ensure_newline(&mut out);
        }
        Ok(out)
    }

    /// Parche del archivo entero (único camino válido para binarios y cambios de modo).
    pub fn patch_for_whole_file(&self) -> Vec<u8> {
        let mut out = self.raw_bytes();
        ensure_newline(&mut out);
        out
    }
}

fn ensure_newline(buf: &mut Vec<u8>) {
    if !buf.is_empty() && buf.last() != Some(&b'\n') {
        buf.push(b'\n');
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::testutil::TestRepo;

    const DIFF_ARGS: [&str; 6] = [
        "diff",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--src-prefix=a/",
        "--dst-prefix=b/",
    ];

    #[test]
    fn crlf_hunks_stay_separate_and_keep_cr() {
        let raw = b"diff --git a/crlf.txt b/crlf.txt\n--- a/crlf.txt\n+++ b/crlf.txt\n@@ -1,3 +1,3 @@\n line1\r\n-line2\r\n+CHANGED\r\n@@ -8,3 +8,3 @@\n line8\r\n-line9\r\n+LATE\r\n";
        let files = parse_diff(raw);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].hunks.len(), 2);
        let added = files[0].hunks[0]
            .lines
            .iter()
            .find(|l| l.kind == DiffLineKind::Addition)
            .unwrap();
        assert_eq!(added.text, "CHANGED\r");
        assert_eq!(rebuild_diff(&files), raw);
    }

    #[test]
    fn binary_path_with_spaces_does_not_swallow_other_side() {
        let raw = "diff --git a/notas/cañón archivo.txt b/notas/cañón archivo.txt\nBinary files a/notas/cañón archivo.txt and b/notas/cañón archivo.txt differ\n";
        let file = &parse_diff(raw.as_bytes())[0];
        assert_eq!(file.path, "notas/cañón archivo.txt");
        assert!(file.old_path.is_none());
        assert!(file.is_binary);

        let rename = parse_diff(b"diff --git a/viejo nombre.txt b/nuevo nombre.txt\n");
        assert_eq!(
            rename[0].old_path.as_ref().unwrap(),
            &GitPath::from("viejo nombre.txt")
        );
        assert_eq!(rename[0].path, "nuevo nombre.txt");
    }

    #[test]
    fn path_containing_b_slash_without_plus_headers() {
        let raw = b"diff --git a/docs/a b/readme.txt b/docs/a b/readme.txt\nold mode 100644\nnew mode 100755\n";
        let file = &parse_diff(raw)[0];
        assert_eq!(file.path, "docs/a b/readme.txt");
        assert!(file.old_path.is_none());

        let rename = b"diff --git a/docs/a b/old.txt b/docs/a b/new.txt\nsimilarity index 90%\nrename from docs/a b/old.txt\nrename to docs/a b/new.txt\n";
        let file = &parse_diff(rename)[0];
        assert_eq!(
            file.old_path.as_ref().unwrap(),
            &GitPath::from("docs/a b/old.txt")
        );
        assert_eq!(file.path, "docs/a b/new.txt");
    }

    #[test]
    fn rename_paths_are_not_prefix_stripped() {
        // Regresión del Swift: `rename from a/b.txt` no lleva prefijo `a/`.
        let raw = b"diff --git a/a/b.txt b/c/d.txt\nsimilarity index 100%\nrename from a/b.txt\nrename to c/d.txt\n";
        let file = &parse_diff(raw)[0];
        assert_eq!(file.old_path.as_ref().unwrap(), &GitPath::from("a/b.txt"));
        assert_eq!(file.path, "c/d.txt");
    }

    #[test]
    fn c_style_and_octal_escapes() {
        assert_eq!(unquote_c_style(br#""caf\303\251""#), "café".as_bytes());
        assert_eq!(unquote_c_style(br#""a\t b\\c""#), b"a\t b\\c");
        assert_eq!(unquote_c_style(br#"x\"y"#), b"x\"y");
        assert_eq!(unquote_c_style(b"\\"), b"\\");

        let header = b"diff --git \"a/caf\\303\\251.txt\" \"b/caf\\303\\251.txt\"\n";
        let file = &parse_diff(header)[0];
        assert_eq!(file.path, "café.txt");
        assert!(file.old_path.is_none());

        let rename = b"diff --git \"a/viejo\\tnombre.txt\" \"b/nuevo\\tnombre.txt\"\nrename from \"viejo\\tnombre.txt\"\nrename to \"nuevo\\tnombre.txt\"\n";
        let file = &parse_diff(rename)[0];
        assert_eq!(
            file.old_path.as_ref().unwrap(),
            &GitPath::from("viejo\tnombre.txt")
        );
        assert_eq!(file.path, "nuevo\tnombre.txt");
    }

    #[test]
    fn non_utf8_path_escapes_round_trip_as_bytes() {
        let raw =
            b"diff --git \"a/x\\377.txt\" \"b/x\\377.txt\"\nBinary files a/x and b/x differ\n";
        let file = &parse_diff(raw)[0];
        assert_eq!(file.path.as_bytes(), b"x\xff.txt");
    }

    #[test]
    fn trims_tab_after_spaced_path() {
        let raw = "diff --git a/notas/cañón archivo.txt b/notas/cañón archivo.txt\n--- a/notas/cañón archivo.txt\t\n+++ b/notas/cañón archivo.txt\t\n@@ -1,2 +1,2 @@\n line1\n-line2\n+LINE2\n";
        let files = parse_diff(raw.as_bytes());
        assert_eq!(files[0].path, "notas/cañón archivo.txt");
        assert_eq!(rebuild_diff(&files), raw.as_bytes());
    }

    #[test]
    fn two_hunks_and_single_hunk_patch() {
        let diff = b"diff --git a/file.txt b/file.txt\nindex 111..222 100644\n--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n line1\n-line2\n+LINE2\n line3\n@@ -20,3 +20,3 @@ fn x()\n line20\n-line21\n+LINE21\n line22\n";
        let files = parse_diff(diff);
        let file = &files[0];
        assert_eq!(file.path, "file.txt");
        assert_eq!(file.hunks.len(), 2);
        assert_eq!((file.hunks[0].old_start, file.hunks[0].new_start), (1, 1));
        assert_eq!(file.hunks[1].heading, "fn x()");
        let adds: Vec<_> = file.hunks[0]
            .lines
            .iter()
            .filter(|l| l.kind == DiffLineKind::Addition)
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(adds, ["LINE2"]);
        assert!(file.hunks[0].raw.starts_with(b"@@ -1,3 +1,3 @@"));

        let patch = file.patch_for_hunk(&file.hunks[0]).unwrap();
        let text = String::from_utf8(patch).unwrap();
        assert!(text.starts_with("diff --git a/file.txt b/file.txt\nindex 111..222 100644\n"));
        assert!(text.contains("+LINE2") && !text.contains("+LINE21"));
        let both = file.patch_for_hunks(&[1, 0, 1]).unwrap();
        assert_eq!(both, diff);
    }

    #[test]
    fn dashed_content_lines_do_not_split_the_hunk() {
        // Se borró una línea `-- x` (aparece como `--- x`) y se añadió `++ y` (`+++ y`).
        let diff = b"diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n--- x\n+++ y\n ctx\n";
        let file = &parse_diff(diff)[0];
        assert_eq!(file.hunks.len(), 1);
        assert_eq!(file.hunks[0].lines.len(), 3);
        assert_eq!(file.hunks[0].lines[0].kind, DiffLineKind::Deletion);
        assert_eq!(file.hunks[0].lines[0].text, "-- x");
        assert_eq!(rebuild_diff(std::slice::from_ref(file)), diff);
    }

    #[test]
    fn no_newline_marker_new_deleted_and_mode_files() {
        let diff = b"diff --git a/n.txt b/n.txt\nnew file mode 100644\nindex 0000000..e69de29\n--- /dev/null\n+++ b/n.txt\n@@ -0,0 +1 @@\n+hola\n\\ No newline at end of file\ndiff --git a/d.txt b/d.txt\ndeleted file mode 100644\nindex e69de29..0000000\n--- a/d.txt\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-a\n-b\ndiff --git a/m.sh b/m.sh\nold mode 100644\nnew mode 100755\n";
        let files = parse_diff(diff);
        assert_eq!(files.len(), 3);
        assert!(files[0].is_new_file && !files[0].is_deleted_file);
        assert_eq!(files[0].hunks[0].lines.len(), 2);
        assert_eq!(files[0].hunks[0].lines[1].kind, DiffLineKind::Meta);
        assert_eq!(
            (files[0].hunks[0].new_start, files[0].hunks[0].new_count),
            (1, 1)
        );
        assert!(files[1].is_deleted_file);
        assert_eq!(files[1].path, "d.txt");
        assert!(files[2].hunks.is_empty() && !files[2].is_binary);
        assert_eq!(rebuild_diff(&files), diff);
        let mode_block = &diff[find(diff, b"diff --git a/m.sh", 0).unwrap()..];
        assert_eq!(files[2].patch_for_whole_file(), mode_block);
    }

    #[test]
    fn trailing_junk_goes_to_trailer_and_round_trips() {
        let diff = b"diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n-- \n2.39.0\n\n";
        let files = parse_diff(diff);
        assert_eq!(files[0].hunks[0].lines.len(), 2);
        assert_eq!(files[0].trailer, b"-- \n2.39.0\n\n");
        assert_eq!(rebuild_diff(&files), diff);
    }

    #[test]
    fn patch_errors() {
        let diff = b"diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n";
        let file = &parse_diff(diff)[0];
        assert_eq!(file.patch_for_hunks(&[]), Err(DiffError::NoHunkSelected));
        assert_eq!(
            file.patch_for_hunks(&[3]),
            Err(DiffError::HunkOutOfRange(3))
        );
        let other = &parse_diff(b"diff --git a/g b/g\n--- a/g\n+++ b/g\n@@ -1 +1 @@\n-x\n+y\n")[0];
        assert_eq!(
            file.patch_for_hunk(&other.hunks[0]),
            Err(DiffError::HunkNotInFile)
        );
    }

    #[test]
    fn truncated_and_garbage_input_never_panics() {
        let full = b"diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n a\r\n-b\r\n+c\r\n d\n\\ No newline at end of file\ndiff --git \"a/q\\303\" \"b/q\\303\"\nBinary files a/q and b/q differ\n";
        for n in 0..=full.len() {
            let files = parse_diff(&full[..n]);
            // Aun truncado, lo reconstruido es exactamente la entrada (o nada si no hay `diff --git `).
            let rebuilt = rebuild_diff(&files);
            if n >= 11 {
                assert_eq!(rebuilt, &full[..n], "n={n}");
            } else {
                assert!(rebuilt.is_empty());
            }
        }
        for s in [
            &b"@@ -1 +1 @@\n"[..],
            b"diff --git \n",
            b"diff --git \"a/x\n",
            b"diff --git a/x b/x\n@@ \n",
            b"diff --git a/x b/x\n@@ -a +b @@\n",
            b"diff --git a/x b/x\n@@ -99999999999 +1 @@\n",
            b"\xff\xfe\0\x01diff --git ",
        ] {
            let _ = parse_diff(s);
        }
        let mut seed = 42_u32;
        for _ in 0..300 {
            let mut buf = b"diff --git a/x b/x\n@@ -1,2 +1,2 @@\n".to_vec();
            for _ in 0..80 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                buf.push(b" +-\\\n@d"[(seed >> 24) as usize % 7]);
            }
            let _ = parse_diff(&buf);
        }
    }

    fn crlf_base() -> Vec<u8> {
        (1..=20)
            .flat_map(|i| format!("l{i}\r\n").into_bytes())
            .collect()
    }

    fn build_complex_repo() -> Option<TestRepo> {
        let repo = TestRepo::init()?;
        repo.write("crlf.txt", &crlf_base());
        repo.write("noeol.txt", b"a\nb\nc");
        repo.write("mi archivo ñ.txt", "uno\ndos\ntres\n".as_bytes());
        repo.write("bin.dat", &[0, 1, 2, 3, 255, 0, 9]);
        repo.write("viejo nombre.txt", b"contenido largo suficiente\npara detectar renombrado\ncon varias lineas\nmas lineas\n");
        repo.write("borrar.txt", b"x\ny\n");
        repo.write("modo.sh", b"#!/bin/sh\n");
        repo.write("dash.txt", b"--- inicio\n+++ fin\nresto\n");
        repo.git(&["add", "-A"]);
        repo.git(&["commit", "-m", "base"]);
        let mut crlf = crlf_base();
        crlf = crlf.replace_range_bytes(b"l2\r\n", b"CAMBIADO\r\n");
        crlf = crlf.replace_range_bytes(b"l18\r\n", b"TARDE\r\n");
        repo.write("crlf.txt", &crlf);
        repo.write("noeol.txt", b"a\nB\nc\nnuevo-final-sin-eol");
        repo.write("mi archivo ñ.txt", "uno\nDOS\ntres\n".as_bytes());
        repo.write("bin.dat", &[0, 1, 2, 3, 4, 5, 255, 0, 9, 9]);
        repo.git(&["mv", "viejo nombre.txt", "nuevo nombre.txt"]);
        repo.git(&["rm", "-q", "borrar.txt"]);
        repo.git(&["update-index", "--chmod=+x", "modo.sh"]);
        repo.write("dash.txt", b"+++ fin\nresto\n--- nuevo\n");
        repo.write("nuevo ñandú.txt", "creado\n".as_bytes());
        repo.git(&["add", "-N", "nuevo ñandú.txt"]);
        Some(repo)
    }

    trait ReplaceBytes {
        fn replace_range_bytes(self, from: &[u8], to: &[u8]) -> Vec<u8>;
    }
    impl ReplaceBytes for Vec<u8> {
        fn replace_range_bytes(self, from: &[u8], to: &[u8]) -> Vec<u8> {
            let pos = self.windows(from.len()).position(|w| w == from).unwrap();
            let mut out = self[..pos].to_vec();
            out.extend_from_slice(to);
            out.extend_from_slice(&self[pos + from.len()..]);
            out
        }
    }

    #[test]
    fn round_trip_against_real_git_output() {
        let Some(repo) = build_complex_repo() else {
            return;
        };
        for extra in [
            &[][..],
            &["--binary"][..],
            &["--cached"][..],
            &["-M", "--cached"][..],
            &["HEAD"][..],
        ] {
            let mut args: Vec<&str> = DIFF_ARGS.to_vec();
            args.extend_from_slice(extra);
            let bytes = repo.git(&args);
            let files = parse_diff(&bytes);
            assert!(!files.is_empty(), "{extra:?}");
            assert_eq!(rebuild_diff(&files), bytes, "round-trip con {extra:?}");
        }
        // Verificaciones semánticas sobre `-M --cached`.
        let mut args: Vec<&str> = DIFF_ARGS.to_vec();
        args.extend_from_slice(&["-M", "--cached"]);
        let files = parse_diff(&repo.git(&args));
        let rename = files.iter().find(|f| f.path == "nuevo nombre.txt").unwrap();
        assert_eq!(
            rename.old_path.as_ref().unwrap(),
            &GitPath::from("viejo nombre.txt")
        );
        assert!(files
            .iter()
            .any(|f| f.path == "borrar.txt" && f.is_deleted_file));
        // El diff no preparado: binario, CRLF, sin EOL, Unicode, intent-to-add.
        let files = parse_diff(&repo.git(&DIFF_ARGS));
        assert!(
            files
                .iter()
                .find(|f| f.path == "bin.dat")
                .unwrap()
                .is_binary
        );
        let crlf = files.iter().find(|f| f.path == "crlf.txt").unwrap();
        assert_eq!(crlf.hunks.len(), 2);
        assert!(crlf.hunks[0].lines.iter().any(|l| l.text == "CAMBIADO\r"));
        let noeol = files.iter().find(|f| f.path == "noeol.txt").unwrap();
        assert!(noeol.hunks[0]
            .lines
            .iter()
            .any(|l| l.kind == DiffLineKind::Meta));
        assert!(files.iter().any(|f| f.path == "mi archivo ñ.txt"));
        let added = files.iter().find(|f| f.path == "nuevo ñandú.txt").unwrap();
        assert!(added.is_new_file);
        let dash = files.iter().find(|f| f.path == "dash.txt").unwrap();
        assert_eq!(dash.hunks.len(), 1);
    }

    #[test]
    fn single_hunk_patch_applies_to_real_index_without_touching_cr() {
        let Some(repo) = build_complex_repo() else {
            return;
        };
        let files = parse_diff(&repo.git(&DIFF_ARGS));
        let crlf = files.iter().find(|f| f.path == "crlf.txt").unwrap();
        assert_eq!(crlf.hunks.len(), 2);
        // Prepara SOLO el segundo hunk.
        let patch = crlf.patch_for_hunk(&crlf.hunks[1]).unwrap();
        repo.git_stdin(&["apply", "--cached", "--whitespace=nowarn"], &patch);
        let staged = repo.git(&[
            "diff",
            "--cached",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--",
            "crlf.txt",
        ]);
        let staged = parse_diff(&staged);
        assert_eq!(staged[0].hunks.len(), 1);
        assert!(staged[0].hunks[0].lines.iter().any(|l| l.text == "TARDE\r"));
        assert!(!staged[0].hunks[0]
            .lines
            .iter()
            .any(|l| l.text.contains("CAMBIADO")));
        // Y el primero, tras quitar el segundo del índice con -R.
        repo.git_stdin(&["apply", "--cached", "-R", "--whitespace=nowarn"], &patch);
        let first = crlf.patch_for_hunk(&crlf.hunks[0]).unwrap();
        repo.git_stdin(&["apply", "--cached", "--whitespace=nowarn"], &first);
        let blob = repo.git(&["show", ":crlf.txt"]);
        assert!(blob.starts_with(b"l1\r\nCAMBIADO\r\nl3\r\n"));
        assert!(blob.ends_with(b"l17\r\nl18\r\nl19\r\nl20\r\n"));

        // Sin EOL y archivo nuevo (intent-to-add).
        let noeol = files.iter().find(|f| f.path == "noeol.txt").unwrap();
        repo.git_stdin(
            &["apply", "--cached"],
            &noeol.patch_for_hunks(&[0]).unwrap(),
        );
        assert_eq!(
            repo.git(&["show", ":noeol.txt"]),
            b"a\nB\nc\nnuevo-final-sin-eol"
        );
        let utf = files.iter().find(|f| f.path == "mi archivo ñ.txt").unwrap();
        repo.git_stdin(&["apply", "--cached"], &utf.patch_for_hunks(&[0]).unwrap());
        assert_eq!(
            repo.git(&["show", ":mi archivo ñ.txt"]),
            "uno\nDOS\ntres\n".as_bytes()
        );
    }
}
