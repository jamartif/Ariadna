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
