# Linux GTK Frontend (Scaffold)

This frontend is Linux-oriented and uses GTK4.

It is intentionally not part of the root Cargo workspace so GTK system dependencies are optional for backend users.

## Run

```bash
cd apps/linux-gtk
cargo run
```

On Linux, install GTK4 development packages first (for example `libgtk-4-dev`, `libgraphene-1.0-dev`, and `pkg-config` on Debian/Ubuntu).

The UI calls the same protocol-agnostic backend command model (`UiCommand`) used by other frontends.
