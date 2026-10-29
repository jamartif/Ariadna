import Foundation

public enum JSONFileStore {
    public static func load<T: Decodable>(_ type: T.Type, from url: URL) throws -> T? {
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        let data = try Data(contentsOf: url)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try decoder.decode(type, from: data)
    }

    public static func save<T: Encodable>(_ value: T, to url: URL) throws {
        let directory = url.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        encoder.dateEncodingStrategy = .iso8601
        try encoder.encode(value).write(to: url, options: .atomic)
    }
}

public enum GitForgePaths {
    public static func configDirectory() -> URL {
        let home = FileManager.default.homeDirectoryForCurrentUser
        #if os(macOS)
        return home.appendingPathComponent("Library/Application Support/GitForge", isDirectory: true)
        #else
        return home.appendingPathComponent(".config/gitforge", isDirectory: true)
        #endif
    }

    public static var recentsURL: URL {
        configDirectory().appendingPathComponent("recents.json")
    }

    public static var settingsURL: URL {
        configDirectory().appendingPathComponent("settings.json")
    }
}

public struct RecentRepositoryStore: Sendable {
    public let fileURL: URL

    public init(fileURL: URL) {
        self.fileURL = fileURL
    }

    public func load() throws -> [RecentRepository] {
        try JSONFileStore.load([RecentRepository].self, from: fileURL) ?? []
    }

    public func remember(_ repository: Repository, branch: String?, at date: Date = Date()) throws {
        var items = try load()
        let previous = items.first { $0.path == repository.path }
        items.removeAll { $0.path == repository.path }
        items.insert(
            RecentRepository(
                path: repository.path,
                name: repository.name,
                lastOpened: date,
                isFavorite: previous?.isFavorite ?? false,
                lastBranch: branch ?? previous?.lastBranch
            ),
            at: 0
        )
        if items.count > 30 {
            let favorites = items.filter(\.isFavorite)
            var trimmed = Array(items.prefix(30))
            for favorite in favorites where !trimmed.contains(where: { $0.path == favorite.path }) {
                trimmed.append(favorite)
            }
            items = trimmed
        }
        try JSONFileStore.save(items, to: fileURL)
    }

    public func setFavorite(path: String, isFavorite: Bool) throws {
        var items = try load()
        guard let index = items.firstIndex(where: { $0.path == path }) else { return }
        items[index].isFavorite = isFavorite
        try JSONFileStore.save(items, to: fileURL)
    }
}
