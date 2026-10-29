import Foundation
import XCTest
@testable import GitCore

final class OperationQueueTests: XCTestCase {
    func testMutationCommandsAreClassified() {
        XCTAssertFalse(GitCLIEngine.isGitMutation(["status", "--porcelain=v2"]))
        XCTAssertFalse(GitCLIEngine.isGitMutation(["-c", "core.quotepath=false", "diff", "--cached"]))
        XCTAssertFalse(GitCLIEngine.isGitMutation(["log", "-n", "5"]))
        XCTAssertFalse(GitCLIEngine.isGitMutation(["remote", "-v"]))
        XCTAssertTrue(GitCLIEngine.isGitMutation(["add", "--", "a.txt"]))
        XCTAssertTrue(GitCLIEngine.isGitMutation(["commit", "-F", "-"]))
        XCTAssertTrue(GitCLIEngine.isGitMutation(["remote", "add", "origin", "/tmp/bare"]))
        XCTAssertTrue(GitCLIEngine.isGitMutation(["pull", "--rebase", "origin"]))
        XCTAssertTrue(GitCLIEngine.isGitMutation(["push", "--set-upstream", "origin", "main"]))
    }

    func testMutationsAreSerialAndReadsOverlap() async throws {
        let runner = OverlapRunner()
        let engine = GitCLIEngine(runner: runner, environment: ProcessEnvironment.sanitized(["PATH": "/usr/bin:/bin", "HOME": "/tmp"]))
        let repo = Repository(path: "/tmp/ariadna-queue", gitDir: "/tmp/ariadna-queue/.git")

        async let readA: GitStatus = engine.status(in: repo, includeIgnored: false)
        async let readB: GitStatus = engine.status(in: repo, includeIgnored: false)
        _ = try await (readA, readB)
        let reads = runner.snapshot(kind: .read)
        XCTAssertEqual(reads.maxActive, 2)

        async let writeA: Void = engine.stage(paths: ["a.txt"], in: repo)
        async let writeB: Void = engine.stage(paths: ["b.txt"], in: repo)
        _ = try await (writeA, writeB)
        let writes = runner.snapshot(kind: .write)
        XCTAssertEqual(writes.maxActive, 1)
        XCTAssertEqual(writes.finished, 2)
    }

    func testCancelledRefreshDoesNotPublishStatus() async throws {
        let runner = OverlapRunner()
        runner.hangStatus = true
        let engine = GitCLIEngine(runner: runner, environment: ProcessEnvironment.sanitized(["PATH": "/usr/bin:/bin", "HOME": "/tmp"]))
        let repo = Repository(path: "/tmp/ariadna-refresh", gitDir: "/tmp/ariadna-refresh/.git")
        let session = RepositorySession(repository: repo, engine: engine)
        let task = Task { try await session.refresh() }
        try await Task.sleep(nanoseconds: 80_000_000)
        task.cancel()
        do {
            try await task.value
            XCTFail("el refresco tenía que cancelarse")
        } catch {
            let text = String(describing: error)
            XCTAssertTrue(
                text.contains("cancel") || text.contains("Cancel") || text.contains("Operación cancelada"),
                "error inesperado: \(text)"
            )
        }
        let status = await session.status
        XCTAssertNil(status.branchName)
        XCTAssertTrue(status.entries.isEmpty)
        XCTAssertNil(status.headOID)
    }

    func testLoginShellPathIsCaptured() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("ariadna-shell-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let shell = root.appendingPathComponent("fake-login-shell")
        let script = "#!/bin/sh\nprintf '%s' '/opt/ariadna-bin:/usr/bin'\n"
        try Data(script.utf8).write(to: shell)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: shell.path)
        let path = ProcessEnvironment.pathFromLoginShell(shell: shell.path)
        XCTAssertEqual(path, "/opt/ariadna-bin:/usr/bin")
        let environment = ProcessEnvironment.sanitizedIncludingLoginPATH([
            "PATH": "/usr/bin",
            "HOME": "/tmp",
            "SHELL": shell.path,
        ])
        XCTAssertEqual(environment["PATH"], "/opt/ariadna-bin:/usr/bin")
    }

    func testBundleIdentifier() {
        XCTAssertEqual(AriadnaIdentity.bundleIdentifier, "es.javiermartinez.ariadna")
        XCTAssertEqual(AriadnaIdentity.productName, "Ariadna")
        XCTAssertEqual(AriadnaIdentity.minimumMacOS, "15.0")
    }
}

private final class OverlapRunner: ProcessRunning, @unchecked Sendable {
    enum Kind { case read, write }

    struct Stats {
        var maxActive: Int
        var finished: Int
    }

    var hangStatus = false
    private let lock = NSLock()
    private var activeReads = 0
    private var activeWrites = 0
    private var maxReads = 0
    private var maxWrites = 0
    private var finishedReads = 0
    private var finishedWrites = 0

    func snapshot(kind: Kind) -> Stats {
        lock.lock()
        defer { lock.unlock() }
        switch kind {
        case .read: return Stats(maxActive: maxReads, finished: finishedReads)
        case .write: return Stats(maxActive: maxWrites, finished: finishedWrites)
        }
    }

    func run(_ request: ProcessRequest) async throws -> ProcessOutput {
        let writing = GitCLIEngine.isGitMutation(request.arguments)
        enter(writing: writing)
        defer { leave(writing: writing) }
        if hangStatus && request.arguments.contains("status") {
            try await Task.sleep(nanoseconds: 2_000_000_000)
        } else {
            try await Task.sleep(nanoseconds: 120_000_000)
        }
        try Task.checkCancellation()
        let stdout: Data
        if request.arguments.contains("status") {
            stdout = Data("# branch.oid abc\u{0}# branch.head main\u{0}".utf8)
        } else {
            stdout = Data()
        }
        return ProcessOutput(executable: request.executable, arguments: request.arguments, exitCode: 0, stdout: stdout, stderr: Data())
    }

    private func enter(writing: Bool) {
        lock.lock()
        defer { lock.unlock() }
        if writing {
            activeWrites += 1
            maxWrites = max(maxWrites, activeWrites)
        } else {
            activeReads += 1
            maxReads = max(maxReads, activeReads)
        }
    }

    private func leave(writing: Bool) {
        lock.lock()
        defer { lock.unlock() }
        if writing {
            activeWrites -= 1
            finishedWrites += 1
        } else {
            activeReads -= 1
            finishedReads += 1
        }
    }
}
