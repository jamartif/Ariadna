import Foundation

/// Carpeta que ya es un repo (`git rev-parse` tuvo éxito).
/// `Sendable` significa que se puede pasar entre tareas sin copiar estado mutable;
/// es un DTO inmutable, como un `record` de C#.
public struct Repository: Sendable, Equatable, Identifiable, Codable {
    public let path: String
    public let gitDir: String

    public var id: String { path }

    public var name: String {
        URL(fileURLWithPath: path).lastPathComponent
    }

    public init(path: String, gitDir: String) {
        self.path = path
        self.gitDir = gitDir
    }
}

public enum ChangeStatus: String, Sendable, Equatable, Codable {
    case unmodified = "."
    case modified = "M"
    case added = "A"
    case deleted = "D"
    case renamed = "R"
    case copied = "C"
    case unmerged = "U"
    case typeChanged = "T"
    case untracked = "?"
    case ignored = "!"
    case unknown = ""

    init(porcelain character: Character) {
        self = ChangeStatus(rawValue: String(character)) ?? .unknown
    }
}

/// Cómo está un archivo en conflicto, según el par XY de `git status`.
public enum ConflictKind: String, Sendable, Equatable, Codable {
    case bothModified
    case bothAdded
    case bothDeleted
    case addedByUs
    case addedByThem
    case deletedByUs
    case deletedByThem
    case unknown

    init(porcelainXY: String) {
        switch porcelainXY {
        case "UU": self = .bothModified
        case "AA": self = .bothAdded
        case "DD": self = .bothDeleted
        case "AU": self = .addedByUs
        case "UA": self = .addedByThem
        case "DU": self = .deletedByUs
        case "UD": self = .deletedByThem
        default: self = .unknown
        }
    }
}

public struct StatusEntry: Sendable, Equatable, Identifiable {
    public let path: String
    public let originalPath: String?
    public let indexStatus: ChangeStatus
    public let workTreeStatus: ChangeStatus
    public let isConflicted: Bool
    public let isUntracked: Bool
    public let isIgnored: Bool
    public let isSubmodule: Bool
    public let conflictKind: ConflictKind?

    public var id: String { path }

    public init(
        path: String,
        originalPath: String?,
        indexStatus: ChangeStatus,
        workTreeStatus: ChangeStatus,
        isConflicted: Bool,
        isUntracked: Bool,
        isIgnored: Bool,
        isSubmodule: Bool,
        conflictKind: ConflictKind?
    ) {
        self.path = path
        self.originalPath = originalPath
        self.indexStatus = indexStatus
        self.workTreeStatus = workTreeStatus
        self.isConflicted = isConflicted
        self.isUntracked = isUntracked
        self.isIgnored = isIgnored
        self.isSubmodule = isSubmodule
        self.conflictKind = conflictKind
    }

    public var isStaged: Bool {
        !isUntracked && !isIgnored && !isConflicted && indexStatus != .unmodified
    }

    public var hasUnstagedChange: Bool {
        isUntracked || (!isIgnored && !isConflicted && workTreeStatus != .unmodified)
    }
}

public struct GitStatus: Sendable, Equatable {
    public let branchName: String?
    public let headOID: String?
    public let isDetached: Bool
    public let isUnborn: Bool
    public let upstream: String?
    public let ahead: Int
    public let behind: Int
    public let entries: [StatusEntry]

    public init(
        branchName: String?,
        headOID: String?,
        isDetached: Bool,
        isUnborn: Bool,
        upstream: String?,
        ahead: Int,
        behind: Int,
        entries: [StatusEntry]
    ) {
        self.branchName = branchName
        self.headOID = headOID
        self.isDetached = isDetached
        self.isUnborn = isUnborn
        self.upstream = upstream
        self.ahead = ahead
        self.behind = behind
        self.entries = entries
    }

    public var staged: [StatusEntry] { entries.filter(\.isStaged) }
    public var unstaged: [StatusEntry] { entries.filter(\.hasUnstagedChange) }
    public var conflicted: [StatusEntry] { entries.filter(\.isConflicted) }
    public var ignored: [StatusEntry] { entries.filter(\.isIgnored) }

    /// Texto para la barra de commit cuando no hay rama. Nil si HEAD es una rama.
    public var detachedCommitWarning: String? {
        guard isDetached else { return nil }
        return "HEAD no apunta a una rama. Si confirmas, el commit no pertenecerá a ninguna rama hasta que crees una."
    }

    public static let empty = GitStatus(
        branchName: nil,
        headOID: nil,
        isDetached: false,
        isUnborn: false,
        upstream: nil,
        ahead: 0,
        behind: 0,
        entries: []
    )
}

public struct Commit: Sendable, Equatable, Identifiable {
    public let sha: String
    public let parents: [String]
    public let subject: String
    public let body: String
    public let authorName: String
    public let authorEmail: String
    public let authorDateISO: String
    public let refs: [String]

    public var id: String { sha }

    public var shortSHA: String { String(sha.prefix(7)) }

    public init(
        sha: String,
        parents: [String],
        subject: String,
        body: String,
        authorName: String,
        authorEmail: String,
        authorDateISO: String,
        refs: [String]
    ) {
        self.sha = sha
        self.parents = parents
        self.subject = subject
        self.body = body
        self.authorName = authorName
        self.authorEmail = authorEmail
        self.authorDateISO = authorDateISO
        self.refs = refs
    }
}

public struct Branch: Sendable, Equatable, Identifiable {
    public let name: String
    public let oid: String
    public let isHead: Bool
    public let upstream: String?
    public let ahead: Int
    public let behind: Int

    public var id: String { name }

    public init(name: String, oid: String, isHead: Bool, upstream: String?, ahead: Int, behind: Int) {
        self.name = name
        self.oid = oid
        self.isHead = isHead
        self.upstream = upstream
        self.ahead = ahead
        self.behind = behind
    }
}

public struct Remote: Sendable, Equatable, Identifiable {
    public let name: String
    public let fetchURL: String
    public let pushURL: String

    public var id: String { name }

    public init(name: String, fetchURL: String, pushURL: String) {
        self.name = name
        self.fetchURL = fetchURL
        self.pushURL = pushURL
    }
}

public enum MergeFastForward: Sendable, Equatable {
    /// `--ff`: avanza si puede y, si no, crea un merge commit.
    case allow
    /// `--ff-only`: falla si no puede avanzar en línea recta.
    case only
    /// `--no-ff`: siempre crea un merge commit.
    case never
}

public enum MergeResult: Sendable, Equatable {
    case upToDate
    case fastForward(newHEAD: String)
    case merged(newHEAD: String)
    case conflicts(paths: [String])
}

public enum IntegrationState: Sendable, Equatable {
    case none
    case merging
    case rebasing
    case cherryPicking
}

public enum DiffScope: Sendable, Equatable {
    case unstaged
    case staged
    case commit(String)
    case between(base: String, head: String)
}

public struct DiffLine: Sendable, Equatable, Identifiable {
    public enum Kind: String, Sendable, Equatable {
        case context
        case addition
        case deletion
        case meta
    }

    public let id: String
    public let kind: Kind
    public let text: String
    public let oldLineNumber: Int?
    public let newLineNumber: Int?

    public init(id: String, kind: Kind, text: String, oldLineNumber: Int?, newLineNumber: Int?) {
        self.id = id
        self.kind = kind
        self.text = text
        self.oldLineNumber = oldLineNumber
        self.newLineNumber = newLineNumber
    }
}

public struct DiffHunk: Sendable, Equatable, Identifiable {
    public let id: String
    public let oldStart: Int
    public let oldCount: Int
    public let newStart: Int
    public let newCount: Int
    public let heading: String
    public let lines: [DiffLine]
    /// Trozo unificado, incluida la línea `@@`, listo para recomponer un parche.
    public let unifiedText: String

    public init(
        id: String,
        oldStart: Int,
        oldCount: Int,
        newStart: Int,
        newCount: Int,
        heading: String,
        lines: [DiffLine],
        unifiedText: String
    ) {
        self.id = id
        self.oldStart = oldStart
        self.oldCount = oldCount
        self.newStart = newStart
        self.newCount = newCount
        self.heading = heading
        self.lines = lines
        self.unifiedText = unifiedText
    }

    public func matchesRange(of other: DiffHunk) -> Bool {
        oldStart == other.oldStart
            && oldCount == other.oldCount
            && newStart == other.newStart
            && newCount == other.newCount
    }
}

public struct FileDiff: Sendable, Equatable, Identifiable {
    public let path: String
    public let oldPath: String?
    public let isBinary: Bool
    public let isNewFile: Bool
    public let isDeletedFile: Bool
    public let header: String
    public let hunks: [DiffHunk]
    public let raw: String

    public var id: String { path }

    public init(
        path: String,
        oldPath: String?,
        isBinary: Bool,
        isNewFile: Bool,
        isDeletedFile: Bool,
        header: String,
        hunks: [DiffHunk],
        raw: String
    ) {
        self.path = path
        self.oldPath = oldPath
        self.isBinary = isBinary
        self.isNewFile = isNewFile
        self.isDeletedFile = isDeletedFile
        self.header = header
        self.hunks = hunks
        self.raw = raw
    }

    /// Parche de un solo hunk. `git apply --cached` lo aplica sobre el índice.
    public func patch(selecting hunk: DiffHunk) -> String {
        var text = header
        if !text.hasSuffix("\n") { text.append("\n") }
        text.append(hunk.unifiedText)
        if !text.hasSuffix("\n") { text.append("\n") }
        return text
    }
}

public enum ConflictChoice: String, Sendable, Equatable, CaseIterable {
    case ours
    case base
    case theirs
}

/// Un lado del modelo a tres vías. El contenido sale del índice
/// (`:1:` base, `:2:` ours, `:3:` theirs), no de adivinar los marcadores.
public struct ConflictSide: Sendable, Equatable {
    public let oid: String?
    public let content: String?
    public let isBinary: Bool

    public var isPresent: Bool { oid != nil }

    public init(oid: String?, content: String?, isBinary: Bool) {
        self.oid = oid
        self.content = content
        self.isBinary = isBinary
    }

    public static let absent = ConflictSide(oid: nil, content: nil, isBinary: false)
}

public struct ConflictHunk: Sendable, Equatable, Identifiable {
    public let id: Int
    public let ours: String
    public let base: String?
    public let theirs: String

    public init(id: Int, ours: String, base: String?, theirs: String) {
        self.id = id
        self.ours = ours
        self.base = base
        self.theirs = theirs
    }
}

public struct ConflictFile: Sendable, Equatable, Identifiable {
    public let path: String
    public let kind: ConflictKind
    public let ours: ConflictSide
    public let base: ConflictSide
    public let theirs: ConflictSide
    public let workingTree: String?
    public let hunks: [ConflictHunk]

    public var id: String { path }

    public init(
        path: String,
        kind: ConflictKind,
        ours: ConflictSide,
        base: ConflictSide,
        theirs: ConflictSide,
        workingTree: String?,
        hunks: [ConflictHunk]
    ) {
        self.path = path
        self.kind = kind
        self.ours = ours
        self.base = base
        self.theirs = theirs
        self.workingTree = workingTree
        self.hunks = hunks
    }

    /// Contenido completo de un lado del índice. No escribe en disco.
    public func takingWholeSide(_ choice: ConflictChoice) -> String {
        switch choice {
        case .ours: return ours.content ?? ""
        case .base: return base.content ?? ""
        case .theirs: return theirs.content ?? ""
        }
    }
}

public struct RecentRepository: Sendable, Equatable, Identifiable, Codable {
    public let path: String
    public var name: String
    public var lastOpened: Date
    public var isFavorite: Bool
    public var lastBranch: String?

    public var id: String { path }

    public init(path: String, name: String, lastOpened: Date, isFavorite: Bool, lastBranch: String?) {
        self.path = path
        self.name = name
        self.lastOpened = lastOpened
        self.isFavorite = isFavorite
        self.lastBranch = lastBranch
    }
}
