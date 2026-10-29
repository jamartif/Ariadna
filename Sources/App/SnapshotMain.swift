#if os(macOS) && SNAPSHOTS && ARIADNA_AI
import AppKit
import AriadnaAI
@preconcurrency import ScreenCaptureKit
import SwiftUI

/// Genera PNG de las vistas principales con un repositorio de ejemplo.
/// Se compila solo con `ARIADNA_AI=1 swift build -Xswiftc -DSNAPSHOTS`.
@main
enum SnapshotMain {
    static func main() async {
        do {
            try await SnapshotSession().render()
        } catch {
            fputs("capturas: \(error)\n", stderr)
            Foundation.exit(1)
        }
    }
}

@MainActor
private struct SnapshotSession {
    func render() async throws {
        let output = outputDirectory()
        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
        let fixture = try SampleRepository.make()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        let model = AppModel()
        NSApplication.shared.setActivationPolicy(.accessory)

        await model.open(path: fixture.work.path)
        if model.repository == nil {
            throw SnapshotFailure.message("no se pudo abrir el ejemplo: \(model.errorMessage ?? "sin mensaje")")
        }
        model.selectedPath = "notas.txt"
        model.commitMessage = "Describe la niebla de la bocana"
        await model.reloadSelectedDiff()
        if model.stagedDiff.isEmpty || model.unstagedDiff.isEmpty {
            throw SnapshotFailure.message("diff vacío: preparado=\(model.stagedDiff.count) trabajo=\(model.unstagedDiff.count) estado=\(model.status.entries.map(\.path)) \(model.errorMessage ?? "")")
        }
        if model.branches.count < 3 || model.remotes.isEmpty {
            throw SnapshotFailure.message("el ejemplo no tiene ramas y remoto: \(model.branches.map(\.name)) remotos=\(model.remotes.map(\.name))")
        }

        let workspace = CGSize(width: 1440, height: 900)
        try await write("staging.png", SnapshotWorkspace(model: model), size: workspace, to: output)
        try await write("diff.png", ChangesDetail(model: model).background(Color(nsColor: .windowBackgroundColor)), size: CGSize(width: 1100, height: 800), to: output)
        try await write("ramas.png", BranchSidebar(model: model).background(Color(nsColor: .windowBackgroundColor)), size: CGSize(width: 420, height: 780), to: output)

        model.section = .history
        await model.loadHistory()
        guard model.commits.count >= 4 else {
            throw SnapshotFailure.message("historial corto: \(model.commits.map(\.subject))")
        }
        var explained = model.commits.first { $0.subject == "Añade la altura del faro" } ?? model.commits[0]
        await model.selectCommit(explained.sha)
        if model.commitDiff.isEmpty {
            for commit in model.commits where commit.sha != explained.sha {
                await model.selectCommit(commit.sha)
                if !model.commitDiff.isEmpty {
                    explained = commit
                    break
                }
            }
        }
        if model.commitDiff.isEmpty {
            throw SnapshotFailure.message("ningún commit tiene diff (\(explained.subject)) \(model.errorMessage ?? "")")
        }
        try await write("historial.png", SnapshotWorkspace(model: model), size: workspace, to: output)

        model.settings.gitBinary = "/usr/bin/git"
        model.settings.pullUsesRebase = true
        model.aiSettings.provider = .copilot
        model.aiSettings.copilotBinary = "copilot"
        model.aiSettings.codexBinary = "codex"
        model.aiSettings.maxContextBytes = 100_000
        try await write(
            "ajustes.png",
            SettingsView(model: model).background(Color(nsColor: .windowBackgroundColor)),
            size: CGSize(width: 640, height: 560),
            to: output
        )

        let suggestion = AISuggestion(
            id: UUID(uuidString: "8C6A1B2E-4F5D-4A1C-9E77-2B0C5D6E7F80")!,
            kind: .commitMessage,
            provider: .copilot,
            commandSummary: "copilot suggest (simulado, sin red)",
            subject: "Describe la niebla de la bocana",
            body: "Añade el párrafo de la niebla en notas.txt y deja el resto del cuaderno como estaba.",
            rationale: "El diff preparado solo toca notas.txt.",
            proposedContent: nil,
            createdAt: Date(timeIntervalSince1970: 1_700_000_000),
            status: .pending,
            transparencyNote: "Datos de ejemplo. No se ha llamado a ningún modelo ni se ha escrito en el repositorio."
        )
        model.suggestion = suggestion
        model.suggestionDraft = suggestion.commitMessageText
        model.showSuggestion = true
        try await write(
            "panel-ia.png",
            SuggestionSheet(model: model).background(Color(nsColor: .windowBackgroundColor)),
            size: CGSize(width: 760, height: 640),
            to: output
        )

        let names = ["staging.png", "diff.png", "historial.png", "ramas.png", "ajustes.png", "panel-ia.png"]
        for name in names {
            let url = output.appendingPathComponent(name)
            guard FileManager.default.fileExists(atPath: url.path) else {
                throw SnapshotFailure.message("falta \(name)")
            }
            fputs("PNG \(url.path)\n", stderr)
        }
    }

    private func outputDirectory() -> URL {
        let argument = CommandLine.arguments.dropFirst().first ?? "docs/screenshots"
        return URL(fileURLWithPath: argument, isDirectory: true)
    }

    private func write<V: View>(_ name: String, _ view: V, size: CGSize, to directory: URL) async throws {
        let framed = view
            .frame(width: size.width, height: size.height)
            .environment(\.colorScheme, .light)
        let windowPNG = await captureWindow(framed, size: size)
        let rendererPNG = captureRenderer(framed, size: size)
        let layerPNG = captureLayer(framed, size: size)
        note(name, "ventana", windowPNG)
        note(name, "renderer", rendererPNG)
        note(name, "capa", layerPNG)
        let candidates = [windowPNG, rendererPNG, layerPNG].compactMap { $0 }
        guard let best = candidates.max(by: { lhs, rhs in
            let left = colorVariety(lhs)
            let right = colorVariety(rhs)
            if left != right { return left < right }
            return lhs.count < rhs.count
        }) else {
            throw SnapshotFailure.message("\(name): no se pudo rasterizar la vista")
        }
        let variety = colorVariety(best)
        fputs("\(name): \(best.count) bytes, variedad \(variety)\n", stderr)
        if variety < 4 || best.count < 20_000 {
            throw SnapshotFailure.message("\(name): la captura parece vacía (\(best.count) bytes, variedad \(variety))")
        }
        try best.write(to: directory.appendingPathComponent(name))
    }

    private func captureWindow<V: View>(_ view: V, size: CGSize) async -> Data? {
        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.titled, .closable, .miniaturizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Ariadna"
        window.appearance = NSAppearance(named: .aqua)
        let hosting = NSHostingView(rootView: view)
        hosting.frame = NSRect(origin: .zero, size: size)
        hosting.autoresizingMask = [.width, .height]
        window.contentView = hosting
        window.setContentSize(size)
        window.setFrameOrigin(NSPoint(x: 80, y: 80))
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
        hosting.layoutSubtreeIfNeeded()
        pump(1.6)
        window.displayIfNeeded()
        let cached = cacheDisplayData(hosting)
        let shot = await captureScreen(window)
        window.orderOut(nil)
        return richer(cached, shot)
    }

    private func cacheDisplayData(_ view: NSView) -> Data? {
        let bounds = view.bounds
        guard bounds.width > 1, bounds.height > 1, let rep = view.bitmapImageRepForCachingDisplay(in: bounds) else { return nil }
        view.cacheDisplay(in: bounds, to: rep)
        return rep.representation(using: .png, properties: [:])
    }

    private func richer(_ first: Data?, _ second: Data?) -> Data? {
        [first, second].compactMap { $0 }.max { colorVariety($0) < colorVariety($1) }
    }

    private func captureScreen(_ window: NSWindow) async -> Data? {
        do {
            let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
            let windowID = CGWindowID(window.windowNumber)
            guard let match = content.windows.first(where: { $0.windowID == windowID }) else {
                fputs("sck: la ventana \(windowID) no está en pantalla (\(content.windows.count) ventanas)\n", stderr)
                return nil
            }
            let filter = SCContentFilter(desktopIndependentWindow: match)
            let config = SCStreamConfiguration()
            config.width = max(1, Int(match.frame.width * 2))
            config.height = max(1, Int(match.frame.height * 2))
            config.showsCursor = false
            config.scalesToFit = true
            let image = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config)
            return NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])
        } catch {
            fputs("sck: \(error)\n", stderr)
            return nil
        }
    }

    private func note(_ name: String, _ method: String, _ data: Data?) {
        if let data {
            fputs("\(name) \(method): \(data.count) bytes, variedad \(colorVariety(data))\n", stderr)
        } else {
            fputs("\(name) \(method): nil\n", stderr)
        }
    }

    private func captureRenderer<V: View>(_ view: V, size: CGSize) -> Data? {
        let renderer = ImageRenderer(content: view)
        renderer.scale = 2
        renderer.proposedSize = ProposedViewSize(width: size.width, height: size.height)
        renderer.isOpaque = true
        guard let image = renderer.cgImage else { return nil }
        let rep = NSBitmapImageRep(cgImage: image)
        return rep.representation(using: .png, properties: [:])
    }

    private func captureLayer<V: View>(_ view: V, size: CGSize) -> Data? {
        let hosting = NSHostingView(rootView: view)
        hosting.frame = NSRect(origin: .zero, size: size)
        hosting.wantsLayer = true
        hosting.layoutSubtreeIfNeeded()
        guard let layer = hosting.layer else { return nil }
        let image = NSImage(size: size)
        image.lockFocus()
        if let context = NSGraphicsContext.current?.cgContext {
            layer.render(in: context)
        }
        image.unlockFocus()
        guard let tiff = image.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff) else { return nil }
        return rep.representation(using: .png, properties: [:])
    }

    private func colorVariety(_ data: Data) -> Int {
        guard let rep = NSBitmapImageRep(data: data) else { return 0 }
        let width = rep.pixelsWide
        let height = rep.pixelsHigh
        if width < 50 || height < 50 { return 0 }
        var seen = Set<UInt32>()
        let stepX = max(1, width / 48)
        let stepY = max(1, height / 48)
        var y = min(48, height / 8)
        while y < height {
            var x = 0
            while x < width {
                if let color = rep.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) {
                    let red = UInt32((color.redComponent * 7).rounded())
                    let green = UInt32((color.greenComponent * 7).rounded())
                    let blue = UInt32((color.blueComponent * 7).rounded())
                    seen.insert((red << 16) | (green << 8) | blue)
                    if seen.count > 48 { return seen.count }
                }
                x += stepX
            }
            y += stepY
        }
        return seen.count
    }

    private func pump(_ seconds: TimeInterval) {
        let deadline = Date().addingTimeInterval(seconds)
        while Date() < deadline {
            RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.05))
        }
    }
}

private struct SnapshotWorkspace: View {
    @Bindable var model: AppModel

    var body: some View {
        HStack(spacing: 0) {
            BranchSidebar(model: model)
                .frame(width: 280)
            Divider()
            VStack(spacing: 0) {
                Picker("Sección", selection: $model.section) {
                    ForEach(WorkspaceSection.allCases) { section in
                        Text(section.title).tag(section)
                    }
                }
                .pickerStyle(.segmented)
                .padding(8)
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
            .frame(width: 420)
            Divider()
            switch model.section {
            case .changes:
                ChangesDetail(model: model)
            case .history:
                HistoryDetail(model: model)
            case .conflicts:
                Color.clear
            }
        }
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

private enum SnapshotFailure: Error, CustomStringConvertible {
    case git(String)
    case message(String)

    var description: String {
        switch self {
        case .git(let text): return text
        case .message(let text): return text
        }
    }
}

private enum SampleRepository {
    struct Fixture {
        let root: URL
        let work: URL
    }

    static func make() throws -> Fixture {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("ariadna-snapshots-\(UUID().uuidString)", isDirectory: true)
        let work = root.appendingPathComponent("cuaderno", isDirectory: true)
        let bare = root.appendingPathComponent("origin.git", isDirectory: true)
        try FileManager.default.createDirectory(at: work, withIntermediateDirectories: true)
        try git(["init", "-b", "main"], at: work)
        try git(["config", "user.name", "Ada Lovelace"], at: work)
        try git(["config", "user.email", "ada@ariadna.local"], at: work)
        try git(["config", "commit.gpgsign", "false"], at: work)

        try write("Cuaderno del faro\n", to: work.appendingPathComponent("README.md"))
        try commit("Inicializa el cuaderno del faro", at: work, date: "2026-03-01T09:00:00Z")

        try write("Cuaderno del faro\n\nAltura: 34 m sobre el nivel del mar.\n", to: work.appendingPathComponent("README.md"))
        try commit("Añade la altura del faro", body: "Queda en 34 metros sobre el nivel del mar.", at: work, date: "2026-03-02T09:00:00Z")

        try git(["checkout", "-b", "feature/notas"], at: work)
        try write("Marea de marzo: alta a las 18:40.\n", to: work.appendingPathComponent("marea.md"))
        try commit("Apunta la marea de marzo", at: work, date: "2026-03-03T09:00:00Z")

        try git(["checkout", "main"], at: work)
        try write("Cuaderno del faro\n\nAltura: 34 m sobre el nivel del mar.\nDistancia a la costa: 2,4 km.\n", to: work.appendingPathComponent("README.md"))
        try commit("Corrige la distancia a la costa", at: work, date: "2026-03-04T09:00:00Z")
        try git(["branch", "experimento"], at: work)

        try git(["init", "--bare", bare.path], at: root)
        try git(["remote", "add", "origin", bare.path], at: work)
        try git(["push", "-u", "origin", "main"], at: work)
        try git(["push", "origin", "feature/notas"], at: work)

        let notes = work.appendingPathComponent("notas.txt")
        try write("El faro se enciende al anochecer.\n", to: notes)
        try commit("Deja constancia de cuándo se enciende el faro", at: work, date: "2026-03-05T09:00:00Z")
        try write("El faro se enciende al anochecer.\nLa marea cubre el muelle.\n", to: notes)
        try git(["add", "--", "notas.txt"], at: work)
        try write("El faro se enciende al anochecer.\nLa marea cubre el muelle.\nHay niebla en la bocana.\n", to: notes)
        try write("ideas sueltas del diario\n", to: work.appendingPathComponent("borrador.txt"))
        return Fixture(root: root, work: work)
    }

    private static func commit(_ subject: String, body: String? = nil, at directory: URL, date: String) throws {
        var args = ["add", "--", "."]
        try git(args, at: directory)
        args = ["commit", "-m", subject]
        if let body {
            args.append(contentsOf: ["-m", body])
        }
        try git(args, at: directory, extraEnv: ["GIT_AUTHOR_DATE": date, "GIT_COMMITTER_DATE": date])
    }

    private static func write(_ text: String, to url: URL) throws {
        try text.write(to: url, atomically: true, encoding: .utf8)
    }

    @discardableResult
    private static func git(_ args: [String], at directory: URL, extraEnv: [String: String] = [:]) throws -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = args
        process.currentDirectoryURL = directory
        var env = ProcessInfo.processInfo.environment
        for (key, value) in extraEnv {
            env[key] = value
        }
        process.environment = env
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        try process.run()
        process.waitUntilExit()
        let errorText = String(data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let output = String(data: stdout.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        if process.terminationStatus != 0 {
            throw SnapshotFailure.git("git \(args.joined(separator: " "))\n\(errorText)")
        }
        return output
    }
}
#elseif os(macOS) && SNAPSHOTS
#error("Compila las capturas con ARIADNA_AI=1")
#else
enum SnapshotMainDisabled {}
#endif
