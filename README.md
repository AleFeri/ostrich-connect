# ostrich-connect

`ostrich-connect` is a Cyberduck-style scaffolder with a protocol-agnostic Rust backend and multiple platform frontends.

## Workspace Layout

- `crates/oc-core`: shared protocol abstractions, command contract, and secure connection types.
- `crates/oc-backend`: protocol registry + session manager + command dispatcher.
- `crates/protocol-ftp`: FTP protocol adapter.
- `crates/protocol-sftp`: SFTP protocol adapter.
- `crates/protocol-ftps`: FTP-SSL (FTPS) protocol adapter.
- `crates/oc-ffi`: C ABI bridge for native frontends (Swift, Linux, others).
- `apps/terminal-ratatui`: terminal frontend implemented with `ratatui`.
- `apps/macos-swift`: macOS Swift frontend scaffold through FFI.
- `apps/linux-gtk`: Linux GTK frontend scaffold.

## Core Architecture

- The backend uses a `ProtocolFactory` + `ProtocolSession` abstraction.
- GUIs send protocol-agnostic commands (`UiCommand`) to the backend.
- Protocol crates implement behavior behind a stable trait boundary.
- Adding a new protocol does not require changing command handling logic.

## Security Choices in the Scaffold

- Credentials use `secrecy::SecretString`.
- Profiles enforce protocol/security compatibility before connect.
- SFTP requires SSH transport and supports strict host-key checking flags.
- FTPS enforces TLS security modes (`tls_explicit` / `tls_implicit`).

## Build

```bash
cargo run -p terminal-ratatui
```

## Platform Frontends

- macOS Swift scaffold: `apps/macos-swift/README.md`
- Linux GTK scaffold: `apps/linux-gtk/README.md`

## Contract and Extensions

- Command contract: `docs/command-contract.md`
- Protocol extension guide: `docs/protocol-extension.md`
- Optional protocol scaffolder script: `scripts/new_protocol.sh`
