import Foundation

/// Cotyping usage + latency metrics. A pure, `Codable` value type (the counters
/// and their derivation) wrapped by `CotypingStatsStore` for persistence.
/// Inspired by Cotabby's `PerformanceMetricsStore`, trimmed to the metrics that
/// matter for a local continuer and that record cleanly at unambiguous pipeline
/// points (an LLM generation completing, an accept tap succeeding).
struct CotypingStats: Codable, Equatable, Sendable {
    /// LLM continuations that completed (regardless of whether they produced text).
    var generations: Int = 0
    /// Generations that failed (network error, bad response, …).
    var errors: Int = 0
    /// Continuation accepts (a phrase chunk or a full accept).
    var accepts: Int = 0
    /// Total characters inserted via accept.
    var charsAccepted: Int = 0
    /// Rolling window of generation latencies, newest last.
    var latenciesMs: [Int] = []
    /// What happened in other apps, by kind of writing surface (a
    /// `CotypingSurfaceClass` raw value). See `CotypingLiveMeasure`.
    var live: [String: CotypingLiveMeasure] = [:]

    /// Cap on retained latency samples — a rolling window for the averages.
    static let maxLatencies = 50

    init() {}

    /// Counters saved before a field existed keep their values instead of
    /// resetting the whole record.
    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        generations = try container.decodeIfPresent(Int.self, forKey: .generations) ?? 0
        errors = try container.decodeIfPresent(Int.self, forKey: .errors) ?? 0
        accepts = try container.decodeIfPresent(Int.self, forKey: .accepts) ?? 0
        charsAccepted = try container.decodeIfPresent(Int.self, forKey: .charsAccepted) ?? 0
        latenciesMs = try container.decodeIfPresent([Int].self, forKey: .latenciesMs) ?? []
        live = try container.decodeIfPresent([String: CotypingLiveMeasure].self, forKey: .live) ?? [:]
    }

    /// Accepts per generation. Phrase acceptance can push this above 1.0 (one
    /// generation accepted in multiple word chunks), which is itself a useful
    /// "how much of each suggestion do I keep" signal.
    var acceptsPerGeneration: Double {
        generations > 0 ? Double(accepts) / Double(generations) : 0
    }
    var avgLatencyMs: Int? {
        guard !latenciesMs.isEmpty else { return nil }
        return latenciesMs.reduce(0, +) / latenciesMs.count
    }
    var medianLatencyMs: Int? { percentile(0.5) }
    var p95LatencyMs: Int? { percentile(0.95) }
    var maxLatencyMs: Int? { latenciesMs.max() }

    /// Nearest-rank percentile over the (small) window. Pure and deterministic.
    private func percentile(_ p: Double) -> Int? {
        guard !latenciesMs.isEmpty else { return nil }
        let sorted = latenciesMs.sorted()
        let ratio = Double(sorted.count - 1) * p
        let idx = max(0, min(sorted.count - 1, Int(ratio.rounded())))
        return sorted[idx]
    }

    mutating func recordGeneration(latencyMs: Int) {
        generations += 1
        latenciesMs.append(max(0, latencyMs))
        if latenciesMs.count > Self.maxLatencies {
            latenciesMs.removeFirst(latenciesMs.count - Self.maxLatencies)
        }
    }
    mutating func recordError() { errors += 1 }
    mutating func recordAccept(charsAccepted acceptedChars: Int, surface: String? = nil) {
        accepts += 1
        charsAccepted += max(0, acceptedChars)
        if let surface { live[surface, default: .init()].accepts += 1 }
    }
    mutating func recordShown(latencyMs: Int, surface: String) {
        live[surface, default: .init()].recordShown(latencyMs: latencyMs)
    }
    mutating func recordInsertion(_ outcome: CotypingInsertionCheck.Outcome, count: Int = 1, surface: String) {
        for _ in 0..<max(0, count) { live[surface, default: .init()].record(outcome) }
    }
    mutating func recordCorrection(surface: String) {
        live[surface, default: .init()].acceptsCorrected += 1
    }
    mutating func reset() { self = CotypingStats() }
}

/// Async persistence seam so JSON encoding and UserDefaults writes never run
/// on the UI actor. Actor implementations also give queued snapshots a single
/// deterministic writer.
protocol CotypingStatsPersisting: Sendable {
    func persist(_ stats: CotypingStats) async
    func remove() async
    func persistUsage(_ usage: CotypingUsageHistory) async
    func removeUsage() async
}

actor UserDefaultsCotypingStatsPersistence: CotypingStatsPersisting {
    private let defaults: UserDefaults
    private let key: String
    private let usageKey: String

    init(defaults: UserDefaults, key: String, usageKey: String) {
        self.defaults = defaults
        self.key = key
        self.usageKey = usageKey
    }

    func persist(_ stats: CotypingStats) {
        guard let data = try? JSONEncoder().encode(stats) else { return }
        defaults.set(data, forKey: key)
    }

    func remove() {
        defaults.removeObject(forKey: key)
    }

    func persistUsage(_ usage: CotypingUsageHistory) {
        guard let data = try? JSONEncoder().encode(usage) else { return }
        defaults.set(data, forKey: usageKey)
    }

    func removeUsage() {
        defaults.removeObject(forKey: usageKey)
    }
}

/// Owns the persisted `CotypingStats` and `CotypingUsageHistory` and records
/// into them from the coordinator. `@MainActor` + `ObservableObject` keeps UI
/// state immediately observable, but persistence is delegated to an actor.
/// Local only — written to UserDefaults as two small JSON blobs; never leaves
/// the device.
@MainActor
final class CotypingStatsStore: ObservableObject {
    static let shared = CotypingStatsStore()

    @Published private(set) var stats: CotypingStats
    /// Day-by-day use for the statistics. Not `@Published`: typed characters
    /// arrive with every keystroke in other apps, and an open Settings window
    /// need not redraw for each. Other changes announce themselves, and the
    /// next generation or accept shows the typing.
    private(set) var usage: CotypingUsageHistory

    private let persistence: any CotypingStatsPersisting
    private let now: () -> Date
    private let calendar: Calendar
    private var persistenceTask: Task<Void, Never>?
    private var revision = 0
    private var enqueuedRevision = 0
    private var usageRevision = 0
    private var enqueuedUsageRevision = 0
    private static let key = "lokalbotv3.cotypingStats"
    private static let usageKey = "lokalbotv3.cotypingUsage"

    init(
        defaults: UserDefaults = .standard,
        persistence: (any CotypingStatsPersisting)? = nil,
        now: @escaping () -> Date = Date.init,
        calendar: Calendar = .current
    ) {
        self.persistence = persistence
            ?? UserDefaultsCotypingStatsPersistence(defaults: defaults, key: Self.key, usageKey: Self.usageKey)
        self.now = now
        self.calendar = calendar
        stats = Self.load(from: defaults)
        usage = Self.loadUsage(from: defaults)
    }

    func recordGeneration(latencyMs: Int) {
        stats.recordGeneration(latencyMs: latencyMs)
        revision &+= 1
        enqueuePersistence()
    }

    func recordError() {
        stats.recordError()
        revision &+= 1
        enqueuePersistence()
    }

    /// Accept counters stay live in the UI for every chunk, but disk remains
    /// dirty until the coordinator closes that suggestion.
    func recordAccept(charsAccepted: Int, surface: String? = nil) {
        stats.recordAccept(charsAccepted: charsAccepted, surface: surface)
        revision &+= 1
    }

    /// Live measurements share the accept counters' persistence boundary: they
    /// are written with the next generation or completed suggestion.
    func recordShown(latencyMs: Int, surface: String) {
        stats.recordShown(latencyMs: latencyMs, surface: surface)
        revision &+= 1
    }

    func recordInsertion(_ outcome: CotypingInsertionCheck.Outcome, count: Int = 1, surface: String) {
        guard outcome != .pending else { return }
        stats.recordInsertion(outcome, count: count, surface: surface)
        revision &+= 1
    }

    func recordCorrection(surface: String) {
        stats.recordCorrection(surface: surface)
        revision &+= 1
    }

    // MARK: - Usage history

    func recordSuggestionShown(surface: String, model: String) {
        objectWillChange.send()
        usage.recordShown(surface: surface, model: model, at: now(), calendar: calendar)
        usageRevision &+= 1
    }

    /// Saved with the suggestion, like the accept counters.
    func recordSuggestionAccept(words: Int, characters: Int, firstOfSuggestion: Bool, surface: String, model: String) {
        objectWillChange.send()
        usage.recordAccept(
            words: words, characters: characters, firstOfSuggestion: firstOfSuggestion,
            surface: surface, model: model, at: now(), calendar: calendar)
        usageRevision &+= 1
    }

    func recordTakenBack(surface: String, model: String) {
        objectWillChange.send()
        usage.recordTakenBack(surface: surface, model: model, at: now(), calendar: calendar)
        usageRevision &+= 1
    }

    func recordTypoFixed() {
        objectWillChange.send()
        usage.recordTypoFixed(at: now(), calendar: calendar)
        usageRevision &+= 1
        enqueuePersistence()
    }

    /// Called for each key typed where autocomplete works, so it neither
    /// redraws nor writes; see `usage`.
    func recordTyped(characters: Int, gapMilliseconds: Int?) {
        usage.recordTyped(characters: characters, gapMilliseconds: gapMilliseconds, at: now(), calendar: calendar)
        usageRevision &+= 1
    }

    func clearUsage() {
        guard usage != CotypingUsageHistory() else { return }
        objectWillChange.send()
        usage = CotypingUsageHistory()
        usageRevision &+= 1
        enqueuedUsageRevision = usageRevision
        let previous = persistenceTask
        let persistence = persistence
        persistenceTask = Task.detached(priority: .utility) {
            if let previous { await previous.value }
            await persistence.removeUsage()
        }
    }

    /// One persistence boundary for all chunks accepted from one suggestion.
    func suggestionCompleted() {
        guard revision != enqueuedRevision || usageRevision != enqueuedUsageRevision else { return }
        enqueuePersistence()
    }

    func clear() {
        guard stats != CotypingStats() else { return }
        stats.reset()
        revision &+= 1
        enqueuedRevision = revision
        let previous = persistenceTask
        let persistence = persistence
        persistenceTask = Task.detached(priority: .utility) {
            if let previous { await previous.value }
            await persistence.remove()
        }
    }

    /// Persists any dirty counters and awaits all background writes. This is
    /// the termination safety net for an in-flight/partially accepted session.
    func flushPersistence() async {
        if revision != enqueuedRevision || usageRevision != enqueuedUsageRevision { enqueuePersistence() }
        let pending = persistenceTask
        await pending?.value
    }

    /// Waits only for work already scheduled by a completed suggestion.
    func waitForPendingPersistence() async {
        let pending = persistenceTask
        await pending?.value
    }

    /// Writes the counters, and the usage history only when it changed.
    private func enqueuePersistence() {
        let persistedStats = stats
        let persistedUsage = usageRevision != enqueuedUsageRevision ? usage : nil
        enqueuedRevision = revision
        enqueuedUsageRevision = usageRevision
        let previous = persistenceTask
        let persistence = persistence
        persistenceTask = Task.detached(priority: .utility) {
            if let previous { await previous.value }
            await persistence.persist(persistedStats)
            if let persistedUsage { await persistence.persistUsage(persistedUsage) }
        }
    }

    /// `--cotyping-measurements`: print what is saved, as JSON, so a script
    /// that drove real apps can check how suggestions and accepts went.
    static func printSaved(defaults: UserDefaults = .standard) -> Int32 {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        guard let data = try? encoder.encode(load(from: defaults)) else { return 1 }
        FileHandle.standardOutput.write(data + Data([10]))
        return 0
    }

    private static func load(from defaults: UserDefaults) -> CotypingStats {
        guard let data = defaults.data(forKey: key),
              let decoded = try? JSONDecoder().decode(CotypingStats.self, from: data) else { return CotypingStats() }
        return decoded
    }

    private static func loadUsage(from defaults: UserDefaults) -> CotypingUsageHistory {
        guard let data = defaults.data(forKey: usageKey),
              let decoded = try? JSONDecoder().decode(CotypingUsageHistory.self, from: data)
        else { return CotypingUsageHistory() }
        return decoded
    }
}
