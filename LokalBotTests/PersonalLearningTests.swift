import CryptoKit
import XCTest
@testable import LokalBot

/// Phase 3 of the personal world model: learning facts and goals from the
/// person's own meeting lines, review of learned preferences, and retraction
/// when the meeting is deleted or corrected.
final class PersonalLearningLinesTests: XCTestCase {
    private func segment(_ speaker: String, _ text: String) -> Transcript.Segment {
        Transcript.Segment(start: 0, end: 1, speaker: speaker, text: text, confidence: nil)
    }

    func testOwnTurnsJoinConsecutiveMicrophoneSegmentsAndDropShortOnes() {
        let turns = PersonalLearning.ownTurns(in: [
            segment("me", "I moved to Belgrade"),
            segment("me", "with my family last spring."),
            segment("them", "Nice, how is it going?"),
            segment("me", "Good, thanks."),
            segment("Them", "And the release?"),
            segment("ME", "I want to run a marathon next year, honestly."),
        ])
        XCTAssertEqual(turns, [
            "I moved to Belgrade with my family last spring.",
            "I want to run a marathon next year, honestly.",
        ])
    }

    func testSharedMicrophoneIsSkippedButASingleLocalVoiceCounts() {
        let shared = PersonalLearning.ownTurns(in: [
            segment("local", "I live in Novi Sad with my two kids."),
            segment("local 2", "I have been at the company for ten years."),
        ])
        XCTAssertEqual(shared, [])

        let single = PersonalLearning.ownTurns(in: [
            segment("local", "I live in Novi Sad with my two kids."),
            segment("remote", "Great to hear."),
        ])
        XCTAssertEqual(single, ["I live in Novi Sad with my two kids."])
    }

    func testSelfReferenceScoreCoversEnglishAndSerbian() {
        XCTAssertGreaterThan(PersonalLearning.selfReferenceScore("I'm leading the platform team now"), 0)
        XCTAssertGreaterThan(PersonalLearning.selfReferenceScore("Ja živim u Beogradu sa porodicom"), 0)
        XCTAssertGreaterThan(PersonalLearning.selfReferenceScore("Ја живим у Београду"), 0)
        XCTAssertEqual(PersonalLearning.selfReferenceScore("The deploy failed again on Tuesday"), 0)
    }

    func testUtterancesReadOnlyThatDaysOwnLinesWithHandles() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("PersonalLearningLines-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        let day = Date(timeIntervalSince1970: 1_790_000_000)
        let meetingID = UUID()
        try MeetingFixture.write([
            .init(id: meetingID, title: "Weekly sync", startedAt: day, transcriptLines: [
                "I live in Belgrade and I work from home most days.",
                "Where do you live these days, by the way?",
                "The deploy failed again on Tuesday afternoon, sadly.",
            ]),
            .init(title: "Yesterday", startedAt: day.addingTimeInterval(-86_400), transcriptLines: [
                "I used to live in Berlin before moving here.",
            ]),
        ], under: root)

        let lines = try PersonalLearning.utterances(day: day, storageRoot: root, calendar: calendar)
        XCTAssertEqual(lines.map(\.text), ["I live in Belgrade and I work from home most days."])
        XCTAssertEqual(lines.first?.handle, "U1")
        XCTAssertEqual(lines.first?.meetingID, meetingID)
        XCTAssertEqual(lines.first?.meetingTitle, "Weekly sync")
    }
}

final class PersonalLearningReplyTests: XCTestCase {
    private let meetingID = UUID()
    private var lines: [PersonalLearning.Utterance] {
        [
            .init(handle: "U1", meetingID: meetingID, meetingTitle: "Sync", dayKey: "2026-10-14",
                  text: "I live in Belgrade and I work from home most days."),
            .init(handle: "U2", meetingID: meetingID, meetingTitle: "Sync", dayKey: "2026-10-14",
                  text: "My goal is to run a marathon next year."),
            .init(handle: "U3", meetingID: meetingID, meetingTitle: "Sync", dayKey: "2026-10-14",
                  text: "I prefer written updates over calls for status."),
        ]
    }

    private func parse(_ output: String) -> [PersonalModel.LearningCandidate] {
        PersonalLearning.candidates(from: output, utterances: lines) { utterance in
            DreamEvidenceProvenance(
                sources: [DreamEvidenceSource(kind: .meeting, id: utterance.meetingID.uuidString,
                                              dayKey: utterance.dayKey)],
                revision: 7)
        }
    }

    func testGroundedItemsBecomeCandidatesWithEvidence() throws {
        let candidates = parse("""
            {"items": [
              {"line": "U1", "kind": "fact", "text": "I live in Belgrade."},
              {"line": "u2", "kind": "goal", "text": "I want to run a marathon next year."}
            ]}
            """)
        XCTAssertEqual(candidates.map(\.text), ["I live in Belgrade.", "I want to run a marathon next year."])
        XCTAssertEqual(candidates.map(\.kind), [.fact, .goal])
        let first = try XCTUnwrap(candidates.first)
        XCTAssertEqual(first.quote, "I live in Belgrade and I work from home most days.")
        XCTAssertEqual(first.evidence.sources.map(\.id), [meetingID.uuidString])
        XCTAssertEqual(first.evidence.revision, 7)
    }

    func testUngroundedUnknownPrincipleAndExtraItemsAreDropped() {
        let candidates = parse("""
            Sure! {"items": [
              {"line": "U9", "kind": "fact", "text": "I live in Belgrade."},
              {"line": "U1", "kind": "principle", "text": "I live in Belgrade."},
              {"line": "U1", "kind": "fact", "text": "I am an excellent chess player."},
              {"line": "U1", "kind": "fact", "text": "I live in Belgrade."},
              {"line": "U2", "kind": "goal", "text": "I want to run a marathon."},
              {"line": "U3", "kind": "preference", "text": "I prefer written updates."},
              {"line": "U1", "kind": "fact", "text": "I work from home most days."}
            ]}
            """)
        XCTAssertEqual(candidates.map(\.text), [
            "I live in Belgrade.", "I want to run a marathon.", "I prefer written updates.",
        ])
        XCTAssertEqual(parse("not json"), [])
        XCTAssertEqual(parse(#"{"items": []}"#), [])
    }

    func testPromptListsOnlyTheLinesAndKnownItems() {
        let known = PersonalClaim(kind: .fact, text: "Maya is my manager", origin: .added, createdAt: Date())
        let prompt = PersonalLearning.prompt(utterances: lines, known: [known])
        XCTAssertTrue(prompt.contains("U1 (Sync): I live in Belgrade"))
        XCTAssertTrue(prompt.contains("- Maya is my manager"))
        XCTAssertTrue(prompt.contains("never infer their values"))
    }
}

final class PersonalModelLearningRulesTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_790_000_000)
    private let meetingID = UUID()

    private func candidate(_ text: String, _ kind: PersonalClaimKind) -> PersonalModel.LearningCandidate {
        PersonalModel.LearningCandidate(
            kind: kind, text: text, quote: "quote: \(text)", meetingID: meetingID, dayKey: "2026-10-14",
            evidence: DreamEvidenceProvenance(
                sources: [DreamEvidenceSource(kind: .meeting, id: meetingID.uuidString, dayKey: "2026-10-14")],
                revision: 3))
    }

    func testFactsAndGoalsAreSavedPreferencesWaitPrinciplesNever() {
        var model = PersonalModel()
        let outcome = model.learn([
            candidate("I live in Belgrade.", .fact),
            candidate("I want to run a marathon next year.", .goal),
            candidate("I prefer written updates.", .preference),
            candidate("Family comes first.", .principle),
        ], now: now)
        XCTAssertEqual(outcome.saved.map(\.text), ["I live in Belgrade.", "I want to run a marathon next year."])
        XCTAssertEqual(outcome.proposed.map(\.text), ["I prefer written updates."])
        XCTAssertEqual(outcome.skipped, 1)
        let saved = model.claims.filter { $0.status == .confirmed }
        XCTAssertTrue(saved.allSatisfy { $0.authority == .learned && $0.evidence != nil && $0.isLearned })
        XCTAssertEqual(model.claims.first { $0.kind == .preference }?.status, .proposed)
    }

    func testDuplicatesForgottenAndDismissedTextAreNotLearnedAgain() throws {
        var model = PersonalModel()
        _ = model.add("I live in Belgrade", kind: .fact, origin: .added, now: now)
        model.learn([candidate("i live in belgrade.", .fact), candidate("I have two kids.", .fact),
                     candidate("I prefer email.", .preference)], now: now)
        let kids = try XCTUnwrap(model.claims.first { $0.text == "I have two kids." })
        let email = try XCTUnwrap(model.claims.first { $0.text == "I prefer email." })
        XCTAssertEqual(model.claims.count, 3)

        model.remove(ids: [kids.id], now: now)
        model.dismiss(id: email.id, now: now)
        XCTAssertEqual(model.tombstones.count, 2)
        let again = model.learn([candidate("I have two kids", .fact), candidate("I prefer email", .preference)], now: now)
        XCTAssertEqual(again.saved.count + again.proposed.count, 0)
        XCTAssertFalse(model.tombstones.contains { $0.key.contains("kids") }, "tombstones keep only a hash")

        // Retraction (the meeting was deleted) leaves no tombstone.
        let learned = model.learn([candidate("I lead the platform team.", .fact)], now: now).saved
        model.retract(ids: Set(learned.map(\.id)))
        XCTAssertEqual(model.tombstones.count, 2)
    }

    func testRewordedKnownItemsAreNotLearnedAgain() {
        var model = PersonalModel()
        _ = model.add("I want a staff engineer role in 2027.", kind: .goal, origin: .added, now: now)
        let outcome = model.learn([
            candidate("I want to become a staff engineer in 2027.", .goal),
            candidate("I work in Belgrade.", .fact),
        ], now: now)
        XCTAssertEqual(outcome.saved.map(\.text), ["I work in Belgrade."])
        XCTAssertFalse(PersonalModel.isNearDuplicate("I live in Belgrade.", "I work in Belgrade."))
    }

    func testReviewKeepAndRestateMoveClaimsOutOfLearning() throws {
        var model = PersonalModel()
        model.learn([candidate("I prefer written updates.", .preference), candidate("I live in Belgrade.", .fact)],
                    now: now)
        let preference = try XCTUnwrap(model.claims.first { $0.kind == .preference })
        let confirmed = try model.confirm(id: preference.id, now: now).get()
        XCTAssertEqual(confirmed.status, .confirmed)
        XCTAssertEqual(confirmed.authority, .confirmed)
        XCTAssertNotNil(confirmed.evidence)

        let fact = try XCTUnwrap(model.claims.first { $0.kind == .fact })
        let kept = try model.keepAsStatement(id: fact.id, now: now).get()
        XCTAssertEqual(kept.authority, .stated)
        XCTAssertNil(kept.evidence)

        model.learn([candidate("I have two kids.", .fact)], now: now)
        let restated = try model.add("I have two kids", kind: .fact, origin: .added, now: now).get()
        XCTAssertEqual(restated.authority, .stated)
        XCTAssertNil(restated.evidence)
        XCTAssertTrue(restated.isLearned)
    }

    func testPruneDropsOldSuggestionsAndTombstones() {
        var model = PersonalModel()
        model.learn([candidate("I prefer email.", .preference), candidate("I have two kids.", .fact)], now: now)
        model.tombstones.append(.init(key: "old", createdAt: now.addingTimeInterval(-PersonalModel.tombstoneLifetime - 1)))
        model.prune(now: now.addingTimeInterval(PersonalModel.proposalLifetime + 1))
        XCTAssertEqual(model.claims.map(\.text), ["I have two kids."])
        XCTAssertTrue(model.tombstones.isEmpty)
    }
}

@MainActor
final class PersonalLearningRetractionTests: XCTestCase {
    private var root: URL!
    private let key = SymmetricKey(data: Data(repeating: 0x2B, count: 32))
    private let now = Date(timeIntervalSince1970: 1_790_000_000)

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("PersonalLearningRetraction-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: root)
    }

    private func candidate(_ text: String, meetingID: UUID, revision: UInt64) -> PersonalModel.LearningCandidate {
        PersonalModel.LearningCandidate(
            kind: .fact, text: text, quote: text, meetingID: meetingID, dayKey: "2026-09-21",
            evidence: DreamEvidenceProvenance(
                sources: [DreamEvidenceSource(kind: .meeting, id: meetingID.uuidString, dayKey: "2026-09-21")],
                revision: revision))
    }

    func testDeletedMeetingRetractsLearnedClaimsButNotStatedOnes() throws {
        let dreamStore = DreamStore(root: root)
        let revision = try dreamStore.evidenceRevision()
        let deletedMeeting = UUID()
        let keptMeeting = UUID()
        let controller = PersonalModelController(
            store: PersonalModelStore(root: root, encryptionKey: { [key] in key }),
            now: { [now] in now }, evidenceCheck: { dreamStore.evidenceCheck() })
        _ = controller.remember("Maya is my manager", origin: .added)
        controller.learn([
            candidate("I live in Belgrade.", meetingID: deletedMeeting, revision: revision),
            candidate("I have two kids.", meetingID: keptMeeting, revision: revision),
        ])
        XCTAssertEqual(controller.usableClaims().count, 3)

        _ = try dreamStore.invalidateEvidence(
            affectedDayKeys: [], affectedMeetingIDs: [deletedMeeting], reportDayKeys: [])
        controller.retractIfLoaded()
        XCTAssertEqual(controller.usableClaims().map(\.text).sorted(), ["I have two kids.", "Maya is my manager"])
        let reopened = PersonalModelController(store: PersonalModelStore(root: root, encryptionKey: { [key] in key }))
        XCTAssertEqual(reopened.currentClaims().count, 2, "retraction is saved, not just filtered")
    }

    func testPendingRevocationWithholdsLearnedClaimsAndScopedQuestionsSkipThem() throws {
        var available = true
        let controller = PersonalModelController(
            store: PersonalModelStore(root: root, encryptionKey: { [key] in key }),
            now: { [now] in now },
            evidenceCheck: { () -> ((DreamEvidenceProvenance) -> Bool)? in
                available ? { _ in false } : nil
            })
        _ = controller.remember("Maya is my manager", origin: .added)
        controller.learn([candidate("I live in Belgrade.", meetingID: UUID(), revision: 1)])

        XCTAssertEqual(controller.usableClaims(includeEvidenceBacked: false).map(\.text), ["Maya is my manager"])
        available = false
        XCTAssertEqual(controller.usableClaims().map(\.text), ["Maya is my manager"])
        XCTAssertEqual(controller.readableClaims()?.map(\.text), ["Maya is my manager"])
        XCTAssertEqual(controller.currentClaims().count, 2, "withheld, not deleted")
        available = true
        XCTAssertEqual(controller.usableClaims().count, 2)
    }

    func testLearnedClaimsAreLabelledForAskAndAgents() throws {
        let controller = PersonalModelController(
            store: PersonalModelStore(root: root, encryptionKey: { [key] in key }), now: { [now] in now })
        let learned = controller.learn([candidate("I live in Belgrade.", meetingID: UUID(), revision: 1)]).saved
        let claim = try XCTUnwrap(learned.first)
        XCTAssertTrue(PersonalContextBuilder.line(for: claim, timeZone: TimeZone(identifier: "UTC")!)
            .hasPrefix("- [fact · learned 2026-09-21]"))
        XCTAssertEqual(PersonalContextProjection.Claim(claim).source, .learnedFromMeeting)

        let gate = PersonalContextAccessGate(root: root)
        try gate.enable()
        var shared = claim
        shared.sharing = .agents
        try gate.writeProjection(PersonalContextProjection(generatedAt: now, claims: [.init(shared)]))
        let report = try PersonalContextTool.report(task: "Where do I live?", kinds: nil, maxItems: nil, gate: gate).get()
        XCTAssertEqual(report.items.first?.authority, "learned")
    }

    func testRunCallsTheModelOnlyWithTheDaysOwnLines() async throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        let meetingID = UUID()
        try MeetingFixture.write([
            .init(id: meetingID, title: "Weekly sync", startedAt: now, transcriptLines: [
                "I live in Belgrade and I work from home most days.",
                "My salary is confidential, please keep it quiet.",
            ]),
        ], under: root)
        let engine = LearningStubEngine(#"{"items": [{"line": "U1", "kind": "fact", "text": "I live in Belgrade."}]}"#)
        let candidates = try await PersonalLearning.run(
            day: now, storageRoot: root, known: [], revision: 4, engine: engine, calendar: calendar)
        XCTAssertEqual(candidates.map(\.text), ["I live in Belgrade."])
        XCTAssertEqual(candidates.first?.meetingID, meetingID)
        XCTAssertEqual(engine.prompts.count, 1)
        XCTAssertTrue(engine.prompts[0].contains("U1 (Weekly sync): I live in Belgrade"))
        XCTAssertFalse(engine.prompts[0].contains("salary"), "the other speaker's line is never sent")

        let quiet = LearningStubEngine("{}")
        let none = try await PersonalLearning.run(
            day: now.addingTimeInterval(86_400 * 3), storageRoot: root, known: [], revision: 4,
            engine: quiet, calendar: calendar)
        XCTAssertTrue(none.isEmpty)
        XCTAssertTrue(quiet.prompts.isEmpty, "no lines, no model call")
    }
}

private final class LearningStubEngine: TextEngine {
    private let output: String
    private(set) var prompts: [String] = []

    init(_ output: String) { self.output = output }

    var displayName: String { "Learning stub" }

    func generate(system: String, prompt: String, context: [String]) async throws -> String {
        prompts.append(prompt)
        return output
    }
}
