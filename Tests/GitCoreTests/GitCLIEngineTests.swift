import Foundation
import XCTest
@testable import GitCore

final class GitCLIEngineTests: XCTestCase {
    private var roots: [URL] = []
    private let engine = GitCLIEngine()

    override func tearDown() {
        for root in roots {
            try? FileManager.default.removeItem(at: root)
        }
        roots = []
        super.tearDown()
    }

    func testOpenStatusStageCommitAndLog() async throws {
        let root = try makeRepo()
        try write("uno\n", name: "a.txt", in: root)
        let repo = try await engine.openRepository(at: root.appendingPathComponent("a.txt").path)
        XCTAssertEqual(
            URL(fileURLWithPath: repo.path).resolvingSymlinksInPath().path,
            root.resolvingSymlinksInPath().path
        )

        var status = try await engine.status(in: repo, includeIgnored: false)
        XCTAssertEqual(status.branchName, "main")
        XCTAssertTrue(status.isUnborn)
        XCTAssertEqual(status.unstaged.map(\.path), ["a.txt"])

        try await engine.stage(paths: ["a.txt"], in: repo)
        status = try await engine.status(in: repo, includeIgnored: false)
        XCTAssertEqual(status.staged.map(\.path), ["a.txt"])
        XCTAssertTrue(status.unstaged.isEmpty)

        let sha = try await engine.commit(message: "asunto corto\n\ncuerpo del cambio\n", in: repo)
        XCTAssertEqual(sha.count, 40)
        let commits = try await engine.log(in: repo, limit: 10)
        XCTAssertEqual(commits.count, 1)
        XCTAssertEqual(commits[0].subject, "asunto corto")
        XCTAssertEqual(commits[0].body, "cuerpo del cambio")
        XCTAssertEqual(commits[0].sha, sha)

        let missing = FileManager.default.temporaryDirectory.appendingPathComponent("gitforge-no-repo-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: missing, withIntermediateDirectories: true)
        roots.append(missing)
        do {
            _ = try await engine.openRepository(at: missing.path)
            XCTFail("no era un repo")
        } catch let error as GitError {
            guard case .notARepository = error else { throw error }
        }
    }

    func testEmptyCommitAndNothingStaged() async throws {
        let repo = try await engine.openRepository(at: makeRepo().path)
        do {
            _ = try await engine.commit(message: "   \n", in: repo)
            XCTFail("mensaje vacío")
        } catch let error as GitError {
            guard case .emptyCommitMessage = error else { throw error }
        }
        try write("x\n", name: "a.txt", in: URL(fileURLWithPath: repo.path))
        try await engine.stage(paths: ["a.txt"], in: repo)
        _ = try await engine.commit(message: "primero", in: repo)
        do {
            _ = try await engine.commit(message: "segundo", in: repo)
            XCTFail("no había nada preparado")
        } catch let error as GitError {
            guard case .nothingToCommit = error else { throw error }
        }
    }

    func testStageSingleHunkAndUnstageIt() async throws {
        let root = try makeRepo()
        let original = numberedLines(40)
        try write(original, name: "file.txt", in: root)
        let repo = try await engine.openRepository(at: root.path)
        try await engine.stage(paths: ["file.txt"], in: repo)
        _ = try await engine.commit(message: "base", in: repo)

        var lines = original.split(separator: "\n").map(String.init)
        lines[1] = "CHANGED-EARLY"
        lines[29] = "CHANGED-LATE"
        try write(lines.joined(separator: "\n") + "\n", name: "file.txt", in: root)

        let unstaged = try await engine.diff(scope: .unstaged, paths: ["file.txt"], in: repo)
        XCTAssertEqual(unstaged.first?.hunks.count, 2)
        let first = try XCTUnwrap(unstaged.first?.hunks.first)
        try await engine.stageHunk(first, path: "file.txt", in: repo)

        let staged = try await engine.diff(scope: .staged, paths: ["file.txt"], in: repo)
        let still = try await engine.diff(scope: .unstaged, paths: ["file.txt"], in: repo)
        let stagedText = staged.map(\.raw).joined()
        let unstagedText = still.map(\.raw).joined()
        XCTAssertTrue(stagedText.contains("CHANGED-EARLY"))
        XCTAssertFalse(stagedText.contains("CHANGED-LATE"))
        XCTAssertTrue(unstagedText.contains("CHANGED-LATE"))
        XCTAssertFalse(unstagedText.contains("CHANGED-EARLY"))

        let stagedHunk = try XCTUnwrap(staged.first?.hunks.first)
        try await engine.unstageHunk(stagedHunk, path: "file.txt", in: repo)
        let after = try await engine.diff(scope: .staged, paths: ["file.txt"], in: repo)
        XCTAssertTrue(after.isEmpty || after.allSatisfy { $0.hunks.isEmpty })
    }

    func testBranchesCheckoutRenameAndDelete() async throws {
        let root = try makeRepo()
        try write("a\n", name: "a.txt", in: root)
        let repo = try await engine.openRepository(at: root.path)
        try await engine.stage(paths: ["a.txt"], in: repo)
        let sha = try await engine.commit(message: "base", in: repo)

        try await engine.createBranch(name: "feature", from: nil, in: repo)
        try await engine.checkout(ref: "feature", in: repo)
        var status = try await engine.status(in: repo, includeIgnored: false)
        XCTAssertEqual(status.branchName, "feature")

        try await engine.renameBranch(from: "feature", to: "feature-2", in: repo)
        try await engine.checkout(ref: sha, in: repo)
        status = try await engine.status(in: repo, includeIgnored: false)
        XCTAssertTrue(status.isDetached)
        XCTAssertEqual(status.headOID, sha)

        try await engine.checkout(ref: "main", in: repo)
        try await engine.deleteBranch(name: "feature-2", force: false, in: repo)
        let names = try await engine.branches(in: repo).map(\.name)
        XCTAssertEqual(names, ["main"])

        do {
            try await engine.deleteBranch(name: "main", force: false, in: repo)
            XCTFail("no se puede borrar la rama actual")
        } catch is GitError {
        }
    }

    func testFastForwardNoFastForwardAndUpToDate() async throws {
        let repo = try await seededRepo()
        try await engine.createBranch(name: "feature", from: nil, in: repo)
        try await engine.checkout(ref: "feature", in: repo)
        try write("feature\n", name: "b.txt", in: URL(fileURLWithPath: repo.path))
        try await engine.stage(paths: ["b.txt"], in: repo)
        _ = try await engine.commit(message: "en feature", in: repo)
        try await engine.checkout(ref: "main", in: repo)

        let ff = try await engine.merge(branch: "feature", fastForward: .allow, in: repo)
        guard case .fastForward = ff else {
            return XCTFail("esperaba fast-forward, llegó \(ff)")
        }

        try await engine.createBranch(name: "topic", from: nil, in: repo)
        try await engine.checkout(ref: "topic", in: repo)
        try write("topic\n", name: "c.txt", in: URL(fileURLWithPath: repo.path))
        try await engine.stage(paths: ["c.txt"], in: repo)
        _ = try await engine.commit(message: "en topic", in: repo)
        try await engine.checkout(ref: "main", in: repo)
        let noFF = try await engine.merge(branch: "topic", fastForward: .never, in: repo)
        guard case .merged = noFF else {
            return XCTFail("esperaba merge commit, llegó \(noFF)")
        }
        let head = try await engine.log(in: repo, limit: 1)
        XCTAssertEqual(head.first?.parents.count, 2)

        let again = try await engine.merge(branch: "topic", fastForward: .allow, in: repo)
        XCTAssertEqual(again, .upToDate)
    }

    func testConflictModelResolutionAndContinue() async throws {
        let repo = try await conflictedRepo()
        let state = try await engine.integrationState(in: repo)
        XCTAssertEqual(state, .merging)
        let files = try await engine.conflicts(in: repo)
        let file = try XCTUnwrap(files.first)
        XCTAssertEqual(file.path, "file.txt")
        XCTAssertEqual(file.kind, .bothModified)
        XCTAssertTrue(file.ours.content?.contains("from-main") == true)
        XCTAssertTrue(file.base.content?.contains("line2") == true)
        XCTAssertTrue(file.theirs.content?.contains("from-feature") == true)
        XCTAssertEqual(file.hunks.count, 1)
        XCTAssertEqual(file.takingWholeSide(.ours).contains("from-main"), true)

        let before = try String(contentsOf: URL(fileURLWithPath: repo.path).appendingPathComponent("file.txt"), encoding: .utf8)
        do {
            try await engine.writeWorkingTreeFile(relativePath: "file.txt", contents: "resuelto\n", in: repo, confirmed: false)
            XCTFail("tenía que exigir confirmación")
        } catch let error as GitError {
            guard case .confirmationRequired = error else { throw error }
        }
        let still = try String(contentsOf: URL(fileURLWithPath: repo.path).appendingPathComponent("file.txt"), encoding: .utf8)
        XCTAssertEqual(still, before)

        try await engine.writeWorkingTreeFile(relativePath: "file.txt", contents: "resuelto\n", in: repo, confirmed: true)
        try await engine.markResolved(path: "file.txt", in: repo)
        let status = try await engine.status(in: repo, includeIgnored: false)
        XCTAssertTrue(status.conflicted.isEmpty)
        try await engine.continueIntegration(in: repo)
        let finished = try await engine.integrationState(in: repo)
        XCTAssertEqual(finished, .none)
        let commits = try await engine.log(in: repo, limit: 1)
        XCTAssertEqual(commits.first?.parents.count, 2)
    }

    func testAbortMergeRestoresCleanTree() async throws {
        let repo = try await conflictedRepo()
        try await engine.abortIntegration(in: repo)
        let status = try await engine.status(in: repo, includeIgnored: false)
        XCTAssertTrue(status.conflicted.isEmpty)
        XCTAssertTrue(status.entries.isEmpty)
        let finished = try await engine.integrationState(in: repo)
        XCTAssertEqual(finished, .none)
        do {
            try await engine.abortIntegration(in: repo)
            XCTFail("ya no había merge")
        } catch let error as GitError {
            guard case .noIntegrationInProgress = error else { throw error }
        }
    }

    func testPathEscapeDoesNotWrite() async throws {
        let repo = try await seededRepo()
        let outside = URL(fileURLWithPath: repo.path).deletingLastPathComponent().appendingPathComponent("gitforge-escape-\(UUID().uuidString).txt")
        defer { try? FileManager.default.removeItem(at: outside) }
        do {
            try await engine.writeWorkingTreeFile(relativePath: "../\(outside.lastPathComponent)", contents: "no", in: repo, confirmed: true)
            XCTFail("la ruta se sale del repo")
        } catch let error as GitError {
            guard case .pathEscapesRepository = error else { throw error }
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: outside.path))
    }

    func testRestoreRequiresConfirmation() async throws {
        let root = try makeRepo()
        try write("original\n", name: "a.txt", in: root)
        let repo = try await engine.openRepository(at: root.path)
        try await engine.stage(paths: ["a.txt"], in: repo)
        _ = try await engine.commit(message: "base", in: repo)
        try write("cambiado\n", name: "a.txt", in: root)
        do {
            try await engine.restoreWorktree(paths: ["a.txt"], in: repo, confirmed: false)
            XCTFail("confirmación")
        } catch let error as GitError {
            guard case .confirmationRequired = error else { throw error }
        }
        var text = try String(contentsOf: root.appendingPathComponent("a.txt"), encoding: .utf8)
        XCTAssertEqual(text, "cambiado\n")
        try await engine.restoreWorktree(paths: ["a.txt"], in: repo, confirmed: true)
        text = try String(contentsOf: root.appendingPathComponent("a.txt"), encoding: .utf8)
        XCTAssertEqual(text, "original\n")
    }

    func testIgnoredFilesAndSessionGate() async throws {
        let root = try makeRepo()
        try write("secreto.txt\n", name: ".gitignore", in: root)
        let repo = try await engine.openRepository(at: root.path)
        try await engine.stage(paths: [".gitignore"], in: repo)
        _ = try await engine.commit(message: "ignore", in: repo)
        try write("no\n", name: "secreto.txt", in: root)
        try write("si\n", name: "visible.txt", in: root)
        let hidden = try await engine.status(in: repo, includeIgnored: false)
        XCTAssertFalse(hidden.entries.contains { $0.path == "secreto.txt" })
        let shown = try await engine.status(in: repo, includeIgnored: true)
        XCTAssertTrue(shown.ignored.contains { $0.path == "secreto.txt" })

        let session = RepositorySession(repository: repo, engine: engine)
        try await session.refresh()
        do {
            try await session.writeResolution(path: "visible.txt", contents: "no", confirmed: false)
            XCTFail("la sesión también exige confirmación")
        } catch let error as GitError {
            guard case .confirmationRequired = error else { throw error }
        }
        let text = try String(contentsOf: root.appendingPathComponent("visible.txt"), encoding: .utf8)
        XCTAssertEqual(text, "si\n")
    }

    func testPushFetchAndPullAgainstBareRemote() async throws {
        let originRoot = try makeRepo()
        try write("v1\n", name: "file.txt", in: originRoot)
        let origin = try await engine.openRepository(at: originRoot.path)
        try await engine.stage(paths: ["file.txt"], in: origin)
        _ = try await engine.commit(message: "v1", in: origin)

        let bare = FileManager.default.temporaryDirectory.appendingPathComponent("gitforge-bare-\(UUID().uuidString)", isDirectory: true)
        roots.append(bare)
        try TempGit.run(["init", "-b", "main", "--bare", bare.path], at: originRoot)
        try await engine.addRemote(name: "origin", url: bare.path, in: origin)
        try await engine.push(remote: "origin", refspec: "main", setUpstream: true, in: origin)
        let remotes = try await engine.listRemotes(in: origin)
        XCTAssertEqual(remotes.map(\.name), ["origin"])

        let clone = FileManager.default.temporaryDirectory.appendingPathComponent("gitforge-clone-\(UUID().uuidString)", isDirectory: true)
        roots.append(clone)
        try TempGit.run(["clone", bare.path, clone.path], at: originRoot)
        try TempGit.run(["config", "user.email", "test@gitforge.local"], at: clone)
        try TempGit.run(["config", "user.name", "Ariadna Test"], at: clone)
        try TempGit.run(["config", "commit.gpgsign", "false"], at: clone)
        try write("v2\n", name: "file.txt", in: clone)
        let second = try await engine.openRepository(at: clone.path)
        try await engine.stage(paths: ["file.txt"], in: second)
        _ = try await engine.commit(message: "v2", in: second)
        try await engine.push(remote: "origin", refspec: "main", setUpstream: false, in: second)

        let pulled = try await engine.pull(remote: "origin", refspec: "main", rebase: false, in: origin)
        guard case .fastForward = pulled else {
            return XCTFail("esperaba fast-forward en el pull, llegó \(pulled)")
        }
        let text = try String(contentsOf: originRoot.appendingPathComponent("file.txt"), encoding: .utf8)
        XCTAssertEqual(text, "v2\n")
        let log = try await engine.log(in: origin, limit: 5)
        XCTAssertEqual(log.first?.subject, "v2")
    }

    func testDetachedHeadWarnsBeforeCommit() async throws {
        let repo = try await seededRepo()
        let commits = try await engine.log(in: repo, limit: 1)
        let sha = try XCTUnwrap(commits.first?.sha)
        try await engine.checkout(ref: sha, in: repo)
        let status = try await engine.status(in: repo, includeIgnored: false)
        XCTAssertTrue(status.isDetached)
        XCTAssertNil(status.branchName)
        XCTAssertNotNil(status.detachedCommitWarning)

        let root = URL(fileURLWithPath: repo.path)
        try write("extra\n", name: "extra.txt", in: root)
        try await engine.stage(paths: ["extra.txt"], in: repo)
        do {
            _ = try await engine.commit(message: "suelto", in: repo)
            XCTFail("el commit detached tenía que pedir confirmación")
        } catch let error as GitError {
            guard case .detachedHeadCommit = error else { throw error }
        }
        let before = try await engine.log(in: repo, limit: 10)
        XCTAssertEqual(before.count, 1)

        let created = try await engine.commit(message: "suelto", in: repo, allowDetached: true)
        XCTAssertEqual(created.count, 40)
        let after = try await engine.status(in: repo, includeIgnored: false)
        XCTAssertTrue(after.isDetached)
        let afterLog = try await engine.log(in: repo, limit: 10)
        XCTAssertEqual(afterLog.count, 2)
    }

    func testStageIsolatedAndContiguousHunks() async throws {
        try await stageFirstOfTwo(file: "aislado.txt", gap: 18, lineEnding: "\n", trailingNewline: true)
        try await stageFirstOfTwo(file: "contiguo.txt", gap: 8, lineEnding: "\n", trailingNewline: true)
    }

    func testStageHunkWithCRLFAndMissingFinalNewline() async throws {
        try await stageFirstOfTwo(file: "crlf.txt", gap: 16, lineEnding: "\r\n", trailingNewline: true)
        try await stageFirstOfTwo(file: "sin-final.txt", gap: 16, lineEnding: "\n", trailingNewline: false)
    }

    func testStageHunkInUnicodePathWithSpaces() async throws {
        try await stageFirstOfTwo(file: "notas/cañón archivo.txt", gap: 16, lineEnding: "\n", trailingNewline: true)
    }

    func testPullRebaseReplaysLocalCommit() async throws {
        let originRoot = try makeRepo()
        try write("base\n", name: "a.txt", in: originRoot)
        let origin = try await engine.openRepository(at: originRoot.path)
        try await engine.stage(paths: ["a.txt"], in: origin)
        _ = try await engine.commit(message: "base", in: origin)
        let bare = FileManager.default.temporaryDirectory.appendingPathComponent("ariadna-bare-\(UUID().uuidString)", isDirectory: true)
        roots.append(bare)
        try TempGit.run(["init", "-b", "main", "--bare", bare.path], at: originRoot)
        try await engine.addRemote(name: "origin", url: bare.path, in: origin)
        try await engine.push(remote: "origin", refspec: "main", setUpstream: true, in: origin)

        let clone = FileManager.default.temporaryDirectory.appendingPathComponent("ariadna-clone-\(UUID().uuidString)", isDirectory: true)
        roots.append(clone)
        try TempGit.run(["clone", bare.path, clone.path], at: originRoot)
        try TempGit.run(["config", "user.email", "test@gitforge.local"], at: clone)
        try TempGit.run(["config", "user.name", "Ariadna Test"], at: clone)
        try TempGit.run(["config", "commit.gpgsign", "false"], at: clone)
        try write("local\n", name: "b.txt", in: clone)
        let second = try await engine.openRepository(at: clone.path)
        try await engine.stage(paths: ["b.txt"], in: second)
        _ = try await engine.commit(message: "local", in: second)

        try write("remoto\n", name: "c.txt", in: originRoot)
        try await engine.stage(paths: ["c.txt"], in: origin)
        _ = try await engine.commit(message: "remoto", in: origin)
        try await engine.push(remote: "origin", refspec: "main", setUpstream: false, in: origin)

        let result = try await engine.pull(remote: "origin", refspec: "main", rebase: true, in: second)
        switch result {
        case .fastForward, .merged:
            break
        default:
            let text = try await engine.log(in: second, limit: 5).map(\.subject)
            XCTFail("el rebase no terminó limpio: \(result) log=\(text)")
        }
        let subjects = try await engine.log(in: second, limit: 5).map(\.subject)
        XCTAssertEqual(subjects.first, "local")
        XCTAssertTrue(subjects.contains("remoto"))
    }

    func testOpeningRepoDoesNotWriteHooksPath() async throws {
        let root = try makeRepo()
        let hooks = root.appendingPathComponent(".githooks")
        try FileManager.default.createDirectory(at: hooks, withIntermediateDirectories: true)
        let hook = hooks.appendingPathComponent("pre-commit")
        try Data("#!/bin/sh\nexit 0\n".utf8).write(to: hook)
        try FileManager.default.setAttributes([.posixPermissions: 0o644], ofItemAtPath: hook.path)
        try TempGit.run(["config", "--unset", "core.hooksPath"], at: root)
        _ = try await engine.openRepository(at: root.path)
        do {
            _ = try TempGit.output(["config", "--local", "--get", "core.hooksPath"], at: root)
            XCTFail("abrir el repo no debe escribir core.hooksPath")
        } catch {
            // git config --get termina mal si la clave no existe: es lo esperado.
        }
        XCTAssertFalse(FileManager.default.isExecutableFile(atPath: hook.path))
    }

    func testCheckoutRemoteTrackingCreatesLocalBranch() async throws {
        let originRoot = try makeRepo()
        try write("base\n", name: "a.txt", in: originRoot)
        let origin = try await engine.openRepository(at: originRoot.path)
        try await engine.stage(paths: ["a.txt"], in: origin)
        _ = try await engine.commit(message: "base", in: origin)
        try await engine.createBranch(name: "feature", from: nil, in: origin)
        try await engine.checkout(ref: "feature", in: origin)
        try write("feature\n", name: "b.txt", in: originRoot)
        try await engine.stage(paths: ["b.txt"], in: origin)
        _ = try await engine.commit(message: "en feature", in: origin)
        try await engine.checkout(ref: "main", in: origin)

        let bare = FileManager.default.temporaryDirectory.appendingPathComponent("ariadna-track-\(UUID().uuidString)", isDirectory: true)
        roots.append(bare)
        try TempGit.run(["init", "-b", "main", "--bare", bare.path], at: originRoot)
        try await engine.addRemote(name: "origin", url: bare.path, in: origin)
        try await engine.push(remote: "origin", refspec: "main", setUpstream: true, in: origin)
        try await engine.push(remote: "origin", refspec: "feature", setUpstream: true, in: origin)

        let clone = FileManager.default.temporaryDirectory.appendingPathComponent("ariadna-track-clone-\(UUID().uuidString)", isDirectory: true)
        roots.append(clone)
        try TempGit.run(["clone", bare.path, clone.path], at: originRoot)
        let second = try await engine.openRepository(at: clone.path)
        let before = try await engine.branches(in: second).map(\.name)
        XCTAssertFalse(before.contains("feature"))

        try await engine.checkout(ref: "origin/feature", in: second)
        let status = try await engine.status(in: second, includeIgnored: false)
        XCTAssertFalse(status.isDetached)
        XCTAssertEqual(status.branchName, "feature")
        let feature = try await engine.branches(in: second).first { $0.name == "feature" }
        XCTAssertEqual(feature?.upstream, "origin/feature")
        XCTAssertEqual(feature?.isHead, true)
    }

    func testCheckoutDirtyTreeExplainsInSpanish() async throws {
        let root = try makeRepo()
        try write("uno\n", name: "a.txt", in: root)
        let repo = try await engine.openRepository(at: root.path)
        try await engine.stage(paths: ["a.txt"], in: repo)
        _ = try await engine.commit(message: "base", in: repo)
        try await engine.createBranch(name: "otra", from: nil, in: repo)
        try await engine.checkout(ref: "otra", in: repo)
        try write("desde-otra\n", name: "a.txt", in: root)
        try await engine.stage(paths: ["a.txt"], in: repo)
        _ = try await engine.commit(message: "otra", in: repo)
        try await engine.checkout(ref: "main", in: repo)
        try write("sucio\n", name: "a.txt", in: root)

        do {
            try await engine.checkout(ref: "otra", in: repo)
            XCTFail("el checkout tenía que negarse")
        } catch let error as GitError {
            guard case .localChangesBlockCheckout(let detail) = error else {
                return XCTFail("esperaba un aviso en español, llegó \(error)")
            }
            let message = error.errorDescription ?? ""
            XCTAssertTrue(message.contains("cambios locales"))
            XCTAssertTrue(message.contains("cambio de rama"))
            XCTAssertFalse(detail.isEmpty)
        }
        let status = try await engine.status(in: repo, includeIgnored: false)
        XCTAssertEqual(status.branchName, "main")
        let text = try String(contentsOf: root.appendingPathComponent("a.txt"), encoding: .utf8)
        XCTAssertEqual(text, "sucio\n")
    }

    func testStageHunkOfUntrackedFile() async throws {
        let root = try makeRepo()
        try write("linea1\nlinea2\nlinea3\n", name: "nuevo.txt", in: root)
        let repo = try await engine.openRepository(at: root.path)
        let preview = try await engine.diff(scope: .unstaged, paths: ["nuevo.txt"], in: repo)
        let hunk = try XCTUnwrap(preview.first?.hunks.first)
        try await engine.stageHunk(hunk, path: "nuevo.txt", in: repo)
        let status = try await engine.status(in: repo, includeIgnored: false)
        let entry = try XCTUnwrap(status.entries.first { $0.path == "nuevo.txt" })
        XCTAssertFalse(entry.isUntracked)
        XCTAssertTrue(entry.isStaged)
        let staged = try await engine.diff(scope: .staged, paths: ["nuevo.txt"], in: repo)
        XCTAssertTrue(staged.map(\.raw).joined().contains("linea2"))
    }

    func testFailedUntrackedHunkDoesNotLeaveIntentToAdd() async throws {
        let root = try makeRepo()
        try write("linea1\nlinea2\n", name: "nuevo.txt", in: root)
        let repo = try await engine.openRepository(at: root.path)
        let bogus = DiffHunk(
            id: "no-esta",
            oldStart: 40,
            oldCount: 1,
            newStart: 40,
            newCount: 1,
            heading: "",
            lines: [],
            unifiedText: "@@ -40 +40 @@\n"
        )
        do {
            try await engine.stageHunk(bogus, path: "nuevo.txt", in: repo)
            XCTFail("ese hunk no existe")
        } catch let error as GitError {
            guard case .hunkNotFound = error else { throw error }
        }
        let status = try await engine.status(in: repo, includeIgnored: false)
        let entry = try XCTUnwrap(status.entries.first { $0.path == "nuevo.txt" })
        XCTAssertTrue(entry.isUntracked)
        XCTAssertFalse(entry.isStaged)
    }

    func testCheckoutRemoteDoesNotSwitchDivergedLocal() async throws {
        let root = try makeRepo()
        try write("base\n", name: "a.txt", in: root)
        let repo = try await engine.openRepository(at: root.path)
        try await engine.stage(paths: ["a.txt"], in: repo)
        let base = try await engine.commit(message: "base", in: repo)
        try await engine.createBranch(name: "feature", from: nil, in: repo)
        let bare = FileManager.default.temporaryDirectory.appendingPathComponent("ariadna-div-\(UUID().uuidString)", isDirectory: true)
        roots.append(bare)
        try TempGit.run(["init", "-b", "main", "--bare", bare.path], at: root)
        try await engine.addRemote(name: "origin", url: bare.path, in: repo)
        try await engine.push(remote: "origin", refspec: "main", setUpstream: true, in: repo)
        try await engine.push(remote: "origin", refspec: "feature", setUpstream: false, in: repo)
        try await engine.checkout(ref: "feature", in: repo)
        try write("local\n", name: "b.txt", in: root)
        try await engine.stage(paths: ["b.txt"], in: repo)
        _ = try await engine.commit(message: "solo-local", in: repo)
        try await engine.checkout(ref: "main", in: repo)

        do {
            try await engine.checkout(ref: "origin/feature", in: repo)
            XCTFail("no debía cambiar a la local desincronizada")
        } catch let error as GitError {
            guard case .divergedLocalBranch(let local, let remote) = error else { throw error }
            XCTAssertEqual(local, "feature")
            XCTAssertEqual(remote, "origin/feature")
            let message = error.errorDescription ?? ""
            XCTAssertTrue(message.contains("no coincide"))
        }
        let stayed = try await engine.status(in: repo, includeIgnored: false)
        XCTAssertEqual(stayed.branchName, "main")
        XCTAssertFalse(stayed.isDetached)

        try await engine.checkout(ref: "origin/feature", in: repo, allowDivergedLocal: true)
        let moved = try await engine.status(in: repo, includeIgnored: false)
        XCTAssertEqual(moved.branchName, "feature")
        XCTAssertNotEqual(moved.headOID, base)
    }

    func testPullRebaseConflictAbortAndContinue() async throws {
        let originRoot = try makeRepo()
        try write("line1\nline2\nline3\n", name: "file.txt", in: originRoot)
        let origin = try await engine.openRepository(at: originRoot.path)
        try await engine.stage(paths: ["file.txt"], in: origin)
        _ = try await engine.commit(message: "base", in: origin)
        let bare = FileManager.default.temporaryDirectory.appendingPathComponent("ariadna-rebase-\(UUID().uuidString)", isDirectory: true)
        roots.append(bare)
        try TempGit.run(["init", "-b", "main", "--bare", bare.path], at: originRoot)
        try await engine.addRemote(name: "origin", url: bare.path, in: origin)
        try await engine.push(remote: "origin", refspec: "main", setUpstream: true, in: origin)

        let clone = FileManager.default.temporaryDirectory.appendingPathComponent("ariadna-rebase-clone-\(UUID().uuidString)", isDirectory: true)
        roots.append(clone)
        try TempGit.run(["clone", bare.path, clone.path], at: originRoot)
        try TempGit.run(["config", "user.email", "test@gitforge.local"], at: clone)
        try TempGit.run(["config", "user.name", "Ariadna Test"], at: clone)
        try TempGit.run(["config", "commit.gpgsign", "false"], at: clone)
        try TempGit.run(["config", "core.hooksPath", "/dev/null"], at: clone)

        try write("line1\nfrom-origin\nline3\n", name: "file.txt", in: originRoot)
        try await engine.stage(paths: ["file.txt"], in: origin)
        _ = try await engine.commit(message: "origin", in: origin)
        try await engine.push(remote: "origin", refspec: "main", setUpstream: false, in: origin)

        let second = try await engine.openRepository(at: clone.path)
        try write("line1\nfrom-clone\nline3\n", name: "file.txt", in: clone)
        try await engine.stage(paths: ["file.txt"], in: second)
        _ = try await engine.commit(message: "clone", in: second)

        let conflicted = try await engine.pull(remote: "origin", refspec: "main", rebase: true, in: second)
        guard case .conflicts(let paths) = conflicted else {
            return XCTFail("esperaba conflicto de rebase, llegó \(conflicted)")
        }
        XCTAssertEqual(paths, ["file.txt"])
        let state = try await engine.integrationState(in: second)
        XCTAssertEqual(state, .rebasing)

        try await engine.abortIntegration(in: second)
        let aborted = try await engine.integrationState(in: second)
        XCTAssertEqual(aborted, .none)
        let clean = try await engine.status(in: second, includeIgnored: false)
        XCTAssertTrue(clean.conflicted.isEmpty)
        XCTAssertEqual(clean.branchName, "main")

        let again = try await engine.pull(remote: "origin", refspec: "main", rebase: true, in: second)
        guard case .conflicts = again else {
            return XCTFail("el segundo rebase también tenía que chocar, llegó \(again)")
        }
        try await engine.writeWorkingTreeFile(
            relativePath: "file.txt",
            contents: "line1\nresuelto\nline3\n",
            in: second,
            confirmed: true
        )
        try await engine.markResolved(path: "file.txt", in: second)
        try await engine.continueIntegration(in: second)
        let finished = try await engine.integrationState(in: second)
        XCTAssertEqual(finished, .none)
        let text = try String(contentsOf: clone.appendingPathComponent("file.txt"), encoding: .utf8)
        XCTAssertEqual(text, "line1\nresuelto\nline3\n")
        let subjects = try await engine.log(in: second, limit: 5).map(\.subject)
        XCTAssertTrue(subjects.contains("clone"))
        XCTAssertTrue(subjects.contains("origin"))
    }

    private func stageFirstOfTwo(file: String, gap: Int, lineEnding: String, trailingNewline: Bool) async throws {
        let root = try makeRepo()
        var lines = (1...30).map { "line\($0)" }
        let original = join(lines, lineEnding: lineEnding, trailingNewline: trailingNewline)
        try writeRaw(original, name: file, in: root)
        let repo = try await engine.openRepository(at: root.path)
        try await engine.stage(paths: [file], in: repo)
        _ = try await engine.commit(message: "base \(file)", in: repo)

        lines[1] = "CHANGED-EARLY"
        lines[1 + gap] = "CHANGED-LATE"
        let updated = join(lines, lineEnding: lineEnding, trailingNewline: trailingNewline)
        try writeRaw(updated, name: file, in: root)

        let unstaged = try await engine.diff(scope: .unstaged, paths: [file], in: repo)
        let hunks = try XCTUnwrap(unstaged.first?.hunks)
        XCTAssertEqual(hunks.count, 2, "\(file) debería tener dos hunks, hay \(hunks.count)")
        try await engine.stageHunk(hunks[0], path: file, in: repo)

        let stagedText = try await engine.diff(scope: .staged, paths: [file], in: repo).map(\.raw).joined()
        let restText = try await engine.diff(scope: .unstaged, paths: [file], in: repo).map(\.raw).joined()
        XCTAssertTrue(stagedText.contains("CHANGED-EARLY"), file)
        XCTAssertFalse(stagedText.contains("CHANGED-LATE"), file)
        XCTAssertTrue(restText.contains("CHANGED-LATE"), file)
        XCTAssertFalse(restText.contains("CHANGED-EARLY"), file)

        let stagedFiles = try await engine.diff(scope: .staged, paths: [file], in: repo)
        let stagedHunk = try XCTUnwrap(stagedFiles.first?.hunks.first)
        try await engine.unstageHunk(stagedHunk, path: file, in: repo)
        let after = try await engine.diff(scope: .unstaged, paths: [file], in: repo)
        XCTAssertEqual(after.first?.hunks.count, 2, file)
    }

    private func join(_ lines: [String], lineEnding: String, trailingNewline: Bool) -> String {
        var text = lines.joined(separator: lineEnding)
        if trailingNewline {
            text += lineEnding
        }
        return text
    }

    private func writeRaw(_ contents: String, name: String, in root: URL) throws {
        let url = root.appendingPathComponent(name)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(contents.utf8).write(to: url)
    }

    private func seededRepo() async throws -> Repository {
        let root = try makeRepo()
        try write("line1\nline2\nline3\n", name: "file.txt", in: root)
        let repo = try await engine.openRepository(at: root.path)
        try await engine.stage(paths: ["file.txt"], in: repo)
        _ = try await engine.commit(message: "base", in: repo)
        return repo
    }

    private func conflictedRepo() async throws -> Repository {
        let repo = try await seededRepo()
        try await engine.createBranch(name: "feature", from: nil, in: repo)
        try await engine.checkout(ref: "feature", in: repo)
        try write("line1\nfrom-feature\nline3\n", name: "file.txt", in: URL(fileURLWithPath: repo.path))
        try await engine.stage(paths: ["file.txt"], in: repo)
        _ = try await engine.commit(message: "feature", in: repo)
        try await engine.checkout(ref: "main", in: repo)
        try write("line1\nfrom-main\nline3\n", name: "file.txt", in: URL(fileURLWithPath: repo.path))
        try await engine.stage(paths: ["file.txt"], in: repo)
        _ = try await engine.commit(message: "main", in: repo)
        let result = try await engine.merge(branch: "feature", fastForward: .allow, in: repo)
        guard case .conflicts(let paths) = result else {
            XCTFail("esperaba conflictos, llegó \(result)")
            return repo
        }
        XCTAssertEqual(paths, ["file.txt"])
        return repo
    }

    @discardableResult
    private func makeRepo() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("gitforge-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        roots.append(root)
        try TempGit.run(["init", "-b", "main"], at: root)
        try TempGit.run(["config", "user.email", "test@gitforge.local"], at: root)
        try TempGit.run(["config", "user.name", "Ariadna Test"], at: root)
        try TempGit.run(["config", "commit.gpgsign", "false"], at: root)
        try TempGit.run(["config", "core.filemode", "false"], at: root)
        try TempGit.run(["config", "core.hooksPath", "/dev/null"], at: root)
        try TempGit.run(["config", "core.autocrlf", "false"], at: root)
        try TempGit.run(["config", "core.safecrlf", "false"], at: root)
        return root
    }

    private func write(_ contents: String, name: String, in root: URL) throws {
        try contents.write(to: root.appendingPathComponent(name), atomically: true, encoding: .utf8)
    }

    private func numberedLines(_ count: Int) -> String {
        (1...count).map { "line\($0)" }.joined(separator: "\n") + "\n"
    }
}

enum TempGit {
    @discardableResult
    static func run(_ arguments: [String], at root: URL) throws -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = arguments
        process.currentDirectoryURL = root
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        try process.run()
        // Leer los dos pipes a la vez: si solo se lee uno, el buffer del otro puede bloquear a git.
        let group = DispatchGroup()
        let outBox = LockedBuffer()
        let errBox = LockedBuffer()
        group.enter()
        DispatchQueue.global().async {
            outBox.data = stdout.fileHandleForReading.readDataToEndOfFile()
            group.leave()
        }
        group.enter()
        DispatchQueue.global().async {
            errBox.data = stderr.fileHandleForReading.readDataToEndOfFile()
            group.leave()
        }
        process.waitUntilExit()
        group.wait()
        if process.terminationStatus != 0 {
            let text = String(decoding: outBox.data + errBox.data, as: UTF8.self)
            throw NSError(
                domain: "TempGit",
                code: Int(process.terminationStatus),
                userInfo: [NSLocalizedDescriptionKey: "git \(arguments.joined(separator: " "))\n\(text)"]
            )
        }
        return String(decoding: outBox.data, as: UTF8.self)
    }

    static func output(_ arguments: [String], at root: URL) throws -> String {
        try run(arguments, at: root)
    }
}

private final class LockedBuffer: @unchecked Sendable {
    var data = Data()
}
