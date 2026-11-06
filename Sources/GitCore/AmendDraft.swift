import Foundation

/// Reglas del cuadro de mensaje cuando se activa o desactiva el amend.
///
/// Viven en el núcleo (y no en `AppModel`) para poder testearlas sin la UI.
public enum AmendDraft {
    /// Mensaje completo del commit para editarlo: asunto y, si hay, cuerpo.
    public static func message(for commit: Commit) -> String {
        commit.body.isEmpty ? commit.subject : commit.subject + "\n\n" + commit.body
    }

    /// Texto que queda en el cuadro al desactivar el amend.
    ///
    /// Si el cuadro sigue con el mensaje que Ariadna puso al activarlo, se borra:
    /// si no, el siguiente commit normal saldría con el mensaje del anterior.
    /// Si el usuario lo editó, se respeta.
    public static func messageAfterDisabling(current: String, prefilled: String?) -> String {
        guard let prefilled, current == prefilled else { return current }
        return ""
    }

    /// Aviso para la barra de commit si el commit a corregir es un merge. Nil si no lo es.
    public static func mergeWarning(for commit: Commit) -> String? {
        guard commit.parents.count > 1 else { return nil }
        return "El último commit es un merge (\(commit.parents.count) padres). Corregirlo lo rehace con los mismos padres: revisa el mensaje y lo preparado."
    }
}

/// Estado del modo amend en la barra de commit.
///
/// Activar el amend lee el último commit de forma asíncrona. Si mientras tanto
/// el usuario desmarca la casilla (o se abre otro repositorio), esa lectura
/// llega tarde y no debe rellenar nada: un commit normal saldría con el mensaje
/// del anterior. Por eso la activación se hace en dos pasos, `beginEnabling()`
/// y `finishEnabling(...)`, y el segundo descarta resultados obsoletos.
///
/// Vive en el núcleo (y no en `AppModel`) para poder testear esa carrera sin la UI.
public struct AmendState: Sendable, Equatable {
    /// Turno de una activación en curso. Solo sirve para `finishEnabling`.
    public struct Ticket: Sendable, Equatable {
        fileprivate let epoch: Int
    }

    /// Qué hacer con la lectura del último commit.
    public enum Outcome: Sendable, Equatable {
        /// Llegó tarde (amend desactivado o repositorio cambiado): no se toca nada.
        case stale
        /// No hay commits que corregir. El amend queda desactivado.
        case noCommit
        /// Aplicado. `message` es el texto que debe quedar en el cuadro.
        case applied(message: String)
    }

    public private(set) var isEnabled = false
    /// Aviso si el commit que se va a corregir es un merge. Nil en otro caso.
    public private(set) var warning: String?
    /// Mensaje que Ariadna puso en el cuadro al activar el amend.
    public private(set) var prefilledMessage: String?
    /// Cambia con cada activación, desactivación y `reset()`: un ticket solo vale
    /// para la última activación, aunque la casilla se marque, desmarque y vuelva
    /// a marcar antes de que llegue la primera lectura.
    private var epoch = 0

    public init() {}

    /// Marca el amend como activo y devuelve el turno para `finishEnabling`.
    public mutating func beginEnabling() -> Ticket {
        isEnabled = true
        epoch &+= 1
        return Ticket(epoch: epoch)
    }

    /// Aplica el último commit leído tras `beginEnabling()`.
    ///
    /// Se vuelve a comprobar el estado después del `await`: si el amend ya no
    /// está activo, o el repositorio cambió, el resultado se descarta.
    public mutating func finishEnabling(_ ticket: Ticket, lastCommit: Commit?, currentMessage: String) -> Outcome {
        guard isEnabled, ticket.epoch == epoch else { return .stale }
        guard let lastCommit else {
            reset()
            return .noCommit
        }
        warning = AmendDraft.mergeWarning(for: lastCommit)
        guard currentMessage.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return .applied(message: currentMessage)
        }
        let message = AmendDraft.message(for: lastCommit)
        prefilledMessage = message
        return .applied(message: message)
    }

    /// Desactiva el amend y devuelve el texto que debe quedar en el cuadro.
    public mutating func disable(currentMessage: String) -> String {
        let message = AmendDraft.messageAfterDisabling(current: currentMessage, prefilled: prefilledMessage)
        isEnabled = false
        warning = nil
        prefilledMessage = nil
        epoch &+= 1
        return message
    }

    /// Vuelve al estado inicial (repositorio nuevo, amend hecho) e invalida lecturas en curso.
    public mutating func reset() {
        isEnabled = false
        warning = nil
        prefilledMessage = nil
        epoch &+= 1
    }
}
