import Foundation

enum DiffParser {
    static func parse(_ text: String) -> [FileDiff] {
        if text.isEmpty { return [] }
        // Hay que cortar por el escalar U+000A. En Swift el carácter "\n" no parte
        // un `\r\n`, porque ese par es un solo Character, y entonces dos hunks CRLF
        // se leen como uno.
        let lines = splitLines(text)
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
            } else if line.hasPrefix("rename from ") {
                if let decoded = decodeDiffPath(String(line.dropFirst("rename from ".count))) {
                    oldPath = decoded
                }
            } else if line.hasPrefix("rename to ") {
                if let decoded = decodeDiffPath(String(line.dropFirst("rename to ".count))) {
                    newPath = decoded
                }
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

    /// Separa `diff --git a/<viejo> b/<nuevo>`.
    /// El lado `a/` no puede ir hasta el final de la línea: si el path tiene espacios,
    /// eso se comería también ` b/...` (diffs binarios o de modo, sin `---`/`+++`).
    private static func pathFromGitLine(_ line: String, side: String) -> String? {
        let paths = pathsFromGitHeader(line)
        let chosen = side == "a" ? paths.old : paths.new
        guard let chosen, !chosen.isEmpty else { return nil }
        return chosen
    }

    private static func pathsFromGitHeader(_ line: String) -> (old: String?, new: String?) {
        let prefix = "diff --git "
        guard line.hasPrefix(prefix) else { return (nil, nil) }
        let rest = String(line.dropFirst(prefix.count))
        if rest.hasPrefix("\"") {
            let quoted = quotedTokens(in: rest)
            if quoted.count >= 2 {
                return (stripDiffSide(quoted[0]), stripDiffSide(quoted[1]))
            }
        }
        // Cada ` b/` es un corte posible. Si las dos mitades coinciden, el path
        // contiene literalmente " b/" (p. ej. `docs/a b/readme.txt`) y el primer corte miente.
        let candidates = headerSplits(rest)
        if let identical = candidates.first(where: { $0.old == $0.new }) {
            return identical
        }
        if candidates.count == 1 {
            return candidates[0]
        }
        return (nil, nil)
    }

    private static func headerSplits(_ rest: String) -> [(old: String?, new: String?)] {
        var splits: [(old: String?, new: String?)] = []
        var search = rest.startIndex
        while let marker = rest.range(of: " b/", range: search..<rest.endIndex) {
            let left = String(rest[..<marker.lowerBound])
            guard left.hasPrefix("a/") else {
                search = marker.upperBound
                continue
            }
            let right = String(rest[marker.upperBound...])
                .trimmingCharacters(in: CharacterSet(charactersIn: "\t\r"))
            splits.append((stripDiffSide(left), right.isEmpty ? nil : right))
            search = rest.index(after: marker.lowerBound)
        }
        return splits
    }

    private static func stripDiffSide(_ raw: String) -> String? {
        var text = unescapeGitPath(raw).trimmingCharacters(in: CharacterSet(charactersIn: "\t\r"))
        if text == "/dev/null" { return nil }
        if text.hasPrefix("a/") || text.hasPrefix("b/") {
            text = String(text.dropFirst(2))
        }
        return text.isEmpty ? nil : text
    }

    /// Conserva las barras invertidas. Las comillas escapadas no cierran el token.
    /// La decodificación (octales, `\t`, `\\`) la hace `unescapeGitPath`.
    private static func quotedTokens(in text: String) -> [String] {
        var tokens: [String] = []
        var current = ""
        var inside = false
        var escaped = false
        for character in text {
            if escaped {
                current.append("\\")
                current.append(character)
                escaped = false
                continue
            }
            if character == "\\" && inside {
                escaped = true
                continue
            }
            if character == "\"" {
                if inside {
                    tokens.append(current)
                    current = ""
                }
                inside.toggle()
                continue
            }
            if inside {
                current.append(character)
            }
        }
        return tokens
    }

    private static func decodeDiffPath(_ raw: String) -> String? {
        let trimmed = raw.trimmingCharacters(in: .whitespaces)
        if trimmed == "/dev/null" { return nil }
        return stripDiffSide(trimmed)
    }

    /// Escapes de estilo C que escribe git al citar paths: `\t`, `\n`, `\\`, `\"` y octales `\303`.
    static func unescapeGitPath(_ raw: String) -> String {
        var text = raw.trimmingCharacters(in: CharacterSet(charactersIn: " \t\r"))
        if text.count >= 2, text.hasPrefix("\""), text.hasSuffix("\"") {
            text = String(text.dropFirst().dropLast())
        }
        var bytes: [UInt8] = []
        let scalars = Array(text.unicodeScalars)
        var index = 0
        while index < scalars.count {
            let scalar = scalars[index]
            if scalar == "\\", index + 1 < scalars.count {
                let next = scalars[index + 1]
                switch next {
                case "n": bytes.append(0x0A); index += 2; continue
                case "t": bytes.append(0x09); index += 2; continue
                case "r": bytes.append(0x0D); index += 2; continue
                case "\\": bytes.append(0x5C); index += 2; continue
                case "\"": bytes.append(0x22); index += 2; continue
                case "a": bytes.append(0x07); index += 2; continue
                case "b": bytes.append(0x08); index += 2; continue
                case "f": bytes.append(0x0C); index += 2; continue
                case "v": bytes.append(0x0B); index += 2; continue
                default:
                    if isOctal(next) {
                        var value = 0
                        var consumed = 0
                        var cursor = index + 1
                        while cursor < scalars.count, consumed < 3, isOctal(scalars[cursor]) {
                            value = value * 8 + Int(scalars[cursor].value - Unicode.Scalar("0").value)
                            consumed += 1
                            cursor += 1
                        }
                        if consumed > 0, value <= 255 {
                            bytes.append(UInt8(value))
                            index = cursor
                            continue
                        }
                    }
                    bytes.append(contentsOf: Array(String(next).utf8))
                    index += 2
                    continue
                }
            }
            bytes.append(contentsOf: Array(String(scalar).utf8))
            index += 1
        }
        return String(decoding: bytes, as: UTF8.self)
    }

    private static func isOctal(_ scalar: Unicode.Scalar) -> Bool {
        scalar.value >= Unicode.Scalar("0").value && scalar.value <= Unicode.Scalar("7").value
    }

    private static func splitLines(_ text: String) -> [String] {
        var lines: [String] = []
        var current = String.UnicodeScalarView()
        for scalar in text.unicodeScalars {
            if scalar == "\n" {
                lines.append(String(current))
                current.removeAll(keepingCapacity: true)
            } else {
                current.append(scalar)
            }
        }
        if !current.isEmpty {
            lines.append(String(current))
        }
        if text.unicodeScalars.last == "\n", lines.last == "" {
            lines.removeLast()
        }
        return lines
    }

    private static func join(_ lines: [String]) -> String {
        guard !lines.isEmpty else { return "" }
        return lines.joined(separator: "\n") + "\n"
    }
}
