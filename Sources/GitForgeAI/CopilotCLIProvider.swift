import Foundation
import GitForgeCore

/// `copilot -p … -s --no-ask-user`, con la sesión que el usuario ya tenga
/// (`copilot login`). No usa API keys.
public struct CopilotCLIProvider: AIProviding, Sendable {
    public let id: AIProviderID = .copilot
    public let displayName = "GitHub Copilot CLI"

    private let runner: any ProcessRunning
    private let binary: String
    private let policy: AIContextPolicy
    private let workingDirectory: URL?
    private let timeout: TimeInterval
    private let now: @Sendable () -> Date
    private let makeID: @Sendable () -> UUID

    public init(
        runner: any ProcessRunning,
        binary: String = "copilot",
        policy: AIContextPolicy = AIContextPolicy(),
        workingDirectory: URL? = nil,
        timeout: TimeInterval = 180,
        now: @escaping @Sendable () -> Date = Date.init,
        makeID: @escaping @Sendable () -> UUID = UUID.init
    ) {
        self.runner = runner
        self.binary = binary
        self.policy = policy
        self.workingDirectory = workingDirectory
        self.timeout = timeout
        self.now = now
        self.makeID = makeID
    }

    public func availability() async -> AIAvailability {
        await CLIAvailability.probe(
            runner: runner,
            provider: .copilot,
            binary: binary,
            directory: directory,
            missingHint: "No encuentro `\(binary)` en el PATH. Instala GitHub Copilot CLI y ejecuta `copilot login`.",
            timeout: min(timeout, 20)
        )
    }

    public func suggestCommitMessage(diff: String, template: String?) async throws -> AISuggestion {
        try policy.validate(payload: diff)
        let prompt = PromptBuilder.commitMessage(diff: diff, template: template)
        let text = try await complete(prompt: prompt)
        let message = AIText.commitMessage(from: text)
        return make(
            kind: .commitMessage,
            prompt: prompt,
            subject: message.subject,
            body: message.body,
            rationale: nil,
            proposedContent: nil,
            note: nil
        )
    }

    public func summarizeDiff(_ diff: String, style: SummaryStyle) async throws -> AISuggestion {
        try policy.validate(payload: diff)
        let prompt = PromptBuilder.summarize(diff: diff, style: style)
        let text = try await complete(prompt: prompt)
        return make(
            kind: .summarizeDiff,
            prompt: prompt,
            subject: nil,
            body: AIText.stripFences(text),
            rationale: nil,
            proposedContent: nil,
            note: nil
        )
    }

    public func explainCommit(sha: String, patch: String, subject: String) async throws -> AISuggestion {
        try policy.validate(payload: patch)
        let prompt = PromptBuilder.explain(sha: sha, subject: subject, patch: patch)
        let text = try await complete(prompt: prompt)
        return make(
            kind: .explainCommit,
            prompt: prompt,
            subject: subject,
            body: AIText.stripFences(text),
            rationale: nil,
            proposedContent: nil,
            note: nil
        )
    }

    public func proposeConflictResolution(_ conflict: ConflictFile) async throws -> AISuggestion {
        let payload = PromptBuilder.conflictPayload(conflict)
        try policy.validate(payload: payload)
        let prompt = PromptBuilder.resolveConflict(conflict)
        let text = try await complete(prompt: prompt)
        let proposal = AIText.conflictProposal(from: text)
        return make(
            kind: .conflictResolution,
            prompt: prompt,
            subject: conflict.path,
            body: proposal.rationale ?? "Propuesta lista para revisar. No se ha escrito en el repositorio.",
            rationale: proposal.rationale,
            proposedContent: proposal.content,
            note: nil
        )
    }

    private var directory: URL {
        workingDirectory ?? FileManager.default.temporaryDirectory
    }

    private func complete(prompt: String) async throws -> String {
        let output = try await CLIInvocation.run(
            runner: runner,
            provider: .copilot,
            binary: binary,
            arguments: CopilotCommand.arguments(prompt: prompt),
            directory: directory,
            timeout: timeout,
            loginHint: "Ejecuta `copilot login` en la terminal y vuelve a intentarlo."
        )
        let text = output.stdoutString.trimmingCharacters(in: .whitespacesAndNewlines)
        if text.isEmpty { throw AIError.emptyResponse(.copilot) }
        return text
    }

    private func make(
        kind: AISuggestionKind,
        prompt: String,
        subject: String?,
        body: String,
        rationale: String?,
        proposedContent: String?,
        note: String?
    ) -> AISuggestion {
        AISuggestion(
            id: makeID(),
            kind: kind,
            provider: .copilot,
            commandSummary: CopilotCommand.summary(promptBytes: prompt.utf8.count),
            subject: subject,
            body: body,
            rationale: rationale,
            proposedContent: proposedContent,
            createdAt: now(),
            status: .pending,
            transparencyNote: note
        )
    }
}
