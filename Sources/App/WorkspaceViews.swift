#if os(macOS)
import AppKit
import SwiftUI
import GitCore

struct BranchSidebar: View {
    @Bindable var model: AppModel

    var body: some View {
        List(selection: Binding(
            get: { model.status.branchName },
            set: { name in
                guard let name, name != model.status.branchName else { return }
                Task { await model.checkout(ref: name) }
            }
        )) {
            Section {
                if model.branches.isEmpty {
                    Text("Sin ramas todavía").foregroundStyle(.secondary)
                }
                ForEach(model.branches) { branch in
                    BranchRow(branch: branch)
                        .tag(branch.name)
                        .contextMenu {
                            Button("Borrar…", systemImage: "trash") { model.branchPendingDelete = branch.name }
                                .disabled(branch.isHead)
                        }
                }
            } header: {
                SidebarHeader(title: "Ramas", count: model.branches.count)
            }
            Section {
                if model.remoteBranches.isEmpty {
                    Text("Sin ramas remotas").foregroundStyle(.secondary)
                }
                ForEach(model.remoteBranches) { branch in
                    Label(branch.name, systemImage: "cloud")
                        .lineLimit(1)
                        .tag(branch.name)
                        .help("Crea la rama local con seguimiento. Si ya existe y no coincide, Ariadna pregunta.")
                }
            } header: {
                SidebarHeader(title: "Seguimiento remoto", count: model.remoteBranches.count)
            }
            Section {
                if model.remotes.isEmpty {
                    Text("Sin remotos").foregroundStyle(.secondary)
                }
                ForEach(model.remotes) { remote in
                    Label {
                        VStack(alignment: .leading, spacing: 1) {
                            Text(remote.name)
                            Text(remote.fetchURL)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                                .truncationMode(.middle)
                                .help(remote.fetchURL)
                        }
                    } icon: {
                        Image(systemName: "network")
                    }
                }
            } header: {
                SidebarHeader(title: "Remotos", count: model.remotes.count)
            }
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .bottom) {
            VStack(alignment: .leading, spacing: 8) {
                if model.status.isDetached {
                    Label("HEAD detached", systemImage: "exclamationmark.triangle.fill")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.orange)
                }
                HStack(spacing: 6) {
                    TextField("Nueva rama", text: $model.newBranchName)
                        .textFieldStyle(.roundedBorder)
                        .onSubmit { Task { await model.createBranch() } }
                    Button {
                        Task { await model.createBranch() }
                    } label: {
                        Image(systemName: "plus")
                    }
                    .help("Crear la rama desde HEAD y cambiar a ella")
                    .disabled(model.isBusy || model.newBranchName.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                if model.branches.count > 1 {
                    Menu {
                        ForEach(model.branches.filter { !$0.isHead }) { branch in
                            Button(branch.name) { Task { await model.merge(branch: branch.name) } }
                        }
                    } label: {
                        Label("Fusionar en la rama actual…", systemImage: "arrow.triangle.merge")
                    }
                    .disabled(model.isBusy)
                }
            }
            .padding(10)
            .background(.bar)
        }
    }
}

private struct SidebarHeader: View {
    let title: String
    let count: Int

    var body: some View {
        HStack {
            Text(title)
            Spacer()
            if count > 0 {
                Text("\(count)")
                    .font(.caption2.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
        }
    }
}

private struct BranchRow: View {
    let branch: Branch

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: branch.isHead ? "checkmark.circle.fill" : "arrow.triangle.branch")
                .foregroundStyle(branch.isHead ? Color.accentColor : Color.secondary)
                .frame(width: 16)
                .accessibilityLabel(branch.isHead ? "Rama actual" : "Rama")
            Text(branch.name)
                .fontWeight(branch.isHead ? .semibold : .regular)
                .lineLimit(1)
            Spacer(minLength: 4)
            AheadBehind(ahead: branch.ahead, behind: branch.behind)
            if let upstream = branch.upstream {
                Text(upstream)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
    }
}

struct ChangesList: View {
    @Bindable var model: AppModel

    var body: some View {
        List(selection: $model.selectedPath) {
            changeSection(
                "Preparados",
                entries: model.status.staged,
                staged: true,
                empty: "Nada preparado",
                bulkTitle: "Quitar todo",
                bulk: { paths in Task { await model.unstage(paths: paths) } }
            )
            changeSection(
                "Sin preparar",
                entries: model.status.unstaged,
                staged: false,
                empty: "El árbol de trabajo está limpio",
                bulkTitle: "Preparar todo",
                bulk: { paths in Task { await model.stage(paths: paths) } }
            )
            if model.includeIgnored {
                changeSection("Ignorados", entries: model.status.ignored, staged: false, empty: "Sin ignorados", bulkTitle: nil, bulk: nil)
            }
        }
        .overlay {
            if model.status.entries.isEmpty && !model.isBusy {
                ContentUnavailableView("Todo al día", systemImage: "checkmark.seal", description: Text("No hay nada que preparar en este repositorio."))
            }
        }
    }

    @ViewBuilder
    private func changeSection(
        _ title: String,
        entries: [StatusEntry],
        staged: Bool,
        empty: String,
        bulkTitle: String?,
        bulk: (([String]) -> Void)?
    ) -> some View {
        Section {
            if entries.isEmpty {
                Text(empty).foregroundStyle(.secondary)
            }
            ForEach(entries) { entry in
                StatusRow(entry: entry, staged: staged, model: model)
                    .tag(entry.path)
            }
        } header: {
            HStack {
                Text(title)
                Text("\(entries.count)")
                    .font(.caption2.monospacedDigit())
                    .padding(.horizontal, 5)
                    .background(.quaternary, in: Capsule())
                Spacer()
                if let bulkTitle, let bulk, !entries.isEmpty {
                    Button(bulkTitle) { bulk(entries.map(\.path)) }
                        .buttonStyle(.borderless)
                        .font(.caption)
                        .disabled(model.isBusy)
                }
            }
        }
    }
}

struct StatusRow: View {
    let entry: StatusEntry
    let staged: Bool
    let model: AppModel

    var body: some View {
        HStack(spacing: 8) {
            StatusBadge(appearance: StatusAppearance(entry: entry, staged: staged))
            VStack(alignment: .leading, spacing: 1) {
                Text(DisplayFormat.fileName(entry.path))
                    .lineLimit(1)
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.head)
            }
            Spacer(minLength: 4)
            if !staged && (entry.isUntracked || entry.hasUnstagedChange) {
                Button {
                    Task { await model.stage(paths: [entry.path]) }
                } label: {
                    Image(systemName: "plus.circle.fill")
                        .foregroundStyle(.green)
                }
                .buttonStyle(.borderless)
                .help("Preparar el archivo")
                .disabled(model.isBusy)
            }
            if staged && entry.isStaged {
                Button {
                    Task { await model.unstage(paths: [entry.path]) }
                } label: {
                    Image(systemName: "minus.circle.fill")
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.borderless)
                .help("Quitar del índice")
                .disabled(model.isBusy)
            }
        }
        .padding(.vertical, 1)
        .contextMenu {
            Button("Copiar ruta", systemImage: "doc.on.doc") { DisplayFormat.copy(entry.path) }
            if entry.hasUnstagedChange && !entry.isUntracked {
                Button("Descartar cambios…", systemImage: "arrow.uturn.backward") { model.pathsPendingRestore = [entry.path] }
            }
        }
    }

    private var subtitle: String {
        if let original = entry.originalPath { return "desde \(original)" }
        let directory = DisplayFormat.directory(entry.path)
        return directory.isEmpty ? "raíz del repositorio" : directory
    }
}

struct CommitBar: View {
    @Bindable var model: AppModel

    private var subjectLength: Int {
        model.commitMessage.split(separator: "\n", omittingEmptySubsequences: false).first.map(\.count) ?? 0
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let warning = model.status.detachedCommitWarning {
                Label(warning, systemImage: "exclamationmark.triangle.fill")
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .accessibilityLabel("Aviso de HEAD detached")
            }
            HStack {
                #if ARIADNA_AI
                Text("La IA rellena el mensaje. El commit solo ocurre con el botón de abajo.")
                #else
                Text("El commit solo incluye lo que está preparado.")
                #endif
                Spacer()
                Text("Asunto \(subjectLength)/72")
                    .monospacedDigit()
                    .foregroundStyle(subjectLength > 72 ? Color.orange : Color.secondary)
                    .help("Se recomienda un asunto de 50 caracteres o menos, y nunca más de 72.")
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            TextEditor(text: $model.commitMessage)
                .font(.body)
                .scrollContentBackground(.hidden)
                .padding(4)
                .frame(minHeight: 72, maxHeight: 120)
                .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 6, style: .continuous).stroke(.quaternary))
                .accessibilityLabel("Mensaje de commit")
            Toggle(isOn: Binding(
                get: { model.amendLastCommit },
                set: { enabled in Task { await model.setAmend(enabled) } }
            )) {
                Text("Corregir el último commit (amend)")
                    .font(.caption)
            }
            .toggleStyle(.checkbox)
            .disabled(model.isBusy || model.status.isUnborn)
            .help("Rehace el último commit con este mensaje y lo preparado. Ariadna se niega si ese commit ya está en un remoto.")
            HStack {
                #if ARIADNA_AI
                Button("Generar mensaje", systemImage: "sparkles") { Task { await model.generateCommitMessage() } }
                    .disabled(model.isBusy)
                Button("Resumir", systemImage: "text.alignleft") { Task { await model.summarizeStaged() } }
                    .disabled(model.isBusy)
                #endif
                Spacer()
                Button {
                    Task { await model.commit() }
                } label: {
                    Label(
                        model.amendLastCommit ? "Corregir commit" : "Crear commit",
                        systemImage: model.amendLastCommit ? "pencil.circle" : "checkmark.circle"
                    )
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.return, modifiers: .command)
                .help("⌘↩")
                .disabled(
                    model.isBusy
                        || model.commitMessage.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        || (model.status.staged.isEmpty && !model.amendLastCommit)
                )
            }
            TextField("Plantilla opcional para el mensaje", text: $model.commitTemplate)
                .textFieldStyle(.roundedBorder)
                .font(.caption)
        }
        .padding(10)
        .background(.bar)
    }
}

struct ChangesDetail: View {
    let model: AppModel

    var body: some View {
        if model.selectedPath == nil {
            ContentUnavailableView("Elige un archivo", systemImage: "doc.text.magnifyingglass", description: Text("El diff de lo preparado y de lo que sigue en el trabajo aparece aquí."))
        } else {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    if !model.stagedDiff.isEmpty {
                        DiffSectionTitle(title: "Preparado", systemImage: "tray.full", color: .green)
                        PatchView(files: model.stagedDiff, onUnstage: { path, hunk in
                            Task { await model.unstageHunk(hunk, path: path) }
                        })
                    }
                    if !model.unstagedDiff.isEmpty {
                        DiffSectionTitle(title: "Sin preparar", systemImage: "pencil.line", color: .orange)
                        PatchView(files: model.unstagedDiff, onStage: { path, hunk in
                            Task { await model.stageHunk(hunk, path: path) }
                        })
                    }
                    if model.stagedDiff.isEmpty && model.unstagedDiff.isEmpty {
                        Text("Este archivo no tiene un diff de texto (puede ser nuevo y aún no rastreado, o binario).")
                            .foregroundStyle(.secondary)
                    }
                }
                .padding()
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}

private struct DiffSectionTitle: View {
    let title: String
    let systemImage: String
    let color: Color

    var body: some View {
        Label(title, systemImage: systemImage)
            .font(.headline)
            .foregroundStyle(color)
    }
}

struct PatchView: View {
    let files: [FileDiff]
    var onStage: ((String, DiffHunk) -> Void)?
    var onUnstage: ((String, DiffHunk) -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(files) { file in
                VStack(alignment: .leading, spacing: 0) {
                    fileHeader(file)
                    if file.isBinary {
                        Text("Archivo binario: no hay diff de texto.")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .padding(10)
                    }
                    ForEach(file.hunks) { hunk in
                        Divider()
                        hunkHeader(hunk, path: file.path)
                        VStack(alignment: .leading, spacing: 0) {
                            ForEach(hunk.lines) { line in
                                lineRow(line)
                            }
                        }
                        .padding(.vertical, 2)
                    }
                }
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                .card()
            }
        }
        .textSelection(.enabled)
    }

    private func fileHeader(_ file: FileDiff) -> some View {
        HStack(spacing: 6) {
            Image(systemName: icon(for: file))
                .foregroundStyle(.secondary)
            Text(DisplayFormat.fileName(file.path))
                .font(.subheadline.weight(.semibold))
            let directory = DisplayFormat.directory(file.path)
            if !directory.isEmpty {
                Text(directory)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.head)
            }
            if file.isNewFile { tag("nuevo", color: .green) }
            if file.isDeletedFile { tag("borrado", color: .red) }
            if let old = file.oldPath, old != file.path { tag("desde \(old)", color: .blue) }
            Spacer()
            let counts = lineCounts(file)
            if counts.added > 0 {
                Text("+\(counts.added)").foregroundStyle(.green)
            }
            if counts.removed > 0 {
                Text("−\(counts.removed)").foregroundStyle(.red)
            }
        }
        .font(.caption.monospacedDigit().weight(.semibold))
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
    }

    private func hunkHeader(_ hunk: DiffHunk, path: String) -> some View {
        HStack {
            Text("@@ −\(hunk.oldStart),\(hunk.oldCount) +\(hunk.newStart),\(hunk.newCount) @@")
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
            if !hunk.heading.isEmpty {
                Text(hunk.heading)
                    .font(.caption.monospaced())
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
            Spacer()
            if let onStage {
                Button("Preparar hunk", systemImage: "plus") { onStage(path, hunk) }
                    .controlSize(.small)
            }
            if let onUnstage {
                Button("Quitar hunk", systemImage: "minus") { onUnstage(path, hunk) }
                    .controlSize(.small)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 5)
        .background(Palette.hunkHeaderBackground)
    }

    private func lineRow(_ line: DiffLine) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 0) {
            Text(line.oldLineNumber.map { String($0) } ?? "")
                .frame(width: 38, alignment: .trailing)
                .foregroundStyle(.tertiary)
            Text(line.newLineNumber.map { String($0) } ?? "")
                .frame(width: 38, alignment: .trailing)
                .foregroundStyle(.tertiary)
            Text(mark(for: line.kind))
                .frame(width: 18)
                .foregroundStyle(color(for: line.kind))
            Text(line.text)
                .foregroundStyle(line.kind == .meta ? Color.secondary : Color.primary)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .font(.system(size: 11.5, design: .monospaced))
        .padding(.vertical, 0.5)
        .background(background(for: line.kind))
    }

    private func tag(_ text: String, color: Color) -> some View {
        Text(text)
            .font(.caption2.weight(.medium))
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .foregroundStyle(color)
            .background(color.opacity(0.14), in: Capsule())
    }

    private func lineCounts(_ file: FileDiff) -> (added: Int, removed: Int) {
        var added = 0
        var removed = 0
        for hunk in file.hunks {
            for line in hunk.lines {
                switch line.kind {
                case .addition: added += 1
                case .deletion: removed += 1
                case .context, .meta: break
                }
            }
        }
        return (added, removed)
    }

    private func icon(for file: FileDiff) -> String {
        if file.isBinary { return "doc.zipper" }
        if file.isNewFile { return "doc.badge.plus" }
        if file.isDeletedFile { return "minus.square" }
        return "doc.text"
    }

    private func mark(for kind: DiffLine.Kind) -> String {
        switch kind {
        case .addition: return "+"
        case .deletion: return "−"
        case .context: return " "
        case .meta: return ""
        }
    }

    private func color(for kind: DiffLine.Kind) -> Color {
        switch kind {
        case .addition: return .green
        case .deletion: return .red
        case .context: return .primary
        case .meta: return .secondary
        }
    }

    private func background(for kind: DiffLine.Kind) -> Color {
        switch kind {
        case .addition: return Palette.addedBackground
        case .deletion: return Palette.deletedBackground
        case .context, .meta: return .clear
        }
    }
}

struct HistoryList: View {
    @Bindable var model: AppModel
    @State private var query = ""

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                TextField("Filtrar por mensaje, autor o hash", text: $query)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit { Task { await model.searchHistory(query) } }
                Button {
                    Task { await model.searchHistory(query) }
                } label: {
                    Label("Buscar en todo", systemImage: "magnifyingglass")
                }
                .disabled(model.isBusy || query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .help("Busca en todo el historial de la rama, no solo en los commits cargados. También con Intro.")
            }
            .padding(8)
            if let search = model.historySearch {
                HStack {
                    Label("\(model.commits.count) resultado(s) para «\(search)»", systemImage: "line.3.horizontal.decrease.circle")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button("Ver los últimos") {
                        query = ""
                        Task { await model.loadHistory() }
                    }
                    .font(.caption)
                    .buttonStyle(.borderless)
                }
                .padding(.horizontal, 10)
                .padding(.bottom, 6)
            }
            List(filtered, selection: $model.selectedCommitSHA) { commit in
                CommitRow(commit: commit, remoteNames: model.remotes.map(\.name))
                    .tag(commit.sha)
            }
            .onChange(of: model.selectedCommitSHA) { _, sha in
                guard let sha else { return }
                Task { await model.selectCommit(sha) }
            }
        }
        .overlay {
            if model.commits.isEmpty && !model.isBusy, let search = model.historySearch {
                ContentUnavailableView.search(text: search)
            } else if model.commits.isEmpty && !model.isBusy {
                ContentUnavailableView("Sin historial cargado", systemImage: "clock", description: Text("Abre esta pestaña para leer los últimos commits. Un repo nuevo todavía no tiene ninguno."))
            }
        }
    }

    private var filtered: [Commit] {
        // Tras una búsqueda completa, la lista ya son los resultados: no se filtra otra vez.
        if model.historySearch != nil { return model.commits }
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !needle.isEmpty else { return model.commits }
        return model.commits.filter { commit in
            commit.subject.lowercased().contains(needle)
                || commit.body.lowercased().contains(needle)
                || commit.authorName.lowercased().contains(needle)
                || commit.sha.lowercased().contains(needle)
        }
    }
}

private struct CommitRow: View {
    let commit: Commit
    let remoteNames: [String]

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            AuthorAvatar(name: commit.authorName, size: 26)
            VStack(alignment: .leading, spacing: 3) {
                Text(commit.subject)
                    .fontWeight(.medium)
                    .lineLimit(2)
                if !commit.refs.isEmpty {
                    HStack(spacing: 4) {
                        ForEach(commit.refs.prefix(3), id: \.self) { ref in
                            RefChip(raw: ref, remoteNames: remoteNames)
                        }
                    }
                }
                HStack(spacing: 4) {
                    Text(commit.shortSHA).monospaced()
                    Text("·")
                    Text(commit.authorName).lineLimit(1)
                    Text("·")
                    Text(DisplayFormat.relative(commit.authorDateISO))
                        .help(DisplayFormat.absolute(commit.authorDateISO))
                    if commit.parents.count > 1 {
                        Image(systemName: "arrow.triangle.merge")
                            .help("Merge commit")
                    }
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 3)
    }
}

struct HistoryDetail: View {
    let model: AppModel

    var body: some View {
        if let sha = model.selectedCommitSHA, let commit = model.commits.first(where: { $0.sha == sha }) {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    Text(commit.subject)
                        .font(.title2.bold())
                        .textSelection(.enabled)
                    HStack(spacing: 10) {
                        AuthorAvatar(name: commit.authorName, size: 36)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(commit.authorName).font(.callout.weight(.semibold))
                            Text(commit.authorEmail).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        VStack(alignment: .trailing, spacing: 2) {
                            Text(DisplayFormat.relative(commit.authorDateISO)).font(.callout)
                            Text(DisplayFormat.absolute(commit.authorDateISO)).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    HStack(spacing: 6) {
                        Button {
                            DisplayFormat.copy(commit.sha)
                        } label: {
                            Label(commit.shortSHA, systemImage: "doc.on.doc")
                                .font(.caption.monospaced())
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        .help("Copiar el hash completo: \(commit.sha)")
                        ForEach(commit.refs, id: \.self) { ref in
                            RefChip(raw: ref, remoteNames: model.remotes.map(\.name))
                        }
                    }
                    if !commit.body.isEmpty {
                        Text(commit.body)
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(10)
                            .card()
                    }
                    #if ARIADNA_AI
                    HStack {
                        Button("Explicar con IA", systemImage: "sparkles") { Task { await model.explainSelectedCommit() } }
                            .disabled(model.isBusy)
                        Text("La explicación no modifica el repositorio.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    #endif
                    Divider()
                    Label("\(model.commitDiff.count) archivo(s) cambiado(s)", systemImage: "doc.on.doc")
                        .font(.headline)
                    PatchView(files: model.commitDiff)
                }
                .padding()
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        } else {
            ContentUnavailableView("Elige un commit", systemImage: "clock.arrow.circlepath", description: Text("Puedes pedir una explicación. No se escribe nada en el disco."))
        }
    }
}
#endif
