import Foundation

/// This Mac's registration with OpenAI. It survives sign-out so the next
/// sign-in reuses the issued client id instead of registering again.
struct ChatGPTPlanRegistration: Codable, Equatable, Sendable {
    var clientID: String
    var subject: String
    var email: String?
}

/// One signed-in session. Stored in the Keychain only.
struct ChatGPTPlanCredentials: Codable, Equatable, Sendable {
    var clientID: String
    var subject: String
    var email: String?
    var idToken: String
    var accessToken: String
    var refreshToken: String?
    var expiresAt: Date
    var scopes: Set<String>

    var allowsPlanUsage: Bool { scopes.contains(ChatGPTPlanOAuth.planScope) }
}

protocol ChatGPTPlanStore: Sendable {
    func hostID() -> String
    func registration() -> ChatGPTPlanRegistration?
    func setRegistration(_ registration: ChatGPTPlanRegistration?)
    func credentials() -> ChatGPTPlanCredentials?
    func setCredentials(_ credentials: ChatGPTPlanCredentials?)
}

/// Keychain items under the app's own service, like the API key.
struct KeychainChatGPTPlanStore: ChatGPTPlanStore {
    static let hostAccount = "chatgpt-plan-host-id"
    static let registrationAccount = "chatgpt-plan-registration"
    static let credentialsAccount = "chatgpt-plan-credentials"

    /// An opaque per-Mac id OpenAI uses to tell this host's sessions apart.
    /// Random, never derived from the account.
    func hostID() -> String {
        if let existing = KeychainSecrets.string(account: Self.hostAccount), !existing.isEmpty {
            return existing
        }
        let created = "urn:uuid:\(UUID().uuidString.lowercased())"
        KeychainSecrets.setString(created, account: Self.hostAccount)
        return KeychainSecrets.string(account: Self.hostAccount) ?? created
    }

    func registration() -> ChatGPTPlanRegistration? {
        decode(Self.registrationAccount)
    }

    func setRegistration(_ registration: ChatGPTPlanRegistration?) {
        encode(registration, account: Self.registrationAccount)
    }

    func credentials() -> ChatGPTPlanCredentials? {
        decode(Self.credentialsAccount)
    }

    func setCredentials(_ credentials: ChatGPTPlanCredentials?) {
        encode(credentials, account: Self.credentialsAccount)
    }

    private func decode<T: Decodable>(_ account: String) -> T? {
        guard let data = KeychainSecrets.data(account: account) else { return nil }
        return try? JSONDecoder().decode(T.self, from: data)
    }

    private func encode<T: Encodable>(_ value: T?, account: String) {
        guard let value, let data = try? JSONEncoder().encode(value) else {
            KeychainSecrets.delete(account: account)
            return
        }
        KeychainSecrets.set(data, account: account)
    }
}

enum ChatGPTPlanStatus: Equatable, Sendable {
    case signedOut(previousEmail: String?)
    case signedIn(email: String?, planUsage: Bool)

    var isSignedIn: Bool {
        if case .signedIn = self { return true }
        return false
    }

    var allowsPlanUsage: Bool {
        if case .signedIn(_, let planUsage) = self { return planUsage }
        return false
    }
}

struct ChatGPTPlanModel: Equatable, Identifiable, Sendable {
    let slug: String
    let displayName: String
    var id: String { slug }

    /// The token-sharing model list (`models` with `slug`, `display_name`
    /// and `visibility`), or the classic `data` list of ids.
    static func parseList(_ data: Data) -> [ChatGPTPlanModel] {
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return [] }
        if let models = json["models"] as? [[String: Any]] {
            return models.compactMap { model in
                guard let slug = model["slug"] as? String, !slug.isEmpty,
                      (model["visibility"] as? String ?? "list") == "list" else { return nil }
                return ChatGPTPlanModel(slug: slug, displayName: model["display_name"] as? String ?? slug)
            }
        }
        let entries = json["data"] as? [[String: Any]] ?? []
        return entries.compactMap { entry in
            (entry["id"] as? String).map { ChatGPTPlanModel(slug: $0, displayName: $0) }
        }
    }
}

/// Signs in with ChatGPT, keeps the session's tokens fresh, and signs out.
/// One shared instance serializes refreshes, so two requests never race a
/// rotating refresh token.
actor ChatGPTPlanAccount {
    typealias Transport = @Sendable (URLRequest) async throws -> (Data, URLResponse)
    typealias BrowserOpener = @Sendable (URL) async -> Bool

    static let shared = ChatGPTPlanAccount()

    /// Refresh this long before the access token expires.
    static let refreshMargin: TimeInterval = 120
    static let signInTimeout: Duration = .seconds(300)

    private let store: ChatGPTPlanStore
    private let transport: Transport
    private let now: @Sendable () -> Date
    private var refreshTask: Task<ChatGPTPlanCredentials, Error>?
    private var signInListener: LoopbackRedirectListener?
    private var cachedModels: (subject: String, models: [ChatGPTPlanModel], fetchedAt: Date)?
    /// The model list changes rarely; settings views share one fetch.
    static let modelListLifetime: TimeInterval = 600

    init(store: ChatGPTPlanStore = KeychainChatGPTPlanStore(),
         transport: @escaping Transport = { try await ChatGPTPlanAccount.liveTransport($0) },
         now: @escaping @Sendable () -> Date = { Date() }) {
        self.store = store
        self.transport = transport
        self.now = now
    }

    /// Account endpoints are fixed OpenAI hosts; never follow a redirect away.
    static func liveTransport(_ request: URLRequest) async throws -> (Data, URLResponse) {
        do {
            return try await accountSession.data(for: request)
        } catch let error as URLError where error.code == .cancelled {
            throw CancellationError()
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            throw TextEngineError.serverUnreachable(request.url?.host ?? "OpenAI", transportCode: (error as NSError).code)
        }
    }

    private static let accountSession = InferenceURLSession.make(requestTimeout: 30, resourceTimeout: 60)

    func status() -> ChatGPTPlanStatus {
        if let credentials = store.credentials() {
            return .signedIn(email: credentials.email, planUsage: credentials.allowsPlanUsage)
        }
        return .signedOut(previousEmail: store.registration()?.email)
    }

    // MARK: Sign-in

    /// Opens OpenAI's sign-in page and completes the authorization code
    /// exchange when the browser returns to the loopback listener.
    /// `newRegistration` registers again, for signing in to another account.
    func signIn(newRegistration: Bool = false, openBrowser: BrowserOpener) async throws -> ChatGPTPlanStatus {
        signInListener?.stop()
        let listener = try await LoopbackRedirectListener.start(
            preferredPort: ChatGPTPlanOAuth.preferredCallbackPort,
            path: ChatGPTPlanOAuth.callbackPath)
        signInListener = listener
        defer {
            listener.stop()
            if signInListener === listener { signInListener = nil }
        }
        let registration = newRegistration ? nil : store.registration()
        let previous = store.credentials()
        let redirectURI = ChatGPTPlanOAuth.redirectURI(port: listener.port)
        let request = ChatGPTPlanOAuth.AuthorizationRequest(
            clientID: registration?.clientID,
            hostID: store.hostID(),
            redirectURI: redirectURI,
            state: ChatGPTPlanOAuth.randomToken(),
            nonce: ChatGPTPlanOAuth.randomToken(),
            pkce: ChatGPTPlanOAuth.PKCE(),
            idTokenHint: previous?.clientID == registration?.clientID ? previous?.idToken : nil,
            loginHint: registration?.email)
        guard await openBrowser(request.url) else {
            throw ChatGPTPlanError.signInFailed("LokalBot couldn't open your browser for ChatGPT sign-in.")
        }
        let query = try await listener.waitForCallback(timeout: Self.signInTimeout)
        let outcome = try ChatGPTPlanOAuth.parseCallback(
            query, expectedState: request.state, expectedClientID: registration?.clientID)
        guard case .code(let code, let returnedClientID) = outcome else {
            throw ChatGPTPlanError.signInFailed("ChatGPT sign-in was cancelled.")
        }
        guard let clientID = registration?.clientID ?? returnedClientID else {
            throw ChatGPTPlanError.signInFailed("ChatGPT didn't return this app's registration. Try again.")
        }
        // The registration is recorded before the exchange, so a failed
        // exchange never strands a client OpenAI already issued.
        if registration == nil, let returnedClientID {
            store.setRegistration(ChatGPTPlanRegistration(clientID: returnedClientID, subject: "", email: nil))
        }
        let (data, response) = try await transport(ChatGPTPlanOAuth.codeExchangeRequest(
            code: code, clientID: clientID, verifier: request.pkce.verifier, redirectURI: redirectURI))
        guard let httpStatus = (response as? HTTPURLResponse)?.statusCode, (200...299).contains(httpStatus) else {
            let code = ChatGPTPlanOAuth.oauthErrorCode(data) ?? "HTTP \((response as? HTTPURLResponse)?.statusCode ?? 0)"
            throw ChatGPTPlanError.signInFailed("ChatGPT sign-in couldn't be completed (\(code)). Try again.")
        }
        let tokens = try ChatGPTPlanOAuth.TokenResponse(json: data)
        guard let idToken = tokens.idToken else {
            throw ChatGPTPlanError.signInFailed("ChatGPT didn't return an ID token.")
        }
        let claims = try ChatGPTIDToken.validate(
            idToken, keys: try await signingKeys(), clientID: clientID, nonce: request.nonce, now: now())
        if let registration, !registration.subject.isEmpty, registration.subject != claims.subject {
            throw ChatGPTPlanError.signInFailed(
                "You signed in to a different ChatGPT account than this registration. "
                    + "Choose \"Use a different account\" to switch accounts.")
        }
        let email = claims.email ?? registration?.email
        store.setRegistration(ChatGPTPlanRegistration(clientID: clientID, subject: claims.subject, email: email))
        let credentials = ChatGPTPlanCredentials(
            clientID: clientID,
            subject: claims.subject,
            email: email,
            idToken: idToken,
            accessToken: tokens.accessToken,
            refreshToken: tokens.refreshToken,
            expiresAt: now().addingTimeInterval(tokens.expiresIn),
            scopes: tokens.scopes)
        refreshTask?.cancel()
        refreshTask = nil
        cachedModels = nil
        store.setCredentials(credentials)
        lokalbotLog("chatgpt plan signed in planUsage=\(credentials.allowsPlanUsage)")
        return status()
    }

    func cancelSignIn() {
        signInListener?.stop()
        signInListener = nil
    }

    /// Stops using the session, asks OpenAI to revoke it, and clears its
    /// tokens. The registration and host id stay for the next sign-in.
    /// Returns false when revocation could not be confirmed.
    @discardableResult
    func signOut() async -> Bool {
        refreshTask?.cancel()
        refreshTask = nil
        cachedModels = nil
        guard let credentials = store.credentials() else { return true }
        store.setCredentials(nil)
        guard let refreshToken = credentials.refreshToken else { return true }
        for attempt in 0..<3 {
            do {
                guard let endpoint = try await discovery().revocationEndpoint else { return false }
                let (_, response) = try await transport(ChatGPTPlanOAuth.revocationRequest(
                    endpoint: endpoint, refreshToken: refreshToken, clientID: credentials.clientID))
                let status = (response as? HTTPURLResponse)?.statusCode ?? 0
                if (200...299).contains(status) { return true }
                if status < 500 { return false }
            } catch {
                if error is CancellationError { return false }
            }
            try? await Task.sleep(for: .seconds(Double(1 << attempt)))
        }
        return false
    }

    /// Forgets this Mac's registration so the next sign-in registers anew.
    func forgetRegistration() async {
        await signOut()
        store.setRegistration(nil)
    }

    // MARK: Tokens

    /// A current access token that may draw on the ChatGPT plan.
    func accessToken() async throws -> String {
        guard let credentials = store.credentials() else { throw ChatGPTPlanError.signedOut }
        guard credentials.allowsPlanUsage else { throw ChatGPTPlanError.planUsageNotGranted }
        if credentials.expiresAt > now().addingTimeInterval(Self.refreshMargin) {
            return credentials.accessToken
        }
        return try await refreshed(from: credentials).accessToken
    }

    /// A token the API rejected may have been revoked or rotated early.
    /// Refreshes once unless another caller already replaced it.
    func accessTokenAfterRejection(of rejected: String) async throws -> String {
        guard let credentials = store.credentials() else { throw ChatGPTPlanError.signedOut }
        if credentials.accessToken != rejected { return credentials.accessToken }
        return try await refreshed(from: credentials).accessToken
    }

    private func refreshed(from credentials: ChatGPTPlanCredentials) async throws -> ChatGPTPlanCredentials {
        if let refreshTask { return try await refreshTask.value }
        let task = Task { try await self.performRefresh(credentials) }
        refreshTask = task
        defer { if refreshTask == task { refreshTask = nil } }
        return try await task.value
    }

    private func performRefresh(_ credentials: ChatGPTPlanCredentials) async throws -> ChatGPTPlanCredentials {
        guard let refreshToken = credentials.refreshToken else {
            store.setCredentials(nil)
            throw ChatGPTPlanError.signInRequired
        }
        let (data, response) = try await transport(ChatGPTPlanOAuth.refreshRequest(
            refreshToken: refreshToken, clientID: credentials.clientID))
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200...299).contains(status) else {
            let code = ChatGPTPlanOAuth.oauthErrorCode(data)
            if let code, ChatGPTPlanOAuth.terminalRefreshErrors.contains(code) {
                lokalbotLog("chatgpt plan refresh rejected code=\(code)")
                // The session is over; the registration stays for sign-in.
                if store.credentials()?.refreshToken == refreshToken { store.setCredentials(nil) }
                throw ChatGPTPlanError.signInRequired
            }
            if code == "invalid_client" {
                throw ChatGPTPlanError.misconfigured("OpenAI rejected this app's registration (invalid_client).")
            }
            throw TextEngineError.fromHTTPResponse(response as? HTTPURLResponse, data: data)
        }
        let tokens = try ChatGPTPlanOAuth.TokenResponse(json: data)
        var next = credentials
        next.accessToken = tokens.accessToken
        // A rotating refresh token is replaced together with the access token.
        if let refreshToken = tokens.refreshToken { next.refreshToken = refreshToken }
        if let idToken = tokens.idToken { next.idToken = idToken }
        if !tokens.scopes.isEmpty { next.scopes = tokens.scopes }
        next.expiresAt = now().addingTimeInterval(tokens.expiresIn)
        // A sign-out during the refresh wins over its result.
        guard store.credentials()?.subject == credentials.subject else { throw ChatGPTPlanError.signedOut }
        store.setCredentials(next)
        return next
    }

    // MARK: Models

    /// The signed-in account's models, fetched at most every ten minutes.
    func models(refresh: Bool = false) async throws -> [ChatGPTPlanModel] {
        guard let subject = store.credentials()?.subject else { throw ChatGPTPlanError.signedOut }
        if !refresh, let cachedModels, cachedModels.subject == subject,
           now().timeIntervalSince(cachedModels.fetchedAt) < Self.modelListLifetime {
            return cachedModels.models
        }
        let models = try await listModels()
        cachedModels = (subject, models, now())
        lokalbotLog("chatgpt plan models count=\(models.count)")
        return models
    }

    func listModels() async throws -> [ChatGPTPlanModel] {
        var token = try await accessToken()
        for attempt in 0..<2 {
            var request = URLRequest(url: URL(string: "\(ChatGPTPlanOAuth.resource)/models")!)
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
            let (data, response) = try await transport(request)
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            if status == 401, attempt == 0 {
                token = try await accessTokenAfterRejection(of: token)
                continue
            }
            guard (200...299).contains(status) else {
                throw ChatGPTPlanEngine.planError(status: status, data: data, response: response as? HTTPURLResponse)
            }
            return ChatGPTPlanModel.parseList(data)
        }
        throw ChatGPTPlanError.signInRequired
    }

    // MARK: Discovery

    private struct Discovery: Sendable {
        var jwksURL: URL?
        var revocationEndpoint: URL?
    }

    private var cachedDiscovery: Discovery?

    private func discovery() async throws -> Discovery {
        if let cachedDiscovery { return cachedDiscovery }
        let (data, response) = try await transport(URLRequest(url: ChatGPTPlanOAuth.discoveryURL))
        guard (response as? HTTPURLResponse).map({ (200...299).contains($0.statusCode) }) == true,
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw ChatGPTPlanError.signInFailed("Couldn't read OpenAI's sign-in configuration. Try again.")
        }
        // Only OpenAI's own hosts may serve its keys or revoke its tokens.
        func openAIURL(_ key: String) -> URL? {
            guard let url = (json[key] as? String).flatMap(URL.init(string:)),
                  url.scheme == "https", let host = url.host?.lowercased(),
                  host == "openai.com" || host.hasSuffix(".openai.com") else { return nil }
            return url
        }
        let discovery = Discovery(jwksURL: openAIURL("jwks_uri"), revocationEndpoint: openAIURL("revocation_endpoint"))
        cachedDiscovery = discovery
        return discovery
    }

    private func signingKeys() async throws -> [ChatGPTIDToken.JSONWebKey] {
        guard let jwksURL = try await discovery().jwksURL else {
            throw ChatGPTPlanError.signInFailed("OpenAI's sign-in configuration has no signing keys.")
        }
        let (data, response) = try await transport(URLRequest(url: jwksURL))
        guard (response as? HTTPURLResponse).map({ (200...299).contains($0.statusCode) }) == true else {
            throw ChatGPTPlanError.signInFailed("Couldn't read OpenAI's signing keys. Try again.")
        }
        return ChatGPTIDToken.JSONWebKey.keySet(data)
    }
}
