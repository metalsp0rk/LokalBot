import AppKit
import CoreGraphics
import Foundation
import ScreenCaptureKit
import Vision

/// Captures the focused window and reads its text on device for
/// `CotypingWindowText`. Never on the keystroke path: the first request in a
/// field starts a read and goes without it; later requests use the result,
/// which is reread in the background every few seconds while the field keeps
/// focus. One read runs at a time. The image is dropped as soon as it is read,
/// and the text is held for the focused field only.
final class CotypingWindowTextCapture: @unchecked Sendable {
    static let shared = CotypingWindowTextCapture()

    /// Everything a read needs, resolved through Accessibility when it starts.
    struct Request: Sendable {
        var key: String
        var processID: pid_t
        var target: CotypingVisibleContext.Target
    }

    typealias Reader = @Sendable (Request) async -> [CotypingWindowText.Line]?

    /// A read costs about 150–250 ms of background work on a full window.
    static let refreshInterval: TimeInterval = 4

    private struct Entry {
        var key: String
        var snapshot: CotypingVisibleContext.Snapshot?
        var startedAt: TimeInterval
        var adoptedAt: TimeInterval
    }

    private let lock = NSLock()
    private var entry: Entry?
    private var reading = false
    private var generation: UInt64 = 0
    private var updateHandler: (@Sendable () -> Void)?
    private let clock: @Sendable () -> TimeInterval
    private let reader: Reader

    init(clock: @escaping @Sendable () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
         reader: @escaping Reader = { await CotypingWindowTextCapture.read($0) }) {
        self.clock = clock
        self.reader = reader
    }

    /// Called after a read changes the text in use, so the model can read the
    /// new prompt before the next keystroke needs it.
    func setUpdateHandler(_ handler: (@Sendable () -> Void)?) {
        lock.withLock { updateHandler = handler }
    }

    /// The window text for `key`, or nil until the first read finishes. A
    /// missing or old read starts one in the background; `request` runs there
    /// too, never on the keystroke, and returning nil (not permitted, no
    /// window) skips the read.
    func snapshot(forKey key: String,
                  request: @escaping @Sendable () -> Request?) -> CotypingVisibleContext.Snapshot? {
        let now = clock()
        lock.lock()
        if let entry, entry.key != key {
            // Another field, window or document: forget the old text.
            self.entry = nil
            generation &+= 1
        }
        let snapshot = entry?.snapshot
        let due = entry.map { now - $0.startedAt >= Self.refreshInterval } ?? true
        guard due, !reading else {
            lock.unlock()
            return snapshot
        }
        reading = true
        // Counted from the start, so a read that fails is not retried per keystroke.
        if entry == nil {
            entry = Entry(key: key, snapshot: nil, startedAt: now, adoptedAt: now)
        } else {
            entry?.startedAt = now
        }
        let generation = generation
        lock.unlock()

        Task.detached(priority: .utility) { [self] in
            guard let request = request(), request.key == key else {
                lock.withLock { reading = false }
                return
            }
            let lines = await reader(request)
            finish(request, lines: lines, generation: generation)
        }
        return snapshot
    }

    /// Forgets the text in use. A read already running cannot put its result back.
    func removeAll() {
        lock.withLock {
            entry = nil
            generation &+= 1
        }
    }

    /// The text in use without starting a read. For tests.
    func cachedSnapshot() -> CotypingVisibleContext.Snapshot? {
        lock.withLock { entry?.snapshot }
    }

    private func finish(_ request: Request, lines: [CotypingWindowText.Line]?, generation: UInt64) {
        // Filtering runs a few expressions per line; keep it outside the lock
        // that a keystroke takes.
        let target = request.target
        let snapshot = lines.map {
            CotypingVisibleContext.Snapshot(
                target: target,
                excerpts: CotypingWindowText.excerpts(
                    from: $0, fieldFrame: target.fieldFrame, windowFrame: target.windowFrame),
                source: .windowText)
        }
        let handler: (@Sendable () -> Void)? = lock.withLock {
            reading = false
            guard generation == self.generation, var current = entry, current.key == request.key,
                  let snapshot else { return nil }
            let now = clock()
            guard CotypingWindowText.shouldReplace(
                current.snapshot, with: snapshot, heldFor: now - current.adoptedAt) else { return nil }
            let changed = current.snapshot?.text != snapshot.text
            current.snapshot = snapshot.excerpts.isEmpty ? nil : snapshot
            current.adoptedAt = now
            entry = current
            return changed ? updateHandler : nil
        }
        handler?()
    }

    // MARK: - Reading the screen

    private struct Image: @unchecked Sendable {
        let image: CGImage
    }

    private static let recognitionQueue = DispatchQueue(
        label: "me.dotenv.LokalBot.cotyping.window-text", qos: .utility)

    /// Captures only the focused window, while its app is still in front.
    /// Never asks for Screen Recording: without it there is no window text.
    static func read(_ request: Request) async -> [CotypingWindowText.Line]? {
        guard CGPreflightScreenCaptureAccess() else { return nil }
        do {
            let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
            let candidates = content.windows.map {
                CotypingWindowText.WindowCandidate(
                    processID: $0.owningApplication?.processID ?? 0, title: $0.title,
                    frame: $0.frame, layer: $0.windowLayer)
            }
            guard let index = CotypingWindowText.windowIndex(
                in: candidates, processID: request.processID,
                title: request.target.windowTitle, frame: request.target.windowFrame) else { return nil }
            let window = content.windows[index]
            let filter = SCContentFilter(desktopIndependentWindow: window)
            guard let size = DictationCaptureSizing.pixels(
                for: window.frame, pointPixelScale: CGFloat(filter.pointPixelScale),
                maximumDimension: 3_000) else { return nil }
            let configuration = SCStreamConfiguration()
            configuration.width = size.width
            configuration.height = size.height
            configuration.showsCursor = false
            configuration.capturesAudio = false
            let image = try await SCScreenshotManager.captureImage(
                contentFilter: filter, configuration: configuration)
            guard NSWorkspace.shared.frontmostApplication?.processIdentifier == request.processID else { return nil }
            return await recognize(Image(image: image), windowFrame: window.frame)
        } catch {
            return nil
        }
    }

    /// The same recognizer as screen memory and Compose: `.fast` drops every
    /// accent and all Cyrillic, and the en-US default cannot read CJK.
    private static func recognize(_ input: Image, windowFrame: CGRect) async -> [CotypingWindowText.Line] {
        await withCheckedContinuation { continuation in
            recognitionQueue.async {
                let request = VNRecognizeTextRequest()
                request.recognitionLevel = .accurate
                request.usesLanguageCorrection = false
                request.automaticallyDetectsLanguage = true
                try? VNImageRequestHandler(cgImage: input.image).perform([request])
                let lines = (request.results ?? []).compactMap { observation -> CotypingWindowText.Line? in
                    guard let text = observation.topCandidates(1).first?.string else { return nil }
                    return .init(text: text, frame: CotypingWindowText.frame(
                        ofNormalized: observation.boundingBox, in: windowFrame))
                }
                continuation.resume(returning: lines)
            }
        }
    }
}
