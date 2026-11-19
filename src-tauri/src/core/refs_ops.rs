//! Referencias del repo que el riel pinta aparte del snapshot diario: refs de pull request
//! (`refs/pull/*`, `refs/merge-requests/*`) y worktrees (con su HEAD, que puede ser «detached»).
//!
//! Van por **su propia lectura explícita** a propósito: meterlas en el snapshot añadía dos lecturas
//! al refresco de cada sesión (y rompía las pruebas de sesión, que cuentan lecturas). Aquí se piden
//! cuando el riel las necesita, con `read_op`, sin tocar el refresco.

use ariadna_git::{Branch, Worktree};
use serde::Serialize;

use super::error::ApiResult;
use super::AppCore;

/// Lo que el riel necesita para sus dos grupos extra.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraRefs {
    /// Referencias de pull request que viven en el repo.
    pub pull_refs: Vec<Branch>,
    /// Todos los worktrees del repo; el principal va primero (`isMain`).
    pub worktrees: Vec<Worktree>,
}

impl AppCore {
    /// Refs de PR y worktrees del repo de una pestaña. Una sola lectura, dos consultas a git.
    pub async fn extra_refs(&self, tab_id: &str) -> ApiResult<ExtraRefs> {
        self.read_op(tab_id, "referencias", |e, r| {
            Box::pin(async move {
                let pull_refs = e.pull_refs(&r).await?;
                let worktrees = e.worktrees(&r).await?;
                Ok(ExtraRefs {
                    pull_refs,
                    worktrees,
                })
            })
        })
        .await
    }
}
