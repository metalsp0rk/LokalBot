import Foundation

/// One thing LokalBot knows about the person: a fact about them, how they want
/// help, a goal, or a principle they decide by. Phase 1 of the personal world
/// model (Docs/personal-world-model-rfc-2026-10-14.md) stores only what the
/// person states, so every claim here is their own words, never an inference.
/// Later phases add evidence-backed and proposed claims as new origins.
struct PersonalClaim: Codable, Identifiable, Equatable, Sendable {
    enum Kind: String, Codable, CaseIterable, Sendable {
        case fact
        case preference
        case goal
        case principle

        var displayName: String {
            switch self {
            case .fact: "Fact"
            case .preference: "Preference"
            case .goal: "Goal"
            case .principle: "Principle"
            }
        }

        var systemImage: String {
            switch self {
            case .fact: "person.text.rectangle"
            case .preference: "slider.horizontal.3"
            case .goal: "flag"
            case .principle: "scale.3d"
            }
        }

        /// A starting kind for a statement typed in Ask. Only obvious English
        /// openings are recognized; principles are never guessed, because the
        /// person decides what counts as one of their values.
        static func suggested(for text: String) -> Kind {
            let lowered = text.lowercased()
            let preferenceOpenings = ["i prefer", "i like", "i don't like", "i do not like", "i dislike",
                                      "answer in", "reply in", "respond in", "always answer", "use "]
            if preferenceOpenings.contains(where: lowered.hasPrefix) { return .preference }
            let goalOpenings = ["my goal", "i want to", "i'm working toward", "i am working toward",
                                "i'm trying to", "i am trying to", "i plan to"]
            if goalOpenings.contains(where: lowered.hasPrefix) { return .goal }
            return .fact
        }
    }

    enum Origin: Codable, Equatable, Sendable {
        /// "Remember …" typed in Ask or Quick Recall.
        case said(conversationID: UUID, messageID: UUID)
        /// Added in Settings → Day Memory → About you.
        case added
    }

    let id: UUID
    var kind: Kind
    var text: String
    let origin: Origin
    /// Included first in every eligible prompt, ahead of relevance ranking.
    var alwaysUse: Bool
    let createdAt: Date
    var updatedAt: Date
    var lastUsedAt: Date?

    init(id: UUID = UUID(), kind: Kind, text: String, origin: Origin,
         alwaysUse: Bool = false, createdAt: Date, updatedAt: Date? = nil,
         lastUsedAt: Date? = nil) {
        self.id = id
        self.kind = kind
        self.text = text
        self.origin = origin
        self.alwaysUse = alwaysUse
        self.createdAt = createdAt
        self.updatedAt = updatedAt ?? createdAt
        self.lastUsedAt = lastUsedAt
    }
}

/// The person's claims plus the rules every edit goes through. Pure value
/// type: the store persists it and the controller publishes it.
struct PersonalModel: Codable, Equatable, Sendable {
    static let currentVersion = 1
    static let maxClaims = 300
    static let maxCharacters = 280

    enum EditError: Error, Equatable, LocalizedError {
        case empty
        case tooLong(Int)
        case full
        case looksSecret
        case notFound
        case unavailable

        var errorDescription: String? {
            switch self {
            case .empty:
                "There's nothing to remember."
            case .tooLong(let count):
                "That's \(count) characters. LokalBot remembers up to \(PersonalModel.maxCharacters); shorten it and try again."
            case .full:
                "LokalBot already remembers \(PersonalModel.maxClaims) things about you. Forget some in Settings → Day Memory first."
            case .looksSecret:
                "This looks like a password, key, or card number. LokalBot doesn't remember those."
            case .notFound:
                "That item was already forgotten."
            case .unavailable:
                "LokalBot couldn't open what it remembers about you, so it won't change it now."
            }
        }
    }

    var version = PersonalModel.currentVersion
    var claims: [PersonalClaim] = []

    /// Prompt-safe text, or why it can't be kept. Over-long text is refused
    /// rather than truncated: a cut sentence can drop a negation.
    static func validatedText(_ raw: String) -> Result<String, EditError> {
        let text = PromptContextSanitizer.sanitize(raw)
        guard !text.isEmpty else { return .failure(.empty) }
        guard text.count <= maxCharacters else { return .failure(.tooLong(text.count)) }
        guard ScreenContextPrivacy.redact(text).count == 0 else { return .failure(.looksSecret) }
        return .success(text)
    }

    /// Case-, width- and diacritic-insensitive key used to merge duplicates.
    static func duplicateKey(_ text: String) -> String {
        var key = text.folding(options: [.caseInsensitive, .diacriticInsensitive, .widthInsensitive],
                               locale: nil)
        key = key.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        while key.hasSuffix(".") { key.removeLast() }
        return key
    }

    /// Adds a claim, or refreshes the existing one when the same text is
    /// already remembered. Saved claims are never evicted to make room.
    @discardableResult
    mutating func add(_ raw: String, kind: PersonalClaim.Kind, origin: PersonalClaim.Origin,
                      now: Date) -> Result<PersonalClaim, EditError> {
        let text: String
        switch Self.validatedText(raw) {
        case .success(let valid): text = valid
        case .failure(let error): return .failure(error)
        }
        let key = Self.duplicateKey(text)
        if let index = claims.firstIndex(where: { Self.duplicateKey($0.text) == key }) {
            claims[index].updatedAt = now
            return .success(claims[index])
        }
        guard claims.count < Self.maxClaims else { return .failure(.full) }
        let claim = PersonalClaim(kind: kind, text: text, origin: origin, createdAt: now)
        claims.append(claim)
        return .success(claim)
    }

    @discardableResult
    mutating func update(id: UUID, text raw: String? = nil, kind: PersonalClaim.Kind? = nil,
                         alwaysUse: Bool? = nil, now: Date) -> Result<PersonalClaim, EditError> {
        guard let index = claims.firstIndex(where: { $0.id == id }) else { return .failure(.notFound) }
        var claim = claims[index]
        if let raw {
            switch Self.validatedText(raw) {
            case .success(let valid): claim.text = valid
            case .failure(let error): return .failure(error)
            }
            let key = Self.duplicateKey(claim.text)
            if claims.contains(where: { $0.id != id && Self.duplicateKey($0.text) == key }) {
                // Editing into an existing claim merges them: keep the edited
                // one, drop the older duplicate.
                claims.removeAll { $0.id != id && Self.duplicateKey($0.text) == key }
            }
        }
        if let kind { claim.kind = kind }
        if let alwaysUse { claim.alwaysUse = alwaysUse }
        claim.updatedAt = now
        guard let current = claims.firstIndex(where: { $0.id == id }) else { return .failure(.notFound) }
        claims[current] = claim
        return .success(claim)
    }

    mutating func remove(ids: Set<UUID>) {
        claims.removeAll { ids.contains($0.id) }
    }

    mutating func markUsed(ids: Set<UUID>, at date: Date) {
        for index in claims.indices where ids.contains(claims[index].id) {
            claims[index].lastUsedAt = date
        }
    }
}

/// Recognizes "Remember …" messages typed in Ask, in English and Serbian
/// (Latin and Cyrillic). Pure; Settings → Add works in any language.
enum RememberIntent {
    private static let triggers = [
        "please remember that ", "please remember: ", "please remember ",
        "remember that ", "remember: ", "remember:", "remember ",
        "molim te zapamti da ", "molim te zapamti ", "zapamti da ", "zapamti: ", "zapamti:", "zapamti ",
        "молим те запамти да ", "молим те запамти ", "запамти да ", "запамти: ", "запамти:", "запамти ",
    ]
    /// After these triggers a question word means the message asks about
    /// the past ("remember when …", "zapamti da li …") rather than stating
    /// something to keep.
    private static let questionCheckedTriggers: Set<String> = [
        "please remember ", "remember ",
        "molim te zapamti da ", "molim te zapamti ", "zapamti da ", "zapamti ",
        "молим те запамти да ", "молим те запамти ", "запамти да ", "запамти ",
    ]
    private static let questionOpenings: Set<String> = [
        "when", "what", "who", "whom", "whose", "where", "how", "why", "if", "whether", "which",
        "kad", "kada", "šta", "sta", "ko", "koga", "gde", "kako", "zašto", "zasto", "li", "koji", "koja",
        "кад", "када", "шта", "ко", "кога", "где", "како", "зашто", "ли", "који", "која",
    ]

    /// The statement to remember, or nil when the message is not a request
    /// to remember something (including every question about the past).
    static func parse(_ message: String) -> String? {
        let trimmed = message.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.hasSuffix("?") else { return nil }
        let lowered = trimmed.lowercased()
        guard let trigger = triggers.first(where: { lowered.hasPrefix($0) }) else { return nil }
        let remainder = String(trimmed.dropFirst(trigger.count))
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !remainder.isEmpty else { return nil }
        if questionCheckedTriggers.contains(trigger) {
            let firstWord = remainder.split(whereSeparator: { !$0.isLetter }).first?.lowercased() ?? ""
            if questionOpenings.contains(firstWord) { return nil }
        }
        return remainder.prefix(1).uppercased() + remainder.dropFirst()
    }
}
