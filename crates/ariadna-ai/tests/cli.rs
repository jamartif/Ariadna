//! Tests de integración con el CLI falso (`ariadna-ai-fake-cli`): comprueban lo que el
//! hijo realmente recibe (argv, entorno, stdin, cwd) y los códigos de error.
#![allow(clippy::unwrap_used)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use ariadna_ai::commands::build_args;
use ariadna_ai::env::COMMON_ALLOW;
use ariadna_ai::orchestrator::{Limiter, Orchestrator, Profile, ProfileKind};
use ariadna_ai::prompt::ConflictInput;
use ariadna_ai::{
    AiError, AiProvider, AiSettings, AiSuggestion, Availability, CallContext, CliProvider,
    CliProviderConfig, ProviderChoice, ProviderId, SuggestionStatus, SummaryStyle,
};
use serde_json::Value;
use tempfile::TempDir;

const FAKE: &str = env!("CARGO_BIN_EXE_ariadna-ai-fake-cli");
const SECRET: &str = "valor-secreto-de-prueba-123";

/// Fija una vez, antes de lanzar ningún hijo, variables sensibles en el entorno del padre.
fn parent_env() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::env::set_var("ARIADNA_TEST_SECRET", SECRET);
        std::env::set_var("ANTHROPIC_API_KEY", SECRET);
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", SECRET);
        std::env::set_var("OPENAI_API_KEY", SECRET);
        std::env::set_var("GITHUB_TOKEN", SECRET);
        std::env::set_var("GH_TOKEN", SECRET);
        std::env::set_var("LC_ALL", "C");
    });
}

struct Fx {
    _tmp: TempDir,
    sandbox: PathBuf,
    dump: PathBuf,
}

fn config(id: ProviderId, behavior: &str, reply: Option<&str>) -> (CliProviderConfig, Fx) {
    parent_env();
    let tmp = tempfile::tempdir().unwrap();
    let sandbox = tmp.path().join("ai-sandbox");
    let dump = tmp.path().join("dump.jsonl");
    let mut cfg = CliProviderConfig::new(id, &sandbox);
    cfg.configured_binary = Some(PathBuf::from(FAKE));
    let mut extra: Vec<(OsString, OsString)> = vec![
        ("FAKE_MODE".into(), id.binary_name().into()),
        ("FAKE_BEHAVIOR".into(), behavior.into()),
        ("FAKE_DUMP".into(), dump.clone().into_os_string()),
    ];
    if let Some(r) = reply {
        extra.push(("FAKE_REPLY".into(), r.into()));
    }
    cfg.extra_env = extra;
    cfg.timeout = Duration::from_secs(20);
    (
        cfg,
        Fx {
            _tmp: tmp,
            sandbox,
            dump,
        },
    )
}

fn provider(id: ProviderId, behavior: &str) -> (CliProvider, Fx) {
    let (cfg, fx) = config(id, behavior, None);
    (CliProvider::new(cfg), fx)
}

fn dumps(fx: &Fx) -> Vec<Value> {
    std::fs::read_to_string(&fx.dump)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn ctx() -> CallContext {
    CallContext::new("rev-1")
}

const DIFF: &str =
    "diff --git a/src/rama-secreta.rs b/src/rama-secreta.rs\n+let unico = 1; // +hola_unico\n";

#[tokio::test]
async fn el_secreto_del_padre_y_las_claves_prohibidas_no_llegan_al_hijo() {
    for id in ProviderId::AUTO_ORDER {
        let (p, fx) = provider(id, "ok");
        p.suggest_commit_message(&ctx(), DIFF, None).await.unwrap();
        let d = dumps(&fx);
        let call = d.last().unwrap();
        let env = call["env"].as_object().unwrap();
        let raw = serde_json::to_string(env).unwrap();
        assert!(!raw.contains(SECRET), "{id:?}: el secreto llegó al hijo");
        for k in env.keys() {
            // Windows sobre ARM64 inyecta estas variables en todo proceso, no las pasa Ariadna.
            let os_injected = k.eq_ignore_ascii_case("PROCESSOR_ARCHITECTURE")
                || k.eq_ignore_ascii_case("PROCESSOR_ARCHITEW6432");
            let ok = k.starts_with("FAKE_")
                || os_injected
                || COMMON_ALLOW
                    .iter()
                    .any(|pat| ariadna_process::name_matches(pat, k));
            assert!(ok, "{id:?}: variable fuera de la lista blanca: {k}");
        }
        for banned in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "OPENAI_API_KEY",
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "ARIADNA_TEST_SECRET",
        ] {
            assert!(
                !env.keys().any(|k| k.eq_ignore_ascii_case(banned)),
                "{id:?}: {banned}"
            );
        }
        // Lo permitido sí pasa.
        assert!(env.keys().any(|k| k.eq_ignore_ascii_case("LC_ALL")));
    }
}

#[tokio::test]
async fn copilot_recibe_el_token_solo_si_el_usuario_lo_declara() {
    let (mut cfg, fx) = config(ProviderId::Copilot, "ok", None);
    cfg.env.copilot_token_var = Some("GH_TOKEN".into());
    let p = CliProvider::new(cfg);
    p.suggest_commit_message(&ctx(), DIFF, None).await.unwrap();
    let env = dumps(&fx).last().unwrap()["env"].clone();
    assert_eq!(env["GH_TOKEN"], SECRET);
    assert!(env.get("GITHUB_TOKEN").is_none());
}

#[tokio::test]
async fn el_prompt_va_por_stdin_y_no_por_argv() {
    for id in ProviderId::AUTO_ORDER {
        let (p, fx) = provider(id, "ok");
        p.suggest_commit_message(&ctx(), DIFF, Some("plantilla-unica"))
            .await
            .unwrap();
        let d = dumps(&fx);
        let call = d.last().unwrap();
        let argv: Vec<String> = call["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let expected: Vec<String> = build_args(id)
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(argv, expected, "{id:?}");
        let joined = argv.join(" ");
        for repo_data in [
            "rama-secreta",
            "hola_unico",
            "plantilla-unica",
            "src/",
            "rev-1",
        ] {
            assert!(!joined.contains(repo_data), "{id:?}: {repo_data} en argv");
        }
        assert!(!argv.iter().any(|a| a == "--bare"));
        let stdin = call["stdin"].as_str().unwrap();
        assert!(stdin.contains("+hola_unico") && stdin.contains("plantilla-unica"));
        assert!(stdin.contains("UNTRUSTED") && stdin.contains("Do not run tools"));
    }
}

#[tokio::test]
async fn claude_usa_los_flags_verificados() {
    let (p, fx) = provider(ProviderId::Claude, "ok");
    p.summarize_diff(&ctx(), DIFF, SummaryStyle::Bullets)
        .await
        .unwrap();
    let argv = dumps(&fx).last().unwrap()["argv"].clone();
    assert_eq!(
        argv,
        serde_json::json!([
            "-p",
            "--output-format",
            "json",
            "--tools",
            "",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--no-session-persistence",
            "--permission-mode",
            "plan"
        ])
    );
}

#[tokio::test]
async fn el_directorio_de_trabajo_esta_vacio_y_se_limpia() {
    let (p, fx) = provider(ProviderId::Claude, "ok");
    p.suggest_commit_message(&ctx(), DIFF, None).await.unwrap();
    let d = dumps(&fx);
    let call = d.last().unwrap();
    assert_eq!(call["cwd_entries"], 0);
    assert!(call["cwd"].as_str().unwrap().contains("ai-sandbox"));
    assert_eq!(std::fs::read_dir(&fx.sandbox).unwrap().count(), 0);

    // También se limpia tras un error.
    let (p, fx) = provider(ProviderId::Claude, "exit:3");
    assert!(p.suggest_commit_message(&ctx(), DIFF, None).await.is_err());
    assert_eq!(std::fs::read_dir(&fx.sandbox).unwrap().count(), 0);
}

#[tokio::test]
async fn el_directorio_de_config_del_perfil_llega_como_variable() {
    let (p, fx) = provider(ProviderId::Codex, "ok");
    let mut c = ctx();
    c.config_dir = Some(PathBuf::from("/perfiles/empresa/codex"));
    p.suggest_commit_message(&c, DIFF, None).await.unwrap();
    let env = dumps(&fx).last().unwrap()["env"].clone();
    assert_eq!(env["CODEX_HOME"], "/perfiles/empresa/codex");
}

/// Port de `testContextLimitBlocksTheProcess`.
#[tokio::test]
async fn el_tope_de_contexto_bloquea_el_proceso() {
    let (mut cfg, fx) = config(ProviderId::Copilot, "ok", None);
    cfg.policy = ariadna_ai::AiContextPolicy::new(8);
    let p = CliProvider::new(cfg);
    let r = p
        .suggest_commit_message(&ctx(), "esto-es-demasiado-largo", None)
        .await;
    assert!(
        matches!(r, Err(AiError::ContextTooLarge { max_bytes: 8, .. })),
        "{r:?}"
    );
    assert!(dumps(&fx).is_empty(), "no se debió lanzar el CLI");
}

#[tokio::test]
async fn timeout_duro() {
    let (mut cfg, _fx) = config(ProviderId::Claude, "sleep", None);
    cfg.timeout = Duration::from_millis(400);
    let p = CliProvider::new(cfg);
    let t = Instant::now();
    let r = p.suggest_commit_message(&ctx(), DIFF, None).await;
    assert_eq!(
        r,
        Err(AiError::Timeout {
            provider: ProviderId::Claude
        })
    );
    assert!(t.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn cancelacion_mata_al_hijo() {
    let (p, _fx) = provider(ProviderId::Codex, "sleep");
    let token = ariadna_process::CancellationToken::new();
    let c = ctx().with_cancel(token.clone());
    let t = Instant::now();
    let h = tokio::spawn(async move { p.suggest_commit_message(&c, DIFF, None).await });
    tokio::time::sleep(Duration::from_millis(400)).await;
    token.cancel();
    assert_eq!(h.await.unwrap(), Err(AiError::Cancelled));
    assert!(t.elapsed() < Duration::from_secs(10));
}

/// Port de `testAuthenticationErrorIsActionable` (para los tres CLI).
#[tokio::test]
async fn el_error_de_login_es_accionable() {
    for id in ProviderId::AUTO_ORDER {
        let (p, _fx) = provider(id, "login");
        let r = p
            .summarize_diff(&ctx(), "diff", SummaryStyle::Bullets)
            .await;
        match r {
            Err(AiError::NotLoggedIn {
                provider,
                login_command,
            }) => {
                assert_eq!(provider, id);
                assert!(login_command.contains(match id {
                    ProviderId::Claude => "/login",
                    ProviderId::Codex => "codex login",
                    ProviderId::Copilot => "copilot login",
                }));
            }
            other => panic!("{id:?}: {other:?}"),
        }
    }
}

#[tokio::test]
async fn cuota_y_codigos_de_salida() {
    for id in ProviderId::AUTO_ORDER {
        let (p, _fx) = provider(id, "quota");
        let r = p.suggest_commit_message(&ctx(), DIFF, None).await;
        assert!(matches!(r, Err(AiError::Quota { .. })), "{id:?}: {r:?}");
    }
    let (p, _fx) = provider(ProviderId::Copilot, "exit:7");
    assert_eq!(
        p.suggest_commit_message(&ctx(), DIFF, None).await,
        Err(AiError::ProviderFailed {
            provider: ProviderId::Copilot,
            exit_code: Some(7),
            detail: "fallo simulado".into()
        })
    );
}

#[tokio::test]
async fn salida_no_reconocida_nunca_es_texto_crudo() {
    for id in [ProviderId::Claude, ProviderId::Codex] {
        let (p, _fx) = provider(id, "garbage");
        let r = p.suggest_commit_message(&ctx(), DIFF, None).await;
        assert_eq!(
            r,
            Err(AiError::BadOutput("formato no reconocido".into())),
            "{id:?}"
        );
    }
    let (p, _fx) = provider(ProviderId::Copilot, "empty");
    assert!(matches!(
        p.suggest_commit_message(&ctx(), DIFF, None).await,
        Err(AiError::BadOutput(_))
    ));
}

#[tokio::test]
async fn tope_de_salida() {
    let (mut cfg, _fx) = config(ProviderId::Copilot, "flood", None);
    cfg.max_output_bytes = 4096;
    let p = CliProvider::new(cfg);
    let r = p.suggest_commit_message(&ctx(), DIFF, None).await;
    assert!(matches!(r, Err(AiError::BadOutput(_))), "{r:?}");
}

#[tokio::test]
async fn disponibilidad_ejecuta_version() {
    let (p, fx) = provider(ProviderId::Claude, "ok");
    match p.availability().await {
        Availability::Available { binary, version } => {
            assert_eq!(binary, PathBuf::from(FAKE));
            assert_eq!(version, "claude 9.9.9 (fake)");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        dumps(&fx).last().unwrap()["argv"],
        serde_json::json!(["--version"])
    );

    let (cfg, _fx) = config(ProviderId::Claude, "version-fail", None);
    assert!(matches!(
        CliProvider::new(cfg).availability().await,
        Availability::Failed { .. }
    ));

    let (mut cfg, _fx) = config(ProviderId::Codex, "ok", None);
    cfg.configured_binary = Some(PathBuf::from("/no/existe/codex"));
    assert!(matches!(
        CliProvider::new(cfg).availability().await,
        Availability::Missing { .. }
    ));
}

/// Port de `testCopilotCommitMessageDoesNotInvokeGitOrWrite`.
#[tokio::test]
async fn copilot_mensaje_de_commit_sin_git_ni_escritura() {
    let (cfg, fx) = config(
        ProviderId::Copilot,
        "ok",
        Some("feat: parsear status\n\nUsa porcelain v2.\n"),
    );
    let p = CliProvider::new(cfg);
    let s = p
        .suggest_commit_message(&ctx(), "diff --git a/a b/a\n+hola\n", None)
        .await
        .unwrap();
    assert_eq!(s.status, SuggestionStatus::Pending);
    assert_eq!(s.provider, ProviderId::Copilot);
    assert_eq!(s.subject.as_deref(), Some("feat: parsear status"));
    assert_eq!(s.body, "Usa porcelain v2.");
    assert!(s.proposed_content.is_none());
    assert!(s.command_summary.contains("copilot -p"));
    assert!(!s.command_summary.contains("+hola"));
    assert_eq!(s.repo_revision, "rev-1");

    let d = dumps(&fx);
    assert_eq!(d.len(), 1);
    let argv: Vec<&str> = d[0]["argv"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for needed in ["-p", "-s", "--no-ask-user", "--deny-tool", "shell", "write"] {
        assert!(argv.contains(&needed), "{needed}");
    }
    assert!(!argv.contains(&"git"));
    assert!(!argv.iter().any(|a| a.contains("allow-tool")));
    let stdin = d[0]["stdin"].as_str().unwrap();
    assert!(stdin.contains("Do not run tools") && stdin.contains("+hola"));
}

const CONFLICT_ORIGINAL: &str =
    "cabecera\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> rama\npie\n";

fn conflict() -> ConflictInput {
    ConflictInput {
        path: "file.txt".into(),
        kind: "bothModified".into(),
        ours: Some("ours\n".into()),
        base: Some("base\n".into()),
        theirs: Some("theirs\n".into()),
        working_tree: Some(CONFLICT_ORIGINAL.into()),
    }
}

/// Port de `testCodexParsesJSONLAndProposesConflictWithoutApplying`.
#[tokio::test]
async fn codex_propone_un_conflicto_sin_aplicarlo() {
    let reply = "---FILE---\ncabecera\nresuelto\npie\n---RATIONALE---\nMe quedo con ambos títulos.";
    let (cfg, fx) = config(ProviderId::Codex, "ok", Some(reply));
    let p = CliProvider::new(cfg);
    let s = p
        .propose_conflict_resolution(&ctx(), &conflict())
        .await
        .unwrap();
    assert_eq!(s.status, SuggestionStatus::Pending);
    assert_eq!(s.provider, ProviderId::Codex);
    assert_eq!(
        s.proposed_content.as_deref(),
        Some("cabecera\nresuelto\npie\n")
    );
    assert_eq!(s.rationale.as_deref(), Some("Me quedo con ambos títulos."));
    assert_eq!(
        s.transparency_note.as_deref(),
        Some("Codex: 10 tokens de entrada, 4 de salida.")
    );
    let d = dumps(&fx);
    let argv: Vec<&str> = d[0]["argv"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        &argv[..5],
        [
            "exec",
            "--json",
            "--sandbox",
            "read-only",
            "--skip-git-repo-check"
        ]
    );
    assert!(!argv.contains(&"git"));
    // El fichero original no se tocó: la capa de IA no escribe.
    assert_eq!(conflict().working_tree.as_deref(), Some(CONFLICT_ORIGINAL));
}

#[tokio::test]
async fn propuesta_que_sale_de_los_hunks_se_rechaza() {
    let reply = "---FILE---\nCABECERA\nresuelto\npie\n---RATIONALE---\nx";
    let (cfg, _fx) = config(ProviderId::Codex, "ok", Some(reply));
    let r = CliProvider::new(cfg)
        .propose_conflict_resolution(&ctx(), &conflict())
        .await;
    assert!(
        matches!(r, Err(AiError::ConflictProposalRejected(_))),
        "{r:?}"
    );

    // Sin el original con marcadores no se puede validar: también se rechaza.
    let (cfg, _fx) = config(ProviderId::Codex, "ok", Some("---FILE---\nx\n"));
    let mut c = conflict();
    c.working_tree = None;
    let r = CliProvider::new(cfg)
        .propose_conflict_resolution(&ctx(), &c)
        .await;
    assert!(matches!(r, Err(AiError::ConflictProposalRejected(_))));
}

#[tokio::test]
async fn explicar_commit_y_secretos_redactados_en_stdin() {
    let (p, fx) = provider(ProviderId::Claude, "ok");
    let patch = format!(
        "diff --git a/.env b/.env\n+K=1\ndiff --git a/a.rs b/a.rs\n+key = \"AKIA{}\"\n",
        "ABCDEFGHIJKLMNOP"
    );
    let s = p
        .explain_commit(&ctx(), "abc123", &patch, "fix: algo")
        .await
        .unwrap();
    assert_eq!(s.subject.as_deref(), Some("fix: algo"));
    let stdin = dumps(&fx).last().unwrap()["stdin"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!stdin.contains(&format!("AKIA{}", "ABCDEFGHIJKLMNOP")));
    assert!(!stdin.contains("K=1"));
    assert!(stdin.contains("[REDACTED]"));
}

fn orchestrator(claude_ok: bool) -> (Orchestrator, Vec<Fx>) {
    let mut fxs = Vec::new();
    let mut provs: Vec<Arc<dyn AiProvider>> = Vec::new();
    for id in ProviderId::AUTO_ORDER {
        let (mut cfg, fx) = config(id, "ok", None);
        if id == ProviderId::Claude && !claude_ok {
            cfg.configured_binary = Some(PathBuf::from("/no/existe/claude"));
        }
        fxs.push(fx);
        provs.push(Arc::new(CliProvider::new(cfg)));
    }
    (Orchestrator::new(provs, Arc::new(Limiter::default())), fxs)
}

#[tokio::test]
async fn orquestador_de_extremo_a_extremo_con_el_cli_falso() {
    let mut profile = Profile::new("Personal", ProfileKind::Personal);
    for id in ProviderId::AUTO_ORDER {
        profile.grant_consent(id);
    }
    // Claude no instalado -> Auto cae a Codex.
    let (o, fxs) = orchestrator(false);
    let s: AiSuggestion = o
        .suggest_commit_message(&profile, ProviderChoice::Auto, &ctx(), DIFF, None)
        .await
        .unwrap();
    assert_eq!(s.provider, ProviderId::Codex);
    assert_eq!(s.status, SuggestionStatus::Pending);
    assert!(dumps(&fxs[2]).is_empty(), "Copilot no se debe llamar");
    // Elegir Claude explícitamente no cae a otro.
    let r = o
        .suggest_commit_message(&profile, ProviderChoice::Claude, &ctx(), DIFF, None)
        .await;
    assert!(matches!(r, Err(AiError::NotInstalled { .. })));

    // Con Claude instalado, Auto usa Claude.
    let (o, _fxs) = orchestrator(true);
    let s = o
        .suggest_commit_message(&profile, ProviderChoice::Auto, &ctx(), DIFF, None)
        .await
        .unwrap();
    assert_eq!(s.provider, ProviderId::Claude);
    assert!(!s.is_stale("rev-1") && s.is_stale("rev-2"));
}

/// Port de `testSettingsRoundTrip`.
#[test]
fn ajustes_ida_y_vuelta() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ai-settings.json");
    let s = AiSettings {
        provider: ProviderChoice::Codex,
        max_context_bytes: 2048,
        ..AiSettings::default()
    };
    s.save(&path).unwrap();
    assert_eq!(AiSettings::load(&path).unwrap(), s);
}
