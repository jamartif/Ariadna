//! Credenciales por PAT contra git de verdad, con un credential helper FALSO que vuelca lo
//! que recibe (stdin, argv y entorno de git) y un servidor HTTP que exige Basic.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]
mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use ariadna_git::{CredentialStore, GitCliEngine, GitError, GitRunner, Secret};
use common::http::{base64, closed_port, HttpServer, Mode};
use common::{git_path, served_bare_repo};
use tempfile::TempDir;

/// Helper de pruebas: guarda en `db` lo último que recibe `store`, lo devuelve en `get`, lo borra
/// en `erase`, y deja en la carpeta el stdin de cada operación y el argv y entorno de sus
/// procesos ancestros (git y el `sh` intermedio).
const HELPER: &str = r#"#!/bin/sh
dir="$(dirname "$0")"
op="$1"
echo "$op | $*" >> "$dir/log.txt"
pid=$$
i=0
while [ $i -lt 4 ]; do
  pid=$(awk '{print $4}' "/proc/$pid/stat" 2>/dev/null)
  [ -z "$pid" ] && break
  [ "$pid" -le 1 ] && break
  tr '\0' ' ' < "/proc/$pid/cmdline" >> "$dir/anc-cmdline-$op.txt" 2>/dev/null
  echo >> "$dir/anc-cmdline-$op.txt"
  tr '\0' '\n' < "/proc/$pid/environ" >> "$dir/anc-environ-$op.txt" 2>/dev/null
  i=$((i+1))
done
case "$op" in
  store) cat > "$dir/stdin-store.txt"; cp "$dir/stdin-store.txt" "$dir/db" ;;
  erase) cat > "$dir/stdin-erase.txt"; rm -f "$dir/db" ;;
  get)   cat > "$dir/stdin-get.txt"; [ -f "$dir/db" ] && grep -E '^(username|password)=' "$dir/db" ;;
esac
exit 0
"#;

struct Helper {
    dir: TempDir,
}

impl Helper {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("helper.sh");
        std::fs::write(&script, HELPER).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }
    fn script(&self) -> PathBuf {
        self.dir.path().join("helper.sh")
    }
    fn engine(&self) -> GitCliEngine {
        self.engine_with(&[])
    }
    fn engine_with(&self, extra: &[(&str, &str)]) -> GitCliEngine {
        let mut r = GitRunner::with_path(git_path())
            .with_config("credential.helper", "")
            .with_config("credential.helper", self.script().display().to_string());
        for (k, v) in extra {
            r = r.with_config(*k, *v);
        }
        GitCliEngine::new(r)
    }
    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(name)).unwrap_or_default()
    }
    fn exists(&self, name: &str) -> bool {
        self.dir.path().join(name).exists()
    }
}

/// Token falso construido en ejecución (evita que gitleaks lo vea como secreto).
fn fake_token() -> String {
    ["ghp_", "Zq", "9InventadoParaTests", "0123456789"].concat()
}

const AZURE: &str = "https://org@dev.azure.com/org/Mi%20Proyecto/_git/repo";

#[tokio::test]
async fn approve_recibe_los_campos_por_stdin_y_el_secreto_no_esta_en_argv_ni_entorno() {
    let h = Helper::new();
    let token = fake_token();
    h.engine()
        .store_credential(AZURE, "cualquiera", Secret::new(token.clone()))
        .await
        .unwrap();

    let stdin = h.read("stdin-store.txt");
    let lines: Vec<&str> = stdin.lines().collect();
    assert!(lines.contains(&"protocol=https"), "{stdin}");
    assert!(lines.contains(&"host=dev.azure.com"), "{stdin}");
    assert!(lines.contains(&"username=cualquiera"), "{stdin}");
    let pw_line = format!("password={token}");
    assert!(
        lines.contains(&pw_line.as_str()),
        "el secreto debe llegar por stdin"
    );
    // Sin `credential.useHttpPath`, git descarta la ruta antes de llamar al helper.
    assert!(!stdin.contains("path="), "{stdin}");

    // El secreto no está en ningún argv ni entorno de git/sh/helper.
    let log = h.read("log.txt");
    let cmdlines = h.read("anc-cmdline-store.txt");
    let environ = h.read("anc-environ-store.txt");
    // `/proc` solo existe en Linux: en macOS no se pueden inspeccionar los ancestros.
    if cfg!(target_os = "linux") {
        assert!(
            cmdlines.contains("credential approve"),
            "ancestros: {cmdlines}"
        );
        assert!(
            environ.contains("GIT_TERMINAL_PROMPT=0"),
            "el entorno de git debe verse"
        );
    }
    let checks = [("log", &log), ("argv", &cmdlines), ("env", &environ)];
    for (name, text) in checks {
        assert!(
            !text.contains(&token),
            "el secreto aparece en {name}: {text}"
        );
    }
}

#[tokio::test]
async fn con_use_http_path_el_helper_recibe_la_ruta_decodificada() {
    let h = Helper::new();
    let e = h.engine_with(&[("credential.useHttpPath", "true")]);
    e.store_credential(AZURE, "u", Secret::new(fake_token()))
        .await
        .unwrap();
    let stdin = h.read("stdin-store.txt");
    assert!(
        stdin.lines().any(|l| l == "path=org/Mi Proyecto/_git/repo"),
        "{stdin}"
    );
}

#[tokio::test]
async fn reject_borra_la_credencial_y_no_envia_el_secreto() {
    let h = Helper::new();
    let token = fake_token();
    let e = h.engine();
    e.store_credential(AZURE, "u", Secret::new(token.clone()))
        .await
        .unwrap();
    assert!(h.exists("db"));
    e.forget_credential(AZURE, "u").await.unwrap();
    assert!(!h.exists("db"));
    let erase = h.read("stdin-erase.txt");
    assert!(
        erase.contains("host=dev.azure.com") && erase.contains("username=u"),
        "{erase}"
    );
    assert!(!erase.contains(&token));
}

#[tokio::test]
async fn entradas_invalidas_no_llegan_a_git_ni_filtran_nada() {
    let h = Helper::new();
    let e = h.engine();
    let token = fake_token();
    let pw = ["cla", "ve-de-url"].concat();
    for url in [
        "ext::sh -c touch% /tmp/pwned".to_string(),
        "-x".to_string(),
        "--upload-pack=touch /tmp/x".to_string(),
        "ssh://git@github.com/o/r.git".to_string(),
        "git@github.com:o/r.git".to_string(),
        "/srv/git/r.git".to_string(),
        "file:///srv/git/r.git".to_string(),
        "https://host.example/x\nhost=evil.com".to_string(),
        format!("https://usuario:{pw}@host.example/x.git"),
        String::new(),
    ] {
        let err = e
            .store_credential(&url, "u", Secret::new(token.clone()))
            .await
            .unwrap_err();
        let shown = format!("{err} | {err:?}");
        assert!(
            matches!(err, GitError::InvalidReference(_)),
            "{url:?} → {shown}"
        );
        assert!(!shown.contains(&token) && !shown.contains(&pw), "{shown}");
        assert!(e.forget_credential(&url, "u").await.is_err(), "{url:?}");
        // `test_credential` sí admite ssh y scp-like (git ls-remote); el resto, no.
        let tested = e.test_credential(&url).await;
        if !(url.starts_with("ssh://") || url.starts_with("git@")) {
            assert!(
                matches!(tested, Err(GitError::InvalidReference(_))),
                "{url:?} → {tested:?}"
            );
        }
    }
    // Campos con saltos de línea (inyección en el protocolo) o vacíos.
    for (user, tok) in [
        ("a\nhost=evil.com", token.as_str()),
        ("u", "x\npassword=y"),
        ("", token.as_str()),
        ("u", ""),
    ] {
        let err = e
            .store_credential(AZURE, user, Secret::new(tok))
            .await
            .unwrap_err();
        assert!(matches!(err, GitError::InvalidReference(_)), "{user:?}");
        assert!(!format!("{err}{err:?}").contains(&token));
    }
    assert!(
        !h.exists("log.txt"),
        "git no debía llegar a llamar al helper"
    );
}

#[tokio::test]
async fn sin_credencial_el_sondeo_da_auth_y_con_pat_correcto_funciona() {
    let root = served_bare_repo();
    let user = "cualquiera";
    let token = fake_token();
    let basic = base64(format!("{user}:{token}").as_bytes());
    let srv = HttpServer::start(Mode::Static {
        root: root.path().to_path_buf(),
        basic: Some(basic),
    });
    let url = srv.url("repo.git");
    let h = Helper::new();
    let e = h.engine();

    // 1. Sin nada guardado: git no puede preguntar (sin terminal) → AUTH.
    let err = e.test_credential(&url).await.unwrap_err();
    assert!(
        matches!(&err, GitError::Auth { host, .. } if host == "127.0.0.1"),
        "{err:?}"
    );

    // 2. PAT incorrecto → AUTH.
    e.store_credential(&url, user, Secret::new("incorrecto"))
        .await
        .unwrap();
    let err = e.test_credential(&url).await.unwrap_err();
    assert!(matches!(err, GitError::Auth { .. }), "{err:?}");

    // 3. PAT correcto → OK; fill devuelve el mismo usuario y secreto.
    e.store_credential(&url, user, Secret::new(token.clone()))
        .await
        .unwrap();
    e.test_credential(&url).await.unwrap();
    e.store_credential(&url, user, Secret::new(token.clone()))
        .await
        .unwrap();
    let filled = e.fill_credential(&url).await.unwrap();
    assert_eq!(filled.username, user);
    assert_eq!(filled.secret.expose(), token);
    assert!(!format!("{filled:?}").contains(&token));

    // 4. Olvidar → AUTH otra vez, y fill también.
    e.forget_credential(&url, user).await.unwrap();
    assert!(matches!(
        e.test_credential(&url).await,
        Err(GitError::Auth { .. })
    ));
    assert!(matches!(
        e.fill_credential(&url).await,
        Err(GitError::Auth { .. })
    ));

    // El token nunca viaja en la URL de las peticiones.
    let reqs = srv.requests.lock().unwrap().join("\n");
    assert!(!reqs.contains(&token));
}

#[tokio::test]
async fn repo_inexistente_y_red_caida_no_son_auth() {
    let root = served_bare_repo();
    let srv = HttpServer::start(Mode::Static {
        root: root.path().to_path_buf(),
        basic: None,
    });
    let e = common::plain_engine();
    // Sin autenticación requerida: OK.
    e.test_credential(&srv.url("repo.git")).await.unwrap();
    // 404 → fallo ordinario, no AUTH.
    let err = e
        .test_credential(&srv.url("no-existe.git"))
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
    // Conexión rehusada → fallo de red, no AUTH.
    let err = e
        .test_credential(&format!("http://127.0.0.1:{}/x.git", closed_port()))
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }), "{err:?}");
    // 403 → AUTH.
    let forbidden = HttpServer::start(Mode::Status(403));
    let err = e
        .test_credential(&forbidden.url("x.git"))
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::Auth { .. }), "{err:?}");
}

#[tokio::test]
async fn detecta_si_hay_credential_helper() {
    let h = Helper::new();
    assert!(h.engine().has_credential_helper().await.unwrap());
    assert!(!common::plain_engine()
        .has_credential_helper()
        .await
        .unwrap());
}
