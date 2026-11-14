//! Errores tipados del cliente de pull requests (spec §4.4, PR-10).
//!
//! **Ningún mensaje contiene el token.** Los mensajes que incorporan texto del servidor pasan
//! antes por el [`Scrubber`](crate::auth::Scrubber) de la operación.

use thiserror::Error;

/// Resultado del cliente de proveedores.
pub type HostingResult<T> = Result<T, HostingError>;

/// Error de una operación contra el proveedor. `message` está en español y se puede mostrar.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HostingError {
    /// 401/403 sin límite de uso, o el gestor de credenciales no tiene token. Si el servidor
    /// dice qué permiso falta, va en `missing_permission`.
    #[error("{message}")]
    Auth {
        message: String,
        missing_permission: Option<String>,
    },
    /// 404 (en GitHub también es lo que ve un token sin acceso a un repo privado).
    #[error("{message}")]
    NotFound { message: String },
    /// Política o regla del proveedor incumplida (no se puede completar, rama protegida…).
    #[error("{message}")]
    Policy { message: String },
    /// 409 / estado obsoleto: la PR cambió, ya existe una igual, la cabeza no coincide.
    #[error("{message}")]
    Conflict { message: String },
    /// 429 o límite de uso agotado.
    #[error("{message}")]
    RateLimited {
        message: String,
        retry_after_secs: Option<u64>,
    },
    /// Sin conexión, DNS, TLS, tiempo agotado o 5xx.
    #[error("{message}")]
    Network { message: String },
    /// La petición no tiene sentido para este proveedor (voto no soportado, id inválido…).
    #[error("{message}")]
    Invalid { message: String },
    /// Respuesta mal formada, de tipo inesperado o por encima del tope de tamaño.
    #[error("{message}")]
    BadResponse { message: String },
    /// Fallo local (lanzar `git credential fill`).
    #[error("{message}")]
    Io { message: String },
}

impl HostingError {
    /// Código estable del contrato IPC (`specs/rust/ipc.md`).
    pub fn code(&self) -> &'static str {
        match self {
            HostingError::Auth { .. } => "AUTH",
            HostingError::NotFound { .. } => "NOT_FOUND",
            HostingError::Policy { .. } => "POLICY",
            HostingError::Conflict { .. } => "CONFLICT",
            HostingError::RateLimited { .. } => "RATE_LIMITED",
            HostingError::Network { .. } => "NETWORK",
            HostingError::Invalid { .. } => "INVALID_REFERENCE",
            HostingError::BadResponse { .. } => "PARSE",
            HostingError::Io { .. } => "IO",
        }
    }

    /// Dato útil para `ApiError.detail`: el permiso que falta o los segundos de espera.
    pub fn detail(&self) -> Option<String> {
        match self {
            HostingError::Auth {
                missing_permission, ..
            } => missing_permission.clone(),
            HostingError::RateLimited {
                retry_after_secs, ..
            } => retry_after_secs.map(|s| s.to_string()),
            _ => None,
        }
    }

    pub(crate) fn auth(message: impl Into<String>, permission: Option<String>) -> Self {
        HostingError::Auth {
            message: message.into(),
            missing_permission: permission,
        }
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        HostingError::Invalid {
            message: message.into(),
        }
    }

    pub(crate) fn bad(message: impl Into<String>) -> Self {
        HostingError::BadResponse {
            message: message.into(),
        }
    }

    pub(crate) fn network(message: impl Into<String>) -> Self {
        HostingError::Network {
            message: message.into(),
        }
    }

    pub(crate) fn policy(message: impl Into<String>) -> Self {
        HostingError::Policy {
            message: message.into(),
        }
    }
}
