# AGENTS.md: guide for AI agents and contributors

Inbetween is an open-source, native image morphing and warping app written in **Rust only**. You place paired key dots on two images, and Inbetween renders the frames in between: shape deforms, colour dissolves. The desktop app is native egui/eframe on wgpu. No Tauri, Electron, webview or JavaScript UI, ever.

The product name is always **Inbetween** in user-facing text. Machine names are lowercase: crates (`inbetween-*`), the desktop binary (`inbetween`), the CLI binary (`ib`), file extensions (`.inbetween`).

The goal is a complete, fast, professional morphing tool where every feature is drivable from the CLI and by agents. `docs/feature-spec.md` is the definition of "complete". Read this file first, then `docs/`.

## 1. Orientation

| Read | Why |
|---|---|
| `docs/roadmap.md` | Milestones, **current focus**, exit criteria |
| `docs/feature-spec.md` | Every feature Inbetween should have, with status, priority and command id. The work list. |
| `docs/architecture.md` | Crate map, layers, the engine/UI seam, the document model (write it in M0) |
| `docs/decisions.md` | Recorded design decisions and why. Append, never silently change. |
| `log/devlog.md` (local) | What the last session landed and what is still open |

## 2. Workspace map

```
crates/
  geom          L0  points, triangles, Delaunay, barycentric and affine math
  raster        L0  RGBA images, sampling, premultiply, blend math
  codecs        L0  image file read/write (standalone, no workspace deps)
  doc           L1  project model: sequences, images, dot sets, curves, effects (pure data)
  morph         L2  triangulation policy, dot interpolation, curves, frame planning
  render-cpu    L3  CPU renderer: the reference oracle
  render-gpu    L3  wgpu renderer, tested against render-cpu
  format        L3  .inbetween project and dot-set files (versioned, serde)
  export        L4  frame sequences, GIF/APNG/WebP, video
  engine        L5  Session + command registry (every action is a command)
  ui-egui       L6  egui shell, thin: all actions go through the engine
  mcp           L6  MCP server over the command registry
  testkit           test helpers and synthetic image generators
apps/
  inbetween              desktop app
  inbetween-cli          headless CLI (render, frame, info, run, commands, mcp)
xtask/              cargo xtask layers | wasm | ci | spec | perf
```

**Layering is enforced** by `cargo xtask layers`. A crate depends only on lower layers. Nothing below L6 may use egui, eframe, winit or rfd. Register new crates in `xtask/src/layers.rs`. Crates appear as their milestone needs them; don't scaffold empty ones.

## 3. Golden rules

### Never crash (outranks feature work)

A malformed file, bad params, a degenerate dot set or a full disk produces an actionable error, never a panic.

- Non-test code never uses `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` or `unimplemented!`. Return the crate's error type and use `?`. Unfinished features return an "unsupported" error.
- No `unsafe`: the workspace sets `unsafe_code = "forbid"`.
- Input-derived numbers are hostile: use `get()` over indexing, `checked_*` or `saturating_*` arithmetic, guard division by zero and NaN, cap allocations sized by input.
- Degenerate geometry is normal input: duplicate dots, collinear dots, zero-area triangles, dots outside the image, fewer than 3 dots. Handle all of them.
- Every crash fix ships with a regression test that panicked before the fix.
- Enforced by clippy: `clippy.toml` allows the panicking forms in tests only; every crate carries `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`.

### The rest

1. **Everything is a command.** New user-visible behaviour is a command in `crates/engine` (id, label, menu path, shortcut, params schema, `enabled`, `run`) with tests. UI, CLI and MCP all dispatch by id. Use the id from `docs/feature-spec.md`. Only pure view state (zoom, panel layout) lives in the shell.
2. **The CPU renderer is the oracle.** `render-gpu` must match `render-cpu` within a stated tolerance on every test scene. Never "fix" the oracle to match the GPU.
3. **Tests are the gate.** Every change comes with tests. The morph invariants below must always hold.
4. **The UI is thin and data-driven.** UI state lives in one serde struct so the CLI and MCP can read and drive it.
5. **Verify UI changes visually.** Render offscreen and look at the PNG. Demo and test images are public-domain or generated, never personal photos.
6. **Keep L0 to L5 wasm-clean.** `cargo xtask wasm` must pass. File-system code sits behind `cfg(not(target_arch = "wasm32"))` or a platform service.
7. **Performance is a feature.** Parallelise per tile or per triangle with rayon, never recompute what a revision cache can hold, and record before/after timings in the dev log.
8. **Dependencies are permissive only** (MIT, Apache-2.0, BSD, zlib, ISC). `cargo deny check` enforces it.

### Morph invariants (must always pass)

- Frame at t=0 equals Image 1; frame at t=1 equals Image 2 (within rounding).
- Identical dot sets on both images reduce the morph to a pure cross-dissolve.
- Warp with one image, unmoved dots: every frame is the identity.
- Output never contains NaN, and alpha stays in [0, 1].
- Rendering is deterministic: same project, same frame, same bytes.

## 4. Independence and clean-room rules

Inbetween is its own product. It is designed from its own spec, public algorithms and published papers.

- **Never name, quote or describe any other morphing product** anywhere in the repository: code, comments, docs, tests, commit messages, issues, PRs, file names or asset names. Features are described only as Inbetween features.
- Never copy code, shaders, assets, dot models or file formats from other software. Inbetween's file formats are its own.
- Never decompile, disassemble or inspect the binaries of other software.
- Research notes live in `plan/` (gitignored, local only) and never get committed or quoted.
- Third-party assets must be permissively licensed and get a row in `ATTRIBUTION.md` in the same change.

## 5. Picking work

1. `docs/roadmap.md`, **Current focus**.
2. `docs/feature-spec.md`: the lowest-priority-number `[ ]` rows in the current milestone.
3. `log/devlog.md`: the "still open" bullets of recent entries.

When you finish a feature, flip its row in `docs/feature-spec.md` to `[~]` or `[x]` in the same commit.

## 6. Before you finish a task

```
cargo test -p <crates you touched>
cargo clippy -p <crates> --all-targets -- -D warnings
cargo xtask layers
cargo xtask wasm          # if you touched L0 to L5
cargo deny check          # if you added a dependency
cargo xtask perf --quick  # if you touched a hot path
```

Then append a terse entry to `log/devlog.md`: what landed, numbers, what is still open. Sessions can end abruptly; the dev log plus a green tree is how the next session picks up. Keep the tree building at every commit.

Commit messages: short imperative subject, a body of one or two lines at most.

## 7. Parallel agents

- Use your own target dir (`CARGO_TARGET_DIR=target/agent-<name>`) and edit only the files you own.
- Put new commands in a new module (`engine/src/<area>_cmds.rs` with a `specs()` function) instead of growing a shared file. Shared files get small, surgical edits; re-read before editing.
- If someone else's in-progress edit breaks the build, wait and retry; don't fix their files.
- Write `Cargo.toml` changes atomically (temp file, then `mv`) so a half-written manifest never breaks other builds.

## 8. Lessons learned

**Tooling**
- Set `CARGO_TARGET_DIR` as an absolute path. A relative one creates a stray `crates/target` when you run from a subfolder, and the workspace then fails to load.
- On Windows, write multi-line files and patch scripts with the Write tool, not Bash heredocs; heredocs containing quotes failed repeatedly.
- `cargo fmt --all` reformats files under you. Patch with Edit or exact-match replacement after reading the current text.
- `cargo xtask wasm` fails when file-reading code is not behind `cfg(not(target_arch = "wasm32"))`. Add the cfg when you add the function.

**Workflow**
- Run `cargo check -p <crate>` after each file; run clippy, wasm and the full tests at the end of a step.
- `cargo test -p inbetween-format` takes about 2 minutes (the robustness test). Run it once per step and filter by test name while iterating.
- Put new shell UI in submodules under `crates/ui-egui/src/app/`. `app.rs` is about 2000 lines and string patches to it are brittle.

**Testing the shell (egui_kittest)**
- egui reports a drag only after the pointer has moved, so use `pointer.press_origin()` to find what was grabbed.
- A slider's label matches two accessibility nodes; use `get_by_role_and_label(Role::Slider, ...)`.
- Screenshots: `INBETWEEN_UI_SHOTS=<dir>` on the kittest suites; a real-window check is `inbetween <project> --screenshot <png>`.

**Planning**
- When a plan has independent steps (for example filters, captions and shell tools once the model and cache exist), say so, so they can run as parallel agents under section 7.
