# Architecture

Inbetween is a Rust-only morphing app. The desktop UI is a thin shell over an engine; every action is a command, so the UI, the CLI (`ib`) and MCP drive the same code.

## Layers

A crate depends only on crates in a strictly lower layer. `cargo xtask layers` enforces this; the registry is `xtask/src/layers.rs`.

| Layer | Crates | Role |
|---|---|---|
| L0 | `geom`, `raster`, `codecs` | Math, pixels, image file I/O. No workspace deps. |
| L1 | `doc` | Project model as pure data. |
| L2 | `morph` | Triangulation policy, dot interpolation, curves, frame planning. |
| L3 | `render-cpu`, `render-gpu`, `format` | Renderers and file formats. |
| L4 | `export` | Frame sequences, animated formats, video. |
| L5 | `engine` | `Session` plus the command registry. |
| L6 | `ui-egui`, `mcp` | Shells over the engine. The only layer allowed egui, eframe, winit, rfd. |
| apps | `inbetween` (desktop), `inbetween-cli` (binary `ib`) | Entry points. |

Crate directories drop the `inbetween-` prefix (`crates/geom` is package `inbetween-geom`). `inbetween-testkit` and `xtask` sit outside the stack. Crates are added when a milestone needs them; none exist yet beyond `xtask`.

L0 to L5 stay wasm-clean (file-system code is gated behind `cfg(not(target_arch = "wasm32"))` or a platform service).

## The engine/UI seam

`engine` owns a `Session` (the open project plus undo history) and a registry of commands. Each command has an id (see `docs/feature-spec.md`), label, menu path, shortcut, params schema, `enabled` and `run`. Shells only dispatch by id and render state. Pure view state (zoom, panel layout) is the only thing a shell keeps to itself, and it lives in one serde struct so the CLI and MCP can read and drive it.

## Document model (target for M3)

- `Project`: one or more `Sequence`s plus output settings.
- `Sequence`: Image 1, Image 2, a paired `DotSet`, frame count, morph type, triangulation option.
- `DotSet`: dots stored as pairs, one position per image. Dots may lie outside the image.
- Image references are relative paths in files; pixels are loaded by the engine, never stored in `doc`.

All types are plain data with `serde`, and new fields use `#[serde(default)]` so older files keep loading.

## Rendering

For frame `t`: interpolate the dot positions, warp each source triangle by triangle onto the interpolated mesh (one shared triangulation), then cross-dissolve. `render-cpu` is the oracle; `render-gpu` must match it within a stated tolerance on every test scene. Output is deterministic.

## Error handling

Every crate has its own error type and returns it with `?`. Non-test code never panics (see AGENTS.md §3).
