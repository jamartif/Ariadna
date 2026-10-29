#if os(macOS)
import SwiftUI

/// Punto de entrada en macOS. En Linux este archivo no se compila: el `#else`
/// de abajo deja un ejecutable que explica que la UI es solo de Apple.
@main
struct GitForgeApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            RootView(model: model)
                .frame(minWidth: 980, minHeight: 640)
        }
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("Abrir repositorio…") {
                    model.revealWelcome()
                }
                .keyboardShortcut("o")
            }
        }
        Settings {
            SettingsView(model: model)
        }
    }
}
#else
import Foundation

@main
enum GitForgeLinuxMain {
    static func main() {
        let message = """
        GitForge: la interfaz SwiftUI solo está disponible en macOS 14 o posterior.
        Los módulos GitForgeCore y GitForgeAI sí se compilan y se prueban en este sistema.

        """
        FileHandle.standardError.write(Data(message.utf8))
        // `exit` vive en libc. En Swift 6 `stderr` no se puede usar directo
        // porque es estado global mutable; FileHandle ya escribió el aviso.
        _exit(64)
    }
}
#endif
