import Combine
import Foundation

/// The app's single owner of the personal model. Ask reads and adds claims
/// through it; Settings → Day Memory → About you edits them. Loads lazily on
/// first use so launch never touches the Keychain for it. If the saved file
/// can't be read, every edit is refused instead of overwriting it.
@MainActor
final class PersonalModelController: ObservableObject {
    @Published private(set) var claims: [PersonalClaim] = []
    @Published private(set) var loadError: String?

    private let store: PersonalModelStore
    private let now: () -> Date
    private var model = PersonalModel()
    private var loaded = false
    private var writable = false
    /// Called after every saved change, forget-all and reload, so the
    /// agent-facing projection never lags behind (nil claims: unreadable).
    var onChange: ((_ claims: [PersonalClaim]?) -> Void)?

    init(store: PersonalModelStore, now: @escaping () -> Date = Date.init) {
        self.store = store
        self.now = now
    }

    /// Newest first, the order Settings shows.
    var orderedClaims: [PersonalClaim] {
        loadIfNeeded()
        return claims.sorted { $0.updatedAt > $1.updatedAt }
    }

    func currentClaims() -> [PersonalClaim] {
        loadIfNeeded()
        return claims
    }

    /// The claims, or nil when the saved file couldn't be read. Agent access
    /// fails closed on nil instead of treating it as "nothing shared".
    func readableClaims() -> [PersonalClaim]? {
        loadIfNeeded()
        return writable ? claims : nil
    }

    func claim(id: UUID) -> PersonalClaim? {
        loadIfNeeded()
        return claims.first { $0.id == id }
    }

    func loadIfNeeded() {
        guard !loaded else { return }
        reload()
    }

    func reload() {
        loaded = true
        do {
            model = try store.load()
            writable = true
            loadError = nil
        } catch {
            model = PersonalModel()
            writable = false
            loadError = Self.message(for: error)
            lokalbotLog("personal model load failed: \(error)")
        }
        claims = model.claims
        onChange?(writable ? claims : nil)
    }

    @discardableResult
    func remember(_ text: String, kind: PersonalClaim.Kind? = nil,
                  origin: PersonalClaim.Origin) -> Result<PersonalClaim, PersonalModel.EditError> {
        mutate { model, date in
            model.add(text, kind: kind ?? PersonalClaim.Kind.suggested(for: text), origin: origin, now: date)
        }
    }

    @discardableResult
    func update(_ id: UUID, text: String? = nil, kind: PersonalClaim.Kind? = nil,
                alwaysUse: Bool? = nil,
                sharing: PersonalClaim.Sharing? = nil) -> Result<PersonalClaim, PersonalModel.EditError> {
        mutate { model, date in
            model.update(id: id, text: text, kind: kind, alwaysUse: alwaysUse, sharing: sharing, now: date)
        }
    }

    @discardableResult
    func forget(_ ids: Set<UUID>) -> Result<Void, PersonalModel.EditError> {
        mutate { model, _ in
            model.remove(ids: ids)
            return .success(())
        }
    }

    @discardableResult
    func forgetAll() -> Result<Void, PersonalModel.EditError> {
        loadIfNeeded()
        do {
            try store.deleteAll()
        } catch {
            lokalbotLog("personal model delete failed: \(error)")
            return .failure(.unavailable)
        }
        model = PersonalModel()
        claims = []
        writable = true
        loadError = nil
        onChange?(claims)
        return .success(())
    }

    /// Stamps claims that went into an answer. Best effort: a failed write
    /// only loses the timestamp.
    func markUsed(_ ids: Set<UUID>) {
        guard !ids.isEmpty else { return }
        _ = mutate { model, date in
            model.markUsed(ids: ids, at: date)
            return .success(())
        }
    }

    private func mutate<T>(
        _ body: (inout PersonalModel, Date) -> Result<T, PersonalModel.EditError>
    ) -> Result<T, PersonalModel.EditError> {
        loadIfNeeded()
        guard writable else { return .failure(.unavailable) }
        var updated = model
        let result = body(&updated, now())
        guard case .success = result, updated != model else { return result }
        do {
            try store.save(updated)
        } catch {
            lokalbotLog("personal model save failed: \(error)")
            return .failure(.unavailable)
        }
        model = updated
        claims = updated.claims
        onChange?(claims)
        return result
    }

    private static func message(for error: Error) -> String {
        switch error as? PersonalModelStore.LoadError {
        case .keyUnavailable:
            "LokalBot couldn't unlock what it remembers about you. It's kept as is and nothing new is saved until it can."
        case .newerVersion:
            "What LokalBot remembers about you was saved by a newer version. Update LokalBot to change it."
        case .unreadable, nil:
            "LokalBot couldn't read what it remembers about you. The file is kept as is and nothing new is saved."
        }
    }
}
