# ostrich-connect

`ostrich-connect` is a Cyberduck-style scaffolder with a protocol-agnostic Rust backend and multiple platform frontends.

## Disclaimer

- This project was done with AI.
- I built it to replace Cyberduck because, in my experience, it is unusable, slow, and full of bugs.
- This is not a demonstration of my programming skills.

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

## Shared Config

- A shared JSON config is managed by the Rust backend for all frontends.
- Default path:
  - macOS: `~/Library/Application Support/ostrich-connect/config.json`
  - Linux: `~/.config/ostrich-connect/config.json`
- Override path with `OSTRICH_CONNECT_CONFIG_PATH`.
- Default editor is `zed --wait`.

## Build

```bash
cargo run -p terminal-ratatui
```

Install CLI + TUI binaries:

```bash
cargo install --path apps/terminal-ratatui --bins
```

This installs:

- `terminal-ratatui` (direct TUI launcher)
- `oc` (convenience CLI)

`oc` usage:

```bash
oc                  # open the TUI
oc ls               # list saved connection names from shared backend config
oc connect my-sftp  # open TUI and auto-connect to that saved connection
```

Optional tab completion:

```bash
# zsh
oc completion zsh > ~/.zfunc/_oc

# bash
oc completion bash > ~/.oc-completion.bash
```

## Platform Frontends

- macOS Swift scaffold: `apps/macos-swift/README.md`
- Linux GTK scaffold: `apps/linux-gtk/README.md`

## Contract and Extensions

- Command contract: `docs/command-contract.md`
- Protocol extension guide: `docs/protocol-extension.md`
- Optional protocol scaffolder script: `scripts/new_protocol.sh`
