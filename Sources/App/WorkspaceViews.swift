#if os(macOS)
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
            Section("Ramas") {
                if model.branches.isEmpty {
                    Text("Sin ramas todavía").foregroundStyle(.secondary)
                }
                ForEach(model.branches) { branch in
                    HStack {
                        Text(branch.name)
                        if branch.isHead {
                            Image(systemName: "checkmark")
                                .accessibilityLabel("Rama actual")
                        }
                        Spacer()
                        if let upstream = branch.upstream {
                            Text(upstream).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                        }
                    }
                    .tag(branch.name)
                    .contextMenu {
                        Button("Borrar…") { model.branchPendingDelete = branch.name }
                    }
                }
            }
            Section("Remotos") {
                if model.remotes.isEmpty {
                    Text("Sin remotos").foregroundStyle(.secondary)
                }
                ForEach(model.remotes) { remote in
                    VStack(alignment: .leading) {
                        Text(remote.name)
                        Text(remote.fetchURL).font(.caption2).foregroundStyle(.secondary).lineLimit(2)
                    }
                }
            }
        }
        .safeAreaInset(edge: .bottom) {
            VStack(alignment: .leading, spacing: 8) {
                if model.status.isDetached {
                    Text("HEAD detached")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.orange)
                }
                HStack {
                    TextField("Nueva rama", text: $model.newBranchName)
                        .textFieldStyle(.roundedBorder)
                    Button("Crear") { Task { await model.createBranch() } }
                        .disabled(model.isBusy || model.newBranchName.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                if model.branches.count > 1 {
                    Menu("Fusionar…") {
                        ForEach(model.branches.filter { !$0.isHead }) { branch in
                            Button(branch.name) { Task { await model.merge(branch: branch.name) } }
                        }
                    }
                    .disabled(model.isBusy)
                }
            }
            .padding(8)
        }
    }
}

struct ChangesList: View {
    @Bindable var model: AppModel

    var body: some View {
        List(selection: $model.selectedPath) {
            changeSection("Preparados", entries: model.status.staged, empty: "Nada preparado")
            changeSection("Sin preparar", entries: model.status.unstaged, empty: "El árbol de trabajo está limpio")
            if model.includeIgnored {
                changeSection("Ignorados", entries: model.status.ignored, empty: "Sin ignorados")
            }
        }
        .overlay {
            if model.status.entries.isEmpty && !model.isBusy {
                ContentUnavailableView("Sin cambios", systemImage: "checkmark.circle", description: Text("No hay nada que preparar en este repositorio."))
            }
        }
    }

    @ViewBuilder
    private func changeSection(_ title: String, entries: [StatusEntry], empty: String) -> some View {
        Section(title) {
            if entries.isEmpty {
                Text(empty).foregroundStyle(.secondary)
            }
            ForEach(entries) { entry in
                StatusRow(entry: entry, model: model)
                    .tag(entry.path)
            }
        }
    }
}

struct StatusRow: View {
    let entry: StatusEntry
    let model: AppModel

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(entry.path).lineLimit(1)
                Text(badge).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if entry.isUntracked || entry.hasUnstagedChange {
                Button("Preparar") { Task { await model.stage(paths: [entry.path]) } }
                    .buttonStyle(.borderless)
                    .disabled(model.isBusy)
            }
            if entry.isStaged {
                Button("Quitar") { Task { await model.unstage(paths: [entry.path]) } }
                    .buttonStyle(.borderless)
                    .disabled(model.isBusy)
            }
        }
        .contextMenu {
            if entry.hasUnstagedChange && !entry.isUntracked {
                Button("Descartar cambios…") { model.pathsPendingRestore = [entry.path] }
            }
        }
    }

    private var badge: String {
        if entry.isUntracked { return "sin seguimiento" }
        if let original = entry.originalPath { return "desde \(original)" }
        return "índice \(entry.indexStatus.rawValue) · trabajo \(entry.workTreeStatus.rawValue)"
    }
}

struct CommitBar: View {
    @Bindable var model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let warning = model.status.detachedCommitWarning {
                Text(warning)
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .accessibilityLabel("Aviso de HEAD detached")
            }
            #if ARIADNA_AI
            Text("La IA rellena el mensaje. El commit solo ocurre con el botón de abajo.")
                .font(.caption)
                .foregroundStyle(.secondary)
            #else
            Text("El commit solo incluye lo que está preparado.")
                .font(.caption)
                .foregroundStyle(.secondary)
            #endif
            TextEditor(text: $model.commitMessage)
                .font(.body)
                .frame(minHeight: 72, maxHeight: 120)
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(.quaternary))
                .accessibilityLabel("Mensaje de commit")
            HStack {
                #if ARIADNA_AI
                Button("Generar mensaje") { Task { await model.generateCommitMessage() } }
                    .disabled(model.isBusy)
                Button("Resumir") { Task { await model.summarizeStaged() } }
                    .disabled(model.isBusy)
                #endif
                Spacer()
                Button("Crear commit") { Task { await model.commit() } }
                    .keyboardShortcut(.return, modifiers: .command)
                    .disabled(model.isBusy || model.commitMessage.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.status.staged.isEmpty)
            }
            TextField("Plantilla opcional para el mensaje", text: $model.commitTemplate)
                .textFieldStyle(.roundedBorder)
                .font(.caption)
        }
        .padding(8)
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
                        Text("Preparado").font(.headline)
                        PatchView(files: model.stagedDiff, onUnstage: { path, hunk in
                            Task { await model.unstageHunk(hunk, path: path) }
                        })
                    }
                    if !model.unstagedDiff.isEmpty {
                        Text("Sin preparar").font(.headline)
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

struct PatchView: View {
    let files: [FileDiff]
    var onStage: ((String, DiffHunk) -> Void)?
    var onUnstage: ((String, DiffHunk) -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(files) { file in
                Text(file.path).font(.subheadline.weight(.semibold))
                if file.isBinary {
                    Text("Archivo binario").foregroundStyle(.secondary)
                }
                ForEach(file.hunks) { hunk in
                    VStack(alignment: .leading, spacing: 0) {
                        HStack {
                            Text(hunk.heading.isEmpty ? "@@ -\(hunk.oldStart) +\(hunk.newStart)" : hunk.heading)
                                .font(.caption.monospaced())
                                .foregroundStyle(.secondary)
                            Spacer()
                            if let onStage {
                                Button("Preparar hunk") { onStage(file.path, hunk) }
                                    .font(.caption)
                            }
                            if let onUnstage {
                                Button("Quitar hunk") { onUnstage(file.path, hunk) }
                                    .font(.caption)
                            }
                        }
                        ForEach(hunk.lines) { line in
                            Text(rendered(line))
                                .font(.system(.caption, design: .monospaced))
                                .foregroundStyle(color(for: line.kind))
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                    .padding(6)
                    .background(Color.primary.opacity(0.04))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                }
            }
        }
        .textSelection(.enabled)
    }

    private func rendered(_ line: DiffLine) -> String {
        let mark: String
        switch line.kind {
        case .addition: mark = "+"
        case .deletion: mark = "-"
        case .context: mark = " "
        case .meta: mark = ""
        }
        return mark + line.text
    }

    private func color(for kind: DiffLine.Kind) -> Color {
        switch kind {
        case .addition: return .green
        case .deletion: return .red
        case .context: return .primary
        case .meta: return .secondary
        }
    }
}

struct HistoryList: View {
    @Bindable var model: AppModel
    @State private var query = ""

    var body: some View {
        VStack {
            TextField("Filtrar por mensaje, autor o hash", text: $query)
                .textFieldStyle(.roundedBorder)
                .padding([.horizontal, .top], 8)
            List(filtered, selection: $model.selectedCommitSHA) { commit in
                VStack(alignment: .leading, spacing: 2) {
                    Text(commit.subject).lineLimit(2)
                    Text("\(commit.shortSHA) · \(commit.authorName) · \(commit.authorDateISO)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .tag(commit.sha)
            }
            .onChange(of: model.selectedCommitSHA) { _, sha in
                guard let sha else { return }
                Task { await model.selectCommit(sha) }
            }
        }
        .overlay {
            if model.commits.isEmpty && !model.isBusy {
                ContentUnavailableView("Sin historial cargado", systemImage: "clock", description: Text("Abre esta pestaña para leer los últimos commits. Un repo nuevo todavía no tiene ninguno."))
            }
        }
    }

    private var filtered: [Commit] {
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

struct HistoryDetail: View {
    let model: AppModel

    var body: some View {
        if let sha = model.selectedCommitSHA, let commit = model.commits.first(where: { $0.sha == sha }) {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    Text(commit.subject).font(.title3.bold())
                    Text(commit.sha).font(.caption.monospaced()).textSelection(.enabled)
                    Text("\(commit.authorName) <\(commit.authorEmail)>").font(.callout)
                    if !commit.body.isEmpty {
                        Text(commit.body).textSelection(.enabled)
                    }
                    if !commit.refs.isEmpty {
                        Text(commit.refs.joined(separator: " · ")).font(.caption).foregroundStyle(.secondary)
                    }
                    #if ARIADNA_AI
                    Button("Explicar con IA") { Task { await model.explainSelectedCommit() } }
                        .disabled(model.isBusy)
                    Text("La explicación no modifica el repositorio.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    #endif
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
