import Foundation

/// The CLI's requests to the running app over the control socket: one JSON
/// object per line, one request per connection. See
/// Docs/cli-configuration-design-2026-10-17.md.
enum AppControlProtocol {
    static let version = 1
    static let maximumMessageBytes = 1_048_576

    /// Passed by `lokalbot-cli` when it starts the app for a request, so the
    /// app comes up in the menu bar like a login launch instead of opening
    /// its main window.
    static let backgroundLaunchArgument = "--lokalbot-background-launch"

    static func isBackgroundLaunch(
        arguments: [String] = ProcessInfo.processInfo.arguments
    ) -> Bool {
        arguments.contains(backgroundLaunchArgument)
    }

    /// Settings sections `open settings <name>` accepts besides setting keys.
    static let settingsDestinations = [
        "models", "permissions", "agentAccess", "screenMemoryAccess", "personalContextAccess",
        "settingsControlAccess", "memoryHealth", "resourceMonitor", "autocompleteStatistics",
        "dictationWritingProfile",
    ]

    enum Method: String, CaseIterable {
        case hello
        case settingsList = "settings.list"
        case settingsGet = "settings.get"
        case settingsSet = "settings.set"
        case status
        case open
    }
}

struct AppControlRequest: Codable, Equatable {
    var protocolVersion: Int
    var method: String
    var params: [String: JSONValue]?

    init(method: AppControlProtocol.Method, params: [String: JSONValue]? = nil) {
        self.init(protocolVersion: AppControlProtocol.version, method: method.rawValue, params: params)
    }

    init(protocolVersion: Int, method: String, params: [String: JSONValue]?) {
        self.protocolVersion = protocolVersion
        self.method = method
        self.params = params
    }

    private enum CodingKeys: String, CodingKey {
        case protocolVersion = "protocol"
        case method
        case params
    }
}

/// A stable error the CLI prints as `[code] message`, like the MCP tools'
/// `[access_disabled]`.
struct AppControlError: Codable, Equatable, LocalizedError {
    var code: String
    var message: String
    /// Structured detail an agent can act on, such as the changes that need
    /// approval.
    var details: JSONValue?

    var errorDescription: String? { "[\(code)] \(message)" }

    static let settingsAccessDisabled = AppControlError(
        code: "settings_access_disabled", message: SettingsControlGate.disabledMessage)

    static func unknownKey(_ key: String) -> AppControlError {
        AppControlError(
            code: "unknown_key",
            message: "No setting is named \(key). Run `lokalbot-cli config search <words>` to find one.")
    }

    static func invalidValue(_ key: String, _ message: String) -> AppControlError {
        AppControlError(code: "invalid_value", message: "\(key) \(message).")
    }

    static func readOnly(_ keys: [String]) -> AppControlError {
        AppControlError(
            code: "read_only",
            message: "\(keys.joined(separator: ", ")) can't be changed from the terminal.",
            details: .array(keys.map(JSONValue.string)))
    }

    /// Phase 2 refuses widening changes outright; in-app approval comes next.
    static func needsApproval(_ changes: [JSONValue]) -> AppControlError {
        let keys = changes.compactMap { $0["key"]?.stringValue }
        return AppControlError(
            code: "needs_approval",
            message: "\(keys.joined(separator: ", ")) would widen what LokalBot captures, reads, records, or sends, "
                + "so the person has to change it in LokalBot. Run `lokalbot-cli open settings <key>` to show them. "
                + "Nothing was changed.",
            details: .array(changes))
    }

    static func invalidRequest(_ message: String) -> AppControlError {
        AppControlError(code: "invalid_request", message: message)
    }

    static func unknownMethod(_ method: String) -> AppControlError {
        AppControlError(code: "unknown_method", message: "LokalBot does not handle \(method).")
    }

    static let unsupportedProtocol = AppControlError(
        code: "app_outdated",
        message: "The running LokalBot does not understand this lokalbot-cli. Restart LokalBot to finish updating it.")

    static func appNotRunning(_ reason: String) -> AppControlError {
        AppControlError(code: "app_not_running", message: reason)
    }

    static let appNotResponding = AppControlError(
        code: "app_not_responding",
        message: "LokalBot is running but did not answer. If it was just updated, restart it.")
}

struct AppControlResponse: Codable, Equatable {
    var protocolVersion: Int
    var result: JSONValue?
    var error: AppControlError?

    static func success(_ result: JSONValue) -> AppControlResponse {
        AppControlResponse(protocolVersion: AppControlProtocol.version, result: result, error: nil)
    }

    static func failure(_ error: AppControlError) -> AppControlResponse {
        AppControlResponse(protocolVersion: AppControlProtocol.version, result: nil, error: error)
    }

    private enum CodingKeys: String, CodingKey {
        case protocolVersion = "protocol"
        case result
        case error
    }
}
