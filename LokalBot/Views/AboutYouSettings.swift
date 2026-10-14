import SwiftUI

/// Settings → Day Memory → About you: what the person told LokalBot about
/// themselves. Phase 1 of the personal world model holds only their own
/// statements, so every row can be edited or forgotten here.
struct AboutYouSettings: View {
    @EnvironmentObject private var app: AppState
    @ObservedObject var personalModel: PersonalModelController
    @ObservedObject var access: PersonalContextAccessManager
    @State private var editing: EditorTarget?
    @State private var confirmingForgetAll = false
    @State private var error: String?

    enum EditorTarget: Identifiable {
        case add
        case edit(PersonalClaim)

        var id: String {
            switch self {
            case .add: "add"
            case .edit(let claim): claim.id.uuidString
            }
        }
    }

    var body: some View {
        Group {
            Toggle(isOn: $app.settings.askMemoryEnabled) {
                SettingsLabel("Remember what I tell Ask",
                              help: "Type “Remember …” in Ask or Quick Recall, or add items here. Ask gives the relevant ones to your Think model with each question.")
            }
            .settingTarget("settings.askMemoryEnabled", selected: app.focusedSettingID)
            .accessibilityIdentifier("settings.askMemory")
            if let loadError = personalModel.loadError {
                Label(loadError, systemImage: "exclamationmark.triangle")
                    .workspaceTextRole(.warning)
            }
            let claims = personalModel.orderedClaims
            if claims.isEmpty {
                SettingsHelp("Nothing yet. Try “Remember that Maya is my manager” or “Remember: answer in bullet points”.")
            } else {
                ForEach(claims) { claim in
                    row(claim)
                }
            }
            agentAccessLine
            HStack(spacing: 8) {
                Button("Add…") { editing = .add }
                    .disabled(personalModel.loadError != nil)
                    .accessibilityIdentifier("settings.aboutYou.add")
                if !claims.isEmpty {
                    Button("Forget everything…", role: .destructive) { confirmingForgetAll = true }
                }
            }
            if let error { Text(error).workspaceTextRole(.warning) }
            SettingsDetails("How LokalBot uses this",
                            "Items stay on this Mac, encrypted with the same key as your Ask history. Each question gets "
                                + "the items marked Always use, your principles and preferences, and the facts and goals "
                                + "that share words with it, up to about 2,000 characters. If your Think model is a "
                                + "remote server you approved, they are sent there with the question. External agents "
                                + "(MCP clients and the CLI) read only items you mark Share with agents, and only while "
                                + "personal context access is on in Settings → Privacy. Agent Mode, Autocomplete, exports, "
                                + "and diagnostics never get them. Turning this off stops saving and using them in Ask; "
                                + "forget them here to delete them.")
        }
        .onAppear { personalModel.loadIfNeeded() }
        .sheet(item: $editing) { target in
            PersonalClaimEditor(target: target) { text, kind, sharing in
                save(target: target, text: text, kind: kind, sharing: sharing)
            }
        }
        .confirmationDialog(
            "Forget everything LokalBot remembers about you?",
            isPresented: $confirmingForgetAll, titleVisibility: .visible
        ) {
            Button("Forget Everything", role: .destructive) {
                if case .failure(let failure) = personalModel.forgetAll() {
                    error = failure.localizedDescription
                } else {
                    error = nil
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Deletes every fact, preference, goal, and principle saved here. Your Ask history is kept.")
        }
    }

    private func row(_ claim: PersonalClaim) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: claim.kind.systemImage)
                .foregroundStyle(Brand.teal)
                .frame(width: 18)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(claim.text)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                SettingsHelp(Self.originLine(for: claim))
            }
            Spacer(minLength: 8)
            if claim.sharing == .agents {
                Image(systemName: "person.2.wave.2")
                    .foregroundStyle(access.isEnabled ? AnyShapeStyle(Brand.teal) : AnyShapeStyle(.secondary))
                    .help(access.isEnabled ? "Shared with agents" : "Marked for agents; personal context access is off")
                    .accessibilityLabel("Shared with agents")
            }
            if claim.alwaysUse {
                Image(systemName: "pin.fill")
                    .foregroundStyle(.secondary)
                    .help("Always used")
                    .accessibilityLabel("Always used")
            }
            Menu {
                Button("Edit…") { editing = .edit(claim) }
                Button(claim.alwaysUse ? "Stop Always Using" : "Always Use") {
                    report(personalModel.update(claim.id, alwaysUse: !claim.alwaysUse))
                }
                Button(claim.sharing == .agents ? "Stop Sharing with Agents" : "Share with Agents") {
                    report(personalModel.update(
                        claim.id, sharing: claim.sharing == .agents ? .lokalBotOnly : .agents))
                }
                Divider()
                Button("Forget", role: .destructive) { report(personalModel.forget([claim.id])) }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .accessibilityLabel("Actions for \(claim.text)")
        }
        .contextMenu {
            Button("Edit…") { editing = .edit(claim) }
            Button("Forget", role: .destructive) { report(personalModel.forget([claim.id])) }
        }
        .accessibilityIdentifier("settings.aboutYou.claim")
    }

    @ViewBuilder private var agentAccessLine: some View {
        let shared = personalModel.claims.filter { $0.sharing == .agents }.count
        if access.isEnabled {
            SettingsHelp(shared == 0
                ? "Personal context access is on, but nothing is shared yet. Choose Share with Agents on an item."
                : shared == 1 ? "Agents can read 1 shared item, when it's relevant to their task."
                : "Agents can read \(shared) shared items, when they're relevant to their task.")
        } else if shared > 0 {
            HStack(spacing: 6) {
                SettingsHelp(shared == 1 ? "1 item is marked for agents, but personal context access is off."
                                         : "\(shared) items are marked for agents, but personal context access is off.")
                Button("Privacy settings…") {
                    app.settingsTab = .privacy
                    app.focusedSettingID = "settings.personalContextAccess"
                }
                .buttonStyle(.workspaceLink)
            }
        }
    }

    static func originLine(for claim: PersonalClaim) -> String {
        let day = claim.createdAt.formatted(date: .abbreviated, time: .omitted)
        let origin: String
        switch claim.origin {
        case .said: origin = "You said this in Ask"
        case .added: origin = "Added in Settings"
        }
        return "\(claim.kind.displayName) · \(origin) · \(day)"
    }

    private func save(target: EditorTarget, text: String, kind: PersonalClaim.Kind,
                      sharing: PersonalClaim.Sharing) -> String? {
        var result: Result<PersonalClaim, PersonalModel.EditError>
        switch target {
        case .add:
            result = personalModel.remember(text, kind: kind, origin: .added)
            if case .success(let claim) = result, claim.sharing != sharing {
                result = personalModel.update(claim.id, sharing: sharing)
            }
        case .edit(let claim):
            result = personalModel.update(claim.id, text: text, kind: kind, sharing: sharing)
        }
        switch result {
        case .success:
            error = nil
            return nil
        case .failure(let failure):
            return failure.localizedDescription
        }
    }

    private func report<T>(_ result: Result<T, PersonalModel.EditError>) {
        if case .failure(let failure) = result {
            error = failure.localizedDescription
        } else {
            error = nil
        }
    }
}

/// Add or edit one claim. Returns an error message from `onSave` to keep the
/// sheet open; nil closes it.
struct PersonalClaimEditor: View {
    @Environment(\.dismiss) private var dismiss
    let target: AboutYouSettings.EditorTarget
    let onSave: (String, PersonalClaim.Kind, PersonalClaim.Sharing) -> String?
    @State private var text: String
    @State private var kind: PersonalClaim.Kind
    @State private var sharesWithAgents: Bool
    @State private var error: String?

    init(target: AboutYouSettings.EditorTarget,
         onSave: @escaping (String, PersonalClaim.Kind, PersonalClaim.Sharing) -> String?) {
        self.target = target
        self.onSave = onSave
        switch target {
        case .add:
            _text = State(initialValue: "")
            _kind = State(initialValue: .fact)
            _sharesWithAgents = State(initialValue: false)
        case .edit(let claim):
            _text = State(initialValue: claim.text)
            _kind = State(initialValue: claim.kind)
            _sharesWithAgents = State(initialValue: claim.sharing == .agents)
        }
    }

    private var isAdding: Bool {
        if case .add = target { return true }
        return false
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(isAdding ? "Add to About you" : "Edit")
                .font(.scaled(.title3).weight(.semibold))
            Picker("Kind", selection: $kind) {
                ForEach(PersonalClaim.Kind.allCases, id: \.self) { kind in
                    Label(kind.displayName, systemImage: kind.systemImage).tag(kind)
                }
            }
            .pickerStyle(.segmented)
            .accessibilityIdentifier("settings.aboutYou.kind")
            TextField(Self.placeholder(for: kind), text: $text, axis: .vertical)
                .lineLimit(3...6)
                .textFieldStyle(.roundedBorder)
                .accessibilityIdentifier("settings.aboutYou.text")
            Toggle("Share with agents", isOn: $sharesWithAgents)
                .help("MCP clients and the CLI can read this item while personal context access is on in Settings → Privacy.")
                .accessibilityIdentifier("settings.aboutYou.share")
            HStack {
                Text("\(text.count)/\(PersonalModel.maxCharacters)")
                    .font(.scaled(.caption).monospacedDigit())
                    .foregroundStyle(text.count > PersonalModel.maxCharacters ? AnyShapeStyle(Brand.error)
                                                                               : AnyShapeStyle(.secondary))
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Save") {
                    if let failure = onSave(text, kind, sharesWithAgents ? .agents : .lokalBotOnly) {
                        error = failure
                    } else {
                        dismiss()
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            if let error { Text(error).workspaceTextRole(.warning) }
        }
        .padding(20)
        .frame(width: 460)
    }

    static func placeholder(for kind: PersonalClaim.Kind) -> String {
        switch kind {
        case .fact: "Maya Chen is my manager."
        case .preference: "Answer in bullet points, amounts in EUR."
        case .goal: "Ship LokalBot 1.0 this year."
        case .principle: "I don't take work that requires relocating my family."
        }
    }
}
