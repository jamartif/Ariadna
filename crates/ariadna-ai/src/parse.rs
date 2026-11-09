//! Parsers de la salida de cada CLI (RF-IA7).
//!
//! Una salida que no se reconoce produce [`AiError::BadOutput`] con el texto fijo
//! «formato no reconocido»: **nunca se devuelve texto crudo a la UI**. Los errores de
//! login y de cuota se detectan por patrones y llevan el comando de login.

use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use crate::models::{AiError, ProviderId};

/// Mensaje fijo de [`AiError::BadOutput`] cuando el formato no es el esperado.
pub const UNRECOGNIZED: &str = "formato no reconocido";

/// Respuesta ya extraída de la salida del CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedReply {
    pub text: String,
    /// Nota informativa (p. ej. tokens).
    pub note: Option<String>,
}

fn bad() -> AiError {
    AiError::BadOutput(UNRECOGNIZED.to_string())
}

/// Recorta un detalle de error a `limit` caracteres.
pub fn truncate_detail(text: &str, limit: usize) -> String {
    let t = text.trim();
    if t.chars().count() <= limit {
        t.to_string()
    } else {
        let mut s: String = t.chars().take(limit).collect();
        s.push('…');
        s
    }
}

fn regexes() -> &'static (Regex, Regex, Regex) {
    static R: OnceLock<(Regex, Regex, Regex)> = OnceLock::new();
    R.get_or_init(|| {
        (
            Regex::new(
                r"(?i)quota|rate.?limit|usage limit|limit reached|too many requests|insufficient_quota|credit balance|\b429\b",
            )
            .expect("regex fija"),
            Regex::new(
                r"(?i)not logged in|not authenticated|please log ?in|please run `?/login|/login\b|login required|authentication required|authentication_error|unauthorized|invalid api key|invalid credentials|sign in to|you need to log in|codex login|copilot login|chatgpt\.com/auth|\b401\b",
            )
            .expect("regex fija"),
            Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").expect("regex fija"),
        )
    })
}

/// Detecta errores de cuota o de sesión en un texto de error del CLI.
///
/// Solo debe usarse sobre salidas de **fallo** (código distinto de 0 o `is_error`):
/// un diff que mencione «unauthorized» no es un error.
pub fn classify_failure(provider: ProviderId, text: &str) -> Option<AiError> {
    let (quota, login, _) = regexes();
    if quota.is_match(text) {
        return Some(AiError::Quota {
            provider,
            detail: truncate_detail(text, 400),
        });
    }
    if login.is_match(text) {
        return Some(AiError::NotLoggedIn {
            provider,
            login_command: provider.login_command().to_string(),
        });
    }
    None
}

fn failure_error(provider: ProviderId, exit_code: Option<i32>, text: &str) -> AiError {
    classify_failure(provider, text).unwrap_or_else(|| AiError::ProviderFailed {
        provider,
        exit_code,
        detail: truncate_detail(text, 2_000),
    })
}

/// Primer objeto JSON de `text` que cumpla `accept`, tolerando banners antes y después.
fn find_json_object(text: &str, accept: impl Fn(&Value) -> bool) -> Option<Value> {
    let trimmed = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        if v.is_object() && accept(&v) {
            return Some(v);
        }
    }
    for (i, _) in trimmed.match_indices('{').take(256) {
        let mut it = serde_json::Deserializer::from_str(&trimmed[i..]).into_iter::<Value>();
        if let Some(Ok(v)) = it.next() {
            if v.is_object() && accept(&v) {
                return Some(v);
            }
        }
    }
    None
}

/// Claude `--output-format json`: objeto con `result` (texto) e `is_error` (bool).
/// Tolera campos extra y líneas de aviso antes del JSON.
pub fn parse_claude(stdout: &str) -> Result<ParsedReply, AiError> {
    let v = find_json_object(stdout, |v| {
        v.get("result").map(Value::is_string).unwrap_or(false)
    })
    .ok_or_else(bad)?;
    let result = v.get("result").and_then(Value::as_str).unwrap_or_default();
    let is_error = v.get("is_error").and_then(Value::as_bool).unwrap_or(false);
    if is_error {
        return Err(failure_error(ProviderId::Claude, None, result));
    }
    if result.trim().is_empty() {
        return Err(AiError::BadOutput("respuesta vacía".into()));
    }
    let note = v.get("usage").and_then(format_usage("Claude"));
    Ok(ParsedReply {
        text: result.to_string(),
        note,
    })
}

fn format_usage(name: &'static str) -> impl Fn(&Value) -> Option<String> {
    move |u: &Value| {
        let input = u.get("input_tokens").and_then(Value::as_u64);
        let output = u.get("output_tokens").and_then(Value::as_u64);
        if input.is_none() && output.is_none() {
            return None;
        }
        Some(format!(
            "{name}: {} tokens de entrada, {} de salida.",
            input.unwrap_or(0),
            output.unwrap_or(0)
        ))
    }
}

fn codex_agent_text(event: &Value) -> Option<String> {
    let item = event.get("item")?;
    let ty = item
        .get("type")
        .or_else(|| item.get("item_type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if ty == "agent_message" || ty == "assistant_message" {
        item.get("text").and_then(Value::as_str).map(String::from)
    } else {
        None
    }
}

fn codex_message(event: &Value) -> Option<String> {
    if let Some(m) = event.get("message").and_then(Value::as_str) {
        return Some(m.to_string());
    }
    match event.get("error") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(e) => e.get("message").and_then(Value::as_str).map(String::from),
        None => None,
    }
}

/// Codex `exec --json`: JSONL de eventos. Extrae el último mensaje del agente y
/// tolera eventos desconocidos. **NO VERIFICADO** contra el CLI real.
pub fn parse_codex(stdout: &str) -> Result<ParsedReply, AiError> {
    let mut messages: Vec<String> = Vec::new();
    let (mut failed, mut completed, mut saw_json) = (false, false, false);
    let mut failure: Option<String> = None;
    let mut note = None;

    for line in stdout.lines() {
        let Ok(ev) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if !ev.is_object() {
            continue;
        }
        saw_json = true;
        match ev.get("type").and_then(Value::as_str).unwrap_or_default() {
            "error" | "turn.failed" => {
                failed = true;
                failure = codex_message(&ev).or(failure);
            }
            "turn.completed" => {
                completed = true;
                note = ev.get("usage").and_then(format_usage("Codex"));
            }
            "item.completed" | "item.updated" => {
                if let Some(t) = codex_agent_text(&ev) {
                    messages.push(t);
                }
            }
            _ => {}
        }
    }
    if failed && !completed {
        return Err(failure_error(
            ProviderId::Codex,
            None,
            failure.as_deref().unwrap_or("el turno terminó con error"),
        ));
    }
    if !saw_json {
        return Err(bad());
    }
    match messages.last() {
        Some(m) if !m.trim().is_empty() => Ok(ParsedReply {
            text: m.clone(),
            note,
        }),
        _ => Err(AiError::BadOutput("respuesta vacía".into())),
    }
}

/// Copilot `-s`: texto plano. Quita secuencias ANSI. **NO VERIFICADO**.
pub fn parse_copilot(stdout: &str) -> Result<ParsedReply, AiError> {
    let clean = regexes().2.replace_all(stdout, "");
    let text = clean.trim();
    if text.is_empty() {
        return Err(AiError::BadOutput("respuesta vacía".into()));
    }
    // Un error con código 0 suele ser corto y empezar por «Error»: no es una respuesta.
    if text.len() < 400 && text.to_ascii_lowercase().starts_with("error") {
        return Err(failure_error(ProviderId::Copilot, None, text));
    }
    Ok(ParsedReply {
        text: text.to_string(),
        note: None,
    })
}

/// Despacha al parser del proveedor.
pub fn parse_output(provider: ProviderId, stdout: &str) -> Result<ParsedReply, AiError> {
    match provider {
        ProviderId::Claude => parse_claude(stdout),
        ProviderId::Codex => parse_codex(stdout),
        ProviderId::Copilot => parse_copilot(stdout),
    }
}

/// Interpreta un fallo del proceso (código distinto de 0) usando stdout y stderr.
pub fn error_from_failed_run(
    provider: ProviderId,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> AiError {
    // Claude/Codex describen el error en un JSON de stdout.
    if let Err(e) = parse_output(provider, stdout) {
        if !matches!(e, AiError::BadOutput(_)) {
            return e;
        }
    }
    let blob = format!("{stderr}\n{stdout}");
    let detail = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    classify_failure(provider, &blob).unwrap_or_else(|| AiError::ProviderFailed {
        provider,
        exit_code,
        detail: truncate_detail(detail, 2_000),
    })
}

// ------------------------------------------------------------------ texto de respuesta

/// Quita una valla de código markdown que envuelva toda la respuesta.
pub fn strip_fences(text: &str) -> String {
    let t = text.trim();
    if !t.starts_with("```") {
        return t.to_string();
    }
    let Some(nl) = t.find('\n') else {
        return t.to_string();
    };
    let mut rest = &t[nl + 1..];
    if let Some(f) = rest.rfind("```") {
        rest = &rest[..f];
    }
    rest.trim().to_string()
}

struct CommitRegexes {
    preamble: Regex,
    subject_label: Regex,
    body_label: Regex,
    heading: Regex,
}

fn commit_regexes() -> &'static CommitRegexes {
    static R: OnceLock<CommitRegexes> = OnceLock::new();
    R.get_or_init(|| CommitRegexes {
        preamble: Regex::new(
            r"(?i)^(here(?:'s| is| are)\b|sure\b|certainly\b|of course\b|aqu[ií]\b|claro\b|esta es\b|este es\b)",
        )
        .expect("regex fija"),
        subject_label: Regex::new(
            r"(?i)^(?:asunto|subject|title|t[ií]tulo|commit message|commit|mensaje de commit|mensaje|summary|resumen)\s*[:：]\s*",
        )
        .expect("regex fija"),
        body_label: Regex::new(
            r"(?i)^(?:cuerpo|body|description|descripci[oó]n|details|detalles)\s*[:：]\s*(.*)$",
        )
        .expect("regex fija"),
        heading: Regex::new(r"^#{1,6}\s+").expect("regex fija"),
    })
}

/// Primer bloque con valla de código, aunque haya una frase de introducción delante
/// (hasta dos líneas). Sin valla de cierre, hasta el final.
fn fenced_block(text: &str) -> Option<String> {
    let start = text.find("```")?;
    let before = text[..start].trim();
    if before.lines().count() > 2 {
        return None;
    }
    let after = &text[start + 3..];
    // La primera línea tras la valla es el lenguaje (`text`, `gitcommit`…) y se descarta.
    let rest = &after[after.find('\n')? + 1..];
    let body = rest.find("```").map_or(rest, |end| &rest[..end]);
    Some(body.trim().to_string())
}

const QUOTES: [(char, char); 5] = [('"', '"'), ('\'', '\''), ('“', '”'), ('«', '»'), ('`', '`')];

/// Quita un par de comillas que envuelva todo el texto.
fn strip_wrapping_quotes(text: &str) -> &str {
    let t = text.trim();
    for (open, close) in QUOTES {
        if t.chars().count() >= 2 && t.starts_with(open) && t.ends_with(close) {
            let inner = &t[open.len_utf8()..t.len() - close.len_utf8()];
            // Si dentro hay otra comilla de cierre, no era un envoltorio del conjunto.
            if !inner.contains(close) || (open != close && !inner.contains(open)) {
                return inner.trim();
            }
        }
    }
    t
}

/// Limpia la respuesta: valla de código, frase de introducción y comillas de conjunto.
fn clean_commit_text(raw: &str) -> String {
    let normalized = raw.replace("\r\n", "\n");
    let mut text = normalized.trim().to_string();
    if text.starts_with("```") {
        text = strip_fences(&text);
    } else if let Some(inner) = fenced_block(&text) {
        text = inner;
    }
    // «Here is the commit message:» / «Aquí tienes el mensaje:» delante del mensaje.
    let mut lines: Vec<&str> = text.lines().collect();
    if lines.len() > 1 {
        let first = lines[0].trim();
        let next_blank = lines[1].trim().is_empty();
        if commit_regexes().preamble.is_match(first) && (first.ends_with(':') || next_blank) {
            lines.remove(0);
            while lines.first().is_some_and(|l| l.trim().is_empty()) {
                lines.remove(0);
            }
            text = lines.join("\n");
        }
    }
    strip_wrapping_quotes(&text).to_string()
}

fn clean_subject(line: &str) -> String {
    let re = commit_regexes();
    // Negritas de markdown (`**Asunto:** …`).
    let mut s = line.replace("**", "").trim().to_string();
    s = re.heading.replace(&s, "").trim().to_string();
    s = re.subject_label.replace(&s, "").trim().to_string();
    strip_wrapping_quotes(&s).to_string()
}

/// Separa asunto y cuerpo de un mensaje de commit. Tolera vallas de código, comillas,
/// una frase de introducción y etiquetas como «Asunto:» o «Cuerpo:».
pub fn split_commit_message(raw: &str) -> (String, String) {
    let text = clean_commit_text(raw);
    let mut lines = text.lines().skip_while(|l| l.trim().is_empty());
    let subject = clean_subject(lines.next().unwrap_or_default());
    let mut rest: Vec<String> = lines.map(|l| l.trim_end().to_string()).collect();
    if let Some(first) = rest.iter().position(|l| !l.trim().is_empty()) {
        if let Some(c) = commit_regexes().body_label.captures(rest[first].trim()) {
            let tail = c[1].trim().to_string();
            rest[first] = tail;
        }
    }
    (subject, rest.join("\n").trim().to_string())
}

fn with_trailing_newline(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        String::new()
    } else {
        format!("{t}\n")
    }
}

/// Extrae contenido y justificación de la respuesta a una propuesta de conflicto
/// (`---FILE--- … ---RATIONALE--- …`).
pub fn parse_conflict_proposal(raw: &str) -> (String, Option<String>) {
    let text = strip_fences(raw);
    let Some(i) = text.find("---FILE---") else {
        return (with_trailing_newline(&text), None);
    };
    let after = &text[i + "---FILE---".len()..];
    match after.find("---RATIONALE---") {
        Some(j) => {
            let content = with_trailing_newline(&after[..j]);
            let rationale = after[j + "---RATIONALE---".len()..].trim();
            (
                content,
                (!rationale.is_empty()).then(|| rationale.to_string()),
            )
        }
        None => (with_trailing_newline(after), None),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const CLAUDE_OK: &str = include_str!("../tests/fixtures/claude_ok.json");
    const CLAUDE_LOGIN: &str = include_str!("../tests/fixtures/claude_login_error.json");
    const CLAUDE_QUOTA: &str = include_str!("../tests/fixtures/claude_quota_error.json");
    const CLAUDE_NO_RESULT: &str = include_str!("../tests/fixtures/claude_sin_result.json");
    const CODEX_OK: &str = include_str!("../tests/fixtures/codex_ok.jsonl");
    const CODEX_LEGACY: &str = include_str!("../tests/fixtures/codex_legacy.jsonl");
    const CODEX_QUOTA: &str = include_str!("../tests/fixtures/codex_quota.jsonl");
    const CODEX_LOGIN: &str = include_str!("../tests/fixtures/codex_login.jsonl");
    const COPILOT_OK: &str = include_str!("../tests/fixtures/copilot_ok.txt");
    const COPILOT_LOGIN: &str = include_str!("../tests/fixtures/copilot_login.txt");

    #[test]
    fn claude_golden_con_banner_y_campos_extra() {
        let r = parse_claude(CLAUDE_OK).unwrap();
        assert!(r.text.starts_with("feat: parsear status porcelain v2"));
        assert_eq!(
            r.note.as_deref(),
            Some("Claude: 120 tokens de entrada, 18 de salida.")
        );
    }

    #[test]
    fn claude_json_en_varias_lineas() {
        let r = parse_claude("{\n  \"is_error\": false,\n  \"result\": \"hola\"\n}\n").unwrap();
        assert_eq!(r.text, "hola");
    }

    #[test]
    fn claude_login_y_cuota() {
        match parse_claude(CLAUDE_LOGIN) {
            Err(AiError::NotLoggedIn {
                provider,
                login_command,
            }) => {
                assert_eq!(provider, ProviderId::Claude);
                assert!(login_command.contains("/login"));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            parse_claude(CLAUDE_QUOTA),
            Err(AiError::Quota { .. })
        ));
    }

    #[test]
    fn claude_sin_result_o_basura_es_formato_no_reconocido() {
        for s in [
            CLAUDE_NO_RESULT,
            "hola, esto no es json",
            "",
            "[1,2]",
            "{\"a\":",
        ] {
            assert_eq!(
                parse_claude(s),
                Err(AiError::BadOutput(UNRECOGNIZED.into())),
                "{s}"
            );
        }
    }

    #[test]
    fn nunca_se_devuelve_texto_crudo_en_el_error() {
        let Err(e) = parse_claude("SECRETO-DEL-REPO no json") else {
            panic!("debía fallar")
        };
        assert!(!e.to_string().contains("SECRETO-DEL-REPO"));
    }

    #[test]
    fn codex_golden_extrae_el_ultimo_mensaje_y_tolera_desconocidos() {
        let r = parse_codex(CODEX_OK).unwrap();
        assert_eq!(
            r.text,
            "---FILE---\nresuelto\n---RATIONALE---\nMe quedo con ambos títulos."
        );
        assert_eq!(
            r.note.as_deref(),
            Some("Codex: 10 tokens de entrada, 4 de salida.")
        );
    }

    #[test]
    fn codex_formato_antiguo_assistant_message() {
        assert_eq!(parse_codex(CODEX_LEGACY).unwrap().text, "hola");
    }

    #[test]
    fn codex_fallos() {
        assert!(matches!(
            parse_codex(CODEX_QUOTA),
            Err(AiError::Quota {
                provider: ProviderId::Codex,
                ..
            })
        ));
        match parse_codex(CODEX_LOGIN) {
            Err(AiError::NotLoggedIn { login_command, .. }) => {
                assert_eq!(login_command, "codex login")
            }
            other => panic!("{other:?}"),
        }
        let failed = r#"{"type":"turn.failed","error":{"message":"quota"}}"#;
        assert!(parse_codex(failed).is_err());
    }

    #[test]
    fn codex_sin_json_o_sin_mensaje() {
        assert_eq!(
            parse_codex("texto plano"),
            Err(AiError::BadOutput(UNRECOGNIZED.into()))
        );
        assert!(matches!(
            parse_codex(r#"{"type":"turn.completed"}"#),
            Err(AiError::BadOutput(_))
        ));
    }

    #[test]
    fn copilot_texto_plano_y_ansi() {
        assert_eq!(
            parse_copilot(COPILOT_OK).unwrap().text,
            "feat: parsear status\n\nUsa porcelain v2."
        );
        assert_eq!(parse_copilot("\x1b[32mhola\x1b[0m\n").unwrap().text, "hola");
        assert!(parse_copilot("  \n").is_err());
    }

    #[test]
    fn copilot_error_con_codigo_cero_se_clasifica() {
        match parse_copilot(COPILOT_LOGIN) {
            Err(AiError::NotLoggedIn { login_command, .. }) => {
                assert_eq!(login_command, "copilot login")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn un_diff_que_dice_unauthorized_no_es_un_error() {
        let r = parse_copilot("fix: devuelve 401 unauthorized correctamente").unwrap();
        assert!(r.text.contains("unauthorized"));
    }

    #[test]
    fn clasifica_fallos_del_proceso() {
        let e = error_from_failed_run(
            ProviderId::Codex,
            Some(1),
            "",
            "Please run `codex login` to continue\n",
        );
        assert!(matches!(e, AiError::NotLoggedIn { .. }));
        let e = error_from_failed_run(
            ProviderId::Copilot,
            Some(2),
            "",
            "HTTP 429 Too Many Requests",
        );
        assert!(matches!(e, AiError::Quota { .. }));
        let e = error_from_failed_run(ProviderId::Copilot, Some(2), "", "boom");
        assert_eq!(
            e,
            AiError::ProviderFailed {
                provider: ProviderId::Copilot,
                exit_code: Some(2),
                detail: "boom".into()
            }
        );
        // Claude: el error viaja en el JSON de stdout.
        let e = error_from_failed_run(ProviderId::Claude, Some(1), CLAUDE_LOGIN, "");
        assert!(matches!(e, AiError::NotLoggedIn { .. }));
    }

    #[test]
    fn detalle_truncado() {
        let long = "x".repeat(5000);
        let e = error_from_failed_run(ProviderId::Copilot, Some(1), "", &long);
        let AiError::ProviderFailed { detail, .. } = e else {
            panic!()
        };
        assert_eq!(detail.chars().count(), 2001);
    }

    #[test]
    fn valla_de_codigo_se_quita() {
        let (s, b) = split_commit_message("```text\nfix: puerta\n\nEvita el write.\n```");
        assert_eq!(s, "fix: puerta");
        assert_eq!(b, "Evita el write.");
        assert_eq!(strip_fences("sin vallas "), "sin vallas");
    }

    fn split(raw: &str) -> (String, String) {
        split_commit_message(raw)
    }

    #[test]
    fn mensaje_limpio_asunto_y_cuerpo() {
        assert_eq!(
            split("feat: añade login\n\n- valida el token\n- guarda la sesión\n"),
            (
                "feat: añade login".into(),
                "- valida el token\n- guarda la sesión".into()
            )
        );
        assert_eq!(
            split("fix: solo asunto"),
            ("fix: solo asunto".into(), String::new())
        );
    }

    #[test]
    fn asunto_y_cuerpo_sin_linea_en_blanco() {
        assert_eq!(
            split("fix: x\n- a\n- b"),
            ("fix: x".into(), "- a\n- b".into())
        );
    }

    #[test]
    fn vallas_con_introduccion_y_con_lenguaje() {
        let raw =
            "Here is the commit message:\n\n```gitcommit\nfeat: nueva vista\n\n- detalle\n```\n";
        assert_eq!(split(raw), ("feat: nueva vista".into(), "- detalle".into()));
        // Sin valla de cierre.
        assert_eq!(
            split("```\nfix: corta\n\ncuerpo").0,
            "fix: corta".to_string()
        );
        // Fin de línea de Windows.
        assert_eq!(
            split("```\r\nfix: crlf\r\n\r\ncuerpo\r\n```").1,
            "cuerpo".to_string()
        );
    }

    #[test]
    fn comillas_alrededor_del_asunto_o_del_mensaje() {
        for raw in [
            "\"fix: puerta\"",
            "'fix: puerta'",
            "“fix: puerta”",
            "«fix: puerta»",
            "`fix: puerta`",
        ] {
            assert_eq!(split(raw).0, "fix: puerta", "{raw}");
        }
        // Mensaje completo entre comillas.
        assert_eq!(
            split("\"fix: puerta\n\n- detalle\""),
            ("fix: puerta".into(), "- detalle".into())
        );
        // Solo el asunto entre comillas.
        assert_eq!(
            split("\"fix: puerta\"\n\n- detalle"),
            ("fix: puerta".into(), "- detalle".into())
        );
        // Una comilla legítima dentro no se toca.
        assert_eq!(split("fix: cierra \"x\" bien").0, "fix: cierra \"x\" bien");
    }

    #[test]
    fn prefijos_de_etiqueta_en_espanol_e_ingles() {
        for (raw, subject) in [
            ("Asunto: corrige el login", "corrige el login"),
            ("Subject: fix login", "fix login"),
            ("**Asunto:** corrige el login", "corrige el login"),
            ("Mensaje de commit: corrige el login", "corrige el login"),
            ("Title: \"fix login\"", "fix login"),
            ("# fix: login", "fix: login"),
        ] {
            assert_eq!(split(raw).0, subject, "{raw}");
        }
        assert_eq!(
            split("Asunto: corrige el login\n\nCuerpo:\n- cambia A\n- cambia B"),
            ("corrige el login".into(), "- cambia A\n- cambia B".into())
        );
        assert_eq!(
            split("Subject: fix\n\nBody: arregla el caso vacío").1,
            "arregla el caso vacío"
        );
    }

    #[test]
    fn un_tipo_conventional_no_se_confunde_con_una_etiqueta() {
        assert_eq!(
            split("docs: actualiza la guía").0,
            "docs: actualiza la guía"
        );
        assert_eq!(split("feat(ui): botón").0, "feat(ui): botón");
    }

    #[test]
    fn respuesta_vacia_da_asunto_vacio() {
        assert_eq!(split("  \n\n "), (String::new(), String::new()));
        assert_eq!(split("```\n```").0, "");
    }

    #[test]
    fn propuesta_de_conflicto() {
        let (c, r) = parse_conflict_proposal("---FILE---\nresuelto\n---RATIONALE---\nPorque sí.");
        assert_eq!(c, "resuelto\n");
        assert_eq!(r.as_deref(), Some("Porque sí."));
        let (c, r) = parse_conflict_proposal("solo texto");
        assert_eq!((c.as_str(), r), ("solo texto\n", None));
    }
}
