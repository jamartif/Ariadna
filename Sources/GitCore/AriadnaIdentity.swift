import Foundation

/// Identidad del producto. El bundle id también está en `Sources/App/Info.plist`.
public enum AriadnaIdentity {
    public static let productName = "Ariadna"
    public static let bundleIdentifier = "es.javiermartinez.ariadna"
    public static let minimumMacOS = "15.0"
}

/// Ajustes de la Fase 1 (git). Los de Copilot/Codex viven en AriadnaAI y no se cargan si el flag está apagado.
public struct AppSettings: Sendable, Equatable, Codable {
    public var gitBinary: String
    public var pullUsesRebase: Bool

    public init(gitBinary: String, pullUsesRebase: Bool) {
        self.gitBinary = gitBinary
        self.pullUsesRebase = pullUsesRebase
    }

    public static let `default` = AppSettings(gitBinary: "git", pullUsesRebase: false)
}

public struct AppSettingsStore: Sendable {
    public let fileURL: URL

    public init(fileURL: URL = AriadnaPaths.settingsURL) {
        self.fileURL = fileURL
    }

    public func load() throws -> AppSettings {
        try JSONFileStore.load(AppSettings.self, from: fileURL) ?? .default
    }

    public func save(_ settings: AppSettings) throws {
        try JSONFileStore.save(settings, to: fileURL)
    }
}
