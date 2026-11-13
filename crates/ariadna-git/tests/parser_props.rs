//! Propiedades (`proptest`) de los parsers de `ariadna-git` (M1.11).
//!
//! 1. **Robustez**: con bytes arbitrarios, UTF-8 inválido o entrada truncada, ningún parser
//!    entra en pánico (un pánico en el parser tumbaría la sesión de un repo).
//! 2. **Invariantes** con entradas bien formadas: `parse → rebuild` reproduce el diff byte a
//!    byte, los contadores del encabezado `@@` cuadran con las líneas clasificadas, los
//!    registros de `status` se cuentan bien y resolver conflictos elimina los marcadores.
#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use ariadna_git::{
    conflict_hunks, has_conflict_markers, parse_branches, parse_commit_log, parse_conflicts,
    parse_diff, parse_remotes, parse_status, rebuild_diff, resolve_all, resolve_hunks,
    unquote_c_style, validate_ref, validate_repo_path, validate_revision_arg, ConflictChoice,
    DiffLineKind,
};
use proptest::collection::vec;
use proptest::prelude::*;

fn cfg() -> ProptestConfig {
    ProptestConfig {
        cases: 512,
        ..ProptestConfig::default()
    }
}

/// Bytes arbitrarios sesgados hacia los separadores y las palabras clave de cada formato.
fn noisy_bytes() -> impl Strategy<Value = Vec<u8>> {
    let atom = prop_oneof![
        4 => any::<u8>().prop_map(|b| vec![b]),
        1 => Just(b"\0".to_vec()),
        1 => Just(b"\n".to_vec()),
        1 => Just(b"\r\n".to_vec()),
        1 => Just(vec![0x1e]),
        1 => Just(vec![0x1f]),
        1 => Just(b"diff --git a/x b/x\n".to_vec()),
        1 => Just(b"--- a/x\n+++ b/x\n".to_vec()),
        1 => Just(b"@@ -1,2 +1,3 @@ f\n".to_vec()),
        1 => Just(b"@@ -4294967295,4294967295 +4294967295 @@\n".to_vec()),
        1 => Just(b"Binary files a/x and b/y differ\n".to_vec()),
        1 => Just(b"rename from \"a\\303\n".to_vec()),
        1 => Just(b"\"\\".to_vec()),
        1 => Just(b"1 .M N... 100644 100644 100644 ".to_vec()),
        1 => Just(b"2 R. N... ".to_vec()),
        1 => Just(b"u UU N... ".to_vec()),
        1 => Just(b"? ".to_vec()),
        1 => Just(b"# branch.ab +1 -2".to_vec()),
        1 => Just(b"<<<<<<< HEAD\n".to_vec()),
        1 => Just(b"||||||| base\n".to_vec()),
        1 => Just(b"=======\n".to_vec()),
        1 => Just(b">>>>>>> x\n".to_vec()),
        1 => Just(vec![0xEF, 0xBB, 0xBF]),
        1 => Just(vec![0xFF, 0xFE, 0xC3]),
    ];
    vec(atom, 0..40).prop_map(|parts| parts.concat())
}

// ---------------------------------------------------------------------------------------
// Generadores de entradas bien formadas
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum L {
    Ctx(Vec<u8>),
    Add(Vec<u8>),
    Del(Vec<u8>),
}

fn line_text() -> impl Strategy<Value = Vec<u8>> {
    // Cualquier byte salvo `\n` (el `\r` y los no UTF-8 son legítimos).
    vec(any::<u8>().prop_filter("sin salto", |b| *b != b'\n'), 0..30)
}

fn hunk_lines() -> impl Strategy<Value = Vec<L>> {
    vec(
        prop_oneof![
            line_text().prop_map(L::Ctx),
            line_text().prop_map(L::Add),
            line_text().prop_map(L::Del),
        ],
        1..25,
    )
}

type HunkSpec = (u32, u32, Vec<L>);
type FileSpec = (String, Vec<HunkSpec>);

fn render_diff(files: &[FileSpec]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, hunks) in files {
        out.extend(format!("diff --git a/{name} b/{name}\nindex 111..222 100644\n").bytes());
        out.extend(format!("--- a/{name}\n+++ b/{name}\n").bytes());
        for (old_start, new_start, lines) in hunks {
            let old = lines.iter().filter(|l| !matches!(l, L::Add(_))).count();
            let new = lines.iter().filter(|l| !matches!(l, L::Del(_))).count();
            out.extend(format!("@@ -{old_start},{old} +{new_start},{new} @@ fn x\n").bytes());
            for l in lines {
                let (sigil, text) = match l {
                    L::Ctx(t) => (b' ', t),
                    L::Add(t) => (b'+', t),
                    L::Del(t) => (b'-', t),
                };
                out.push(sigil);
                out.extend(text);
                out.push(b'\n');
            }
        }
    }
    out
}

fn diff_files() -> impl Strategy<Value = Vec<FileSpec>> {
    vec(
        (
            "[a-z][a-z0-9_]{0,8}(/[a-z0-9_]{1,8}){0,2}\\.[a-z]{1,3}",
            vec((1u32..5000, 1u32..5000, hunk_lines()), 1..4),
        ),
        1..4,
    )
}

proptest! {
    #![proptest_config(cfg())]

    // ------------------------------- robustez ------------------------------------------

    #[test]
    fn status_never_panics(bytes in noisy_bytes()) {
        let _ = parse_status(&bytes);
    }

    #[test]
    fn status_never_panics_on_raw_bytes(bytes in vec(any::<u8>(), 0..400)) {
        let _ = parse_status(&bytes);
    }

    #[test]
    fn diff_never_panics(bytes in noisy_bytes()) {
        let files = parse_diff(&bytes);
        // Aunque la entrada sea basura, reconstruir y recortar un hunk tampoco debe fallar.
        let _ = rebuild_diff(&files);
        for f in &files {
            let _ = f.patch_for_whole_file();
            if !f.hunks.is_empty() {
                let all: Vec<usize> = (0..f.hunks.len()).collect();
                let _ = f.patch_for_hunks(&all);
            }
        }
    }

    #[test]
    fn diff_never_panics_on_raw_bytes(bytes in vec(any::<u8>(), 0..600)) {
        let _ = parse_diff(&bytes);
    }

    #[test]
    fn log_branches_remotes_never_panic(bytes in noisy_bytes()) {
        let _ = parse_commit_log(&bytes);
        let _ = parse_branches(&bytes);
        let _ = parse_remotes(&bytes);
    }

    #[test]
    fn conflicts_never_panic(bytes in noisy_bytes()) {
        let parsed = parse_conflicts(&bytes);
        let _ = conflict_hunks(&bytes);
        let _ = has_conflict_markers(&bytes);
        let _ = resolve_all(&bytes, ConflictChoice::Ours);
        let _ = resolve_all(&bytes, ConflictChoice::Base);
        let all: HashMap<usize, ConflictChoice> = parsed
            .regions
            .iter()
            .map(|r| (r.hunk.id, ConflictChoice::Theirs))
            .collect();
        let _ = resolve_hunks(&bytes, &all);
        for r in &parsed.regions {
            prop_assert!(r.start < r.end && r.end <= bytes.len());
        }
    }

    #[test]
    fn unquote_and_validators_never_panic(bytes in vec(any::<u8>(), 0..200), text in ".{0,80}") {
        let _ = unquote_c_style(&bytes);
        let _ = validate_repo_path(&bytes);
        let _ = validate_ref(&text);
        let _ = validate_revision_arg(&text);
    }

    // ------------------------------- truncado ------------------------------------------

    #[test]
    fn every_prefix_of_a_valid_diff_parses(files in diff_files(), cut in 0.0f64..1.0) {
        let full = render_diff(&files);
        let at = (full.len() as f64 * cut) as usize;
        let _ = parse_diff(&full[..at]);
        let _ = parse_diff(&full[at..]);
    }

    // ------------------------------- invariantes ---------------------------------------

    #[test]
    fn diff_roundtrip_and_counts(files in diff_files()) {
        let bytes = render_diff(&files);
        let parsed = parse_diff(&bytes);
        prop_assert_eq!(rebuild_diff(&parsed), bytes);
        prop_assert_eq!(parsed.len(), files.len());
        for (parsed, (name, hunks)) in parsed.iter().zip(&files) {
            prop_assert_eq!(parsed.path.to_string_lossy().into_owned(), name.clone());
            prop_assert_eq!(parsed.hunks.len(), hunks.len());
            for (h, (old_start, new_start, lines)) in parsed.hunks.iter().zip(hunks) {
                prop_assert_eq!(h.old_start, *old_start);
                prop_assert_eq!(h.new_start, *new_start);
                prop_assert_eq!(h.lines.len(), lines.len());
                let count = |k: DiffLineKind| h.lines.iter().filter(|l| l.kind == k).count() as u32;
                let (ctx, add, del) = (
                    count(DiffLineKind::Context),
                    count(DiffLineKind::Addition),
                    count(DiffLineKind::Deletion),
                );
                prop_assert_eq!(h.old_count, ctx + del);
                prop_assert_eq!(h.new_count, ctx + add);
            }
        }
    }

    #[test]
    fn whole_file_patch_of_a_parsed_diff_reparses_identically(files in diff_files()) {
        let bytes = render_diff(&files);
        for f in parse_diff(&bytes) {
            let patch = f.patch_for_whole_file();
            let again = parse_diff(&patch);
            prop_assert_eq!(again.len(), 1);
            prop_assert_eq!(&again[0].hunks.len(), &f.hunks.len());
            for (a, b) in again[0].hunks.iter().zip(&f.hunks) {
                prop_assert_eq!(&a.raw, &b.raw);
            }
        }
    }

    #[test]
    fn status_counts_match_generated_records(
        untracked in vec("[a-zA-Z0-9 _./-]{1,20}", 0..30),
        modified in vec("[a-zA-Z0-9 _./-]{1,20}", 0..30),
        renamed in vec(("[a-z ]{1,10}", "[a-z ]{1,10}"), 0..10),
    ) {
        let mut raw = b"# branch.oid abc\0# branch.head main\0".to_vec();
        for p in &modified {
            raw.extend(format!("1 .M N... 100644 100644 100644 aaa bbb {p}\0").bytes());
        }
        for (to, from) in &renamed {
            raw.extend(format!("2 R. N... 100644 100644 100644 aaa bbb R100 {to}\0{from}\0").bytes());
        }
        for p in &untracked {
            raw.extend(format!("? {p}\0").bytes());
        }
        let st = parse_status(&raw);
        prop_assert_eq!(st.entries.len(), modified.len() + renamed.len() + untracked.len());
        prop_assert_eq!(st.entries.iter().filter(|e| e.is_untracked).count(), untracked.len());
        prop_assert_eq!(st.entries.iter().filter(|e| e.original_path.is_some()).count(), renamed.len());
        // Truncar en cualquier punto no entra en pánico y nunca inventa entradas.
        for cut in (0..raw.len()).step_by(7) {
            prop_assert!(parse_status(&raw[..cut]).entries.len() <= st.entries.len());
        }
    }

    #[test]
    fn commit_log_roundtrip(
        commits in vec(
            ("[0-9a-f]{40}", "[a-zA-Z ]{1,12}", "[a-z0-9.@]{1,12}", "[ -~&&[^\u{1e}\u{1f}]]{0,30}", "[ -~\n]{0,30}"),
            0..40
        )
    ) {
        let mut raw = Vec::new();
        for (i, (sha, author, email, subject, body)) in commits.iter().enumerate() {
            if i > 0 { raw.push(b'\n'); }
            raw.extend(format!(
                "{sha}\x1f\x1f{author}\x1f{email}\x1f2024-01-01T00:00:00+00:00\x1f{subject}\x1f{body}\x1f\x1e"
            ).bytes());
        }
        let parsed = parse_commit_log(&raw);
        prop_assert_eq!(parsed.len(), commits.len());
        for (p, (sha, author, _, subject, _)) in parsed.iter().zip(&commits) {
            prop_assert_eq!(&p.sha, sha);
            prop_assert_eq!(&p.author_name, author);
            prop_assert_eq!(&p.subject, subject);
        }
    }

    #[test]
    fn resolving_every_conflict_leaves_no_markers(
        chunks in vec(
            (line_text(), vec(line_text(), 0..4), vec(line_text(), 0..4), vec(line_text(), 0..4), any::<bool>()),
            0..5
        ),
        crlf in any::<bool>(),
        pick in 0usize..3,
    ) {
        let eol: &[u8] = if crlf { b"\r\n" } else { b"\n" };
        // El texto «normal» no puede parecer un marcador.
        let plain = |t: &[u8]| -> Vec<u8> {
            let mut v = b"x".to_vec();
            v.extend(t.iter().filter(|b| **b != b'\n'));
            v
        };
        let join = |ls: &[Vec<u8>]| -> Vec<u8> {
            ls.iter().flat_map(|l| plain(l).into_iter().chain(eol.iter().copied())).collect()
        };
        let (mut file, mut expected) = (Vec::new(), Vec::new());
        let choice = [ConflictChoice::Ours, ConflictChoice::Theirs, ConflictChoice::Base][pick];
        for (mid, ours, base, theirs, diff3) in &chunks {
            let keep = join(std::slice::from_ref(mid));
            file.extend(&keep);
            expected.extend(&keep);
            file.extend(b"<<<<<<< HEAD"); file.extend(eol);
            file.extend(join(ours));
            if *diff3 { file.extend(b"||||||| base"); file.extend(eol); file.extend(join(base)); }
            file.extend(b"======="); file.extend(eol);
            file.extend(join(theirs));
            file.extend(b">>>>>>> rama"); file.extend(eol);
            match choice {
                ConflictChoice::Ours => expected.extend(join(ours)),
                ConflictChoice::Theirs => expected.extend(join(theirs)),
                ConflictChoice::Base if *diff3 => expected.extend(join(base)),
                ConflictChoice::Base => {}
            }
        }
        let n_diff3_missing = chunks.iter().filter(|c| !c.4).count();
        prop_assert_eq!(conflict_hunks(&file).len(), chunks.len());
        prop_assert_eq!(has_conflict_markers(&file), !chunks.is_empty());
        match resolve_all(&file, choice) {
            Ok(out) => {
                prop_assert_eq!(out, expected);
            }
            Err(_) => {
                // Solo es legítimo fallar al pedir `Base` sin sección base.
                prop_assert!(choice == ConflictChoice::Base && n_diff3_missing > 0);
            }
        }
        if choice != ConflictChoice::Base {
            prop_assert!(!has_conflict_markers(&resolve_all(&file, choice).unwrap()));
        }
    }
}
