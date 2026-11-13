//! Clonado desde URL contra git de verdad (remoto bare local, `file://` y HTTP simulado).
#![allow(clippy::unwrap_used)]
mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ariadna_git::{
    CloneAuth, ClonePhase, CloneProgress, CloneRequest, CloneService, GitCliEngine, GitError,
    GitRunner, Secret,
};
use ariadna_process::CancellationToken;
use common::http::{HttpServer, Mode};
use common::{git_path, plain_engine, TestRepo};
use tempfile::TempDir;

/// Remoto bare con varias ramas y bastantes objetos para que git emita progreso.
struct Remote {
    _origin: TestRepo,
    bare: TempDir,
}

impl Remote {
    fn new() -> Self {
        let r = TestRepo::with_initial_commit();
        for i in 0..40 {
            r.write(
                &format!("src/f{i}.txt"),
                format!("contenido {i}\n").repeat(50).as_bytes(),
            );
            r.commit_all(&format!("commit {i}"));
        }
        r.git(&["checkout", "-q", "-b", "feature/x"]);
        r.write("solo-feature.txt", b"hola\n");
        r.commit_all("en feature");
        r.git(&["checkout", "-q", "main"]);
        let bare = r.with_bare_origin();
        r.git(&["push", "-q", "origin", "main", "feature/x"]);
        Self { _origin: r, bare }
    }
    fn file_url(&self) -> String {
        format!("file://{}", self.bare.path().display())
    }
}

fn dest_in(base: &TempDir, rel: &str) -> PathBuf {
    dunce::canonicalize(base.path()).unwrap().join(rel)
}

fn collector() -> (
    Arc<Mutex<Vec<CloneProgress>>>,
    impl Fn(CloneProgress) + Send,
) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let e2 = events.clone();
    (events, move |p| e2.lock().unwrap().push(p))
}

fn req(url: &str, dest: &Path, branch: Option<&str>) -> CloneRequest {
    CloneRequest {
        url: url.to_string(),
        destination: dest.to_path_buf(),
        branch: branch.map(str::to_string),
    }
}

fn head_branch(path: &Path) -> String {
    let out = std::process::Command::new(git_path())
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(path)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[tokio::test]
async fn clon_feliz_con_rama_progreso_en_orden_y_repo_abierto() {
    let remote = Remote::new();
    let tmp = tempfile::tempdir().unwrap();
    // Padres inexistentes: también se crean.
    let dest = dest_in(&tmp, "a/b/clon");
    let (events, cb) = collector();
    let repo = plain_engine()
        .clone_repository(
            req(&remote.file_url(), &dest, Some("feature/x")),
            cb,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(repo.path, dunce::canonicalize(&dest).unwrap());
    assert_eq!(head_branch(&dest), "feature/x");
    assert!(dest.join("solo-feature.txt").exists() && dest.join("src/f39.txt").exists());

    let ev = events.lock().unwrap().clone();
    assert_eq!(ev[0].phase, ClonePhase::Starting, "{ev:?}");
    assert!(
        ev.windows(2).all(|w| w[0].phase <= w[1].phase),
        "fases fuera de orden: {ev:?}"
    );
    assert!(
        ev.iter()
            .any(|p| p.phase == ClonePhase::Receiving && p.percent == Some(100)),
        "{ev:?}"
    );
    // Sin repetidos seguidos y porcentajes no decrecientes dentro de una fase.
    assert!(ev.windows(2).all(|w| w[0] != w[1]), "{ev:?}");
    for w in ev.windows(2) {
        if w[0].phase == w[1].phase {
            assert!(w[0].percent <= w[1].percent, "{ev:?}");
        }
    }
    // Los submódulos no se descargan y el remoto queda registrado sin credenciales.
    let cfg = std::fs::read_to_string(dest.join(".git/config")).unwrap();
    assert!(
        cfg.contains("[remote \"origin\"]") && !cfg.contains("ext"),
        "{cfg}"
    );
}

#[tokio::test]
async fn clon_de_ruta_local_en_destino_existente_vacio() {
    let remote = Remote::new();
    let tmp = tempfile::tempdir().unwrap();
    let dest = dest_in(&tmp, "vacio");
    std::fs::create_dir(&dest).unwrap();
    let url = remote.bare.path().display().to_string();
    let repo = plain_engine()
        .clone_repository(req(&url, &dest, None), |_| {}, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(head_branch(&repo.path), "main");
}

#[tokio::test]
async fn destino_con_contenido_da_error_claro_y_no_se_toca() {
    let remote = Remote::new();
    let tmp = tempfile::tempdir().unwrap();
    let dest = dest_in(&tmp, "ocupado");
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(dest.join("mio.txt"), b"importante").unwrap();
    let err = plain_engine()
        .clone_repository(
            req(&remote.file_url(), &dest, None),
            |_| {},
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, GitError::Io(m) if m.contains("no está vacía")),
        "{err:?}"
    );
    assert_eq!(std::fs::read(dest.join("mio.txt")).unwrap(), b"importante");
    assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 1);
    // Un fichero como destino.
    let file = dest_in(&tmp, "fichero");
    std::fs::write(&file, b"x").unwrap();
    assert!(plain_engine()
        .clone_repository(
            req(&remote.file_url(), &file, None),
            |_| {},
            CancellationToken::new()
        )
        .await
        .is_err());
    // Relativo.
    assert!(plain_engine()
        .clone_repository(
            req(&remote.file_url(), Path::new("rel"), None),
            |_| {},
            CancellationToken::new()
        )
        .await
        .is_err());
}

#[tokio::test]
async fn urls_peligrosas_se_rechazan_sin_ejecutar_nada() {
    let tmp = tempfile::tempdir().unwrap();
    let pwned = dest_in(&tmp, "pwned");
    let pw = ["cla", "ve-de-url"].concat();
    let urls = [
        format!("ext::sh -c touch% {}", pwned.display()),
        format!("ext::sh -c 'touch {}'", pwned.display()),
        "-x".to_string(),
        "--upload-pack=touch /tmp/ariadna-pwned".to_string(),
        "-c core.sshCommand=touch".to_string(),
        "fd::17".to_string(),
        "ftp://host.example/x.git".to_string(),
        "git://host.example/x.git".to_string(),
        "https://host.example/x\n.git".to_string(),
        format!("https://usuario:{pw}@host.example/x.git"),
        String::new(),
        "relativa/x.git".to_string(),
    ];
    for (i, url) in urls.iter().enumerate() {
        let dest = dest_in(&tmp, &format!("d{i}"));
        let err = plain_engine()
            .clone_repository(req(url, &dest, None), |_| {}, CancellationToken::new())
            .await
            .unwrap_err();
        assert!(
            matches!(err, GitError::InvalidReference(_)),
            "{url:?} → {err:?}"
        );
        let shown = format!("{err} {err:?}");
        assert!(!shown.contains(&pw), "{shown}");
        assert!(!dest.exists(), "{url:?} dejó el destino");
    }
    assert!(!pwned.exists());
    // Rama peligrosa.
    for branch in ["--upload-pack=x", "-b", "a b", "a..b", ""] {
        let dest = dest_in(&tmp, "rama");
        let err = plain_engine()
            .clone_repository(
                req("file:///no/importa.git", &dest, Some(branch)),
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, GitError::InvalidReference(_)),
            "{branch:?} → {err:?}"
        );
        assert!(!dest.exists());
    }
}

#[tokio::test]
async fn url_inexistente_o_rama_inexistente_no_deja_basura() {
    let remote = Remote::new();
    let tmp = tempfile::tempdir().unwrap();
    // Padres nuevos: desaparecen todos.
    let dest = dest_in(&tmp, "x/y/clon");
    let err = plain_engine()
        .clone_repository(
            req("file:///ruta/que/no/existe.git", &dest, None),
            |_| {},
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, GitError::CommandFailed { exit_code: 128, .. }),
        "{err:?}"
    );
    assert!(!dest_in(&tmp, "x").exists(), "quedó basura");
    // Destino existente vacío: se conserva vacío.
    let vacio = dest_in(&tmp, "vacio");
    std::fs::create_dir(&vacio).unwrap();
    let err = plain_engine()
        .clone_repository(
            req(&remote.file_url(), &vacio, Some("no-existe")),
            |_| {},
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
    assert!(vacio.is_dir() && std::fs::read_dir(&vacio).unwrap().next().is_none());
    // El resto del disco intacto.
    assert!(remote.bare.path().join("HEAD").exists());
}

#[cfg(unix)]
fn slow_engine(dir: &Path) -> GitCliEngine {
    use std::os::unix::fs::PermissionsExt;
    // `uploadpack.packObjectsHook` solo se respeta en config protegida (línea de comandos): el
    // servidor tarda 6 s en empezar a enviar el pack.
    let hook = dir.join("hook.sh");
    std::fs::write(&hook, "#!/bin/sh\nsleep 6\nexec \"$@\"\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    GitCliEngine::new(
        GitRunner::with_path(git_path())
            .with_config("credential.helper", "")
            .with_config("uploadpack.packObjectsHook", hook.display().to_string()),
    )
}

/// Cancela en cuanto el destino aparece en disco (git ya está clonando).
#[cfg(unix)]
async fn cancel_when_exists(dest: PathBuf, token: CancellationToken) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline && !dest.exists() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    token.cancel();
}

#[cfg(unix)]
#[tokio::test]
// Con git >= 2.53 el clon local no respeta `uploadpack.packObjectsHook` y termina antes de poder
// cancelarlo. Pendiente: ralentizar el servidor de otra forma (p. ej. HTTP propio con retardo).
#[ignore = "git 2.53 ignora el hook que ralentiza el clon local; ver tasks.md M4"]
async fn cancelar_a_mitad_deja_el_disco_limpio() {
    let remote = Remote::new();
    let tmp = tempfile::tempdir().unwrap();
    let engine = slow_engine(tmp.path());

    // 1. Destino nuevo con padres nuevos: no queda nada.
    let dest = dest_in(&tmp, "p/q/clon");
    let token = CancellationToken::new();
    tokio::spawn(cancel_when_exists(dest.clone(), token.clone()));
    let started = Instant::now();
    let err = engine
        .clone_repository(req(&remote.file_url(), &dest, None), |_| {}, token)
        .await
        .unwrap_err();
    assert_eq!(err, GitError::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "no se canceló a tiempo"
    );
    assert!(!dest_in(&tmp, "p").exists(), "quedó basura tras cancelar");

    // 2. Destino existente y vacío: se vacía pero la carpeta se conserva.
    let vacio = dest_in(&tmp, "vacio");
    std::fs::create_dir(&vacio).unwrap();
    let token = CancellationToken::new();
    let t2 = token.clone();
    let v2 = vacio.clone();
    tokio::spawn(async move {
        // Espera a que git cree `.git` dentro.
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline && !v2.join(".git").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        t2.cancel();
    });
    let err = engine
        .clone_repository(req(&remote.file_url(), &vacio, None), |_| {}, token)
        .await
        .unwrap_err();
    assert_eq!(err, GitError::Cancelled);
    assert!(vacio.is_dir() && std::fs::read_dir(&vacio).unwrap().next().is_none());

    // 3. Cancelado antes de empezar: ni se crea nada.
    let dest = dest_in(&tmp, "nunca");
    let token = CancellationToken::new();
    token.cancel();
    let err = engine
        .clone_repository(req(&remote.file_url(), &dest, None), |_| {}, token)
        .await
        .unwrap_err();
    assert_eq!(err, GitError::Cancelled);
    assert!(!dest.exists());
}

#[tokio::test]
async fn fallo_de_autenticacion_http_es_auth_y_limpia() {
    let srv = HttpServer::start(Mode::Status(401));
    let tmp = tempfile::tempdir().unwrap();
    let dest = dest_in(&tmp, "n/clon");
    let url = srv.url("o/r.git");
    let err = plain_engine()
        .clone_repository(req(&url, &dest, None), |_| {}, CancellationToken::new())
        .await
        .unwrap_err();
    assert!(
        matches!(&err, GitError::Auth { host, hint } if host == "127.0.0.1" && !hint.is_empty()),
        "{err:?}"
    );
    assert!(!dest_in(&tmp, "n").exists());

    // Servidor caído: fallo ordinario, no AUTH.
    let url = format!("http://127.0.0.1:{}/o/r.git", common::http::closed_port());
    let err = plain_engine()
        .clone_repository(req(&url, &dest, None), |_| {}, CancellationToken::new())
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
    assert!(!dest_in(&tmp, "n").exists());
}

// --- PAT de un solo uso (GIT_ASKPASS temporal) -------------------------------------------

const PAT: &str = "pat-de-prueba-9f3a7c1e";

/// Servidor HTTP que exige `Basic usuario:PAT` y sirve un bare real.
fn private_server(user: &str) -> (HttpServer, tempfile::TempDir) {
    let root = common::served_bare_repo();
    let basic = common::http::base64(format!("{user}:{PAT}").as_bytes());
    let srv = HttpServer::start(Mode::Static {
        root: root.path().to_path_buf(),
        basic: Some(basic),
    });
    (srv, root)
}

fn all_text_under(dir: &Path) -> String {
    let mut out = String::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(s) = std::fs::read(&p) {
                out.push_str(&String::from_utf8_lossy(&s));
            }
        }
    }
    out
}

#[tokio::test]
async fn clona_un_repo_privado_con_pat_y_no_deja_rastro_del_token() {
    let (srv, _root) = private_server("persona");
    let tmp = tempfile::tempdir().unwrap();
    let dest = dest_in(&tmp, "privado");
    let url = srv.url("repo.git");
    let auth = CloneAuth::new("persona", Secret::new(PAT)).unwrap();
    let repo = plain_engine()
        .clone_repository_with_auth(
            req(&url, &dest, None),
            auth,
            |_| {},
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(repo.path, dunce::canonicalize(&dest).unwrap());
    assert!(dest.join("README.md").exists() || std::fs::read_dir(&dest).unwrap().count() > 1);
    // El token no está en ningún archivo del repo clonado (.git/config incluido).
    assert!(!all_text_under(&dest).contains(PAT));
    // La URL guardada es la limpia (sin usuario ni token).
    let cfg = std::fs::read_to_string(dest.join(".git/config")).unwrap();
    assert!(cfg.contains(&url), "{cfg}");
}

#[tokio::test]
async fn el_usuario_vacio_usa_un_nombre_neutro_y_un_token_incorrecto_es_auth_sin_eco() {
    // Servidor que solo acepta `git:<PAT>`: el usuario por defecto funciona.
    let (srv, _root) = private_server("git");
    let tmp = tempfile::tempdir().unwrap();
    let url = srv.url("repo.git");
    let ok = CloneAuth::new("", Secret::new(PAT)).unwrap();
    plain_engine()
        .clone_repository_with_auth(
            req(&url, &dest_in(&tmp, "ok"), None),
            ok,
            |_| {},
            CancellationToken::new(),
        )
        .await
        .unwrap();

    // Token equivocado: AUTH, nada en el disco y el token que mandamos no aparece en el error.
    let wrong = ["equivocado-", "zzz-5521"].concat();
    let dest = dest_in(&tmp, "n/mal");
    let bad = CloneAuth::new("git", Secret::new(wrong.clone())).unwrap();
    let err = plain_engine()
        .clone_repository_with_auth(
            req(&url, &dest, None),
            bad,
            |_| {},
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::Auth { .. }), "{err:?}");
    let shown = format!("{err} {err:?}");
    assert!(!shown.contains(&wrong) && !shown.contains(PAT), "{shown}");
    assert!(!dest_in(&tmp, "n").exists());
}

#[tokio::test]
async fn sin_token_el_repo_privado_pide_credenciales() {
    let (srv, _root) = private_server("git");
    let tmp = tempfile::tempdir().unwrap();
    let err = plain_engine()
        .clone_repository(
            req(&srv.url("repo.git"), &dest_in(&tmp, "x"), None),
            |_| {},
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::Auth { .. }), "{err:?}");
}

#[tokio::test]
async fn un_token_con_un_helper_del_sistema_no_se_entrega_a_ningun_helper() {
    // Un helper «espía» que escribiría lo que reciba en un archivo: con credenciales de un
    // solo uso no debe invocarse ni para `get` ni para `store`.
    let (srv, _root) = private_server("git");
    let tmp = tempfile::tempdir().unwrap();
    let spy_log = tmp.path().join("spy.log");
    let spy = tmp.path().join("spy.sh");
    std::fs::write(
        &spy,
        format!(
            "#!/bin/sh\ncat >> '{}'\necho \"$1\" >> '{}'\n",
            spy_log.display(),
            spy_log.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&spy, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let engine = GitCliEngine::new(
        GitRunner::with_path(git_path())
            .with_config("credential.helper", format!("!{}", spy.display())),
    );
    let auth = CloneAuth::new("git", Secret::new(PAT)).unwrap();
    engine
        .clone_repository_with_auth(
            req(&srv.url("repo.git"), &dest_in(&tmp, "c"), None),
            auth,
            |_| {},
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let log = std::fs::read_to_string(&spy_log).unwrap_or_default();
    assert!(!log.contains(PAT), "el helper recibió el token: {log}");
    assert!(log.is_empty(), "no debía invocarse ningún helper: {log}");
}

#[tokio::test]
async fn el_pat_en_clonados_locales_se_ignora_y_no_se_filtra() {
    let remote = Remote::new();
    let tmp = tempfile::tempdir().unwrap();
    let dest = dest_in(&tmp, "local");
    let auth = CloneAuth::new("u", Secret::new(PAT)).unwrap();
    plain_engine()
        .clone_repository_with_auth(
            req(&remote.file_url(), &dest, None),
            auth,
            |_| {},
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(!all_text_under(&dest.join(".git")).contains(PAT));
}
