import AppKit
import XCTest
@testable import LokalBot

@MainActor
final class AppDelegateLifecycleTests: XCTestCase {
    /// SwiftUI quits an app whose main scene is a single `Window` once that
    /// window closes, unless the app delegate answers otherwise. Closing the
    /// main window used to quit LokalBot, menu-bar-only included (issue #248).
    func testClosingTheLastWindowKeepsTheAppRunning() {
        let delegate: NSApplicationDelegate = AppDelegate()

        XCTAssertEqual(
            delegate.applicationShouldTerminateAfterLastWindowClosed?(NSApplication.shared),
            false)
    }
}
