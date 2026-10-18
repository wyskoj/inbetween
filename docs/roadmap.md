# Inbetween roadmap

## Current focus

**P0 is complete (tagged v0.1.0): a headless morph engine.** `ib` turns two images and a dot set into a correct, deterministic PNG frame sequence, and the morph invariants (AGENTS.md §3) pass. M0 to M6 are done; the baseline is in `docs/perf.md`.

**Next: M7** (wgpu renderer matching the CPU oracle), then M8 (egui shell).

*Original P0 goal, for reference:* a headless morph engine with no UI.

## P0 milestones

### M0: Scaffold
- Cargo workspace, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml` (panicking forms allowed in tests only), `unsafe_code = "forbid"`.
- `xtask` with `layers` and `ci`; `cargo-deny` config with the permissive licence allow-list.
- GitHub Actions: fmt, clippy `-D warnings`, test, layers, deny, on Windows, macOS and Linux.
- `docs/architecture.md`, `docs/decisions.md`, `log/devlog.md`, `.gitignore` (`plan/`, `log/`, `target/`).

**Exit:** green CI on an empty workspace.

### M1: Geometry (`geom`)
- Point, triangle, axis-aligned bounds; barycentric coordinates; affine transform from 3 point pairs (handles degenerate triangles by returning an error).
- Delaunay triangulation of a dot set plus the 4 output corners. A permissive crate (e.g. `spade`) is fine; record the choice in `docs/decisions.md`.
- Duplicate and collinear dots are deduplicated or rejected with a clear error.

**Exit:** property tests (random dot sets) prove every triangulation covers the frame exactly, with no overlaps or gaps, and never panics.

### M2: Raster and codecs (`raster`, `codecs`)
- RGBA image type with f32 premultiplied working storage; 8-bit sRGB in and out at the I/O boundary.
- Bilinear sampling with edge clamp; alpha-correct blending.
- PNG and JPEG read/write, preserving alpha.

**Exit:** round-trip tests, a synthetic image generator in `testkit` (gradients, checkerboards, labelled grids).

### M3: CPU morph (`doc`, `morph`, `render-cpu`)
- Pure-data `doc` types: a sequence holds Image 1, Image 2, a paired dot set, frame count, morph type.
- Frame t: interpolate dots (linear for now), warp each source triangle-by-triangle onto the interpolated mesh using one shared triangulation, cross-dissolve.
- Sources of different sizes are fitted to the output size first; fit mode recorded in `docs/decisions.md`.
- Morph types: Morph and Warp Image 1 only. Others come in P1.
- Triangulation option: once at frame 0 (default) or per frame.

**Exit:** all five morph invariants pass; per-triangle parallel rendering with rayon.

### M4: File formats (`format`)
- `.inbetween` project file: versioned JSON, relative image paths, forward-compatible (`#[serde(default)]` on new fields).
- `.inbetweendots` dot-set file for importing and exporting dots alone.
- Fuzz the parsers (`cargo fuzz`) for at least a few minutes in CI on changed code.

**Exit:** round-trip tests; malformed files return errors, never panics.

### M5: Engine and CLI (`engine`, `inbetween-cli`)
- Minimal command registry with the first commands: `project.open`, `project.save`, `dots.import`, `dots.export`, `sequence.length`, `export.frame`, `export.sequence`.
- CLI:
  - `ib render project.inbetween --out frames/` renders every frame
  - `ib frame project.inbetween --t 0.5 --out mid.png` renders one frame
  - `ib info project.inbetween` prints sequences, sizes, dot counts
  - `ib commands` lists every command with its params schema
- A `panic_hunt` test that calls every command with adversarial params.

**Exit:** a public-domain demo pair with a hand-placed dot set renders end to end from the CLI.

### M6: Performance baseline
- `xtask perf`: time a 1920×1080 frame with 1,000 dot pairs, CPU, release build.
- Starting budget: under 100 ms per frame on a 4-core machine. Record the baseline; tighten it later.

**Exit:** P0 complete. Tag `v0.1.0`.

## After P0 (outline only)

| Milestone | Goal |
|---|---|
| M7 | wgpu renderer matching the CPU oracle; real-time preview speed |
| M8 | egui shell: two source panels, live preview, add/move/delete dots, zoom and pan, undo |
| M9 | Remaining morph types, sequence length with hold frames, playback controls |
| M10 | Feature and shape curves, multi-sequence projects, sequence list |
| M11 | Effects (background, mask, foreground), GIF/APNG/WebP and video export |
| M12 | P2 block: layer projects, camera, track curves, transition/light/trail/radial |
| M13 | Face tools on open landmark models |
| M14 | MCP server, packaging and signed releases |
