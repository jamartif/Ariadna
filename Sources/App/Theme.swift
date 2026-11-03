#if os(macOS)
import AppKit
import SwiftUI
import GitCore

/// Piezas visuales compartidas: insignias de estado, avatares, chips de refs y fechas.
enum Palette {
    static let added = Color.green
    static let deleted = Color.red
    static let modified = Color.orange
    static let renamed = Color.blue
    static let untracked = Color.teal
    static let conflict = Color.pink
    static let neutral = Color.gray

    static let addedBackground = Color.green.opacity(0.13)
    static let deletedBackground = Color.red.opacity(0.13)
    static let hunkHeaderBackground = Color.accentColor.opacity(0.08)
    static let cardBackground = Color.primary.opacity(0.035)
    static let cardBorder = Color.primary.opacity(0.08)

    private static let avatarColors: [Color] = [.blue, .purple, .pink, .orange, .teal, .indigo, .mint, .brown]

    /// Color estable para un nombre: la misma persona sale siempre del mismo color.
    static func avatarColor(for name: String) -> Color {
        let sum = name.unicodeScalars.reduce(0) { ($0 &* 31 &+ Int($1.value)) & 0x7fff_ffff }
        return avatarColors[sum % avatarColors.count]
    }
}

/// Letra, color y texto accesible para un estado de archivo.
struct StatusAppearance {
    let letter: String
    let color: Color
    let label: String

    init(_ status: ChangeStatus) {
        let values = Self.values(for: status)
        letter = values.letter
        color = values.color
        label = values.label
    }

    private static func values(for status: ChangeStatus) -> (letter: String, color: Color, label: String) {
        switch status {
        case .modified: return ("M", Palette.modified, "Modificado")
        case .added: return ("A", Palette.added, "Nuevo")
        case .deleted: return ("D", Palette.deleted, "Borrado")
        case .renamed: return ("R", Palette.renamed, "Renombrado")
        case .copied: return ("C", Palette.renamed, "Copiado")
        case .unmerged: return ("!", Palette.conflict, "En conflicto")
        case .typeChanged: return ("T", .purple, "Cambio de tipo")
        case .untracked: return ("U", Palette.untracked, "Sin seguimiento")
        case .ignored: return ("I", Palette.neutral, "Ignorado")
        case .unmodified, .unknown: return ("•", Palette.neutral, "Sin cambios")
        }
    }

    /// Estado que importa según la sección: el del índice en «Preparados», el del árbol en el resto.
    init(entry: StatusEntry, staged: Bool) {
        if entry.isConflicted {
            self.init(.unmerged)
        } else if entry.isUntracked {
            self.init(.untracked)
        } else if entry.isIgnored {
            self.init(.ignored)
        } else {
            self.init(staged ? entry.indexStatus : entry.workTreeStatus)
        }
    }
}

struct StatusBadge: View {
    let appearance: StatusAppearance

    var body: some View {
        Text(appearance.letter)
            .font(.system(size: 10, weight: .bold, design: .rounded))
            .foregroundStyle(.white)
            .frame(width: 18, height: 18)
            .background(appearance.color.gradient, in: RoundedRectangle(cornerRadius: 4, style: .continuous))
            .help(appearance.label)
            .accessibilityLabel(appearance.label)
    }
}

/// Círculo con las iniciales del autor.
struct AuthorAvatar: View {
    let name: String
    var size: CGFloat = 26

    var body: some View {
        Text(initials)
            .font(.system(size: size * 0.4, weight: .semibold, design: .rounded))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(Palette.avatarColor(for: name).gradient, in: Circle())
            .accessibilityHidden(true)
    }

    private var initials: String {
        let words = name.split(whereSeparator: { $0 == " " || $0 == "." || $0 == "-" || $0 == "_" })
        let letters = words.prefix(2).compactMap { $0.first.map { String($0).uppercased() } }
        return letters.isEmpty ? "?" : letters.joined()
    }
}

/// Chip para una ref de `git log --format=%D` (`HEAD -> main`, `tag: v1`, `origin/main`).
struct RefChip: View {
    let raw: String
    var remoteNames: [String] = []

    var body: some View {
        Label(text, systemImage: icon)
            .labelStyle(.titleAndIcon)
            .font(.caption2.weight(.medium))
            .lineLimit(1)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .foregroundStyle(color)
            .background(color.opacity(0.14), in: Capsule())
    }

    private var isHead: Bool { raw.hasPrefix("HEAD") }
    private var isTag: Bool { raw.hasPrefix("tag: ") }
    private var isRemote: Bool {
        !isHead && !isTag && remoteNames.contains { raw.hasPrefix($0 + "/") }
    }

    private var text: String {
        if isHead, let arrow = raw.range(of: "-> ") { return String(raw[arrow.upperBound...]) }
        if isTag { return String(raw.dropFirst(5)) }
        return raw
    }

    private var icon: String {
        if isHead { return "location.fill" }
        if isTag { return "tag.fill" }
        if isRemote { return "cloud.fill" }
        return "arrow.triangle.branch"
    }

    private var color: Color {
        if isHead { return .accentColor }
        if isTag { return .orange }
        if isRemote { return .purple }
        return .green
    }
}

/// Contador `↑2 ↓1` de commits por delante y por detrás del upstream.
struct AheadBehind: View {
    let ahead: Int
    let behind: Int

    var body: some View {
        HStack(spacing: 4) {
            if ahead > 0 {
                Text("↑\(ahead)").foregroundStyle(.green)
            }
            if behind > 0 {
                Text("↓\(behind)").foregroundStyle(.orange)
            }
        }
        .font(.caption2.monospacedDigit().weight(.semibold))
        .accessibilityLabel("\(ahead) por delante, \(behind) por detrás")
    }
}

/// Fondo de tarjeta redondeada que se usa en diffs y detalles.
struct CardBackground: ViewModifier {
    func body(content: Content) -> some View {
        content
            .background(Palette.cardBackground, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).stroke(Palette.cardBorder))
    }
}

extension View {
    func card() -> some View { modifier(CardBackground()) }
}

@MainActor
enum DisplayFormat {
    private static let isoParser: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        return formatter
    }()

    private static let relativeFormatter: RelativeDateTimeFormatter = {
        let formatter = RelativeDateTimeFormatter()
        formatter.locale = Locale(identifier: "es_ES")
        formatter.unitsStyle = .full
        return formatter
    }()

    static func date(fromISO text: String) -> Date? {
        isoParser.date(from: text)
    }

    /// «hace 3 días». Si la fecha no se puede leer, devuelve el texto tal cual.
    static func relative(_ iso: String, now: Date = Date()) -> String {
        guard let date = date(fromISO: iso) else { return iso }
        return relativeFormatter.localizedString(for: date, relativeTo: now)
    }

    /// «5 mar 2026, 10:00».
    static func absolute(_ iso: String) -> String {
        guard let date = date(fromISO: iso) else { return iso }
        return date.formatted(.dateTime.day().month(.abbreviated).year().hour().minute().locale(Locale(identifier: "es_ES")))
    }

    static func fileName(_ path: String) -> String {
        (path as NSString).lastPathComponent
    }

    /// Carpeta del archivo, o cadena vacía si está en la raíz.
    static func directory(_ path: String) -> String {
        (path as NSString).deletingLastPathComponent
    }

    static func copy(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }
}
#endif
