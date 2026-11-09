#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use ariadna_process::{run, CancellationToken, EnvPolicy, ProcessError, ProcessSpec};

fn fake() -> ProcessSpec {
    ProcessSpec::new(env!("CARGO_BIN_EXE_ariadna-proc-fake"))
}

#[tokio::test]
async fn captura_stdout_y_codigo_de_salida() {
    let out = run(
        fake().args(["args", "uno", "dos palabras"]),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(out.success());
    let s = out.stdout_lossy();
    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(lines, ["uno", "dos palabras"]);
}

#[tokio::test]
async fn codigo_distinto_de_cero_y_stderr() {
    let out = run(fake().args(["stderr", "fallo"]), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(out.code, Some(3));
    assert_eq!(out.stderr_lossy(), "fallo");
    assert!(!out.success());
}

#[tokio::test]
async fn stdin_grande_sin_bloqueo() {
    // 1 MiB por stdin: se escribe en paralelo a la lectura del eco.
    let data = vec![b'a'; 1024 * 1024];
    let out = run(
        fake().arg("echo-stdin").stdin(data.clone()),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(out.stdout.len(), data.len());
    assert!(!out.truncated);
}

#[tokio::test]
async fn tope_de_salida_trunca_y_no_bloquea() {
    let out = run(
        fake().args(["flood", "5000000"]).max_output_bytes(1000),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(out.stdout.len(), 1000);
    assert!(out.truncated);
}

#[tokio::test]
async fn timeout_termina_el_proceso() {
    let t0 = Instant::now();
    let out = run(
        fake()
            .args(["sleep", "30000"])
            .timeout(Some(Duration::from_millis(300))),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(out.timed_out);
    assert!(!out.success());
    assert!(t0.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn cancelacion_termina_el_proceso() {
    let token = CancellationToken::new();
    let t = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        t.cancel();
    });
    let t0 = Instant::now();
    let out = run(fake().args(["sleep", "30000"]).timeout(None), token)
        .await
        .unwrap();
    assert!(out.cancelled);
    assert!(t0.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn timeout_mata_tambien_al_nieto() {
    // Si el nieto sobreviviera mantendría abiertos los pipes y `run` no volvería pronto.
    let t0 = Instant::now();
    let out = run(
        fake()
            .arg("spawn-child")
            .timeout(Some(Duration::from_millis(500))),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(out.timed_out);
    assert!(
        t0.elapsed() < Duration::from_secs(10),
        "tardó {:?}",
        t0.elapsed()
    );
}

#[tokio::test]
async fn entorno_saneado_llega_al_hijo() {
    let spec = fake()
        .arg("env")
        .env_policy(EnvPolicy::Allowlist {
            allow: vec!["SystemRoot".into(), "PATH".into()],
            always_remove: vec![],
        })
        .env("ARIADNA_PRUEBA", "si");
    let out = run(spec, CancellationToken::new()).await.unwrap();
    let s = out.stdout_lossy();
    assert!(s.contains("ARIADNA_PRUEBA=si"), "{s}");
    assert!(
        !s.to_uppercase().contains("USERPROFILE="),
        "no debería heredarse: {s}"
    );
}

#[tokio::test]
async fn programa_inexistente_da_error_de_spawn() {
    let err = run(
        ProcessSpec::new("no-existe-ariadna-xyz"),
        CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ProcessError::Spawn { .. }));
}
