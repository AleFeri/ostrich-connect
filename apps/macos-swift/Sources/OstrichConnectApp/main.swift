import COstrichFFI
import AppKit
import Foundation
import SwiftUI

enum ProtocolKind: String, CaseIterable, Identifiable {
    case ftp
    case sftp
    case ftps

    var id: String { rawValue }

    var label: String {
        rawValue.uppercased()
    }
}

enum ConnectionSecurity: String, CaseIterable, Identifiable {
    case plainText = "plain_text"
    case tlsExplicit = "tls_explicit"
    case tlsImplicit = "tls_implicit"
    case sshTransport = "ssh_transport"

    var id: String { rawValue }

    var label: String {
        switch self {
        case .plainText:
            return "Plain Text"
        case .tlsExplicit:
            return "TLS Explicit"
        case .tlsImplicit:
            return "TLS Implicit"
        case .sshTransport:
            return "SSH Transport"
        }
    }

    static func options(for protocolKind: ProtocolKind) -> [ConnectionSecurity] {
        switch protocolKind {
        case .ftp:
            return [.plainText]
        case .sftp:
            return [.sshTransport]
        case .ftps:
            return [.tlsExplicit, .tlsImplicit]
        }
    }
}

enum RemoteEntryKind: String {
    case file
    case directory
    case symlink
}

enum AppRoute {
    case connections
    case navigator
}

enum FormMode {
    case create
    case edit(UUID)
}

enum StatusLevel {
    case info
    case success
    case warning
    case error

    var color: Color {
        switch self {
        case .info:
            return .secondary
        case .success:
            return .green
        case .warning:
            return .orange
        case .error:
            return .red
        }
    }
}

struct ConnectionProfileData: Equatable {
    var protocolKind: ProtocolKind
    var host: String
    var port: Int
    var username: String
    var password: String?
    var privateKeyPem: String?
    var privateKeyPath: String?
    var security: ConnectionSecurity
    var strictHostKeyChecking: Bool
    var passiveMode: Bool
}

struct SavedConnection: Identifiable, Equatable {
    var id: UUID
    var name: String
    var profile: ConnectionProfileData
}

struct RemoteEntry: Identifiable, Equatable {
    var id: String { path }
    var name: String
    var path: String
    var kind: RemoteEntryKind
    var size: UInt64
    var modifiedUnix: Int64?
}

enum DraftValidationError: LocalizedError {
    case message(String)

    var errorDescription: String? {
        switch self {
        case let .message(message):
            return message
        }
    }
}

struct ConnectionDraft {
    var name: String
    var protocolKind: ProtocolKind
    var host: String
    var port: String
    var username: String
    var password: String
    var privateKeyPath: String
    var privateKeyPem: String
    var security: ConnectionSecurity
    var strictHostKeyChecking: Bool
    var passiveMode: Bool

    init(
        name: String,
        protocolKind: ProtocolKind,
        host: String,
        port: String,
        username: String,
        password: String,
        privateKeyPath: String,
        privateKeyPem: String,
        security: ConnectionSecurity,
        strictHostKeyChecking: Bool,
        passiveMode: Bool
    ) {
        self.name = name
        self.protocolKind = protocolKind
        self.host = host
        self.port = port
        self.username = username
        self.password = password
        self.privateKeyPath = privateKeyPath
        self.privateKeyPem = privateKeyPem
        self.security = security
        self.strictHostKeyChecking = strictHostKeyChecking
        self.passiveMode = passiveMode
    }

    static func defaults(protocolKind: ProtocolKind = .sftp) -> ConnectionDraft {
        let defaults = protocolDefaults(protocolKind: protocolKind)
        return ConnectionDraft(
            name: "",
            protocolKind: protocolKind,
            host: "",
            port: String(defaults.port),
            username: "",
            password: "",
            privateKeyPath: "",
            privateKeyPem: "",
            security: defaults.security,
            strictHostKeyChecking: defaults.strictHostKeyChecking,
            passiveMode: defaults.passiveMode
        )
    }

    init(connection: SavedConnection) {
        name = connection.name
        protocolKind = connection.profile.protocolKind
        host = connection.profile.host
        port = String(connection.profile.port)
        username = connection.profile.username
        password = connection.profile.password ?? ""
        privateKeyPath = connection.profile.privateKeyPath ?? ""
        privateKeyPem = connection.profile.privateKeyPem ?? ""
        security = connection.profile.security
        strictHostKeyChecking = connection.profile.strictHostKeyChecking
        passiveMode = connection.profile.passiveMode
    }

    mutating func applyDefaults(for protocolKind: ProtocolKind) {
        self.protocolKind = protocolKind
        let defaults = protocolDefaults(protocolKind: protocolKind)
        port = String(defaults.port)
        security = defaults.security
        strictHostKeyChecking = defaults.strictHostKeyChecking
        passiveMode = defaults.passiveMode
    }

    func toSavedConnection(existingID: UUID?) throws -> SavedConnection {
        guard !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw DraftValidationError.message("Connection name is required.")
        }
        guard !host.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw DraftValidationError.message("Host is required.")
        }
        guard !username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw DraftValidationError.message("Username is required.")
        }
        guard let parsedPort = Int(port), (1 ... 65535).contains(parsedPort) else {
            throw DraftValidationError.message("Port must be between 1 and 65535.")
        }

        let trimmedPassword = password.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedPath = privateKeyPath.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedPem = privateKeyPem.trimmingCharacters(in: .whitespacesAndNewlines)

        switch protocolKind {
        case .ftp, .ftps:
            if trimmedPassword.isEmpty {
                throw DraftValidationError.message("FTP/FTPS requires a password.")
            }
        case .sftp:
            if trimmedPassword.isEmpty && trimmedPath.isEmpty && trimmedPem.isEmpty {
                throw DraftValidationError.message(
                    "SFTP requires password, private key path, or private key PEM."
                )
            }
        }

        let profile = ConnectionProfileData(
            protocolKind: protocolKind,
            host: host.trimmingCharacters(in: .whitespacesAndNewlines),
            port: parsedPort,
            username: username.trimmingCharacters(in: .whitespacesAndNewlines),
            password: trimmedPassword.isEmpty ? nil : trimmedPassword,
            privateKeyPem: trimmedPem.isEmpty ? nil : trimmedPem,
            privateKeyPath: trimmedPath.isEmpty ? nil : trimmedPath,
            security: security,
            strictHostKeyChecking: strictHostKeyChecking,
            passiveMode: passiveMode
        )

        return SavedConnection(
            id: existingID ?? UUID(),
            name: name.trimmingCharacters(in: .whitespacesAndNewlines),
            profile: profile
        )
    }
}

private func protocolDefaults(protocolKind: ProtocolKind) -> (
    port: Int, security: ConnectionSecurity, strictHostKeyChecking: Bool, passiveMode: Bool
) {
    switch protocolKind {
    case .ftp:
        return (port: 21, security: .plainText, strictHostKeyChecking: false, passiveMode: true)
    case .sftp:
        return (port: 22, security: .sshTransport, strictHostKeyChecking: true, passiveMode: false)
    case .ftps:
        return (port: 21, security: .tlsExplicit, strictHostKeyChecking: false, passiveMode: true)
    }
}

enum BackendResponse {
    case connected(sessionID: UUID, protocolKind: ProtocolKind, peer: String)
    case disconnected
    case directory(path: String, entries: [RemoteEntry])
    case transferCompleted(destination: String)
    case pathDeleted
    case pathRenamed
    case config(defaultEditor: String, connections: [SavedConnection])
    case supportedProtocols([ProtocolKind])
    case ok(String)
    case error(code: String, message: String)
    case unknown(status: String)
}

struct BackendBridgeError: Error, LocalizedError {
    let message: String

    var errorDescription: String? {
        message
    }
}

final class BackendBridge {
    private var handle: OpaquePointer?

    init?() {
        guard let raw = oc_backend_new() else {
            return nil
        }
        handle = raw
    }

    deinit {
        if let handle {
            oc_backend_free(handle)
        }
    }

    func execute(command: [String: Any]) throws -> BackendResponse {
        guard let handle else {
            throw BackendBridgeError(message: "Backend handle not initialized.")
        }
        guard JSONSerialization.isValidJSONObject(command) else {
            throw BackendBridgeError(message: "Command JSON is invalid.")
        }

        let data = try JSONSerialization.data(withJSONObject: command, options: [])
        guard let json = String(data: data, encoding: .utf8) else {
            throw BackendBridgeError(message: "Could not encode command JSON.")
        }

        let rawResponse: UnsafeMutablePointer<CChar>? = json.withCString { rawCommand in
            oc_backend_execute(handle, rawCommand)
        }

        guard let rawResponse else {
            throw BackendBridgeError(message: "No response from Rust backend.")
        }
        defer { oc_string_free(rawResponse) }

        let responseJSON = String(cString: rawResponse)
        return try BackendResponseParser.parse(responseJSON: responseJSON)
    }
}

enum BackendResponseParser {
    static func parse(responseJSON: String) throws -> BackendResponse {
        let data = Data(responseJSON.utf8)
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw BackendBridgeError(message: "Invalid response JSON: \(responseJSON)")
        }
        guard let status = object["status"] as? String else {
            throw BackendBridgeError(message: "Response is missing status field.")
        }

        switch status {
        case "connected":
            guard
                let sessionIDRaw = object["session_id"] as? String,
                let sessionID = UUID(uuidString: sessionIDRaw),
                let protocolRaw = object["protocol"] as? String,
                let protocolKind = ProtocolKind(rawValue: protocolRaw),
                let peer = object["peer"] as? String
            else {
                throw BackendBridgeError(message: "Malformed connected response.")
            }
            return .connected(sessionID: sessionID, protocolKind: protocolKind, peer: peer)

        case "disconnected":
            return .disconnected

        case "directory":
            let path = object["path"] as? String ?? "/"
            let entriesRaw = object["entries"] as? [[String: Any]] ?? []
            let entries = entriesRaw.compactMap { item -> RemoteEntry? in
                guard
                    let name = item["name"] as? String,
                    let entryPath = item["path"] as? String,
                    let kindRaw = item["kind"] as? String,
                    let kind = RemoteEntryKind(rawValue: kindRaw)
                else {
                    return nil
                }
                let size = (item["size"] as? NSNumber)?.uint64Value ?? 0
                let modifiedUnix = (item["modified_unix"] as? NSNumber)?.int64Value
                return RemoteEntry(
                    name: name,
                    path: entryPath,
                    kind: kind,
                    size: size,
                    modifiedUnix: modifiedUnix
                )
            }
            return .directory(path: path, entries: entries)

        case "transfer_completed":
            let destination = object["destination"] as? String ?? ""
            return .transferCompleted(destination: destination)

        case "path_deleted":
            return .pathDeleted

        case "path_renamed":
            return .pathRenamed

        case "config":
            let configRaw = object["config"] as? [String: Any] ?? [:]
            let defaultEditor = (configRaw["default_editor"] as? String)?
                .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            let connectionsRaw = configRaw["connections"] as? [[String: Any]] ?? []
            let connections = connectionsRaw.compactMap(parseSavedConnection)
            return .config(defaultEditor: defaultEditor, connections: connections)

        case "supported_protocols":
            let protocols = (object["protocols"] as? [String] ?? [])
                .compactMap { ProtocolKind(rawValue: $0) }
            return .supportedProtocols(protocols)

        case "ok":
            return .ok(object["message"] as? String ?? "ok")

        case "error":
            return .error(
                code: object["code"] as? String ?? "unknown",
                message: object["message"] as? String ?? "Unknown error"
            )

        default:
            return .unknown(status: status)
        }
    }

    private static func parseSavedConnection(item: [String: Any]) -> SavedConnection? {
        guard
            let name = item["name"] as? String,
            let profileRaw = item["profile"] as? [String: Any],
            let profile = parseProfile(item: profileRaw)
        else {
            return nil
        }

        return SavedConnection(id: UUID(), name: name, profile: profile)
    }

    private static func parseProfile(item: [String: Any]) -> ConnectionProfileData? {
        guard
            let protocolRaw = item["protocol"] as? String,
            let protocolKind = ProtocolKind(rawValue: protocolRaw),
            let host = item["host"] as? String,
            let port = (item["port"] as? NSNumber)?.intValue,
            let username = item["username"] as? String,
            let securityRaw = item["security"] as? String,
            let security = ConnectionSecurity(rawValue: securityRaw)
        else {
            return nil
        }

        return ConnectionProfileData(
            protocolKind: protocolKind,
            host: host,
            port: port,
            username: username,
            password: optionalString(item["password"]),
            privateKeyPem: optionalString(item["private_key_pem"]),
            privateKeyPath: optionalString(item["private_key_path"]),
            security: security,
            strictHostKeyChecking: item["strict_host_key_checking"] as? Bool ?? false,
            passiveMode: item["passive_mode"] as? Bool ?? false
        )
    }

    private static func optionalString(_ value: Any?) -> String? {
        guard let string = value as? String else {
            return nil
        }
        let trimmed = string.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
}

@MainActor
final class AppViewModel: ObservableObject {
    @Published var route: AppRoute = .connections
    @Published var connections: [SavedConnection] = []
    @Published var selectedConnectionID: UUID?
    @Published var sessionID: UUID?
    @Published var currentPath: String = "/"
    @Published var entries: [RemoteEntry] = []
    @Published var selectedEntryID: String?
    @Published var statusMessage: String = "Create a connection to start."
    @Published var statusLevel: StatusLevel = .info

    @Published var formMode: FormMode = .create
    @Published var draft: ConnectionDraft = .defaults()
    @Published var formError: String?
    @Published var isFormPresented: Bool = false

    @Published var navigatorSearch: String = ""

    @Published var pendingDownloadEntry: RemoteEntry?
    @Published var showDownloadConfirmation: Bool = false

    let backendReady: Bool
    private let backend: BackendBridge?
    private var defaultEditor: String = "zed --wait"

    init() {
        backend = BackendBridge()
        backendReady = backend != nil
        if !backendReady {
            setStatus("Could not initialize Rust backend bridge.", level: .error)
            return
        }
        loadConfig()
    }

    var selectedConnection: SavedConnection? {
        guard let selectedConnectionID else { return nil }
        return connections.first { $0.id == selectedConnectionID }
    }

    var selectedEntry: RemoteEntry? {
        guard let selectedEntryID else { return nil }
        return entries.first { $0.id == selectedEntryID }
    }

    func openCreateForm() {
        formMode = .create
        draft = .defaults()
        formError = nil
        isFormPresented = true
    }

    func openEditForm() {
        guard let selectedConnection else {
            setStatus("Select a connection to edit.", level: .warning)
            return
        }
        formMode = .edit(selectedConnection.id)
        draft = ConnectionDraft(connection: selectedConnection)
        formError = nil
        isFormPresented = true
    }

    func saveDraft() {
        do {
            let connection: SavedConnection
            let baseMessage: String
            switch formMode {
            case .create:
                connection = try draft.toSavedConnection(existingID: nil)
                connections.append(connection)
                selectedConnectionID = connection.id
                baseMessage = "Connection created."
            case let .edit(existingID):
                connection = try draft.toSavedConnection(existingID: existingID)
                if let index = connections.firstIndex(where: { $0.id == existingID }) {
                    connections[index] = connection
                    selectedConnectionID = existingID
                    baseMessage = "Connection updated."
                } else {
                    connections.append(connection)
                    selectedConnectionID = connection.id
                    baseMessage = "Connection created."
                }
            }
            formError = nil
            isFormPresented = false
            if persistConfig() {
                setStatus(baseMessage, level: .success)
            }
        } catch {
            formError = error.localizedDescription
        }
    }

    func deleteSelectedConnection() {
        guard let selectedConnectionID else {
            setStatus("Select a connection to delete.", level: .warning)
            return
        }
        connections.removeAll { $0.id == selectedConnectionID }
        self.selectedConnectionID = connections.first?.id
        if persistConfig() {
            setStatus("Connection deleted.", level: .warning)
        }
    }

    func connectSelectedConnection() {
        guard let selectedConnection else {
            setStatus("Select a connection first.", level: .warning)
            return
        }
        guard let response = execute(
            command: [
                "command": "connect",
                "profile": profileDictionary(from: selectedConnection.profile),
            ]
        ) else { return }

        switch response {
        case let .connected(sessionID, _, peer):
            self.sessionID = sessionID
            currentPath = "/"
            entries = []
            selectedEntryID = nil
            route = .navigator
            navigatorSearch = ""
            setStatus("Connected to \(peer).", level: .success)
            refreshDirectory()
        case let .error(_, message):
            setStatus("Connection failed: \(message)", level: .error)
        default:
            setStatus("Unexpected connect response.", level: .warning)
        }
    }

    func disconnectAndReturn() {
        if let sessionID {
            _ = execute(command: ["command": "disconnect", "session_id": sessionID.uuidString])
        }
        route = .connections
        self.sessionID = nil
        currentPath = "/"
        entries = []
        selectedEntryID = nil
        navigatorSearch = ""
        setStatus("Disconnected.", level: .info)
    }

    func refreshDirectory() {
        guard let sessionID else {
            setStatus("No active session.", level: .warning)
            return
        }
        guard let response = execute(
            command: [
                "command": "list_directory",
                "session_id": sessionID.uuidString,
                "path": currentPath,
            ]
        ) else { return }

        switch response {
        case let .directory(path, entries):
            currentPath = path
            self.entries = sortEntries(entries)
            selectedEntryID = self.entries.first?.id
            setStatus("Loaded \(self.entries.count) entries.", level: .success)
        case let .error(_, message):
            setStatus("List failed: \(message)", level: .error)
        default:
            setStatus("Unexpected directory response.", level: .warning)
        }
    }

    func goUpDirectory() {
        currentPath = parentPath(for: currentPath)
        refreshDirectory()
    }

    func openSelectedEntry() {
        guard let selectedEntry else {
            setStatus("Select a folder to open.", level: .warning)
            return
        }
        openEntry(selectedEntry)
    }

    func openEntry(_ entry: RemoteEntry) {
        guard entry.kind == .directory else {
            setStatus("Selected item is not a directory.", level: .warning)
            return
        }
        currentPath = normalizeRemotePath(entry.path)
        refreshDirectory()
    }

    func openFirstUnambiguousMatch() {
        let query = navigatorSearch.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !query.isEmpty else {
            setStatus("Type a folder prefix first.", level: .warning)
            return
        }
        let matches = entries.filter {
            $0.kind == .directory && $0.name.lowercased().hasPrefix(query)
        }
        if matches.count == 1 {
            openEntry(matches[0])
            navigatorSearch = ""
        } else if matches.isEmpty {
            setStatus("No matching folder for '\(query)'.", level: .warning)
        } else {
            setStatus("\(matches.count) matches for '\(query)'. Keep typing.", level: .info)
        }
    }

    func requestDownloadSelected() {
        guard let selectedEntry else {
            setStatus("Select a file to download.", level: .warning)
            return
        }
        guard selectedEntry.kind != .directory else {
            setStatus("Directory download is not implemented.", level: .warning)
            return
        }
        pendingDownloadEntry = selectedEntry
        showDownloadConfirmation = true
    }

    func cancelPendingDownload() {
        pendingDownloadEntry = nil
        showDownloadConfirmation = false
    }

    func confirmDownload() {
        guard let pendingDownloadEntry else {
            return
        }
        defer {
            self.pendingDownloadEntry = nil
            showDownloadConfirmation = false
        }
        guard let sessionID else {
            setStatus("No active session.", level: .warning)
            return
        }

        let destination = downloadDestination(for: pendingDownloadEntry)
        let destinationURL = URL(fileURLWithPath: destination)
        let parentURL = destinationURL.deletingLastPathComponent()
        do {
            try FileManager.default.createDirectory(
                at: parentURL,
                withIntermediateDirectories: true
            )
        } catch {
            setStatus("Could not prepare Downloads folder: \(error.localizedDescription)", level: .error)
            return
        }

        guard let response = execute(
            command: [
                "command": "download_file",
                "session_id": sessionID.uuidString,
                "remote_path": pendingDownloadEntry.path,
                "local_path": destination,
            ]
        ) else { return }

        switch response {
        case let .transferCompleted(destination):
            setStatus("Downloaded to \(destination)", level: .success)
        case let .error(_, message):
            setStatus("Download failed: \(message)", level: .error)
        default:
            setStatus("Unexpected download response.", level: .warning)
        }
    }

    func downloadDestination(for entry: RemoteEntry) -> String {
        let downloads = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask)
            .first ?? URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent("Downloads")
        return downloads.appendingPathComponent(entry.name).path
    }

    private func loadConfig() {
        guard let response = execute(command: ["command": "load_config"]) else {
            return
        }

        switch response {
        case let .config(defaultEditor, connections):
            self.defaultEditor = defaultEditor.isEmpty ? "zed --wait" : defaultEditor
            self.connections = connections
            selectedConnectionID = connections.first?.id
            setStatus("Loaded \(connections.count) saved connections.", level: .info)
        case let .error(_, message):
            setStatus("Could not load config: \(message)", level: .error)
        default:
            setStatus("Unexpected config response.", level: .warning)
        }
    }

    @discardableResult
    private func persistConfig() -> Bool {
        guard let response = execute(
            command: [
                "command": "save_config",
                "config": configDictionary(),
            ]
        ) else {
            return false
        }

        switch response {
        case .ok:
            return true
        case let .error(_, message):
            setStatus("Config save failed: \(message)", level: .error)
            return false
        default:
            setStatus("Unexpected save_config response.", level: .warning)
            return false
        }
    }

    private func execute(command: [String: Any]) -> BackendResponse? {
        guard let backend else {
            setStatus("Backend bridge not available.", level: .error)
            return nil
        }
        do {
            return try backend.execute(command: command)
        } catch {
            setStatus("Backend error: \(error.localizedDescription)", level: .error)
            return nil
        }
    }

    private func configDictionary() -> [String: Any] {
        let savedConnections = connections.map { connection in
            [
                "name": connection.name,
                "profile": profileDictionary(from: connection.profile),
            ]
        }
        return [
            "default_editor": defaultEditor,
            "connections": savedConnections,
        ]
    }

    private func profileDictionary(from profile: ConnectionProfileData) -> [String: Any] {
        [
            "protocol": profile.protocolKind.rawValue,
            "host": profile.host,
            "port": profile.port,
            "username": profile.username,
            "password": valueOrNull(profile.password),
            "private_key_pem": valueOrNull(profile.privateKeyPem),
            "private_key_path": valueOrNull(profile.privateKeyPath),
            "security": profile.security.rawValue,
            "strict_host_key_checking": profile.strictHostKeyChecking,
            "passive_mode": profile.passiveMode,
        ]
    }

    private func valueOrNull(_ value: String?) -> Any {
        guard let value, !value.isEmpty else {
            return NSNull()
        }
        return value
    }

    private func setStatus(_ message: String, level: StatusLevel) {
        statusMessage = message
        statusLevel = level
    }

    private func sortEntries(_ entries: [RemoteEntry]) -> [RemoteEntry] {
        entries.sorted { left, right in
            let leftRank = rank(kind: left.kind)
            let rightRank = rank(kind: right.kind)
            if leftRank == rightRank {
                return left.name.localizedCaseInsensitiveCompare(right.name) == .orderedAscending
            }
            return leftRank < rightRank
        }
    }

    private func rank(kind: RemoteEntryKind) -> Int {
        switch kind {
        case .directory:
            return 0
        case .symlink:
            return 1
        case .file:
            return 2
        }
    }

    private func normalizeRemotePath(_ path: String) -> String {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty || trimmed == "." {
            return "/"
        }
        if trimmed.hasPrefix("/") {
            return trimmed
        }
        return "/" + trimmed
    }

    private func parentPath(for path: String) -> String {
        let normalized = normalizeRemotePath(path)
        if normalized == "/" {
            return "/"
        }
        let pathURL = URL(fileURLWithPath: normalized)
        let parent = pathURL.deletingLastPathComponent().path
        return parent.isEmpty ? "/" : parent
    }
}

struct RootView: View {
    @StateObject private var viewModel = AppViewModel()

    var body: some View {
        ZStack {
            WindowGlassBackground()
                .ignoresSafeArea()

            LinearGradient(
                colors: [Color.white.opacity(0.14), .clear],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            .ignoresSafeArea()

            Group {
                if viewModel.backendReady {
                    content
                } else {
                    GlassPanel {
                        VStack(spacing: 10) {
                            Image(systemName: "xmark.octagon.fill")
                                .font(.system(size: 42))
                                .foregroundStyle(.red)
                            Text("Backend Unavailable")
                                .font(.title2.weight(.semibold))
                            Text(viewModel.statusMessage)
                                .foregroundStyle(.secondary)
                                .multilineTextAlignment(.center)
                        }
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                }
            }
            .padding(16)
        }
        .frame(minWidth: 1120, minHeight: 760)
        .sheet(isPresented: $viewModel.isFormPresented) {
            ConnectionFormSheet(viewModel: viewModel)
        }
        .alert(
            "Download File?",
            isPresented: $viewModel.showDownloadConfirmation,
            presenting: viewModel.pendingDownloadEntry
        ) { entry in
            Button("Cancel", role: .cancel) {
                viewModel.cancelPendingDownload()
            }
            Button("Download") {
                viewModel.confirmDownload()
            }
        } message: { entry in
            Text("Download '\(entry.name)' to \(viewModel.downloadDestination(for: entry))?")
        }
    }

    @ViewBuilder
    private var content: some View {
        switch viewModel.route {
        case .connections:
            ConnectionsPage(viewModel: viewModel)
        case .navigator:
            NavigatorPage(viewModel: viewModel)
        }
    }
}

struct ConnectionsPage: View {
    @ObservedObject var viewModel: AppViewModel

    var body: some View {
        VStack(spacing: 12) {
            HeaderStrip(
                title: "Connection Manager",
                status: viewModel.statusMessage,
                level: viewModel.statusLevel
            )

            HSplitView {
                sidebar
                    .frame(minWidth: 360, maxWidth: 460)
                details
            }

            actionBar
        }
    }

    private var sidebar: some View {
        GlassPanel {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Text("Saved Connections")
                        .font(.headline)
                    Spacer()
                    Text("\(viewModel.connections.count)")
                        .font(.caption.weight(.semibold))
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .background(.white.opacity(0.15), in: Capsule())
                }

                List(selection: $viewModel.selectedConnectionID) {
                    if viewModel.connections.isEmpty {
                        EmptyStateView(
                            title: "No Connections",
                            systemImage: "externaldrive.badge.plus",
                            message: "Create one with the New button."
                        )
                        .frame(maxWidth: .infinity, minHeight: 240)
                    }

                    ForEach(viewModel.connections) { connection in
                        HStack(spacing: 10) {
                            Image(systemName: "network")
                                .frame(width: 18)
                                .foregroundStyle(.secondary)
                            VStack(alignment: .leading, spacing: 4) {
                                Text(connection.name)
                                    .font(.headline)
                                    .lineLimit(1)
                                Text("\(connection.profile.host):\(connection.profile.port)")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                            Spacer(minLength: 8)
                            Text(connection.profile.protocolKind.label)
                                .font(.caption2.weight(.semibold))
                                .padding(.horizontal, 7)
                                .padding(.vertical, 4)
                                .background(protocolBadgeBackground(connection.profile.protocolKind))
                                .clipShape(Capsule())
                        }
                        .tag(connection.id)
                    }
                }
                .listStyle(.sidebar)
                .scrollContentBackground(.hidden)
                .background(Color.clear)
            }
        }
    }

    private var details: some View {
        GlassPanel {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Text("Details")
                        .font(.headline)
                    Spacer()
                    Image(systemName: "slider.horizontal.3")
                        .foregroundStyle(.secondary)
                }
                Divider()

                if let connection = viewModel.selectedConnection {
                    DetailGrid(connection: connection)
                } else {
                    EmptyStateView(
                        title: "Select a Connection",
                        systemImage: "arrow.left.circle",
                        message: "Choose a saved connection from the sidebar."
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }

                Spacer(minLength: 0)
            }
        }
    }

    private var actionBar: some View {
        GlassPanel {
            HStack {
                ControlGroup {
                    Button("New") {
                        viewModel.openCreateForm()
                    }
                    .keyboardShortcut("n", modifiers: [.command])

                    Button("Edit") {
                        viewModel.openEditForm()
                    }
                    .disabled(viewModel.selectedConnection == nil)

                    Button("Delete") {
                        viewModel.deleteSelectedConnection()
                    }
                    .disabled(viewModel.selectedConnection == nil)
                }

                Spacer()

                Button("Connect") {
                    viewModel.connectSelectedConnection()
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .disabled(viewModel.selectedConnection == nil)
                .keyboardShortcut(.return, modifiers: [])
            }
        }
    }

    private func protocolBadgeBackground(_ kind: ProtocolKind) -> Color {
        switch kind {
        case .ftp:
            return .orange.opacity(0.28)
        case .sftp:
            return .blue.opacity(0.28)
        case .ftps:
            return .mint.opacity(0.28)
        }
    }
}

struct NavigatorPage: View {
    @ObservedObject var viewModel: AppViewModel

    var body: some View {
        VStack(spacing: 12) {
            HeaderStrip(
                title: "File Navigator",
                status: viewModel.statusMessage,
                level: viewModel.statusLevel
            )

            pathAndSearchBar

            HSplitView {
                fileList
                    .frame(minWidth: 740)
                inspector
                    .frame(minWidth: 280, maxWidth: 360)
            }

            actionBar
        }
    }

    private var pathAndSearchBar: some View {
        GlassPanel {
            HStack(spacing: 12) {
                Label(viewModel.currentPath, systemImage: "folder")
                    .font(.system(.subheadline, design: .monospaced))
                    .lineLimit(1)

                Spacer(minLength: 16)

                TextField("Type folder prefix and press Return", text: $viewModel.navigatorSearch)
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 320)
                    .onSubmit {
                        viewModel.openFirstUnambiguousMatch()
                    }

                Button("Open Unique Match") {
                    viewModel.openFirstUnambiguousMatch()
                }
                .disabled(viewModel.navigatorSearch.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }

    private var fileList: some View {
        GlassPanel {
            List(selection: $viewModel.selectedEntryID) {
                if viewModel.entries.isEmpty {
                    EmptyStateView(
                        title: "No Files",
                        systemImage: "folder.badge.questionmark",
                        message: "This folder is empty or not loaded."
                    )
                    .frame(maxWidth: .infinity, minHeight: 240)
                }

                ForEach(viewModel.entries) { entry in
                    HStack(spacing: 10) {
                        Image(systemName: icon(for: entry.kind))
                            .foregroundStyle(iconColor(for: entry.kind))
                            .frame(width: 20)

                        Text(entry.kind == .directory ? "\(entry.name)/" : entry.name)
                            .lineLimit(1)

                        Spacer()

                        if entry.kind != .directory {
                            Text(
                                ByteCountFormatter.string(
                                    fromByteCount: Int64(entry.size),
                                    countStyle: .file
                                )
                            )
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        }
                    }
                    .tag(entry.id)
                    .contentShape(Rectangle())
                    .onTapGesture(count: 2) {
                        if entry.kind == .directory {
                            viewModel.openEntry(entry)
                        } else {
                            viewModel.requestDownloadSelected()
                        }
                    }
                }
            }
            .listStyle(.inset)
            .scrollContentBackground(.hidden)
            .background(Color.clear)
        }
    }

    private var inspector: some View {
        GlassPanel {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Text("Inspector")
                        .font(.headline)
                    Spacer()
                    Image(systemName: "info.circle")
                        .foregroundStyle(.secondary)
                }
                Divider()

                if let entry = viewModel.selectedEntry {
                    inspectorLine("Name", entry.name)
                    inspectorLine("Path", entry.path)
                    inspectorLine("Type", entry.kind.rawValue)
                    if entry.kind != .directory {
                        inspectorLine(
                            "Size",
                            ByteCountFormatter.string(
                                fromByteCount: Int64(entry.size),
                                countStyle: .file
                            )
                        )
                    }
                    if let modified = entry.modifiedUnix {
                        inspectorLine("Modified", modifiedDateLabel(modified))
                    }

                    Divider()
                    if entry.kind == .directory {
                        Button("Open Folder") {
                            viewModel.openSelectedEntry()
                        }
                        .buttonStyle(.borderedProminent)
                    } else {
                        Button("Download File") {
                            viewModel.requestDownloadSelected()
                        }
                        .buttonStyle(.borderedProminent)
                    }
                } else {
                    EmptyStateView(
                        title: "No Selection",
                        systemImage: "cursorarrow.rays",
                        message: "Select a file or folder from the list."
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }

                Spacer(minLength: 0)
            }
        }
    }

    private var actionBar: some View {
        GlassPanel {
            HStack {
                ControlGroup {
                    Button("Connections") {
                        viewModel.disconnectAndReturn()
                    }
                    .keyboardShortcut(.escape, modifiers: [])

                    Button("Up") {
                        viewModel.goUpDirectory()
                    }

                    Button("Refresh") {
                        viewModel.refreshDirectory()
                    }
                    .keyboardShortcut("r", modifiers: [.command])
                }

                Spacer()

                Button("Open Folder") {
                    viewModel.openSelectedEntry()
                }
                .disabled(viewModel.selectedEntry?.kind != .directory)

                Button("Download") {
                    viewModel.requestDownloadSelected()
                }
                .disabled(viewModel.selectedEntry == nil || viewModel.selectedEntry?.kind == .directory)
            }
        }
    }

    private func icon(for kind: RemoteEntryKind) -> String {
        switch kind {
        case .directory:
            return "folder.fill"
        case .symlink:
            return "link"
        case .file:
            return "doc.fill"
        }
    }

    private func iconColor(for kind: RemoteEntryKind) -> Color {
        switch kind {
        case .directory:
            return .blue
        case .symlink:
            return .mint
        case .file:
            return .secondary
        }
    }

    private func inspectorLine(_ title: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            Text(value)
                .textSelection(.enabled)
                .lineLimit(2)
        }
    }

    private func modifiedDateLabel(_ unix: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(unix))
        return modifiedDateFormatter.string(from: date)
    }
}

struct ConnectionFormSheet: View {
    @ObservedObject var viewModel: AppViewModel
    @FocusState private var focusedField: Field?

    private enum Field {
        case name
        case host
        case port
        case username
        case password
        case privateKeyPath
    }

    private var title: String {
        switch viewModel.formMode {
        case .create:
            return "Create Connection"
        case .edit:
            return "Edit Connection"
        }
    }

    var body: some View {
        VStack(spacing: 12) {
            HeaderStrip(
                title: title,
                status: "Configure transport and authentication settings.",
                level: .info
            )

            GlassPanel {
                Form {
                    Section("Connection") {
                        TextField("Name", text: $viewModel.draft.name)
                            .focused($focusedField, equals: .name)

                        Picker("Protocol", selection: protocolBinding) {
                            ForEach(ProtocolKind.allCases) { protocolKind in
                                Text(protocolKind.label).tag(protocolKind)
                            }
                        }

                        TextField("Host", text: $viewModel.draft.host)
                            .focused($focusedField, equals: .host)
                        TextField("Port", text: $viewModel.draft.port)
                            .focused($focusedField, equals: .port)
                        TextField("Username", text: $viewModel.draft.username)
                            .focused($focusedField, equals: .username)
                    }

                    Section("Authentication") {
                        SecureField("Password (optional for key auth)", text: $viewModel.draft.password)
                            .focused($focusedField, equals: .password)
                        TextField("Private Key Path (e.g. ~/.ssh/id_rsa)", text: $viewModel.draft.privateKeyPath)
                            .focused($focusedField, equals: .privateKeyPath)
                        TextEditor(text: $viewModel.draft.privateKeyPem)
                            .font(.system(.body, design: .monospaced))
                            .frame(minHeight: 110)
                    }

                    Section("Transport") {
                        Picker("Security", selection: $viewModel.draft.security) {
                            ForEach(ConnectionSecurity.options(for: viewModel.draft.protocolKind)) { security in
                                Text(security.label).tag(security)
                            }
                        }
                        Toggle("Strict Host Key Checking", isOn: $viewModel.draft.strictHostKeyChecking)
                        Toggle("Passive Mode", isOn: $viewModel.draft.passiveMode)
                    }
                }
                .scrollContentBackground(.hidden)
                .background(Color.clear)
            }

            if let formError = viewModel.formError {
                HStack {
                    Image(systemName: "exclamationmark.triangle.fill")
                    Text(formError)
                    Spacer()
                }
                .font(.subheadline)
                .foregroundStyle(.red)
            }

            GlassPanel {
                HStack {
                    Spacer()
                    Button("Cancel") {
                        viewModel.isFormPresented = false
                        viewModel.formError = nil
                    }
                    Button("Save") {
                        viewModel.saveDraft()
                    }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.return, modifiers: [])
                }
            }
        }
        .padding(16)
        .frame(width: 760, height: 690)
        .onAppear {
            focusedField = .name
        }
    }

    private var protocolBinding: Binding<ProtocolKind> {
        Binding(
            get: { viewModel.draft.protocolKind },
            set: { newValue in
                viewModel.draft.applyDefaults(for: newValue)
            }
        )
    }
}

struct HeaderStrip: View {
    let title: String
    let status: String
    let level: StatusLevel

    var body: some View {
        GlassPanel {
            HStack(spacing: 10) {
                Text(title)
                    .font(.title3.weight(.semibold))
                Spacer(minLength: 10)
                Image(systemName: statusSymbol(level))
                    .foregroundStyle(level.color)
                Text(status)
                    .font(.subheadline)
                    .lineLimit(2)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func statusSymbol(_ level: StatusLevel) -> String {
        switch level {
        case .info:
            return "info.circle.fill"
        case .success:
            return "checkmark.circle.fill"
        case .warning:
            return "exclamationmark.triangle.fill"
        case .error:
            return "xmark.octagon.fill"
        }
    }
}

struct DetailGrid: View {
    let connection: SavedConnection

    var body: some View {
        Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 8) {
            row("Name", connection.name)
            row("Protocol", connection.profile.protocolKind.label)
            row("Host", connection.profile.host)
            row("Port", String(connection.profile.port))
            row("Username", connection.profile.username)
            row("Password", connection.profile.password == nil ? "-" : "••••••••")
            row("Private Key Path", connection.profile.privateKeyPath ?? "-")
            row("Private Key PEM", connection.profile.privateKeyPem == nil ? "-" : "<provided>")
            row("Security", connection.profile.security.label)
            row(
                "Strict Host Key Checking",
                connection.profile.strictHostKeyChecking ? "true" : "false"
            )
            row("Passive Mode", connection.profile.passiveMode ? "true" : "false")
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func row(_ title: String, _ value: String) -> some View {
        GridRow {
            Text(title)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.secondary)
            Text(value)
                .textSelection(.enabled)
                .lineLimit(2)
            Spacer(minLength: 0)
        }
    }
}

struct EmptyStateView: View {
    let title: String
    let systemImage: String
    let message: String

    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: systemImage)
                .font(.system(size: 28))
                .foregroundStyle(.secondary)
            Text(title)
                .font(.headline)
            Text(message)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .padding(.horizontal, 12)
    }
}

struct GlassPanel<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        content
            .padding(14)
            .background(
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .fill(.regularMaterial)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .stroke(Color.white.opacity(0.16), lineWidth: 1)
            )
            .shadow(color: .black.opacity(0.18), radius: 10, y: 4)
    }
}

struct WindowGlassBackground: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.state = .active
        view.blendingMode = .behindWindow
        view.material = .underWindowBackground
        return view
    }

    func updateNSView(_ nsView: NSVisualEffectView, context: Context) {}
}

private let modifiedDateFormatter: DateFormatter = {
    let formatter = DateFormatter()
    formatter.dateStyle = .medium
    formatter.timeStyle = .short
    return formatter
}()

final class OstrichAppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)

        DispatchQueue.main.async {
            NSApp.windows.first?.makeKeyAndOrderFront(nil)
        }
    }
}

@main
struct OstrichConnectApp: App {
    @NSApplicationDelegateAdaptor(OstrichAppDelegate.self) private var appDelegate

    var body: some Scene {
        WindowGroup("ostrich-connect") {
            RootView()
        }
        .windowStyle(.hiddenTitleBar)
        .defaultSize(width: 1180, height: 790)
    }
}
