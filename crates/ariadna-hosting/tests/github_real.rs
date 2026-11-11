//! SPIKE REAL contra GitHub (`#[ignore]`): recorre el cliente `GitHub` contra la API real en un
//! repositorio privado **desechable**. Solo se ejecuta con
//! `ARIADNA_SPIKE_REPO=jamartif/ariadna-spike-pr` (el nombre debe contener «spike»).
//!
//! ```text
//! ARIADNA_SPIKE_REPO=jamartif/ariadna-spike-pr \
//! ARIADNA_SPIKE_HELPER='!gh.exe auth git-credential' \
//!   cargo test -p ariadna-hosting --test github_real -- --ignored --nocapture
//! ```
//!
//! * La credencial sale de `git credential fill` (con el helper de `ARIADNA_SPIKE_HELPER`,
//!   solo para este proceso: `GitCredentialFill::with_helper_override`). Si no se define, se usa
//!   la configuración normal de git.
//! * **Regla inamovible:** no se imprime ni se guarda el token ni ninguna cabecera
//!   `Authorization`. Solo se muestran las cabeceras `x-accepted-github-permissions`,
//!   `x-oauth-scopes` y `x-accepted-oauth-scopes`, y toda salida pasa por un redactor.
//! * Los PR de la prueba se crean y se cierran; las ramas temporales se borran. El repositorio
//!   no se elimina (el token no tiene permiso de borrado).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ariadna_hosting::*;
use reqwest::Method;
use serde_json::{json, Value};
use zeroize::Zeroizing;

const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64(input: &[u8]) -> String {
    let mut out = String::new();
    for c in input.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

// --- peticiones «crudas» (solo estado HTTP y cabeceras de permisos) ------------------------

struct RawReply {
    status: u16,
    perms: String,
    scopes: String,
    accepted_scopes: String,
    msg: String,
    body: Value,
}

struct Raw {
    c: reqwest::Client,
    user: String,
    tok: Zeroizing<String>,
    base: String,
}

impl Raw {
    async fn send(&self, m: Method, path: &str, body: Option<Value>) -> RawReply {
        self.send_as(m, path, body, &self.user, &self.tok).await
    }

    async fn send_as(
        &self,
        m: Method,
        path: &str,
        body: Option<Value>,
        user: &str,
        tok: &str,
    ) -> RawReply {
        let mut rb = self
            .c
            .request(m, format!("{}{path}", self.base))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "ariadna-spike")
            .basic_auth(user, Some(tok));
        if let Some(b) = body {
            rb = rb
                .header("Content-Type", "application/json")
                .body(serde_json::to_vec(&b).unwrap());
        }
        let r = rb.send().await.expect("red");
        let status = r.status().as_u16();
        let h = |n: &str| {
            r.headers()
                .get(n)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned()
        };
        let (perms, scopes, accepted_scopes) = (
            h("x-accepted-github-permissions"),
            h("x-oauth-scopes"),
            h("x-accepted-oauth-scopes"),
        );
        let text = r.text().await.unwrap_or_default();
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let mut msg = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        if let Some(e) = body.pointer("/errors/0").map(|e| {
            e.get("message")
                .and_then(Value::as_str)
                .map_or_else(|| e.to_string(), str::to_owned)
        }) {
            msg = format!("{msg} · {e}");
        }
        RawReply {
            status,
            perms,
            scopes,
            accepted_scopes,
            msg,
            body,
        }
    }
}

// --- informe -------------------------------------------------------------------------------

struct Row {
    id: String,
    verdict: &'static str,
    http: String,
    note: String,
}

struct Ctx {
    gh: Box<dyn PullRequestProvider>,
    raw: Raw,
    repo: String,
    ts: u64,
    rows: Vec<Row>,
    branches: Vec<String>,
}

impl Ctx {
    fn push(&mut self, id: &str, verdict: &'static str, http: &str, note: impl Into<String>) {
        self.rows.push(Row {
            id: id.to_owned(),
            verdict,
            http: http.to_owned(),
            note: note.into(),
        });
    }

    /// Operación que debe salir bien.
    fn ok<R>(&mut self, id: &str, r: HostingResult<R>, http: &str, note: &str) -> Option<R> {
        match r {
            Ok(v) => {
                self.push(id, "OK", http, note);
                Some(v)
            }
            Err(e) => {
                self.push(id, "FALLO", http, format!("{} · {e}", e.code()));
                None
            }
        }
    }

    /// Operación que debe fallar con un código de error concreto.
    fn err<R>(&mut self, id: &str, r: HostingResult<R>, want: &str, http: &str, note: &str) {
        match r {
            Err(e) if e.code() == want => {
                self.push(id, "OK", http, format!("error real {want}: {note} · «{e}»"))
            }
            Err(e) => self.push(
                id,
                "FALLO",
                http,
                format!("esperaba {want}, llegó {} · {e}", e.code()),
            ),
            Ok(_) => self.push(id, "FALLO", http, format!("esperaba {want}, salió bien")),
        }
    }

    async fn fixture_branch(&mut self, name: &str, file: &str, content: &str) -> bool {
        let repo = self.repo.clone();
        let r = self
            .raw
            .send(
                Method::GET,
                &format!("/repos/{repo}/git/ref/heads/main"),
                None,
            )
            .await;
        let Some(sha) = r
            .body
            .pointer("/object/sha")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            self.push(
                &format!("fixture {name}"),
                "FALLO",
                &r.status.to_string(),
                r.msg,
            );
            return false;
        };
        let r = self
            .raw
            .send(
                Method::POST,
                &format!("/repos/{repo}/git/refs"),
                Some(json!({"ref": format!("refs/heads/{name}"), "sha": sha})),
            )
            .await;
        if r.status != 201 {
            self.push(
                &format!("fixture {name}"),
                "FALLO",
                &r.status.to_string(),
                r.msg,
            );
            return false;
        }
        self.branches.push(name.to_owned());
        self.put_file(name, file, content).await
    }

    async fn put_file(&mut self, branch: &str, file: &str, content: &str) -> bool {
        let repo = self.repo.clone();
        let cur = self
            .raw
            .send(
                Method::GET,
                &format!("/repos/{repo}/contents/{file}?ref={branch}"),
                None,
            )
            .await;
        let mut body = json!({"message": format!("spike: {file}"), "content": b64(content.as_bytes()), "branch": branch});
        if cur.status == 200 {
            body["sha"] = cur.body.get("sha").cloned().unwrap_or(Value::Null);
        }
        let r = self
            .raw
            .send(
                Method::PUT,
                &format!("/repos/{repo}/contents/{file}"),
                Some(body),
            )
            .await;
        if !(200..300).contains(&r.status) {
            self.push(
                &format!("fixture {branch}/{file}"),
                "FALLO",
                &r.status.to_string(),
                r.msg,
            );
            return false;
        }
        true
    }

    async fn open_pr(
        &mut self,
        id: &str,
        branch: &str,
        draft: bool,
        reviewers: Vec<String>,
    ) -> Option<u64> {
        let r = self
            .gh
            .create(&NewPullRequest {
                title: format!("Spike {branch}"),
                description: format!("PR de prueba **{branch}**\n\n- punto 1\n- punto 2"),
                source_branch: branch.to_owned(),
                target_branch: "main".into(),
                draft,
                reviewers,
                work_item_ids: vec![],
            })
            .await;
        let d = self.ok(id, r, "201", if draft { "borrador" } else { "" })?;
        Some(d.summary.id)
    }

    /// `mergeable` se calcula en segundo plano: reintenta mientras llegue `null`.
    async fn settled(&mut self, id: &str, n: u64) -> Option<PullRequestDetail> {
        let mut first_unknown = None;
        for i in 0..10 {
            let d = self.gh.detail(n).await;
            match d {
                Ok(d) => {
                    if first_unknown.is_none() {
                        first_unknown = Some(d.summary.merge_status == MergeStatus::Unknown);
                    }
                    if d.summary.merge_status != MergeStatus::Unknown || i == 9 {
                        let note = format!(
                            "mergeable_state={:?}; merge_status={:?}; primer intento desconocido={}",
                            d.merge_detail,
                            d.summary.merge_status,
                            first_unknown.unwrap_or(false)
                        );
                        self.push(id, "OK", "200", note);
                        return Some(d);
                    }
                }
                Err(e) => {
                    self.push(id, "FALLO", "?", format!("{} · {e}", e.code()));
                    return None;
                }
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        None
    }

    fn print(&self, secret: &str) {
        let scrub = |s: &str| {
            if secret.len() >= 4 {
                s.replace(secret, "***")
            } else {
                s.to_owned()
            }
        };
        println!("\n| punto | resultado | HTTP | nota |\n|---|---|---|---|");
        for r in &self.rows {
            let note: String = scrub(&r.note)
                .replace('\n', " ")
                .replace('|', "/")
                .chars()
                .take(260)
                .collect();
            println!("| {} | {} | {} | {} |", r.id, r.verdict, r.http, note);
        }
        let (ok, fail, lim) = self.rows.iter().fold((0, 0, 0), |a, r| match r.verdict {
            "OK" => (a.0 + 1, a.1, a.2),
            "FALLO" => (a.0, a.1 + 1, a.2),
            _ => (a.0, a.1, a.2 + 1),
        });
        println!("\nTOTAL: {ok} OK · {fail} FALLO · {lim} LÍMITE");
    }
}

fn hunk_new_start(patch: &str) -> Option<u32> {
    let h = patch.lines().find(|l| l.starts_with("@@"))?;
    let plus = h.split('+').nth(1)?;
    plus.split([',', ' ']).next()?.parse().ok()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "contra GitHub real: ARIADNA_SPIKE_REPO=jamartif/ariadna-spike-pr"]
async fn spike_github_real() {
    let Ok(repo) = std::env::var("ARIADNA_SPIKE_REPO") else {
        println!("ARIADNA_SPIKE_REPO no definido: no se hace nada.");
        return;
    };
    assert!(
        repo.contains("spike") && repo.matches('/').count() == 1,
        "solo repos desechables"
    );
    let _ = rustls::crypto::ring::default_provider().install_default();

    let mut fill = GitCredentialFill::new("git");
    if let Ok(h) = std::env::var("ARIADNA_SPIKE_HELPER") {
        fill = fill.with_helper_override(h);
    }
    let fill = Arc::new(fill);
    let web = format!("https://github.com/{repo}");
    let auth = fill.credentials(&web).await.expect("credencial del helper");
    let (user, tok) = match &auth {
        AuthMethod::Pat { username, secret } => {
            (username.clone(), Zeroizing::new(secret.expose().to_owned()))
        }
        _ => panic!("método no previsto"),
    };
    let secret = tok.to_string();
    drop(auth);

    let info = RemoteInfo::parse(&web);
    let gh = provider_for(&info, fill.clone(), ProviderSettings::default()).expect("proveedor");
    let raw = Raw {
        c: reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap(),
        user,
        tok,
        base: "https://api.github.com".into(),
    };
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut cx = Ctx {
        gh,
        raw,
        repo: repo.clone(),
        ts,
        rows: vec![],
        branches: vec![],
    };

    let mut perms: Vec<(String, u16, String)> = vec![];
    let mut scopes = String::new();

    s01_soporte_y_usuario(&mut cx).await;
    let ids = s02_fixtures_y_creacion(&mut cx).await;
    s03_errores_de_creacion(&mut cx, &ids).await;
    s04_listado(&mut cx).await;
    s05_detalle_y_diff(&mut cx, &ids).await;
    s06_hilos(&mut cx, &ids).await;
    s07_votos(&mut cx, &ids).await;
    s08_cerrar_reabrir(&mut cx, &ids).await;
    s09_errores_http(&mut cx, &ids).await;
    s10_completar(&mut cx, &ids).await;
    cleanup(&mut cx).await;
    probe_permissions(&mut cx, &ids, &mut perms, &mut scopes).await;

    cx.print(&secret);
    println!("\nPermisos observados (cabeceras de la respuesta; vacío = GitHub no la envía):");
    println!("scopes del token (x-oauth-scopes): «{scopes}»");
    println!("| endpoint | HTTP | x-accepted-github-permissions / x-accepted-oauth-scopes |\n|---|---|---|");
    for (e, st, p) in &perms {
        println!("| {e} | {st} | {p} |");
    }
    let fails: Vec<_> = cx
        .rows
        .iter()
        .filter(|r| r.verdict == "FALLO")
        .map(|r| r.id.clone())
        .collect();
    assert!(fails.is_empty(), "hay FALLO: {fails:?}");
}

struct Ids {
    a: Option<u64>,
    b: Option<u64>,
    cf1: Option<u64>,
    cf2: Option<u64>,
    mrg: Option<u64>,
    sq: Option<u64>,
    rb: Option<u64>,
    empty: String,
}

async fn s01_soporte_y_usuario(cx: &mut Ctx) {
    let caps = cx.gh.capabilities();
    let okc = caps.provider == Provider::GitHub && caps.resolve_threads && caps.draft;
    cx.push(
        "pull_request_support / capabilities",
        if okc { "OK" } else { "FALLO" },
        "-",
        format!("{:?}", caps.votes),
    );
    let r = cx.gh.current_user().await;
    cx.ok("GET /user (current_user)", r, "200", "");
    let info = RemoteInfo::parse(&format!("https://github.com/{}", cx.repo));
    cx.push(
        "pull_request_support (remoto)",
        if info.provider == Provider::GitHub && info.is_complete() {
            "OK"
        } else {
            "FALLO"
        },
        "-",
        "RemoteInfo reconoce el remoto",
    );
}

async fn s02_fixtures_y_creacion(cx: &mut Ctx) -> Ids {
    let ts = cx.ts;
    let (mrg, sq, rb, cf1, cf2, empty) = (
        format!("spike/mrg-{ts}"),
        format!("spike/sq-{ts}"),
        format!("spike/rb-{ts}"),
        format!("spike/cf1-{ts}"),
        format!("spike/cf2-{ts}"),
        format!("spike/empty-{ts}"),
    );
    let cfile = format!("conflict-{ts}.txt");
    // Base común del conflicto, directamente en main (repo desechable).
    let repo = cx.repo.clone();
    let main_has = cx
        .raw
        .send(
            Method::GET,
            &format!("/repos/{repo}/git/ref/heads/main"),
            None,
        )
        .await;
    cx.push(
        "fixture main existe",
        if main_has.status == 200 {
            "OK"
        } else {
            "FALLO"
        },
        &main_has.status.to_string(),
        main_has.msg.clone(),
    );
    let base_ok = {
        let body = json!({"message": "spike: base del conflicto", "content": b64(b"l1\nl2\nl3\n"), "branch": "main"});
        let r = cx
            .raw
            .send(
                Method::PUT,
                &format!("/repos/{repo}/contents/{cfile}"),
                Some(body),
            )
            .await;
        r.status == 201
    };
    let mut ok = base_ok;
    ok &= cx
        .fixture_branch(&mrg, &format!("m-{ts}.txt"), "merge\n")
        .await;
    ok &= cx
        .fixture_branch(&sq, &format!("s-{ts}.txt"), "squash\n")
        .await;
    ok &= cx
        .fixture_branch(&rb, &format!("r-{ts}.txt"), "rebase\n")
        .await;
    ok &= cx.fixture_branch(&cf1, &cfile, "l1\nl2-cf1\nl3\n").await;
    ok &= cx.fixture_branch(&cf2, &cfile, "l1\nl2-cf2\nl3\n").await;
    ok &= {
        let r = cx
            .raw
            .send(
                Method::GET,
                &format!("/repos/{repo}/git/ref/heads/main"),
                None,
            )
            .await;
        let sha = r
            .body
            .pointer("/object/sha")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let c = cx
            .raw
            .send(
                Method::POST,
                &format!("/repos/{repo}/git/refs"),
                Some(json!({"ref": format!("refs/heads/{empty}"), "sha": sha})),
            )
            .await;
        if c.status == 201 {
            cx.branches.push(empty.clone());
        }
        c.status == 201
    };
    cx.push(
        "fixtures (ramas temporales)",
        if ok { "OK" } else { "FALLO" },
        "201",
        "ramas y archivos creados por la API de contenidos",
    );

    // Creación con el cliente.
    let a = cx
        .open_pr(
            "crear PR spike/feature-a → main",
            "spike/feature-a",
            false,
            vec![],
        )
        .await;
    let b = cx
        .open_pr(
            "crear PR spike/feature-b (borrador)",
            "spike/feature-b",
            true,
            vec![],
        )
        .await;
    if let Some(n) = b {
        let d = cx.gh.detail(n).await;
        if let Some(d) = cx.ok("detalle del borrador (is_draft)", d, "200", "") {
            let v = if d.summary.is_draft { "OK" } else { "FALLO" };
            cx.push(
                "borrador reconocido",
                v,
                "200",
                format!("is_draft={}", d.summary.is_draft),
            );
        }
    }
    let cf1p = cx.open_pr("crear PR cf1", &cf1, false, vec![]).await;
    let cf2p = cx.open_pr("crear PR cf2", &cf2, false, vec![]).await;
    let mrgp = cx.open_pr("crear PR mrg", &mrg, false, vec![]).await;
    // Pedir revisión a la propia autora: GitHub la rechaza; ¿queda la PR creada a medias?
    let me = cx.raw.user.clone();
    let r = cx
        .gh
        .create(&NewPullRequest {
            title: format!("Spike {sq}"),
            description: String::new(),
            source_branch: sq.clone(),
            target_branch: "main".into(),
            draft: false,
            reviewers: vec![me],
            work_item_ids: vec![],
        })
        .await;
    let sqp = match r {
        Ok(d) => {
            let note = d.merge_detail.clone().unwrap_or_default();
            let v = if note.starts_with("PR creada;") {
                "LÍMITE"
            } else {
                "FALLO"
            };
            cx.push(
                "crear con revisor = autora",
                v,
                "201 + 422",
                format!("PR creada y anotada: {note}"),
            );
            Some(d.summary.id)
        }
        Err(e) => {
            cx.push(
                "crear con revisor = autora",
                "FALLO",
                "422",
                format!("{} · {e}", e.code()),
            );
            None
        }
    };
    let rbp = cx.open_pr("crear PR rb", &rb, false, vec![]).await;
    Ids {
        a,
        b,
        cf1: cf1p,
        cf2: cf2p,
        mrg: mrgp,
        sq: sqp,
        rb: rbp,
        empty,
    }
}

async fn s03_errores_de_creacion(cx: &mut Ctx, ids: &Ids) {
    // 422 duplicada.
    let dup = cx
        .gh
        .create(&NewPullRequest {
            title: "dup".into(),
            description: String::new(),
            source_branch: "spike/feature-a".into(),
            target_branch: "main".into(),
            draft: false,
            reviewers: vec![],
            work_item_ids: vec![],
        })
        .await;
    let repo = cx.repo.clone();
    let raw = cx
        .raw
        .send(
            Method::POST,
            &format!("/repos/{repo}/pulls"),
            Some(json!({"title": "dup", "head": "spike/feature-a", "base": "main"})),
        )
        .await;
    let http = raw.status.to_string();
    let want = if ids.a.is_some() {
        "CONFLICT"
    } else {
        "INVALID_REFERENCE"
    };
    cx.err("crear PR duplicada", dup, want, &http, &raw.msg);
    // 422 rama sin cambios.
    let empty = cx
        .gh
        .create(&NewPullRequest {
            title: "vacía".into(),
            description: String::new(),
            source_branch: ids.empty.clone(),
            target_branch: "main".into(),
            draft: false,
            reviewers: vec![],
            work_item_ids: vec![],
        })
        .await;
    let raw = cx
        .raw
        .send(
            Method::POST,
            &format!("/repos/{repo}/pulls"),
            Some(json!({"title": "v", "head": ids.empty, "base": "main"})),
        )
        .await;
    cx.err(
        "crear PR sin cambios",
        empty,
        "INVALID_REFERENCE",
        &raw.status.to_string(),
        &raw.msg,
    );
    // Rama inexistente.
    let ghost = cx
        .gh
        .create(&NewPullRequest {
            title: "x".into(),
            description: String::new(),
            source_branch: "spike/no-existe".into(),
            target_branch: "main".into(),
            draft: false,
            reviewers: vec![],
            work_item_ids: vec![],
        })
        .await;
    let raw = cx
        .raw
        .send(
            Method::POST,
            &format!("/repos/{repo}/pulls"),
            Some(json!({"title": "x", "head": "spike/no-existe", "base": "main"})),
        )
        .await;
    cx.err(
        "crear PR con rama inexistente",
        ghost,
        "INVALID_REFERENCE",
        &raw.status.to_string(),
        &raw.msg,
    );
}

async fn s04_listado(cx: &mut Ctx) {
    for (f, name) in [
        (PrFilter::Open, "abiertas"),
        (PrFilter::Mine, "mías"),
        (PrFilter::Reviewing, "como revisor"),
        (PrFilter::Closed, "cerradas"),
    ] {
        let r = cx.gh.list(&ListQuery::new(f, 1)).await;
        if let Some(p) = cx.ok(&format!("listar {name}"), r, "200", "") {
            let n = p.items.len();
            let last = cx.rows.last_mut().unwrap();
            last.note = format!("{n} PR en la página 1; has_more={}", p.has_more);
        }
    }
    // Paginación de 1 en 1.
    let q = ListQuery {
        filter: PrFilter::Open,
        page: 1,
        per_page: 1,
    };
    let p1 = cx.gh.list(&q).await;
    let p2 = cx.gh.list(&ListQuery { page: 2, ..q }).await;
    if let (Some(p1), Some(p2)) = (
        cx.ok(
            "paginación página 1 (per_page=1)",
            p1,
            "200 + Link next",
            "",
        ),
        cx.ok("paginación página 2 (per_page=1)", p2, "200", ""),
    ) {
        let distinct = p1.items.first().map(|i| i.id) != p2.items.first().map(|i| i.id);
        let v = if p1.has_more && distinct && p1.items.len() == 1 {
            "OK"
        } else {
            "FALLO"
        };
        cx.push(
            "paginación coherente (Link rel=next)",
            v,
            "200",
            format!(
                "p1.has_more={} p1={:?} p2={:?}",
                p1.has_more,
                p1.items.first().map(|i| i.id),
                p2.items.first().map(|i| i.id)
            ),
        );
    }
    // Estado de las PR en el listado.
    let page = cx.gh.list(&ListQuery::new(PrFilter::Open, 1)).await;
    if let Ok(p) = page {
        let draft = p.items.iter().filter(|i| i.is_draft).count();
        let me = cx.raw.user.clone();
        let mine = p
            .items
            .iter()
            .all(|i| i.author.login.eq_ignore_ascii_case(&me));
        cx.push(
            "listado: borradores y autor",
            "OK",
            "200",
            format!("{draft} borrador(es); todas de {me}: {mine}"),
        );
    }
}

async fn s05_detalle_y_diff(cx: &mut Ctx, ids: &Ids) {
    if let Some(n) = ids.a {
        if let Some(d) = cx
            .settled("detalle PR A (mergeable/checks/revisores)", n)
            .await
        {
            let note = format!(
                "revisores={} checks={:?} commits={} archivos={:?} fork={} head={}",
                d.summary.reviewers.len(),
                d.summary.checks,
                d.commits.len(),
                d.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
                d.is_fork,
                d.source_commit
                    .as_deref()
                    .unwrap_or("-")
                    .chars()
                    .take(7)
                    .collect::<String>()
            );
            cx.push("detalle PR A: campos", "OK", "200", note);
        }
        let r = cx.gh.diff(n).await;
        if let Some(files) = cx.ok("archivos y diff (patch) PR A", r, "200", "") {
            let f = files.iter().find(|f| f.change.path == "docs/a.md");
            let v = if f.is_some_and(|f| {
                f.patch
                    .as_deref()
                    .is_some_and(|p| p.starts_with("--- /dev/null\n+++ b/docs/a.md\n@@"))
            }) {
                "OK"
            } else {
                "FALLO"
            };
            let last = cx.rows.last_mut().unwrap();
            last.verdict = v;
            last.note = format!(
                "{} archivo(s); parche de docs/a.md con cabecera esperada: {v}",
                files.len()
            );
        }
    }
    if let Some(n) = ids.b {
        let r = cx.gh.diff(n).await;
        cx.ok("archivos y diff PR B (borrador)", r, "200", "");
    }
}

async fn s06_hilos(cx: &mut Ctx, ids: &Ids) {
    let Some(n) = ids.a else { return };
    // Línea real del diff.
    let patch = cx
        .gh
        .diff(n)
        .await
        .ok()
        .and_then(|fs| fs.into_iter().find(|f| f.change.path == "docs/a.md"))
        .and_then(|f| f.patch);
    let line = patch.as_deref().and_then(hunk_new_start).unwrap_or(1);
    let r = cx
        .gh
        .create_thread(
            n,
            &NewThread {
                body: "Comentario **en línea** del spike".into(),
                anchor: Some(ThreadAnchor {
                    path: "docs/a.md".into(),
                    line,
                    side: DiffSide::New,
                }),
            },
        )
        .await;
    let t = cx.ok("crear hilo en línea (RIGHT)", r, "201 + GraphQL", "");
    let general = cx
        .gh
        .create_thread(
            n,
            &NewThread {
                body: "Comentario general del spike".into(),
                anchor: None,
            },
        )
        .await;
    cx.ok("comentario general (issues/comments)", general, "201", "");
    let Some(t) = t else { return };
    let r = cx.gh.reply(n, &t.id, "Respuesta del spike").await;
    if let Some(t2) = cx.ok("responder hilo (GraphQL)", r, "200", "") {
        let v = if t2.comments.len() == 2 {
            "OK"
        } else {
            "FALLO"
        };
        cx.push(
            "responder: hilo con 2 comentarios",
            v,
            "200",
            format!("{} comentarios", t2.comments.len()),
        );
    }
    let r = cx.gh.set_thread_resolved(n, &t.id, true).await;
    if let Some(t2) = cx.ok("resolver hilo (GraphQL)", r, "200", "") {
        let v = if t2.status == ThreadStatus::Resolved {
            "OK"
        } else {
            "FALLO"
        };
        cx.push(
            "resolver: estado Resolved",
            v,
            "200",
            format!("{:?} can_resolve={}", t2.status, t2.can_resolve),
        );
    }
    let r = cx.gh.set_thread_resolved(n, &t.id, false).await;
    if let Some(t2) = cx.ok("reabrir hilo (GraphQL)", r, "200", "") {
        let v = if t2.status == ThreadStatus::Active {
            "OK"
        } else {
            "FALLO"
        };
        cx.push(
            "reabrir: estado Active",
            v,
            "200",
            format!("{:?}", t2.status),
        );
    }
    let r = cx.gh.threads(n).await;
    if let Some(ts) = cx.ok("listar hilos (inline + generales)", r, "200", "") {
        let inline = ts.iter().filter(|t| t.anchor.is_some()).count();
        let v = if inline == 1 && ts.len() == 2 {
            "OK"
        } else {
            "FALLO"
        };
        let anchor = ts
            .iter()
            .find_map(|t| t.anchor.as_ref())
            .map(|a| format!("{}:{} {:?}", a.path, a.line, a.side));
        cx.push(
            "hilos: forma y ancla",
            v,
            "200",
            format!("{} hilos, {inline} en línea, ancla={anchor:?}", ts.len()),
        );
    }
    // Línea fuera del diff → 422.
    let r = cx
        .gh
        .create_thread(
            n,
            &NewThread {
                body: "fuera".into(),
                anchor: Some(ThreadAnchor {
                    path: "docs/a.md".into(),
                    line: 9999,
                    side: DiffSide::New,
                }),
            },
        )
        .await;
    cx.err(
        "comentario en línea fuera del diff",
        r,
        "INVALID_REFERENCE",
        "422",
        "line fuera del diff",
    );
    // Lado izquierdo (LEFT) en cf1 (línea 2 sustituida).
    if let Some(c) = ids.cf1 {
        let r = cx
            .gh
            .create_thread(
                c,
                &NewThread {
                    body: "Sobre la línea antigua".into(),
                    anchor: Some(ThreadAnchor {
                        path: format!("conflict-{}.txt", cx.ts),
                        line: 2,
                        side: DiffSide::Old,
                    }),
                },
            )
            .await;
        if let Some(t) = cx.ok("hilo en línea lado izquierdo (LEFT)", r, "201", "") {
            let v = if t
                .anchor
                .as_ref()
                .is_some_and(|a| a.side == DiffSide::Old && a.line == 2)
            {
                "OK"
            } else {
                "FALLO"
            };
            cx.push(
                "LEFT: ancla devuelta Old/2",
                v,
                "200",
                format!("{:?}", t.anchor),
            );
        }
    }
}

async fn s07_votos(cx: &mut Ctx, ids: &Ids) {
    let Some(n) = ids.a else { return };
    let r = cx
        .gh
        .vote(n, Vote::Comment, Some("Revisión COMMENT del spike"))
        .await;
    if let Some(d) = cx.ok("review COMMENT", r, "200", "") {
        let me = cx.raw.user.clone();
        let v = d
            .summary
            .reviewers
            .iter()
            .find(|r| r.user.login.eq_ignore_ascii_case(&me))
            .map(|r| r.vote);
        cx.push(
            "review COMMENT: voto reflejado",
            if v == Some(Vote::Comment) {
                "OK"
            } else {
                "FALLO"
            },
            "200",
            format!("voto={v:?}"),
        );
    }
    let r = cx.gh.vote(n, Vote::Comment, None).await;
    cx.err(
        "review COMMENT sin cuerpo (validación local)",
        r,
        "INVALID_REFERENCE",
        "-",
        "no sale a la red",
    );
    let repo = cx.repo.clone();
    let raw = cx
        .raw
        .send(
            Method::POST,
            &format!("/repos/{repo}/pulls/{n}/reviews"),
            Some(json!({"event": "APPROVE"})),
        )
        .await;
    let r = cx.gh.vote(n, Vote::Approve, None).await;
    let http = raw.status.to_string();
    // LÍMITE documentado: la autora no puede aprobar su propia PR.
    match r {
        Err(e) if e.code() == "POLICY" => cx.push(
            "review APPROVE propia PR",
            "LÍMITE",
            &http,
            format!("POLICY · {e}"),
        ),
        Err(e) => cx.push(
            "review APPROVE propia PR",
            "FALLO",
            &http,
            format!("{} · {e} · raw: {}", e.code(), raw.msg),
        ),
        Ok(_) => cx.push(
            "review APPROVE propia PR",
            "FALLO",
            &http,
            "GitHub la aceptó",
        ),
    }
    let raw = cx
        .raw
        .send(
            Method::POST,
            &format!("/repos/{repo}/pulls/{n}/reviews"),
            Some(json!({"event": "REQUEST_CHANGES", "body": "x"})),
        )
        .await;
    let r = cx
        .gh
        .vote(n, Vote::Reject, Some("Cambios pedidos por el spike"))
        .await;
    let http = raw.status.to_string();
    match r {
        Err(e) if e.code() == "POLICY" => cx.push(
            "review REQUEST_CHANGES propia PR",
            "LÍMITE",
            &http,
            format!("POLICY · {e}"),
        ),
        Err(e) => cx.push(
            "review REQUEST_CHANGES propia PR",
            "FALLO",
            &http,
            format!("{} · {e} · raw: {}", e.code(), raw.msg),
        ),
        Ok(_) => cx.push(
            "review REQUEST_CHANGES propia PR",
            "FALLO",
            &http,
            "GitHub la aceptó",
        ),
    }
}

async fn s08_cerrar_reabrir(cx: &mut Ctx, ids: &Ids) {
    let Some(n) = ids.a else { return };
    let r = cx.gh.close(n).await;
    if let Some(d) = cx.ok("cerrar PR A", r, "200", "") {
        cx.push(
            "cerrar: estado Abandoned",
            if d.summary.state == PrState::Abandoned {
                "OK"
            } else {
                "FALLO"
            },
            "200",
            format!("{:?}", d.summary.state),
        );
    }
    let r = cx.gh.list(&ListQuery::new(PrFilter::Closed, 1)).await;
    if let Some(p) = cx.ok("listar cerradas contiene A", r, "200", "") {
        let v = if p.items.iter().any(|i| i.id == n) {
            "OK"
        } else {
            "FALLO"
        };
        cx.push("cerradas incluye la PR cerrada", v, "200", "");
    }
    let r = cx.gh.reopen(n).await;
    if let Some(d) = cx.ok("reabrir PR A", r, "200", "") {
        cx.push(
            "reabrir: estado Active",
            if d.summary.state == PrState::Active {
                "OK"
            } else {
                "FALLO"
            },
            "200",
            format!("{:?}", d.summary.state),
        );
    }
}

async fn s09_errores_http(cx: &mut Ctx, ids: &Ids) {
    let repo = cx.repo.clone();
    let r = cx.gh.detail(99_999_999).await;
    let raw = cx
        .raw
        .send(Method::GET, &format!("/repos/{repo}/pulls/99999999"), None)
        .await;
    cx.err(
        "404 PR inexistente (detalle)",
        r,
        "NOT_FOUND",
        &raw.status.to_string(),
        &raw.msg,
    );
    let r = cx.gh.summary(99_999_999).await;
    cx.err("404 PR inexistente (resumen)", r, "NOT_FOUND", "404", "");
    let r = cx.gh.threads(99_999_999).await;
    cx.err(
        "404 hilos de PR inexistente",
        r,
        "NOT_FOUND",
        "200 GraphQL / 404",
        "",
    );
    // 401 deliberado: un único intento, solo contra /user, con credenciales falsas.
    let fake = provider_for(
        &RemoteInfo::parse(&format!("https://github.com/{repo}")),
        Arc::new(StaticCredentials::pat(
            "nadie",
            "ghp_falso_spike_0000000000",
        )),
        ProviderSettings::default(),
    )
    .expect("proveedor");
    match fake.current_user().await {
        Err(HostingError::Auth { message, .. }) if message.contains("rechazó el token") => cx
            .push(
                "401 token inválido (1 intento, GET /user)",
                "OK",
                "401",
                "AUTH «GitHub rechazó el token» (cuerpo real: Requires authentication)",
            ),
        other => cx.push(
            "401 token inválido (1 intento, GET /user)",
            "FALLO",
            "?",
            format!("{:?}", other.map(|u| u.login)),
        ),
    }
    // 409: sha obsoleto en el merge (no llega a fusionar).
    if let Some(n) = ids.a {
        let raw = cx
            .raw
            .send(Method::PUT, &format!("/repos/{repo}/pulls/{n}/merge"), Some(json!({"merge_method": "merge", "sha": "0000000000000000000000000000000000000000"})))
            .await;
        cx.push(
            "409 merge con sha obsoleto (raw)",
            if raw.status == 409 { "OK" } else { "FALLO" },
            &raw.status.to_string(),
            raw.msg,
        );
    }
}

async fn s10_completar(cx: &mut Ctx, ids: &Ids) {
    let repo = cx.repo.clone();
    // Borrador: no se puede fusionar.
    if let Some(n) = ids.b {
        let r = cx
            .gh
            .complete(
                n,
                &CompleteOptions {
                    strategy: MergeStrategy::Merge,
                    delete_source_branch: false,
                    complete_work_items: false,
                    auto_complete: false,
                    commit_message: None,
                },
            )
            .await;
        match r {
            Err(e) => cx.push(
                "completar borrador",
                "LÍMITE",
                "405/422",
                format!("{} · {e}", e.code()),
            ),
            Ok(_) => cx.push(
                "completar borrador",
                "FALLO",
                "200",
                "GitHub fusionó un borrador (inesperado)",
            ),
        }
    }
    // Conflicto: primero cf1 (merge), luego cf2 queda con conflicto.
    if let Some(c1) = ids.cf1 {
        let _ = cx.settled("detalle cf1 antes de fusionar", c1).await;
        let r = cx
            .gh
            .complete(
                c1,
                &CompleteOptions {
                    strategy: MergeStrategy::Merge,
                    delete_source_branch: true,
                    complete_work_items: false,
                    auto_complete: false,
                    commit_message: Some("Fusión cf1\n\nCuerpo del mensaje".into()),
                },
            )
            .await;
        if let Some(d) = cx.ok("completar cf1 (merge + borrar rama)", r, "200 + 204", "") {
            cx.push(
                "cf1: estado Completed",
                if d.summary.state == PrState::Completed {
                    "OK"
                } else {
                    "FALLO"
                },
                "200",
                format!("{:?} merge_detail={:?}", d.summary.state, d.merge_detail),
            );
            cx.branches.retain(|b| !b.starts_with("spike/cf1-"));
            let g = cx
                .raw
                .send(
                    Method::GET,
                    &format!("/repos/{repo}/branches/spike/cf1-{}", cx.ts),
                    None,
                )
                .await;
            cx.push(
                "borrar rama de origen tras merge",
                if g.status == 404 { "OK" } else { "FALLO" },
                &g.status.to_string(),
                "",
            );
        }
        let r = cx.gh.reopen(c1).await;
        let raw = cx
            .raw
            .send(
                Method::PATCH,
                &format!("/repos/{repo}/pulls/{c1}"),
                Some(json!({"state": "open"})),
            )
            .await;
        match r {
            Err(e) => cx.push(
                "reabrir una fusionada",
                "LÍMITE",
                &raw.status.to_string(),
                format!("{} · {e} · raw: {}", e.code(), raw.msg),
            ),
            Ok(_) => cx.push(
                "reabrir una fusionada",
                "FALLO",
                &raw.status.to_string(),
                "GitHub la reabrió",
            ),
        }
    }
    if let Some(c2) = ids.cf2 {
        let d = cx.settled("detalle cf2 (debe ser conflictiva)", c2).await;
        if let Some(d) = d {
            let v = if d.summary.merge_status == MergeStatus::Conflicts {
                "OK"
            } else {
                "FALLO"
            };
            cx.push(
                "cf2: merge_status=Conflicts",
                v,
                "200",
                format!("{:?} / {:?}", d.summary.merge_status, d.merge_detail),
            );
        }
        let raw = cx
            .raw
            .send(
                Method::PUT,
                &format!("/repos/{repo}/pulls/{c2}/merge"),
                Some(json!({"merge_method": "merge"})),
            )
            .await;
        let r = cx
            .gh
            .complete(
                c2,
                &CompleteOptions {
                    strategy: MergeStrategy::Merge,
                    delete_source_branch: false,
                    complete_work_items: false,
                    auto_complete: false,
                    commit_message: None,
                },
            )
            .await;
        let http = raw.status.to_string();
        match r {
            Err(e) if e.code() == "POLICY" || e.code() == "CONFLICT" => cx.push(
                "merge con conflicto",
                "OK",
                &http,
                format!("{} · {e} · raw: {}", e.code(), raw.msg),
            ),
            Err(e) => cx.push(
                "merge con conflicto",
                "FALLO",
                &http,
                format!("{} · {e}", e.code()),
            ),
            Ok(_) => cx.push("merge con conflicto", "FALLO", &http, "se fusionó"),
        }
    }
    // Estrategias.
    for (id, name, strat, msg) in [
        (ids.mrg, "merge", MergeStrategy::Merge, None),
        (
            ids.sq,
            "squash",
            MergeStrategy::Squash,
            Some("Squash del spike\n\ncuerpo".to_string()),
        ),
        (ids.rb, "rebase", MergeStrategy::Rebase, None),
    ] {
        let Some(n) = id else { continue };
        let r = cx
            .gh
            .complete(
                n,
                &CompleteOptions {
                    strategy: strat,
                    delete_source_branch: true,
                    complete_work_items: false,
                    auto_complete: false,
                    commit_message: msg,
                },
            )
            .await;
        if let Some(d) = cx.ok(&format!("completar con {name}"), r, "200", "") {
            let v = if d.summary.state == PrState::Completed {
                "OK"
            } else {
                "FALLO"
            };
            cx.push(
                &format!("{name}: estado Completed"),
                v,
                "200",
                format!("{:?} merge_detail={:?}", d.summary.state, d.merge_detail),
            );
        }
    }
    // Vuelve a pedir el listado de cerradas para ver las fusionadas.
    let r = cx.gh.list(&ListQuery::new(PrFilter::Closed, 1)).await;
    if let Some(p) = cx.ok("listar cerradas con fusionadas", r, "200", "") {
        let c = p
            .items
            .iter()
            .filter(|i| i.state == PrState::Completed)
            .count();
        let a = p
            .items
            .iter()
            .filter(|i| i.state == PrState::Abandoned)
            .count();
        cx.push(
            "cerradas: Completed vs Abandoned",
            if c >= 1 { "OK" } else { "FALLO" },
            "200",
            format!("{c} completadas, {a} abandonadas (el listado trae merged_at)"),
        );
    }
}

/// Cierra todas las PR abiertas y borra las ramas temporales que sigan existiendo.
async fn cleanup(cx: &mut Ctx) {
    let repo = cx.repo.clone();
    let mut closed = 0;
    for _ in 0..5 {
        let Ok(p) = cx.gh.list(&ListQuery::new(PrFilter::Open, 1)).await else {
            break;
        };
        if p.items.is_empty() {
            break;
        }
        for i in &p.items {
            if cx.gh.close(i.id).await.is_ok() {
                closed += 1;
            }
        }
    }
    let left = cx
        .gh
        .list(&ListQuery::new(PrFilter::Open, 1))
        .await
        .map(|p| p.items.len())
        .unwrap_or(99);
    cx.push(
        "limpieza: PR abiertas restantes",
        if left == 0 { "OK" } else { "FALLO" },
        "-",
        format!("cerradas {closed}; quedan {left}"),
    );
    let mut deleted = 0;
    for b in cx.branches.clone() {
        let r = cx
            .raw
            .send(
                Method::DELETE,
                &format!("/repos/{repo}/git/refs/heads/{b}"),
                None,
            )
            .await;
        if r.status == 204 {
            deleted += 1;
        }
    }
    cx.push(
        "limpieza: ramas temporales borradas",
        "OK",
        "204",
        format!("{deleted} borradas por API (permiso Contents: write basta; borrar repo no)"),
    );
}

async fn probe_permissions(
    cx: &mut Ctx,
    ids: &Ids,
    out: &mut Vec<(String, u16, String)>,
    scopes: &mut String,
) {
    let repo = cx.repo.clone();
    let n = ids.a.unwrap_or(1);
    let sha = cx
        .gh
        .detail(n)
        .await
        .ok()
        .and_then(|d| d.source_commit)
        .unwrap_or_default();
    let probes: Vec<(&str, Method, String, Option<Value>)> = vec![
        ("GET /user", Method::GET, "/user".into(), None),
        (
            "GET pulls (listar)",
            Method::GET,
            format!("/repos/{repo}/pulls?per_page=1"),
            None,
        ),
        (
            "GET pulls/{n}",
            Method::GET,
            format!("/repos/{repo}/pulls/{n}"),
            None,
        ),
        (
            "GET pulls/{n}/files",
            Method::GET,
            format!("/repos/{repo}/pulls/{n}/files"),
            None,
        ),
        (
            "GET pulls/{n}/reviews",
            Method::GET,
            format!("/repos/{repo}/pulls/{n}/reviews"),
            None,
        ),
        (
            "GET pulls/{n}/commits",
            Method::GET,
            format!("/repos/{repo}/pulls/{n}/commits"),
            None,
        ),
        (
            "GET commits/{sha}/check-runs",
            Method::GET,
            format!("/repos/{repo}/commits/{sha}/check-runs"),
            None,
        ),
        (
            "GET commits/{sha}/status",
            Method::GET,
            format!("/repos/{repo}/commits/{sha}/status"),
            None,
        ),
        (
            "GET issues/{n}/comments",
            Method::GET,
            format!("/repos/{repo}/issues/{n}/comments"),
            None,
        ),
        (
            "POST graphql (viewer)",
            Method::POST,
            "/graphql".into(),
            Some(json!({"query": "{viewer{login}}"})),
        ),
        (
            "POST pulls (crear, cuerpo inválido)",
            Method::POST,
            format!("/repos/{repo}/pulls"),
            Some(json!({})),
        ),
        (
            "PATCH pulls/{n} (cerrar, estado inválido)",
            Method::PATCH,
            format!("/repos/{repo}/pulls/{n}"),
            Some(json!({"state": "bogus"})),
        ),
        (
            "POST pulls/{n}/comments (en línea, inválido)",
            Method::POST,
            format!("/repos/{repo}/pulls/{n}/comments"),
            Some(json!({})),
        ),
        (
            "POST pulls/{n}/reviews (evento inválido)",
            Method::POST,
            format!("/repos/{repo}/pulls/{n}/reviews"),
            Some(json!({"event": "BOGUS"})),
        ),
        (
            "POST issues/{n}/comments (inválido)",
            Method::POST,
            format!("/repos/{repo}/issues/{n}/comments"),
            Some(json!({})),
        ),
        (
            "POST pulls/{n}/requested_reviewers (inválido)",
            Method::POST,
            format!("/repos/{repo}/pulls/{n}/requested_reviewers"),
            Some(json!({})),
        ),
        (
            "PUT pulls/99999999/merge (404)",
            Method::PUT,
            format!("/repos/{repo}/pulls/99999999/merge"),
            Some(json!({})),
        ),
        (
            "DELETE git/refs/heads/{inexistente}",
            Method::DELETE,
            format!("/repos/{repo}/git/refs/heads/spike/no-existe"),
            None,
        ),
    ];
    for (name, m, path, body) in probes {
        let r = cx.raw.send(m, &path, body).await;
        if scopes.is_empty() {
            *scopes = r.scopes.clone();
        }
        let p = if r.perms.is_empty() {
            r.accepted_scopes.clone()
        } else {
            r.perms.clone()
        };
        let note = if r.status >= 400 && !r.msg.is_empty() {
            format!("{p} · {}", r.msg.chars().take(80).collect::<String>())
        } else {
            p
        };
        out.push((name.to_owned(), r.status, note));
    }
}
