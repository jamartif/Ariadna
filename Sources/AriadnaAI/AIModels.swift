import Foundation
import GitCore

public enum AIProviderID: String, Sendable, Equatable, Codable, CaseIterable {
    case copilot
    case codex

    public var displayName: String {
        switch self {
        case .copilot: return "GitHub Copilot"
        case .codex: return "Codex"
        }
    }

    public var loginCommand: String {
        switch self {
        case .copilot: return "copilot login"
        case .codex: return "codex login"
        }
    }
}

public enum AIProviderChoice: String, Sendable, Equatable, Codable, CaseIterable {
    case copilot
    case codex
    /// Usa el primer CLI que responda a `--version`. Si esa llamada falla por
    /// login o por el proceso, prueba el otro. No cambia de proveedor en silencio
    /// cuando el usuario eligió uno concreto.
    case auto
}

public enum AISuggestionKind: String, Sendable, Equatable, Codable {
    case commitMessage
    case summarizeDiff
    case explainCommit
    case conflictResolution
}

public enum AISuggestionStatus: String, Sendable, Equatable, Codable {
    case pending
    case accepted
    case rejected
}

public enum SummaryStyle: String, Sendable, Equatable {
    case bullets
    case pullRequest
}

/// Propuesta de la IA. Nace en `pending`.
/// Cambiar el estado NO escribe en el repositorio: no hay un método `apply` aquí.
public struct AISuggestion: Sendable, Equatable, Identifiable {
    public let id: UUID
    public let kind: AISuggestionKind
    public let provider: AIProviderID
    public let commandSummary: String
    public let subject: String?
    public let body: String
    public let rationale: String?
    /// Texto de archivo propuesto (conflictos). Sigue en memoria hasta que el usuario escriba.
    public let proposedContent: String?
    public let createdAt: Date
    public let status: AISuggestionStatus
    public let transparencyNote: String?

    public init(
        id: UUID,
        kind: AISuggestionKind,
        provider: AIProviderID,
        commandSummary: String,
        subject: String?,
        body: String,
        rationale: String?,
        proposedContent: String?,
        createdAt: Date,
        status: AISuggestionStatus,
        transparencyNote: String?
    ) {
        self.id = id
        self.kind = kind
        self.provider = provider
        self.commandSummary = commandSummary
        self.subject = subject
        self.body = body
        self.rationale = rationale
        self.proposedContent = proposedContent
        self.createdAt = createdAt
        self.status = status
        self.transparencyNote = transparencyNote
    }

    public func withStatus(_ status: AISuggestionStatus) -> AISuggestion {
        AISuggestion(
            id: id,
            kind: kind,
            provider: provider,
            commandSummary: commandSummary,
            subject: subject,
            body: body,
            rationale: rationale,
            proposedContent: proposedContent,
            createdAt: createdAt,
            status: status,
            transparencyNote: transparencyNote
        )
    }

    public var commitMessageText: String {
        if let subject, !subject.isEmpty {
            if body.isEmpty || body == subject { return subject }
            return subject + "\n\n" + body
        }
        return body
    }
}

public struct AIAvailability: Sendable, Equatable {
    public enum State: Sendable, Equatable {
        case available(binary: String)
        case missing(binary: String, hint: String)
        case failed(binary: String, detail: String)
    }

    public let provider: AIProviderID
    public let state: State

    public init(provider: AIProviderID, state: State) {
        self.provider = provider
        self.state = state
    }

    public var isAvailable: Bool {
        if case .available = state { return true }
        return false
    }

    public var hint: String {
        switch state {
        case .available:
            return ""
        case .missing(_, let hint), .failed(_, let hint):
            return hint
        }
    }
}

public struct AIContextPolicy: Sendable, Equatable {
    public var maxBytes: Int

    public init(maxBytes: Int = 100_000) {
        self.maxBytes = max(1, maxBytes)
    }

    public func validate(payload: String) throws {
        let bytes = payload.utf8.count
        if bytes > maxBytes {
            throw AIError.contextTooLarge(bytes: bytes, maxBytes: maxBytes)
        }
    }
}

public struct AISettings: Sendable, Equatable, Codable {
    public var provider: AIProviderChoice
    public var maxContextBytes: Int
    public var gitBinary: String
    public var copilotBinary: String
    public var codexBinary: String
    public var pullUsesRebase: Bool

    public init(
        provider: AIProviderChoice,
        maxContextBytes: Int,
        gitBinary: String,
        copilotBinary: String,
        codexBinary: String,
        pullUsesRebase: Bool
    ) {
        self.provider = provider
        self.maxContextBytes = maxContextBytes
        self.gitBinary = gitBinary
        self.copilotBinary = copilotBinary
        self.codexBinary = codexBinary
        self.pullUsesRebase = pullUsesRebase
    }

    public static let `default` = AISettings(
        provider: .auto,
        maxContextBytes: 100_000,
        gitBinary: "git",
        copilotBinary: "copilot",
        codexBinary: "codex",
        pullUsesRebase: false
    )
}

/// Como una interfaz de C#. Cada proveedor sabe invocar su CLI.
/// Ningún método escribe en el working tree.
public protocol AIProviding: Sendable {
    var id: AIProviderID { get }
    var displayName: String { get }
    func availability() async -> AIAvailability
    func suggestCommitMessage(diff: String, template: String?) async throws -> AISuggestion
    func summarizeDiff(_ diff: String, style: SummaryStyle) async throws -> AISuggestion
    func explainCommit(sha: String, patch: String, subject: String) async throws -> AISuggestion
    func proposeConflictResolution(_ conflict: ConflictFile) async throws -> AISuggestion
}
