# Decisions

Append new entries; never silently change old ones. Format: `D<n> (date): decision. Why.`

**D1 (2026-10-18): Rust only, native egui/eframe on wgpu.** No Tauri, Electron, webview or JavaScript UI. One language, one binary, and the engine stays shared with the CLI and MCP.

**D2 (2026-10-18): Product is "Inbetween"; the CLI binary is `ib`.** Crates are `inbetween-*`, the desktop binary is `inbetween`, project files are `.inbetween` and dot sets `.inbetweendots`. `ib` is short to type for a tool used constantly from a shell.

**D3 (2026-10-18): Layering enforced by `cargo xtask layers`.** A crate may depend only on strictly lower layers, and UI crates are banned below L6. Registry lives in `xtask/src/layers.rs`. It keeps the engine reusable headless and wasm-clean.

**D4 (2026-10-18): Panicking forms banned in non-test code.** Crate-level `deny` attributes plus `clippy.toml` test exceptions, and `unsafe_code = "forbid"` workspace-wide. Hostile input must produce errors, not crashes.

**D5 (2026-10-18): Licences restricted to permissive ones.** `deny.toml` allows MIT, Apache-2.0, BSD, Zlib, ISC (plus Unicode and CC0 for data crates).

**D6 (2026-10-18): `xtask` uses `serde` and `serde_json` to read `cargo metadata`.** Both are MIT/Apache-2.0 and avoid hand-parsing manifests.

**D7 (2026-10-18): Delaunay via `spade` (MIT/Apache-2.0), geometry in `f64`.** Exact predicates make the triangulation robust to collinear and near-degenerate dots, so we do not write our own. Duplicate dots are rejected with `DuplicateDot { first, second }` rather than silently merged, so paired dot sets keep their indices. Collinear dots and fewer than three dots are accepted because the four frame corners always give the mesh area. A dot on a frame corner replaces that corner. Dots outside the frame are allowed; the mesh then covers their convex hull, which contains the frame. `MAX_DOTS` is 100,000.

**D8 (2026-10-18): Working pixels are premultiplied f32 holding sRGB-encoded values (no linearisation).** The 8-bit sRGB boundary then only divides or multiplies by 255 and premultiplies, so every non-transparent 8-bit pixel round-trips exactly (tested for all 255 alpha x 256 colour values), and t=0 / t=1 frames match the sources. Dissolves therefore blend in display space, as users of image tools expect; a linear-light option can be added later by converting at the same boundary. Fully transparent pixels normalise to `[0,0,0,0]` on output.

**D9 (2026-10-18): Image coordinates put pixel centres at +0.5; sampling is bilinear with edge clamp on premultiplied values.** Premultiplied interpolation is alpha-correct (no dark fringes next to transparency). NaN and infinite positions are clamped, never propagated.

**D10 (2026-10-18): PNG and JPEG via the `image` crate with only the `png` and `jpeg` features** (MIT/Apache-2.0). Decode checks the header against limits first (side at most 32,768, at most 100,000,000 pixels). 16-bit PNGs reduce to 8-bit. JPEG output has no alpha, so it composites over white. The `codecs` crate has no workspace dependencies; file helpers are `cfg(not(wasm32))`.

**D11 (2026-10-18): `cargo xtask wasm` checks every registered L0 to L5 crate for `wasm32-unknown-unknown`,** and runs in `xtask ci` and GitHub Actions.
