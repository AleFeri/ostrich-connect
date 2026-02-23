# macOS Swift Frontend (Scaffold)

This app demonstrates how to call the Rust backend through the `oc-ffi` C ABI.

## Build Rust FFI Library

From repo root:

```bash
cargo build -p oc-ffi
```

This generates a dynamic library like `target/debug/liboc_ffi.dylib`.

## Run Swift App

```bash
cd apps/macos-swift
export DYLD_LIBRARY_PATH="../../target/debug:$DYLD_LIBRARY_PATH"
swift run
```

The Swift frontend sends JSON commands and receives JSON responses, without protocol-specific UI logic.
