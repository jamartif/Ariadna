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
