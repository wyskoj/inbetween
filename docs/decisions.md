# Decisions

Append new entries; never silently change old ones. Format: `D<n> (date): decision. Why.`

**D1 (2026-10-18): Rust only, native egui/eframe on wgpu.** No Tauri, Electron, webview or JavaScript UI. One language, one binary, and the engine stays shared with the CLI and MCP.

**D2 (2026-10-18): Product is "Inbetween"; the CLI binary is `ib`.** Crates are `inbetween-*`, the desktop binary is `inbetween`, project files are `.inbetween` and dot sets `.inbetweendots`. `ib` is short to type for a tool used constantly from a shell.

**D3 (2026-10-18): Layering enforced by `cargo xtask layers`.** A crate may depend only on strictly lower layers, and UI crates are banned below L6. Registry lives in `xtask/src/layers.rs`. It keeps the engine reusable headless and wasm-clean.

**D4 (2026-10-18): Panicking forms banned in non-test code.** Crate-level `deny` attributes plus `clippy.toml` test exceptions, and `unsafe_code = "forbid"` workspace-wide. Hostile input must produce errors, not crashes.

**D5 (2026-10-18): Licences restricted to permissive ones.** `deny.toml` allows MIT, Apache-2.0, BSD, Zlib, ISC (plus Unicode and CC0 for data crates).

**D6 (2026-10-18): `xtask` uses `serde` and `serde_json` to read `cargo metadata`.** Both are MIT/Apache-2.0 and avoid hand-parsing manifests.
