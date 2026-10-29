import Foundation
import GitForgeCore

/// `codex exec --json`, con la sesión de `codex login` (cuenta ChatGPT).
/// `--sandbox read-only` impide que el agente escriba aunque ignore el prompt.
public struct CodexExecProvider: AIProviding, Sendable {
    public let id: AIProviderID = .codex
    public let displayName = "Codex CLI"

    private let runner: any ProcessRunning
    private let binary: String
    private let policy: AIContextPolicy
    private let workingDirectory: URL?
    private let timeout: TimeInterval
    private let now: @Sendable () -> Date
    private let makeID: @Sendable () -> UUID

    public init(
        runner: any ProcessRunning,
        binary: String = "codex",
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
            provider: .codex,
            binary: binary,
            directory: directory,
            missingHint: "No encuentro `\(binary)` en el PATH. Instala Codex CLI y ejecuta `codex login` con tu cuenta de ChatGPT.",
            timeout: min(timeout, 20)
        )
    }

    public func suggestCommitMessage(diff: String, template: String?) async throws -> AISuggestion {
        try policy.validate(payload: diff)
        let prompt = PromptBuilder.commitMessage(diff: diff, template: template)
        let parsed = try await complete(prompt: prompt)
        let message = AIText.commitMessage(from: parsed.message)
        return make(
            kind: .commitMessage,
            prompt: prompt,
            subject: message.subject,
            body: message.body,
            rationale: nil,
            proposedContent: nil,
            note: parsed.transparencyNote
        )
    }

    public func summarizeDiff(_ diff: String, style: SummaryStyle) async throws -> AISuggestion {
        try policy.validate(payload: diff)
        let prompt = PromptBuilder.summarize(diff: diff, style: style)
        let parsed = try await complete(prompt: prompt)
        return make(
            kind: .summarizeDiff,
            prompt: prompt,
            subject: nil,
            body: AIText.stripFences(parsed.message),
            rationale: nil,
            proposedContent: nil,
            note: parsed.transparencyNote
        )
    }

    public func explainCommit(sha: String, patch: String, subject: String) async throws -> AISuggestion {
        try policy.validate(payload: patch)
        let prompt = PromptBuilder.explain(sha: sha, subject: subject, patch: patch)
        let parsed = try await complete(prompt: prompt)
        return make(
            kind: .explainCommit,
            prompt: prompt,
            subject: subject,
            body: AIText.stripFences(parsed.message),
            rationale: nil,
            proposedContent: nil,
            note: parsed.transparencyNote
        )
    }

    public func proposeConflictResolution(_ conflict: ConflictFile) async throws -> AISuggestion {
        let payload = PromptBuilder.conflictPayload(conflict)
        try policy.validate(payload: payload)
        let prompt = PromptBuilder.resolveConflict(conflict)
        let parsed = try await complete(prompt: prompt)
        let proposal = AIText.conflictProposal(from: parsed.message)
        return make(
            kind: .conflictResolution,
            prompt: prompt,
            subject: conflict.path,
            body: proposal.rationale ?? "Propuesta lista para revisar. No se ha escrito en el repositorio.",
            rationale: proposal.rationale,
            proposedContent: proposal.content,
            note: parsed.transparencyNote
        )
    }

    private var directory: URL {
        workingDirectory ?? FileManager.default.temporaryDirectory
    }

    private func complete(prompt: String) async throws -> CodexOutputParser.Parsed {
        let output = try await CLIInvocation.run(
            runner: runner,
            provider: .codex,
            binary: binary,
            arguments: CodexCommand.arguments(prompt: prompt),
            directory: directory,
            timeout: timeout,
            loginHint: "Ejecuta `codex login` en la terminal (cuenta ChatGPT) y vuelve a intentarlo."
        )
        return try CodexOutputParser.parse(output.stdoutString)
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
            provider: .codex,
            commandSummary: CodexCommand.summary(promptBytes: prompt.utf8.count),
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
