import Foundation
import XCTest
@testable import GitForgeAI
import GitForgeCore

final class AITests: XCTestCase {
    func testCopilotCommitMessageDoesNotInvokeGitOrWrite() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("gitforge-ai-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }

        let runner = ScriptedRunner { call in
            if call.arguments == ["--version"] {
                return ProcessOutput(executable: call.executable, arguments: call.arguments, exitCode: 0, stdout: Data("copilot 1\n".utf8), stderr: Data())
            }
            return ProcessOutput(
                executable: call.executable,
                arguments: call.arguments,
                exitCode: 0,
                stdout: Data("feat: parsear status\n\nUsa porcelain v2.\n".utf8),
                stderr: Data()
            )
        }
        let provider = CopilotCLIProvider(
            runner: runner,
            policy: AIContextPolicy(maxBytes: 10_000),
            workingDirectory: directory
        )
        let suggestion = try await provider.suggestCommitMessage(diff: "diff --git a/a b/a\n+hola\n", template: nil)
        XCTAssertEqual(suggestion.status, .pending)
        XCTAssertEqual(suggestion.provider, .copilot)
        XCTAssertEqual(suggestion.subject, "feat: parsear status")
        XCTAssertEqual(suggestion.body, "Usa porcelain v2.")
        XCTAssertNil(suggestion.proposedContent)
        XCTAssertTrue(suggestion.commandSummary.contains("copilot -p"))
        XCTAssertFalse(suggestion.commandSummary.contains("+hola"))

        let calls = runner.calls()
        XCTAssertEqual(calls.count, 1)
        let call = try XCTUnwrap(calls.first)
        XCTAssertEqual(call.executable, "copilot")
        XCTAssertEqual(call.cwd, directory.path)
        XCTAssertTrue(call.arguments.contains("-p"))
        XCTAssertTrue(call.arguments.contains("-s"))
        XCTAssertTrue(call.arguments.contains("--no-ask-user"))
        XCTAssertTrue(call.arguments.contains("--deny-tool"))
        XCTAssertTrue(call.arguments.contains("shell"))
        XCTAssertTrue(call.arguments.contains("write"))
        XCTAssertFalse(call.arguments.contains("git"))
        XCTAssertFalse(call.arguments.contains { $0.contains("allow-tool") })
        let prompt = call.arguments[call.arguments.firstIndex(of: "-p")! + 1]
        XCTAssertTrue(prompt.contains("Do not run tools"))
        XCTAssertTrue(prompt.contains("+hola"))
    }

    func testContextLimitBlocksTheProcess() async throws {
        let runner = ScriptedRunner { call in
            ProcessOutput(executable: call.executable, arguments: call.arguments, exitCode: 0, stdout: Data("x".utf8), stderr: Data())
        }
        let provider = CopilotCLIProvider(runner: runner, policy: AIContextPolicy(maxBytes: 8))
        do {
            _ = try await provider.suggestCommitMessage(diff: "esto-es-demasiado-largo", template: nil)
            XCTFail("tenía que rechazar el diff")
        } catch let error as AIError {
            guard case .contextTooLarge = error else { throw error }
        }
        XCTAssertTrue(runner.calls().isEmpty)
    }

    func testCodexParsesJSONLAndProposesConflictWithoutApplying() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("gitforge-codex-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let json = """
        {"type":"thread.started","thread_id":"abc"}
        {"type":"turn.started"}
        {"type":"item.completed","item":{"id":"item_3","type":"agent_message","text":"---FILE---\\nresuelto\\n---RATIONALE---\\nMe quedo con ambos títulos."}}
        {"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":4}}
        """
        let runner = ScriptedRunner { call in
            ProcessOutput(executable: call.executable, arguments: call.arguments, exitCode: 0, stdout: Data(json.utf8), stderr: Data())
        }
        let provider = CodexExecProvider(runner: runner, workingDirectory: directory)
        let conflict = ConflictFile(
            path: "file.txt",
            kind: .bothModified,
            ours: ConflictSide(oid: "a", content: "ours\n", isBinary: false),
            base: ConflictSide(oid: "b", content: "base\n", isBinary: false),
            theirs: ConflictSide(oid: "c", content: "theirs\n", isBinary: false),
            workingTree: nil,
            hunks: []
        )
        let suggestion = try await provider.proposeConflictResolution(conflict)
        XCTAssertEqual(suggestion.status, .pending)
        XCTAssertEqual(suggestion.provider, .codex)
        XCTAssertEqual(suggestion.proposedContent, "resuelto\n")
        XCTAssertEqual(suggestion.rationale, "Me quedo con ambos títulos.")
        XCTAssertEqual(suggestion.transparencyNote, "Codex: 10 tokens de entrada, 4 de salida.")
        let call = try XCTUnwrap(runner.calls().first)
        XCTAssertEqual(Array(call.arguments.prefix(5)), ["exec", "--json", "--sandbox", "read-only", "--skip-git-repo-check"])
        XCTAssertEqual(call.cwd, directory.path)
        XCTAssertFalse(call.arguments.contains("git"))
    }

    func testCodexLegacyAssistantMessageAndFailure() throws {
        let legacy = """
        {"type":"item.completed","item":{"id":"item_3","item_type":"assistant_message","text":"hola"}}
        {"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}
        """
        let parsed = try CodexOutputParser.parse(legacy)
        XCTAssertEqual(parsed.message, "hola")

        let failed = """
        {"type":"turn.failed","error":{"message":"quota"}}
        """
        XCTAssertThrowsError(try CodexOutputParser.parse(failed))
    }

    func testOrchestratorAutoFallsBackAndExplicitChoiceDoesNot() async throws {
        let runner = ScriptedRunner { call in
            if call.executable == "copilot" {
                throw ProcessError.executableNotFound("copilot")
            }
            if call.arguments == ["--version"] {
                return ProcessOutput(executable: call.executable, arguments: call.arguments, exitCode: 0, stdout: Data("codex 1\n".utf8), stderr: Data())
            }
            let json = """
            {"type":"item.completed","item":{"type":"agent_message","text":"chore: limpiar"}}
            {"type":"turn.completed"}
            """
            return ProcessOutput(executable: call.executable, arguments: call.arguments, exitCode: 0, stdout: Data(json.utf8), stderr: Data())
        }
        let copilot = CopilotCLIProvider(runner: runner, binary: "copilot")
        let codex = CodexExecProvider(runner: runner, binary: "codex")
        let auto = AIOrchestrator(choice: .auto, copilot: copilot, codex: codex)
        let suggestion = try await auto.suggestCommitMessage(diff: "diff\n")
        XCTAssertEqual(suggestion.provider, .codex)
        XCTAssertEqual(suggestion.subject, "chore: limpiar")
        XCTAssertEqual(suggestion.status, .pending)
        let accepted = await auto.accept(suggestion)
        XCTAssertEqual(accepted.status, .accepted)
        let rejected = await auto.reject(suggestion)
        XCTAssertEqual(rejected.status, .rejected)
        XCTAssertFalse(runner.calls().contains { $0.executable == "copilot" && $0.arguments.contains("-p") })

        runner.reset()
        let explicit = AIOrchestrator(choice: .copilot, copilot: copilot, codex: codex)
        do {
            _ = try await explicit.suggestCommitMessage(diff: "diff\n")
            XCTFail("Copilot no está instalado")
        } catch let error as AIError {
            guard case .providerUnavailable = error else { throw error }
        }
        XCTAssertFalse(runner.calls().contains { $0.executable == "codex" })
    }

    func testAuthenticationErrorIsActionable() async throws {
        let runner = ScriptedRunner { call in
            ProcessOutput(
                executable: call.executable,
                arguments: call.arguments,
                exitCode: 1,
                stdout: Data(),
                stderr: Data("Please run `codex login` to continue\n".utf8)
            )
        }
        let provider = CodexExecProvider(runner: runner)
        do {
            _ = try await provider.summarizeDiff("diff", style: .bullets)
            XCTFail("tenía que pedir login")
        } catch let error as AIError {
            guard case .notAuthenticated(let id, let hint) = error else { throw error }
            XCTAssertEqual(id, .codex)
            XCTAssertTrue(hint.contains("codex login"))
        }
    }

    func testSettingsRoundTrip() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("gitforge-settings-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: url) }
        let store = AISettingsStore(fileURL: url)
        var settings = AISettings.default
        settings.provider = .codex
        settings.maxContextBytes = 2048
        try store.save(settings)
        let loaded = try store.load()
        XCTAssertEqual(loaded, settings)
    }

    func testCommitMessageFencesAreStripped() {
        let parsed = AIText.commitMessage(from: "```text\nfix: puerta\n\nEvita el write.\n```")
        XCTAssertEqual(parsed.subject, "fix: puerta")
        XCTAssertEqual(parsed.body, "Evita el write.")
    }
}

final class ScriptedRunner: ProcessRunning, @unchecked Sendable {
    struct Call: Equatable {
        var executable: String
        var arguments: [String]
        var cwd: String?
    }

    private let lock = NSLock()
    private var recorded: [Call] = []
    private let handler: @Sendable (Call) throws -> ProcessOutput

    init(handler: @escaping @Sendable (Call) throws -> ProcessOutput) {
        self.handler = handler
    }

    func calls() -> [Call] {
        lock.lock()
        defer { lock.unlock() }
        return recorded
    }

    func reset() {
        lock.lock()
        recorded = []
        lock.unlock()
    }

    func run(_ request: ProcessRequest) async throws -> ProcessOutput {
        let call = Call(
            executable: request.executable,
            arguments: request.arguments,
            cwd: request.currentDirectory?.path
        )
        remember(call)
        return try handler(call)
    }

    /// NSLock no se puede usar dentro de una función `async` en Swift 6.
    private func remember(_ call: Call) {
        lock.lock()
        recorded.append(call)
        lock.unlock()
    }
}
