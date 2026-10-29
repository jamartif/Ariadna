#if os(macOS)
import SwiftUI
import GitCore
#if ARIADNA_AI
import AriadnaAI
#endif

struct ConflictList: View {
    @Bindable var model: AppModel

    var body: some View {
        List(model.conflicts, selection: Binding(
            get: { model.selectedPath },
            set: { path in
                guard let path else { return }
                model.selectConflict(path)
            }
        )) { file in
            VStack(alignment: .leading, spacing: 2) {
                Text(file.path)
                Text(label(file.kind)).font(.caption).foregroundStyle(.secondary)
            }
            .tag(file.path)
        }
        .overlay {
            if model.conflicts.isEmpty {
                ContentUnavailableView(
                    "No hay conflictos",
                    systemImage: "checkmark.circle",
                    description: Text("Aparecen aquí cuando un merge, pull o rebase no puede juntar un archivo solo.")
                )
            }
        }
    }

    private func label(_ kind: ConflictKind) -> String {
        switch kind {
        case .bothModified: return "modificado en los dos lados"
        case .bothAdded: return "añadido en los dos lados"
        case .bothDeleted: return "borrado en los dos lados"
        case .addedByUs: return "añadido por nosotros"
        case .addedByThem: return "añadido por ellos"
        case .deletedByUs: return "borrado por nosotros"
        case .deletedByThem: return "borrado por ellos"
        case .unknown: return "conflicto"
        }
    }
}

struct ConflictStudioView: View {
    @Bindable var model: AppModel
    let path: String

    private var file: ConflictFile? {
        model.conflicts.first { $0.path == path }
    }

    var body: some View {
        if let file {
            VStack(alignment: .leading, spacing: 8) {
                Text(file.path).font(.headline)
                Text("Tres vías desde el índice de git. Pedir una propuesta no escribe el archivo.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                ViewThatFits(in: .horizontal) {
                    HStack(alignment: .top, spacing: 8) { panes(file) }
                    VStack(alignment: .leading, spacing: 8) { panes(file) }
                }
                hunkButtons(file)
                Text("Resultado").font(.headline)
                TextEditor(text: resultBinding)
                    .font(.system(.body, design: .monospaced))
                    .frame(minHeight: 140)
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(.quaternary))
                    .accessibilityLabel("Resultado del conflicto")
                HStack {
                    #if ARIADNA_AI
                    Button("Pedir propuesta") { Task { await model.proposeResolution(path: path) } }
                        .disabled(model.isBusy)
                    #endif
                    Button("Escribir en el archivo…") { model.pathPendingWrite = path }
                        .disabled(model.isBusy)
                    Button("Marcar resuelto") { Task { await model.markResolved(path: path) } }
                        .disabled(model.isBusy)
                }
            }
            .padding()
        }
    }

    @ViewBuilder
    private func panes(_ file: ConflictFile) -> some View {
        sidePane("Nuestra", text: file.ours.content, missing: !file.ours.isPresent, binary: file.ours.isBinary) {
            model.takeSide(.ours, path: path)
        }
        sidePane("Base", text: file.base.content, missing: !file.base.isPresent, binary: file.base.isBinary) {
            model.takeSide(.base, path: path)
        }
        sidePane("De ellos", text: file.theirs.content, missing: !file.theirs.isPresent, binary: file.theirs.isBinary) {
            model.takeSide(.theirs, path: path)
        }
    }

    private func sidePane(_ title: String, text: String?, missing: Bool, binary: Bool, take: @escaping () -> Void) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(title).font(.subheadline.weight(.semibold))
                Spacer()
                Button("Usar") { take() }
                    .font(.caption)
                    .disabled(missing && text == nil)
            }
            ScrollView {
                Text(body(text: text, missing: missing, binary: binary))
                    .font(.system(.caption, design: .monospaced))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .textSelection(.enabled)
            }
            .frame(minHeight: 120, maxHeight: 220)
            .padding(6)
            .background(Color.primary.opacity(0.04))
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .accessibilityLabel("Panel \(title)")
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func body(text: String?, missing: Bool, binary: Bool) -> String {
        if binary { return "Binario: no se muestra como texto." }
        if missing { return "Este lado no existe (el archivo se borró o no había ancestro)." }
        return text ?? ""
    }

    @ViewBuilder
    private func hunkButtons(_ file: ConflictFile) -> some View {
        if !file.hunks.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                Text("Por hunk, usando los marcadores del archivo").font(.caption).foregroundStyle(.secondary)
                ForEach(file.hunks) { hunk in
                    HStack {
                        Text("Hunk \(hunk.id + 1)").font(.caption)
                        Button("Nuestra") { model.takeHunk(hunk.id, choice: .ours, path: path) }
                        Button("Base") { model.takeHunk(hunk.id, choice: .base, path: path) }
                        Button("De ellos") { model.takeHunk(hunk.id, choice: .theirs, path: path) }
                    }
                    .buttonStyle(.borderless)
                }
            }
        }
    }

    private var resultBinding: Binding<String> {
        Binding(
            get: { model.conflictResults[path] ?? "" },
            set: { model.conflictResults[path] = $0 }
        )
    }
}

#if ARIADNA_AI
struct SuggestionSheet: View {
    @Bindable var model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).font(.title3.bold())
            if let suggestion = model.suggestion {
                Text(suggestion.provider.displayName).font(.caption).foregroundStyle(.secondary)
                Text(suggestion.commandSummary)
                    .font(.caption2.monospaced())
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                if let note = suggestion.transparencyNote {
                    Text(note).font(.caption).foregroundStyle(.secondary)
                }
            }
            Text(safetyLine)
                .font(.callout)
                .padding(8)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(.yellow.opacity(0.15))
                .clipShape(RoundedRectangle(cornerRadius: 6))
            TextEditor(text: $model.suggestionDraft)
                .font(.system(.body, design: .monospaced))
                .frame(minHeight: 180)
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(.quaternary))
                .accessibilityLabel("Propuesta editable")
            HStack {
                if model.suggestion?.kind == .commitMessage {
                    Button("Usar este mensaje") { Task { await model.useCommitSuggestion() } }
                }
                if model.suggestion?.kind == .conflictResolution {
                    Button("Aplicar al editor") { Task { await model.applySuggestionToEditor() } }
                }
                Button("Copiar") { model.copyDraft() }
                Button("Regenerar") { Task { await model.regenerate() } }
                    .disabled(model.isBusy)
                Spacer()
                Button("Descartar", role: .cancel) { Task { await model.discardSuggestion() } }
            }
        }
        .padding(20)
        .frame(minWidth: 560, minHeight: 420)
    }

    private var title: String {
        switch model.suggestion?.kind {
        case .commitMessage: return "Mensaje de commit propuesto"
        case .summarizeDiff: return "Resumen propuesto"
        case .explainCommit: return "Explicación del commit"
        case .conflictResolution: return "Resolución propuesta"
        case nil: return "Propuesta"
        }
    }

    private var safetyLine: String {
        switch model.suggestion?.kind {
        case .conflictResolution:
            return "Aplicar solo copia el texto al editor de resultado. Escribir en disco pide otra confirmación, y marcar resuelto es un paso aparte."
        case .commitMessage:
            return "Usar el mensaje solo rellena el cuadro. Crear commit es otro botón."
        default:
            return "Esto es una propuesta. No se ha escrito nada en el repositorio."
        }
    }
}
#endif

struct SettingsView: View {
    @Bindable var model: AppModel

    var body: some View {
        Form {
            TextField("Ejecutable de git", text: $model.settings.gitBinary)
            Toggle("Pull con rebase", isOn: $model.settings.pullUsesRebase)
            #if ARIADNA_AI
            Picker("Proveedor de IA", selection: $model.aiSettings.provider) {
                Text("Automático").tag(AIProviderChoice.auto)
                Text("GitHub Copilot").tag(AIProviderChoice.copilot)
                Text("Codex").tag(AIProviderChoice.codex)
            }
            TextField("Ejecutable de copilot", text: $model.aiSettings.copilotBinary)
            TextField("Ejecutable de codex", text: $model.aiSettings.codexBinary)
            TextField("Máximo de bytes de contexto", value: $model.aiSettings.maxContextBytes, format: .number)
            Text("Sin API keys. La app usa el login que ya tengas en `copilot login` y `codex login`.")
                .font(.caption)
                .foregroundStyle(.secondary)
            #else
            Text("La asistencia de Copilot y Codex es la Fase 3. Para compilarla: ARIADNA_AI=1 swift build.")
                .font(.caption)
                .foregroundStyle(.secondary)
            #endif
            Button("Guardar") { Task { await model.saveSettings() } }
                .disabled(model.isBusy)
        }
        .padding()
        .frame(width: 480)
    }
}
#endif
