import Foundation
import NaturalLanguage

/// The overnight learning pass (personal world model, Phase 3). After the
/// day's review, it reads only the person's own lines from that day's
/// meetings and asks the Think model for at most three durable things about
/// them. Every result must point at one of those lines and share words with
/// it; anything else is dropped. Facts and goals are saved, preferences wait
/// for review, and principles are never learned (`PersonalModel.learn`).
enum PersonalLearning {
    /// One of the person's own turns: consecutive microphone segments of one
    /// meeting, joined. `handle` is what the model cites.
    struct Utterance: Equatable, Sendable {
        var handle: String
        var meetingID: UUID
        var meetingTitle: String
        var dayKey: String
        var text: String
    }

    static let maxPerNight = 3
    static let maxUtterances = 40
    static let utteranceCharacterBudget = 6_000
    static let maxTurnCharacters = 400
    static let minimumWords = 5
    static let maxOutputTokens = 1_024

    // MARK: - The person's lines

    /// The person's turns from `day`'s meetings, most self-descriptive first
    /// within the budget, then put back in meeting order with handles. A
    /// meeting whose microphone was shared by several diarized local voices is
    /// skipped: there is no telling which of them is the person.
    static func utterances(day: Date, storageRoot: URL,
                           calendar: Calendar = .current) throws -> [Utterance] {
        let start = calendar.startOfDay(for: day)
        let end = calendar.date(byAdding: .day, value: 1, to: start) ?? start.addingTimeInterval(86_400)
        let meetings = try SessionLookup.loadAllMeetings(root: storageRoot)
            .filter { $0.startedAt >= start && $0.startedAt < end }
            .sorted { $0.startedAt < $1.startedAt }
        let dayKey = DreamDay.key(for: start, calendar: calendar)
        var turns: [(order: Int, score: Int, utterance: Utterance)] = []
        for meeting in meetings {
            let url = storageRoot.appendingPathComponent(meeting.relativePath, isDirectory: true)
                .appendingPathComponent("transcript.json")
            guard let data = try? Data(contentsOf: url),
                  let transcript = try? JSONDecoder().decode(Transcript.self, from: data) else { continue }
            for text in ownTurns(in: transcript.segments) {
                let utterance = Utterance(handle: "", meetingID: meeting.id, meetingTitle: meeting.title,
                                          dayKey: dayKey, text: text)
                turns.append((turns.count, selfReferenceScore(text), utterance))
            }
        }
        let ranked = turns.sorted { lhs, rhs in
            if lhs.score != rhs.score { return lhs.score > rhs.score }
            return lhs.order < rhs.order
        }
        var chosen: [(order: Int, utterance: Utterance)] = []
        var used = 0
        for turn in ranked where turn.score > 0 {
            guard chosen.count < maxUtterances, used + turn.utterance.text.count <= utteranceCharacterBudget else {
                continue
            }
            used += turn.utterance.text.count
            chosen.append((turn.order, turn.utterance))
        }
        var result: [Utterance] = []
        for (index, turn) in chosen.sorted(by: { $0.order < $1.order }).enumerated() {
            var utterance = turn.utterance
            utterance.handle = "U\(index + 1)"
            result.append(utterance)
        }
        return result
    }

    /// Joins consecutive segments of the person's own speech into turns.
    static func ownTurns(in segments: [Transcript.Segment]) -> [String] {
        let localKeys = Set(segments.map { Transcript.canonicalSpeakerKey($0.speaker) }
            .filter { $0 == "local" || $0.hasPrefix("local ") })
        let sharedMicrophone = localKeys.count > 1
        func isPerson(_ segment: Transcript.Segment) -> Bool {
            let key = Transcript.canonicalSpeakerKey(segment.speaker)
            return key == "me" || (key == "local" && !sharedMicrophone)
        }
        var turns: [String] = []
        var current = ""
        func flush() {
            let text = PromptContextSanitizer.sanitize(current, maxCharacters: maxTurnCharacters)
            if text.split(whereSeparator: \.isWhitespace).count >= minimumWords { turns.append(text) }
            current = ""
        }
        for segment in segments {
            if isPerson(segment) {
                current += (current.isEmpty ? "" : " ") + segment.text
            } else if !current.isEmpty {
                flush()
            }
        }
        if !current.isEmpty { flush() }
        return turns
    }

    private static let selfReferences: Set<String> = [
        "i", "i'm", "im", "i've", "i'd", "i'll", "my", "me", "mine", "myself",
        "ja", "moj", "moja", "moje", "moji", "mene", "meni", "sam", "imam", "zivim", "radim",
        "zelim", "planiram", "hocu",
        "ја", "мој", "моја", "моје", "мене", "мени", "сам", "имам", "живим", "радим", "желим", "планирам", "хоћу",
    ]

    /// How many first-person words a line has (English and Serbian). Lines
    /// without any are not about the person and are never sent.
    static func selfReferenceScore(_ text: String) -> Int {
        let folded = text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil)
        var score = 0
        for raw in folded.split(whereSeparator: { !$0.isLetter && $0 != "'" }) {
            let word = raw.trimmingCharacters(in: CharacterSet(charactersIn: "'"))
            if selfReferences.contains(word) { score += 1 }
        }
        return score
    }

    // MARK: - Prompt

    static let system = """
        You keep a small private profile of the user from things they said about themselves. \
        You read only the user's own lines from their meetings; nobody else's speech is included. \
        Reply with JSON only.
        """

    static func prompt(utterances: [Utterance], known: [PersonalClaim]) -> String {
        var lines: [String] = []
        lines.append("""
            From the user's lines below, pick at most \(maxPerNight) things about the user that will still be \
            true and useful in a month:
            - fact: their life, role, work, circumstances, or relationships ("I live in Belgrade", \
            "I lead the platform team", "I have two kids").
            - goal: a longer-term goal they set for themselves ("I want to run a marathon next year").
            - preference: how they like to work or be helped ("I prefer written updates to calls").

            Rules:
            - Use only what the line clearly says about the user. Never guess, never turn one remark into \
            a trait, and never infer their values, beliefs, or personality.
            - Skip project status, tasks, deadlines, meeting logistics, small talk, jokes, hypotheticals, \
            quotes of other people, and anything about other people's private lives.
            - Skip health, money, religion, politics, and sexuality.
            - Skip anything already in the known list, even if worded differently.
            - Write each item as one short first-person sentence under 200 characters, built from the \
            line's own words: shorten it rather than rephrase it, and never translate it (a Serbian line stays \
            Serbian, an English line stays English). Drop filler \
            ("honestly", "so", "haha"). Turn a relative time into the year it means ("next year" becomes \
            "in 2027"); never invent a day or month.
            - Return {"items": []} when nothing qualifies. That is the usual answer.

            Reply as {"items": [{"line": "U3", "kind": "fact", "text": "I live in Belgrade."}]}.
            """)
        if !known.isEmpty {
            lines.append("")
            lines.append("Already known (do not repeat):")
            for claim in known.prefix(40) {
                lines.append("- \(PromptContextSanitizer.sanitize(claim.text, maxCharacters: PersonalModel.maxCharacters))")
            }
        }
        lines.append("")
        lines.append("The user's lines, from meetings on \(utterances.first?.dayKey ?? "the reviewed day"):")
        for utterance in utterances {
            let title = PromptContextSanitizer.sanitize(utterance.meetingTitle, maxCharacters: 80)
            lines.append("\(utterance.handle) (\(title)): \(utterance.text)")
        }
        return lines.joined(separator: "\n")
    }

    static var schema: [String: Any] {
        [
            "type": "object",
            "properties": [
                "items": [
                    "type": "array",
                    "items": [
                        "type": "object",
                        "properties": [
                            "line": ["type": "string"],
                            "kind": ["type": "string", "enum": ["fact", "goal", "preference"]],
                            "text": ["type": "string"],
                        ],
                        "required": ["line", "kind", "text"],
                    ],
                ],
            ],
            "required": ["items"],
        ]
    }

    // MARK: - Reading the reply

    private struct Reply: Decodable {
        struct Item: Decodable {
            var line: String
            var kind: String
            var text: String
        }
        var items: [Item]
    }

    /// An item the model proposed, tied to the line it cites.
    struct Proposal: Equatable, Sendable {
        var kind: PersonalClaimKind
        var text: String
        var utterance: Utterance
    }

    /// Every item that cites a real line with a learnable kind, in reply
    /// order. Grounding happens after this.
    static func proposals(from output: String, utterances: [Utterance]) -> [Proposal] {
        guard let start = output.firstIndex(of: "{"), let end = output.lastIndex(of: "}"),
              let reply = try? JSONDecoder().decode(Reply.self, from: Data(output[start...end].utf8)) else {
            return []
        }
        var byHandle: [String: Utterance] = [:]
        for utterance in utterances where byHandle[utterance.handle.uppercased()] == nil {
            byHandle[utterance.handle.uppercased()] = utterance
        }
        var result: [Proposal] = []
        for item in reply.items {
            guard let utterance = byHandle[item.line.trimmingCharacters(in: .whitespaces).uppercased()],
                  let kind = PersonalClaimKind(rawValue: item.kind.lowercased()), kind != .principle else { continue }
            result.append(Proposal(kind: kind, text: item.text, utterance: utterance))
        }
        return result
    }

    /// Word-overlap grounding only: what `run` falls back to without an
    /// embedder. At most `maxPerNight`.
    static func candidates(from output: String, utterances: [Utterance],
                           evidence: (Utterance) -> DreamEvidenceProvenance) -> [PersonalModel.LearningCandidate] {
        var result: [PersonalModel.LearningCandidate] = []
        for proposal in proposals(from: output, utterances: utterances) where result.count < maxPerNight {
            guard isGrounded(proposal.text, in: proposal.utterance.text) else { continue }
            result.append(candidate(proposal, needsReview: false, evidence: evidence))
        }
        return result
    }

    static func isGrounded(_ claim: String, in line: String) -> Bool {
        let claimWords = PersonalContextSelection.contentWords(claim)
        guard !claimWords.isEmpty else { return false }
        let shared = claimWords.intersection(PersonalContextSelection.contentWords(line)).count
        return shared >= max(1, Int((Double(claimWords.count) / 3).rounded(.up)))
    }

    // MARK: - Grounding across languages

    /// Vectors for texts, L2-normalized, or nil when no embedder is available.
    typealias Embed = ([String]) async -> [[Float]]?

    /// EmbeddingGemma 2 with the sentence-similarity prompt, measured on
    /// Serbian lines against English items (2026-10-14): correct
    /// translations 0.83–0.95, unrelated claims 0.73–0.79, and a wrong
    /// detail ("doctor" for engineer, the wrong city) 0.80–0.90. Below the
    /// floor an item is dropped. Above it, a fact or goal is saved only with a
    /// shared word root or a very high score; otherwise it waits for review,
    /// because embeddings alone can't tell a wrong detail from a right one.
    /// Recalibrate whenever the embedding model changes.
    static let crossLanguageFloor: Float = 0.82
    static let confidentSimilarity: Float = 0.90

    enum Grounding: Equatable {
        case grounded
        case needsReview
        case rejected
    }

    /// Word overlap for items in the line's language. An item the model
    /// translated (the line is Serbian, the item English) can't share words,
    /// so it is compared by meaning instead, when an embedder is available.
    static func groundedCandidates(
        from output: String, utterances: [Utterance], embed: Embed?,
        evidence: (Utterance) -> DreamEvidenceProvenance
    ) async -> [PersonalModel.LearningCandidate] {
        let all = proposals(from: output, utterances: utterances)
        var verdicts = all.map { proposal -> Grounding? in
            if isGrounded(proposal.text, in: proposal.utterance.text) { return .grounded }
            return isCrossLanguage(proposal.text, proposal.utterance.text) ? nil : .rejected
        }
        let pending = verdicts.indices.filter { verdicts[$0] == nil }
        if !pending.isEmpty {
            var texts: [String] = []
            for index in pending {
                texts.append(all[index].text)
                texts.append(all[index].utterance.text)
            }
            let vectors = await embed?(texts)
            for (offset, index) in pending.enumerated() {
                guard let vectors, vectors.count == texts.count else {
                    verdicts[index] = .rejected
                    continue
                }
                let similarity = dot(vectors[offset * 2], vectors[offset * 2 + 1])
                verdicts[index] = crossLanguageVerdict(
                    similarity: similarity, claim: all[index].text, line: all[index].utterance.text)
            }
        }
        var result: [PersonalModel.LearningCandidate] = []
        for (proposal, verdict) in zip(all, verdicts) where result.count < maxPerNight {
            switch verdict {
            case .grounded: result.append(candidate(proposal, needsReview: false, evidence: evidence))
            case .needsReview: result.append(candidate(proposal, needsReview: true, evidence: evidence))
            case .rejected, nil: continue
            }
        }
        return result
    }

    static func crossLanguageVerdict(similarity: Float, claim: String, line: String) -> Grounding {
        guard similarity >= crossLanguageFloor else { return .rejected }
        if similarity >= confidentSimilarity || sharesWordRoot(claim, line) { return .grounded }
        return .needsReview
    }

    /// True when the two texts' dominant languages differ, so word overlap
    /// can't apply. Unknown counts as the same language: the stricter path.
    static func isCrossLanguage(_ claim: String, _ line: String) -> Bool {
        guard let claimLanguage = NLLanguageRecognizer.dominantLanguage(for: claim),
              let lineLanguage = NLLanguageRecognizer.dominantLanguage(for: line) else { return false }
        return claimLanguage != lineLanguage
    }

    /// A content word of the item and of the line (Cyrillic transliterated)
    /// start the same way: "platform" in "platformu", "marathon" in
    /// "maraton", "sad" in "sadu".
    static func sharesWordRoot(_ claim: String, _ line: String) -> Bool {
        let latinLine = line.applyingTransform(.toLatin, reverse: false) ?? line
        let lineWords = PersonalContextSelection.contentWords(latinLine)
        for word in PersonalContextSelection.contentWords(claim) {
            for other in lineWords {
                let shared = zip(word, other).prefix { $0 == $1 }.count
                if shared >= 4 || (shared >= 3 && shared == min(word.count, other.count)) { return true }
            }
        }
        return false
    }

    private static func dot(_ lhs: [Float], _ rhs: [Float]) -> Float {
        var sum: Float = 0
        for index in 0..<min(lhs.count, rhs.count) { sum += lhs[index] * rhs[index] }
        return sum
    }

    private static func candidate(_ proposal: Proposal, needsReview: Bool,
                                  evidence: (Utterance) -> DreamEvidenceProvenance) -> PersonalModel.LearningCandidate {
        PersonalModel.LearningCandidate(
            kind: proposal.kind, text: proposal.text, quote: proposal.utterance.text,
            meetingID: proposal.utterance.meetingID, dayKey: proposal.utterance.dayKey,
            evidence: evidence(proposal.utterance), needsReview: needsReview)
    }

    // MARK: - Run

    /// One night's pass. Returns no candidates without calling the model when
    /// the day has none of the person's own lines.
    static func run(day: Date, storageRoot: URL, known: [PersonalClaim], revision: UInt64,
                    engine: TextEngine, embed: Embed? = nil, calendar: Calendar = .current) async throws
        -> [PersonalModel.LearningCandidate] {
        let lines = try utterances(day: day, storageRoot: storageRoot, calendar: calendar)
        guard !lines.isEmpty else { return [] }
        let output = try await engine.generate(
            system: system, prompt: prompt(utterances: lines, known: known), context: [],
            schema: schema,
            options: TextGenerationOptions(maxTokens: maxOutputTokens, reasoningBudgetTokens: 0, temperature: 0))
        return await groundedCandidates(from: output, utterances: lines, embed: embed) { utterance in
            DreamEvidenceProvenance(
                sources: [DreamEvidenceSource(kind: .meeting, id: utterance.meetingID.uuidString,
                                              dayKey: utterance.dayKey)],
                revision: revision)
        }
    }
}
