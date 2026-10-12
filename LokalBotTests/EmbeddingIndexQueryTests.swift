import XCTest
@testable import LokalBot

/// Ask and Quick Recall search meetings and then screens on every debounced
/// keystroke. The typed query is embedded once for both sources, at the
/// caller's priority, while indexing stays background work.
@MainActor
final class EmbeddingIndexQueryTests: XCTestCase {
    private final class Recorder {
        struct Call: Equatable {
            let texts: [String]
            let prefix: String
            let priority: InferencePriority
        }

        private(set) var calls: [Call] = []
        var failNext = false

        var queries: [Call] { calls.filter { $0.prefix == EmbeddingIndex.queryPrefix } }
        var documents: [Call] { calls.filter { $0.prefix == EmbeddingIndex.documentPrefix } }

        func embed(_ texts: [String], prefix: String, priority: InferencePriority) throws -> [[Float]] {
            calls.append(Call(texts: texts, prefix: prefix, priority: priority))
            if failNext {
                failNext = false
                throw URLError(.cannotConnectToHost)
            }
            return texts.map { _ in [1, 0] }
        }
    }

    private struct Library {
        let root: URL
        let databaseURL: URL
        let store: ActivityStore
        let index: EmbeddingIndex
        let recorder: Recorder
        let meetingID: UUID
        let screenID: Int64
    }

    private var roots: [URL] = []

    override func tearDown() {
        for root in roots { try? FileManager.default.removeItem(at: root) }
        roots = []
        super.tearDown()
    }

    private func vectorData(_ values: [Float]) -> Data {
        values.withUnsafeBufferPointer { buffer in
            Data(bytes: buffer.baseAddress!, count: buffer.count * MemoryLayout<Float>.stride)
        }
    }

    /// One embedded meeting chunk and one embedded screen, both matching the
    /// recorder's query vector exactly.
    private func makeLibrary() throws -> Library {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("embedding-query-\(UUID().uuidString)", isDirectory: true)
        roots.append(root)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let databaseURL = root.appendingPathComponent("lokalbotv3.sqlite")
        let store = ActivityStore(databaseURL: databaseURL)
        let screenID = try store.insertScreenshot(
            ts: Date(), path: "/tmp/synthetic-screen.enc", app: "Editor",
            windowTitle: "Launch", ocr: "Launch budget review")
        let recorder = Recorder()
        let index = EmbeddingIndex(
            databaseURL: databaseURL, storage: StorageManager(rootURL: root)
        ) { texts, prefix, priority in
            try recorder.embed(texts, prefix: prefix, priority: priority)
        }
        let database = try XCTUnwrap(SQLiteDatabase(url: databaseURL))
        let meetingID = UUID()
        let vector = vectorData([1, 0])
        XCTAssertTrue(database.run(
            "INSERT INTO embeddings (meeting_id, start, text, vec) VALUES (?1, ?2, ?3, ?4)",
            bind: [meetingID.uuidString, 30.0, "We agreed on the launch budget.", vector]))
        XCTAssertTrue(database.run(
            "INSERT INTO embedded_meetings (meeting_id, source_mtime, model_id) VALUES (?1, ?2, ?3)",
            bind: [meetingID.uuidString, 1.0, EmbeddingIndex.indexVersion]))
        XCTAssertTrue(database.run("""
            INSERT INTO screen_embeddings (snapshot_id, ts, app, text, vec, model_id)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            """, bind: [screenID, Date().timeIntervalSince1970, "Editor",
                         "Launch budget review", vector, EmbeddingIndex.indexVersion]))
        return Library(root: root, databaseURL: databaseURL, store: store, index: index,
                       recorder: recorder, meetingID: meetingID, screenID: screenID)
    }

    func testMeetingAndScreenSearchShareOneQueryEmbedding() async throws {
        let library = try makeLibrary()

        // Typing pauses after a space; the padded query is the same query.
        let meetings = await library.index.search("launch budget ", priority: .interactive)
        let screens = await library.index.searchScreen("launch budget", priority: .interactive)

        XCTAssertEqual(meetings.map(\.meetingID), [library.meetingID])
        XCTAssertEqual(screens.map(\.snapshotID), [library.screenID])
        XCTAssertEqual(library.recorder.queries, [
            .init(texts: ["launch budget"], prefix: EmbeddingIndex.queryPrefix, priority: .interactive),
        ])
    }

    func testChangedQueryEmbedsAgainAndFailedEmbeddingIsRetried() async throws {
        let library = try makeLibrary()
        library.recorder.failNext = true

        let failed = await library.index.search("launch budget", priority: .interactive)
        let retried = await library.index.searchScreen("launch budget", priority: .interactive)
        let changed = await library.index.search("hiring plan")

        XCTAssertTrue(failed.isEmpty)
        XCTAssertEqual(retried.map(\.snapshotID), [library.screenID])
        XCTAssertEqual(changed.map(\.meetingID), [library.meetingID])
        XCTAssertEqual(library.recorder.queries.map(\.texts),
                       [["launch budget"], ["launch budget"], ["hiring plan"]])
        // Callers that pass no priority keep the previous background lease.
        XCTAssertEqual(library.recorder.queries.map(\.priority),
                       [.interactive, .interactive, .background])
    }

    func testIndexingStaysBackgroundWork() async throws {
        let library = try makeLibrary()
        let unembeddedScreen = try library.store.insertScreenshot(
            ts: Date(), path: "/tmp/synthetic-screen-2.enc", app: "Editor",
            windowTitle: "Hiring", ocr: "Hiring plan draft")
        let meetingID = UUID()
        try MeetingFixture.write([
            .init(id: meetingID, title: "Hiring sync", startedAt: Date(),
                  transcriptLines: ["We will open two roles."]),
        ], under: library.root)
        let meeting = try XCTUnwrap(StorageManager(rootURL: library.root).loadMeetings()
            .first { $0.id == meetingID })

        await library.index.reindexScreenText()
        try await library.index.index(meeting)

        XCTAssertEqual(library.recorder.documents.count, 2)
        XCTAssertEqual(library.recorder.documents.first?.texts, ["Hiring plan draft"])
        XCTAssertEqual(library.recorder.documents.map(\.priority), [.background, .background])
        XCTAssertTrue(library.recorder.queries.isEmpty)
        let database = try XCTUnwrap(SQLiteDatabase(url: library.databaseURL))
        XCTAssertTrue(database.hasRow(
            "SELECT 1 FROM screen_embeddings WHERE snapshot_id = ?1", bind: [unembeddedScreen]))
    }
}
