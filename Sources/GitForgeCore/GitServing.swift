import Foundation

/// Contrato del motor Git.
///
/// En C# esto sería una `interface`. La app y los tests dependen de este
/// contrato, no de `Foundation.Process` directamente.
///
/// La implementación de este MVP es `GitCLIEngine`: ejecuta el `git` del
/// sistema. Así el comportamiento (hooks, merge, credenciales) es el mismo
/// que en la terminal. No enlazamos libgit2.
public protocol GitServing: Sendable {
    func openRepository(at path: String) async throws -> Repository
    func status(in repository: Repository, includeIgnored: Bool) async throws -> GitStatus
    func stage(paths: [String], in repository: Repository) async throws
    func unstage(paths: [String], in repository: Repository) async throws
    func stageHunk(_ hunk: DiffHunk, path: String, in repository: Repository) async throws
    func unstageHunk(_ hunk: DiffHunk, path: String, in repository: Repository) async throws
    func commit(message: String, in repository: Repository) async throws -> String
    func log(in repository: Repository, limit: Int) async throws -> [Commit]
    func diff(scope: DiffScope, paths: [String], in repository: Repository) async throws -> [FileDiff]
    func branches(in repository: Repository) async throws -> [Branch]
    func createBranch(name: String, from startPoint: String?, in repository: Repository) async throws
    func renameBranch(from oldName: String, to newName: String, in repository: Repository) async throws
    func checkout(ref: String, in repository: Repository) async throws
    func deleteBranch(name: String, force: Bool, in repository: Repository) async throws
    func merge(branch: String, fastForward: MergeFastForward, in repository: Repository) async throws -> MergeResult
    func integrationState(in repository: Repository) async throws -> IntegrationState
    func abortIntegration(in repository: Repository) async throws
    func continueIntegration(in repository: Repository) async throws
    func conflicts(in repository: Repository) async throws -> [ConflictFile]
    /// Escribe el archivo de trabajo solo si `confirmed` es `true`. No hace `git add`.
    func writeWorkingTreeFile(relativePath: String, contents: String, in repository: Repository, confirmed: Bool) async throws
    func markResolved(path: String, in repository: Repository) async throws
    func restoreWorktree(paths: [String], in repository: Repository, confirmed: Bool) async throws
    func listRemotes(in repository: Repository) async throws -> [Remote]
    func addRemote(name: String, url: String, in repository: Repository) async throws
    func fetch(remote: String, in repository: Repository) async throws
    func pull(remote: String, refspec: String?, rebase: Bool, in repository: Repository) async throws -> MergeResult
    func push(remote: String, refspec: String?, setUpstream: Bool, in repository: Repository) async throws
}
