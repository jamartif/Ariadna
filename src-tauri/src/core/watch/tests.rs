//! Filtro de rutas y coalescedor (puros, sin disco) y vigilante real sobre carpetas temporales.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use super::*;

// --- filtro de rutas --------------------------------------------------------------------

fn filter() -> PathFilter {
    PathFilter::new(Path::new("/w/repo"), Path::new("/w/repo/.git"), None)
}

fn v(f: &PathFilter, p: &str) -> Verdict {
    f.classify(Path::new(p))
}

#[test]
fn el_worktree_dispara_y_el_ruido_no() {
    let f = filter();
    assert_eq!(v(&f, "/w/repo/src/main.rs"), Verdict::Worktree);
    assert_eq!(v(&f, "/w/repo/README.md"), Verdict::Worktree);
    assert_eq!(v(&f, "/w/repo/src"), Verdict::Worktree);
    for noise in [
        "/w/repo/node_modules/x/index.js",
        "/w/repo/ui/node_modules",
        "/w/repo/target/debug/ariadna",
        "/w/repo/ui/dist/app.js",
        "/w/repo/ui/.svelte-kit/output/a.js",
        "/w/repo/src/.main.rs.swp",
        "/w/repo/src/main.rs~",
        "/w/repo/src/.#main.rs",
        "/w/repo/.DS_Store",
        "/w/repo/sub/.git/HEAD",
        "/w/repo/tmp.tmp",
        "/w/repo/4913",
    ] {
        assert_eq!(v(&f, noise), Verdict::Ignore, "{noise}");
    }
}

#[test]
fn rutas_fuera_o_la_propia_raiz_se_ignoran() {
    let f = filter();
    assert_eq!(v(&f, "/w/repo"), Verdict::Ignore);
    assert_eq!(v(&f, "/w/otro/a.txt"), Verdict::Ignore);
    assert_eq!(v(&f, "/w/repo-hermano/a.txt"), Verdict::Ignore);
    assert_eq!(v(&f, "/w"), Verdict::Ignore);
}

#[test]
fn dentro_de_git_solo_cuenta_el_estado() {
    let f = filter();
    for state in [
        "/w/repo/.git/HEAD",
        "/w/repo/.git/index",
        "/w/repo/.git/MERGE_HEAD",
        "/w/repo/.git/CHERRY_PICK_HEAD",
        "/w/repo/.git/REBASE_HEAD",
        "/w/repo/.git/packed-refs",
        "/w/repo/.git/refs/heads/main",
        "/w/repo/.git/refs/remotes/origin/main",
        "/w/repo/.git/refs/tags/v1",
        "/w/repo/.git/rebase-merge/done",
    ] {
        assert_eq!(v(&f, state), Verdict::GitState, "{state}");
    }
    for noise in [
        "/w/repo/.git/objects/ab/cdef",
        "/w/repo/.git/objects",
        "/w/repo/.git/index.lock",
        "/w/repo/.git/HEAD.lock",
        "/w/repo/.git/refs/heads/main.lock",
        "/w/repo/.git/refs",
        "/w/repo/.git/logs/HEAD",
        "/w/repo/.git/hooks/pre-commit",
        "/w/repo/.git/FETCH_HEAD",
        "/w/repo/.git/COMMIT_EDITMSG",
        "/w/repo/.git/gc.log",
        "/w/repo/.git",
    ] {
        assert_eq!(v(&f, noise), Verdict::Ignore, "{noise}");
    }
}

#[test]
fn worktree_enlazado_usa_su_git_dir_y_el_comun() {
    let f = PathFilter::new(
        Path::new("/w/wt"),
        Path::new("/w/main/.git/worktrees/wt"),
        Some(Path::new("/w/main/.git")),
    );
    assert_eq!(v(&f, "/w/wt/a.txt"), Verdict::Worktree);
    assert_eq!(v(&f, "/w/main/.git/worktrees/wt/HEAD"), Verdict::GitState);
    assert_eq!(v(&f, "/w/main/.git/refs/heads/x"), Verdict::GitState);
    assert_eq!(v(&f, "/w/main/.git/objects/ab/cd"), Verdict::Ignore);
}

#[test]
fn carpetas_de_visual_studio_nunca_disparan() {
    let f = filter();
    assert_eq!(v(&f, "/w/repo/.vs/App/v17/.suo"), Verdict::Ignore);
}

#[test]
fn bin_obj_y_packages_solo_se_ignoran_si_git_los_ignora() {
    let tmp = tempfile::tempdir().unwrap();
    let work = dunce::canonicalize(tmp.path()).unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&work)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-q"]);
    std::fs::write(work.join(".gitignore"), "App/bin/\nApp/obj/\npackages/\n").unwrap();
    for d in [
        "App/bin/Debug",
        "App/obj",
        "packages/Newtonsoft",
        "bin",
        "tools/obj",
    ] {
        std::fs::create_dir_all(work.join(d)).unwrap();
    }
    let f = PathFilter::new(&work, &work.join(".git"), None)
        .with_ignore_probe(IgnoreProbe::new(PathBuf::from("git"), work.clone()));
    let c = |rel: &str| f.classify(&work.join(rel));

    assert_eq!(c("App/bin/Debug/App.dll"), Verdict::Ignore);
    assert_eq!(c("App/obj/project.assets.json"), Verdict::Ignore);
    assert_eq!(c("App/obj"), Verdict::Ignore);
    assert_eq!(c("packages/Newtonsoft/lib.dll"), Verdict::Ignore);
    // Versionadas (git no las ignora): se vigilan.
    assert_eq!(c("bin/deploy.sh"), Verdict::Worktree);
    assert_eq!(c("tools/obj/notes.md"), Verdict::Worktree);
    assert_eq!(c("App/Program.cs"), Verdict::Worktree);
    // Sin sonda (git desconocido) no se ignoran: mejor un refresco de más.
    assert_eq!(
        filter().classify(Path::new("/w/repo/App/obj/x")),
        Verdict::Worktree
    );
}

#[cfg(windows)]
#[test]
fn windows_ignora_mayusculas_y_acepta_prefijo_verbatim() {
    let f = PathFilter::new(
        Path::new(r"C:\Users\x\Repo"),
        Path::new(r"C:\Users\x\Repo\.git"),
        None,
    );
    assert_eq!(v(&f, r"c:\users\x\repo\SRC\a.rs"), Verdict::Worktree);
    assert_eq!(v(&f, r"\\?\C:\Users\x\Repo\src\a.rs"), Verdict::Worktree);
    assert_eq!(v(&f, r"C:\Users\x\Repo\.git\index"), Verdict::GitState);
    assert_eq!(
        v(&f, r"C:\Users\x\Repo\.git\objects\ab\cd"),
        Verdict::Ignore
    );
    assert_eq!(
        v(&f, r"C:\Users\x\Repo\node_modules\a\b.js"),
        Verdict::Ignore
    );
}

// --- coalescedor ------------------------------------------------------------------------

fn co() -> (Coalescer, Instant) {
    (
        Coalescer::new(Duration::from_millis(300), Duration::from_secs(2)),
        Instant::now(),
    )
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn sin_eventos_no_hay_plazo_ni_disparo() {
    let (mut c, t0) = co();
    assert!(!c.is_pending());
    assert_eq!(c.deadline(), None);
    assert!(!c.fire_if_due(t0 + Duration::from_secs(60)));
}

#[test]
fn un_evento_dispara_tras_el_silencio_y_una_sola_vez() {
    let (mut c, t0) = co();
    c.push(t0);
    assert!(!c.fire_if_due(t0 + ms(299)));
    assert!(c.fire_if_due(t0 + ms(300)));
    assert!(!c.is_pending());
    assert!(!c.fire_if_due(t0 + ms(900)), "no dispara dos veces");
}

#[test]
fn una_rafaga_se_funde_en_un_disparo() {
    let (mut c, t0) = co();
    for i in 0..20 {
        c.push(t0 + ms(i * 50));
    }
    // Último evento a 950 ms: toca a 1250 ms, no antes.
    assert!(!c.fire_if_due(t0 + ms(1249)));
    assert!(c.fire_if_due(t0 + ms(1250)));
    assert!(!c.is_pending());
}

#[test]
fn una_tormenta_continua_dispara_en_el_tope() {
    let (mut c, t0) = co();
    let mut fired_at = None;
    for i in 0..100u64 {
        let now = t0 + ms(i * 100);
        if c.fire_if_due(now) {
            fired_at = Some(i * 100);
            break;
        }
        c.push(now);
    }
    assert_eq!(
        fired_at,
        Some(2000),
        "el tope es 2 s desde el primer evento"
    );
}

#[test]
fn tras_disparar_una_nueva_rafaga_empieza_de_cero() {
    let (mut c, t0) = co();
    c.push(t0);
    assert!(c.fire_if_due(t0 + ms(300)));
    c.push(t0 + ms(5000));
    assert_eq!(c.deadline(), Some(t0 + ms(5300)));
}

// --- vigilante real ---------------------------------------------------------------------

fn fast() -> WatchTimings {
    WatchTimings {
        quiet: ms(100),
        max_wait: ms(600),
        poll_every: ms(100),
    }
}

#[derive(Clone, Default)]
struct Counter(Arc<AtomicUsize>);

impl Counter {
    fn get(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
    fn reset(&self) {
        self.0.store(0, Ordering::SeqCst);
    }
    fn trigger(&self) -> Trigger {
        let n = Arc::clone(&self.0);
        Arc::new(move || {
            let n = Arc::clone(&n);
            Box::pin(async move {
                n.fetch_add(1, Ordering::SeqCst);
            })
        })
    }
}

/// Espera (hasta `secs`) a que se cumpla la condición; sin sleeps fijos.
async fn until(secs: u64, what: &str, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        if cond() {
            return;
        }
        tokio::time::sleep(ms(20)).await;
    }
    panic!("tiempo agotado esperando: {what}");
}

/// Comprueba que algo NO ocurre durante `quiet` (única espera fija: inevitable en un negativo).
async fn stays(c: &Counter, expected: usize, quiet: Duration, what: &str) {
    tokio::time::sleep(quiet).await;
    assert_eq!(c.get(), expected, "{what}");
}

/// Repo mínimo (sin git instalado): `.git/{HEAD,refs,objects}` más un worktree.
struct Fake {
    _tmp: tempfile::TempDir,
    work: PathBuf,
    git: PathBuf,
}

impl Fake {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let work = dunce::canonicalize(tmp.path()).unwrap().join("repo");
        let git = work.join(".git");
        std::fs::create_dir_all(git.join("refs/heads")).unwrap();
        std::fs::create_dir_all(git.join("objects/ab")).unwrap();
        std::fs::create_dir_all(work.join("node_modules/pkg")).unwrap();
        std::fs::create_dir_all(work.join("src")).unwrap();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(work.join("src/main.rs"), "fn main() {}\n").unwrap();
        Self {
            _tmp: tmp,
            work,
            git,
        }
    }

    fn start(&self, c: &Counter) -> TabWatcher {
        let w = TabWatcher::start(
            WatchConfig {
                worktree: self.work.clone(),
                git_dir: self.git.clone(),
                timings: fast(),
                git: None,
            },
            c.trigger(),
            Arc::new(AtomicBool::new(true)),
            Arc::new(|_| {}),
        );
        assert!(
            !w.is_polling(),
            "en un tempdir local debe vigilar de forma nativa"
        );
        // FSEvents (macOS) reentrega eventos de justo antes de arrancar el flujo, es decir, la
        // creación del propio repo de prueba. Se espera a que se asienten (más que el
        // debounce de `fast()`) y se parte de cero: solo interesa lo que ocurra después.
        if cfg!(target_os = "macos") {
            std::thread::sleep(Duration::from_millis(1500));
            c.reset();
        }
        w
    }
}

#[tokio::test]
async fn crear_un_archivo_dispara_exactamente_un_refresco() {
    let fx = Fake::new();
    let c = Counter::default();
    let _w = fx.start(&c);
    std::fs::write(fx.work.join("nuevo.txt"), "hola\n").unwrap();
    until(10, "refresco tras crear", || c.get() >= 1).await;
    stays(&c, 1, ms(700), "una creación = un solo disparo").await;
}

#[tokio::test]
async fn editar_y_borrar_disparan_cada_uno_su_refresco() {
    let fx = Fake::new();
    let c = Counter::default();
    let _w = fx.start(&c);
    let file = fx.work.join("src/main.rs");
    std::fs::write(&file, "fn main() { println!(); }\n").unwrap();
    until(10, "refresco tras editar", || c.get() == 1).await;
    stays(&c, 1, ms(400), "edición").await;
    std::fs::remove_file(&file).unwrap();
    until(10, "refresco tras borrar", || c.get() == 2).await;
    stays(&c, 2, ms(400), "borrado").await;
}

#[tokio::test]
async fn el_ruido_no_dispara_y_el_estado_de_git_si() {
    let fx = Fake::new();
    let c = Counter::default();
    let _w = fx.start(&c);

    std::fs::write(fx.git.join("objects/ab/cdef"), b"x").unwrap();
    std::fs::write(fx.git.join("index.lock"), b"").unwrap();
    std::fs::write(fx.git.join("refs/heads/main.lock"), b"").unwrap();
    std::fs::write(fx.work.join("node_modules/pkg/index.js"), b"x").unwrap();
    std::fs::write(fx.work.join("src/.main.rs.swp"), b"x").unwrap();
    stays(&c, 0, ms(700), "ruido").await;

    // Como hace git: escribe `index.lock` y lo renombra a `index`.
    std::fs::rename(fx.git.join("index.lock"), fx.git.join("index")).unwrap();
    until(10, "refresco tras tocar .git/index", || c.get() == 1).await;
    stays(&c, 1, ms(400), "index").await;

    std::fs::write(fx.git.join("refs/heads/rama"), "0".repeat(40)).unwrap();
    until(10, "refresco tras crear una rama", || c.get() == 2).await;
}

#[tokio::test]
async fn una_carpeta_nueva_pasa_a_vigilarse() {
    let fx = Fake::new();
    let c = Counter::default();
    let _w = fx.start(&c);
    std::fs::create_dir_all(fx.work.join("src/nuevo/hondo")).unwrap();
    std::fs::write(fx.work.join("src/nuevo/hondo/a.txt"), "1").unwrap();
    until(10, "refresco por la carpeta nueva", || c.get() >= 1).await;
    let before = c.get();
    // Un archivo posterior en la carpeta recién creada también se ve.
    tokio::time::sleep(ms(300)).await;
    std::fs::write(fx.work.join("src/nuevo/hondo/b.txt"), "2").unwrap();
    until(10, "refresco por el archivo en la carpeta nueva", || {
        c.get() > before
    })
    .await;
}

#[tokio::test]
async fn soltar_el_vigilante_lo_para() {
    let fx = Fake::new();
    let c = Counter::default();
    let w = fx.start(&c);
    std::fs::write(fx.work.join("a.txt"), "1").unwrap();
    until(10, "primer refresco", || c.get() == 1).await;
    drop(w);
    std::fs::write(fx.work.join("b.txt"), "2").unwrap();
    std::fs::write(fx.git.join("HEAD"), "ref: refs/heads/otra\n").unwrap();
    stays(&c, 1, ms(700), "tras soltarlo no hay más disparos").await;
}

#[tokio::test]
async fn si_no_puede_vigilar_sondea_solo_con_foco_y_avisa_una_vez() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("no-existe");
    let c = Counter::default();
    let focused = Arc::new(AtomicBool::new(true));
    let notes: Arc<Mutex<Vec<String>>> = Arc::default();
    let n2 = Arc::clone(&notes);
    let w = TabWatcher::start(
        WatchConfig {
            worktree: missing.clone(),
            git_dir: missing.join(".git"),
            timings: fast(),
            git: None,
        },
        c.trigger(),
        Arc::clone(&focused),
        Arc::new(move |r| n2.lock().unwrap().push(r)),
    );
    assert!(w.is_polling());
    assert_eq!(notes.lock().unwrap().len(), 1, "aviso único");

    until(10, "sondeo con foco", || c.get() >= 3).await;

    // Sin foco, el sondeo se detiene (tras dejar terminar el ciclo en vuelo).
    focused.store(false, Ordering::SeqCst);
    tokio::time::sleep(ms(300)).await;
    let frozen = c.get();
    stays(&c, frozen, ms(500), "sin foco no se sondea").await;

    // Al recuperar el foco vuelve a sondear.
    focused.store(true, Ordering::SeqCst);
    until(10, "sondeo al recuperar el foco", || c.get() > frozen).await;
    assert_eq!(notes.lock().unwrap().len(), 1);
}
