import Foundation

/// Think backends that run on the user's own ChatGPT or Claude plan instead
/// of an API key. Requests count toward that plan's usage limits.
///
/// Each plan has its own approval entry in
/// `AppSettings.approvedRemoteInferenceOrigins`. The entries are not URL
/// origins, so approving an API server (`https://api.openai.com` with a key)
/// never approves plan usage, and the reverse: the two run under different
/// provider terms.
enum SubscriptionPlan: String, CaseIterable, Identifiable, Sendable {
    case chatGPT
    case claude

    var id: String { rawValue }

    init?(backend: AppSettings.SummarizerBackend) {
        switch backend {
        case .chatGPTPlan: self = .chatGPT
        case .claudePlan: self = .claude
        case .builtIn, .appleIntelligence, .ollama, .openAICompatible: return nil
        }
    }

    var backend: AppSettings.SummarizerBackend {
        switch self {
        case .chatGPT: .chatGPTPlan
        case .claude: .claudePlan
        }
    }

    var title: String {
        switch self {
        case .chatGPT: "ChatGPT plan"
        case .claude: "Claude plan"
        }
    }

    var providerName: String {
        switch self {
        case .chatGPT: "OpenAI"
        case .claude: "Anthropic"
        }
    }

    /// Where Think content goes. Claude requests travel through the user's
    /// own Claude Code installation, which sends them to Anthropic.
    var destinationHost: String {
        switch self {
        case .chatGPT: "api.openai.com"
        case .claude: "api.anthropic.com"
        }
    }

    /// The approval entry, also used as the request gate's origin.
    var consentKey: String {
        switch self {
        case .chatGPT: "chatgpt-plan://api.openai.com"
        case .claude: "claude-plan://api.anthropic.com"
        }
    }

    /// The provider's own page for checking and capping plan usage.
    var manageUsageURL: URL {
        switch self {
        case .chatGPT: URL(string: "https://chatgpt.com/settings/usage")!
        case .claude: URL(string: "https://claude.ai/settings/usage")!
        }
    }

    static func plan(forConsentKey key: String) -> SubscriptionPlan? {
        allCases.first { $0.consentKey == key }
    }

    func isApproved(in settings: AppSettings) -> Bool {
        settings.approvedRemoteInferenceOrigins.contains(consentKey)
    }

    func model(in settings: AppSettings) -> String {
        switch self {
        case .chatGPT: settings.chatGPTPlanModel
        case .claude: settings.claudePlanModel
        }
    }

    /// The approval list shows a plan entry by name rather than its key.
    static func approvalDisplayName(_ entry: String) -> String {
        guard let plan = plan(forConsentKey: entry) else { return entry }
        return "\(plan.title) · \(plan.destinationHost)"
    }
}

enum SubscriptionPlanError: LocalizedError, Equatable, Sendable {
    case approvalRequired(SubscriptionPlan)
    case noModel(SubscriptionPlan)

    var errorDescription: String? {
        switch self {
        case .approvalRequired(let plan):
            "Allow sending context to \(plan.providerName) with your \(plan.title) under Settings → Models → Think before Think uses it."
        case .noModel(let plan):
            "Choose a model for your \(plan.title) under Settings → Models."
        }
    }
}
