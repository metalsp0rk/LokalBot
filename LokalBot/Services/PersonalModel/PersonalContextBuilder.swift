import Foundation

/// Picks the smallest useful slice of the personal model for one question and
/// renders it as a prompt block. Pure and deterministic, so the same inputs
/// always produce the same prompt and the selection is testable offline.
///
/// Priority, newest first within each step:
/// 1. claims marked Always use;
/// 2. principles and preferences, which apply to most questions;
/// 3. facts and goals sharing a content word with the question or the
///    person's previous two messages, most shared words first;
/// 4. the remaining facts and goals.
/// Claims that don't fit the character budget are skipped, not truncated.
enum PersonalContextBuilder {
    static let characterBudget = 2_000

    struct Selection: Equatable {
        var claims: [PersonalClaim]
        var block: String

        static let empty = Selection(claims: [], block: "")
    }

    static func select(claims: [PersonalClaim], question: String, recentUserTurns: [String] = [],
                       budget: Int = characterBudget, timeZone: TimeZone = .current) -> Selection {
        guard !claims.isEmpty else { return .empty }
        var queryWords = contentWords(question)
        for turn in recentUserTurns.suffix(2) {
            queryWords.formUnion(contentWords(turn))
        }

        let newestFirst = claims.sorted(by: isNewer)
        var ordered: [PersonalClaim] = []
        var placed: Set<UUID> = []
        func place(_ claim: PersonalClaim) {
            guard !placed.contains(claim.id) else { return }
            placed.insert(claim.id)
            ordered.append(claim)
        }
        for claim in newestFirst where claim.alwaysUse { place(claim) }
        for claim in newestFirst where claim.kind == .principle || claim.kind == .preference { place(claim) }
        // Stable sort by shared-word count keeps newest-first among ties.
        var matches: [(claim: PersonalClaim, score: Int)] = []
        for claim in newestFirst where !placed.contains(claim.id) {
            let score = contentWords(claim.text).intersection(queryWords).count
            if score > 0 { matches.append((claim, score)) }
        }
        let bestFirst = matches.enumerated().sorted { lhs, rhs in
            if lhs.element.score != rhs.element.score { return lhs.element.score > rhs.element.score }
            return lhs.offset < rhs.offset
        }
        for match in bestFirst { place(match.element.claim) }
        for claim in newestFirst { place(claim) }

        var chosen: [PersonalClaim] = []
        var lines: [String] = []
        var used = header.count + footer.count
        for claim in ordered {
            let line = self.line(for: claim, timeZone: timeZone)
            guard used + line.count + 1 <= budget else { continue }
            used += line.count + 1
            chosen.append(claim)
            lines.append(line)
        }
        guard !chosen.isEmpty else { return .empty }
        let block = ([header] + lines + [footer]).joined(separator: "\n")
        return Selection(claims: chosen, block: block)
    }

    private static func isNewer(_ lhs: PersonalClaim, _ rhs: PersonalClaim) -> Bool {
        if lhs.updatedAt != rhs.updatedAt { return lhs.updatedAt > rhs.updatedAt }
        return lhs.id.uuidString < rhs.id.uuidString
    }

    static let header = "What the user has told LokalBot about themselves (their own statements, not evidence):"
    static let footer = "Use these only when they bear on the question. They are not sources: never cite them as a meeting or screen source. If retrieved evidence contradicts one, say so. If two conflict, prefer the newer one and point out the conflict."

    static func line(for claim: PersonalClaim, timeZone: TimeZone = .current) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = timeZone
        formatter.dateFormat = "yyyy-MM-dd"
        let text = PromptContextSanitizer.sanitize(claim.text, maxCharacters: PersonalModel.maxCharacters)
        return "- [\(claim.kind.rawValue) · \(formatter.string(from: claim.updatedAt))] \(text)"
    }

    private static let stopwords: Set<String> = [
        "the", "and", "for", "are", "but", "not", "you", "your", "with", "that", "this", "have",
        "has", "had", "was", "were", "will", "would", "should", "could", "what", "when", "where",
        "which", "who", "whom", "why", "how", "about", "from", "into", "than", "then", "them",
        "they", "their", "there", "these", "those", "our", "out", "can", "did", "does", "doing",
        "just", "like", "more", "most", "some", "such", "only", "also", "very", "too", "any",
        "all", "its", "it's", "i'm", "myself", "mine", "me", "my", "his", "her", "him", "she",
        "one", "get", "got", "let", "make", "made", "want", "need", "tell", "know", "think",
        "remember", "please", "always", "never",
    ]

    /// Case- and diacritic-folded words of three or more letters, minus
    /// stopwords. Works for any script, so Serbian Latin and Cyrillic match
    /// themselves.
    static func contentWords(_ text: String) -> Set<String> {
        let folded = text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil)
        var words: Set<String> = []
        for raw in folded.split(whereSeparator: { !$0.isLetter && !$0.isNumber && $0 != "'" }) {
            let word = raw.trimmingCharacters(in: CharacterSet(charactersIn: "'"))
            guard word.count >= 3, !stopwords.contains(word) else { continue }
            words.insert(word)
        }
        return words
    }
}
