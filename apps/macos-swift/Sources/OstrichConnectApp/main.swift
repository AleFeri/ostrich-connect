import Foundation
import COstrichFFI

func sendCommand(_ handle: OpaquePointer?, command: [String: Any]) {
    guard let jsonData = try? JSONSerialization.data(withJSONObject: command),
          let json = String(data: jsonData, encoding: .utf8) else {
        print("Could not encode command")
        return
    }

    json.withCString { raw in
        guard let responseRaw = oc_backend_execute(handle, raw) else {
            print("No response")
            return
        }

        let response = String(cString: responseRaw)
        print("response: \(response)")
        oc_string_free(responseRaw)
    }
}

guard let backend = oc_backend_new() else {
    fatalError("Could not initialize Rust backend")
}
defer { oc_backend_free(backend) }

sendCommand(backend, command: ["command": "supported_protocols"])

let connectCommand: [String: Any] = [
    "command": "connect",
    "profile": [
        "protocol": "sftp",
        "host": "localhost",
        "port": 22,
        "username": "demo",
        "password": "change-me",
        "private_key_pem": NSNull(),
        "private_key_path": "~/.ssh/id_rsa",
        "security": "ssh_transport",
        "strict_host_key_checking": true,
        "passive_mode": false
    ]
]

sendCommand(backend, command: connectCommand)
