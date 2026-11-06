import Foundation

/// Errores del motor Git. El texto está en español para la UI;
/// el stderr original de `git` se conserva tal cual (suele venir en inglés).
public enum GitError: Error, Sendable, Equatable, LocalizedError {
    case notARepository(path: String)
    case gitNotFound(String)
    case commandFailed(command: String, exitCode: Int32, stderr: String)
    case nothingToCommit
    case emptyCommitMessage
    /// HEAD detached. El commit no se hace hasta que el usuario confirme.
    case detachedHeadCommit
    /// El amend no es seguro. El motivo es un valor, no un texto, para que los tests
    /// y la UI no dependan de la redacción.
    case cannotAmend(AmendRefusal)
    /// `git switch` se negó porque pisaría cambios locales.
    case localChangesBlockCheckout(detail: String)
    /// La local ya existe y no apunta al mismo commit que la rama remota, o sigue a otra.
    case divergedLocalBranch(local: String, remote: String)
    case invalidReference(String)
    case hunkNotFound(path: String)
    case noIntegrationInProgress
    case confirmationRequired(action: String)
    case pathEscapesRepository(path: String)
    case timeout(command: String)
    case cancelled
    case parse(String)

    public var errorDescription: String? {
        switch self {
        case .notARepository(let path):
            return "No hay un repositorio Git en \(path)."
        case .gitNotFound(let binary):
            return "No encuentro el ejecutable de git (\(binary)). Instálalo o indica la ruta en Ajustes."
        case .commandFailed(let command, let exitCode, let stderr):
            let detail = stderr.trimmingCharacters(in: .whitespacesAndNewlines)
            if detail.isEmpty {
                return "git terminó con código \(exitCode) al ejecutar \(command)."
            }
            return "git terminó con código \(exitCode).\n\(Self.truncate(detail))"
        case .nothingToCommit:
            return "No hay cambios preparados para el commit."
        case .emptyCommitMessage:
            return "El mensaje de commit está vacío."
        case .detachedHeadCommit:
            return "HEAD no está en una rama. El commit quedaría suelto. Confírmalo o crea una rama antes."
        case .cannotAmend(let reason):
            return "No corrijo el último commit. \(reason.explanation)"
        case .divergedLocalBranch(let local, let remote):
            return "La rama local «\(local)» no coincide con \(remote). No cambio de rama hasta que lo confirmes."
        case .localChangesBlockCheckout(let detail):
            let extra = detail.trimmingCharacters(in: .whitespacesAndNewlines)
            if extra.isEmpty {
                return "Hay cambios locales que impedirían el cambio de rama. Haz commit, descártalos o guárdalos antes."
            }
            return "Hay cambios locales que impedirían el cambio de rama. Haz commit, descártalos o guárdalos antes.\n\(extra)"
        case .invalidReference(let name):
            return "La referencia «\(name)» no es válida."
        case .hunkNotFound(let path):
            return "Ese hunk ya no está en \(path). Actualiza el diff e inténtalo de nuevo."
        case .noIntegrationInProgress:
            return "No hay un merge, rebase o cherry-pick en curso."
        case .confirmationRequired(let action):
            return "Hace falta confirmación explícita para \(action)."
        case .pathEscapesRepository(let path):
            return "La ruta \(path) sale del repositorio. No se ha escrito nada."
        case .timeout(let command):
            return "La operación tardó demasiado y se canceló: \(command)."
        case .cancelled:
            return "Operación cancelada."
        case .parse(let detail):
            return "No pude interpretar la salida de git: \(detail)"
        }
    }

    static func truncate(_ text: String, limit: Int = 4_000) -> String {
        guard text.count > limit else { return text }
        return String(text.prefix(limit)) + "…"
    }
}

/// Por qué no se puede corregir el último commit.
public enum AmendRefusal: Sendable, Equatable {
    /// El repositorio todavía no tiene commits.
    case noCommits
    /// HEAD no está en una rama.
    case detachedHead
    /// Hay un merge, rebase o cherry-pick a medias.
    case integrationInProgress
    /// El commit ya está en una rama remota (p. ej. `origin/main`).
    case alreadyPublished(remoteRef: String)

    /// Texto para la UI.
    public var explanation: String {
        switch self {
        case .noCommits:
            return "Todavía no hay ningún commit que corregir."
        case .detachedHead:
            return "HEAD no está en una rama. Crea una rama antes de corregir el commit."
        case .integrationInProgress:
            return "Hay un merge, rebase o cherry-pick en curso. Termínalo o abórtalo antes."
        case .alreadyPublished(let remoteRef):
            return "El último commit ya está en \(remoteRef). Corregirlo obligaría a un push forzado."
        }
    }
}

/// Puerta explícita para escrituras. No tiene valor por defecto a propósito:
/// quien llama tiene que decidir `confirmed: true` en el sitio de la llamada.
public enum UserConfirmation {
    public static func require(_ confirmed: Bool, action: String) throws {
        guard confirmed else {
            throw GitError.confirmationRequired(action: action)
        }
    }
}
