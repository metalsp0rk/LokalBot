import Foundation

/// What autocomplete did for one period: suggestions shown and taken, and the
/// words and characters they wrote. Counts only.
struct CotypingUsageCounts: Codable, Equatable, Sendable {
    /// Suggestions that appeared at the caret. A suggestion shown again at a
    /// new caret, for example after an accept the app drew late, counts again.
    var shown = 0
    /// Shown suggestions you took at least part of.
    var used = 0
    /// Accept key presses that inserted suggested text.
    var accepts = 0
    /// Words inserted from suggestions.
    var words = 0
    /// Characters inserted from suggestions.
    var characters = 0
    /// Accepts deleted or undone with the very next key.
    var takenBack = 0

    private enum CodingKeys: String, CodingKey {
        case shown, used, accepts, words, characters, takenBack
    }

    init() {}

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        func count(_ key: CodingKeys) throws -> Int {
            try container.decodeIfPresent(Int.self, forKey: key) ?? 0
        }
        shown = try count(.shown)
        used = try count(.used)
        accepts = try count(.accepts)
        words = try count(.words)
        characters = try count(.characters)
        takenBack = try count(.takenBack)
    }

    /// Zero counts are left out, which keeps a quiet day to a few bytes.
    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        func count(_ value: Int, _ key: CodingKeys) throws {
            if value != 0 { try container.encode(value, forKey: key) }
        }
        try count(shown, .shown)
        try count(used, .used)
        try count(accepts, .accepts)
        try count(words, .words)
        try count(characters, .characters)
        try count(takenBack, .takenBack)
    }

    var isEmpty: Bool { self == CotypingUsageCounts() }

    /// Share of shown suggestions you took part of. An accept just after
    /// midnight can land on a later day than its suggestion, so `used` is
    /// capped at `shown`.
    var acceptanceRate: Double? {
        shown > 0 ? Double(min(used, shown)) / Double(shown) : nil
    }

    /// Words you took from each suggestion you used.
    var wordsPerUsedSuggestion: Double? {
        used > 0 ? Double(words) / Double(used) : nil
    }

    /// Key presses you did not make: every inserted character, less the
    /// accept presses that inserted them.
    var keystrokesSaved: Int { max(0, characters - accepts) }

    mutating func merge(_ other: CotypingUsageCounts) {
        shown += other.shown
        used += other.used
        accepts += other.accepts
        words += other.words
        characters += other.characters
        takenBack += other.takenBack
    }
}

/// One day or month of autocomplete use, with how much you typed yourself
/// where autocomplete could suggest. No text, app name or window title.
struct CotypingUsageBucket: Codable, Equatable, Sendable {
    var suggestions = CotypingUsageCounts()
    /// By kind of app (a `CotypingSurfaceClass` raw value).
    var surfaces: [String: CotypingUsageCounts] = [:]
    /// By autocomplete model ID.
    var models: [String: CotypingUsageCounts] = [:]
    /// Characters you typed yourself in fields autocomplete serves.
    var typedCharacters = 0
    /// Gaps between consecutive typed characters, up to
    /// `CotypingUsageHistory.maxTypingGapMilliseconds`: their sum and count
    /// give your typing speed.
    var typingMilliseconds = 0
    var typingGaps = 0
    /// Misspellings replaced by accepting a correction.
    var typosFixed = 0

    private enum CodingKeys: String, CodingKey {
        case suggestions, surfaces, models, typedCharacters, typingMilliseconds, typingGaps, typosFixed
    }

    init() {}

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        func count(_ key: CodingKeys) throws -> Int {
            try container.decodeIfPresent(Int.self, forKey: key) ?? 0
        }
        suggestions = try container.decodeIfPresent(CotypingUsageCounts.self, forKey: .suggestions) ?? .init()
        surfaces = try container.decodeIfPresent([String: CotypingUsageCounts].self, forKey: .surfaces) ?? [:]
        models = try container.decodeIfPresent([String: CotypingUsageCounts].self, forKey: .models) ?? [:]
        typedCharacters = try count(.typedCharacters)
        typingMilliseconds = try count(.typingMilliseconds)
        typingGaps = try count(.typingGaps)
        typosFixed = try count(.typosFixed)
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        func count(_ value: Int, _ key: CodingKeys) throws {
            if value != 0 { try container.encode(value, forKey: key) }
        }
        if !suggestions.isEmpty { try container.encode(suggestions, forKey: .suggestions) }
        if !surfaces.isEmpty { try container.encode(surfaces, forKey: .surfaces) }
        if !models.isEmpty { try container.encode(models, forKey: .models) }
        try count(typedCharacters, .typedCharacters)
        try count(typingMilliseconds, .typingMilliseconds)
        try count(typingGaps, .typingGaps)
        try count(typosFixed, .typosFixed)
    }

    var isEmpty: Bool { self == CotypingUsageBucket() }

    /// Share of the text written where autocomplete works that came from
    /// suggestions rather than your own keystrokes.
    var shareOfText: Double? {
        let total = suggestions.characters + typedCharacters
        return total > 0 ? Double(suggestions.characters) / Double(total) : nil
    }

    /// Average time per typed character, once there are enough gaps to say.
    var millisecondsPerKeystroke: Double? {
        typingGaps >= CotypingUsageHistory.minimumTypingGaps
            ? Double(typingMilliseconds) / Double(typingGaps) : nil
    }

    mutating func merge(_ other: CotypingUsageBucket) {
        suggestions.merge(other.suggestions)
        for (key, counts) in other.surfaces { surfaces[key, default: .init()].merge(counts) }
        for (key, counts) in other.models { models[key, default: .init()].merge(counts) }
        typedCharacters += other.typedCharacters
        typingMilliseconds += other.typingMilliseconds
        typingGaps += other.typingGaps
        typosFixed += other.typosFixed
    }

    /// Adds to the totals and to the given app kind and model.
    mutating func update(surface: String, model: String, _ change: (inout CotypingUsageCounts) -> Void) {
        change(&suggestions)
        change(&surfaces[surface, default: .init()])
        change(&models[model, default: .init()])
    }
}

/// Autocomplete use by local calendar day and month. Daily rows are kept for
/// `retainedDays`, for the week and month charts; monthly rows are kept, so
/// longer ranges still add up and cost a few hundred bytes a month.
struct CotypingUsageHistory: Codable, Equatable, Sendable {
    /// "2026-10-14" → that day's counts.
    var days: [String: CotypingUsageBucket] = [:]
    /// "2026-10" → that month's counts.
    var months: [String: CotypingUsageBucket] = [:]

    static let retainedDays = 92
    /// A longer gap between two typed characters is a pause, not typing speed.
    static let maxTypingGapMilliseconds = 1_000
    /// Typing speed is shown once this many gaps have been measured.
    static let minimumTypingGaps = 200
    /// Used for time saved until your own speed is known: 40 words a minute
    /// at five characters a word.
    static let fallbackMillisecondsPerKeystroke = 300.0

    init() {}

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        days = try container.decodeIfPresent([String: CotypingUsageBucket].self, forKey: .days) ?? [:]
        months = try container.decodeIfPresent([String: CotypingUsageBucket].self, forKey: .months) ?? [:]
    }

    var isEmpty: Bool { months.values.allSatisfy(\.isEmpty) }

    mutating func recordShown(surface: String, model: String, at date: Date, calendar: Calendar = .current) {
        record(at: date, calendar: calendar) { bucket in
            bucket.update(surface: surface, model: model) { $0.shown += 1 }
        }
    }

    /// One accept press. `firstOfSuggestion` marks the suggestion as used.
    mutating func recordAccept(
        words: Int, characters: Int, firstOfSuggestion: Bool,
        surface: String, model: String, at date: Date, calendar: Calendar = .current
    ) {
        record(at: date, calendar: calendar) { bucket in
            bucket.update(surface: surface, model: model) { counts in
                counts.accepts += 1
                counts.words += max(0, words)
                counts.characters += max(0, characters)
                if firstOfSuggestion { counts.used += 1 }
            }
        }
    }

    mutating func recordTakenBack(surface: String, model: String, at date: Date, calendar: Calendar = .current) {
        record(at: date, calendar: calendar) { bucket in
            bucket.update(surface: surface, model: model) { $0.takenBack += 1 }
        }
    }

    /// Characters you typed. `gapMilliseconds` is the time since the previous
    /// typed character, when it was short enough to be typing.
    mutating func recordTyped(characters: Int, gapMilliseconds: Int?, at date: Date, calendar: Calendar = .current) {
        record(at: date, calendar: calendar) { bucket in
            bucket.typedCharacters += max(0, characters)
            if let gap = gapMilliseconds, gap >= 0, gap <= Self.maxTypingGapMilliseconds {
                bucket.typingMilliseconds += gap
                bucket.typingGaps += 1
            }
        }
    }

    mutating func recordTypoFixed(at date: Date, calendar: Calendar = .current) {
        record(at: date, calendar: calendar) { $0.typosFixed += 1 }
    }

    private mutating func record(at date: Date, calendar: Calendar, _ change: (inout CotypingUsageBucket) -> Void) {
        let day = Self.dayKey(date, calendar: calendar)
        if days[day] == nil { prune(before: date, calendar: calendar) }
        change(&days[day, default: .init()])
        change(&months[Self.monthKey(date, calendar: calendar), default: .init()])
    }

    /// Drops daily rows older than `retainedDays`; their months keep the totals.
    private mutating func prune(before date: Date, calendar: Calendar) {
        guard let cutoff = calendar.date(byAdding: .day, value: -(Self.retainedDays - 1),
                                         to: calendar.startOfDay(for: date)) else { return }
        // Zero-padded keys sort in date order.
        let oldest = Self.dayKey(cutoff, calendar: calendar)
        days = days.filter { $0.key >= oldest }
    }

    static func dayKey(_ date: Date, calendar: Calendar) -> String {
        let parts = calendar.dateComponents([.year, .month, .day], from: date)
        return String(format: "%04d-%02d-%02d", parts.year ?? 0, parts.month ?? 0, parts.day ?? 0)
    }

    static func monthKey(_ date: Date, calendar: Calendar) -> String {
        let parts = calendar.dateComponents([.year, .month], from: date)
        return String(format: "%04d-%02d", parts.year ?? 0, parts.month ?? 0)
    }
}

/// The periods the statistics can show.
enum CotypingUsageRange: String, CaseIterable, Identifiable, Sendable {
    case week, month, year, allTime

    var id: String { rawValue }

    var label: String {
        switch self {
        case .week: "7 days"
        case .month: "30 days"
        case .year: "12 months"
        case .allTime: "All time"
        }
    }
}

/// One range of the history, ready to show: its total and one bar per day or
/// month, empty periods included so the chart keeps an even time axis.
struct CotypingUsageSummary: Equatable, Sendable {
    struct Bar: Identifiable, Equatable, Sendable {
        var start: Date
        var bucket: CotypingUsageBucket
        var id: Date { start }
    }

    var range: CotypingUsageRange
    var total: CotypingUsageBucket
    var bars: [Bar]
    /// `.day` or `.month`.
    var barUnit: Calendar.Component
    /// Time per keystroke from everything recorded, or nil while too little
    /// typing has been measured. Typing speed is a habit, not a property of
    /// the range, so a short range borrows the longer measurement.
    var millisecondsPerKeystroke: Double?

    /// Your measured typing speed in words a minute, at five characters a word.
    var wordsPerMinute: Int? {
        millisecondsPerKeystroke.map { Int((60_000 / ($0 * 5)).rounded()) }
    }

    /// Keystrokes saved, at your typing speed or the fallback.
    var timeSavedSeconds: Double {
        Double(total.suggestions.keystrokesSaved)
            * (millisecondsPerKeystroke ?? CotypingUsageHistory.fallbackMillisecondsPerKeystroke) / 1_000
    }

    /// Days in the range with any suggestion accepted.
    var activeBars: Int { bars.filter { $0.bucket.suggestions.accepts > 0 }.count }
}

extension CotypingUsageHistory {
    func summary(_ range: CotypingUsageRange, now: Date = Date(), calendar: Calendar = .current) -> CotypingUsageSummary {
        let today = calendar.startOfDay(for: now)
        var bars: [CotypingUsageSummary.Bar] = []
        let unit: Calendar.Component
        switch range {
        case .week, .month:
            unit = .day
            let count = range == .week ? 7 : 30
            for offset in stride(from: count - 1, through: 0, by: -1) {
                guard let day = calendar.date(byAdding: .day, value: -offset, to: today) else { continue }
                bars.append(.init(start: day, bucket: days[Self.dayKey(day, calendar: calendar)] ?? .init()))
            }
        case .year, .allTime:
            unit = .month
            guard let thisMonth = calendar.dateInterval(of: .month, for: now)?.start else { break }
            var count = 12
            if range == .allTime {
                // From the first recorded month, and never fewer than one bar.
                let first = months.filter { !$0.value.isEmpty }.keys.min()
                    .flatMap { Self.monthStart($0, calendar: calendar) } ?? thisMonth
                count = max(1, (calendar.dateComponents([.month], from: first, to: thisMonth).month ?? 0) + 1)
            }
            for offset in stride(from: count - 1, through: 0, by: -1) {
                guard let month = calendar.date(byAdding: .month, value: -offset, to: thisMonth) else { continue }
                bars.append(.init(start: month, bucket: months[Self.monthKey(month, calendar: calendar)] ?? .init()))
            }
        }
        var total = CotypingUsageBucket()
        for bar in bars { total.merge(bar.bucket) }
        var everything = CotypingUsageBucket()
        for month in months.values {
            everything.typingMilliseconds += month.typingMilliseconds
            everything.typingGaps += month.typingGaps
        }
        return CotypingUsageSummary(
            range: range, total: total, bars: bars, barUnit: unit,
            millisecondsPerKeystroke: everything.millisecondsPerKeystroke)
    }

    private static func monthStart(_ key: String, calendar: Calendar) -> Date? {
        let parts = key.split(separator: "-").compactMap { Int($0) }
        guard parts.count == 2 else { return nil }
        return calendar.date(from: DateComponents(year: parts[0], month: parts[1], day: 1))
    }
}
