import SwiftUI

/// Settings → Day Memory → About you: what the person told LokalBot about
/// themselves. Phase 1 of the personal world model holds only their own
/// statements, so every row can be edited or forgotten here.
struct AboutYouSettings: View {
    @EnvironmentObject private var app: AppState
    @ObservedObject var personalModel: PersonalModelController
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
                                + "remote server you approved, they are sent there with the question. They are never "
                                + "given to MCP clients, the CLI, Agent Mode, Autocomplete, exports, or diagnostics. "
                                + "Turning this off stops saving and using them; forget them here to delete them.")
        }
        .onAppear { personalModel.loadIfNeeded() }
        .sheet(item: $editing) { target in
            PersonalClaimEditor(target: target) { text, kind in
                save(target: target, text: text, kind: kind)
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

    static func originLine(for claim: PersonalClaim) -> String {
        let day = claim.createdAt.formatted(date: .abbreviated, time: .omitted)
        let origin: String
        switch claim.origin {
        case .said: origin = "You said this in Ask"
        case .added: origin = "Added in Settings"
        }
        return "\(claim.kind.displayName) · \(origin) · \(day)"
    }

    private func save(target: EditorTarget, text: String, kind: PersonalClaim.Kind) -> String? {
        let result: Result<PersonalClaim, PersonalModel.EditError>
        switch target {
        case .add:
            result = personalModel.remember(text, kind: kind, origin: .added)
        case .edit(let claim):
            result = personalModel.update(claim.id, text: text, kind: kind)
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
    let onSave: (String, PersonalClaim.Kind) -> String?
    @State private var text: String
    @State private var kind: PersonalClaim.Kind
    @State private var error: String?

    init(target: AboutYouSettings.EditorTarget,
         onSave: @escaping (String, PersonalClaim.Kind) -> String?) {
        self.target = target
        self.onSave = onSave
        switch target {
        case .add:
            _text = State(initialValue: "")
            _kind = State(initialValue: .fact)
        case .edit(let claim):
            _text = State(initialValue: claim.text)
            _kind = State(initialValue: claim.kind)
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
            HStack {
                Text("\(text.count)/\(PersonalModel.maxCharacters)")
                    .font(.scaled(.caption).monospacedDigit())
                    .foregroundStyle(text.count > PersonalModel.maxCharacters ? AnyShapeStyle(Brand.error)
                                                                               : AnyShapeStyle(.secondary))
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Save") {
                    if let failure = onSave(text, kind) {
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
