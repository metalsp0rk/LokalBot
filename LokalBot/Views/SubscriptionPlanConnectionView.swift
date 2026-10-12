import AppKit
import SwiftUI

/// Account, model, and consent for one subscription plan in Model
/// connections. Signing in, approval, and the model are separate steps:
/// a sign-in alone never lets context leave the Mac.
struct SubscriptionPlanConnectionView: View {
    @ObservedObject var app: AppState
    let plan: SubscriptionPlan
    @Binding var model: String
    var chatGPTAccount: ChatGPTPlanAccount = .shared

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            switch plan {
            case .chatGPT:
                ChatGPTPlanAccountSection(app: app, model: $model, account: chatGPTAccount)
            case .claude:
                ClaudePlanAccountSection(model: $model)
            }
            SubscriptionPlanConsentSection(app: app, plan: plan)
        }
    }
}

// MARK: - ChatGPT

private struct ChatGPTPlanAccountSection: View {
    @ObservedObject var app: AppState
    @Binding var model: String
    @State private var status: ChatGPTPlanStatus?
    @State private var signingIn = false
    @State private var signInTask: Task<Void, Never>?
    @State private var accountError: String?
    @State private var models: [ChatGPTPlanModel] = []
    @State private var listingModels = false
    @State private var showingNotice = false
    @State private var revocationUnconfirmed = false
    let account: ChatGPTPlanAccount

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Use your ChatGPT plan for Think instead of an API key. Requests count toward your plan's usage, which you can cap in ChatGPT settings.")
                .workspaceTextRole(.supporting)
                .fixedSize(horizontal: false, vertical: true)
            accountRow
            if let accountError {
                Label(accountError, systemImage: "exclamationmark.triangle").workspaceTextRole(.warning)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if revocationUnconfirmed {
                Text("Signed out on this Mac, but OpenAI didn't confirm it. You can disconnect LokalBot in ChatGPT settings.")
                    .workspaceTextRole(.supporting)
                    .fixedSize(horizontal: false, vertical: true)
            }
            modelRow
        }
        .task { await refresh() }
        .onDisappear {
            signInTask?.cancel()
            Task { await account.cancelSignIn() }
        }
        .alert("You're using your ChatGPT plan", isPresented: $showingNotice) {
            Button("Got it") { app.settings.chatGPTPlanNoticeAcknowledged = true }
            Button("Manage usage") {
                app.settings.chatGPTPlanNoticeAcknowledged = true
                NSWorkspace.shared.open(SubscriptionPlan.chatGPT.manageUsageURL)
            }
        } message: {
            Text("Eligible requests from LokalBot now use your ChatGPT plan. Manage usage and app access in ChatGPT settings. LokalBot has no charges of its own.")
        }
    }

    @ViewBuilder
    private var accountRow: some View {
        switch status {
        case nil:
            ProgressView().controlSize(.small)
        case .signedOut(let previousEmail)?:
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 12) {
                    Button(signingIn ? "Waiting for your browser…" : "Continue with ChatGPT") {
                        startSignIn(newRegistration: false)
                    }
                    .primaryActionButton()
                    .disabled(signingIn)
                    .accessibilityIdentifier("models.chatGPTPlan.signIn")
                    if signingIn {
                        Button("Cancel") { cancelSignIn() }
                    } else if previousEmail != nil {
                        Button("Use a different account") { startSignIn(newRegistration: true) }
                            .buttonStyle(.workspaceLink)
                    }
                }
                if signingIn {
                    Text("Finish signing in in your browser, then return here.")
                        .workspaceTextRole(.supporting)
                } else if let previousEmail {
                    Text("Last signed in as \(previousEmail).").workspaceTextRole(.supporting)
                }
            }
        case .signedIn(let email, let planUsage)?:
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 10) {
                    Label(email.map { "Connected · \($0)" } ?? "Connected", systemImage: "checkmark.circle.fill")
                        .font(AppFont.scaled(.body))
                        .accessibilityIdentifier("models.chatGPTPlan.connected")
                    Spacer(minLength: 8)
                    Button("Sign out") { signOut() }.disabled(signingIn)
                    Button("Use a different account") { startSignIn(newRegistration: true) }
                        .buttonStyle(.workspaceLink).disabled(signingIn)
                }
                if planUsage {
                    HStack(spacing: 8) {
                        Text("Using ChatGPT plan").font(AppFont.scaled(.body).weight(.medium))
                        Link("Manage usage", destination: SubscriptionPlan.chatGPT.manageUsageURL)
                    }
                } else {
                    Text("This sign-in doesn't allow requests on your ChatGPT plan, so Think can't use it.")
                        .workspaceTextRole(.warning)
                        .fixedSize(horizontal: false, vertical: true)
                    Button(signingIn ? "Waiting for your browser…" : "Allow plan usage…") {
                        startSignIn(newRegistration: false)
                    }
                    .disabled(signingIn)
                }
            }
        }
    }

    private var modelRow: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Model").font(AppFont.scaled(.body))
            HStack(spacing: 8) {
                TextField("Model from your ChatGPT plan", text: $model)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("models.chatGPTPlan.model")
                if !models.isEmpty {
                    Picker("Available models", selection: $model) {
                        Text("Choose a model").tag("")
                        ForEach(models) { Text($0.displayName).tag($0.slug) }
                    }
                    .labelsHidden().frame(maxWidth: 220)
                }
                Button(listingModels ? "Loading…" : "List models") {
                    Task { await loadModels() }
                }
                .disabled(listingModels || status?.allowsPlanUsage != true)
            }
        }
    }

    private func refresh() async {
        status = await account.status()
        if status?.allowsPlanUsage == true, models.isEmpty { await loadModels() }
    }

    private func loadModels() async {
        listingModels = true
        defer { listingModels = false }
        do {
            models = try await account.listModels()
            accountError = models.isEmpty ? "Your ChatGPT plan returned no models for LokalBot." : nil
            if model.isEmpty, let first = models.first { model = first.slug }
        } catch {
            accountError = error.localizedDescription
            status = await account.status()
        }
    }

    private func startSignIn(newRegistration: Bool) {
        signInTask?.cancel()
        accountError = nil
        revocationUnconfirmed = false
        signingIn = true
        signInTask = Task {
            defer { signingIn = false }
            do {
                if newRegistration { await account.forgetRegistration() }
                let result = try await account.signIn(newRegistration: newRegistration) { url in
                    await MainActor.run { NSWorkspace.shared.open(url) }
                }
                status = result
                models = []
                app.modelChecks.invalidate(for: app.settings)
                if result.allowsPlanUsage {
                    if !app.settings.chatGPTPlanNoticeAcknowledged { showingNotice = true }
                    await loadModels()
                }
            } catch is CancellationError {
                status = await account.status()
            } catch {
                accountError = error.localizedDescription
                status = await account.status()
            }
        }
    }

    private func cancelSignIn() {
        signInTask?.cancel()
        Task { await account.cancelSignIn() }
    }

    private func signOut() {
        accountError = nil
        signingIn = true
        Task {
            defer { signingIn = false }
            revocationUnconfirmed = !(await account.signOut())
            status = await account.status()
            models = []
            app.modelChecks.invalidate(for: app.settings)
        }
    }
}

// MARK: - Claude

private struct ClaudePlanAccountSection: View {
    @Binding var model: String
    @State private var executable: URL?
    @State private var authStatus: ClaudeCodeCLI.AuthStatus?
    @State private var version: String?
    @State private var checking = true
    @State private var statusError: String?

    static let setupURL = URL(string: "https://code.claude.com/docs/en/setup")!

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Use your Claude plan for Think through Claude Code, Anthropic's command-line app. LokalBot runs your own Claude Code with its tools turned off. You sign in to Claude Code yourself; LokalBot never sees your Claude sign-in.")
                .workspaceTextRole(.supporting)
                .fixedSize(horizontal: false, vertical: true)
            statusRow
            modelRow
        }
        .task { await check(refresh: false) }
    }

    @ViewBuilder
    private var statusRow: some View {
        VStack(alignment: .leading, spacing: 8) {
            if checking {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("Looking for Claude Code…").workspaceTextRole(.supporting)
                }
            } else if executable == nil {
                Label("Claude Code isn't installed on this Mac.", systemImage: "exclamationmark.triangle")
                    .workspaceTextRole(.warning)
                HStack(spacing: 12) {
                    Link("How to install Claude Code", destination: Self.setupURL)
                    Button("Check again") { Task { await check(refresh: true) } }
                }
            } else if let authStatus, authStatus.loggedIn {
                Label(signedInText(authStatus), systemImage: "checkmark.circle.fill")
                    .font(AppFont.scaled(.body))
                    .accessibilityIdentifier("models.claudePlan.connected")
                if !authStatus.usesSubscription {
                    Text("Claude Code is set to sign in with \(authStatus.authMethod ?? "another method") rather than a Claude plan, so requests are billed that way.")
                        .workspaceTextRole(.warning)
                        .fixedSize(horizontal: false, vertical: true)
                }
                HStack(spacing: 12) {
                    Link("Manage usage", destination: SubscriptionPlan.claude.manageUsageURL)
                    Button("Check again") { Task { await check(refresh: true) } }
                }
            } else {
                Label("Claude Code isn't signed in.", systemImage: "person.crop.circle.badge.exclamationmark")
                    .workspaceTextRole(.warning)
                Text("Open Terminal, run claude, and sign in with your Claude account. Then check again.")
                    .workspaceTextRole(.supporting)
                    .fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 12) {
                    Button("Open Terminal") { openTerminal() }
                    Button("Check again") { Task { await check(refresh: true) } }
                }
            }
            if let statusError {
                Text(statusError).workspaceTextRole(.warning).fixedSize(horizontal: false, vertical: true)
            }
            if let executable, !checking {
                Text([executable.path, version].compactMap { $0 }.joined(separator: " · "))
                    .font(AppFont.scaled(.callout)).settingsSecondary().textSelection(.enabled)
            }
        }
    }

    private var modelRow: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Model").font(AppFont.scaled(.body))
            HStack(spacing: 8) {
                TextField("Claude Code default", text: $model)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("models.claudePlan.model")
                Menu("Choose") {
                    Button("Claude Code default") { model = "" }
                    Button("sonnet") { model = "sonnet" }
                    Button("opus") { model = "opus" }
                    Button("haiku") { model = "haiku" }
                }
                .fixedSize()
            }
            Text("An alias follows Anthropic's newest model of that family. A full model ID also works.")
                .workspaceTextRole(.supporting)
        }
    }

    private func signedInText(_ status: ClaudeCodeCLI.AuthStatus) -> String {
        var parts = ["Signed in"]
        if let email = status.email { parts.append(email) }
        if let plan = status.planName { parts.append("\(plan) plan") }
        return parts.joined(separator: " · ")
    }

    private func check(refresh: Bool) async {
        checking = true
        statusError = nil
        defer { checking = false }
        executable = await ClaudeCodeLocator.shared.executable(refresh: refresh)
        guard let executable else {
            authStatus = nil
            return
        }
        version = await ClaudeCodeCLI.version(executable: executable)
        do {
            authStatus = try await ClaudeCodeCLI.authStatus(executable: executable)
        } catch {
            authStatus = nil
            statusError = error.localizedDescription
        }
    }

    private func openTerminal() {
        guard let terminal = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.Terminal") else {
            return
        }
        NSWorkspace.shared.openApplication(at: terminal, configuration: NSWorkspace.OpenConfiguration())
    }
}

// MARK: - Consent

private struct SubscriptionPlanConsentSection: View {
    @ObservedObject var app: AppState
    let plan: SubscriptionPlan

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("Remote processing · \(plan.providerName)", systemImage: "network")
                .font(AppFont.scaled(.body).weight(.semibold))
            Text("Think may send meeting transcripts, screen text, and inherited dictation requests to "
                 + "\(plan.providerName) with your \(plan.title) when you use those features. Scheduled daily "
                 + "summaries and overnight review also run there and can send activity titles, captured screen "
                 + "text, meeting evidence, and retained Dream memory without a prompt each time. "
                 + "\(plan.providerName)'s terms for your account apply to this content.")
                .workspaceTextRole(.trust)
                .fixedSize(horizontal: false, vertical: true)
            Toggle("Allow sending context to \(plan.providerName) with my \(plan.title)", isOn: approval)
                .font(AppFont.scaled(.body))
                .accessibilityIdentifier("models.planConsent")
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.quaternary.opacity(0.3), in: RoundedRectangle(cornerRadius: Brand.Radius.row))
    }

    private var approval: Binding<Bool> {
        Binding {
            plan.isApproved(in: app.settings)
        } set: { approved in
            app.settings.approvedRemoteInferenceOrigins.removeAll { $0 == plan.consentKey }
            if approved { app.settings.approvedRemoteInferenceOrigins.append(plan.consentKey) }
        }
    }
}
