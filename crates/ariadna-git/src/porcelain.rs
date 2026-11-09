//! Parser de `git status --porcelain=v2 -z --branch` sobre bytes.
//!
//! Con `-z` cada registro termina en NUL, así que los espacios, los saltos de línea y
//! los bytes no UTF-8 de las rutas no rompen el análisis. El parser **nunca entra en
//! pánico**: los registros mal formados se omiten.

use crate::models::{ChangeStatus, ConflictKind, GitPath, GitStatus, StatusEntry};

/// Interpreta la salida completa de `git status --porcelain=v2 -z --branch`.
pub fn parse_status(bytes: &[u8]) -> GitStatus {
    let mut segments: Vec<&[u8]> = bytes.split(|b| *b == 0).collect();
    if segments.last().is_some_and(|s| s.is_empty()) {
        segments.pop();
    }

    let mut status = GitStatus::default();
    let mut index = 0;
    while index < segments.len() {
        let segment = segments[index];
        index += 1;
        if segment.is_empty() {
            continue;
        }
        if let Some(header) = segment.strip_prefix(b"# ") {
            parse_header(header, &mut status);
        } else if segment.starts_with(b"1 ") {
            if let Some(entry) = ordinary(segment) {
                status.entries.push(entry);
            }
        } else if segment.starts_with(b"2 ") {
            // El registro de renombrado ocupa dos segmentos: destino y origen.
            if let Some(mut entry) = renamed(segment) {
                if let Some(original) = segments.get(index) {
                    entry.original_path = Some(GitPath::from(*original));
                    index += 1;
                }
                status.entries.push(entry);
            }
        } else if segment.starts_with(b"u ") {
            if let Some(entry) = unmerged(segment) {
                status.entries.push(entry);
            }
        } else if let Some(path) = segment.strip_prefix(b"? ") {
            status.entries.push(simple(path, true, false));
        } else if let Some(path) = segment.strip_prefix(b"! ") {
            status.entries.push(simple(path, false, true));
        }
    }
    status
}

fn parse_header(header: &[u8], status: &mut GitStatus) {
    let text = |prefix: &[u8]| {
        header
            .strip_prefix(prefix)
            .map(|v| String::from_utf8_lossy(v).into_owned())
    };
    if let Some(value) = text(b"branch.oid ") {
        if value == "(initial)" {
            status.is_unborn = true;
            status.head_oid = None;
        } else {
            status.head_oid = Some(value);
        }
    } else if let Some(value) = text(b"branch.head ") {
        if value == "(detached)" {
            status.is_detached = true;
            status.branch_name = None;
        } else {
            status.branch_name = Some(value);
        }
    } else if let Some(value) = text(b"branch.upstream ") {
        status.upstream = Some(value);
    } else if let Some(value) = text(b"branch.ab ") {
        let (ahead, behind) = parse_ahead_behind(&value);
        status.ahead = ahead;
        status.behind = behind;
    }
}

fn parse_ahead_behind(value: &str) -> (u32, u32) {
    let mut parts = value.split(' ');
    let (Some(a), Some(b), None) = (parts.next(), parts.next(), parts.next()) else {
        return (0, 0);
    };
    let ahead = a.trim_start_matches('+').parse().unwrap_or(0);
    let behind = b.trim_start_matches('-').parse().unwrap_or(0);
    (ahead, behind)
}

/// Parte `count - 1` campos separados por un espacio; el último campo (la ruta) es el
/// resto del registro y puede contener espacios.
fn split_fields(record: &[u8], count: usize) -> Option<Vec<&[u8]>> {
    let mut rest = record.get(2..)?;
    let mut fields = Vec::with_capacity(count);
    while fields.len() < count - 1 {
        let space = rest.iter().position(|b| *b == b' ')?;
        let token = &rest[..space];
        if token.is_empty() {
            return None;
        }
        fields.push(token);
        rest = &rest[space + 1..];
    }
    if rest.is_empty() {
        return None;
    }
    fields.push(rest);
    Some(fields)
}

fn xy_status(xy: &[u8], at: usize) -> ChangeStatus {
    if xy.len() < 2 {
        return ChangeStatus::Unknown;
    }
    ChangeStatus::from_porcelain(xy[at])
}

fn ordinary(record: &[u8]) -> Option<StatusEntry> {
    let fields = split_fields(record, 8)?;
    Some(changed_entry(&fields, 7))
}

fn renamed(record: &[u8]) -> Option<StatusEntry> {
    let fields = split_fields(record, 9)?;
    Some(changed_entry(&fields, 8))
}

fn changed_entry(fields: &[&[u8]], path_field: usize) -> StatusEntry {
    let xy = fields[0];
    let index_status = xy_status(xy, 0);
    let work_tree_status = xy_status(xy, 1);
    let conflicted =
        index_status == ChangeStatus::Unmerged || work_tree_status == ChangeStatus::Unmerged;
    StatusEntry {
        path: GitPath::from(fields[path_field]),
        original_path: None,
        index_status,
        work_tree_status,
        is_conflicted: conflicted,
        is_untracked: false,
        is_ignored: false,
        is_submodule: fields[1].starts_with(b"S"),
        conflict_kind: conflicted.then(|| ConflictKind::from_porcelain_xy(xy)),
    }
}

fn unmerged(record: &[u8]) -> Option<StatusEntry> {
    let fields = split_fields(record, 10)?;
    let xy = fields[0];
    Some(StatusEntry {
        path: GitPath::from(fields[9]),
        original_path: None,
        index_status: xy_status(xy, 0),
        work_tree_status: xy_status(xy, 1),
        is_conflicted: true,
        is_untracked: false,
        is_ignored: false,
        is_submodule: fields[1].starts_with(b"S"),
        conflict_kind: Some(ConflictKind::from_porcelain_xy(xy)),
    })
}

fn simple(path: &[u8], untracked: bool, ignored: bool) -> StatusEntry {
    StatusEntry {
        path: GitPath::from(path),
        original_path: None,
        index_status: ChangeStatus::Unmodified,
        work_tree_status: if untracked {
            ChangeStatus::Untracked
        } else if ignored {
            ChangeStatus::Ignored
        } else {
            ChangeStatus::Unmodified
        },
        is_conflicted: false,
        is_untracked: untracked,
        is_ignored: ignored,
        is_submodule: false,
        conflict_kind: None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn ordinary_untracked_and_branch_headers() {
        let sample = b"# branch.oid abcdef\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +1 -2\x001 .M N... 100644 100644 100644 aaa bbb file.txt\0? sub/weird name.txt\0";
        let status = parse_status(sample);
        assert_eq!(status.branch_name.as_deref(), Some("main"));
        assert_eq!(status.head_oid.as_deref(), Some("abcdef"));
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!((status.ahead, status.behind), (1, 2));
        assert!(!status.is_detached);
        assert_eq!(status.entries.len(), 2);

        let modified = &status.entries[0];
        assert_eq!(modified.path, "file.txt");
        assert_eq!(modified.index_status, ChangeStatus::Unmodified);
        assert_eq!(modified.work_tree_status, ChangeStatus::Modified);
        assert!(modified.has_unstaged_change());
        assert!(!modified.is_staged());

        let untracked = &status.entries[1];
        assert_eq!(untracked.path, "sub/weird name.txt");
        assert!(untracked.is_untracked);
    }

    #[test]
    fn rename_with_spaces_and_unicode() {
        let sample = "2 R. N... 100644 100644 100644 aaa bbb R100 notas/cañón archivo.txt\0viejo nombre.txt\0";
        let status = parse_status(sample.as_bytes());
        let entry = &status.entries[0];
        assert_eq!(entry.path, "notas/cañón archivo.txt");
        assert_eq!(
            entry.original_path.as_ref().unwrap(),
            &GitPath::from("viejo nombre.txt")
        );
        assert_eq!(entry.index_status, ChangeStatus::Renamed);
        assert!(!entry.path.as_bytes().contains(&0));
    }

    #[test]
    fn rename_modified_and_conflict() {
        let status =
            parse_status(b"2 RM N... 100644 100644 100644 aaa bbb R100 renamed.txt\0file.txt\0");
        let renamed = &status.entries[0];
        assert_eq!(renamed.path, "renamed.txt");
        assert_eq!(
            renamed.original_path.as_ref().unwrap(),
            &GitPath::from("file.txt")
        );
        assert_eq!(renamed.index_status, ChangeStatus::Renamed);
        assert_eq!(renamed.work_tree_status, ChangeStatus::Modified);
        assert!(renamed.is_staged() && renamed.has_unstaged_change());

        let status = parse_status(b"u UU N... 100644 100644 100644 100644 a b c conflict.txt\0");
        let entry = &status.entries[0];
        assert!(entry.is_conflicted);
        assert_eq!(entry.conflict_kind, Some(ConflictKind::BothModified));
        assert!(!entry.is_staged());
        assert_eq!(status.conflicted().len(), 1);
    }

    #[test]
    fn all_conflict_kinds() {
        for (xy, kind) in [
            ("AA", ConflictKind::BothAdded),
            ("DD", ConflictKind::BothDeleted),
            ("AU", ConflictKind::AddedByUs),
            ("UA", ConflictKind::AddedByThem),
            ("DU", ConflictKind::DeletedByUs),
            ("UD", ConflictKind::DeletedByThem),
        ] {
            let rec = format!("u {xy} N... 100644 100644 100644 100644 a b c f.txt\0");
            assert_eq!(
                parse_status(rec.as_bytes()).entries[0].conflict_kind,
                Some(kind),
                "{xy}"
            );
        }
    }

    #[test]
    fn detached_and_unborn() {
        let status = parse_status(b"# branch.oid deadbeef\0# branch.head (detached)\0");
        assert!(status.is_detached);
        assert!(status.branch_name.is_none());
        assert_eq!(status.head_oid.as_deref(), Some("deadbeef"));
        assert!(status.detached_commit_warning().is_some());

        let fresh = parse_status(b"# branch.oid (initial)\0# branch.head main\0");
        assert!(fresh.is_unborn);
        assert!(fresh.head_oid.is_none());
        assert_eq!(fresh.branch_name.as_deref(), Some("main"));
    }

    #[test]
    fn ignored_untracked_and_submodule() {
        let status =
            parse_status(b"! target/\0? nuevo.txt\x001 .M S.M. 160000 160000 160000 aaa bbb sub\0");
        assert!(status.entries[0].is_ignored);
        assert_eq!(status.entries[0].work_tree_status, ChangeStatus::Ignored);
        assert!(status.entries[1].is_untracked);
        assert!(status.entries[2].is_submodule);
        assert_eq!(status.ignored().len(), 1);
        assert_eq!(status.unstaged().len(), 2);
        assert_eq!(status.staged().len(), 0);
    }

    #[test]
    fn non_utf8_and_newline_paths_are_preserved() {
        let mut data = b"? ".to_vec();
        data.extend_from_slice(&[b'a', 0xFF, 0xFE, b'\n', b'z']);
        data.push(0);
        let status = parse_status(&data);
        assert_eq!(
            status.entries[0].path.as_bytes(),
            &[b'a', 0xFF, 0xFE, b'\n', b'z']
        );
    }

    #[test]
    fn path_with_leading_space_and_double_spaces() {
        let status = parse_status(b"1 .M N... 100644 100644 100644 a b   raro  nombre \0");
        assert_eq!(status.entries[0].path.as_bytes(), b"  raro  nombre ");
    }

    #[test]
    fn upstream_gone_has_no_ab() {
        let status = parse_status(b"# branch.oid a\0# branch.head x\0# branch.upstream origin/x\0");
        assert_eq!(status.upstream.as_deref(), Some("origin/x"));
        assert_eq!((status.ahead, status.behind), (0, 0));
    }

    #[test]
    fn empty_input_is_empty_status() {
        assert_eq!(parse_status(b""), GitStatus::default());
        assert_eq!(parse_status(b"\0\0\0"), GitStatus::default());
    }

    #[test]
    fn garbage_and_truncated_input_never_panics() {
        let samples: [&[u8]; 12] = [
            b"1",
            b"1 ",
            b"1 .M",
            b"1 .M N... 100644",
            b"2 R. N... 100644 100644 100644 aaa bbb R100",
            b"2 R. N... 100644 100644 100644 aaa bbb R100 nuevo",
            b"u UU N...",
            b"# branch.ab",
            b"# branch.ab +x -y",
            b"# branch.ab +1 -2 -3",
            b"\xff\xfe\0\x01\x02\0? \0! ",
            b"? \0",
        ];
        for s in samples {
            let _ = parse_status(s);
        }
        // Rename truncado: se conserva sin origen.
        let status = parse_status(b"2 R. N... 100644 100644 100644 aaa bbb R100 nuevo\0");
        assert!(status.entries[0].original_path.is_none());
        // Todos los prefijos de una salida válida.
        let full = b"# branch.oid abc\0# branch.head main\0# branch.ab +1 -2\x002 R. N... 100644 100644 100644 a b R100 n\0o\0u UU N... 100644 100644 100644 100644 a b c f\0? x\0! y\0";
        for n in 0..full.len() {
            let _ = parse_status(&full[..n]);
        }
        // Basura pseudoaleatoria determinista.
        let mut seed = 0x1234_5678_u32;
        for _ in 0..200 {
            let mut buf = Vec::new();
            for _ in 0..64 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                buf.push((seed >> 24) as u8);
            }
            let _ = parse_status(&buf);
        }
    }
}
