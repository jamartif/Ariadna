//! Historial paginado y ramas con confirmación (`specs/rust/ipc.md`).

use ariadna_git::{Commit, IntegrationState, MergeFastForward, MergeResult};

use super::api::Done;
use super::app::AppCore;
use super::error::{ApiError, ApiResult};

/// Tope por página (el motor limita además a 2 000 por llamada).
pub const MAX_PAGE: usize = 500;

impl AppCore {
    /// Una página del historial: `limit` commits tras saltar `skip`. Con `query` busca en todo el
    /// historial de HEAD (mensaje, autor o hash) y pagina el resultado.
    pub async fn log_page(
        &self,
        tab_id: &str,
        skip: usize,
        limit: usize,
        query: Option<String>,
    ) -> ApiResult<Vec<Commit>> {
        let limit = limit.clamp(1, MAX_PAGE);
        self.read_op(tab_id, "historial", move |e, r| {
            Box::pin(async move {
                match query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
                    Some(q) => {
                        let all = e.search_log(&r, q, skip.saturating_add(limit)).await?;
                        Ok(all.into_iter().skip(skip).collect())
                    }
                    None => e.log_page(&r, skip, limit).await,
                }
            })
        })
        .await
    }

    /// Página siguiente del historial por **cursor** (sin `--skip`): `cursor` es la frontera del
    /// recorrido (ver `ariadna_git::history_cursor`), `limit` el tamaño de la página. El orden es
    /// el de `log_page`; el coste no depende de la profundidad.
    pub async fn log_after(
        &self,
        tab_id: &str,
        cursor: Vec<String>,
        limit: usize,
    ) -> ApiResult<Vec<Commit>> {
        let limit = limit.clamp(1, MAX_PAGE);
        self.read_op(tab_id, "historial", move |e, r| {
            Box::pin(async move { e.log_after(&r, &cursor, limit).await })
        })
        .await
    }

    /// Borrar una rama local con confirmación nativa. Si git la considera no fusionada se pide
    /// una segunda confirmación, más seria, para forzar el borrado.
    pub async fn delete_branch(&self, tab_id: &str, name: String, force: bool) -> ApiResult<Done> {
        let (s, meta) = self.live(tab_id)?;
        let ok = self
            .inner
            .prompter
            .confirm(
                "Borrar rama",
                &format!(
                    "En {}: se borrará la rama local «{name}».{}",
                    Self::context(&s, &meta),
                    if force {
                        " Se fuerza el borrado aunque tenga commits sin fusionar."
                    } else {
                        ""
                    }
                ),
                &format!("Rama: {name}\nRepositorio: {}", meta.path),
            )
            .await;
        if !ok {
            return Ok(Done { done: false });
        }
        let attempt = |force: bool| {
            self.mutate_op(
                tab_id,
                "borrar rama",
                (name.clone(), force),
                |e, r, (n, f)| Box::pin(async move { e.delete_branch(&r, &n, f).await }),
            )
        };
        match attempt(force).await {
            Ok(()) => Ok(Done { done: true }),
            // Único texto de git que se mira aquí (`LC_ALL=C`): `git branch -d` no distingue
            // «no fusionada» con otro código de salida.
            Err(e)
                if !force
                    && e.detail
                        .as_deref()
                        .is_some_and(|d| d.contains("not fully merged")) =>
            {
                let again = self
                    .inner
                    .prompter
                    .confirm(
                        "La rama no está fusionada",
                        &format!(
                            "«{name}» tiene commits que no están en ninguna otra rama. Si la borras podrías perderlos."
                        ),
                        "¿Borrarla de todos modos?",
                    )
                    .await;
                if !again {
                    return Ok(Done { done: false });
                }
                attempt(true).await?;
                Ok(Done { done: true })
            }
            Err(e) => Err(e),
        }
    }

    /// Fusiona `branch` en la rama actual **solo si no hay conflictos**. Si los hay, aborta el
    /// merge (el repo queda como estaba) y devuelve `MERGE_CONFLICTS` con las rutas afectadas.
    pub async fn merge_clean(
        &self,
        tab_id: &str,
        branch: String,
        fast_forward: MergeFastForward,
    ) -> ApiResult<MergeResult> {
        let result = self.merge(tab_id, branch.clone(), fast_forward).await?;
        let MergeResult::Conflicts { paths } = &result else {
            return Ok(result);
        };
        let shown: Vec<String> = paths
            .iter()
            .take(20)
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        // Aborto limpio: sin diálogo (el usuario no ha resuelto nada que perder).
        let state = self
            .read_op(tab_id, "estado de la integración", |e, r| {
                Box::pin(async move { e.integration_state(&r).await })
            })
            .await?;
        if state != IntegrationState::None {
            self.mutate_op(tab_id, "abortar merge", (), |e, r, ()| {
                Box::pin(async move { e.abort_integration(&r).await })
            })
            .await?;
        }
        Err(ApiError::new(
            "MERGE_CONFLICTS",
            format!(
                "Hay conflictos al fusionar «{branch}» ({} archivo(s)). La fusión se ha cancelado y no se ha cambiado nada. Usa `merge_start` para dejarla en curso y resolver los conflictos.",
                paths.len()
            ),
            Some(shown.join("\n")),
        ))
    }
}
