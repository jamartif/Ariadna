//! `GitCliEngine`: implementación de [`GitServing`](crate::GitServing) que lanza el `git`
//! del sistema a través de [`GitRunner`](crate::GitRunner).
//!
//! Los métodos viven como **métodos inherentes** repartidos por archivos:
//! * `core_ops`: abrir repo, status, stage/unstage, commit, amend, log, diff.
//! * `branch_ops`: ramas, remotos, merge, integración, conflictos.
//!
//! `impl GitServing for GitCliEngine` (en `serving_impl.rs`) solo reenvía a esos métodos, que
//! deben tener **exactamente** la misma firma que el trait.

use crate::runner::GitRunner;

mod branch_ops;
mod core_ops;
mod serving_impl;

/// Motor git por CLI.
#[derive(Debug, Clone)]
pub struct GitCliEngine {
    pub(crate) runner: GitRunner,
}

impl GitCliEngine {
    pub fn new(runner: GitRunner) -> Self {
        Self { runner }
    }

    pub fn runner(&self) -> &GitRunner {
        &self.runner
    }
}
