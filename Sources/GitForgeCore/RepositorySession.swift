import Foundation

/// Sesión de un repo abierto. La UI llama aquí, no arma comandos git.
///
/// Otro `actor`: el estado (`status`, `branches`…) solo se lee con `await`,
/// parecido a no tocar un objeto de UI desde un hilo de fondo.
public actor RepositorySession {
    public let repository: Repository
    private let engine: any GitServing

    public private(set) var status: GitStatus
    public private(set) var branches: [Branch]
    public private(set) var remotes: [Remote]
    public private(set) var conflicts: [ConflictFile]
    public private(set) var integration: IntegrationState

    public init(repository: Repository, engine: any GitServing) {
        self.repository = repository
        self.engine = engine
        self.status = .empty
        self.branches = []
        self.remotes = []
        self.conflicts = []
        self.integration = .none
    }

    public func refresh(includeIgnored: Bool = false) async throws {
        status = try await engine.status(in: repository, includeIgnored: includeIgnored)
        branches = try await engine.branches(in: repository)
        remotes = try await engine.listRemotes(in: repository)
        integration = try await engine.integrationState(in: repository)
        conflicts = try await engine.conflicts(in: repository)
    }

    public func stage(paths: [String]) async throws {
        try await engine.stage(paths: paths, in: repository)
        try await refresh()
    }

    public func unstage(paths: [String]) async throws {
        try await engine.unstage(paths: paths, in: repository)
        try await refresh()
    }

    public func stageHunk(_ hunk: DiffHunk, path: String) async throws {
        try await engine.stageHunk(hunk, path: path, in: repository)
        try await refresh()
    }

    public func unstageHunk(_ hunk: DiffHunk, path: String) async throws {
        try await engine.unstageHunk(hunk, path: path, in: repository)
        try await refresh()
    }

    public func commit(message: String) async throws -> String {
        let sha = try await engine.commit(message: message, in: repository)
        try await refresh()
        return sha
    }

    public func createBranch(name: String, from startPoint: String? = nil) async throws {
        try await engine.createBranch(name: name, from: startPoint, in: repository)
        try await refresh()
    }

    public func renameBranch(from oldName: String, to newName: String) async throws {
        try await engine.renameBranch(from: oldName, to: newName, in: repository)
        try await refresh()
    }

    public func checkout(ref: String) async throws {
        try await engine.checkout(ref: ref, in: repository)
        try await refresh()
    }

    public func deleteBranch(name: String, force: Bool) async throws {
        try await engine.deleteBranch(name: name, force: force, in: repository)
        try await refresh()
    }

    public func merge(branch: String, fastForward: MergeFastForward = .allow) async throws -> MergeResult {
        let result = try await engine.merge(branch: branch, fastForward: fastForward, in: repository)
        try await refresh()
        return result
    }

    public func abortIntegration() async throws {
        try await engine.abortIntegration(in: repository)
        try await refresh()
    }

    public func continueIntegration() async throws {
        try await engine.continueIntegration(in: repository)
        try await refresh()
    }

    public func diff(scope: DiffScope, paths: [String] = []) async throws -> [FileDiff] {
        try await engine.diff(scope: scope, paths: paths, in: repository)
    }

    public func log(limit: Int = 200) async throws -> [Commit] {
        try await engine.log(in: repository, limit: limit)
    }

    /// Escribe el resultado en el archivo solo con confirmación. No lo prepara.
    public func writeResolution(path: String, contents: String, confirmed: Bool) async throws {
        try await engine.writeWorkingTreeFile(
            relativePath: path,
            contents: contents,
            in: repository,
            confirmed: confirmed
        )
        if confirmed {
            try await refresh()
        }
    }

    public func markResolved(path: String) async throws {
        try await engine.markResolved(path: path, in: repository)
        try await refresh()
    }

    public func restoreWorktree(paths: [String], confirmed: Bool) async throws {
        try await engine.restoreWorktree(paths: paths, in: repository, confirmed: confirmed)
        if confirmed {
            try await refresh()
        }
    }

    public func fetch(remote: String) async throws {
        try await engine.fetch(remote: remote, in: repository)
        try await refresh()
    }

    public func pull(remote: String, refspec: String? = nil, rebase: Bool = false) async throws -> MergeResult {
        let result = try await engine.pull(remote: remote, refspec: refspec, rebase: rebase, in: repository)
        try await refresh()
        return result
    }

    public func push(remote: String, refspec: String? = nil, setUpstream: Bool = false) async throws {
        try await engine.push(remote: remote, refspec: refspec, setUpstream: setUpstream, in: repository)
        try await refresh()
    }
}
