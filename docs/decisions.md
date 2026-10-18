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

**D12 (2026-10-18): Dot coordinates live in output-frame pixels** (y down, origin top-left). Sources of other sizes are fitted to the output first, so dots are placed on the fitted images. Fit modes: `stretch` (default; each axis scaled independently), `contain` (uniform, transparent bars) and `cover` (uniform, cropped). Only `contain` reads transparent outside the source; the others repeat edge pixels. Same-size sources fit as an exact identity.

**D13 (2026-10-18): One shared mesh, built from Image 1's dots; the last frame uses a mesh from Image 2's dots.** The default policy (`frame0`) triangulates once and reuses the connectivity, interpolating vertex positions. When dots move far the shared mesh can fold in the middle of the sequence, but frame 0 is Image 1 and the last frame is Image 2 exactly because the end frames use their own meshes. `per_frame` re-triangulates each interpolated frame, falling back to the shared mesh on frames where two dots coincide. Duplicate dots on either image are an error that names the image.

**D14 (2026-10-18): Frame time is `i / (n - 1)`; a one-frame sequence is Image 1.** `Point::lerp` is now written `a*(1-t) + b*t` so it returns `a` at t=0 and `b` at t=1 bit-exactly.

**D15 (2026-10-18): Deterministic parallel rendering.** Each mesh triangle renders into its own buffer (rayon `par_iter().collect()` keeps order); buffers merge sequentially in triangle order, so overlapping or shared-edge pixels resolve the same way on any thread count. Pixel centres count as inside a triangle with `1e-9` slack so shared edges leave no gaps; any pixel still unclaimed (folded meshes) is sampled unwarped. If the inverse map for a layer is singular, that layer is sampled unwarped in that triangle.

**D16 (2026-10-18): File formats are JSON in a named, versioned envelope.** `.inbetween` is `{"format":"inbetween-project","version":1,"project":{...}}` and `.inbetweendots` is `{"format":"inbetween-dots","version":1,"pairs":[...]}`. JSON (via `serde_json`, MIT/Apache-2.0) keeps files diffable and writable by agents. The marker is checked before the body so a wrong or newer file gets a clear message. Reading ignores unknown fields and defaults missing ones, so adding an optional field never needs a version bump; a higher version than the build knows is refused (`NewerVersion`), never half-read. A UTF-8 BOM is tolerated.

**D17 (2026-10-18): Reading is bounded and writing is safe.** Files over 64 MiB, projects over 10,000 sequences, and any model-limit breach (frames, dots, output size, non-finite numbers) are errors. Image paths must be relative: absolute paths, drive letters and UNC paths are rejected on read and write; `..` is allowed so a project can sit beside an `images` folder. `make_relative` builds such paths lexically. Writes go to a temporary file in the same directory and are renamed over the target, so a full disk or crash leaves the old file intact; an invalid project is never written.

**D18 (2026-10-18): Parser robustness is tested twice.** `crates/format/tests/robustness.rs` runs deterministic mutation fuzzing on stable Rust in every `cargo test` (all truncations, all single-byte changes, 60,000 mutated seeds, random bytes, depth bombs), asserting no panic and write/read stability. `fuzz/` holds libFuzzer targets (`project`, `dots`) run by `.github/workflows/fuzz.yml`: 3 minutes per target on pull requests touching `format`, `doc`, `geom` or `fuzz`, 15 minutes weekly. The fuzz crate sits outside the workspace because `libfuzzer-sys` carries the (permissive) NCSA licence, which `deny.toml` does not allow.

**D19 (2026-10-18): Commands are data plus two function pointers.** A `CommandSpec` has an id, label, menu path, shortcut, description, a static parameter schema (`ParamSpec`: name, kind, required, range, default), an `enabled(&Session)` check with a reason, and `run(&mut Session, &Params)`. `Registry::run` validates parameters against the schema before `run` is reached: the value must be an object, unknown keys are rejected (the error lists the valid ones), types and ranges are checked, strings are capped at 4,096 bytes and may not contain NUL. Commands therefore never see raw JSON. `Registry::run_caught` additionally turns a panic into `CommandError::Internal` for shells that must keep going; tests use `run` so a panic stays visible. Commands live in `<area>_cmds.rs` modules with a `specs()` function. File-touching commands are compiled out on wasm32 (a platform service will replace them).

**D20 (2026-10-18): Image paths resolve against the project file's folder,** are re-checked as relative on every load, and loaded images are cached per `Session` keyed by full path with a (modified time, length) stamp, so a rendered sequence reads each source once and an edited file reloads. `export.sequence` numbers frames from 1 (`frame_0001.png`) and its `prefix` may not contain path separators. `sequence.length` accepts 2 to 999, matching the spec (hold frames arrive in M9); the model itself still allows 1 to 100,000.

**D21 (2026-10-18): Two bugs the M5 tests found, fixed at the source.** (1) serde_json parses floats one bit off unless its `float_roundtrip` feature is on, so dot positions drifted across save and load; the feature is now enabled and a 20,000-pair bit-exact test guards it. (2) Opening a Windows device name such as `CON` blocked forever; `read_file`, `read_project` and `read_dots` now refuse anything that is not a regular file.

**D22 (2026-10-18): The demo pair is generated, not photographic.** `testkit::demo` draws a sun face and a boxy moon face from formulas and returns matching dot pairs; `demo/` holds the rendered PNGs and project, and a test fails if they drift from the generator. No third-party assets, so no `ATTRIBUTION.md` row is needed.
