import Foundation

public enum AIError: Error, Sendable, Equatable, LocalizedError {
    case contextTooLarge(bytes: Int, maxBytes: Int)
    case providerUnavailable(AIProviderID, hint: String)
    case notAuthenticated(AIProviderID, hint: String)
    case providerFailed(AIProviderID, exitCode: Int32, detail: String)
    case emptyResponse(AIProviderID)
    case noProviderAvailable(hints: [String])
    case timedOut(AIProviderID)
    case cancelled

    /// En modo Auto se puede probar el otro CLI. Un diff enorme no se arregla cambiando de proveedor.
    var allowsFallback: Bool {
        switch self {
        case .notAuthenticated, .providerUnavailable, .providerFailed, .timedOut, .emptyResponse:
            return true
        case .contextTooLarge, .noProviderAvailable, .cancelled:
            return false
        }
    }

    public var errorDescription: String? {
        switch self {
        case .contextTooLarge(let bytes, let maxBytes):
            return "El diff ocupa \(bytes) bytes y el límite es \(maxBytes). Acorta el cambio o sube el límite en Ajustes. No se ha enviado nada."
        case .providerUnavailable(let provider, let hint):
            return "\(provider.displayName) no está listo. \(hint)"
        case .notAuthenticated(let provider, let hint):
            return "\(provider.displayName) no tiene sesión. \(hint)"
        case .providerFailed(let provider, let exitCode, let detail):
            let trimmed = detail.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmed.isEmpty {
                return "\(provider.displayName) terminó con código \(exitCode)."
            }
            return "\(provider.displayName) terminó con código \(exitCode).\n\(GitErrorTruncation.truncate(trimmed))"
        case .emptyResponse(let provider):
            return "\(provider.displayName) no devolvió texto."
        case .noProviderAvailable(let hints):
            let extra = hints.filter { !$0.isEmpty }.joined(separator: " ")
            return "No hay un proveedor de IA disponible. \(extra)"
        case .timedOut(let provider):
            return "\(provider.displayName) tardó demasiado y se canceló la espera."
        case .cancelled:
            return "Petición a la IA cancelada."
        }
    }
}

enum GitErrorTruncation {
    static func truncate(_ text: String, limit: Int = 2_000) -> String {
        guard text.count > limit else { return text }
        return String(text.prefix(limit)) + "…"
    }
}

enum AIAuth {
    static func isAuthenticationFailure(_ text: String) -> Bool {
        let lowered = text.lowercased()
        let needles = [
            "not logged in",
            "not authenticated",
            "please login",
            "please log in",
            "copilot login",
            "codex login",
            "authentication required",
            "unauthorized",
            "you need to log in",
            "sign in to",
            "login required",
            "chatgpt.com/auth",
        ]
        return needles.contains { lowered.contains($0) }
    }
}
