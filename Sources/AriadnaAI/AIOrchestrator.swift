import Foundation
import GitCore

/// Elige Copilot o Codex y devuelve sugerencias en estado `pending`.
/// `accept` y `reject` solo cambian ese estado en memoria.
public actor AIOrchestrator {
    private var choice: AIProviderChoice
    private let copilot: any AIProviding
    private let codex: any AIProviding

    public init(choice: AIProviderChoice, copilot: any AIProviding, codex: any AIProviding) {
        self.choice = choice
        self.copilot = copilot
        self.codex = codex
    }

    public func setChoice(_ choice: AIProviderChoice) {
        self.choice = choice
    }

    public func availabilities() async -> [AIAvailability] {
        await [copilot.availability(), codex.availability()]
    }

    public func suggestCommitMessage(diff: String, template: String? = nil) async throws -> AISuggestion {
        try await perform { provider in
            try await provider.suggestCommitMessage(diff: diff, template: template)
        }
    }

    public func summarizeDiff(_ diff: String, style: SummaryStyle) async throws -> AISuggestion {
        try await perform { provider in
            try await provider.summarizeDiff(diff, style: style)
        }
    }

    public func explainCommit(sha: String, patch: String, subject: String) async throws -> AISuggestion {
        try await perform { provider in
            try await provider.explainCommit(sha: sha, patch: patch, subject: subject)
        }
    }

    public func proposeConflictResolution(_ conflict: ConflictFile) async throws -> AISuggestion {
        try await perform { provider in
            try await provider.proposeConflictResolution(conflict)
        }
    }

    /// El usuario pulsó aceptar. No llama a git.
    public func accept(_ suggestion: AISuggestion) -> AISuggestion {
        suggestion.withStatus(.accepted)
    }

    /// El usuario descartó la propuesta. No llama a git.
    public func reject(_ suggestion: AISuggestion) -> AISuggestion {
        suggestion.withStatus(.rejected)
    }

    private func perform(
        _ body: (any AIProviding) async throws -> AISuggestion
    ) async throws -> AISuggestion {
        let providers = try await selectedProviders()
        var lastError: Error?
        for provider in providers {
            do {
                let suggestion = try await body(provider)
                if suggestion.status == .pending {
                    return suggestion
                }
                return suggestion.withStatus(.pending)
            } catch let error as AIError {
                lastError = error
                if choice == .auto && error.allowsFallback && providers.count > 1 {
                    continue
                }
                throw error
            } catch {
                throw error
            }
        }
        if let lastError { throw lastError }
        throw AIError.noProviderAvailable(hints: [])
    }

    private func selectedProviders() async throws -> [any AIProviding] {
        switch choice {
        case .copilot:
            try await ensureAvailable(copilot)
            return [copilot]
        case .codex:
            try await ensureAvailable(codex)
            return [codex]
        case .auto:
            let copilotAvailability = await copilot.availability()
            let codexAvailability = await codex.availability()
            var ready: [any AIProviding] = []
            if copilotAvailability.isAvailable { ready.append(copilot) }
            if codexAvailability.isAvailable { ready.append(codex) }
            if ready.isEmpty {
                throw AIError.noProviderAvailable(hints: [copilotAvailability.hint, codexAvailability.hint])
            }
            return ready
        }
    }

    private func ensureAvailable(_ provider: any AIProviding) async throws {
        let availability = await provider.availability()
        guard availability.isAvailable else {
            throw AIError.providerUnavailable(provider.id, hint: availability.hint)
        }
    }
}
