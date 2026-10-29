import Foundation
import GitCore

public struct AISettingsStore: Sendable {
    public let fileURL: URL

    public init(fileURL: URL = AriadnaPaths.aiSettingsURL) {
        self.fileURL = fileURL
    }

    public func load() throws -> AISettings {
        try JSONFileStore.load(AISettings.self, from: fileURL) ?? .default
    }

    public func save(_ settings: AISettings) throws {
        try JSONFileStore.save(settings, to: fileURL)
    }
}
