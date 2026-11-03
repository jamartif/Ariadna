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
        #if ARIADNA_AI
        .sheet(isPresented: $model.showSuggestion) {
            SuggestionSheet(model: model)
        }
        #endif
        .confirmationDialog(
            "La rama local no coincide con el remoto",
            isPresented: Binding(
                get: { model.pendingDivergedCheckout != nil },
                set: { if !$0 { model.pendingDivergedCheckout = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button("Cambiar a la rama local") {
                if let ref = model.pendingDivergedCheckout {
                    model.pendingDivergedCheckout = nil
                    Task { await model.checkout(ref: ref, allowDivergedLocal: true) }
                }
            }
            Button("Cancelar", role: .cancel) { model.pendingDivergedCheckout = nil }
        } message: {
            Text(model.divergedCheckoutMessage)
        }
        .confirmationDialog(
            "Commit fuera de una rama",
            isPresented: $model.confirmDetachedCommit,
            titleVisibility: .visible
        ) {
            Button("Crear commit igualmente") {
                Task { await model.commitDetached() }
            }
            Button("Cancelar", role: .cancel) {}
        } message: {
            Text(model.status.detachedCommitWarning ?? "HEAD está detached.")
        }
        .animation(.snappy, value: model.infoMessage)
        .overlay(alignment: .bottom) {
            if let info = model.infoMessage {
                Label(info, systemImage: "info.circle.fill")
                    .font(.callout)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(.regularMaterial, in: Capsule())
                    .overlay(Capsule().stroke(Color.primary.opacity(0.08)))
                    .shadow(color: .black.opacity(0.12), radius: 10, y: 4)
                    .padding(.bottom, 16)
                    .onTapGesture { model.infoMessage = nil }
                    .help("Pulsa para cerrar")
                    .accessibilityLabel(info)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
    }
}

struct WelcomeView: View {
    @Bindable var model: AppModel

    private var welcomeSubtitle: String {
        #if ARIADNA_AI
        "Cliente Git para el día a día. Las propuestas de Copilot y Codex se revisan antes de tocar el repositorio."
        #else
        "Cliente Git para el día a día: estado, ramas, merge y conflictos en un solo sitio."
        #endif
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack(spacing: 16) {
                Image(systemName: "point.3.connected.trianglepath.dotted")
                    .font(.system(size: 34, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 64, height: 64)
                    .background(
                        LinearGradient(colors: [.indigo, .purple, .pink], startPoint: .topLeading, endPoint: .bottomTrailing),
                        in: RoundedRectangle(cornerRadius: 16, style: .continuous)
                    )
                    .shadow(color: .purple.opacity(0.3), radius: 8, y: 4)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 4) {
                    Text("Ariadna")
                        .font(.largeTitle.bold())
                    Text(welcomeSubtitle)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("Abrir un repositorio")
                    .font(.headline)
                HStack {
                    TextField("Ruta del repositorio", text: $model.pathDraft)
                        .textFieldStyle(.roundedBorder)
                        .onSubmit { Task { await model.open(path: model.pathDraft) } }
                    Button("Examinar…", systemImage: "folder") { model.browseAndFillPath() }
                    Button("Abrir") {
                        Task { await model.open(path: model.pathDraft) }
                    }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(model.isBusy)
                }
                if model.isBusy {
                    ProgressView("Abriendo…")
                        .controlSize(.small)
                }
            }
            .padding(14)
            .card()

            Text("Recientes")
                .font(.headline)
            if model.recents.isEmpty {
                ContentUnavailableView(
                    "Todavía no hay repositorios",
                    systemImage: "folder",
                    description: Text("Abre una carpeta que ya tenga .git. Clonar e init quedan para una fase posterior.")
                )
            } else {
                List(sortedRecents) { recent in
                    HStack(spacing: 10) {
                        Image(systemName: "folder.fill")
                            .font(.title2)
                            .foregroundStyle(Palette.avatarColor(for: recent.name).gradient)
                            .accessibilityHidden(true)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(recent.name).font(.body.weight(.semibold))
                            Text(recent.path)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                                .truncationMode(.middle)
                            if let branch = recent.lastBranch {
                                Label(branch, systemImage: "arrow.triangle.branch")
                                    .font(.caption2)
                                    .foregroundStyle(.tertiary)
                            }
                        }
                        Spacer()
                        Button {
                            model.toggleFavorite(recent.path)
                        } label: {
                            Image(systemName: recent.isFavorite ? "star.fill" : "star")
                                .foregroundStyle(recent.isFavorite ? Color.yellow : Color.secondary)
                        }
                        .buttonStyle(.borderless)
                        .help(recent.isFavorite ? "Quitar de favoritos" : "Marcar como favorito")
                        Button("Abrir") {
                            Task { await model.open(path: recent.path) }
                        }
                        .disabled(model.isBusy)
                    }
                    .padding(.vertical, 4)
                    .contentShape(Rectangle())
                    .onTapGesture(count: 2) {
                        Task { await model.open(path: recent.path) }
                    }
                }
                .listStyle(.inset)
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            }
        }
        .padding(28)
        .frame(maxWidth: 860, maxHeight: .infinity, alignment: .topLeading)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// Favoritos primero; dentro de cada grupo se respeta el orden de uso.
    private var sortedRecents: [RecentRepository] {
        model.recents.filter(\.isFavorite) + model.recents.filter { !$0.isFavorite }
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
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.orange)
            Text(model.integrationLabel)
                .fontWeight(.medium)
            Spacer()
            Button("Continuar") { Task { await model.continueIntegration() } }
                .disabled(model.isBusy || !model.conflicts.isEmpty)
            Button("Abortar") { model.confirmAbort = true }
                .disabled(model.isBusy)
        }
        .padding(10)
        .background(.orange.opacity(0.14))
        .overlay(alignment: .bottom) { Divider() }
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
        ToolbarItem(placement: .navigation) {
            if let branch = model.status.branchName {
                Label(branch, systemImage: model.status.isDetached ? "exclamationmark.triangle" : "arrow.triangle.branch")
                    .labelStyle(.titleAndIcon)
                    .font(.callout.weight(.medium))
                    .help("Rama actual")
            } else if model.status.isDetached {
                Label("HEAD detached", systemImage: "exclamationmark.triangle")
                    .labelStyle(.titleAndIcon)
                    .foregroundStyle(.orange)
            }
        }
        ToolbarItemGroup {
            if model.isBusy { ProgressView().controlSize(.small) }
            Button("Actualizar", systemImage: "arrow.clockwise") { Task { await model.refresh() } }
                .keyboardShortcut("r", modifiers: .command)
                .help("Volver a leer el estado (⌘R)")
                .disabled(model.isBusy)
            Button("Fetch", systemImage: "arrow.down.circle") { Task { await model.fetch() } }
                .help("Traer cambios del remoto sin tocar tu rama")
                .disabled(model.isBusy)
            Button(model.status.behind > 0 ? "Pull (\(model.status.behind))" : "Pull", systemImage: "arrow.down.to.line") {
                Task { await model.pull() }
            }
            .help("Traer e integrar los cambios del remoto")
            .disabled(model.isBusy)
            Button(model.status.ahead > 0 ? "Push (\(model.status.ahead))" : "Push", systemImage: "arrow.up.to.line") {
                Task { await model.push() }
            }
            .help("Subir tus commits al remoto")
            .disabled(model.isBusy)
        }
    }
}
#endif
