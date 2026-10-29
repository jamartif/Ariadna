import Foundation

enum DiffParser {
    static func parse(_ text: String) -> [FileDiff] {
        if text.isEmpty { return [] }
        // Una línea vacía al final solo existe porque el diff termina en `\n`.
        // En el formato unificado una línea de contexto vacía llega como `" "`, no como `""`.
        var lines = text.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        if lines.last == "" {
            lines.removeLast()
        }
        var files: [FileDiff] = []
        var index = 0
        var fileIndex = 0
        while index < lines.count {
            if lines[index].hasPrefix("diff --git ") {
                let start = index
                index += 1
                while index < lines.count && !lines[index].hasPrefix("diff --git ") {
                    index += 1
                }
                let block = Array(lines[start..<index])
                if let file = parseFile(block, fileIndex: fileIndex) {
                    files.append(file)
                    fileIndex += 1
                }
            } else {
                index += 1
            }
        }
        return files
    }

    private static func parseFile(_ lines: [String], fileIndex: Int) -> FileDiff? {
        guard let gitLine = lines.first, gitLine.hasPrefix("diff --git ") else { return nil }
        var oldPath = pathFromGitLine(gitLine, side: "a")
        var newPath = pathFromGitLine(gitLine, side: "b")
        var isNew = false
        var isDeleted = false
        var headerLines: [String] = []
        var hunkStart: Int?
        var binary = false

        for (offset, line) in lines.enumerated() {
            if line.hasPrefix("@@ ") {
                hunkStart = offset
                break
            }
            headerLines.append(line)
            if line.hasPrefix("--- ") {
                let decoded = decodeDiffPath(String(line.dropFirst(4)))
                if decoded == nil { isNew = true } else { oldPath = decoded }
            } else if line.hasPrefix("+++ ") {
                let decoded = decodeDiffPath(String(line.dropFirst(4)))
                if decoded == nil { isDeleted = true } else { newPath = decoded }
            } else if line.hasPrefix("new file mode") {
                isNew = true
            } else if line.hasPrefix("deleted file mode") {
                isDeleted = true
            } else if line.hasPrefix("Binary files ") || line.hasPrefix("GIT binary patch") {
                binary = true
            }
        }

        let path = newPath ?? oldPath ?? "unknown"
        let raw = join(lines)
        let header = join(headerLines)
        guard !binary, let hunkStart else {
            return FileDiff(
                path: path,
                oldPath: oldPath == path ? nil : oldPath,
                isBinary: binary,
                isNewFile: isNew,
                isDeletedFile: isDeleted,
                header: header,
                hunks: [],
                raw: raw
            )
        }

        var hunks: [DiffHunk] = []
        var cursor = hunkStart
        var hunkIndex = 0
        while cursor < lines.count {
            guard lines[cursor].hasPrefix("@@ ") else { break }
            let headerLine = lines[cursor]
            guard let range = parseHunkHeader(headerLine) else { break }
            var body: [String] = []
            var next = cursor + 1
            while next < lines.count && !lines[next].hasPrefix("@@ ") {
                body.append(lines[next])
                next += 1
            }
            let hunkLines = classify(
                body,
                fileIndex: fileIndex,
                hunkIndex: hunkIndex,
                oldStart: range.oldStart,
                newStart: range.newStart
            )
            let unified = ([headerLine] + body).joined(separator: "\n") + "\n"
            hunks.append(
                DiffHunk(
                    id: "\(path)#\(hunkIndex)#\(range.oldStart)#\(range.newStart)",
                    oldStart: range.oldStart,
                    oldCount: range.oldCount,
                    newStart: range.newStart,
                    newCount: range.newCount,
                    heading: range.heading,
                    lines: hunkLines,
                    unifiedText: unified
                )
            )
            hunkIndex += 1
            cursor = next
        }

        return FileDiff(
            path: path,
            oldPath: oldPath == path ? nil : oldPath,
            isBinary: false,
            isNewFile: isNew,
            isDeletedFile: isDeleted,
            header: header,
            hunks: hunks,
            raw: raw
        )
    }

    private static func classify(_ body: [String], fileIndex: Int, hunkIndex: Int, oldStart: Int, newStart: Int) -> [DiffLine] {
        var oldLine = oldStart
        var newLine = newStart
        var lines: [DiffLine] = []
        for (lineIndex, raw) in body.enumerated() {
            let id = "\(fileIndex):\(hunkIndex):\(lineIndex)"
            if raw.hasPrefix("\\") {
                lines.append(DiffLine(id: id, kind: .meta, text: raw, oldLineNumber: nil, newLineNumber: nil))
                continue
            }
            let prefix = raw.first
            let text = raw.isEmpty ? "" : String(raw.dropFirst())
            switch prefix {
            case "+":
                lines.append(DiffLine(id: id, kind: .addition, text: text, oldLineNumber: nil, newLineNumber: newLine))
                newLine += 1
            case "-":
                lines.append(DiffLine(id: id, kind: .deletion, text: text, oldLineNumber: oldLine, newLineNumber: nil))
                oldLine += 1
            default:
                let context = prefix == " " || prefix == nil ? text : raw
                lines.append(DiffLine(id: id, kind: .context, text: context, oldLineNumber: oldLine, newLineNumber: newLine))
                oldLine += 1
                newLine += 1
            }
        }
        return lines
    }

    private static func parseHunkHeader(_ line: String) -> (oldStart: Int, oldCount: Int, newStart: Int, newCount: Int, heading: String)? {
        // @@ -l,s +l,s @@ optional heading
        guard line.hasPrefix("@@ ") else { return nil }
        let payload = line.dropFirst(3)
        guard let closing = payload.range(of: " @@") else { return nil }
        let ranges = payload[..<closing.lowerBound]
        let heading = payload[closing.upperBound...].trimmingCharacters(in: .whitespaces)
        let parts = ranges.split(separator: " ")
        guard parts.count == 2 else { return nil }
        guard let old = parseRange(parts[0], minus: true), let new = parseRange(parts[1], minus: false) else {
            return nil
        }
        return (old.start, old.count, new.start, new.count, String(heading))
    }

    private static func parseRange(_ token: Substring, minus: Bool) -> (start: Int, count: Int)? {
        var text = token
        if minus {
            guard text.first == "-" else { return nil }
        } else {
            guard text.first == "+" else { return nil }
        }
        text = text.dropFirst()
        let bits = text.split(separator: ",", maxSplits: 1, omittingEmptySubsequences: false)
        guard let start = Int(bits[0]) else { return nil }
        if bits.count == 1 { return (start, 1) }
        guard let count = Int(bits[1]) else { return nil }
        return (start, count)
    }

    private static func pathFromGitLine(_ line: String, side: String) -> String? {
        // diff --git a/old b/new. Buscamos el lado " b/" para no romper paths con espacios.
        let marker = " \(side)/"
        guard let range = line.range(of: marker, options: .backwards) else { return nil }
        let raw = String(line[range.upperBound...])
        return raw.isEmpty ? nil : raw
    }

    private static func decodeDiffPath(_ raw: String) -> String? {
        var text = raw
        if text == "/dev/null" { return nil }
        if text.hasPrefix("\"") && text.hasSuffix("\"") && text.count >= 2 {
            text = String(text.dropFirst().dropLast())
            text = text.replacingOccurrences(of: "\\\"", with: "\"")
            text = text.replacingOccurrences(of: "\\\\", with: "\\")
        }
        if text.hasPrefix("a/") || text.hasPrefix("b/") {
            text = String(text.dropFirst(2))
        }
        return text
    }

    private static func join(_ lines: [String]) -> String {
        guard !lines.isEmpty else { return "" }
        return lines.joined(separator: "\n") + "\n"
    }
}
