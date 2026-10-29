#if os(macOS)
import SwiftUI

struct RootView: View {
    @Bindable var model: AppModel

    var body: some View {
        Group {
            if model.repository == nil {
                WelcomeView(model: model)
            } else {
                WorkspaceView(model: model)
            }
        }
        .alert(
            "No se pudo completar",
            isPresented: Binding(
                get: { model.errorMessage != nil },
                set: { if !$0 { model.errorMessage = nil } }
            )
        ) {
            Button("Aceptar", role: .cancel) {}
        } message: {
            Text(model.errorMessage ?? "")
        }
        .sheet(isPresented: $model.showSuggestion) {
            SuggestionSheet(model: model)
        }
        .overlay(alignment: .bottom) {
            if let info = model.infoMessage {
                Text(info)
                    .font(.callout)
                    .padding(10)
                    .background(.thinMaterial)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .padding(.bottom, 12)
                    .onTapGesture { model.infoMessage = nil }
                    .accessibilityLabel(info)
            }
        }
    }
}

struct WelcomeView: View {
    @Bindable var model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("GitForge")
                .font(.largeTitle.bold())
            Text("Cliente Git para el día a día. Las propuestas de Copilot y Codex se revisan antes de tocar el repositorio.")
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            HStack {
                TextField("Ruta del repositorio", text: $model.pathDraft)
                    .textFieldStyle(.roundedBorder)
                Button("Examinar…") { model.browseAndFillPath() }
                Button("Abrir") {
                    Task { await model.open(path: model.pathDraft) }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(model.isBusy)
            }

            if model.isBusy {
                ProgressView("Abriendo…")
            }

            Text("Recientes")
                .font(.headline)
            if model.recents.isEmpty {
                ContentUnavailableView(
                    "Todavía no hay repositorios",
                    systemImage: "folder",
                    description: Text("Abre una carpeta que ya tenga .git. Clonar e init quedan para una fase posterior.")
                )
            } else {
                List(model.recents) { recent in
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(recent.name).font(.body.weight(.semibold))
                            Text(recent.path).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                            if let branch = recent.lastBranch {
                                Text(branch).font(.caption2).foregroundStyle(.tertiary)
                            }
                        }
                        Spacer()
                        Button(recent.isFavorite ? "Quitar favorito" : "Favorito") {
                            model.toggleFavorite(recent.path)
                        }
                        .buttonStyle(.borderless)
                        Button("Abrir") {
                            Task { await model.open(path: recent.path) }
                        }
                        .disabled(model.isBusy)
                    }
                }
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

struct WorkspaceView: View {
    @Bindable var model: AppModel

    var body: some View {
        NavigationSplitView {
            BranchSidebar(model: model)
                .navigationSplitViewColumnWidth(min: 200, ideal: 240, max: 320)
        } content: {
            VStack(spacing: 0) {
                if model.integration != .none {
                    integrationBanner
                }
                Picker("Sección", selection: $model.section) {
                    ForEach(WorkspaceSection.allCases) { section in
                        Text(sectionLabel(section)).tag(section)
                    }
                }
                .pickerStyle(.segmented)
                .padding(8)
                .accessibilityLabel("Sección del repositorio")

                switch model.section {
                case .changes:
                    ChangesList(model: model)
                    CommitBar(model: model)
                case .history:
                    HistoryList(model: model)
                case .conflicts:
                    ConflictList(model: model)
                }
            }
            .navigationSplitViewColumnWidth(min: 280, ideal: 360, max: 480)
        } detail: {
            detail
        }
        .navigationTitle(model.repositoryName)
        .toolbar { toolbar }
        .onChange(of: model.section) { _, section in
            if section == .history {
                Task { await model.loadHistory() }
            }
        }
        .onChange(of: model.selectedPath) { _, _ in
            Task { await model.reloadSelectedDiff() }
        }
        .confirmationDialog(
            "Abortar la operación en curso",
            isPresented: $model.confirmAbort,
            titleVisibility: .visible
        ) {
            Button("Abortar", role: .destructive) {
                Task { await model.abortIntegration() }
            }
            Button("Cancelar", role: .cancel) {}
        } message: {
            Text("Se descartará el merge, rebase o cherry-pick sin terminar.")
        }
        .confirmationDialog(
            "Escribir la resolución en el archivo",
            isPresented: Binding(
                get: { model.pathPendingWrite != nil },
                set: { if !$0 { model.pathPendingWrite = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button("Escribir archivo") {
                if let path = model.pathPendingWrite {
                    model.pathPendingWrite = nil
                    Task { await model.writeResult(path: path) }
                }
            }
            Button("Cancelar", role: .cancel) { model.pathPendingWrite = nil }
        } message: {
            Text("Sustituye el archivo en disco. No hace git add ni commit.")
        }
        .confirmationDialog(
            "Descartar cambios del archivo",
            isPresented: Binding(
                get: { model.pathsPendingRestore != nil },
                set: { if !$0 { model.pathsPendingRestore = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button("Descartar", role: .destructive) {
                if let paths = model.pathsPendingRestore {
                    model.pathsPendingRestore = nil
                    Task { await model.restore(paths: paths) }
                }
            }
            Button("Cancelar", role: .cancel) { model.pathsPendingRestore = nil }
        } message: {
            Text("Se pierde lo que no está en el último commit. No afecta a lo que ya está preparado en el índice.")
        }
        .confirmationDialog(
            "Borrar rama",
            isPresented: Binding(
                get: { model.branchPendingDelete != nil },
                set: { if !$0 { model.branchPendingDelete = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button("Borrar", role: .destructive) {
                if let name = model.branchPendingDelete {
                    model.branchPendingDelete = nil
                    Task { await model.deleteBranch(name: name, force: false) }
                }
            }
            Button("Forzar borrado", role: .destructive) {
                if let name = model.branchPendingDelete {
                    model.branchPendingDelete = nil
                    Task { await model.deleteBranch(name: name, force: true) }
                }
            }
            Button("Cancelar", role: .cancel) { model.branchPendingDelete = nil }
        } message: {
            Text("Borrar elimina la referencia local. Forzar también borra ramas sin fusionar.")
        }
    }

    private var integrationBanner: some View {
        HStack {
            Text(model.integrationLabel)
            Spacer()
            Button("Continuar") { Task { await model.continueIntegration() } }
                .disabled(model.isBusy || !model.conflicts.isEmpty)
            Button("Abortar") { model.confirmAbort = true }
                .disabled(model.isBusy)
        }
        .padding(8)
        .background(.yellow.opacity(0.18))
        .accessibilityElement(children: .combine)
    }

    private func sectionLabel(_ section: WorkspaceSection) -> String {
        if section == .conflicts && !model.conflicts.isEmpty {
            return "Conflictos (\(model.conflicts.count))"
        }
        return section.title
    }

    @ViewBuilder
    private var detail: some View {
        switch model.section {
        case .changes:
            ChangesDetail(model: model)
        case .history:
            HistoryDetail(model: model)
        case .conflicts:
            if let path = model.selectedPath, model.conflicts.contains(where: { $0.path == path }) {
                ConflictStudioView(model: model, path: path)
            } else {
                ContentUnavailableView(
                    "Sin conflicto seleccionado",
                    systemImage: "arrow.triangle.branch",
                    description: Text("Elige un archivo en conflicto para ver nuestra versión, la base y la entrante.")
                )
            }
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItemGroup {
            Button("Actualizar") { Task { await model.refresh() } }
                .disabled(model.isBusy)
            Button("Fetch") { Task { await model.fetch() } }
                .disabled(model.isBusy)
            Button("Pull") { Task { await model.pull() } }
                .disabled(model.isBusy)
            Button("Push") { Task { await model.push() } }
                .disabled(model.isBusy)
            if model.isBusy { ProgressView().controlSize(.small) }
        }
    }
}
#endif
