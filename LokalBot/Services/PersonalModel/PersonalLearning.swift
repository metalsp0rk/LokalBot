import Foundation

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

    /// Grounded candidates from the model's reply: the line must exist, the
    /// kind must be fact, goal, or preference, and the text must share at least
    /// a third of its content words with that line. At most `maxPerNight`.
    static func candidates(from output: String, utterances: [Utterance],
                           evidence: (Utterance) -> DreamEvidenceProvenance) -> [PersonalModel.LearningCandidate] {
        guard let start = output.firstIndex(of: "{"), let end = output.lastIndex(of: "}"),
              let reply = try? JSONDecoder().decode(Reply.self, from: Data(output[start...end].utf8)) else {
            return []
        }
        let byHandle = Dictionary(utterances.map { ($0.handle.uppercased(), $0) }, uniquingKeysWith: { first, _ in first })
        var result: [PersonalModel.LearningCandidate] = []
        for item in reply.items {
            guard result.count < maxPerNight,
                  let utterance = byHandle[item.line.trimmingCharacters(in: .whitespaces).uppercased()],
                  let kind = PersonalClaimKind(rawValue: item.kind.lowercased()), kind != .principle,
                  isGrounded(item.text, in: utterance.text) else { continue }
            result.append(PersonalModel.LearningCandidate(
                kind: kind, text: item.text, quote: utterance.text, meetingID: utterance.meetingID,
                dayKey: utterance.dayKey, evidence: evidence(utterance)))
        }
        return result
    }

    static func isGrounded(_ claim: String, in line: String) -> Bool {
        let claimWords = PersonalContextSelection.contentWords(claim)
        guard !claimWords.isEmpty else { return false }
        let shared = claimWords.intersection(PersonalContextSelection.contentWords(line)).count
        return shared >= max(1, Int((Double(claimWords.count) / 3).rounded(.up)))
    }

    // MARK: - Run

    /// One night's pass. Returns no candidates without calling the model when
    /// the day has none of the person's own lines.
    static func run(day: Date, storageRoot: URL, known: [PersonalClaim], revision: UInt64,
                    engine: TextEngine, calendar: Calendar = .current) async throws
        -> [PersonalModel.LearningCandidate] {
        let lines = try utterances(day: day, storageRoot: storageRoot, calendar: calendar)
        guard !lines.isEmpty else { return [] }
        let output = try await engine.generate(
            system: system, prompt: prompt(utterances: lines, known: known), context: [],
            schema: schema,
            options: TextGenerationOptions(maxTokens: maxOutputTokens, reasoningBudgetTokens: 0, temperature: 0))
        return candidates(from: output, utterances: lines) { utterance in
            DreamEvidenceProvenance(
                sources: [DreamEvidenceSource(kind: .meeting, id: utterance.meetingID.uuidString,
                                              dayKey: utterance.dayKey)],
                revision: revision)
        }
    }
}
