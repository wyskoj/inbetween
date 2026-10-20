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

Crate directories drop the `inbetween-` prefix (`crates/geom` is package `inbetween-geom`). `inbetween-testkit` and `xtask` sit outside the stack. Crates are added when a milestone needs them. Present so far: `geom`, `raster`, `codecs`, `doc`, `morph`, `render-cpu`, `format`, `testkit`.

L0 to L5 stay wasm-clean (file-system code is gated behind `cfg(not(target_arch = "wasm32"))` or a platform service).

## The engine/UI seam

`engine` owns a `Session` (the open project plus undo history) and a registry of commands. Each command has an id (see `docs/feature-spec.md`), label, menu path, shortcut, params schema, `enabled` and `run`. Shells only dispatch by id and render state. Pure view state (zoom, panel layout) is the only thing a shell keeps to itself, and it lives in one serde struct so the CLI and MCP can read and drive it.

## Document model (`doc`, built in M3)

- `Project`: one or more `Sequence`s plus output settings.
- `Sequence`: Image 1, Image 2, a paired `DotSet`, frame count, morph type, triangulation option.
- `DotSet`: dots stored as pairs, one position per image. Dots may lie outside the image.
- Image references are relative paths in files; pixels are loaded by the engine, never stored in `doc`.

All types are plain data with `serde`, and new fields use `#[serde(default)]` so older files keep loading.

## Rendering

For frame `t`: interpolate the dot positions, warp each source triangle by triangle onto the interpolated mesh (one shared triangulation), then cross-dissolve. `render-cpu` is the oracle; `render-gpu` must match it within a stated tolerance on every test scene. Output is deterministic.

## Error handling

Every crate has its own error type and returns it with `?`. Non-test code never panics (see AGENTS.md §3).

## Frame pipeline (as built in M3)

`morph::Planner::new(sequence, w, h)` validates the sequence and triangulates the dots on Image 1 (the shared mesh) and on Image 2 (used only for the last frame) once. `plan(t)` then returns a `FramePlan`: for each mesh triangle its position on Image 1, on Image 2 and in this frame, plus the dissolve weight. `render_cpu::Renderer` turns a plan into pixels: for every pixel centre inside a frame triangle, the inverse affine map finds the matching point on each source, which is fitted to the output size, sampled bilinearly and dissolved. Triangles render in parallel and merge in triangle order.

## Engine and command registry (as built in M5)

`inbetween-engine` holds `Session` (project, its file path, an image cache) and `Registry` (the `CommandSpec` list). Shells never edit a project directly: they call `registry.run(session, "export.frame", params)` with JSON parameters and get JSON back, or a `CommandError` whose message says what to change and whose `code()` is stable for scripts. `Registry::describe()` serialises every command and its parameter schema, which is what `ib commands --json` prints and what MCP will expose.

`ib` (`apps/inbetween-cli`) is a thin shell: each subcommand opens a session with `project.open` and runs one or two commands. `ib run <id> --params '{...}'` reaches any command, so every feature is scriptable the day it is added.

## GPU renderer (as built in M7)

`inbetween-render-gpu` (L3) mirrors `render-cpu`'s shape: `GpuContext` (device and queue, shareable and cloneable) and `GpuRenderer::new(ctx, sequence, output, image1, image2)`, which uploads the sources once as `Rgba32Float` textures. `draw(t)` plans on the CPU with `morph::Planner`, uploads one vertex buffer and draws the mesh into a texture exposed by `texture()`, which a live preview can show directly; `render_t` / `render_frame` also read the frame back into a raster `Image`. It compiles for wasm32 (checked by `cargo xtask wasm`). The CPU renderer remains the oracle: see D24 for the tolerance and `crates/render-gpu/tests/matches_cpu.rs` for the scenes.

## The desktop shell (as built in M8)

`ui-egui` (L6) plus `apps/inbetween` (the `inbetween` binary). `InbetweenApp::show(&egui::Context)` draws everything for one frame and works with or without a GPU handle, so the same code runs in the window and in egui_kittest tests. Data flows one way: the `Session` is read into a per-frame snapshot (dots, mesh triangles, textures), panels draw the snapshot and return `Action`s, and the app dispatches them as commands. The preview refreshes only when `(session.revision(), frame)` changes. `inbetween project.inbetween --screenshot out.png` captures the real window, including the GPU path, for visual checks.

## Curves and multi-sequence projects (as built in M10)

`doc::Curve` is plain data (points plus a `linear` flag) with `eval`, `insert`, `move_point` and `remove`; a `Sequence` carries a feature and a shape curve and a `visible` flag, and a `Project` knows its movie length and which sequence shows a movie frame. `morph::Planner` applies the curves when it plans a frame, so both renderers and every export see them. The engine's sequence-list commands (`list_cmds.rs`), curve commands (`curve_cmds.rs`) and `export.movie` are ordinary registry commands. In the shell, `seqlist.rs` draws the list and `curves.rs` the curve editor; both only return actions (`ListAction`, `CurveAction`) that `InbetweenApp` turns into commands, and a curve drag is one undo step through `edit.beginGroup`. The playing sequence is `ViewState::sequence`; whole-movie playback in the shell is not built yet (export and `project.info` cover the movie).

## Effects and animated export (as built in M11)

`doc::Effects` (back colour, background, mask, foreground) is plain data on every `Sequence`, validated with it and saved with the project. `render-cpu::Renderer` takes the loaded effect images (`with_effect_images`) and applies `effects::apply` after the morph; `engine::Prepared::renderer()` builds it from a session. The GPU renderer does not draw effects, so the shell previews an effect sequence on the CPU. `inbetween-export` (L4) turns RGBA8 frames into GIF, APNG and animated WebP bytes, and starts `ffmpeg` for video; the engine's `movie::render_movie` walks the visible sequences (range, reverse, join-duplicate dropping) and feeds `export.animation`, `export.video` and `export.movie`. See D34 and D35.


## Layers, camera, track and more effects (as built in M12)

`doc::Project::kind` (`ProjectType`) picks sequential or layered playback; `Project::layers_at` says which frame of which sequence each layer shows, and `engine::movie::render_layers` composites them. `doc::Camera`, `doc::Track` and `Sequence::master_curve` are plain data on every `Sequence`; `Effects` gained transition, light, trail and radial. The track bends dot paths inside `morph::Planner` (so the GPU renderer gets it too). `render-cpu` draws the rest: per-pixel transition mixing in `draw`, `camera::apply` after the morph, `trail::combine` over earlier frames (`render_frame` only), then `effects::apply` (light, mask, rays, background, foreground) and the master fade. Commands: `project.type`, `camera.keyframes`, `camera.curve`, `curve.master`, `curve.track`, `effect.transition/light/trail/radial`. See D36.

## Face tools (as built in M13)

`doc::FaceModel` (landmarks on a unit face box, enabled and important flags, guide lines; presets `standard` and `minimal`) is plain data on the `Project`. `morph::face` holds the maths: place the model on a `FaceBox`, fit a box to landmark positions, smart drag, mix weights and the weighted mean shape. `format` reads and writes the model (`.inbetweenface`), landmark positions (`.inbetweenlandmarks`, the hand-over from any detector) and mix recipes (`.inbetweenmix`). The engine's `face_cmds.rs` (model, locate, smart drag) and `face_media.rs` (extract, mix, library) are ordinary registry commands. A face is the model's enabled landmarks as consecutive dots in a sequence, so several faces are several runs of dots addressed by `first`. See D37.

Since M19 the bundled detector (`crates/detect`, L2, D52) finds the faces: `detector` runs two ONNX models with `tract-onnx` (box detector, then a 468-point mesh on a rotated crop), `anchors` and `geometry` hold the model-independent maths, and `mesh_map` maps standard-model landmark names to mesh indices with a similarity-fit fallback. The engine's `face_detect.rs` runs it on each image's processed source, maps points through the sequence fit into output pixels, caches detections per source image, and holds `face.detect`, `face.locateAll` and the helpers `face.locate` (`source` detect) and `face.extract` use. The crate takes only `Image` in and points out; it knows nothing of `doc`.

## Projects and files (as built in M15)

`codecs` reads PNG, JPEG, BMP, TIFF, TGA, GIF and (flattened) PSD; `IMAGE_EXTENSIONS` is the one list of them. The project envelope (`format`) may carry an optional `thumbnail` (base64 PNG, small); `write_project_with_thumbnail` and `read_thumbnail` handle it and the project itself never depends on it. Image paths are relative to the project file where possible and absolute otherwise. `engine/src/file_cmds.rs` holds the commands that work across projects (`file.appendProject`, `file.wizard`, `file.openVisual`); `project_cmds.rs` has `file.new`, `project.saveAs` and the save path that rebases image references and renders the thumbnail; `export_cmds.rs` has `frame.exportEdit`. In the shell, `dragdrop.rs` decides what a drop means, `open_visual.rs` draws the Open Visually window from the `file.openVisual` result, and `app.rs` holds the unsaved-changes prompt (`REPLACES_PROJECT`), Open Recent and the drop handling. `ib run --save out.inbetween` writes the session's project after a command, so `ib run file.wizard --params '{"images": ["a.png", "b.png"]}' --save out.inbetween` makes a project from the command line. See D39 to D42.


## Source edits and captions (M16)

`raster` holds the pixel operations (`transform`, `adjust`, `filter`). `doc::ImageEdit` and `doc::Caption` are plain data on every `Sequence` (`edit1`, `edit2`, `caption`) and on `Project` (`caption`); `ImageEdit::geometry` defines the edited size and the edited-to-source map. `render-cpu::source::apply_edit` applies an edit and `render-cpu::caption` draws captions (`render_layer`, `composite`, `caption_bounds`, bundled Roboto). `Session::source` caches processed sources and `Session::caption_layer` caches drawn caption layers; `Prepared` carries the sequence and movie layers. Commands live in `engine/src/image_cmds.rs`, `filter_cmds.rs` and `caption_cmds.rs`. The shell adds `crop.rs` (box geometry), `app/crop_ui.rs`, `app/adjust_ui.rs` and `app/caption_ui.rs`. See D43 to D47.

## Movie, preview and workspace (M18)
Movie settings are engine commands (`movie_cmds.rs`: `movie.size`, `movie.fps`); `Session::render_movie_frame` renders any movie frame. In the shell, `app/transport_ui.rs` owns the playhead (sequence or movie scope), `actions.rs` plus `app/actions_ui.rs` the view actions, `app/layout_ui.rs` the layouts, splitters and dual window, and `app/frames_ui.rs` the frame strip. See D49 to D51.
