//! Acciones del menú contextual del riel de ramas que no tenían comando propio: publicar una
//! rama cualquiera (`push_branch`), rebasar la actual sobre otra (`rebase_onto`), borrar una rama
//! en el remoto (`delete_remote_branch`) y el diff de una rama contra otra (`branch_diff_against`,
//! `git diff A...B`).
//!
//! El resto de acciones del menú reutilizan lo que ya había: `create_branch`, `checkout`,
//! `rename_branch`, `delete_branch`, `merge_start`, `fetch`, `pull` y `open_remote_url`.

use ariadna_git::{
    validate_ref, Confirmation, FileDiff, IntegrationState, MergeResult, Remote, Snapshot,
};
use serde::Serialize;

use super::api::Done;
use super::app::AppCore;
use super::error::{ApiError, ApiResult};
use super::merge_ops::MergeState;

/// Remoto en el que se publicó la rama (la UI lo nombra en el aviso).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushBranchResult {
    pub remote: String,
}

/// Resultado de un rebase. `done: false` = el usuario canceló la confirmación nativa (no se tocó
/// nada). Con `state`, el rebase quedó **en curso** con conflictos (se resuelven como un merge).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseOutcome {
    pub done: bool,
    pub result: MergeResult,
    pub state: Option<MergeState>,
}

fn invalid(what: impl Into<String>) -> ApiError {
    ApiError::new("INVALID_REFERENCE", what.into(), None)
}

impl AppCore {
    /// Remoto de una rama local: el de su upstream; si no tiene, `origin`; si no, el primero.
    /// `remotes` es la configuración **real** del repo (`git remote`), no el snapshot: así una
    /// rama recién borrada o un remoto quitado hace un momento no despistan.
    fn remote_of(snap: &Snapshot, remotes: &[Remote], branch: &str) -> Option<String> {
        let local = snap.branches.iter().find(|b| b.name == branch)?;
        if let Some(upstream) = local.upstream.as_deref() {
            if let Some((remote, _)) = upstream.split_once('/') {
                if remotes.iter().any(|r| r.name == remote) {
                    return Some(remote.to_owned());
                }
            }
        }
        if remotes.iter().any(|r| r.name == "origin") {
            return Some("origin".to_owned());
        }
        remotes.first().map(|r| r.name.clone())
    }

    /// Publica `branch` en su remoto (`git push [--set-upstream] <remoto> <rama>`). El remoto lo
    /// decide Rust (el del upstream, o `origin`, o el primero): la UI no lo elige. Con
    /// `set_upstream` y una rama sin origen, queda publicada y siguiéndolo.
    pub async fn push_branch(
        &self,
        tab_id: &str,
        branch: String,
        set_upstream: bool,
    ) -> ApiResult<PushBranchResult> {
        let branch = branch.trim().to_owned();
        if validate_ref(&branch).is_err() {
            return Err(invalid("El nombre de la rama no es válido."));
        }
        let (s, _) = self.live(tab_id)?;
        let snap = Self::fresh_snapshot(&s).await;
        if !snap.branches.iter().any(|b| b.name == branch) {
            return Err(invalid("Esa rama ya no existe."));
        }
        let remotes = self
            .read_op(tab_id, "remotos", |e, r| {
                Box::pin(async move { e.list_remotes(&r).await })
            })
            .await?;
        let Some(remote) = Self::remote_of(&snap, &remotes, &branch) else {
            return Err(ApiError::new(
                "POLICY",
                "Este repositorio no tiene remotos: no hay dónde subir la rama.",
                None,
            ));
        };
        self.mutate_op(
            tab_id,
            "subir rama",
            (remote.clone(), branch, set_upstream),
            |e, r, (n, b, u)| Box::pin(async move { e.push(&r, &n, Some(&b), u).await }),
        )
        .await?;
        Ok(PushBranchResult { remote })
    }

    /// Rebasa la rama actual sobre `branch` (`git rebase`). Reescribe historia, así que **pide
    /// confirmación nativa**. Con conflictos el rebase queda en curso (`state`) y se resuelve con
    /// el estudio de conflictos, igual que un merge.
    pub async fn rebase_onto(&self, tab_id: &str, branch: String) -> ApiResult<RebaseOutcome> {
        let branch = branch.trim().to_owned();
        if validate_ref(&branch).is_err() {
            return Err(invalid("La rama sobre la que rebasar no es válida."));
        }
        let (s, meta) = self.live(tab_id)?;
        let snap = Self::fresh_snapshot(&s).await;
        if snap.integration != IntegrationState::None {
            return Err(invalid(
                "Hay un merge, rebase o cherry-pick en curso. Termínalo o abórtalo antes.",
            ));
        }
        let Some(current) = snap.status.branch_name.clone() else {
            return Err(invalid(format!(
                "HEAD no está en una rama: no hay nada que rebasar sobre «{branch}»."
            )));
        };
        if current == branch {
            return Err(invalid(
                "La rama actual no se puede rebasar sobre sí misma.",
            ));
        }
        let exists = snap
            .branches
            .iter()
            .chain(snap.remote_branches.iter())
            .any(|b| b.name == branch);
        if !exists {
            return Err(invalid("Esa rama ya no existe."));
        }
        let ok = self
            .inner
            .prompter
            .confirm(
                "Rebasar una rama",
                &format!(
                    "En {}: los commits de «{current}» que no están en «{branch}» se reescribirán encima de ella y cambiarán de hash. El reflog conserva los anteriores por si hay que deshacer.",
                    Self::context(&s, &meta)
                ),
                &format!(
                    "Rama actual: {current}\nEncima de: {branch}\nRepositorio: {}",
                    meta.path
                ),
            )
            .await;
        if !ok {
            return Ok(RebaseOutcome {
                done: false,
                result: MergeResult::UpToDate,
                state: None,
            });
        }
        let confirmation = Confirmation::granted("rebasar la rama actual");
        let result = s
            .mutate("rebase", move |e, r| {
                let (b, c) = (branch.clone(), confirmation.clone());
                Box::pin(async move { e.rebase_onto(&r, &b, &c).await })
            })
            .await?;
        let state = if matches!(result, MergeResult::Conflicts { .. }) {
            Some(self.merge_state(tab_id).await?)
        } else {
            None
        };
        Ok(RebaseOutcome {
            done: true,
            result,
            state,
        })
    }

    /// Borra una rama en el **remoto** (`git push <remoto> --delete <rama>`) tras confirmación
    /// nativa. No toca la copia local. `done: false` si se cancela.
    pub async fn delete_remote_branch(
        &self,
        tab_id: &str,
        remote: String,
        branch: String,
    ) -> ApiResult<Done> {
        let remote = remote.trim().to_owned();
        let branch = branch.trim().to_owned();
        if validate_ref(&remote).is_err() || validate_ref(&branch).is_err() {
            return Err(invalid("El remoto o la rama no son válidos."));
        }
        let (s, meta) = self.live(tab_id)?;
        let snap = Self::fresh_snapshot(&s).await;
        if !snap.remotes.iter().any(|r| r.name == remote) {
            return Err(invalid("Ese remoto ya no existe."));
        }
        let full = format!("{remote}/{branch}");
        if !snap.remote_branches.iter().any(|b| b.name == full) {
            return Err(invalid("Esa rama ya no está en el remoto."));
        }
        let tracking = snap
            .branches
            .iter()
            .filter(|b| b.upstream.as_deref() == Some(full.as_str()))
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>();
        let mut message = format!(
            "En {}: se borrará la rama «{branch}» del remoto «{remote}». Quien la tenga descargada la conservará, pero dejará de existir en el servidor.",
            Self::context(&s, &meta)
        );
        if !tracking.is_empty() {
            message.push_str(&format!(
                " {} rama(s) local(es) dejarán de tener origen.",
                tracking.len()
            ));
        }
        let detail = match tracking.is_empty() {
            true => format!("Rama: {full}\nRepositorio: {}", meta.path),
            false => format!(
                "Rama: {full}\nLocales que la siguen: {}\nRepositorio: {}",
                tracking.join(", "),
                meta.path
            ),
        };
        let ok = self
            .inner
            .prompter
            .confirm("Borrar rama del remoto", &message, &detail)
            .await;
        if !ok {
            return Ok(Done { done: false });
        }
        let confirmation = Confirmation::granted("borrar la rama del remoto");
        s.mutate("borrar rama del remoto", move |e, r| {
            let (rm, b, c) = (remote.clone(), branch.clone(), confirmation.clone());
            Box::pin(async move { e.delete_remote_branch(&r, &rm, &b, &c).await })
        })
        .await?;
        Ok(Done { done: true })
    }

    /// `git diff base...head`: lo que `head` aporta desde que se separó de `base`. Es el diff que
    /// se ve antes de fusionar («qué trae esta rama»). Sin historia común no hay diff
    /// (`INVALID_REFERENCE`). Solo lectura.
    pub async fn branch_diff_against(
        &self,
        tab_id: &str,
        base: String,
        head: String,
    ) -> ApiResult<Vec<FileDiff>> {
        let base = base.trim().to_owned();
        let head = head.trim().to_owned();
        if validate_ref(&base).is_err() || validate_ref(&head).is_err() {
            return Err(invalid("Alguna de las ramas no es un nombre válido."));
        }
        self.read_op(tab_id, "diff entre ramas", move |e, r| {
            Box::pin(async move { e.diff_against(&r, &base, &head).await })
        })
        .await
    }
}
