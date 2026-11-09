//! Parsers de listados: `git log`, `git for-each-ref`/ramas y `git remote -v`
//! (port de `CommitLogParser`, `BranchListParser` y `RemoteListParser` del Swift).
//!
//! Trabajan sobre bytes; los campos de texto se convierten con pérdida (nombres de rama,
//! asuntos y autores se muestran en la UI). Nunca entran en pánico.

use std::collections::BTreeMap;

use crate::models::{Branch, Commit, Remote};

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    bytes.trim_ascii()
}

/// Formato de `git log --pretty=format:` con separadores ASCII: `0x1f` entre campos y
/// `0x1e` entre commits (el cuerpo puede tener saltos de línea). Campos, en orden:
/// `sha, parents, autor, email, fecha ISO, asunto, cuerpo, refs`.
pub fn parse_commit_log(bytes: &[u8]) -> Vec<Commit> {
    bytes
        .split(|b| *b == 0x1e)
        .filter(|r| !r.is_empty())
        .filter_map(|record| {
            let fields: Vec<&[u8]> = record.split(|b| *b == 0x1f).collect();
            if fields.len() < 8 {
                return None;
            }
            let words = |f: &[u8]| -> Vec<String> {
                f.split(|b| *b == b' ')
                    .filter(|w| !w.is_empty())
                    .map(lossy)
                    .collect()
            };
            let refs = fields[7]
                .split(|b| *b == b',')
                .map(|r| lossy(trim_ascii(r)))
                .filter(|r| !r.is_empty())
                .collect();
            Some(Commit {
                // `--pretty=format:` separa commits con "\n": el hash de todos menos el primero lo trae delante.
                sha: lossy(trim_ascii(fields[0])),
                parents: words(fields[1]),
                subject: lossy(fields[5]),
                body: lossy(trim_ascii(fields[6])),
                author_name: lossy(fields[2]),
                author_email: lossy(fields[3]),
                author_date_iso: lossy(fields[4]),
                refs,
            })
        })
        .collect()
}

/// Ramas: una por línea, campos separados por NUL:
/// `nombre, oid, marcador HEAD (*), upstream, track (" [ahead 1, behind 2]")`.
pub fn parse_branches(bytes: &[u8]) -> Vec<Branch> {
    bytes
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .filter_map(|line| {
            let fields: Vec<&[u8]> = line.split(|b| *b == 0).collect();
            if fields.len() < 5 || fields[0].is_empty() {
                return None;
            }
            let (ahead, behind) = parse_track(&lossy(fields[4]));
            Some(Branch {
                name: lossy(fields[0]),
                oid: lossy(fields[1]),
                is_head: trim_ascii(fields[2]) == b"*",
                upstream: (!fields[3].is_empty()).then(|| lossy(fields[3])),
                ahead,
                behind,
            })
        })
        .collect()
}

/// `[ahead 2, behind 3]` → `(2, 3)`.
pub fn parse_track(raw: &str) -> (u32, u32) {
    let (mut ahead, mut behind) = (0, 0);
    let text = raw.replace(['[', ']'], "");
    for piece in text.split(',') {
        let bits: Vec<&str> = piece.split_whitespace().collect();
        if let [kind, value] = bits[..] {
            if let Ok(n) = value.parse::<u32>() {
                match kind {
                    "ahead" => ahead = n,
                    "behind" => behind = n,
                    _ => {}
                }
            }
        }
    }
    (ahead, behind)
}

/// Salida de `git remote -v`, ordenada por nombre.
pub fn parse_remotes(bytes: &[u8]) -> Vec<Remote> {
    let mut fetch: BTreeMap<String, String> = BTreeMap::new();
    let mut push: BTreeMap<String, String> = BTreeMap::new();
    for line in bytes.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        let line = lossy(line);
        let Some((name, rest)) = line.split_once('\t') else {
            continue;
        };
        if let Some(url) = rest.strip_suffix(" (fetch)") {
            fetch.insert(name.to_owned(), url.to_owned());
        } else if let Some(url) = rest.strip_suffix(" (push)") {
            push.insert(name.to_owned(), url.to_owned());
        }
    }
    fetch
        .iter()
        .map(|(name, url)| Remote {
            name: name.clone(),
            fetch_url: url.clone(),
            push_url: push.get(name).cloned().unwrap_or_else(|| url.clone()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn commit_log_keeps_multiline_body() {
        let raw = "abc\x1fparent\x1fAda\x1fada@example.com\x1f2026-01-02T03:04:05Z\x1fasunto\x1fcuerpo\nlinea 2\x1fHEAD -> main\x1e";
        let commits = parse_commit_log(raw.as_bytes());
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].subject, "asunto");
        assert_eq!(commits[0].body, "cuerpo\nlinea 2");
        assert_eq!(commits[0].parents, ["parent"]);
        assert_eq!(commits[0].refs, ["HEAD -> main"]);
        assert_eq!(commits[0].short_sha(), "abc");
    }

    #[test]
    fn commit_log_trims_separator_newline_from_hashes() {
        let one = "aaa\x1f\x1fAda\x1fada@example.com\x1f2026-01-02T03:04:05Z\x1funo\x1f\x1f\x1e";
        let two =
            "\nbbb\x1faaa\x1fAda\x1fada@example.com\x1f2026-01-01T03:04:05Z\x1fdos\x1f\x1f\x1e";
        let commits = parse_commit_log(format!("{one}{two}").as_bytes());
        assert_eq!(
            commits.iter().map(|c| c.sha.as_str()).collect::<Vec<_>>(),
            ["aaa", "bbb"]
        );
        assert_eq!(
            commits
                .iter()
                .map(|c| c.subject.as_str())
                .collect::<Vec<_>>(),
            ["uno", "dos"]
        );
        assert!(commits[0].parents.is_empty());
    }

    #[test]
    fn branch_track_and_list() {
        assert_eq!(parse_track(" [ahead 2, behind 3]"), (2, 3));
        assert_eq!(parse_track("[behind 4]"), (0, 4));
        assert_eq!(parse_track(""), (0, 0));
        let text = b"main\0abc\0*\0origin/main\0 [ahead 1]\nfeature\0def\0 \0\0\n";
        let branches = parse_branches(text);
        assert_eq!(branches.len(), 2);
        assert!(branches[0].is_head);
        assert_eq!(branches[0].upstream.as_deref(), Some("origin/main"));
        assert_eq!(branches[0].ahead, 1);
        assert!(!branches[1].is_head);
        assert_eq!(branches[1].upstream, None);
    }

    #[test]
    fn remotes_sorted_and_push_defaults_to_fetch() {
        let text = b"origin\t/tmp/bare (fetch)\norigin\t/tmp/bare (push)\nbackup\tgit@example:repo.git (fetch)\n";
        let remotes = parse_remotes(text);
        assert_eq!(
            remotes.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["backup", "origin"]
        );
        assert_eq!(remotes[1].fetch_url, "/tmp/bare");
        assert_eq!(remotes[1].push_url, "/tmp/bare");
        assert_eq!(remotes[0].push_url, "git@example:repo.git");
    }

    #[test]
    fn garbage_never_panics() {
        for s in [
            &b""[..],
            b"\x1e\x1e",
            b"\x1f\x1f",
            b"a\0b\n\0\0\0\0\n",
            b"\xff\xfe\t(fetch)",
            b"x\t (fetch)\n\t(push)",
        ] {
            let _ = parse_commit_log(s);
            let _ = parse_branches(s);
            let _ = parse_remotes(s);
        }
        let _ = parse_track("[ahead , behind x, ]");
        let _ = parse_track("[ahead 99999999999999999999]");
    }
}
