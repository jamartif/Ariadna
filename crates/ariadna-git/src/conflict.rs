//! Marcadores de conflicto (`<<<<<<<`, `|||||||`, `=======`, `>>>>>>>`) sobre bytes, sin E/S.
//!
//! El modelo a tres vías de verdad son las etapas del índice (`:1:`, `:2:`, `:3:`); estos
//! marcadores sirven para el editor y para elegir un lado hunk a hunk **sin escribir en
//! disco**: las funciones devuelven los bytes nuevos y es el motor quien decide escribirlos
//! (en binario, tras confirmación).
//!
//! Todo se conserva byte a byte fuera de las regiones de conflicto: finales de línea
//! (LF, CRLF o mezclados), BOM UTF-8 y bytes no UTF-8. Soporta el estilo `merge`
//! (2 vías) y `diff3`/`zdiff3` (con sección base).

use std::collections::HashMap;

use crate::models::{ConflictChoice, ConflictHunk};

/// Región con marcadores: offsets de bytes `[start, end)` dentro del archivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictRegion {
    pub hunk: ConflictHunk,
    /// Inicio de la línea `<<<<<<<`.
    pub start: usize,
    /// Fin (exclusivo) de la línea `>>>>>>>`, incluido su salto de línea.
    pub end: usize,
}

/// Resultado de analizar un archivo con marcadores.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedConflicts {
    pub regions: Vec<ConflictRegion>,
    /// Hay un `<<<<<<<` sin su `>>>>>>>`: el resto del archivo no se interpreta.
    pub unterminated: bool,
}

/// Errores al resolver.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConflictError {
    #[error("el conflicto {0} no tiene sección base (el archivo no está en estilo diff3)")]
    MissingBase(usize),
    #[error("el archivo tiene marcadores sin cerrar; no se resuelve automáticamente")]
    Unterminated,
}

/// Final de línea dominante de un archivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    /// Sin saltos de línea.
    None,
    Lf,
    Crlf,
    Mixed,
}

const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// ¿Empieza el archivo con BOM UTF-8?
pub fn has_bom(bytes: &[u8]) -> bool {
    bytes.starts_with(BOM)
}

/// Final de línea del archivo.
pub fn detect_eol(bytes: &[u8]) -> Eol {
    let (mut lf, mut crlf) = (0usize, 0usize);
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            if i > 0 && bytes[i - 1] == b'\r' {
                crlf += 1;
            } else {
                lf += 1;
            }
        }
    }
    match (lf, crlf) {
        (0, 0) => Eol::None,
        (_, 0) => Eol::Lf,
        (0, _) => Eol::Crlf,
        _ => Eol::Mixed,
    }
}

/// `true` si la línea (sin EOL) es un marcador `c` de exactamente 7 caracteres,
/// opcionalmente seguido de espacio y etiqueta.
fn is_marker(line: &[u8], c: u8) -> bool {
    line.len() >= 7 && line[..7].iter().all(|b| *b == c) && (line.len() == 7 || line[7] == b' ')
}

fn trim_eol(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

struct Line<'a> {
    raw: &'a [u8],
    start: usize,
    end: usize,
}

fn lines_with_offsets(bytes: &[u8]) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|b| *b == b'\n')
            .map_or(bytes.len(), |p| start + p + 1);
        out.push(Line {
            raw: &bytes[start..end],
            start,
            end,
        });
        start = end;
    }
    out
}

/// Analiza los marcadores. No entra en pánico con entrada corrupta.
pub fn parse_conflicts(bytes: &[u8]) -> ParsedConflicts {
    let lines = lines_with_offsets(bytes);
    let mut parsed = ParsedConflicts::default();
    let mut index = 0;
    while index < lines.len() {
        // El BOM solo puede aparecer al principio del archivo, delante del primer marcador.
        let mut head = lines[index].raw;
        if index == 0 {
            head = head.strip_prefix(BOM).unwrap_or(head);
        }
        if !is_marker(trim_eol(head), b'<') {
            index += 1;
            continue;
        }
        let start = lines[index].start
            + if index == 0 && has_bom(bytes) {
                BOM.len()
            } else {
                0
            };
        let (mut ours, mut base, mut theirs) = (Vec::new(), Vec::new(), Vec::new());
        let mut saw_base = false;
        let mut phase = 1; // 1 ours, 2 base, 3 theirs
        let mut closed = false;
        index += 1;
        while index < lines.len() {
            let raw = lines[index].raw;
            let text = trim_eol(raw);
            if phase == 1 && is_marker(text, b'|') {
                saw_base = true;
                phase = 2;
            } else if (phase == 1 || phase == 2) && is_marker(text, b'=') {
                phase = 3;
            } else if phase == 3 && is_marker(text, b'>') {
                parsed.regions.push(ConflictRegion {
                    hunk: ConflictHunk {
                        id: parsed.regions.len(),
                        ours: std::mem::take(&mut ours),
                        base: saw_base.then(|| std::mem::take(&mut base)),
                        theirs: std::mem::take(&mut theirs),
                    },
                    start,
                    end: lines[index].end,
                });
                index += 1;
                closed = true;
                break;
            } else {
                let target = match phase {
                    1 => &mut ours,
                    2 => &mut base,
                    _ => &mut theirs,
                };
                target.extend_from_slice(raw);
            }
            index += 1;
        }
        if !closed {
            parsed.unterminated = true;
            break;
        }
    }
    parsed
}

/// Hunks de conflicto del archivo.
pub fn conflict_hunks(bytes: &[u8]) -> Vec<ConflictHunk> {
    parse_conflicts(bytes)
        .regions
        .into_iter()
        .map(|r| r.hunk)
        .collect()
}

fn side(hunk: &ConflictHunk, choice: ConflictChoice) -> Result<&[u8], ConflictError> {
    match choice {
        ConflictChoice::Ours => Ok(&hunk.ours),
        ConflictChoice::Theirs => Ok(&hunk.theirs),
        ConflictChoice::Base => hunk
            .base
            .as_deref()
            .ok_or(ConflictError::MissingBase(hunk.id)),
    }
}

/// Resuelve los hunks que tengan elección en `choices` (clave = `ConflictHunk::id`);
/// los demás conservan sus marcadores. Devuelve los bytes nuevos; no escribe nada.
///
/// Elegir `Base` en un hunk sin sección base es un error (no se borra contenido en silencio).
/// Con marcadores sin cerrar se rechaza para no truncar el archivo.
pub fn resolve_hunks(
    bytes: &[u8],
    choices: &HashMap<usize, ConflictChoice>,
) -> Result<Vec<u8>, ConflictError> {
    let parsed = parse_conflicts(bytes);
    if parsed.unterminated && !choices.is_empty() {
        return Err(ConflictError::Unterminated);
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut cursor = 0;
    for region in &parsed.regions {
        out.extend_from_slice(&bytes[cursor..region.start]);
        match choices.get(&region.hunk.id) {
            Some(choice) => out.extend_from_slice(side(&region.hunk, *choice)?),
            None => out.extend_from_slice(&bytes[region.start..region.end]),
        }
        cursor = region.end;
    }
    out.extend_from_slice(&bytes[cursor..]);
    Ok(out)
}

/// Resuelve **todos** los hunks con el mismo lado (elección por archivo).
pub fn resolve_all(bytes: &[u8], choice: ConflictChoice) -> Result<Vec<u8>, ConflictError> {
    let parsed = parse_conflicts(bytes);
    if parsed.unterminated {
        return Err(ConflictError::Unterminated);
    }
    let choices: HashMap<usize, ConflictChoice> =
        parsed.regions.iter().map(|r| (r.hunk.id, choice)).collect();
    resolve_hunks(bytes, &choices)
}

/// ¿Quedan marcadores sin resolver?
pub fn has_conflict_markers(bytes: &[u8]) -> bool {
    let parsed = parse_conflicts(bytes);
    !parsed.regions.is_empty() || parsed.unterminated
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::testutil::TestRepo;

    const DIFF3: &str = "line1\n<<<<<<< HEAD\nfrom-main\n||||||| base\nline2\n=======\nfrom-feature\n>>>>>>> feature\nline3";

    #[test]
    fn diff3_hunk_and_resolution() {
        let hunks = conflict_hunks(DIFF3.as_bytes());
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].ours, b"from-main\n");
        assert_eq!(hunks[0].base.as_deref(), Some(&b"line2\n"[..]));
        assert_eq!(hunks[0].theirs, b"from-feature\n");

        let choices = HashMap::from([(0, ConflictChoice::Ours)]);
        let resolved = resolve_hunks(DIFF3.as_bytes(), &choices).unwrap();
        assert_eq!(resolved, b"line1\nfrom-main\nline3");
        let unchanged = resolve_hunks(DIFF3.as_bytes(), &HashMap::new()).unwrap();
        assert_eq!(unchanged, DIFF3.as_bytes());
        assert_eq!(
            resolve_all(DIFF3.as_bytes(), ConflictChoice::Base).unwrap(),
            b"line1\nline2\nline3"
        );
        assert_eq!(
            resolve_all(DIFF3.as_bytes(), ConflictChoice::Theirs).unwrap(),
            b"line1\nfrom-feature\nline3"
        );
    }

    #[test]
    fn two_way_style_has_no_base() {
        let text = b"<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\n";
        let hunks = conflict_hunks(text);
        assert_eq!(hunks[0].base, None);
        assert_eq!(
            resolve_all(text, ConflictChoice::Base),
            Err(ConflictError::MissingBase(0))
        );
        assert_eq!(resolve_all(text, ConflictChoice::Ours).unwrap(), b"a\n");
    }

    #[test]
    fn per_hunk_choice_with_several_hunks() {
        let text = b"x\n<<<<<<< a\n1\n=======\n2\n>>>>>>> b\nmid\n<<<<<<< a\n3\n=======\n4\n>>>>>>> b\ny\n";
        let choices = HashMap::from([(1, ConflictChoice::Theirs)]);
        let out = resolve_hunks(text, &choices).unwrap();
        assert_eq!(out, b"x\n<<<<<<< a\n1\n=======\n2\n>>>>>>> b\nmid\n4\ny\n");
        assert!(has_conflict_markers(&out));
        assert!(!has_conflict_markers(
            &resolve_all(text, ConflictChoice::Ours).unwrap()
        ));
    }

    #[test]
    fn crlf_is_preserved_everywhere() {
        let text = b"l1\r\n<<<<<<< HEAD\r\nours\r\n||||||| base\r\nbase\r\n=======\r\ntheirs\r\n>>>>>>> feat\r\nl3\r\n";
        let hunks = conflict_hunks(text);
        assert_eq!(hunks[0].ours, b"ours\r\n");
        assert_eq!(hunks[0].base.as_deref(), Some(&b"base\r\n"[..]));
        assert_eq!(hunks[0].theirs, b"theirs\r\n");
        let out = resolve_all(text, ConflictChoice::Theirs).unwrap();
        assert_eq!(out, b"l1\r\ntheirs\r\nl3\r\n");
        assert_eq!(detect_eol(&out), Eol::Crlf);
    }

    #[test]
    fn markers_with_lf_inside_crlf_file_are_handled() {
        let text = b"l1\r\n<<<<<<< HEAD\nours\r\n=======\ntheirs\r\n>>>>>>> feat\nl3\r\n";
        assert_eq!(
            resolve_all(text, ConflictChoice::Ours).unwrap(),
            b"l1\r\nours\r\nl3\r\n"
        );
    }

    #[test]
    fn bom_is_preserved_even_when_first_line_is_a_marker() {
        let mut text = BOM.to_vec();
        text.extend_from_slice(b"<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> x\ntail\n");
        let out = resolve_all(&text, ConflictChoice::Theirs).unwrap();
        assert_eq!(out, [BOM, b"theirs\ntail\n"].concat());
        assert!(has_bom(&out));

        let mut with_text = BOM.to_vec();
        with_text.extend_from_slice(b"head\n<<<<<<< a\n1\n=======\n2\n>>>>>>> b\n");
        assert_eq!(
            resolve_all(&with_text, ConflictChoice::Ours).unwrap(),
            [BOM, b"head\n1\n"].concat()
        );
    }

    #[test]
    fn eight_marker_chars_or_glued_text_are_not_markers() {
        let text = b"<<<<<<<< no\n<<<<<<<x\n=======\n>>>>>>>\n";
        assert!(conflict_hunks(text).is_empty());
        assert!(!has_conflict_markers(text));
    }

    #[test]
    fn non_utf8_bytes_survive() {
        let text = b"\xff\xfe\n<<<<<<< a\n\x80ours\n=======\n\x81theirs\n>>>>>>> b\n\xc3\n";
        assert_eq!(
            resolve_all(text, ConflictChoice::Ours).unwrap(),
            b"\xff\xfe\n\x80ours\n\xc3\n"
        );
    }

    #[test]
    fn unterminated_markers_are_reported_and_not_truncated() {
        let text = b"a\n<<<<<<< x\nours\n=======\ntheirs\n";
        let parsed = parse_conflicts(text);
        assert!(parsed.unterminated && parsed.regions.is_empty());
        assert!(has_conflict_markers(text));
        assert_eq!(
            resolve_all(text, ConflictChoice::Ours),
            Err(ConflictError::Unterminated)
        );
        assert_eq!(resolve_hunks(text, &HashMap::new()).unwrap(), text);
    }

    #[test]
    fn eol_detection() {
        assert_eq!(detect_eol(b"a"), Eol::None);
        assert_eq!(detect_eol(b"a\nb\n"), Eol::Lf);
        assert_eq!(detect_eol(b"a\r\nb\r\n"), Eol::Crlf);
        assert_eq!(detect_eol(b"a\r\nb\n"), Eol::Mixed);
    }

    #[test]
    fn garbage_and_prefixes_never_panic() {
        let full =
            b"\xef\xbb\xbf<<<<<<< a\r\n1\r\n||||||| b\r\n0\r\n=======\r\n2\r\n>>>>>>> c\r\nz";
        for n in 0..=full.len() {
            let _ = parse_conflicts(&full[..n]);
            let _ = resolve_all(&full[..n], ConflictChoice::Base);
        }
        let mut seed = 7_u32;
        for _ in 0..300 {
            let mut buf = Vec::new();
            for _ in 0..60 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let pick = (seed >> 24) as usize % 6;
                buf.extend_from_slice(
                    [
                        &b"<<<<<<< a\n"[..],
                        b"=======\n",
                        b">>>>>>> b\n",
                        b"||||||| c\r\n",
                        b"x\n",
                        b"\xff",
                    ][pick],
                );
            }
            let _ = resolve_all(&buf, ConflictChoice::Ours);
        }
    }

    fn make_conflict(
        style: &str,
        base: &[u8],
        ours: &[u8],
        theirs: &[u8],
    ) -> Option<(TestRepo, Vec<u8>)> {
        let repo = TestRepo::init()?;
        repo.git(&["config", "merge.conflictStyle", style]);
        repo.write("f.txt", base);
        repo.git(&["add", "."]);
        repo.git(&["commit", "-qm", "base"]);
        repo.git(&["checkout", "-qb", "feature"]);
        repo.write("f.txt", theirs);
        repo.git(&["commit", "-qam", "theirs"]);
        repo.git(&["checkout", "-q", "main"]);
        repo.write("f.txt", ours);
        repo.git(&["commit", "-qam", "ours"]);
        let merge = std::process::Command::new("git")
            .current_dir(repo.path())
            .args(["merge", "feature"])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(!merge.status.success());
        let work = std::fs::read(repo.path().join("f.txt")).unwrap();
        Some((repo, work))
    }

    #[test]
    fn real_git_conflicts_resolve_to_the_index_sides() {
        let bom = |s: &[u8]| [BOM, s].concat();
        type Case = (&'static str, Vec<u8>, Vec<u8>, Vec<u8>);
        let cases: [Case; 3] = [
            (
                "diff3",
                b"a\nb\nc\n".to_vec(),
                b"a\nOURS\nc\n".to_vec(),
                b"a\nTHEIRS\nc\n".to_vec(),
            ),
            (
                "merge",
                b"a\r\nb\r\nc\r\n".to_vec(),
                b"a\r\nOURS\r\nc\r\n".to_vec(),
                b"a\r\nTHEIRS\r\nc\r\n".to_vec(),
            ),
            (
                "zdiff3",
                bom(b"a\r\nb\r\nc\r\n"),
                bom(b"a\r\nOURS\r\nc\r\n"),
                bom(b"a\r\nTHEIRS\r\nc\r\n"),
            ),
        ];
        for (style, base, ours, theirs) in cases {
            let Some((repo, work)) = make_conflict(style, &base, &ours, &theirs) else {
                return;
            };
            let hunks = conflict_hunks(&work);
            assert_eq!(
                hunks.len(),
                1,
                "{style}: {}",
                String::from_utf8_lossy(&work)
            );
            assert_eq!(hunks[0].base.is_some(), style != "merge", "{style}");
            assert_eq!(
                resolve_all(&work, ConflictChoice::Ours).unwrap(),
                repo.git(&["show", ":2:f.txt"]),
                "{style}"
            );
            assert_eq!(
                resolve_all(&work, ConflictChoice::Theirs).unwrap(),
                repo.git(&["show", ":3:f.txt"]),
                "{style}"
            );
            if style != "merge" {
                assert_eq!(
                    resolve_all(&work, ConflictChoice::Base).unwrap(),
                    repo.git(&["show", ":1:f.txt"]),
                    "{style}"
                );
            }
        }
    }
}
