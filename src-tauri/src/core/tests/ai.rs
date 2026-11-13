//! IA en el núcleo con un CLI falso (script de shell): la sugerencia solo rellena, nunca
//! toca el repo; el consentimiento, los filtros y la cancelación son reales.
//! Solo en unix: el CLI falso es un script `sh`.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ariadna_ai::resolve::SearchEnv;

use super::{wait_until, Env};
use crate::core::api::{AiCommitOptions, AiSettingsPatch, ApiSessionEventKind, AppEvent};
use crate::core::error::ApiError;

const REPLY: &str = r#"{"type":"result","subtype":"success","is_error":false,"result":"Añade el saludo\n\n- Crea saludo.txt con un texto de prueba."}"#;

/// Escribe un `claude` falso en `dir`. `body` es lo que hace cuando NO le piden `--version`.
fn fake_claude(dir: &Path, body: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("claude");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo \"claude 9.9.9 (fake)\"; exit 0; fi\n{body}\n"
    );
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Respuesta correcta que vuelca stdin y la lista de argumentos en `dump`.
fn ok_script(dump: &Path) -> String {
    format!(
        "cat > '{d}'\nprintf '%s\\n' \"$@\" > '{d}.args'\ncat <<'EOF'\n{REPLY}\nEOF",
        d = dump.display()
    )
}

struct Ai {
    env: Env,
    repo: PathBuf,
    tab: String,
    bin_dir: PathBuf,
    dump: PathBuf,
}

impl Ai {
    /// Repo con un commit, `claude` falso configurado y sin ningún otro CLI visible.
    async fn new(script: impl FnOnce(&Path) -> String) -> Self {
        let env = Env::new();
        let repo = env.repo("ia");
        let bin_dir = env.root.join("bin");
        let dump = env.root.join("stdin.txt");
        let bin = fake_claude(&bin_dir, &script(&dump));
        env.core.inner.ai.set_search_env(SearchEnv::default());
        env.core
            .ai_update_settings(AiSettingsPatch {
                binary_path: Some(crate::core::api::BinaryPathPatch {
                    id: "claude".into(),
                    path: Some(bin.to_string_lossy().into_owned()),
                }),
                ..AiSettingsPatch::default()
            })
            .await
            .unwrap();
        let tab = env.open(&repo).await;
        Self {
            env,
            repo,
            tab,
            bin_dir,
            dump,
        }
    }

    async fn stage(&self, rel: &str, contents: &[u8]) {
        self.env.write(&self.repo, rel, contents);
        self.env.git(&self.repo, &["add", rel]);
    }

    async fn generate(&self) -> Result<crate::core::api::AiCommitSuggestion, ApiError> {
        self.env
            .core
            .ai_generate_commit_message(&self.tab, AiCommitOptions::default())
            .await
    }

    fn head(&self) -> String {
        self.env.git(&self.repo, &["rev-parse", "HEAD"])
    }

    fn staged_names(&self) -> String {
        self.env
            .git(&self.repo, &["diff", "--cached", "--name-only"])
    }

    fn stdin(&self) -> String {
        std::fs::read_to_string(&self.dump).unwrap_or_default()
    }
}

#[tokio::test]
async fn propone_asunto_y_cuerpo_sin_tocar_el_repo() {
    let ai = Ai::new(ok_script).await;
    ai.stage("saludo.txt", b"hola mundo\n").await;
    let (head, staged) = (ai.head(), ai.staged_names());

    let s = ai.generate().await.unwrap();

    assert_eq!(s.subject, "Añade el saludo");
    assert!(s.body.contains("Crea saludo.txt"), "{}", s.body);
    assert_eq!(s.provider, "claude");
    assert!(!s.command_summary.contains("hola mundo"));
    // La IA solo propone: ni commit, ni cambios en lo preparado.
    assert_eq!(ai.head(), head);
    assert_eq!(ai.staged_names(), staged);
    assert_eq!(
        ai.env.git(&ai.repo, &["status", "--porcelain"]),
        "A  saludo.txt\n"
    );
    // El prompt viaja por stdin (no por argv) y lleva el diff preparado.
    let stdin = ai.stdin();
    assert!(stdin.contains("hola mundo"), "{stdin}");
    assert!(
        stdin.contains("inicial"),
        "los asuntos recientes van como pista de estilo"
    );
    let args = std::fs::read_to_string(format!("{}.args", ai.dump.display())).unwrap();
    assert!(!args.contains("hola mundo"), "el diff no debe ir en argv");
}

#[tokio::test]
async fn emite_eventos_de_progreso_de_la_operacion() {
    let ai = Ai::new(ok_script).await;
    ai.stage("a.txt", b"uno\n").await;
    ai.generate().await.unwrap();
    let kinds: Vec<_> = ai
        .env
        .sink
        .all()
        .into_iter()
        .filter_map(|e| match e {
            AppEvent::Session(s) => Some(s.kind),
            _ => None,
        })
        .collect();
    let started = kinds
        .iter()
        .position(|k| matches!(k, ApiSessionEventKind::OperationStarted { .. }))
        .expect("OperationStarted");
    let finished = kinds
        .iter()
        .position(|k| matches!(k, ApiSessionEventKind::OperationFinished { ok: true, .. }))
        .expect("OperationFinished ok");
    assert!(started < finished);
}

#[tokio::test]
async fn sin_nada_preparado_falla_y_no_llama_al_cli() {
    let ai = Ai::new(ok_script).await;
    ai.env.write(&ai.repo, "suelto.txt", b"sin preparar\n");
    let e = ai.generate().await.unwrap_err();
    assert_eq!(e.code, "AI_NO_STAGED_CHANGES");
    assert!(!ai.dump.exists(), "el CLI no debe ejecutarse");
    assert_eq!(ai.env.prompter.count(), 0, "ni preguntar el consentimiento");
}

#[tokio::test]
async fn ia_desactivada_no_ejecuta_nada() {
    let ai = Ai::new(ok_script).await;
    ai.stage("a.txt", b"uno\n").await;
    ai.env
        .core
        .ai_update_settings(AiSettingsPatch {
            enabled: Some(false),
            ..AiSettingsPatch::default()
        })
        .await
        .unwrap();
    let e = ai.generate().await.unwrap_err();
    assert_eq!(e.code, "AI_DISABLED");
    assert!(!ai.dump.exists());
    assert_eq!(ai.env.prompter.count(), 0);
}

#[tokio::test]
async fn el_consentimiento_se_pide_una_vez_y_si_se_rechaza_no_se_envia_nada() {
    let ai = Ai::new(ok_script).await;
    ai.stage("a.txt", b"uno\n").await;

    ai.env.prompter.reject_all();
    let e = ai.generate().await.unwrap_err();
    assert_eq!(e.code, "AI_CONSENT_REQUIRED");
    assert_eq!(ai.env.prompter.count(), 1);
    assert!(ai.env.prompter.last().title.contains("Claude"));
    assert!(!ai.dump.exists(), "sin permiso, el CLI no recibe nada");

    ai.env.prompter.accept_all();
    ai.generate().await.unwrap();
    assert_eq!(ai.env.prompter.count(), 2);
    ai.generate().await.unwrap();
    assert_eq!(
        ai.env.prompter.count(),
        2,
        "ya consentido: no se vuelve a preguntar"
    );
    // El consentimiento persiste en ai-settings.json.
    let saved = std::fs::read_to_string(ai.env.core.inner.ai.settings_file()).unwrap();
    assert!(saved.contains("claude"), "{saved}");
}

#[tokio::test]
async fn los_archivos_sensibles_nunca_salen_del_equipo() {
    let ai = Ai::new(ok_script).await;
    ai.stage(".env", b"TOKEN=super-secreto-del-env\n").await;
    ai.stage("lib.txt", b"codigo normal\n").await;
    ai.generate().await.unwrap();
    let stdin = ai.stdin();
    assert!(stdin.contains("codigo normal"));
    assert!(!stdin.contains("super-secreto-del-env"), "{stdin}");

    // Si TODO lo preparado es sensible: error propio y el CLI no vuelve a ejecutarse.
    let solo = Ai::new(ok_script).await;
    solo.stage(".env", b"TOKEN=otro-secreto\n").await;
    solo.stage("clave.pem", b"-----BEGIN PRIVATE KEY-----\nabc\n")
        .await;
    let e = solo.generate().await.unwrap_err();
    assert_eq!(e.code, "AI_SENSITIVE_FILE");
    assert!(!solo.dump.exists());
}

#[tokio::test]
async fn sin_proveedor_instalado_dice_cual_y_como_arreglarlo() {
    let ai = Ai::new(ok_script).await;
    ai.stage("a.txt", b"uno\n").await;
    std::fs::remove_file(ai.bin_dir.join("claude")).unwrap();
    let e = ai.generate().await.unwrap_err();
    assert!(
        matches!(e.code.as_str(), "AI_NOT_INSTALLED" | "COMMAND_FAILED"),
        "{e:?}"
    );
    assert!(!e.message.is_empty());
    assert_eq!(ai.env.prompter.count(), 0);
}

#[tokio::test]
async fn si_el_proveedor_falla_el_error_es_tipado() {
    let basura = Ai::new(|_| "cat > /dev/null\necho 'esto no es JSON'".to_owned()).await;
    basura.stage("a.txt", b"uno\n").await;
    assert_eq!(basura.generate().await.unwrap_err().code, "AI_BAD_OUTPUT");

    let cuota = Ai::new(|_| {
        "cat > /dev/null\necho 'usage limit reached, quota exceeded' >&2\nexit 1".to_owned()
    })
    .await;
    cuota.stage("a.txt", b"uno\n").await;
    let e = cuota.generate().await.unwrap_err();
    assert!(
        matches!(
            e.code.as_str(),
            "AI_QUOTA" | "COMMAND_FAILED" | "AI_NOT_LOGGED_IN"
        ),
        "{e:?}"
    );
    // Aun fallando, el repo sigue intacto.
    assert_eq!(cuota.staged_names(), "a.txt\n");
}

#[tokio::test]
async fn un_cli_colgado_acaba_en_timeout() {
    let ai = Ai::new(|_| "cat > /dev/null\nsleep 30".to_owned()).await;
    ai.stage("a.txt", b"uno\n").await;
    ai.env.core.inner.ai.set_timeout(Duration::from_millis(400));
    let t = Instant::now();
    let e = ai.generate().await.unwrap_err();
    assert_eq!(e.code, "AI_TIMEOUT", "{e:?}");
    assert!(t.elapsed() < Duration::from_secs(10));
    assert!(!ai.env.core.inner.ai.is_running(&ai.tab), "se da de baja");
}

#[tokio::test]
async fn cancelar_mata_el_proceso_y_devuelve_cancelled() {
    let ai = Ai::new(|_| "cat > /dev/null\nsleep 30".to_owned()).await;
    ai.stage("a.txt", b"uno\n").await;
    let core = ai.env.core.clone();
    let tab = ai.tab.clone();
    let task = tokio::spawn(async move {
        core.ai_generate_commit_message(&tab, AiCommitOptions::default())
            .await
    });
    wait_until("la generación se registra", || {
        ai.env.core.inner.ai.is_running(&ai.tab)
    })
    .await;
    let t = Instant::now();
    // Puede que aún no haya arrancado el hijo: se cancela hasta que haga efecto.
    assert!(ai.env.core.ai_cancel(&ai.tab).unwrap());
    let e = task.await.unwrap().unwrap_err();
    assert_eq!(e.code, "CANCELLED", "{e:?}");
    assert!(
        t.elapsed() < Duration::from_secs(10),
        "no esperó al sleep 30"
    );
    assert!(
        !ai.env.core.ai_cancel(&ai.tab).unwrap(),
        "ya no queda nada que cancelar"
    );
    // El resultado cancelado deja constancia en el evento de fin de operación.
    let finished_err = ai.env.sink.all().into_iter().any(|e| {
        matches!(e, AppEvent::Session(s)
            if matches!(s.kind, ApiSessionEventKind::OperationFinished { ok: false, .. }))
    });
    assert!(finished_err);
}

#[tokio::test]
async fn el_diff_enorme_se_rechaza_sin_llamar_al_cli() {
    let ai = Ai::new(ok_script).await;
    let big = "linea de relleno para superar el tope\n".repeat(4000);
    ai.stage("grande.txt", big.as_bytes()).await;
    let e = ai.generate().await.unwrap_err();
    assert_eq!(e.code, "AI_CONTEXT_TOO_LARGE", "{e:?}");
    assert!(!ai.dump.exists());
}

#[tokio::test]
async fn la_vista_previa_no_ejecuta_el_cli_ni_pregunta() {
    let ai = Ai::new(ok_script).await;
    ai.stage("a.txt", b"uno\n").await;
    ai.stage(".env", b"X=1\n").await;
    let p = ai.env.core.ai_preview_context(&ai.tab, None).await.unwrap();
    assert_eq!(p.files.len(), 1);
    assert_eq!(p.excluded.len(), 1);
    assert_eq!(p.provider.as_deref(), Some("claude"));
    assert!(!ai.dump.exists());
    assert_eq!(ai.env.prompter.count(), 0);
}

#[tokio::test]
async fn la_regla_de_perfil_decide_con_que_perfil_se_genera_y_donde_se_guarda_el_consentimiento() {
    let ai = Ai::new(ok_script).await;
    // Perfil elegido en Ajustes: Empresa (Claude desactivado). El remoto es de GitHub → Personal.
    ai.env
        .core
        .ai_update_settings(AiSettingsPatch {
            profile: Some("empresa".into()),
            ..AiSettingsPatch::default()
        })
        .await
        .unwrap();
    ai.env.git(
        &ai.repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/javie/ariadna.git",
        ],
    );
    ai.env.core.refresh(&ai.tab).await.unwrap();
    ai.stage("saludo.txt", b"hola\n").await;

    let s = ai
        .generate()
        .await
        .expect("la regla habilita Claude (perfil Personal)");
    assert_eq!(s.provider, "claude");
    let asked = ai.env.prompter.last();
    assert!(
        asked.message.contains("perfil Personal"),
        "{}",
        asked.message
    );

    // El consentimiento quedó en Personal, no en el perfil «activo» (Empresa).
    let stored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(ai.env.core.inner.ai.settings_file()).unwrap())
            .unwrap();
    assert_eq!(stored["activeProfile"], "empresa");
    assert_eq!(stored["personal"]["consent"], serde_json::json!(["claude"]));
    assert_eq!(stored["empresa"]["consent"], serde_json::json!([]));

    // Sin la regla, el mismo repo usa Empresa y Claude no está habilitado.
    let mut cfg = ai.env.core.get_settings().settings;
    cfg.profile_rules.clear();
    ai.env.core.save_settings(cfg).await.unwrap();
    ai.stage("otro.txt", b"otro\n").await;
    let e = ai.generate().await.unwrap_err();
    assert!(e.code.starts_with("AI_"), "{e:?}");
}

// ---------------------------------------------------------------- aislamiento, cuenta y login

use std::sync::{Arc, Mutex};

use ariadna_ai::login::TerminalLauncher;

use crate::core::api::{AiAccountsRequest, AiLoginRequest, AiPullRequestOptions, BinaryPathPatch};

/// Terminal falsa: guarda la ruta y el contenido del script en vez de abrir una ventana.
#[derive(Default)]
struct RecordingTerminal {
    opened: Mutex<Vec<(PathBuf, String)>>,
}

impl TerminalLauncher for RecordingTerminal {
    fn open(&self, script: &Path) -> Result<(), ariadna_ai::AiError> {
        let text = std::fs::read_to_string(script).unwrap();
        self.opened
            .lock()
            .unwrap()
            .push((script.to_path_buf(), text));
        Ok(())
    }
}

/// `claude` falso que responde a `auth status --json` con la cuenta y el directorio recibido.
fn account_script(dump: &Path) -> String {
    format!(
        "if [ \"$1\" = \"auth\" ]; then\n\
         printf '{{\"loggedIn\":true,\"authMethod\":\"claude.ai\",\"configDirectory\":\"%s\",\"email\":\"ana@empresa.com\",\"orgName\":\"Empresa SL\",\"subscriptionType\":\"team\"}}\\n' \"$CLAUDE_CONFIG_DIR\"\n\
         exit 0\nfi\n\
         printf '%s' \"$CLAUDE_CONFIG_DIR\" > '{d}.cfg'\n{}",
        ok_script(dump),
        d = dump.display()
    )
}

async fn patch(ai: &Ai, p: AiSettingsPatch) -> crate::core::api::AiState {
    ai.env.core.ai_update_settings(p).await.unwrap()
}

async fn use_claude_in(ai: &Ai, profile: &str) {
    patch(
        ai,
        AiSettingsPatch {
            profile: Some(profile.into()),
            provider_enabled: Some(crate::core::api::ProviderEnabledPatch {
                id: "claude".into(),
                enabled: true,
            }),
            ..AiSettingsPatch::default()
        },
    )
    .await;
}

fn cfg_seen(ai: &Ai) -> String {
    std::fs::read_to_string(format!("{}.cfg", ai.dump.display())).unwrap_or_default()
}

fn by_state<'a>(
    st: &'a crate::core::api::AiState,
    id: &str,
) -> &'a crate::core::api::AiProviderState {
    st.providers
        .iter()
        .find(|p| p.id.binary_name() == id)
        .unwrap()
}

#[tokio::test]
async fn empresa_aisla_la_sesion_y_personal_usa_la_normal_hasta_que_se_active() {
    let ai = Ai::new(account_script).await;
    ai.stage("a.txt", b"uno\n").await;
    let profiles = ai.env.core.inner.ai.profiles_dir();

    use_claude_in(&ai, "empresa").await;
    let s = ai.generate().await.unwrap();
    assert_eq!(s.provider, "claude");
    let empresa = profiles.join("empresa").join("claude");
    assert_eq!(cfg_seen(&ai), empresa.to_string_lossy());
    assert!(empresa.is_dir(), "el directorio aislado se crea al usarlo");

    // Personal conserva el login normal del CLI: ni variable ni directorio nuevo.
    patch(
        &ai,
        AiSettingsPatch {
            profile: Some("personal".into()),
            ..AiSettingsPatch::default()
        },
    )
    .await;
    std::fs::remove_file(format!("{}.cfg", ai.dump.display())).unwrap();
    ai.generate().await.unwrap();
    assert_eq!(cfg_seen(&ai), "");
    assert!(!profiles.join("personal").exists());

    // Activar el aislamiento de Personal le da su propio directorio, distinto del de Empresa.
    let st = patch(
        &ai,
        AiSettingsPatch {
            isolated: Some(true),
            ..AiSettingsPatch::default()
        },
    )
    .await;
    assert!(st.profile.isolated);
    ai.generate().await.unwrap();
    let personal = profiles.join("personal").join("claude");
    assert_eq!(cfg_seen(&ai), personal.to_string_lossy());
    assert_ne!(personal, empresa);

    // Desactivarlo en Empresa devuelve la sesión normal.
    use_claude_in(&ai, "empresa").await;
    patch(
        &ai,
        AiSettingsPatch {
            isolated: Some(false),
            ..AiSettingsPatch::default()
        },
    )
    .await;
    ai.generate().await.unwrap();
    assert_eq!(cfg_seen(&ai), "");
}

#[tokio::test]
async fn el_estado_avisa_de_los_proveedores_que_comparten_login() {
    let ai = Ai::new(ok_script).await;
    let st = patch(&ai, AiSettingsPatch::default()).await;
    // Personal: sin aislamiento por defecto.
    assert!(!st.profile.isolated);
    assert!(!by_state(&st, "claude").isolated && !by_state(&st, "claude").shares_login);
    assert!(by_state(&st, "copilot").shares_login);
    let st = patch(
        &ai,
        AiSettingsPatch {
            profile: Some("empresa".into()),
            ..AiSettingsPatch::default()
        },
    )
    .await;
    assert!(st.profile.isolated);
    assert!(by_state(&st, "codex").isolated && !by_state(&st, "codex").isolation_verified);
    assert!(by_state(&st, "claude").isolation_verified);
    // Copilot nunca se marca aislado aunque el perfil lo esté: comparte login.
    assert!(by_state(&st, "copilot").shares_login && !by_state(&st, "copilot").isolated);
}

#[tokio::test]
async fn la_cuenta_sale_del_cli_con_el_directorio_de_cada_perfil() {
    let ai = Ai::new(account_script).await;
    use_claude_in(&ai, "empresa").await;
    let empresa = ai
        .env
        .core
        .ai_accounts(AiAccountsRequest {
            profile: Some("empresa".into()),
            ..AiAccountsRequest::default()
        })
        .await
        .unwrap();
    let c = &empresa.accounts[0];
    assert_eq!(c.email.as_deref(), Some("ana@empresa.com"));
    assert_eq!(c.organization.as_deref(), Some("Empresa SL"));
    let dir = ai
        .env
        .core
        .inner
        .ai
        .profiles_dir()
        .join("empresa")
        .join("claude");
    assert_eq!(
        c.config_dir.as_deref(),
        Some(dir.to_string_lossy().as_ref())
    );
    assert!(empresa.profile.isolated);
    // Codex y Copilot: «no disponible» y sin lanzar nada.
    assert_eq!(empresa.accounts.len(), 3);
    for a in &empresa.accounts[1..] {
        assert_eq!(a.state, ariadna_ai::AccountState::Unavailable);
    }

    let personal = ai
        .env
        .core
        .ai_accounts(AiAccountsRequest {
            profile: Some("personal".into()),
            ..AiAccountsRequest::default()
        })
        .await
        .unwrap();
    assert_eq!(
        personal.accounts[0].config_dir, None,
        "usa la sesión normal"
    );

    // Por pestaña, el perfil sale de la regla repo → perfil (aquí ninguna: el activo).
    let by_tab = ai
        .env
        .core
        .ai_accounts(AiAccountsRequest {
            tab_id: Some(ai.tab.clone()),
            ..AiAccountsRequest::default()
        })
        .await
        .unwrap();
    assert_eq!(by_tab.profile.name, "Empresa");
    assert!(ai
        .env
        .core
        .ai_accounts(AiAccountsRequest {
            profile: Some("otro".into()),
            ..AiAccountsRequest::default()
        })
        .await
        .is_err());
}

fn login_req(profile: &str, provider: &str, launch: bool) -> AiLoginRequest {
    AiLoginRequest {
        tab_id: None,
        profile: Some(profile.into()),
        provider: provider.into(),
        launch,
    }
}

#[tokio::test]
async fn el_login_usa_el_entorno_aislado_y_nunca_pide_credenciales() {
    let ai = Ai::new(ok_script).await;
    let term = Arc::new(RecordingTerminal::default());
    ai.env.core.inner.ai.set_launcher(term.clone());
    use_claude_in(&ai, "empresa").await;

    // Solo mostrar el comando: no se abre nada.
    let shown = ai
        .env
        .core
        .ai_login(login_req("empresa", "claude", false))
        .await
        .unwrap();
    assert!(!shown.launched && term.opened.lock().unwrap().is_empty());
    let dir = ai
        .env
        .core
        .inner
        .ai
        .profiles_dir()
        .join("empresa")
        .join("claude");
    assert_eq!(
        shown.config_dir.as_deref(),
        Some(dir.to_string_lossy().as_ref())
    );
    assert_eq!(
        shown.command.as_deref(),
        Some(format!("CLAUDE_CONFIG_DIR='{}' claude auth login", dir.display()).as_str())
    );
    assert!(shown.isolated && !shown.shares_login && shown.verified);

    // Abrir la terminal: el script fija el directorio, limpia claves ajenas y no lleva datos del repo.
    let opened = ai
        .env
        .core
        .ai_login(login_req("empresa", "claude", true))
        .await
        .unwrap();
    assert!(opened.launched, "{:?}", opened.note);
    let (path, text) = term.opened.lock().unwrap()[0].clone();
    assert!(path.starts_with(ai.env.core.inner.ai.profiles_dir().join("empresa")));
    assert!(text.contains(&format!("export CLAUDE_CONFIG_DIR='{}'", dir.display())));
    assert!(text.contains("unset ANTHROPIC_API_KEY"));
    assert!(text.contains(&format!(
        "{}' auth login",
        ai.bin_dir.join("claude").display()
    )));
    assert!(!text.contains(&ai.repo.to_string_lossy().into_owned()));

    // Personal (sin aislar): comando simple, sin variable ni directorio.
    let p = ai
        .env
        .core
        .ai_login(login_req("personal", "claude", false))
        .await
        .unwrap();
    assert_eq!(p.command.as_deref(), Some("claude auth login"));
    assert!(p.config_dir.is_none() && !p.isolated);
    assert!(p.note.unwrap().contains("sesión normal"));
}

#[tokio::test]
async fn copilot_comparte_login_y_el_login_avisa() {
    let ai = Ai::new(ok_script).await;
    let bin = fake_claude(&ai.env.root.join("cp"), "exit 0");
    patch(
        &ai,
        AiSettingsPatch {
            binary_path: Some(BinaryPathPatch {
                id: "copilot".into(),
                path: Some(bin.to_string_lossy().into_owned()),
            }),
            ..AiSettingsPatch::default()
        },
    )
    .await;
    let term = Arc::new(RecordingTerminal::default());
    ai.env.core.inner.ai.set_launcher(term.clone());
    let info = ai
        .env
        .core
        .ai_login(login_req("empresa", "copilot", true))
        .await
        .unwrap();
    assert!(info.shares_login && !info.isolated && info.config_dir.is_none());
    assert!(!info.verified);
    assert!(
        info.note.as_deref().unwrap().contains("comparten"),
        "{info:?}"
    );
    assert_eq!(info.command.as_deref(), Some("copilot login"));
    let text = term.opened.lock().unwrap()[0].1.clone();
    assert!(!text.contains("export CLAUDE_CONFIG_DIR") && !text.contains("export CODEX_HOME"));
}

#[tokio::test]
async fn login_con_errores_claros() {
    let ai = Ai::new(ok_script).await;
    let term = Arc::new(RecordingTerminal::default());
    ai.env.core.inner.ai.set_launcher(term.clone());
    // Codex no está instalado.
    let e = ai
        .env
        .core
        .ai_login(login_req("personal", "codex", true))
        .await
        .unwrap_err();
    assert_eq!(e.code, "AI_NOT_INSTALLED");
    // Claude está desactivado en Empresa.
    let e = ai
        .env
        .core
        .ai_login(login_req("empresa", "claude", true))
        .await
        .unwrap_err();
    assert!(e.code.starts_with("AI_"), "{e:?}");
    // Proveedor o perfil desconocidos.
    for (profile, provider) in [("personal", "gemini"), ("jefe", "claude")] {
        let e = ai
            .env
            .core
            .ai_login(login_req(profile, provider, false))
            .await
            .unwrap_err();
        assert_eq!(e.code, "PARSE");
    }
    assert!(term.opened.lock().unwrap().is_empty());
}

// ---------------------------------------------------------------- título y descripción de PR

const PR_REPLY: &str = r#"{"type":"result","subtype":"success","is_error":false,"result":"Añade el módulo de acceso\n\n## Resumen\n\nNuevo módulo.\n\n- Crea acceso.txt"}"#;

fn pr_script(dump: &Path) -> String {
    format!(
        "cat > '{d}'\nprintf '%s\\n' \"$@\" > '{d}.args'\ncat <<'EOF'\n{PR_REPLY}\nEOF",
        d = dump.display()
    )
}

impl Ai {
    /// Rama `feature` con dos commits sobre `main`; `main` avanza después con otro archivo.
    fn branch_with_commits(&self) {
        let (e, r) = (&self.env, &self.repo);
        e.git(r, &["checkout", "-q", "-b", "feature"]);
        e.write(r, "acceso.txt", b"login con usuario\n");
        e.git(r, &["add", "-A"]);
        e.git(
            r,
            &[
                "commit",
                "-q",
                "-m",
                "Añade el módulo de acceso",
                "-m",
                "Detalle del acceso.",
            ],
        );
        e.write(r, "tokens.rs", b"let k = \"AKIAABCDEFGHIJKLMNOP\";\n");
        e.write(r, ".env", b"SECRETO_ENV=1\n");
        e.git(r, &["add", "-f", "-A"]);
        e.git(r, &["commit", "-q", "-m", "Corrige la configuración"]);
        e.git(r, &["checkout", "-q", "main"]);
        e.write(r, "solo-main.txt", b"cambio ajeno\n");
        e.git(r, &["add", "-A"]);
        e.git(r, &["commit", "-q", "-m", "avanza main"]);
        e.git(r, &["checkout", "-q", "feature"]);
    }

    async fn pr(
        &self,
        base: Option<&str>,
    ) -> Result<crate::core::api::AiPullRequestSuggestion, ApiError> {
        self.env
            .core
            .ai_generate_pull_request(
                &self.tab,
                AiPullRequestOptions {
                    base: base.map(String::from),
                    ..AiPullRequestOptions::default()
                },
            )
            .await
    }
}

#[tokio::test]
async fn propone_titulo_y_descripcion_de_pr_sin_tocar_el_repo() {
    let ai = Ai::new(pr_script).await;
    ai.branch_with_commits();
    ai.env.core.refresh(&ai.tab).await.unwrap();
    let head = ai.head();
    let status = ai.env.git(&ai.repo, &["status", "--porcelain"]);

    let s = ai.pr(None).await.unwrap();
    assert_eq!(s.title, "Añade el módulo de acceso");
    assert!(s.body.contains("## Resumen") && s.body.contains("- Crea acceso.txt"));
    assert_eq!(
        (s.provider.as_str(), s.base.as_str(), s.head.as_str()),
        ("claude", "main", "feature")
    );
    assert_eq!(s.commit_count, 2);

    // Solo propone: ni commits ni cambios.
    assert_eq!(ai.head(), head);
    assert_eq!(ai.env.git(&ai.repo, &["status", "--porcelain"]), status);

    let stdin = ai.stdin();
    // Commits (asunto y cuerpo) y diff contra el ancestro común…
    assert!(stdin.contains("Añade el módulo de acceso") && stdin.contains("Detalle del acceso."));
    assert!(stdin.contains("login con usuario"));
    assert!(stdin.contains("feature") && stdin.contains("BASE BRANCH"));
    // …sin lo que main añadió después, sin .env y con los secretos redactados.
    assert!(!stdin.contains("cambio ajeno"), "{stdin}");
    assert!(!stdin.contains("SECRETO_ENV"));
    assert!(!stdin.contains("AKIAABCDEFGHIJKLMNOP"));
    assert!(stdin.contains("[REDACTED]"));
    // Por stdin, nunca por argv.
    let args = std::fs::read_to_string(format!("{}.args", ai.dump.display())).unwrap();
    assert!(!args.contains("acceso") && !args.contains("feature"));
    // El consentimiento nombra la PR.
    let asked = ai.env.prompter.last();
    assert!(asked.message.contains("pull request"), "{}", asked.message);
}

#[tokio::test]
async fn pr_respeta_el_consentimiento_y_la_desactivacion() {
    let ai = Ai::new(pr_script).await;
    ai.branch_with_commits();
    ai.env.core.refresh(&ai.tab).await.unwrap();
    ai.env.prompter.reject_all();
    let e = ai.pr(Some("main")).await.unwrap_err();
    assert_eq!(e.code, "AI_CONSENT_REQUIRED");
    assert_eq!(ai.stdin(), "", "no se lanzó el CLI");

    patch(
        &ai,
        AiSettingsPatch {
            enabled: Some(false),
            ..AiSettingsPatch::default()
        },
    )
    .await;
    assert_eq!(ai.pr(Some("main")).await.unwrap_err().code, "AI_DISABLED");
}

#[tokio::test]
async fn pr_errores_de_base_y_ramas_sin_cambios() {
    let ai = Ai::new(pr_script).await;
    // En `main` no hay base distinta de la propia rama.
    assert_eq!(ai.pr(None).await.unwrap_err().code, "AI_BASE_NOT_FOUND");
    ai.env.git(&ai.repo, &["checkout", "-q", "-b", "vacia"]);
    ai.env.core.refresh(&ai.tab).await.unwrap();
    // Una rama sin commits propios no tiene nada que describir.
    assert_eq!(
        ai.pr(Some("main")).await.unwrap_err().code,
        "AI_NO_BRANCH_CHANGES"
    );
    // Base inexistente o con forma sospechosa: nunca llega a git.
    assert_eq!(
        ai.pr(Some("no-existe")).await.unwrap_err().code,
        "AI_BASE_NOT_FOUND"
    );
    for bad in ["--upload-pack=x", "a..b", "-p", "main;ls", "a b"] {
        assert_eq!(ai.pr(Some(bad)).await.unwrap_err().code, "PARSE", "{bad}");
    }
    assert_eq!(ai.stdin(), "");
    assert_eq!(ai.env.prompter.count(), 0);
}

#[tokio::test]
async fn pr_solo_con_archivos_sensibles_no_envia_nada() {
    let ai = Ai::new(pr_script).await;
    let (e, r) = (&ai.env, &ai.repo);
    e.git(r, &["checkout", "-q", "-b", "solo-env"]);
    e.write(r, ".env", b"CLAVE=1\n");
    e.git(r, &["add", "-f", ".env"]);
    e.git(r, &["commit", "-q", "-m", "config"]);
    ai.env.core.refresh(&ai.tab).await.unwrap();
    assert_eq!(
        ai.pr(Some("main")).await.unwrap_err().code,
        "AI_SENSITIVE_FILE"
    );
    assert_eq!(ai.stdin(), "");
}

// ------------------------------------------------------------------ conflictos (tres versiones)

use ariadna_git::MergeFastForward;

use crate::core::api::AiConflictSuggestion;

/// JSON de respuesta del CLI falso con un `---FILE---`/`---RATIONALE---` ya montado.
fn conflict_reply(content: &str) -> String {
    let result = format!("---FILE---\n{content}---RATIONALE---\nSe combinan los dos cambios.\n");
    serde_json::json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "result": result
    })
    .to_string()
}

/// Script que contesta `json` (guardado junto al `dump`) y anota stdin.
fn conflict_script(json: String) -> impl FnOnce(&Path) -> String {
    move |dump: &Path| {
        let reply = dump.with_extension("json");
        std::fs::write(&reply, &json).unwrap();
        format!(
            "cat > '{d}'\nprintf '%s\n' \"$@\" > '{d}.args'\ncat '{r}'",
            d = dump.display(),
            r = reply.display()
        )
    }
}

impl Ai {
    /// `main` y `feature` cambian la misma línea de `rel`: el merge queda en curso con conflicto.
    async fn conflicted_file(
        script: impl FnOnce(&Path) -> String,
        rel: &str,
        base: &[u8],
        theirs: &[u8],
        ours: &[u8],
    ) -> Self {
        let env = Env::new();
        let repo = env.repo("conflicto");
        let bin_dir = env.root.join("bin");
        let dump = env.root.join("stdin.txt");
        let bin = fake_claude(&bin_dir, &script(&dump));
        env.core.inner.ai.set_search_env(SearchEnv::default());
        env.core
            .ai_update_settings(AiSettingsPatch {
                binary_path: Some(crate::core::api::BinaryPathPatch {
                    id: "claude".into(),
                    path: Some(bin.to_string_lossy().into_owned()),
                }),
                ..AiSettingsPatch::default()
            })
            .await
            .unwrap();
        let commit = |bytes: &[u8], msg: &str| {
            env.write(&repo, rel, bytes);
            env.git(&repo, &["add", "-A"]);
            env.git(&repo, &["commit", "-q", "-m", msg]);
        };
        commit(base, "base");
        env.git(&repo, &["checkout", "-q", "-b", "feature"]);
        commit(theirs, "feature");
        env.git(&repo, &["checkout", "-q", "main"]);
        commit(ours, "main");
        let tab = env.open(&repo).await;
        let ai = Self {
            env,
            repo,
            tab,
            bin_dir,
            dump,
        };
        ai.env
            .core
            .merge_start(&ai.tab, "feature".into(), MergeFastForward::Allow)
            .await
            .unwrap();
        ai
    }

    /// Conflicto de texto en `c.txt` (la línea del medio cambia en los dos lados).
    async fn conflicted(script: impl FnOnce(&Path) -> String) -> Self {
        Self::conflicted_file(
            script,
            "c.txt",
            b"uno\ndos\ntres\n",
            b"uno\nDOS-feature\ntres\n",
            b"uno\nDOS-main\ntres\n",
        )
        .await
    }

    async fn resolve(&self, path: &str) -> Result<AiConflictSuggestion, ApiError> {
        self.env
            .core
            .ai_resolve_conflict(&self.tab, path.into())
            .await
    }

    fn disk(&self, rel: &str) -> Vec<u8> {
        std::fs::read(self.repo.join(rel)).unwrap()
    }
}

#[tokio::test]
async fn propone_el_archivo_resuelto_sin_escribirlo() {
    let ai = Ai::conflicted(conflict_script(conflict_reply(
        "uno\nDOS-main\ny DOS-feature\ntres\n",
    )))
    .await;
    let before = ai.disk("c.txt");
    let index_before = ai.env.git(&ai.repo, &["ls-files", "--unmerged", "c.txt"]);

    let s = ai.resolve("c.txt").await.unwrap();

    assert_eq!(s.path.to_string_lossy(), "c.txt");
    assert_eq!(s.content, "uno\nDOS-main\ny DOS-feature\ntres\n");
    assert_eq!(s.rationale.as_deref(), Some("Se combinan los dos cambios."));
    assert_eq!(s.provider, "claude");
    assert!(!s.command_summary.contains("DOS-main"));
    // Nada escrito ni marcado como resuelto: el archivo conserva los marcadores y el índice
    // sigue con las etapas 1/2/3.
    assert_eq!(ai.disk("c.txt"), before);
    assert!(String::from_utf8_lossy(&before).contains("<<<<<<<"));
    assert_eq!(
        ai.env.git(&ai.repo, &["ls-files", "--unmerged", "c.txt"]),
        index_before
    );
    // El prompt lleva las tres versiones y va por stdin, no por argv.
    let stdin = ai.stdin();
    for side in ["DOS-main", "DOS-feature", "uno", "tres"] {
        assert!(stdin.contains(side), "falta {side} en el prompt");
    }
    let args = std::fs::read_to_string(format!("{}.args", ai.dump.display())).unwrap();
    assert!(!args.contains("DOS-main"));
    // Un solo diálogo de consentimiento, del perfil y proveedor que responde.
    assert_eq!(ai.env.prompter.count(), 1);
    assert!(ai.env.prompter.last().message.contains("perfil Personal"));
}

#[tokio::test]
async fn una_propuesta_con_marcadores_se_rechaza() {
    let ai = Ai::conflicted(conflict_script(conflict_reply(
        "uno\n<<<<<<< HEAD\nDOS-main\n=======\nDOS-feature\n>>>>>>> feature\ntres\n",
    )))
    .await;
    let e = ai.resolve("c.txt").await.unwrap_err();
    assert_eq!(e.code, "AI_CONFLICT_INVALID_PROPOSAL", "{e:?}");
    assert!(!e.message.contains("DOS-main"), "sin contenido en el error");
    assert!(String::from_utf8_lossy(&ai.disk("c.txt")).contains("<<<<<<<"));
}

#[tokio::test]
async fn una_propuesta_que_toca_fuera_de_los_bloques_se_rechaza() {
    let ai = Ai::conflicted(conflict_script(conflict_reply(
        "OTRA CABECERA\nDOS-main\ntres\n",
    )))
    .await;
    let e = ai.resolve("c.txt").await.unwrap_err();
    assert_eq!(e.code, "AI_CONFLICT_INVALID_PROPOSAL", "{e:?}");
}

#[tokio::test]
async fn un_conflicto_binario_no_se_envia() {
    let ai = Ai::conflicted_file(
        ok_script,
        "logo.png",
        b"\x00\x01base\x00",
        b"\x00\x01suyo\x00",
        b"\x00\x01nuestro\x00",
    )
    .await;
    let e = ai.resolve("logo.png").await.unwrap_err();
    assert_eq!(e.code, "AI_CONFLICT_NOT_TEXT", "{e:?}");
    assert!(!ai.dump.exists(), "no se llamó al CLI");
    assert_eq!(ai.env.prompter.count(), 0, "ni se preguntó nada");
}

#[tokio::test]
async fn un_archivo_sensible_no_se_envia() {
    let ai = Ai::conflicted_file(ok_script, ".env", b"A=1\n", b"A=2\n", b"A=3\n").await;
    let e = ai.resolve(".env").await.unwrap_err();
    assert_eq!(e.code, "AI_SENSITIVE_FILE", "{e:?}");
    assert!(!ai.dump.exists());
}

#[tokio::test]
async fn con_la_ia_desactivada_no_se_llama_al_cli() {
    let ai = Ai::conflicted(ok_script).await;
    ai.env
        .core
        .ai_update_settings(AiSettingsPatch {
            enabled: Some(false),
            ..AiSettingsPatch::default()
        })
        .await
        .unwrap();
    let e = ai.resolve("c.txt").await.unwrap_err();
    assert_eq!(e.code, "AI_DISABLED", "{e:?}");
    assert!(!ai.dump.exists());
}

#[tokio::test]
async fn sin_consentimiento_no_se_envia_nada() {
    let ai = Ai::conflicted(ok_script).await;
    ai.env.prompter.reject_all();
    let e = ai.resolve("c.txt").await.unwrap_err();
    assert_eq!(e.code, "AI_CONSENT_REQUIRED", "{e:?}");
    assert!(!ai.dump.exists());
    assert!(String::from_utf8_lossy(&ai.disk("c.txt")).contains("<<<<<<<"));
}

#[tokio::test]
async fn un_archivo_que_ya_no_esta_en_conflicto_da_conflict_not_found() {
    let ai = Ai::conflicted(ok_script).await;
    let e = ai.resolve("README.md").await.unwrap_err();
    assert_eq!(e.code, "CONFLICT_NOT_FOUND", "{e:?}");
}

#[tokio::test]
async fn cancelar_la_resolucion_mata_el_proceso_y_no_toca_nada() {
    let ai = Ai::conflicted(|_| "cat > /dev/null\nsleep 30".to_owned()).await;
    let core = ai.env.core.clone();
    let tab = ai.tab.clone();
    let task = tokio::spawn(async move { core.ai_resolve_conflict(&tab, "c.txt".into()).await });
    wait_until("la resolución se registra", || {
        ai.env.core.inner.ai.is_running(&ai.tab)
    })
    .await;
    let t = Instant::now();
    assert!(ai.env.core.ai_cancel(&ai.tab).unwrap());
    let e = task.await.unwrap().unwrap_err();
    assert_eq!(e.code, "CANCELLED", "{e:?}");
    assert!(
        t.elapsed() < Duration::from_secs(10),
        "no esperó al sleep 30"
    );
    assert!(String::from_utf8_lossy(&ai.disk("c.txt")).contains("<<<<<<<"));
}

#[tokio::test]
async fn un_cli_colgado_en_un_conflicto_acaba_en_timeout() {
    let ai = Ai::conflicted(|_| "cat > /dev/null\nsleep 30".to_owned()).await;
    ai.env.core.inner.ai.set_timeout(Duration::from_millis(400));
    let t = Instant::now();
    let e = ai.resolve("c.txt").await.unwrap_err();
    assert_eq!(e.code, "AI_TIMEOUT", "{e:?}");
    assert!(t.elapsed() < Duration::from_secs(10));
    assert!(!ai.env.core.inner.ai.is_running(&ai.tab), "se da de baja");
}
