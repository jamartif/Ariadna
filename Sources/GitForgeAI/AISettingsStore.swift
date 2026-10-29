import Foundation
import GitForgeCore

public struct AISettingsStore: Sendable {
    public let fileURL: URL

    public init(fileURL: URL = GitForgePaths.settingsURL) {
        self.fileURL = fileURL
    }

    public func load() throws -> AISettings {
        try JSONFileStore.load(AISettings.self, from: fileURL) ?? .default
    }

    public func save(_ settings: AISettings) throws {
        try JSONFileStore.save(settings, to: fileURL)
    }
}
