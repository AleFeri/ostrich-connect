# Adding a New Protocol

The codebase is designed to be open for extension and closed for modification.

## Steps

1. Create a new crate implementing `ProtocolFactory` and `ProtocolSession` from `oc-core`.
2. Register that factory in composition roots (`oc-ffi`, `terminal-ratatui`, and any native frontend that constructs a backend instance).
3. Keep frontend code unchanged: all UI logic already talks through `UiCommand`.

## Why This Is OCP-Friendly

- Existing command and backend dispatch code does not depend on protocol-specific APIs.
- New protocols are plugged in through trait objects and registry registration.
- Frontends never branch on protocol internals; they only submit commands.

## Optional Script

Use:

```bash
./scripts/new_protocol.sh webdav
```

This creates `crates/oc-protocol-webdav` with boilerplate trait implementations.
