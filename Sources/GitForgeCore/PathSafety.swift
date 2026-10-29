import Foundation

enum PathSafety {
    /// Resuelve una ruta relativa y comprueba que sigue dentro del repo.
    /// Rechaza absolutas, NUL y `..` que se salgan de la raíz.
    static func resolve(relativePath: String, repoRoot: String) throws -> URL {
        try validateGitPath(relativePath)
        let root = URL(fileURLWithPath: repoRoot, isDirectory: true).resolvingSymlinksInPath().standardizedFileURL
        let candidate = URL(fileURLWithPath: relativePath, relativeTo: root).standardizedFileURL
        guard isInside(candidate, root: root) else {
            throw GitError.pathEscapesRepository(path: relativePath)
        }
        if FileManager.default.fileExists(atPath: candidate.path) {
            let resolved = candidate.resolvingSymlinksInPath().standardizedFileURL
            guard isInside(resolved, root: root) else {
                throw GitError.pathEscapesRepository(path: relativePath)
            }
        }
        return candidate
    }

    static func validateGitPath(_ path: String) throws {
        if path.isEmpty || path.contains("\0") || path.hasPrefix("/") || path.hasPrefix("~") {
            throw GitError.pathEscapesRepository(path: path)
        }
        if path.split(separator: "/").contains("..") {
            throw GitError.pathEscapesRepository(path: path)
        }
    }

    /// Nombres de rama/remoto que se van a interpolar como un solo argumento.
    /// Git recibe el nombre como argv, no como shell; aun así rechazamos
    /// formas que `git check-ref-format` tampoco aceptaría.
    static func validateRef(_ name: String) throws {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let forbidden = CharacterSet(charactersIn: " ~^:?*[\\")
        if trimmed.isEmpty
            || trimmed != name
            || trimmed.hasPrefix("-")
            || trimmed.hasPrefix("/")
            || trimmed.hasSuffix("/")
            || trimmed.hasSuffix(".lock")
            || trimmed.contains("..")
            || trimmed.contains("//")
            || trimmed.contains("@{")
            || trimmed.rangeOfCharacter(from: forbidden) != nil
            || trimmed.contains("\0")
        {
            throw GitError.invalidReference(name)
        }
    }

    private static func isInside(_ url: URL, root: URL) -> Bool {
        let rootPath = root.path.hasSuffix("/") ? root.path : root.path + "/"
        let path = url.path
        return path == root.path || path.hasPrefix(rootPath)
    }
}
