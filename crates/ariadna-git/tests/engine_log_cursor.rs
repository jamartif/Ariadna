//! Paginación del historial por cursor (frontera) frente a `--skip`: mismo orden y contenido
//! en historiales con ramas, merges, octopus y fechas repetidas.
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::HashSet;
use std::io::Write;
use std::process::{Command, Stdio};

use ariadna_git::cursor::append_new;
use ariadna_git::{history_cursor, Commit, GitError};
use common::{git_path, TestRepo};

/// Generador congruencial: reproducible sin dependencias.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % n
    }
}

/// Historial sintético con `fast-import`: cada commit cuelga de uno reciente; el 20 % es un merge
/// con otro commit; las fechas no decrecen pero se repiten a menudo (desempate de git). El
/// último commit fusiona (octopus) todas las puntas para que todo sea alcanzable desde HEAD.
fn build_graph(t: &TestRepo, commits: usize, seed: u64, tie_percent: u64) {
    let mut rng = Lcg(seed);
    let mut stream = Vec::new();
    let mut has_child: Vec<bool> = Vec::new();
    let mut time = 1_700_000_000_u64;
    for i in 1..=commits {
        let mut ps: Vec<usize> = Vec::new();
        if i > 1 {
            let window = 12.min(i - 1) as u64;
            ps.push(i - 1 - rng.next(window) as usize);
            if rng.next(100) < 20 {
                let other = 1 + rng.next((i - 1) as u64) as usize;
                if !ps.contains(&other) {
                    ps.push(other);
                }
            }
        }
        if rng.next(100) >= tie_percent {
            time += 1 + rng.next(3);
        }
        for p in &ps {
            has_child[*p - 1] = true;
        }
        has_child.push(false);
        write_commit(&mut stream, i, time, &ps);
    }
    // Octopus final con todas las puntas (git acepta muchos padres).
    let tips: Vec<usize> = (1..=commits).filter(|i| !has_child[*i - 1]).collect();
    time += 1;
    write_commit(&mut stream, commits + 1, time, &tips);
    stream.extend(b"done\n");
    let mut child = Command::new(git_path())
        .args(["fast-import", "--quiet", "--done"])
        .current_dir(t.path())
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&stream).unwrap();
    assert!(child.wait().unwrap().success());
    t.git(&["reset", "-q", "--hard", "main"]);
}

fn write_commit(stream: &mut Vec<u8>, mark: usize, time: u64, parents: &[usize]) {
    let msg = format!("commit {mark}");
    write!(
        stream,
        "commit refs/heads/main\nmark :{mark}\ncommitter Test <t@example.invalid> {time} +0000\ndata {}\n{msg}\n",
        msg.len()
    )
    .unwrap();
    if let Some((first, rest)) = parents.split_first() {
        writeln!(stream, "from :{first}").unwrap();
        for p in rest {
            writeln!(stream, "merge :{p}").unwrap();
        }
    }
    let body = format!("l{mark}\n");
    write!(
        stream,
        "M 100644 inline f{}.txt\ndata {}\n{body}\n",
        mark % 7,
        body.len()
    )
    .unwrap();
}

async fn by_skip(t: &TestRepo, size: usize) -> Vec<Commit> {
    let repo = t.engine.open_repository(t.path()).await.unwrap();
    let mut all = Vec::new();
    loop {
        let page = t.engine.log_page(&repo, all.len(), size).await.unwrap();
        if page.is_empty() {
            return all;
        }
        all.extend(page);
    }
}

async fn by_cursor(t: &TestRepo, size: usize) -> Vec<Commit> {
    let repo = t.engine.open_repository(t.path()).await.unwrap();
    let mut all = t.engine.log_page(&repo, 0, size).await.unwrap();
    loop {
        let cursor = history_cursor(&all);
        if cursor.is_empty() {
            return all;
        }
        let page = t.engine.log_after(&repo, &cursor, size).await.unwrap();
        if page.is_empty() {
            return all;
        }
        // Lo que ya se mostró (fechas iguales) se descarta; una página de puros repetidos se
        // pide por `--skip`, que no tiene este problema.
        if append_new(&mut all, page) == 0 {
            let page = t.engine.log_page(&repo, all.len(), size).await.unwrap();
            if append_new(&mut all, page) == 0 {
                return all;
            }
        }
    }
}

fn shas(c: &[Commit]) -> Vec<&str> {
    c.iter().map(|c| c.sha.as_str()).collect()
}

/// Primera posición en que difieren dos listas (para un mensaje de fallo legible).
fn first_diff(a: &[Commit], b: &[Commit]) -> String {
    match a.iter().zip(b).position(|(x, y)| x.sha != y.sha) {
        Some(i) => format!(
            "primera diferencia en {i}: «{}» (padres {:?}) vs «{}» (padres {:?})",
            a[i].subject, a[i].parents, b[i].subject, b[i].parents
        ),
        None => format!("misma base, longitudes {} y {}", a.len(), b.len()),
    }
}

#[tokio::test]
async fn cursor_pages_equal_skip_pages_with_merges_and_ties() {
    // (commits, semilla, % de fechas repetidas): sin empates, algunos y casi todos.
    for (n, seed, ties) in [(250, 1, 0), (250, 2, 40), (180, 3, 90), (120, 4, 100)] {
        let t = TestRepo::init();
        build_graph(&t, n, seed, ties);
        let repo = t.engine.open_repository(t.path()).await.unwrap();
        let full = t.engine.log(&repo, 5_000).await.unwrap();
        assert!(full.len() > n, "todo debía ser alcanzable: {}", full.len());
        assert!(
            full.iter().any(|c| c.parents.len() > 1),
            "el grafo debe tener merges"
        );
        for size in [1, 2, 3, 7, 25, 100, 1_000] {
            let skip = by_skip(&t, size).await;
            let cur = by_cursor(&t, size).await;
            assert!(
                shas(&skip) == shas(&full),
                "skip≠log n={n} seed={seed} ties={ties} size={size}: {}",
                first_diff(&skip, &full)
            );
            if shas(&cur) != shas(&full) {
                let i = cur
                    .iter()
                    .zip(&full)
                    .position(|(x, y)| x.sha != y.sha)
                    .unwrap_or(0);
                let start = (i / size) * size;
                let fr = history_cursor(&full[..start]);
                let pos = |sha: &str| fr.iter().position(|s| s == sha);
                let front: Vec<String> = vec![
                    format!(
                        "mío {} @{} pos {:?}; real {} @{} pos {:?}; len {}",
                        cur[i].subject,
                        cur[i].author_date_iso,
                        pos(&cur[i].sha),
                        full[i].subject,
                        full[i].author_date_iso,
                        pos(&full[i].sha),
                        fr.len()
                    ),
                    format!("padres del primero: {:?}", full[0].parents.len()),
                ];
                panic!(
                    "cursor≠log n={n} seed={seed} ties={ties} size={size}: {}\nfrontera en {start}: {front:?}\nlog real desde ahí: {:?}",
                    first_diff(&cur, &full),
                    full[start..(start + 6).min(full.len())]
                        .iter()
                        .map(|c| format!("{} @{}", c.subject, c.author_date_iso))
                        .collect::<Vec<_>>()
                );
            }
            // El contenido (no solo el hash) también coincide.
            assert_eq!(cur, full, "contenido n={n} seed={seed} size={size}");
            let unique: HashSet<&str> = shas(&cur).into_iter().collect();
            assert_eq!(unique.len(), cur.len(), "sin repetidos");
        }
    }
}

#[tokio::test]
async fn linear_history_cursor_is_the_parent_of_the_last_commit() {
    let t = TestRepo::with_initial_commit();
    for i in 0..9 {
        t.write("f.txt", format!("{i}\n").as_bytes());
        t.commit_all(&format!("c{i}"));
    }
    let repo = t.engine.open_repository(t.path()).await.unwrap();
    let first = t.engine.log_page(&repo, 0, 4).await.unwrap();
    let cursor = history_cursor(&first);
    assert_eq!(cursor, first.last().unwrap().parents);
    let next = t.engine.log_after(&repo, &cursor, 4).await.unwrap();
    assert_eq!(
        shas(&next),
        shas(&t.engine.log_page(&repo, 4, 4).await.unwrap())
    );
    // Tras el commit raíz no queda historial.
    let all = t.engine.log_page(&repo, 0, 100).await.unwrap();
    assert!(history_cursor(&all).is_empty());
    assert!(t.engine.log_after(&repo, &[], 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_bad_cursor_is_rejected_before_reaching_git() {
    let t = TestRepo::with_initial_commit();
    let repo = t.engine.open_repository(t.path()).await.unwrap();
    for bad in ["-rf", "HEAD", "abc1234", "zz", "--all", &"g".repeat(40), ""] {
        let err = t
            .engine
            .log_after(&repo, &[bad.to_owned()], 10)
            .await
            .unwrap_err();
        assert!(
            matches!(err, GitError::InvalidReference(_)),
            "{bad:?}: {err:?}"
        );
    }
    // Un SHA con forma válida pero que no existe es un error de git, no un historial vacío.
    let err = t
        .engine
        .log_after(&repo, &["1".repeat(40)], 10)
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
    // Demasiados commits en la frontera.
    let many = vec!["a".repeat(40); 1_001];
    assert!(matches!(
        t.engine.log_after(&repo, &many, 10).await.unwrap_err(),
        GitError::InvalidReference(_)
    ));
}

#[tokio::test]
async fn empty_repo_and_exact_size_page_have_no_next_page() {
    // Repo sin commits: la primera página puede ser vacía o error de git, nunca un cursor.
    let t = TestRepo::init();
    let repo = t.engine.open_repository(t.path()).await.unwrap();
    if let Ok(page) = t.engine.log_page(&repo, 0, 10).await {
        assert!(page.is_empty() && history_cursor(&page).is_empty());
    }
    // Historial con ramas y un merge; página del tamaño exacto del total.
    let t = TestRepo::with_initial_commit();
    let base = t
        .git(&["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_owned();
    t.git(&["checkout", "-q", "-b", "feat"]);
    t.write("a.txt", b"a\n");
    t.commit_all("feat 1");
    t.git(&["checkout", "-q", "-b", "other", &base]);
    t.write("b.txt", b"b\n");
    t.commit_all("other 1");
    t.git(&["checkout", "-q", &base]);
    t.write("c.txt", b"c\n");
    t.commit_all("main 1");
    t.git(&["merge", "-q", "--no-ff", "-m", "merge feat", "feat"]);
    t.git(&["merge", "-q", "--no-ff", "-m", "merge other", "other"]);
    let repo = t.engine.open_repository(t.path()).await.unwrap();
    let full = t.engine.log(&repo, 5_000).await.unwrap();
    let total = full.len();
    assert!(full.iter().filter(|c| c.parents.len() > 1).count() >= 2);
    // Página exacta: no queda frontera ni más historial.
    let exact = t.engine.log_page(&repo, 0, total).await.unwrap();
    assert_eq!(exact, full);
    assert!(history_cursor(&exact).is_empty());
    assert!(t
        .engine
        .log_page(&repo, total, total)
        .await
        .unwrap()
        .is_empty());
    // Concatenar todas las páginas (cualquier tamaño) == `git log` completo, mismo orden.
    for size in [1, 2, total - 1, total, total + 1] {
        assert_eq!(by_skip(&t, size).await, full, "skip size={size}");
        assert_eq!(by_cursor(&t, size).await, full, "cursor size={size}");
    }
    // Y coincide con la salida cruda de git.
    let raw = t.git(&["log", "--format=%H"]);
    let want: Vec<&str> = raw.lines().collect();
    assert_eq!(shas(&full), want);
}
