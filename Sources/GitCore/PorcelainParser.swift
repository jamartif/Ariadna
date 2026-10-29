import Foundation

/// Interpreta `git status --porcelain=v2 --branch -z`.
/// El formato v2 es estable (a diferencia del texto pensado para humanos).
/// Con `-z` cada registro termina en NUL, así que los espacios del path no rompen el parseo.
enum PorcelainParser {
    static func parse(_ text: String) -> GitStatus {
        var segments = text.split(separator: "\0", omittingEmptySubsequences: false).map(String.init)
        if segments.last?.isEmpty == true {
            segments.removeLast()
        }

        var branchName: String?
        var headOID: String?
        var isDetached = false
        var isUnborn = false
        var upstream: String?
        var ahead = 0
        var behind = 0
        var entries: [StatusEntry] = []

        var index = 0
        while index < segments.count {
            let segment = segments[index]
            if segment.isEmpty {
                index += 1
                continue
            }

            if segment.hasPrefix("# ") {
                let header = segment.dropFirst(2)
                if header.hasPrefix("branch.oid ") {
                    let value = String(header.dropFirst("branch.oid ".count))
                    if value == "(initial)" {
                        isUnborn = true
                        headOID = nil
                    } else {
                        headOID = value
                    }
                } else if header.hasPrefix("branch.head ") {
                    let value = String(header.dropFirst("branch.head ".count))
                    if value == "(detached)" {
                        isDetached = true
                        branchName = nil
                    } else {
                        branchName = value
                    }
                } else if header.hasPrefix("branch.upstream ") {
                    upstream = String(header.dropFirst("branch.upstream ".count))
                } else if header.hasPrefix("branch.ab ") {
                    let parsed = parseAheadBehind(String(header.dropFirst("branch.ab ".count)))
                    ahead = parsed.ahead
                    behind = parsed.behind
                }
                index += 1
                continue
            }

            if segment.hasPrefix("1 ") {
                if let entry = ordinary(segment) { entries.append(entry) }
                index += 1
                continue
            }

            if segment.hasPrefix("2 ") {
                let original = index + 1 < segments.count ? segments[index + 1] : nil
                if let entry = renamed(segment, originalPath: original) { entries.append(entry) }
                index += 2
                continue
            }

            if segment.hasPrefix("u ") {
                if let entry = unmerged(segment) { entries.append(entry) }
                index += 1
                continue
            }

            if segment.hasPrefix("? ") {
                let path = String(segment.dropFirst(2))
                entries.append(simple(path: path, untracked: true, ignored: false))
                index += 1
                continue
            }

            if segment.hasPrefix("! ") {
                let path = String(segment.dropFirst(2))
                entries.append(simple(path: path, untracked: false, ignored: true))
                index += 1
                continue
            }

            index += 1
        }

        return GitStatus(
            branchName: branchName,
            headOID: headOID,
            isDetached: isDetached,
            isUnborn: isUnborn,
            upstream: upstream,
            ahead: ahead,
            behind: behind,
            entries: entries
        )
    }

    private static func ordinary(_ segment: String) -> StatusEntry? {
        guard let fields = splitFields(segment, prefixLength: 2, count: 8) else { return nil }
        return changedEntry(fields: fields, originalPath: nil, rename: false)
    }

    private static func renamed(_ segment: String, originalPath: String?) -> StatusEntry? {
        guard let fields = splitFields(segment, prefixLength: 2, count: 9) else { return nil }
        return changedEntry(fields: fields, originalPath: originalPath, rename: true)
    }

    private static func unmerged(_ segment: String) -> StatusEntry? {
        guard let fields = splitFields(segment, prefixLength: 2, count: 10) else { return nil }
        let xy = fields[0]
        let indexStatus = status(xy, at: 0)
        let workTreeStatus = status(xy, at: 1)
        return StatusEntry(
            path: fields[9],
            originalPath: nil,
            indexStatus: indexStatus,
            workTreeStatus: workTreeStatus,
            isConflicted: true,
            isUntracked: false,
            isIgnored: false,
            isSubmodule: fields[1].hasPrefix("S"),
            conflictKind: ConflictKind(porcelainXY: xy)
        )
    }

    private static func changedEntry(fields: [String], originalPath: String?, rename: Bool) -> StatusEntry {
        let xy = fields[0]
        let indexStatus = status(xy, at: 0)
        let workTreeStatus = status(xy, at: 1)
        let path = rename ? fields[8] : fields[7]
        let conflicted = indexStatus == .unmerged || workTreeStatus == .unmerged
        return StatusEntry(
            path: path,
            originalPath: originalPath,
            indexStatus: indexStatus,
            workTreeStatus: workTreeStatus,
            isConflicted: conflicted,
            isUntracked: false,
            isIgnored: false,
            isSubmodule: fields[1].hasPrefix("S"),
            conflictKind: conflicted ? ConflictKind(porcelainXY: xy) : nil
        )
    }

    private static func simple(path: String, untracked: Bool, ignored: Bool) -> StatusEntry {
        StatusEntry(
            path: path,
            originalPath: nil,
            indexStatus: .unmodified,
            workTreeStatus: untracked ? .untracked : (ignored ? .ignored : .unmodified),
            isConflicted: false,
            isUntracked: untracked,
            isIgnored: ignored,
            isSubmodule: false,
            conflictKind: nil
        )
    }

    /// Parte los campos fijos separados por un espacio. El último campo es el path
    /// y puede contener espacios (`"mi archivo.txt"`).
    private static func splitFields(_ segment: String, prefixLength: Int, count: Int) -> [String]? {
        var rest = segment.dropFirst(prefixLength)
        var fields: [String] = []
        while fields.count < count - 1 {
            guard let space = rest.firstIndex(of: " ") else { return nil }
            let token = rest[..<space]
            if token.isEmpty { return nil }
            fields.append(String(token))
            rest = rest[rest.index(after: space)...]
        }
        if rest.isEmpty { return nil }
        fields.append(String(rest))
        return fields
    }

    private static func status(_ xy: String, at index: Int) -> ChangeStatus {
        guard xy.count >= 2 else { return .unknown }
        let character = xy[xy.index(xy.startIndex, offsetBy: index)]
        return ChangeStatus(porcelain: character)
    }

    private static func parseAheadBehind(_ value: String) -> (ahead: Int, behind: Int) {
        let parts = value.split(separator: " ")
        guard parts.count == 2 else { return (0, 0) }
        let ahead = Int(parts[0].drop(while: { $0 == "+" })) ?? 0
        let behind = Int(parts[1].drop(while: { $0 == "-" })) ?? 0
        return (ahead, behind)
    }
}

enum CommitLogParser {
    /// Formato producido por `git log --pretty=format:` con separadores ASCII.
    /// `%x1f` separa campos y `%x1e` separa commits, porque el cuerpo puede tener saltos de línea.
    static func parse(_ text: String) -> [Commit] {
        let records = text.split(separator: "\u{1e}", omittingEmptySubsequences: true)
        var commits: [Commit] = []
        for record in records {
            let fields = record.split(separator: "\u{1f}", omittingEmptySubsequences: false).map(String.init)
            guard fields.count >= 8 else { continue }
            let parents = fields[1].split(separator: " ").map(String.init).filter { !$0.isEmpty }
            let refs = fields[7]
                .split(separator: ",")
                .map { $0.trimmingCharacters(in: .whitespaces) }
                .filter { !$0.isEmpty }
            commits.append(
                Commit(
                    sha: fields[0],
                    parents: parents,
                    subject: fields[5],
                    body: fields[6].trimmingCharacters(in: .whitespacesAndNewlines),
                    authorName: fields[2],
                    authorEmail: fields[3],
                    authorDateISO: fields[4],
                    refs: refs
                )
            )
        }
        return commits
    }
}

enum BranchListParser {
    static func parse(_ text: String) -> [Branch] {
        let lines = text.split(separator: "\n", omittingEmptySubsequences: true)
        return lines.compactMap { line in
            let fields = line.split(separator: "\0", omittingEmptySubsequences: false).map(String.init)
            guard fields.count >= 5 else { return nil }
            let name = fields[0]
            if name.isEmpty { return nil }
            let headMarker = fields[2].trimmingCharacters(in: .whitespaces)
            let upstream = fields[3].isEmpty ? nil : fields[3]
            let track = parseTrack(fields[4])
            return Branch(
                name: name,
                oid: fields[1],
                isHead: headMarker == "*",
                upstream: upstream,
                ahead: track.ahead,
                behind: track.behind
            )
        }
    }

    static func parseTrack(_ raw: String) -> (ahead: Int, behind: Int) {
        var ahead = 0
        var behind = 0
        let text = raw.replacingOccurrences(of: "[", with: "").replacingOccurrences(of: "]", with: "")
        for piece in text.split(separator: ",") {
            let bits = piece.split(separator: " ").map(String.init)
            guard bits.count == 2, let value = Int(bits[1]) else { continue }
            if bits[0] == "ahead" { ahead = value }
            if bits[0] == "behind" { behind = value }
        }
        return (ahead, behind)
    }
}

enum RemoteListParser {
    static func parse(_ text: String) -> [Remote] {
        var fetch: [String: String] = [:]
        var push: [String: String] = [:]
        for line in text.split(separator: "\n", omittingEmptySubsequences: true) {
            let parts = line.split(separator: "\t", maxSplits: 1, omittingEmptySubsequences: false)
            guard parts.count == 2 else { continue }
            let name = String(parts[0])
            let rest = String(parts[1])
            if rest.hasSuffix(" (fetch)") {
                fetch[name] = String(rest.dropLast(" (fetch)".count))
            } else if rest.hasSuffix(" (push)") {
                push[name] = String(rest.dropLast(" (push)".count))
            }
        }
        return fetch.keys.sorted().map { name in
            Remote(name: name, fetchURL: fetch[name] ?? "", pushURL: push[name] ?? fetch[name] ?? "")
        }
    }
}
