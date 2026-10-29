import Foundation
import GitCore

/// Los prompts van en inglés porque los modelos siguen mejor las convenciones
/// de mensajes de commit. La interfaz sigue en español.
enum PromptBuilder {
    static func commitMessage(diff: String, template: String?) -> String {
        var lines = [
            "Write a Git commit message for the staged diff below.",
            "Rules:",
            "- Output ONLY the commit message. No markdown fences and no preamble.",
            "- First line: subject, imperative mood, at most 72 characters.",
            "- Optional body after one blank line.",
            "- Do not run tools, do not modify files, and do not create the commit.",
        ]
        if let template, !template.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            lines.append("Follow this template when it fits:")
            lines.append(template)
        }
        lines.append("")
        lines.append("Staged diff:")
        lines.append(diff)
        return lines.joined(separator: "\n")
    }

    static func summarize(diff: String, style: SummaryStyle) -> String {
        let shape = style == .pullRequest
            ? "Write a short pull request description with a title line and bullet points."
            : "Summarize the changes as bullet points."
        return """
        \(shape)
        Rules:
        - Output only the summary. No markdown fences.
        - Do not run tools and do not modify files.

        Diff:
        \(diff)
        """
    }

    static func explain(sha: String, subject: String, patch: String) -> String {
        """
        Explain this Git commit in plain language for a developer reading the history.
        Mention behavior changes and possible risks. Do not modify files. Do not run tools.
        Output only the explanation.

        Commit: \(sha)
        Subject: \(subject)

        Patch:
        \(patch)
        """
    }

    static func resolveConflict(_ conflict: ConflictFile) -> String {
        let ours = conflict.ours.content ?? "(lado nuestro ausente)"
        let base = conflict.base.content ?? "(sin ancestro común)"
        let theirs = conflict.theirs.content ?? "(lado entrante ausente)"
        return """
        Resolve a Git merge conflict. Output exactly this shape and nothing else:
        ---FILE---
        <full resolved file contents>
        ---RATIONALE---
        <one short paragraph>

        Rules:
        - The file contents must be the complete file, without conflict markers.
        - Do not run tools and do not write the file yourself. A human will review it.

        Path: \(conflict.path)
        Kind: \(conflict.kind.rawValue)

        OURS:
        \(ours)

        BASE:
        \(base)

        THEIRS:
        \(theirs)
        """
    }

    static func conflictPayload(_ conflict: ConflictFile) -> String {
        [conflict.path, conflict.ours.content ?? "", conflict.base.content ?? "", conflict.theirs.content ?? ""]
            .joined(separator: "\n")
    }
}

enum AIText {
    static func stripFences(_ text: String) -> String {
        var trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.hasPrefix("```") else { return trimmed }
        guard let newline = trimmed.firstIndex(of: "\n") else { return trimmed }
        trimmed = String(trimmed[trimmed.index(after: newline)...])
        if let fence = trimmed.range(of: "```", options: .backwards) {
            trimmed = String(trimmed[..<fence.lowerBound])
        }
        return trimmed.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    static func commitMessage(from raw: String) -> (subject: String, body: String) {
        let text = stripFences(raw)
        let pieces = text.split(separator: "\n", maxSplits: 1, omittingEmptySubsequences: false).map(String.init)
        let subject = pieces.first?.trimmingCharacters(in: .whitespaces) ?? ""
        let body = pieces.count > 1 ? pieces[1].trimmingCharacters(in: .whitespacesAndNewlines) : ""
        return (subject, body)
    }

    static func conflictProposal(from raw: String) -> (content: String, rationale: String?) {
        let text = stripFences(raw)
        guard let fileRange = text.range(of: "---FILE---") else {
            return (ensureTrailingNewline(text), nil)
        }
        let after = text[fileRange.upperBound...]
        if let rationaleRange = after.range(of: "---RATIONALE---") {
            let content = ensureTrailingNewline(String(after[..<rationaleRange.lowerBound]))
            let rationale = String(after[rationaleRange.upperBound...]).trimmingCharacters(in: .whitespacesAndNewlines)
            return (content, rationale.isEmpty ? nil : rationale)
        }
        return (ensureTrailingNewline(String(after)), nil)
    }

    private static func ensureTrailingNewline(_ raw: String) -> String {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty { return "" }
        return trimmed + "\n"
    }
}

enum CodexOutputParser {
    struct Parsed: Equatable {
        let message: String
        let transparencyNote: String?
    }

    static func parse(_ stdout: String) throws -> Parsed {
        let lines = stdout.split(separator: "\n", omittingEmptySubsequences: true).map(String.init)
        var messages: [String] = []
        var failure: String?
        var failed = false
        var completed = false
        var usageNote: String?
        var sawJSON = false

        for line in lines {
            guard let data = line.data(using: .utf8),
                  let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
            else { continue }
            sawJSON = true
            let type = object["type"] as? String ?? ""
            if type == "error" {
                failed = true
                failure = message(in: object) ?? failure
            } else if type == "turn.failed" {
                failed = true
                failure = message(in: object) ?? failure
            } else if type == "turn.completed" {
                completed = true
                if let usage = object["usage"] as? [String: Any] {
                    usageNote = formatUsage(usage)
                }
            } else if type == "item.completed" || type == "item.updated" {
                if let text = agentText(in: object) {
                    messages.append(text)
                }
            }
        }

        if failed && !completed {
            throw AIError.providerFailed(
                .codex,
                exitCode: 1,
                detail: failure ?? "Codex terminó el turno con error."
            )
        }
        if let last = messages.last, !last.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return Parsed(message: last, transparencyNote: usageNote)
        }
        if !sawJSON {
            let plain = stdout.trimmingCharacters(in: .whitespacesAndNewlines)
            if plain.isEmpty { throw AIError.emptyResponse(.codex) }
            return Parsed(message: plain, transparencyNote: nil)
        }
        throw AIError.emptyResponse(.codex)
    }

    private static func agentText(in object: [String: Any]) -> String? {
        guard let item = object["item"] as? [String: Any] else { return nil }
        let itemType = (item["type"] as? String) ?? (item["item_type"] as? String) ?? ""
        guard itemType == "agent_message" || itemType == "assistant_message" else { return nil }
        return item["text"] as? String
    }

    private static func message(in object: [String: Any]) -> String? {
        if let message = object["message"] as? String { return message }
        if let error = object["error"] as? String { return error }
        if let error = object["error"] as? [String: Any] {
            if let message = error["message"] as? String { return message }
        }
        if let item = object["item"] as? [String: Any], let text = item["text"] as? String {
            return text
        }
        return nil
    }

    private static func formatUsage(_ usage: [String: Any]) -> String? {
        let input = usage["input_tokens"] as? Int
        let output = usage["output_tokens"] as? Int
        guard input != nil || output != nil else { return nil }
        return "Codex: \(input ?? 0) tokens de entrada, \(output ?? 0) de salida."
    }
}
