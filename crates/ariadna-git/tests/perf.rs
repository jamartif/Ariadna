//! Pruebas de rendimiento contra NF-3, NF-4 y NF-5 (M2.12 / M7.2) con repos sintéticos.
//!
//! Están marcadas `#[ignore]`: no corren con `cargo test`. Ejecútalas en release:
//!
//! ```text
//! cargo test -p ariadna-git --release --test perf -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Cada prueba imprime líneas `PERF <nombre> <valor> <unidad>` que se copian a
//! `specs/rust/rendimiento.md`. Los umbrales de NF-3 se afirman con `assert!`.
//! `ARIADNA_PERF_FILES` (por defecto 50 000) y `ARIADNA_PERF_COMMITS` (10 000) cambian el
//! tamaño de los repos sintéticos; `ARIADNA_PERF_COMMIT_GRAPH=1` escribe el commit-graph.
#![allow(clippy::unwrap_used)]

mod common;

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ariadna_git::{pathspec_stdin, DiffScope, GitPath};
use common::{git_path, TestRepo};

fn files_n() -> usize {
    std::env::var("ARIADNA_PERF_FILES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50_000)
}

fn report(name: &str, d: Duration) {
    eprintln!("PERF {name} {:.0} ms", d.as_secs_f64() * 1000.0);
}

/// Mediana de `n` ejecuciones.
async fn median<F, Fut>(n: usize, mut f: F) -> Duration
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut times = Vec::new();
    for _ in 0..n {
        let t = Instant::now();
        f().await;
        times.push(t.elapsed());
    }
    times.sort();
    times[times.len() / 2]
}

fn rel(i: usize) -> String {
    format!(
        "src/m{:03}/sub{:02}/archivo con espacio {i}.txt",
        i % 500,
        i % 20
    )
}

fn populate(t: &TestRepo, n: usize) -> Vec<GitPath> {
    (0..n)
        .map(|i| {
            let r = rel(i);
            t.write(&r, format!("contenido {i}\n").repeat(3).as_bytes());
            GitPath::from(r)
        })
        .collect()
}

/// `git fast-import` con `commits` commits lineales, cada uno toca un archivo.
fn fast_import_history(t: &TestRepo, commits: usize) {
    let mut stream = Vec::with_capacity(commits * 140);
    for i in 1..=commits {
        let body = format!("linea {i}\n");
        write!(
            stream,
            "commit refs/heads/main\nmark :{i}\ncommitter Test <test@example.invalid> {} +0000\ndata {}\ncommit número {i}\n",
            1_700_000_000 + i,
            "commit número ".len() + i.to_string().len() + 1
        )
        .unwrap();
        if i > 1 {
            writeln!(stream, "from :{}", i - 1).unwrap();
        }
        write!(
            stream,
            "M 100644 inline f{}.txt\ndata {}\n{body}\n",
            i % 100,
            body.len()
        )
        .unwrap();
    }
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

#[tokio::test]
#[ignore = "rendimiento: cargo test --release --test perf -- --ignored"]
async fn nf3_status_untracked_50k() {
    let n = files_n();
    let t = TestRepo::with_initial_commit();
    let repo = t.engine.open_repository(t.path()).await.unwrap();
    let t0 = Instant::now();
    populate(&t, n);
    eprintln!(
        "PERF crear_{n}_archivos {:.0} ms",
        t0.elapsed().as_secs_f64() * 1000.0
    );

    // Primera lectura (frío para la caché del SO) y mediana de 5 más.
    let t1 = Instant::now();
    let st = t.engine.status(&repo, false).await.unwrap();
    report("status_sin_seguimiento_primera", t1.elapsed());
    assert_eq!(st.entries.len(), n);
    let d = median(5, || async {
        let st = t.engine.status(&repo, false).await.unwrap();
        assert_eq!(st.entries.len(), n);
    })
    .await;
    report("status_sin_seguimiento_mediana", d);
    assert!(
        d < Duration::from_secs(2),
        "NF-3: status de {n} archivos sin seguimiento tardó {d:?} (> 2 s)"
    );
}

#[tokio::test]
#[ignore = "rendimiento: cargo test --release --test perf -- --ignored"]
async fn nf3_status_modified_50k_and_stage_via_stdin() {
    let n = files_n();
    let t = TestRepo::with_initial_commit();
    let repo = t.engine.open_repository(t.path()).await.unwrap();
    let paths = populate(&t, n);

    // Tamaño del cuerpo por stdin frente al límite de argv (ARG_MAX ≈ 2 MiB en Linux,
    // 32 767 caracteres en Windows): demuestra que sin stdin no cabría.
    let body = pathspec_stdin(paths.iter().map(GitPath::as_bytes)).unwrap();
    eprintln!("PERF stdin_bytes_{n}_rutas {} bytes", body.len());
    let argv_total: usize = paths.iter().map(|p| p.as_bytes().len() + 1).sum();
    eprintln!("PERF argv_equivalente {argv_total} bytes");

    let t1 = Instant::now();
    t.engine.stage(&repo, &paths).await.unwrap();
    report("stage_50k_rutas_por_stdin", t1.elapsed());
    let t1 = Instant::now();
    let st = t.engine.status(&repo, false).await.unwrap();
    report("status_50k_preparados", t1.elapsed());
    assert_eq!(st.staged().len(), n);

    let t1 = Instant::now();
    t.engine.commit(&repo, "masivo", false).await.unwrap();
    report("commit_50k_archivos", t1.elapsed());

    // 50 000 archivos rastreados y modificados.
    for (i, p) in paths.iter().enumerate() {
        t.write(&p.to_string_lossy(), format!("cambio {i}\n").as_bytes());
    }
    let t1 = Instant::now();
    let st = t.engine.status(&repo, false).await.unwrap();
    report("status_modificados_primera", t1.elapsed());
    assert_eq!(st.unstaged().len(), n);
    let d = median(5, || async {
        let st = t.engine.status(&repo, false).await.unwrap();
        assert_eq!(st.unstaged().len(), n);
    })
    .await;
    report("status_modificados_mediana", d);
    assert!(
        d < Duration::from_secs(2),
        "NF-3: status de {n} archivos modificados tardó {d:?} (> 2 s)"
    );

    let t1 = Instant::now();
    t.engine.stage(&repo, &paths).await.unwrap();
    report("stage_modificados_por_stdin", t1.elapsed());
    let t1 = Instant::now();
    t.engine.unstage(&repo, &paths).await.unwrap();
    report("unstage_50k_por_stdin", t1.elapsed());
    assert_eq!(
        t.engine
            .status(&repo, false)
            .await
            .unwrap()
            .unstaged()
            .len(),
        n
    );

    // Diff de todo el árbol (50 000 archivos, un hunk cada uno).
    let t1 = Instant::now();
    let diffs = t
        .engine
        .diff(&repo, &DiffScope::Unstaged, &[])
        .await
        .unwrap();
    report("diff_50k_archivos_sin_rutas", t1.elapsed());
    assert_eq!(diffs.len(), n);

    // `diff` con 50 000 rutas: no cabe en argv, así que el motor pide el diff completo y
    // filtra en memoria.
    let t1 = Instant::now();
    let r = t
        .engine
        .diff(&repo, &DiffScope::Unstaged, &paths)
        .await
        .unwrap();
    report("diff_50k_rutas_filtrado_en_memoria", t1.elapsed());
    assert_eq!(r.len(), n);
}

#[tokio::test]
#[ignore = "rendimiento: cargo test --release --test perf -- --ignored"]
async fn nf4_log_ten_thousand_commits_paginated() {
    let total: usize = std::env::var("ARIADNA_PERF_COMMITS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);
    let t = TestRepo::init();
    let repo = t.engine.open_repository(t.path()).await.unwrap();
    let t0 = Instant::now();
    fast_import_history(&t, total);
    eprintln!(
        "PERF crear_{total}_commits {:.0} ms",
        t0.elapsed().as_secs_f64() * 1000.0
    );

    if std::env::var("ARIADNA_PERF_COMMIT_GRAPH").is_ok() {
        t.git(&["commit-graph", "write", "--reachable"]);
        eprintln!("PERF commit_graph activo");
    }

    // Una página de 200 en distintas profundidades: `git log --skip` es O(skip).
    for pct in [0, 25, 50, 75, 100] {
        let skip = (total - 200) * pct / 100;
        let d = median(5, || async {
            let page = t.engine.log_page(&repo, skip, 200).await.unwrap();
            assert_eq!(page.len(), 200);
        })
        .await;
        report(&format!("log_pagina_200_skip_{skip}"), d);
        if pct == 0 {
            assert!(d < Duration::from_millis(100), "primera página: {d:?}");
        }
    }

    // La misma página por cursor (`git log <frontera>`, sin `--skip`): coste constante. En un
    // historial lineal la frontera es el padre del último commit de lo ya cargado.
    for pct in [0, 25, 50, 75, 100] {
        let skip = (total - 200) * pct / 100;
        let expected = t.engine.log_page(&repo, skip, 200).await.unwrap();
        let cursor = if skip == 0 {
            vec![expected[0].sha.clone()]
        } else {
            t.engine
                .log_page(&repo, skip - 1, 1)
                .await
                .unwrap()
                .remove(0)
                .parents
        };
        let d = median(5, || async {
            let page = t.engine.log_after(&repo, &cursor, 200).await.unwrap();
            assert_eq!(page.len(), 200);
        })
        .await;
        assert_eq!(
            t.engine.log_after(&repo, &cursor, 200).await.unwrap(),
            expected,
            "el cursor debe dar la misma página que skip {skip}"
        );
        report(&format!("log_pagina_200_cursor_prof_{skip}"), d);
        assert!(
            d < Duration::from_millis(100),
            "página por cursor a {skip}: {d:?}"
        );
    }

    // Recorrido completo por cursor, de 200 en 200, en cualquier tamaño (lineal por construcción).
    // Solo se suma el tiempo de git (`log_after`), no el de calcular la frontera en el test.
    {
        let mut git_time = Duration::ZERO;
        let mut shown = t.engine.log_page(&repo, 0, 200).await.unwrap();
        let mut slowest = Duration::ZERO;
        loop {
            let cursor = ariadna_git::history_cursor(&shown);
            if cursor.is_empty() {
                break;
            }
            let p = Instant::now();
            let page = t.engine.log_after(&repo, &cursor, 200).await.unwrap();
            slowest = slowest.max(p.elapsed());
            git_time += p.elapsed();
            if ariadna_git::cursor::append_new(&mut shown, page) == 0 {
                break;
            }
        }
        assert_eq!(shown.len(), total);
        report("log_recorrido_completo_cursor_solo_git", git_time);
        report("log_pagina_mas_lenta_cursor", slowest);
        assert!(
            slowest < Duration::from_millis(100),
            "página por cursor: {slowest:?}"
        );
    }

    // Recorrido completo, sin repetidos ni huecos (cuadrático: solo hasta 20 000 commits).
    if total <= 20_000 {
        let t1 = Instant::now();
        let mut seen = std::collections::HashSet::new();
        let mut skip = 0;
        let mut slowest = Duration::ZERO;
        loop {
            let p = Instant::now();
            let page = t.engine.log_page(&repo, skip, 200).await.unwrap();
            slowest = slowest.max(p.elapsed());
            if page.is_empty() {
                break;
            }
            for c in &page {
                assert!(seen.insert(c.sha.clone()), "commit repetido entre páginas");
            }
            skip += page.len();
        }
        assert_eq!(seen.len(), total);
        report("log_recorrido_completo", t1.elapsed());
        report("log_pagina_mas_lenta", slowest);
        assert!(
            slowest < Duration::from_millis(500),
            "una página de 200 commits tardó {slowest:?}"
        );
    }

    let t1 = Instant::now();
    let found = t
        .engine
        .search_log(&repo, "número 9999", 500)
        .await
        .unwrap();
    report("search_log_texto", t1.elapsed());
    assert!(!found.is_empty());
    let t1 = Instant::now();
    let found = t
        .engine
        .search_log(&repo, "test@example", 500)
        .await
        .unwrap();
    report("search_log_autor", t1.elapsed());
    assert_eq!(found.len(), 500);
}

#[tokio::test]
#[ignore = "rendimiento: cargo test --release --test perf -- --ignored"]
async fn nf5_large_diffs() {
    let t = TestRepo::with_initial_commit();
    let repo = t.engine.open_repository(t.path()).await.unwrap();

    // A: archivo de 200 000 líneas (~ 2,6 MB) con todas las líneas cambiadas.
    let old: String = (0..200_000)
        .map(|i| format!("linea numero {i}\n"))
        .collect();
    let new: String = (0..200_000)
        .map(|i| format!("LINEA CAMBIADA {i}\n"))
        .collect();
    t.write("grande.txt", old.as_bytes());
    // B: una sola línea de 2 MB.
    t.write("larga.txt", "x".repeat(2_000_000).as_bytes());
    // C: binario de 5 MB.
    t.write("datos.bin", &vec![0u8; 5_000_000]);
    t.commit_all("base");
    t.write("grande.txt", new.as_bytes());
    t.write("larga.txt", "y".repeat(2_000_000).as_bytes());
    let mut bin = vec![0u8; 5_000_000];
    bin[100] = 1;
    t.write("datos.bin", &bin);

    let t1 = Instant::now();
    let files = t
        .engine
        .diff(&repo, &DiffScope::Unstaged, &[])
        .await
        .unwrap();
    report("diff_grande_3_archivos", t1.elapsed());
    let find = |n: &str| files.iter().find(|f| f.path == *n).unwrap();
    let big = find("grande.txt");
    let lines: usize = big.hunks.iter().map(|h| h.lines.len()).sum();
    eprintln!("PERF diff_grande_lineas {lines} lineas");
    assert_eq!(lines, 400_000);
    assert!(find("datos.bin").is_binary);
    let long = find("larga.txt");
    let longest = long
        .hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .map(|l| l.text.len())
        .max()
        .unwrap();
    eprintln!("PERF linea_mas_larga_devuelta {longest} bytes");

    // Solo el archivo grande, medido varias veces.
    let d = median(3, || async {
        let f = t
            .engine
            .diff(&repo, &DiffScope::Unstaged, &[GitPath::from("grande.txt")])
            .await
            .unwrap();
        assert_eq!(f.len(), 1);
    })
    .await;
    report("diff_archivo_400k_lineas_mediana", d);

    // Serialización a JSON de lo que cruzaría el IPC.
    let t1 = Instant::now();
    let json = serde_json::to_vec(big).unwrap();
    report("serializar_diff_400k_lineas", t1.elapsed());
    eprintln!("PERF json_diff_400k_lineas {} bytes", json.len());

    // Por encima del tope de salida del runner (64 MiB).
    t.write("enorme.txt", "z".repeat(1_000).as_bytes());
    t.commit_all("enorme base");
    let huge: String = (0..1_400_000)
        .map(|i| format!("fila {i} {}\n", "q".repeat(60)))
        .collect();
    t.write("enorme.txt", huge.as_bytes());
    let t1 = Instant::now();
    let r = t
        .engine
        .diff(&repo, &DiffScope::Unstaged, &[GitPath::from("enorme.txt")])
        .await;
    report("diff_sobre_tope_64MiB", t1.elapsed());
    // Antes devolvía Ok con el diff cortado a media línea; ahora es un error claro.
    eprintln!(
        "PERF diff_sobre_tope_resultado {}",
        match &r {
            Ok(v) => format!("ok({} archivos)", v.len()),
            Err(e) => format!(
                "error: {}",
                e.to_string().chars().take(120).collect::<String>()
            ),
        }
    );
    assert!(
        r.is_err(),
        "un diff de más de 64 MiB no puede devolverse truncado"
    );
}
