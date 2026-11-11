//! `git credential fill` no interactivo con un `git` falso (solo Unix: usa un script `sh`).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

use ariadna_hosting::{AuthMethod, CredentialSource, GitCredentialFill};
use common::{assert_no_secret, TOKEN};

fn fake_git(dir: &Path, body: &str) -> std::path::PathBuf {
    let p = dir.join("git");
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

/// Una sola prueba con varios pasos: crear y ejecutar scripts a la vez desde varios hilos
/// provoca `ETXTBSY` en Linux.
#[tokio::test]
async fn git_credential_fill_no_interactivo_y_sin_dejar_rastro() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let o = out.display();

    // 1. Camino feliz: argv, stdin y entorno.
    std::env::set_var("ARIADNA_FUGA", "no-debe-llegar");
    let git = fake_git(
        tmp.path(),
        &format!(
            "echo \"$@\" > {o}/argv\nenv > {o}/env\ncat > {o}/stdin\nprintf 'protocol=https\\nhost=dev.azure.com\\nusername=PAT-user\\npassword={TOKEN}\\n'"
        ),
    );
    let fill = GitCredentialFill::new(&git);
    let auth = fill
        .credentials("https://dev.azure.com/org/proj/_git/repo")
        .await
        .unwrap();
    match &auth {
        AuthMethod::Pat { username, secret } => {
            assert_eq!(username, "PAT-user");
            assert_eq!(secret.expose(), TOKEN);
        }
        other => panic!("{other:?}"),
    }
    assert_no_secret(&format!("{auth:?}"));
    let read = |n: &str| std::fs::read_to_string(out.join(n)).unwrap();
    assert_eq!(read("argv").trim(), "credential fill");
    assert_eq!(
        read("stdin"),
        "url=https://dev.azure.com/org/proj/_git/repo\n\n"
    );
    let env = read("env");
    assert!(env.contains("GIT_TERMINAL_PROMPT=0"), "{env}");
    assert!(env.contains("GCM_INTERACTIVE=never"), "{env}");
    assert!(
        !env.contains("ARIADNA_FUGA"),
        "el entorno no está saneado:\n{env}"
    );
    assert!(env.contains("PATH="), "se conserva la lista blanca");
    for f in ["argv", "stdin", "env"] {
        assert!(!read(f).contains(TOKEN), "el secreto no debe estar en {f}");
    }

    // 2. Una contraseña incrustada en la URL no se reenvía al helper.
    fill.credentials("https://usuario:contrasena-ajena@dev.azure.com/org/proj/_git/repo")
        .await
        .unwrap();
    let stdin = read("stdin");
    assert!(!stdin.contains("contrasena-ajena"), "{stdin}");

    // 3. El helper falla (no tiene la credencial): AUTH, sin texto del secreto.
    let git = fake_git(tmp.path(), "cat > /dev/null\nexit 128");
    let e = GitCredentialFill::new(&git)
        .credentials("https://github.com/o/r")
        .await
        .unwrap_err();
    assert_eq!(e.code(), "AUTH");
    assert!(e.to_string().contains("github.com"));

    // 4. Sale bien pero sin contraseña: AUTH.
    let git = fake_git(tmp.path(), "cat > /dev/null\nprintf 'username=solo\\n'");
    let e = GitCredentialFill::new(&git)
        .credentials("https://github.com/o/r")
        .await
        .unwrap_err();
    assert_eq!(e.code(), "AUTH");

    // 5. Entradas inválidas ni siquiera lanzan git.
    let fill = GitCredentialFill::new(tmp.path().join("no-existe"));
    for bad in [
        "http://github.com/o/r",
        "ssh://git@github.com/o/r",
        "github.com/o/r",
        "https://github.com/o\n/r",
        "",
    ] {
        assert_eq!(
            fill.credentials(bad).await.unwrap_err().code(),
            "INVALID_REFERENCE",
            "{bad:?}"
        );
    }

    // 6. git inexistente: IO.
    assert_eq!(
        fill.credentials("https://github.com/o/r")
            .await
            .unwrap_err()
            .code(),
        "IO"
    );

    // 7. El helper se cuelga: se corta por tiempo y es AUTH.
    let git = fake_git(tmp.path(), "sleep 20");
    let started = Instant::now();
    let e = GitCredentialFill::new(&git)
        .with_timeout(Duration::from_millis(300))
        .credentials("https://github.com/o/r")
        .await
        .unwrap_err();
    assert_eq!(e.code(), "AUTH");
    assert!(started.elapsed() < Duration::from_secs(10));
}
