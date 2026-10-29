import Foundation

/// Marcadores de conflicto (`<<<<<<<`, `|||||||`, `=======`, `>>>>>>>`).
/// El modelo a tres vías de verdad son las etapas del índice; estos marcadores
/// sirven para el editor y para elegir un lado hunk a hunk sin escribir disco.
public enum ConflictMarkers {
    public static func hunks(in workingTree: String) -> [ConflictHunk] {
        parse(workingTree).hunks
    }

    /// `choices` indica, por índice de hunk, qué lado conservar.
    /// Los hunks sin elección se dejan con sus marcadores.
    public static func resolve(_ workingTree: String, choices: [Int: ConflictChoice]) -> String {
        let parsed = parse(workingTree)
        var output = ""
        var cursor = workingTree.startIndex
        for region in parsed.regions {
            output.append(contentsOf: workingTree[cursor..<region.start])
            if let choice = choices[region.hunk.id] {
                output.append(text(for: choice, hunk: region.hunk))
            } else {
                output.append(contentsOf: workingTree[region.start..<region.end])
            }
            cursor = region.end
        }
        output.append(contentsOf: workingTree[cursor...])
        return output
    }

    private struct Region {
        let hunk: ConflictHunk
        let start: String.Index
        let end: String.Index
    }

    private struct Parsed {
        var hunks: [ConflictHunk] = []
        var regions: [Region] = []
    }

    private static func parse(_ text: String) -> Parsed {
        let lines = splitKeepingEnds(text)
        var parsed = Parsed()
        var index = 0
        var hunkID = 0
        while index < lines.count {
            let content = lines[index].content
            if content.hasPrefix("<<<<<<<") {
                let start = lines[index].start
                var ours: [String] = []
                var base: [String] = []
                var theirs: [String] = []
                var sawBase = false
                var phase = 1 // 1 ours, 2 base, 3 theirs
                index += 1
                var closed = false
                while index < lines.count {
                    let line = lines[index].content
                    if phase == 1 && line.hasPrefix("|||||||") {
                        sawBase = true
                        phase = 2
                    } else if (phase == 1 || phase == 2) && line.hasPrefix("=======") {
                        phase = 3
                    } else if phase == 3 && line.hasPrefix(">>>>>>>") {
                        let end = lines[index].end
                        let hunk = ConflictHunk(
                            id: hunkID,
                            ours: join(ours),
                            base: sawBase ? join(base) : nil,
                            theirs: join(theirs)
                        )
                        parsed.hunks.append(hunk)
                        parsed.regions.append(Region(hunk: hunk, start: start, end: end))
                        hunkID += 1
                        index += 1
                        closed = true
                        break
                    } else {
                        switch phase {
                        case 1: ours.append(lines[index].raw)
                        case 2: base.append(lines[index].raw)
                        default: theirs.append(lines[index].raw)
                        }
                    }
                    index += 1
                }
                if !closed { break }
            } else {
                index += 1
            }
        }
        return parsed
    }

    private struct Line {
        let content: String
        let raw: String
        let start: String.Index
        let end: String.Index
    }

    private static func splitKeepingEnds(_ text: String) -> [Line] {
        var lines: [Line] = []
        var cursor = text.startIndex
        while cursor < text.endIndex {
            let start = cursor
            if let newline = text[cursor...].firstIndex(of: "\n") {
                let contentEnd = newline
                let end = text.index(after: newline)
                let raw = String(text[start..<end])
                let content = String(text[start..<contentEnd])
                lines.append(Line(content: content, raw: raw, start: start, end: end))
                cursor = end
            } else {
                let raw = String(text[start...])
                lines.append(Line(content: raw, raw: raw, start: start, end: text.endIndex))
                break
            }
        }
        return lines
    }

    private static func join(_ rawLines: [String]) -> String {
        rawLines.joined()
    }

    private static func text(for choice: ConflictChoice, hunk: ConflictHunk) -> String {
        switch choice {
        case .ours: return hunk.ours
        case .base: return hunk.base ?? ""
        case .theirs: return hunk.theirs
        }
    }
}
