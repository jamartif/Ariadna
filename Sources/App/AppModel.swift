#if os(macOS)
import AppKit
import Observation
import SwiftUI
import GitCore
#if ARIADNA_AI
import AriadnaAI
#endif

/// Sección central del workspace.
enum WorkspaceSection: String, CaseIterable, Identifiable {
    case changes
    case history
    case conflicts

    var id: String { rawValue }

    var title: String {
        switch self {
        case .changes: return "Cambios"
        case .history: return "Historial"
        case .conflicts: return "Conflictos"
        }
    }
}

#if ARIADNA_AI
enum PendingAIAction: Equatable {
    case commitMessage
    case summarize
    case explain
    case conflict(String)
}
#endif

/// Estado de la ventana.
///
/// `@MainActor` es el hilo de la UI (como el `SynchronizationContext` de WPF).
/// `@Observable` avisa a SwiftUI cuando cambia una propiedad (como `INotifyPropertyChanged`).
@MainActor
@Observable
final class AppModel {
    var settings: AppSettings
    var recents: [RecentRepository] = []
    var repository: Repository?
    var status: GitStatus = .empty
    var branches: [Branch] = []
    var remotes: [Remote] = []
    var conflicts: [ConflictFile] = []
    var integration: IntegrationState = .none
    var commits: [Commit] = []
    var selectedPath: String?
    var selectedCommitSHA: String?
    var stagedDiff: [FileDiff] = []
    var unstagedDiff: [FileDiff] = []
    var commitDiff: [FileDiff] = []
    var commitMessage = ""
    var commitTemplate = ""
    #if ARIADNA_AI
    var aiSettings: AISettings = .default
    var suggestion: AISuggestion?
    var suggestionDraft = ""
    var showSuggestion = false
    var pendingAction: PendingAIAction?
    #endif
    var errorMessage: String?
    var infoMessage: String?
    var isBusy = false
    var includeIgnored = false
    var section: WorkspaceSection = .changes
    var conflictResults: [String: String] = [:]
    var newBranchName = ""
    var pathDraft = ""
    var branchPendingDelete: String?
    var pathPendingWrite: String?
    var pathsPendingRestore: [String]?
    var confirmAbort = false
    var confirmDetachedCommit = false

    private let runner: FoundationProcessRunner
    private var engine: GitCLIEngine
    private var session: RepositorySession?
    private let settingsStore: AppSettingsStore
    private let recentsStore: RecentRepositoryStore
    #if ARIADNA_AI
    private let aiStore = AISettingsStore()
    private var orchestrator: AIOrchestrator
    #endif

    init() {
        let runner = FoundationProcessRunner()
        let settingsStore = AppSettingsStore()
        let loaded = (try? settingsStore.load()) ?? .default
        self.runner = runner
        self.settingsStore = settingsStore
        self.settings = loaded
        self.recentsStore = RecentRepositoryStore(fileURL: AriadnaPaths.recentsURL)
        self.recents = (try? self.recentsStore.load()) ?? []
        self.engine = GitCLIEngine(gitExecutable: loaded.gitBinary)
        #if ARIADNA_AI
        let ai = (try? aiStore.load()) ?? .default
        self.aiSettings = ai
        self.orchestrator = Self.makeOrchestrator(settings: ai, runner: runner)
        #endif
    }

    var repositoryName: String { repository?.name ?? AriadnaIdentity.productName }

    var integrationLabel: String {
        switch integration {
        case .none: return ""
        case .merging: return "Hay un merge en curso."
        case .rebasing: return "Hay un rebase en curso."
        case .cherryPicking: return "Hay un cherry-pick en curso."
        }
    }

    func revealWelcome() {
        repository = nil
        session = nil
    }

    func open(path: String) async {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            errorMessage = "Indica la carpeta del repositorio."
            return
        }
        await perform {
            let repo = try await self.engine.openRepository(at: trimmed)
            let session = RepositorySession(repository: repo, engine: self.engine)
            try await session.refresh(includeIgnored: self.includeIgnored)
            self.session = session
            self.repository = repo
            try await self.pull(from: session)
            try? self.recentsStore.remember(repo, branch: self.status.branchName)
            self.recents = (try? self.recentsStore.load()) ?? self.recents
            self.section = self.conflicts.isEmpty ? .changes : .conflicts
            self.pathDraft = repo.path
            await self.reloadSelectedDiff()
        }
    }

    func refresh() async {
        await perform {
            guard let session = self.session else { return }
            try await session.refresh(includeIgnored: self.includeIgnored)
            try await self.pull(from: session)
            await self.reloadSelectedDiff()
        }
    }

    func stage(paths: [String]) async {
        await perform {
            guard let session = self.session else { return }
            try await session.stage(paths: paths)
            try await self.pull(from: session)
            await self.reloadSelectedDiff()
        }
    }

    func unstage(paths: [String]) async {
        await perform {
            guard let session = self.session else { return }
            try await session.unstage(paths: paths)
            try await self.pull(from: session)
            await self.reloadSelectedDiff()
        }
    }

    func stageHunk(_ hunk: DiffHunk, path: String) async {
        await perform {
            guard let session = self.session else { return }
            try await session.stageHunk(hunk, path: path)
            try await self.pull(from: session)
            await self.reloadSelectedDiff()
        }
    }

    func unstageHunk(_ hunk: DiffHunk, path: String) async {
        await perform {
            guard let session = self.session else { return }
            try await session.unstageHunk(hunk, path: path)
            try await self.pull(from: session)
            await self.reloadSelectedDiff()
        }
    }

    func commit() async {
        if status.isDetached {
            confirmDetachedCommit = true
            return
        }
        await commitNow(allowDetached: false)
    }

    func commitDetached() async {
        await commitNow(allowDetached: true)
    }

    private func commitNow(allowDetached: Bool) async {
        await perform {
            guard let session = self.session else { return }
            _ = try await session.commit(message: self.commitMessage, allowDetached: allowDetached)
            self.commitMessage = ""
            try await self.pull(from: session)
            self.commits = try await session.log(limit: 200)
            await self.reloadSelectedDiff()
            self.infoMessage = allowDetached
                ? "Commit creado fuera de una rama. No se ha hecho push."
                : "Commit creado. No se ha hecho push."
        }
    }

    func createBranch() async {
        let name = newBranchName.trimmingCharacters(in: .whitespacesAndNewlines)
        await perform {
            guard let session = self.session else { return }
            try await session.createBranch(name: name)
            try await session.checkout(ref: name)
            self.newBranchName = ""
            try await self.pull(from: session)
        }
    }

    func checkout(ref: String) async {
        await perform {
            guard let session = self.session else { return }
            try await session.checkout(ref: ref)
            try await self.pull(from: session)
            await self.reloadSelectedDiff()
        }
    }

    func deleteBranch(name: String, force: Bool) async {
        if branches.first(where: { $0.name == name })?.isHead == true {
            errorMessage = "No puedo borrar la rama en la que estás. Cambia a otra antes."
            return
        }
        await perform {
            guard let session = self.session else { return }
            try await session.deleteBranch(name: name, force: force)
            try await self.pull(from: session)
        }
    }

    func merge(branch: String) async {
        await perform {
            guard let session = self.session else { return }
            let result = try await session.merge(branch: branch, fastForward: .allow)
            try await self.pull(from: session)
            self.describe(result)
            if !self.conflicts.isEmpty { self.section = .conflicts }
        }
    }

    func abortIntegration() async {
        await perform {
            guard let session = self.session else { return }
            try await session.abortIntegration()
            try await self.pull(from: session)
            self.conflictResults = [:]
            self.infoMessage = "Operación abortada. El árbol vuelve al estado anterior."
        }
    }

    func continueIntegration() async {
        await perform {
            guard let session = self.session else { return }
            try await session.continueIntegration()
            try await self.pull(from: session)
            self.commits = try await session.log(limit: 200)
            self.infoMessage = "Operación continuada."
        }
    }

    func fetch() async {
        await perform {
            guard let session = self.session else { return }
            guard let remote = self.remotes.first?.name else {
                self.errorMessage = "Este repositorio no tiene remotos."
                return
            }
            try await session.fetch(remote: remote)
            try await self.pull(from: session)
            self.infoMessage = "Fetch de \(remote) terminado."
        }
    }

    func pull() async {
        await perform {
            guard let session = self.session else { return }
            guard let remote = self.remotes.first?.name else {
                self.errorMessage = "Este repositorio no tiene remotos."
                return
            }
            let result = try await session.pull(remote: remote, refspec: nil, rebase: self.settings.pullUsesRebase)
            try await self.pull(from: session)
            self.describe(result)
        }
    }

    func push() async {
        await perform {
            guard let session = self.session else { return }
            guard let remote = self.remotes.first?.name else {
                self.errorMessage = "Este repositorio no tiene remotos."
                return
            }
            guard let branch = self.status.branchName else {
                self.errorMessage = "HEAD está detached. Crea o elige una rama antes de hacer push."
                return
            }
            let needsUpstream = self.branches.first(where: \.isHead)?.upstream == nil
            try await session.push(remote: remote, refspec: branch, setUpstream: needsUpstream)
            try await self.pull(from: session)
            self.infoMessage = "Push de \(branch) a \(remote) terminado."
        }
    }

    func loadHistory() async {
        await perform {
            guard let session = self.session else { return }
            self.commits = try await session.log(limit: 200)
        }
    }

    func selectCommit(_ sha: String) async {
        selectedCommitSHA = sha
        await perform {
            guard let session = self.session else { return }
            self.commitDiff = try await session.diff(scope: .commit(sha))
        }
    }

    func reloadSelectedDiff() async {
        guard let session, let selectedPath else {
            stagedDiff = []
            unstagedDiff = []
            return
        }
        stagedDiff = (try? await session.diff(scope: .staged, paths: [selectedPath])) ?? []
        unstagedDiff = (try? await session.diff(scope: .unstaged, paths: [selectedPath])) ?? []
    }

    #if ARIADNA_AI
    func generateCommitMessage() async {
        pendingAction = .commitMessage
        await perform {
            guard let session = self.session else { return }
            let files = try await session.diff(scope: .staged)
            let diff = files.map(\.raw).joined(separator: "\n")
            if diff.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                self.errorMessage = "No hay cambios preparados. Prepara archivos o hunks y vuelve a generar."
                return
            }
            let template = self.commitTemplate.trimmingCharacters(in: .whitespacesAndNewlines)
            let suggestion = try await self.orchestrator.suggestCommitMessage(
                diff: diff,
                template: template.isEmpty ? nil : template
            )
            self.present(suggestion)
        }
    }

    func summarizeStaged() async {
        pendingAction = .summarize
        await perform {
            guard let session = self.session else { return }
            let files = try await session.diff(scope: .staged)
            let diff = files.map(\.raw).joined(separator: "\n")
            if diff.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                self.errorMessage = "No hay cambios preparados para resumir."
                return
            }
            let suggestion = try await self.orchestrator.summarizeDiff(diff, style: .pullRequest)
            self.present(suggestion)
        }
    }

    func explainSelectedCommit() async {
        pendingAction = .explain
        await perform {
            guard let session = self.session, let sha = self.selectedCommitSHA else { return }
            guard let commit = self.commits.first(where: { $0.sha == sha }) else { return }
            let files = try await session.diff(scope: .commit(sha))
            let patch = files.map(\.raw).joined(separator: "\n")
            let suggestion = try await self.orchestrator.explainCommit(sha: sha, patch: patch, subject: commit.subject)
            self.present(suggestion)
        }
    }

    func proposeResolution(path: String) async {
        pendingAction = .conflict(path)
        await perform {
            guard let conflict = self.conflicts.first(where: { $0.path == path }) else { return }
            let suggestion = try await self.orchestrator.proposeConflictResolution(conflict)
            self.present(suggestion)
        }
    }

    func regenerate() async {
        switch pendingAction {
        case .commitMessage: await generateCommitMessage()
        case .summarize: await summarizeStaged()
        case .explain: await explainSelectedCommit()
        case .conflict(let path): await proposeResolution(path: path)
        case nil: break
        }
    }

    func useCommitSuggestion() async {
        guard let suggestion else { return }
        let accepted = await orchestrator.accept(suggestion)
        commitMessage = suggestionDraft
        self.suggestion = accepted
        showSuggestion = false
        infoMessage = "Mensaje colocado en el cuadro. El commit no se crea hasta que pulses Crear commit."
    }

    func applySuggestionToEditor() async {
        guard let suggestion, let content = suggestion.proposedContent else { return }
        let path = suggestion.subject ?? selectedPath
        guard let path else { return }
        let accepted = await orchestrator.accept(suggestion)
        conflictResults[path] = content
        self.suggestion = accepted
        showSuggestion = false
        infoMessage = "La propuesta está en el editor. El archivo en disco no ha cambiado."
    }

    func discardSuggestion() async {
        guard let suggestion else {
            showSuggestion = false
            return
        }
        _ = await orchestrator.reject(suggestion)
        self.suggestion = nil
        showSuggestion = false
    }

    func copyDraft() {
        let text = suggestionDraft
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        infoMessage = "Copiado. El repositorio no ha cambiado."
    }
    #endif

    func selectConflict(_ path: String) {
        selectedPath = path
        guard conflictResults[path] == nil, let file = conflicts.first(where: { $0.path == path }) else { return }
        conflictResults[path] = file.workingTree ?? file.takingWholeSide(.ours)
    }

    func takeSide(_ choice: ConflictChoice, path: String) {
        guard let file = conflicts.first(where: { $0.path == path }) else { return }
        conflictResults[path] = file.takingWholeSide(choice)
    }

    func takeHunk(_ id: Int, choice: ConflictChoice, path: String) {
        guard let file = conflicts.first(where: { $0.path == path }), let working = file.workingTree else { return }
        let base = conflictResults[path] ?? working
        let source = base.contains("<<<<<<<") ? base : working
        conflictResults[path] = ConflictMarkers.resolve(source, choices: [id: choice])
    }

    func writeResult(path: String) async {
        await perform {
            guard let session = self.session else { return }
            let contents = self.conflictResults[path] ?? ""
            try await session.writeResolution(path: path, contents: contents, confirmed: true)
            try await self.pull(from: session)
            self.infoMessage = "Archivo escrito. Márcalo como resuelto cuando el contenido sea el definitivo. Aún no hay commit."
        }
    }

    func markResolved(path: String) async {
        await perform {
            guard let session = self.session else { return }
            try await session.markResolved(path: path)
            try await self.pull(from: session)
            self.infoMessage = "Archivo preparado. Si no quedan conflictos, puedes continuar el merge."
        }
    }

    func restore(paths: [String]) async {
        await perform {
            guard let session = self.session else { return }
            try await session.restoreWorktree(paths: paths, confirmed: true)
            try await self.pull(from: session)
            await self.reloadSelectedDiff()
        }
    }

    func toggleFavorite(_ path: String) {
        let current = recents.first(where: { $0.path == path })?.isFavorite ?? false
        try? recentsStore.setFavorite(path: path, isFavorite: !current)
        recents = (try? recentsStore.load()) ?? recents
    }

    func saveSettings() async {
        await perform {
            try self.settingsStore.save(self.settings)
            self.engine = GitCLIEngine(gitExecutable: self.settings.gitBinary)
            #if ARIADNA_AI
            try self.aiStore.save(self.aiSettings)
            self.orchestrator = Self.makeOrchestrator(settings: self.aiSettings, runner: self.runner)
            #endif
            if let path = self.repository?.path {
                let repo = try await self.engine.openRepository(at: path)
                let session = RepositorySession(repository: repo, engine: self.engine)
                try await session.refresh(includeIgnored: self.includeIgnored)
                self.session = session
                self.repository = repo
                try await self.pull(from: session)
            }
            self.infoMessage = "Ajustes guardados."
        }
    }

    func browseAndFillPath() {
        if let path = FolderPicker.choose() {
            pathDraft = path
        }
    }

    #if ARIADNA_AI
    private func present(_ suggestion: AISuggestion) {
        self.suggestion = suggestion
        switch suggestion.kind {
        case .commitMessage:
            suggestionDraft = suggestion.commitMessageText
        case .conflictResolution:
            suggestionDraft = suggestion.proposedContent ?? suggestion.body
        default:
            suggestionDraft = suggestion.body
        }
        showSuggestion = true
    }
    #endif

    private func describe(_ result: MergeResult) {
        switch result {
        case .upToDate:
            infoMessage = "Ya estaba al día."
        case .fastForward:
            infoMessage = "Fast-forward hecho."
        case .merged:
            infoMessage = "Merge commit creado."
        case .conflicts(let paths):
            infoMessage = "Hay conflictos en \(paths.count) archivo(s). Nada se ha resuelto solo."
            section = .conflicts
        }
    }

    private func pull(from session: RepositorySession) async throws {
        status = await session.status
        branches = await session.branches
        remotes = await session.remotes
        conflicts = await session.conflicts
        integration = await session.integration
        if let path = repository?.path {
            try? recentsStore.remember(Repository(path: path, gitDir: repository?.gitDir ?? ""), branch: status.branchName)
        }
    }

    private func perform(_ work: () async throws -> Void) async {
        if isBusy { return }
        isBusy = true
        defer { isBusy = false }
        do {
            try await work()
        } catch is CancellationError {
            infoMessage = "Operación cancelada."
        } catch let error as GitError where error == .cancelled {
            infoMessage = "Operación cancelada."
        } catch {
            errorMessage = (error as? LocalizedError)?.errorDescription ?? String(describing: error)
        }
    }

    #if ARIADNA_AI
    private static func makeOrchestrator(settings: AISettings, runner: any ProcessRunning) -> AIOrchestrator {
        let policy = AIContextPolicy(maxBytes: settings.maxContextBytes)
        let copilot = CopilotCLIProvider(runner: runner, binary: settings.copilotBinary, policy: policy)
        let codex = CodexExecProvider(runner: runner, binary: settings.codexBinary, policy: policy)
        return AIOrchestrator(choice: settings.provider, copilot: copilot, codex: codex)
    }
    #endif
}

enum FolderPicker {
    static func choose() -> String? {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = false
        panel.prompt = "Abrir"
        panel.message = "Elige la carpeta de un repositorio Git"
        guard panel.runModal() == .OK else { return nil }
        return panel.url?.path
    }
}
#endif
