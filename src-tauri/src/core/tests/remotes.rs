//! Remotos: lista con proveedor, añadir / renombrar / quitar con confirmación nativa y
//! resolución de enlaces web validados. Remotos locales (carpetas bare) y URL falsas en la
//! configuración; nada toca la red.

use ariadna_hosting::Provider;

use super::*;
use crate::core::WebTarget;

fn set_remote(env: &Env, repo: &Path, name: &str, url: &str) {
    env.git(repo, &["remote", "add", name, url]);
}

async fn url_of(
    env: &Env,
    tab: &str,
    remote: &str,
    kind: WebTarget,
    branch: Option<&str>,
) -> Result<String, crate::core::ApiError> {
    env.core
        .remote_web_url(tab, remote, kind, branch)
        .await
        .map(|u| u.to_string())
}

#[tokio::test]
async fn lista_remotos_con_proveedor_y_capacidades() {
    let env = Env::new();
    let repo = env.repo("r");
    set_remote(&env, &repo, "origin", "git@github.com:acme/app.git");
    set_remote(
        &env,
        &repo,
        "azure",
        "https://org@dev.azure.com/org/proj/_git/app",
    );
    set_remote(&env, &repo, "local", &Env::s(&env.bare("bare.git")));
    let tab = env.open(&repo).await;

    let views = env.core.list_remote_views(&tab).await.unwrap();
    let by = |n: &str| views.iter().find(|v| v.name == n).unwrap();
    assert_eq!(by("origin").provider, Provider::GitHub);
    assert_eq!(by("origin").host, "github.com");
    assert!(by("origin").ssh && by("origin").can_open && by("origin").can_create_pull_request);
    assert_eq!(by("azure").provider, Provider::AzureDevOps);
    assert_eq!(by("azure").provider_label, "Azure DevOps");
    assert!(by("azure").can_create_pull_request);
    // Remoto local: sin proveedor ni página web.
    assert_eq!(by("local").provider, Provider::Other);
    assert!(!by("local").can_open && !by("local").can_create_pull_request);
    // La serialización que ve la UI.
    let json = serde_json::to_value(by("origin")).unwrap();
    assert_eq!(json["provider"], "github");
    assert_eq!(json["canCreatePullRequest"], true);
    assert_eq!(
        serde_json::to_value(by("azure")).unwrap()["provider"],
        "azureDevOps"
    );
}

#[tokio::test]
async fn anadir_remoto_pide_confirmacion_y_cancelar_no_cambia_nada() {
    let env = Env::new();
    let repo = env.repo("r");
    let bare = env.bare("bare.git");
    let tab = env.open(&repo).await;

    env.prompter.reject_all();
    let r = env
        .core
        .add_remote(&tab, "origin".into(), Env::s(&bare))
        .await
        .unwrap();
    assert!(!r.done);
    let asked = env.prompter.last();
    assert!(asked.message.contains("«origin»"), "{asked:?}");
    assert!(asked.detail.contains(&Env::s(&bare)), "{asked:?}");
    assert!(env.git(&repo, &["remote"]).trim().is_empty());

    env.prompter.accept_all();
    let r = env
        .core
        .add_remote(&tab, "  origin ".into(), format!(" {} ", Env::s(&bare)))
        .await
        .unwrap();
    assert!(r.done);
    assert_eq!(env.git(&repo, &["remote"]).trim(), "origin");
    // El snapshot (y por tanto la lista) lo refleja sin que la UI pida nada.
    let views = env.core.list_remote_views(&tab).await.unwrap();
    assert_eq!(views.len(), 1);
}

#[tokio::test]
async fn anadir_remoto_rechaza_contrasenas_transportes_raros_y_nombres_malos_sin_preguntar() {
    let env = Env::new();
    let repo = env.repo("r");
    let tab = env.open(&repo).await;
    let before = env.prompter.count();
    for (name, url) in [
        ("o", "https://usuario:secreto@github.com/a/b.git"),
        ("o", "https://:token@github.com/a/b.git"),
        (
            "o",
            "https://ghp_abcdefghijklmnopqrstuvwxyz0123456789@github.com/o/r.git",
        ),
        ("o", "https://github_pat_11ABCDEF@github.com/o/r.git"),
        ("o", "https://glpat-xxxxxxxxxxxxxxxxxxxx@gitlab.com/o/r.git"),
        ("o", "https://usuario%3Asecreto@github.com/a/b.git"),
        (
            "o",
            "https://abcdefghijklmnopqrstuvwxyz0123456789abcd@dev.azure.com/o/p/_git/r",
        ),
        ("o", "ext::sh -c touch% /tmp/pwned"),
        ("o", "fd::17/foo"),
        ("o", "-oProxyCommand=x"),
        ("o", ""),
        ("o", "https://github.com/a\nb"),
        ("", "https://github.com/a/b.git"),
        ("-x", "https://github.com/a/b.git"),
        ("a b", "https://github.com/a/b.git"),
    ] {
        let e = env
            .core
            .add_remote(&tab, name.into(), url.into())
            .await
            .unwrap_err();
        assert_eq!(e.code, "INVALID_REFERENCE", "{name:?} {url:?}");
        assert!(!e.message.contains("secreto"), "no se repite el secreto");
        assert!(!e.message.contains("ghp_"), "no se repite el token");
    }
    assert_eq!(
        env.prompter.count(),
        before,
        "no se pregunta por entradas inválidas"
    );
    assert!(env.git(&repo, &["remote"]).trim().is_empty());
    // Con usuario sin contraseña sí vale.
    let r = env
        .core
        .add_remote(
            &tab,
            "ok".into(),
            "https://usuario@github.com/a/b.git".into(),
        )
        .await
        .unwrap();
    assert!(r.done);
    // Y también una organización de Azure DevOps como usuario.
    let r = env
        .core
        .add_remote(
            &tab,
            "azure".into(),
            "https://mi-organizacion@dev.azure.com/mi-organizacion/p/_git/r".into(),
        )
        .await
        .unwrap();
    assert!(r.done);
}

#[test]
fn userinfo_que_parece_token_se_rechaza() {
    use crate::core::remote_ops::userinfo_has_secret;
    for secret in [
        "u:p",
        ":token",
        "u%3ap",
        "ghp_xxxxxxxxxxxxxxxxxxxx",
        "GHP_short",
        "github_pat_x",
        "glpat-x",
        "0123456789abcdef0123456789abcdef",
    ] {
        assert!(userinfo_has_secret(secret), "{secret}");
    }
    for user in [
        "git",
        "usuario",
        "mi-organizacion",
        "jamartif",
        "first.last",
    ] {
        assert!(!userinfo_has_secret(user), "{user}");
    }
}

#[tokio::test]
async fn renombrar_remoto_actualiza_ramas_de_seguimiento_y_upstream() {
    let env = Env::new();
    let repo = env.repo("r");
    let bare = env.bare("bare.git");
    set_remote(&env, &repo, "origin", &Env::s(&bare));
    let tab = env.open(&repo).await;
    env.core
        .push(&tab, "origin".into(), Some("main".into()), true)
        .await
        .unwrap();

    env.prompter.reject_all();
    let r = env
        .core
        .rename_remote(&tab, "origin".into(), "principal".into())
        .await
        .unwrap();
    assert!(!r.done);
    assert_eq!(env.git(&repo, &["remote"]).trim(), "origin");

    env.prompter.accept_all();
    let r = env
        .core
        .rename_remote(&tab, "origin".into(), "principal".into())
        .await
        .unwrap();
    assert!(r.done);
    let asked = env.prompter.last();
    assert!(asked.detail.contains("Antes: origin") && asked.detail.contains("Después: principal"));
    assert_eq!(env.git(&repo, &["remote"]).trim(), "principal");
    assert_eq!(
        env.git(&repo, &["config", "branch.main.remote"]).trim(),
        "principal"
    );
    let snap = env.core.get_snapshot(&tab).await.unwrap();
    assert!(snap
        .0
        .remote_branches
        .iter()
        .any(|b| b.name == "principal/main"));
    assert!(snap.0.remotes.iter().all(|r| r.name == "principal"));
}

#[tokio::test]
async fn renombrar_remoto_valida_antes_de_preguntar() {
    let env = Env::new();
    let repo = env.repo("r");
    set_remote(&env, &repo, "origin", &Env::s(&env.bare("bare.git")));
    let tab = env.open(&repo).await;
    let before = env.prompter.count();
    for new in ["", "-x", "a b", "a..b"] {
        let e = env
            .core
            .rename_remote(&tab, "origin".into(), new.into())
            .await
            .unwrap_err();
        assert_eq!(e.code, "INVALID_REFERENCE", "{new:?}");
    }
    let e = env
        .core
        .rename_remote(&tab, "fantasma".into(), "x".into())
        .await
        .unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
    assert_eq!(env.prompter.count(), before);
}

#[tokio::test]
async fn quitar_remoto_avisa_de_lo_que_se_pierde_y_respeta_cancelar() {
    let env = Env::new();
    let repo = env.repo("r");
    let bare = env.bare("bare.git");
    set_remote(&env, &repo, "origin", &Env::s(&bare));
    let tab = env.open(&repo).await;
    env.core
        .push(&tab, "origin".into(), Some("main".into()), true)
        .await
        .unwrap();

    env.prompter.reject_all();
    let r = env.core.remove_remote(&tab, "origin".into()).await.unwrap();
    assert!(!r.done);
    let asked = env.prompter.last();
    assert!(
        asked.message.contains("1 ramas de seguimiento"),
        "{asked:?}"
    );
    assert!(asked.message.contains("1 ramas locales"), "{asked:?}");
    assert!(asked.message.contains("No se toca el servidor"));
    assert!(asked.detail.contains(&Env::s(&bare)));
    assert_eq!(env.git(&repo, &["remote"]).trim(), "origin");

    env.prompter.accept_all();
    let r = env.core.remove_remote(&tab, "origin".into()).await.unwrap();
    assert!(r.done);
    assert!(env.git(&repo, &["remote"]).trim().is_empty());
    // El servidor sigue intacto.
    assert_eq!(
        env.git(&bare, &["rev-parse", "main"]),
        env.git(&repo, &["rev-parse", "HEAD"])
    );
    let e = env
        .core
        .remove_remote(&tab, "origin".into())
        .await
        .unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
}

#[tokio::test]
async fn fetch_pull_y_push_funcionan_contra_un_remoto_local() {
    let env = Env::new();
    let repo = env.repo("r");
    let bare = env.bare("bare.git");
    set_remote(&env, &repo, "origin", &Env::s(&bare));
    let tab = env.open(&repo).await;
    env.core
        .push(&tab, "origin".into(), Some("main".into()), true)
        .await
        .unwrap();

    // Otro clon sube un commit; fetch lo trae y pull lo integra.
    let other = env.root.join("otro");
    env.git(&env.root, &["clone", "-q", &Env::s(&bare), &Env::s(&other)]);
    env.write(&other, "nuevo.txt", b"x\n");
    env.git(&other, &["add", "-A"]);
    env.git(&other, &["commit", "-q", "-m", "desde otro"]);
    env.git(&other, &["push", "-q", "origin", "main"]);

    env.core.fetch(&tab, "origin".into()).await.unwrap();
    let snap = env.core.get_snapshot(&tab).await.unwrap();
    let main = snap.0.branches.iter().find(|b| b.name == "main").unwrap();
    assert_eq!(main.behind, 1);
    env.core
        .pull(&tab, "origin".into(), None, Some(false))
        .await
        .unwrap();
    assert!(repo.join("nuevo.txt").exists());
}

#[tokio::test]
async fn enlaces_web_por_proveedor_con_host_del_remoto() {
    let env = Env::new();
    let repo = env.repo("r");
    set_remote(&env, &repo, "gh", "git@github.com:acme/app.git");
    set_remote(
        &env,
        &repo,
        "az",
        "https://org@dev.azure.com/org/proj/_git/app",
    );
    set_remote(&env, &repo, "gl", "https://gitlab.com/g/s/app.git");
    set_remote(&env, &repo, "bb", "git@bitbucket.org:w/app.git");
    set_remote(&env, &repo, "ghe", "https://github.acme.com/o/r.git");
    let tab = env.open(&repo).await;
    let u = |r: &'static str, k, b| {
        let env = &env;
        let tab = tab.clone();
        async move { url_of(env, &tab, r, k, b).await }
    };

    assert_eq!(
        u("gh", WebTarget::Repo, None).await.unwrap(),
        "https://github.com/acme/app"
    );
    assert_eq!(
        u("gh", WebTarget::Branch, Some("feature/x")).await.unwrap(),
        "https://github.com/acme/app/tree/feature/x"
    );
    assert_eq!(
        u("gh", WebTarget::PullRequest, Some("feature/x"))
            .await
            .unwrap(),
        "https://github.com/acme/app/pull/new/feature/x"
    );
    assert_eq!(
        u("az", WebTarget::PullRequest, Some("feature/x"))
            .await
            .unwrap(),
        "https://dev.azure.com/org/proj/_git/app/pullrequestcreate?sourceRef=feature%2Fx"
    );
    assert_eq!(
        u("az", WebTarget::Branch, Some("main")).await.unwrap(),
        "https://dev.azure.com/org/proj/_git/app?version=GBmain"
    );
    assert!(u("gl", WebTarget::PullRequest, Some("a"))
        .await
        .unwrap()
        .contains("/-/merge_requests/new"));
    assert!(u("bb", WebTarget::PullRequest, Some("a"))
        .await
        .unwrap()
        .contains("/pull-requests/new"));
    // Servidor propio reconocido por heurística: el host es el del remoto.
    let ghe = u("ghe", WebTarget::PullRequest, Some("a")).await.unwrap();
    assert!(
        ghe.starts_with("https://github.acme.com/o/r/pull/new/"),
        "{ghe}"
    );
}

#[tokio::test]
async fn enlaces_web_rechazan_lo_que_no_se_puede_validar() {
    let env = Env::new();
    let repo = env.repo("r");
    set_remote(&env, &repo, "local", &Env::s(&env.bare("bare.git")));
    set_remote(&env, &repo, "gh", "https://github.com/acme/app.git");
    set_remote(&env, &repo, "otro", "https://git.acme.com/x/r.git");
    set_remote(&env, &repo, "puerto", "https://git.acme.com:8443/x/r.git");
    set_remote(&env, &repo, "js", "javascript:alert(1)");
    set_remote(&env, &repo, "falso", "https://github.com.evil.com/o/r.git");
    let tab = env.open(&repo).await;

    // Remoto local, esquema raro o con puerto: sin página web conocida (POLICY).
    for r in ["local", "js"] {
        let e = url_of(&env, &tab, r, WebTarget::Repo, None)
            .await
            .unwrap_err();
        assert_eq!(e.code, "POLICY", "{r}");
    }
    // El puerto de una URL https no se conserva (no se distingue del de SSH): el enlace va al
    // puerto estándar del mismo host, nunca a otro host.
    assert_eq!(
        url_of(&env, &tab, "puerto", WebTarget::Repo, None)
            .await
            .unwrap(),
        "https://git.acme.com/x/r"
    );
    // Proveedor desconocido: repo y rama sí (host del remoto); PR no.
    assert_eq!(
        url_of(&env, &tab, "otro", WebTarget::Repo, None)
            .await
            .unwrap(),
        "https://git.acme.com/x/r"
    );
    let e = url_of(&env, &tab, "otro", WebTarget::PullRequest, Some("a"))
        .await
        .unwrap_err();
    assert_eq!(e.code, "POLICY");
    // `github.com.evil.com` no se hace pasar por GitHub: no hay PR y el host es el suyo.
    let e = url_of(&env, &tab, "falso", WebTarget::PullRequest, Some("a"))
        .await
        .unwrap_err();
    assert_eq!(e.code, "POLICY");
    let repo_url = url_of(&env, &tab, "falso", WebTarget::Repo, None)
        .await
        .unwrap();
    assert!(
        repo_url.starts_with("https://github.com.evil.com/"),
        "{repo_url}"
    );
    // Ramas inválidas o ausentes y remotos inexistentes.
    for b in [
        None,
        Some(""),
        Some("--upload-pack=x"),
        Some("a..b"),
        Some("a b"),
        Some("a\nb"),
    ] {
        let e = url_of(&env, &tab, "gh", WebTarget::PullRequest, b)
            .await
            .unwrap_err();
        assert_eq!(e.code, "INVALID_REFERENCE", "{b:?}");
    }
    let e = url_of(&env, &tab, "no-existe", WebTarget::Repo, None)
        .await
        .unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
    // `?` no es válido en una rama de git: se rechaza antes de construir nada.
    let e = url_of(&env, &tab, "gh", WebTarget::PullRequest, Some("x?y"))
        .await
        .unwrap_err();
    assert_eq!(e.code, "INVALID_REFERENCE");
    // Caracteres válidos en git pero con significado en una URL se codifican y no escapan de la ruta.
    let tricky = url_of(&env, &tab, "gh", WebTarget::PullRequest, Some("r&d#1%2F"))
        .await
        .unwrap();
    assert_eq!(
        tricky,
        "https://github.com/acme/app/pull/new/r%26d%231%252F"
    );
}
