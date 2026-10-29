import Foundation

/// Motor Git que habla con el ejecutable `git`.
///
/// La clase no guarda estado del repo. Las mutaciones pasan por `GitOperationGate`
/// de una en una. Las lecturas pueden solaparse entre sí, no con una mutación.
public final class GitCLIEngine: GitServing, Sendable {
    private let runner: any ProcessRunning
    private let gitExecutable: String
    private let environment: [String: String]
    private let timeout: TimeInterval
    private let gate = GitOperationGate()

    public init(
        runner: any ProcessRunning = FoundationProcessRunner(),
        gitExecutable: String = "git",
        environment: [String: String]? = nil,
        timeout: TimeInterval = 180
    ) {
        self.runner = runner
        self.gitExecutable = gitExecutable
        self.environment = environment ?? ProcessEnvironment.sanitizedIncludingLoginPATH()
        self.timeout = timeout
    }

    /// `true` si el subcomando cambia índice, HEAD, refs o el remoto.
    static func isGitMutation(_ arguments: [String]) -> Bool {
        var index = 0
        while index < arguments.count {
            let argument = arguments[index]
            if argument == "-c" || argument == "-C" {
                index += 2
                continue
            }
            if argument.hasPrefix("-") {
                index += 1
                continue
            }
            break
        }
        guard index < arguments.count else { return true }
        let command = arguments[index]
        switch command {
        case "status", "log", "diff", "show", "rev-parse", "for-each-ref", "ls-files", "cat-file", "version":
            return false
        case "remote":
            let subcommand = arguments.dropFirst(index + 1).first { !$0.hasPrefix("-") }
            return subcommand != nil
        default:
            return true
        }
    }

    public func openRepository(at path: String) async throws -> Repository {
        let url = URL(fileURLWithPath: path)
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) else {
            throw GitError.notARepository(path: path)
        }
        let directory = isDirectory.boolValue ? url : url.deletingLastPathComponent()
        let top = try await run(in: directory, arguments: ["rev-parse", "--show-toplevel"], allowNonZero: true)
        guard top.exitCode == 0 else {
            throw GitError.notARepository(path: path)
        }
        let toplevel = trim(top.stdoutString)
        let gitDir = try await run(
            in: URL(fileURLWithPath: toplevel, isDirectory: true),
            arguments: ["rev-parse", "--absolute-git-dir"]
        )
        return Repository(path: toplevel, gitDir: trim(gitDir.stdoutString))
    }

    public func status(in repository: Repository, includeIgnored: Bool) async throws -> GitStatus {
        var arguments = ["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all"]
        if includeIgnored {
            arguments.append("--ignored=matching")
        }
        let output = try await run(in: repository, arguments: arguments)
        return PorcelainParser.parse(output.stdoutString)
    }

    public func stage(paths: [String], in repository: Repository) async throws {
        try await mutatePaths(["add", "--"], paths: paths, in: repository)
    }

    public func unstage(paths: [String], in repository: Repository) async throws {
        try await mutatePaths(["restore", "--staged", "--"], paths: paths, in: repository)
    }

    public func stageHunk(_ hunk: DiffHunk, path: String, in repository: Repository) async throws {
        try await applyHunk(hunk, path: path, scope: .unstaged, reverse: false, in: repository)
    }

    public func unstageHunk(_ hunk: DiffHunk, path: String, in repository: Repository) async throws {
        try await applyHunk(hunk, path: path, scope: .staged, reverse: true, in: repository)
    }

    public func commit(message: String, in repository: Repository, allowDetached: Bool) async throws -> String {
        if message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            throw GitError.emptyCommitMessage
        }
        if !allowDetached {
            let current = try await status(in: repository, includeIgnored: false)
            if current.isDetached {
                throw GitError.detachedHeadCommit
            }
        }
        let output = try await run(
            in: repository,
            arguments: ["commit", "-F", "-"],
            stdin: Data(message.utf8),
            allowNonZero: true
        )
        if output.exitCode != 0 {
            let blob = output.stdoutString + "\n" + output.stderrString
            if blob.contains("nothing to commit") || blob.contains("no changes added to commit") {
                throw GitError.nothingToCommit
            }
            throw failed(output, arguments: ["commit", "-F", "-"])
        }
        return try await requireHEAD(in: repository)
    }

    public func log(in repository: Repository, limit: Int) async throws -> [Commit] {
        let capped = max(1, min(limit, 2_000))
        let format = "%H%x1f%P%x1f%an%x1f%ae%x1f%aI%x1f%s%x1f%b%x1f%D%x1e"
        let output = try await run(
            in: repository,
            arguments: ["log", "-n", String(capped), "--pretty=format:\(format)"],
            allowNonZero: true
        )
        if output.exitCode != 0 {
            let blob = output.stderrString
            if blob.contains("does not have any commits") {
                return []
            }
            throw failed(output, arguments: ["log"])
        }
        return CommitLogParser.parse(output.stdoutString)
    }

    public func diff(scope: DiffScope, paths: [String], in repository: Repository) async throws -> [FileDiff] {
        for path in paths {
            try PathSafety.validateGitPath(path)
        }
        let arguments: [String]
        switch scope {
        case .unstaged:
            arguments = ["diff", "--unified=3", "--"] + paths
        case .staged:
            arguments = ["diff", "--cached", "--unified=3", "--"] + paths
        case .commit(let revision):
            try PathSafety.validateRef(revision)
            var show = ["show", "-m", "--first-parent", "--format=", "--patch", "--unified=3", revision]
            if !paths.isEmpty {
                show.append("--")
                show.append(contentsOf: paths)
            }
            arguments = show
        case .between(let base, let head):
            try PathSafety.validateRef(base)
            try PathSafety.validateRef(head)
            arguments = ["diff", "--unified=3", base, head, "--"] + paths
        }
        let output = try await run(in: repository, arguments: arguments, allowNonZero: false)
        var parsed = DiffParser.parse(output.stdoutString)
        if case .unstaged = scope {
            parsed = try await appendingUntrackedDiffs(parsed, paths: paths, in: repository)
        }
        return parsed
    }

    public func branches(in repository: Repository) async throws -> [Branch] {
        let format = "%(refname:short)%00%(objectname)%00%(HEAD)%00%(upstream:short)%00%(upstream:track)"
        let output = try await run(
            in: repository,
            arguments: ["for-each-ref", "--format=\(format)", "refs/heads"]
        )
        return BranchListParser.parse(output.stdoutString)
    }

    public func remoteTrackingBranches(in repository: Repository) async throws -> [Branch] {
        let format = "%(refname:short)%00%(objectname)%00%(HEAD)%00%(upstream:short)%00%(upstream:track)"
        let output = try await run(
            in: repository,
            arguments: ["for-each-ref", "--format=\(format)", "refs/remotes"]
        )
        return BranchListParser.parse(output.stdoutString).filter { !$0.name.hasSuffix("/HEAD") }
    }

    public func createBranch(name: String, from startPoint: String?, in repository: Repository) async throws {
        try PathSafety.validateRef(name)
        var arguments = ["branch", "--", name]
        if let startPoint {
            try PathSafety.validateRef(startPoint)
            arguments.append(startPoint)
        }
        try await run(in: repository, arguments: arguments)
    }

    public func renameBranch(from oldName: String, to newName: String, in repository: Repository) async throws {
        try PathSafety.validateRef(oldName)
        try PathSafety.validateRef(newName)
        try await run(in: repository, arguments: ["branch", "-m", "--", oldName, newName])
    }

    public func checkout(ref: String, in repository: Repository, allowDivergedLocal: Bool) async throws {
        try PathSafety.validateRef(ref)
        let locals = try await branches(in: repository)
        if locals.contains(where: { $0.name == ref }) {
            try await switchRef(["switch", "--", ref], in: repository)
            return
        }
        // origin/main no deja HEAD detached: crea (o reutiliza) la rama local con seguimiento.
        if !ref.hasSuffix("/HEAD"), try await isRemoteTracking(ref, in: repository) {
            let remoteNames = try await listRemotes(in: repository).map(\.name)
            let localName = localBranchName(forRemoteTracking: ref, remotes: remoteNames)
            try PathSafety.validateRef(localName)
            if let local = locals.first(where: { $0.name == localName }) {
                let remoteOID = try await requireRevision("refs/remotes/\(ref)", in: repository)
                let tracksAnotherBranch = local.upstream != nil && local.upstream != ref
                if local.oid != remoteOID || tracksAnotherBranch {
                    guard allowDivergedLocal else {
                        throw GitError.divergedLocalBranch(local: localName, remote: ref)
                    }
                }
                try await switchRef(["switch", "--", localName], in: repository)
            } else {
                try await switchRef(["switch", "-c", localName, "--track", ref], in: repository)
            }
            return
        }
        try await switchRef(["switch", "--detach", "--", ref], in: repository)
    }

    public func deleteBranch(name: String, force: Bool, in repository: Repository) async throws {
        try PathSafety.validateRef(name)
        try await run(in: repository, arguments: ["branch", force ? "-D" : "-d", "--", name])
    }

    public func merge(branch: String, fastForward: MergeFastForward, in repository: Repository) async throws -> MergeResult {
        try PathSafety.validateRef(branch)
        let flag: String
        switch fastForward {
        case .allow: flag = "--ff"
        case .only: flag = "--ff-only"
        case .never: flag = "--no-ff"
        }
        let output = try await run(
            in: repository,
            arguments: ["-c", "merge.conflictStyle=diff3", "merge", flag, "--", branch],
            extraEnvironment: nonInteractiveGit,
            allowNonZero: true
        )
        return try await classifyMerge(output, arguments: ["merge", flag, branch], in: repository)
    }

    public func integrationState(in repository: Repository) async throws -> IntegrationState {
        if try await gitPathIsDirectory("rebase-merge", in: repository) {
            return .rebasing
        }
        if try await gitPathIsDirectory("rebase-apply", in: repository) {
            return .rebasing
        }
        if try await refExists("CHERRY_PICK_HEAD", in: repository) {
            return .cherryPicking
        }
        if try await refExists("MERGE_HEAD", in: repository) {
            return .merging
        }
        return .none
    }

    public func abortIntegration(in repository: Repository) async throws {
        switch try await integrationState(in: repository) {
        case .none:
            throw GitError.noIntegrationInProgress
        case .merging:
            try await run(in: repository, arguments: ["merge", "--abort"])
        case .rebasing:
            try await run(in: repository, arguments: ["rebase", "--abort"])
        case .cherryPicking:
            try await run(in: repository, arguments: ["cherry-pick", "--abort"])
        }
    }

    public func continueIntegration(in repository: Repository) async throws {
        switch try await integrationState(in: repository) {
        case .none:
            throw GitError.noIntegrationInProgress
        case .merging:
            try await run(
                in: repository,
                arguments: ["merge", "--continue"],
                extraEnvironment: nonInteractiveGit
            )
        case .rebasing:
            try await run(
                in: repository,
                arguments: ["-c", "core.editor=/usr/bin/true", "rebase", "--continue"],
                extraEnvironment: nonInteractiveGit
            )
        case .cherryPicking:
            try await run(
                in: repository,
                arguments: ["-c", "core.editor=/usr/bin/true", "cherry-pick", "--continue"],
                extraEnvironment: nonInteractiveGit
            )
        }
    }

    public func conflicts(in repository: Repository) async throws -> [ConflictFile] {
        let listing = try await run(in: repository, arguments: ["ls-files", "-u", "-z"])
        let stages = UnmergedIndexParser.parse(listing.stdoutString)
        if stages.isEmpty { return [] }

        let current = try await status(in: repository, includeIgnored: false)
        var kinds: [String: ConflictKind] = [:]
        for entry in current.conflicted {
            if let kind = entry.conflictKind {
                kinds[entry.path] = kind
            }
        }

        var grouped: [String: [Int: String]] = [:]
        for stage in stages {
            grouped[stage.path, default: [:]][stage.stage] = stage.oid
        }

        var files: [ConflictFile] = []
        for path in grouped.keys.sorted() {
            let byStage = grouped[path] ?? [:]
            let ours = try await side(oid: byStage[2], in: repository)
            let base = try await side(oid: byStage[1], in: repository)
            let theirs = try await side(oid: byStage[3], in: repository)
            let working = try readWorkingTree(path: path, in: repository)
            let kind = kinds[path] ?? inferredKind(byStage)
            files.append(
                ConflictFile(
                    path: path,
                    kind: kind,
                    ours: ours,
                    base: base,
                    theirs: theirs,
                    workingTree: working.text,
                    hunks: working.hunks
                )
            )
        }
        return files
    }

    public func writeWorkingTreeFile(
        relativePath: String,
        contents: String,
        in repository: Repository,
        confirmed: Bool
    ) async throws {
        try UserConfirmation.require(confirmed, action: "escribir \(relativePath) en el disco")
        let root = repository.path
        try await gate.write {
            let url = try PathSafety.resolve(relativePath: relativePath, repoRoot: root)
            let directory = url.deletingLastPathComponent()
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            try Data(contents.utf8).write(to: url, options: .atomic)
        }
    }

    public func markResolved(path: String, in repository: Repository) async throws {
        try PathSafety.validateGitPath(path)
        try await run(in: repository, arguments: ["add", "--", path])
    }

    public func restoreWorktree(paths: [String], in repository: Repository, confirmed: Bool) async throws {
        try UserConfirmation.require(confirmed, action: "descartar cambios del árbol de trabajo")
        try await mutatePaths(["restore", "--worktree", "--"], paths: paths, in: repository)
    }

    public func listRemotes(in repository: Repository) async throws -> [Remote] {
        let output = try await run(in: repository, arguments: ["remote", "-v"])
        return RemoteListParser.parse(output.stdoutString)
    }

    public func addRemote(name: String, url: String, in repository: Repository) async throws {
        try PathSafety.validateRef(name)
        if url.isEmpty || url.hasPrefix("-") || url.contains("\n") || url.contains("\0") {
            throw GitError.invalidReference(url)
        }
        try await run(in: repository, arguments: ["remote", "add", "--", name, url])
    }

    public func fetch(remote: String, in repository: Repository) async throws {
        try PathSafety.validateRef(remote)
        try await run(in: repository, arguments: ["fetch", "--", remote])
    }

    public func pull(remote: String, refspec: String?, rebase: Bool, in repository: Repository) async throws -> MergeResult {
        try PathSafety.validateRef(remote)
        if let refspec { try PathSafety.validateRef(refspec) }
        var arguments = ["-c", "merge.conflictStyle=diff3", "pull", rebase ? "--rebase" : "--no-rebase", remote]
        if let refspec { arguments.append(refspec) }
        let output = try await run(
            in: repository,
            arguments: arguments,
            extraEnvironment: nonInteractiveGit,
            allowNonZero: true
        )
        return try await classifyMerge(output, arguments: arguments, in: repository)
    }

    public func push(remote: String, refspec: String?, setUpstream: Bool, in repository: Repository) async throws {
        try PathSafety.validateRef(remote)
        if let refspec { try PathSafety.validateRef(refspec) }
        var arguments = ["push"]
        if setUpstream { arguments.append("--set-upstream") }
        arguments.append(remote)
        if let refspec { arguments.append(refspec) }
        try await run(in: repository, arguments: arguments)
    }

    // MARK: - Privado

    private func appendingUntrackedDiffs(_ parsed: [FileDiff], paths: [String], in repository: Repository) async throws -> [FileDiff] {
        guard !paths.isEmpty else { return parsed }
        let already = Set(parsed.filter { !$0.hunks.isEmpty }.map(\.path))
        let current = try await status(in: repository, includeIgnored: false)
        let missing = paths.filter { path in
            !already.contains(path) && current.entries.contains { $0.path == path && $0.isUntracked }
        }
        guard !missing.isEmpty else { return parsed }
        var extra: [FileDiff] = []
        for path in missing {
            // Exit 1 es lo normal: hay diferencias contra /dev/null.
            let output = try await run(
                in: repository,
                arguments: ["diff", "--no-index", "--unified=3", "--", "/dev/null", path],
                allowNonZero: true
            )
            guard output.exitCode == 0 || output.exitCode == 1 else {
                throw failed(output, arguments: ["diff", "--no-index", path])
            }
            extra.append(contentsOf: DiffParser.parse(output.stdoutString))
        }
        return parsed + extra
    }

    private func switchRef(_ arguments: [String], in repository: Repository) async throws {
        do {
            try await run(in: repository, arguments: arguments)
        } catch let error as GitError {
            if case .commandFailed(_, _, let stderr) = error, Self.blocksBranchSwitch(stderr) {
                throw GitError.localChangesBlockCheckout(detail: GitError.truncate(stderr))
            }
            throw error
        }
    }

    private func isRemoteTracking(_ ref: String, in repository: Repository) async throws -> Bool {
        let output = try await run(
            in: repository,
            arguments: ["rev-parse", "-q", "--verify", "refs/remotes/\(ref)"],
            allowNonZero: true
        )
        return output.exitCode == 0
    }

    private func localBranchName(forRemoteTracking ref: String, remotes: [String]) -> String {
        let ordered = remotes.sorted { $0.count > $1.count }
        for remote in ordered {
            let prefix = remote + "/"
            if ref.hasPrefix(prefix) {
                return String(ref.dropFirst(prefix.count))
            }
        }
        if let slash = ref.firstIndex(of: "/") {
            return String(ref[ref.index(after: slash)...])
        }
        return ref
    }

    private static func blocksBranchSwitch(_ text: String) -> Bool {
        let lowered = text.lowercased()
        return lowered.contains("would be overwritten")
            || lowered.contains("local changes")
            || lowered.contains("please commit your changes or stash")
    }

    private var nonInteractiveGit: [String: String] {
        [
            "GIT_MERGE_AUTOEDIT": "no",
            "GIT_EDITOR": "/usr/bin/true",
            "GIT_SEQUENCE_EDITOR": "/usr/bin/true",
        ]
    }

    private func applyHunk(
        _ hunk: DiffHunk,
        path: String,
        scope: DiffScope,
        reverse: Bool,
        in repository: Repository
    ) async throws {
        try PathSafety.validateGitPath(path)
        var intentToAdd = false
        if !reverse, case .unstaged = scope {
            let current = try await status(in: repository, includeIgnored: false)
            if current.entries.contains(where: { $0.path == path && $0.isUntracked }) {
                // Sin esto, `git diff` no ve el archivo y el hunk no existe.
                try await run(in: repository, arguments: ["add", "-N", "--", path])
                intentToAdd = true
            }
        }
        do {
            try Task.checkCancellation()
            let files = try await diff(scope: scope, paths: [path], in: repository)
            guard let file = files.first(where: { $0.path == path || $0.oldPath == path }) else {
                throw GitError.hunkNotFound(path: path)
            }
            guard let match = file.hunks.first(where: { $0.matchesRange(of: hunk) }) else {
                throw GitError.hunkNotFound(path: path)
            }
            var arguments = ["apply", "--whitespace=nowarn", "--cached"]
            if reverse { arguments.append("--reverse") }
            arguments.append("-")
            try await run(in: repository, arguments: arguments, stdin: Data(file.patch(selecting: match).utf8))
        } catch {
            if intentToAdd {
                await undoIntentToAdd(path: path, in: repository)
            }
            throw error
        }
    }

    /// Quita un `git add -N` si el stage del hunk no llegó a completarse.
    private func undoIntentToAdd(path: String, in repository: Repository) async {
        do {
            try await run(in: repository, arguments: ["restore", "--staged", "--", path], allowNonZero: true)
            let status = try await self.status(in: repository, includeIgnored: false)
            let stillIndexed = status.entries.contains { $0.path == path && !$0.isUntracked }
            if stillIndexed {
                try await run(in: repository, arguments: ["reset", "-q", "--", path], allowNonZero: true)
            }
        } catch {
            // El índice puede quedar con el intent-to-add si git no responde; no tapamos el error original.
        }
    }

    private func mutatePaths(_ prefix: [String], paths: [String], in repository: Repository) async throws {
        guard !paths.isEmpty else { return }
        for path in paths {
            try PathSafety.validateGitPath(path)
        }
        try await run(in: repository, arguments: prefix + paths)
    }

    private func classifyMerge(_ output: ProcessOutput, arguments: [String], in repository: Repository) async throws -> MergeResult {
        let text = output.stdoutString + "\n" + output.stderrString
        if output.exitCode == 0 {
            if text.localizedCaseInsensitiveContains("Already up to date") {
                return .upToDate
            }
            let head = try await requireHEAD(in: repository)
            if text.contains("Fast-forward") {
                return .fastForward(newHEAD: head)
            }
            return .merged(newHEAD: head)
        }
        let current = try await status(in: repository, includeIgnored: false)
        let paths = current.conflicted.map(\.path).sorted()
        if !paths.isEmpty
            || text.contains("CONFLICT")
            || text.contains("Automatic merge failed")
            || text.contains("fix conflicts")
            || text.contains("could not apply")
        {
            return .conflicts(paths: paths)
        }
        throw failed(output, arguments: arguments)
    }

    private func requireRevision(_ revision: String, in repository: Repository) async throws -> String {
        let output = try await run(in: repository, arguments: ["rev-parse", revision])
        return trim(output.stdoutString)
    }

    private func requireHEAD(in repository: Repository) async throws -> String {
        let output = try await run(in: repository, arguments: ["rev-parse", "HEAD"], allowNonZero: true)
        guard output.exitCode == 0 else {
            throw failed(output, arguments: ["rev-parse", "HEAD"])
        }
        return trim(output.stdoutString)
    }

    private func refExists(_ name: String, in repository: Repository) async throws -> Bool {
        let output = try await run(
            in: repository,
            arguments: ["rev-parse", "-q", "--verify", name],
            allowNonZero: true
        )
        return output.exitCode == 0
    }

    private func gitPathIsDirectory(_ name: String, in repository: Repository) async throws -> Bool {
        let output = try await run(
            in: repository,
            arguments: ["rev-parse", "--git-path", name],
            allowNonZero: true
        )
        guard output.exitCode == 0 else { return false }
        let raw = trim(output.stdoutString)
        let base = URL(fileURLWithPath: repository.path, isDirectory: true)
        let url = URL(fileURLWithPath: raw, relativeTo: base).standardizedFileURL
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) && isDirectory.boolValue
    }

    private func side(oid: String?, in repository: Repository) async throws -> ConflictSide {
        guard let oid else { return .absent }
        let output = try await run(in: repository, arguments: ["cat-file", "blob", oid], allowNonZero: true)
        guard output.exitCode == 0 else {
            return ConflictSide(oid: oid, content: nil, isBinary: false)
        }
        if output.stdout.contains(0) {
            return ConflictSide(oid: oid, content: nil, isBinary: true)
        }
        return ConflictSide(oid: oid, content: output.stdoutString, isBinary: false)
    }

    private func readWorkingTree(path: String, in repository: Repository) throws -> (text: String?, hunks: [ConflictHunk]) {
        let url = try PathSafety.resolve(relativePath: path, repoRoot: repository.path)
        guard FileManager.default.fileExists(atPath: url.path) else {
            return (nil, [])
        }
        let data = try Data(contentsOf: url)
        if data.contains(0) {
            return (nil, [])
        }
        let text = String(decoding: data, as: UTF8.self)
        return (text, ConflictMarkers.hunks(in: text))
    }

    private func inferredKind(_ stages: [Int: String]) -> ConflictKind {
        let hasBase = stages[1] != nil
        let hasOurs = stages[2] != nil
        let hasTheirs = stages[3] != nil
        switch (hasBase, hasOurs, hasTheirs) {
        case (false, true, true):
            return .bothAdded
        case (_, false, true):
            return .deletedByUs
        case (_, true, false):
            return .deletedByThem
        case (_, false, false):
            return .bothDeleted
        default:
            return .bothModified
        }
    }

    @discardableResult
    private func run(
        in repository: Repository,
        arguments: [String],
        stdin: Data? = nil,
        extraEnvironment: [String: String] = [:],
        allowNonZero: Bool = false
    ) async throws -> ProcessOutput {
        try await run(
            in: URL(fileURLWithPath: repository.path, isDirectory: true),
            arguments: arguments,
            stdin: stdin,
            extraEnvironment: extraEnvironment,
            allowNonZero: allowNonZero
        )
    }

    @discardableResult
    private func run(
        in directory: URL,
        arguments: [String],
        stdin: Data? = nil,
        extraEnvironment: [String: String] = [:],
        allowNonZero: Bool = false
    ) async throws -> ProcessOutput {
        var environment = self.environment
        for (key, value) in extraEnvironment {
            environment[key] = value
        }
        let fullArguments = ["-c", "core.quotepath=false"] + arguments
        let request = ProcessRequest(
            executable: gitExecutable,
            arguments: fullArguments,
            currentDirectory: directory,
            environment: environment,
            stdin: stdin,
            timeout: timeout
        )
        let output: ProcessOutput
        do {
            let runner = self.runner
            let mutating = Self.isGitMutation(arguments)
            if mutating {
                output = try await gate.write {
                    try await runner.run(request)
                }
            } else {
                output = try await gate.read {
                    try await runner.run(request)
                }
            }
        } catch {
            if error is CancellationError {
                throw GitError.cancelled
            }
            throw map(error, arguments: fullArguments)
        }
        if !allowNonZero && output.exitCode != 0 {
            throw failed(output, arguments: fullArguments)
        }
        return output
    }

    private func map(_ error: Error, arguments: [String]) -> Error {
        guard let processError = error as? ProcessError else { return error }
        let command = commandLine(arguments)
        switch processError {
        case .executableNotFound(let binary):
            return GitError.gitNotFound(binary)
        case .timedOut:
            return GitError.timeout(command: command)
        case .cancelled:
            return GitError.cancelled
        case .failedToLaunch(let message):
            return GitError.commandFailed(command: command, exitCode: -1, stderr: message)
        }
    }

    private func failed(_ output: ProcessOutput, arguments: [String]) -> GitError {
        GitError.commandFailed(
            command: commandLine(arguments),
            exitCode: output.exitCode,
            stderr: output.stderrString.isEmpty ? output.stdoutString : output.stderrString
        )
    }

    private func commandLine(_ arguments: [String]) -> String {
        (["git"] + arguments).joined(separator: " ")
    }

    private func trim(_ text: String) -> String {
        text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

enum UnmergedIndexParser {
    struct Stage: Equatable {
        let oid: String
        let stage: Int
        let path: String
    }

    /// `git ls-files -u -z`: `modo SP oid SP etapa TAB path NUL`.
    static func parse(_ text: String) -> [Stage] {
        var segments = text.split(separator: "\0", omittingEmptySubsequences: false).map(String.init)
        if segments.last?.isEmpty == true {
            segments.removeLast()
        }
        var stages: [Stage] = []
        for segment in segments where !segment.isEmpty {
            guard let tab = segment.firstIndex(of: "\t") else { continue }
            let meta = segment[..<tab].split(separator: " ")
            guard meta.count == 3, let stage = Int(meta[2]) else { continue }
            let path = String(segment[segment.index(after: tab)...])
            stages.append(Stage(oid: String(meta[1]), stage: stage, path: path))
        }
        return stages
    }
}
