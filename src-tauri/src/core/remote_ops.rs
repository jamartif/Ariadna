//! Remotos de una pestaña: lista con proveedor, añadir / renombrar / quitar con confirmación
//! nativa y resolución de los enlaces «Abrir en el navegador» / «Crear pull request» (D19).
//!
//! `fetch`, `pull` y `push` ya existen como comandos de pestaña (`commands.rs`). Aquí **no** se
//! abre nada: [`AppCore::remote_web_url`] devuelve una URL validada y la capa Tauri la abre.

use ariadna_git::{validate_ref, Remote};
use ariadna_hosting::{Provider, RemoteInfo};
use serde::{Deserialize, Serialize};
use url::Url;

use super::api::Done;
use super::app::AppCore;
use super::error::{ApiError, ApiResult};
use super::external::validate_remote_web_url;

/// Qué página del proveedor se quiere abrir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebTarget {
    /// Página principal del repositorio.
    Repo,
    /// Página de una rama (`branch` obligatorio).
    Branch,
    /// Página de «nueva pull request» desde `branch` (solo enlace; la API es M8).
    PullRequest,
}

/// Un remoto con lo que la UI necesita para pintarlo (proveedor, host, enlaces disponibles).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteView {
    pub name: String,
    pub fetch_url: String,
    pub push_url: String,
    pub provider: Provider,
    pub provider_label: String,
    /// Host normalizado (`dev.azure.com` para todas las formas de Azure DevOps). Vacío en local.
    pub host: String,
    /// `true` si hay página web del repositorio y su host pasa la política de enlaces.
    pub can_open: bool,
    /// `true` si el proveedor tiene página de «nueva pull request» conocida.
    pub can_create_pull_request: bool,
    /// Usa SSH (`git@host:ruta` o `ssh://`).
    pub ssh: bool,
}

impl RemoteView {
    fn of(remote: &Remote) -> Self {
        let info = RemoteInfo::parse(&remote.fetch_url);
        let can_open = info
            .repo_web_url()
            .is_some_and(|u| validate_remote_web_url(&u, &info.host).is_ok());
        let can_create_pull_request = can_open
            && info.provider != Provider::Other
            && info.new_pull_request_url("x").is_some();
        Self {
            name: remote.name.clone(),
            fetch_url: remote.fetch_url.clone(),
            push_url: remote.push_url.clone(),
            provider: info.provider,
            provider_label: info.provider.label().to_owned(),
            host: info.host.clone(),
            can_open,
            can_create_pull_request,
            ssh: info.ssh,
        }
    }
}

fn invalid(what: &str) -> ApiError {
    ApiError::new("INVALID_REFERENCE", what.to_owned(), None)
}

/// Rechaza lo que no debe llegar a `git remote add`: contraseñas en la URL (acabarían en
/// `.git/config` en claro) y transportes auxiliares `algo::…` (p. ej. `ext::`, que ejecutaría
/// programas). `-x` lo rechaza el motor.
pub(crate) fn check_new_remote_url(url: &str) -> ApiResult<()> {
    let url = url.trim();
    if url.is_empty() {
        return Err(invalid("La dirección del remoto está vacía."));
    }
    if url.starts_with('-') {
        return Err(invalid("La dirección del remoto no puede empezar por «-»."));
    }
    if url.chars().any(char::is_control) {
        return Err(invalid(
            "La dirección del remoto lleva caracteres no válidos.",
        ));
    }
    let scheme_end = url.find(|c: char| !(c.is_ascii_alphanumeric() || "+.-".contains(c)));
    if scheme_end.is_some_and(|i| url[i..].starts_with("::")) {
        return Err(invalid(
            "Ese tipo de dirección (transporte auxiliar de git) no se admite.",
        ));
    }
    if let Some((_, rest)) = url.split_once("://") {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        if let Some((userinfo, _)) = authority.rsplit_once('@') {
            if userinfo.contains(':') {
                return Err(ApiError::new(
                    "INVALID_REFERENCE",
                    "La dirección no puede llevar la contraseña o el token: se guardaría en claro en el repositorio. Quítala; git la pedirá cuando haga falta.",
                    None,
                ));
            }
        }
    }
    Ok(())
}

impl AppCore {
    /// Remotos de la pestaña con proveedor y capacidades de enlace.
    pub async fn list_remote_views(&self, tab_id: &str) -> ApiResult<Vec<RemoteView>> {
        let (s, _) = self.live(tab_id)?;
        let snap = Self::fresh_snapshot(&s).await;
        Ok(snap.remotes.iter().map(RemoteView::of).collect())
    }

    /// Añade un remoto tras confirmación nativa.
    pub async fn add_remote(&self, tab_id: &str, name: String, url: String) -> ApiResult<Done> {
        let name = name.trim().to_owned();
        let url = url.trim().to_owned();
        check_new_remote_url(&url)?;
        if validate_ref(&name).is_err() {
            return Err(invalid("El nombre del remoto no es válido."));
        }
        let (s, meta) = self.live(tab_id)?;
        let ok = self
            .inner
            .prompter
            .confirm(
                "Añadir remoto",
                &format!(
                    "En {}: se añadirá el remoto «{name}».",
                    Self::context(&s, &meta)
                ),
                &format!(
                    "Remoto: {name}\nDirección: {url}\nRepositorio: {}",
                    meta.path
                ),
            )
            .await;
        if !ok {
            return Ok(Done { done: false });
        }
        self.mutate_op(tab_id, "añadir remoto", (name, url), |e, r, (n, u)| {
            Box::pin(async move { e.add_remote(&r, &n, &u).await })
        })
        .await?;
        Ok(Done { done: true })
    }

    /// Renombra un remoto tras confirmación nativa (git reescribe sus ramas de seguimiento).
    pub async fn rename_remote(&self, tab_id: &str, old: String, new: String) -> ApiResult<Done> {
        let new = new.trim().to_owned();
        if validate_ref(&new).is_err() {
            return Err(invalid("El nombre nuevo del remoto no es válido."));
        }
        let (s, meta) = self.live(tab_id)?;
        let snap = Self::fresh_snapshot(&s).await;
        if !snap.remotes.iter().any(|r| r.name == old) {
            return Err(invalid("Ese remoto ya no existe."));
        }
        let ok = self
            .inner
            .prompter
            .confirm(
                "Renombrar remoto",
                &format!(
                    "En {}: el remoto «{old}» pasará a llamarse «{new}». Sus ramas de seguimiento y las ramas locales que lo usan como origen se actualizan.",
                    Self::context(&s, &meta)
                ),
                &format!("Antes: {old}\nDespués: {new}\nRepositorio: {}", meta.path),
            )
            .await;
        if !ok {
            return Ok(Done { done: false });
        }
        self.mutate_op(tab_id, "renombrar remoto", (old, new), |e, r, (o, n)| {
            Box::pin(async move { e.rename_remote(&r, &o, &n).await })
        })
        .await?;
        Ok(Done { done: true })
    }

    /// Quita un remoto tras confirmación nativa que avisa de lo que se pierde.
    pub async fn remove_remote(&self, tab_id: &str, name: String) -> ApiResult<Done> {
        let (s, meta) = self.live(tab_id)?;
        let snap = Self::fresh_snapshot(&s).await;
        let Some(remote) = snap.remotes.iter().find(|r| r.name == name) else {
            return Err(invalid("Ese remoto ya no existe."));
        };
        let prefix = format!("{name}/");
        let tracking = snap
            .remote_branches
            .iter()
            .filter(|b| b.name.starts_with(&prefix))
            .count();
        let using = snap
            .branches
            .iter()
            .filter(|b| {
                b.upstream
                    .as_deref()
                    .is_some_and(|u| u.starts_with(&prefix))
            })
            .count();
        let ok = self
            .inner
            .prompter
            .confirm(
                "Quitar remoto",
                &format!(
                    "En {}: se quitará el remoto «{name}». Se borran sus {tracking} ramas de seguimiento locales y {using} ramas locales dejarán de tener origen. No se toca el servidor.",
                    Self::context(&s, &meta)
                ),
                &format!(
                    "Remoto: {name}\nDirección: {}\nRepositorio: {}",
                    remote.fetch_url, meta.path
                ),
            )
            .await;
        if !ok {
            return Ok(Done { done: false });
        }
        self.mutate_op(tab_id, "quitar remoto", name, |e, r, n| {
            Box::pin(async move { e.remove_remote(&r, &n).await })
        })
        .await?;
        Ok(Done { done: true })
    }

    /// URL web (repo, rama o nueva PR) de un remoto de la pestaña, **validada**: https, sin
    /// credenciales y con el host del propio remoto. El host sale de la configuración real del
    /// repo, nunca de lo que mande la UI; de la UI solo llegan el nombre del remoto y la rama.
    pub async fn remote_web_url(
        &self,
        tab_id: &str,
        remote: &str,
        target: WebTarget,
        branch: Option<&str>,
    ) -> ApiResult<Url> {
        let (s, _) = self.live(tab_id)?;
        let snap = Self::fresh_snapshot(&s).await;
        let Some(r) = snap.remotes.iter().find(|r| r.name == remote) else {
            return Err(invalid("Ese remoto ya no existe."));
        };
        let info = RemoteInfo::parse(&r.fetch_url);
        let branch = match (target, branch.map(str::trim)) {
            (WebTarget::Repo, _) => None,
            (_, Some(b)) if validate_ref(b).is_ok() => Some(b),
            _ => return Err(invalid("Falta una rama válida para ese enlace.")),
        };
        let raw = match (target, branch) {
            (WebTarget::Repo, _) => info.repo_web_url(),
            (WebTarget::Branch, Some(b)) => info.branch_web_url(b),
            (WebTarget::PullRequest, Some(b)) => info.new_pull_request_url(b),
            _ => None,
        };
        let Some(raw) = raw else {
            return Err(ApiError::new(
                "POLICY",
                format!(
                    "No se conoce la página web de «{remote}» ({}): no se puede abrir ese enlace.",
                    info.provider.label()
                ),
                None,
            ));
        };
        validate_remote_web_url(&raw, &info.host)
    }
}
