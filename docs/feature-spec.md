# Inbetween feature spec

Everything Inbetween should do, grouped by area. This is the work list: pick unchecked rows in the current milestone (see `docs/roadmap.md`) and flip them when done.

**Status:** `[ ]` missing · `[~]` partial · `[x]` done
**Priority:** **P0** engine MVP · **P1** core editor · **P2** advanced
**Ids** are the planned command ids. Every user-visible behaviour is a command.

---

## 0. Engine behaviour

A morph is *shape deformation* plus *feature (colour) cross-dissolve*, driven by paired key dots that are triangulated into a mesh.

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `engine.triangulate` | Triangulate paired dots, auto-adding the 4 image corners | P0 |
| [x] | `engine.warp` | Per-triangle affine warp of each source toward the interpolated dot positions | P0 |
| [x] | `engine.dissolve` | Cross-dissolve the two warped images by the feature amount | P0 |
| [x] | `engine.alpha` | Full alpha pipeline: 32-bit RGBA sources in, RGBA frames out (PNG, BMP, TIFF, TGA, GIF and PSD in; PNG and JPEG out) | P0 |
| [x] | `morph.option.dropCornerTris` | Option: discard triangles touching the 4 corners (backdrop effect on non-alpha images) | P1 |
| [x] | `morph.option.retriangulate` | Option: re-triangulate every intermediate frame instead of only frame 0 | P1 |
| [x] | `engine.outsideDots` | Dots may sit outside the image bounds (engine, files, CLI and MCP; the shell only adds dots inside the frame, though a dot can be dragged past the edge) | P1 |

### Morph types (per sequence)

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `morph.type.morph` | Image 1 morphs into Image 2 (default) | P0 |
| [x] | `morph.type.morphHold` | Morph while Image 1 never fades (partial morph using an alpha Image 2) | P1 |
| [x] | `morph.type.warp1` | Image 1 deforms into Image 2's shape; Image 2's colours never appear | P0 |
| [x] | `morph.type.warp2` | Image 2 deforms from Image 1's shape; Image 1's colours never appear | P1 |
| [x] | `morph.type.fadeOut1` | Image 1 fades to black | P1 |
| [x] | `morph.type.fadeIn2` | Image 2 fades in from black | P1 |
| [x] | `morph.type.direct` | No morph; Image 2 cuts in at the end (slideshow when paired with a Transition effect) | P1 |

**Oracle tests:** frame 0 equals Image 1; last frame equals Image 2; identical dot sets reduce to a pure cross-dissolve; Warp 1 with identical images and unmoved dots is the identity.

---

## 1. Projects and files

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `file.new` / `project.open` / `project.save` / `project.saveAs` | `.inbetween` project file: blank New (Ctrl+N), Open, Save, Save As (Ctrl+Shift+S), recent files, unsaved-changes prompt on replace and on close, cross-drive images kept as absolute paths, embedded thumbnail | P0 |
| [x] | `file.openVisual` | Open dialog that shows project thumbnails: recent projects or a chosen folder | P2 |
| [x] | `file.appendProject` | Merge another project's sequences into this one | P1 |
| [x] | `file.wizard` | New-project wizard: pick type, pick an ordered image list (Add, Up, Down, Remove), optional loop back to the first, build sequences | P1 |
| [x] | `dots.export` / `dots.import` | Export/import a dot set on its own (`.inbetweendots`) | P1 |
| [x] | `frame.exportEdit` | Export a single frame together with its dot set | P2 |
| [x] | `file.dragDrop` | Drop images onto either source panel (done), a project or dot file, or several images to open the wizard; images and sound files onto the Effects window's slots | P1 |

**Image import:** BMP, JPEG, TIFF, PNG, PSD (flattened), TGA, GIF; keep alpha for BMP/TIFF/PNG/PSD/TGA. P0 for PNG/JPEG, P1 for the rest. *Status: done (`codecs` reads all of them; PSD is an in-house flattened-composite reader; GIF reads the first frame; import only, export stays PNG/JPEG).*

---

## 2. Source image editing (per image)

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `image.crop` | Drag crop; free or locked aspect; crop may extend past the image; arbitrary-angle rotate inside crop | P1 |
| [x] | `image.rotate` | 90° CW, 90° CCW, 180°, arbitrary angle | P1 |
| [x] | `image.flip` | Horizontal, vertical | P1 |
| [x] | `image.adjust` | Brightness, contrast, R/G/B, hue, saturation, negative, grayscale | P1 |
| [x] | `image.filter.*` | Smooth, blur, Gaussian, zoom blur, radial blur, sharpen, emboss, mosaic, oil paint, splash, cylinder, whirl, pinch | P2 |
| [x] | `image.caption` | Per-image caption (see §8) | P2 |
| [x] | `image.swap` | Swap Image 1 and Image 2 (F3) | P1 |
| [x] | `image.saveToLibrary` | Store image plus dots in the face library (§10) | P2 |

All source edits are undoable and non-destructive (dots stay attached to the edited image).

---

## 3. Key dots

### Editing

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `dots.add` | Click to add; partner dot auto-created on the other image | P0 |
| [x] | `dots.option.autoPartner` | Toggle auto-creating the partner dot; when off, dots are added per image and shown as unpaired | P1 |
| [x] | `dots.addSamePos` | Modifier (Alt) puts the partner at identical coordinates (boundary/anchor dots for warps) | P1 |
| [x] | `dots.option.autoJump` | Toggle moving the cursor to the partner position after adding | P2 |
| [x] | `dots.move` | Drag a dot; preview updates live | P0 |
| [x] | `dots.delete` | Delete tool, or right-click any dot with any tool | P0 |
| [x] | `dots.select` | Marquee select; Shift adds, Alt subtracts; clicking a line selects its connected dots | P1 |
| [x] | `dots.transform` | Move, scale, rotate, skew, flip a selection | P1 |
| [x] | `dots.nudge` | Arrow keys move selection 1 px; Ctrl+arrow 10 px | P1 |
| [x] | `edit.cut/copy/paste/delete` | Clipboard ops on dots, keeping connected lines | P1 |
| [x] | `dots.selectAll` / `dots.clearAll` | | P1 |
| [x] | `dots.matching` | Paired vs unpaired dots shown in different colours; partner blinks when hovering an unpaired dot | P1 |
| [x] | `dots.autoMatch` | Generate partner dots at plausible positions automatically | P2 |
| [x] |  | Limit: at least 2048 dot pairs (saved, reloaded, rendered and drawn in tests) | P0 |

### Lines (organizational only; never affect the morph)

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `lines.addPolyline` | Click repeatedly to drop dots joined by segments; one modifier finishes with a final dot, another (or right-click) ends without one | P1 |
| [x] | `lines.connect` | Connect any two existing dots | P1 |
| [x] | `lines.insertDot` | Clicking on a segment inserts a dot into it | P1 |
| [x] | `lines.delete` | Remove a segment, keep its dots; deleting a dot removes its segments | P1 |
| [x] | `lines.drag` | Drag a segment to move it with all connected dots and lines | P1 |

### Display

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `view.toggle.dots/lines/triangles/track/grid` | Show/hide dots, lines, triangle mesh, morph track, grid | P1 |
| [x] | `prefs.dotStyle` | Several dot styles plus highlighted variants | P2 |
| [x] | `prefs.dotColor` | Single colour, or cycle 8 colours per pair; dot opacity 64–255; antialiased | P2 |
| [x] |  | Hover highlights a dot pair; hovering a line highlights everything connected | P1 |
| [x] | `view.magnifier` | Loupe around the cursor, 1×–4× | P2 |

---

## 4. Sequences and project types

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `sequence.length` | 2–999 morph frames, plus 0–999 hold frames on each end, set separately; enter as frames or seconds; optional apply-to-all | P0 |
| [x] | `sequence.add` | Add a continued sequence (previous Image 2 becomes new Image 1) | P1 |
| [x] | `sequence.addBlank` | Add an empty sequence | P1 |
| [x] | `sequence.delete/duplicate/rename` | | P1 |
| [x] | `sequence.visibility` | Hide/show; modifier-click toggles solo | P1 |
| [x] | `sequence.reorder` | Up, down, to top, to bottom, drag in list (drag a title onto another row) | P1 |
| [x] | `sequence.link` | Make Image 1 equal the previous sequence's Image 2 (seamless join) | P1 |
| [x] | `sequence.copyFrame` | Copy a source image with its dots into another sequence (drag one thumbnail onto another in the list) | P1 |
| [x] | `project.type.sequence` | Sequences play one after another; movie length is the sum | P1 |
| [x] | `project.type.layer` | All sequences play at once as stacked layers (first = top); the movie has one length (the longest layer; shorter ones hold their last frame, or give them one length with `sequence.length` apply-to-all); switchable at any time from the list or `project.type` | P2 |
| [x] | `view.sequenceList` | Panel listing sequences with both thumbnails, name, visibility (F2) | P1 |

---

## 5. Curves

All curves are smooth splines through ≥2 control points, with per-frame tick marks, live-editable during playback, and can be applied to all sequences. A **curve library** saves and loads named curves.

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `curve.feature` | Remaps the colour-dissolve amount over time | P1 |
| [x] | `curve.shape` | Remaps the shape-interpolation amount over time | P1 |
| [x] | `curve.master` | Controls the sequence's overall opacity (fade in/out) | P2 |
| [~] | `curve.track` | Dots travel along a 2D curve instead of a straight line; adjustable tension 0–20; flip H/V; endpoints movable (enables caricature/anti-face extrapolation) | P2 |
| [x] | `curve.edit` | Click to add point, drag to move, right-click to delete, linear toggle, reset | P1 |
| [x] | `curve.library` | Save, load, delete library curves | P2 |

---

## 6. Camera (per sequence, and per effect)

*Status: built. `camera.keyframes` (with `op` copy, paste, save, load, reset, apply_all and the `aspect` locks free, movie, image1, image2) and `effect.camera` are commands; the Camera window and the handles on the preview drive them (D60). The camera curve and master curve are in the Curves window.*

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `camera.keyframes` | Keyframes of offset/scale/rotation along a timeline; frames between are interpolated smoothly | P2 |
| [x] | `camera.handles` | On-canvas frame: drag to move, square handles to scale, circle handle to rotate; numeric entry; negative scale flips | P2 |
| [x] | `camera.aspect` | Lock keyframe proportion to movie, Image 1 or Image 2, or free | P2 |
| [x] | `camera.curve` | Optional easing curve for the camera path | P2 |
| [x] | `camera.copyPaste/reset/applyAll/load/save` | | P2 |
| [x] | `effect.camera` | Same camera controls drive an individual effect layer (animates a still effect) | P2 |

---

## 7. Effects

*Status: built. Every row is a command (`effect.*` with `scope=sequence|movie`, `effect.sound`, `movie.adjust`, `movie.filter.*`), saved in the project and drawn by the CPU renderer (D55 to D58). The Effects window has a thumbnail slot, on/off box and Clear for each effect, and files can be dropped onto the slots. Effects are CPU only; the GPU port is listed under Known debt.*

Movie-level effects cover the whole frame; sequence-level effects apply only inside that sequence's camera zone. Each effect has an on/off switch, a thumbnail slot, and a clear action.

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `effect.backColor` | Solid colour behind the morph; can tint a background image | P1 |
| [x] | `effect.background` | Image behind the morph, opacity 0–100 (visible through alpha or a mask) | P1 |
| [x] | `effect.mask` | Grayscale image controls morph opacity; intensity −100..100; invert option | P1 |
| [x] | `effect.foreground` | RGBA overlay (frames, graphics), opacity 0–100 | P1 |
| [x] | `effect.transition` | Grayscale image controls *when* each pixel dissolves (dark first); border softness 0–100; invert | P2 |
| [x] | `effect.light` | Grayscale image brightens/darkens the morph; separate bright and dark intensity | P2 |
| [x] | `effect.trail` | Motion blur (overlay on) or motion trail (overlay off); tint colour; intensity | P2 |
| [x] | `effect.radial` | Radiating rays over or under the morph; tint colour; intensity | P2 |
| [x] | `effect.sound` | WAV/MP3 track starting at frame 0; cut at movie end, loop, fade in/out lengths | P2 |
| [x] | `movie.adjust` / `movie.filter.*` | Same adjustments and filters as §2, applied to the output (filters also per sequence) | P2 |

---

## 8. Captions

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `caption.*` | Captions at three scopes: per source image, per sequence, whole movie | P2 |
| [x] |  | Multi-line text; font, size, style, colour, border size and colour, alignment | P2 |
| [x] |  | Shadow with angle, distance, colour, size (size 0 = none) | P2 |
| [x] |  | Position by drag or by normalized numeric coordinates | P2 |

---

## 9. Movie, preview and playback

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `movie.size` / `movie.fps` | Match an image's size, presets, or custom; dots move with their pictures, sequence and movie captions scale; project frame rate | P0 |
| [x] | `preview.scrub` | Scrub 0–100% in 1% steps; mouse wheel steps one frame; preview one sequence or all (sequence or whole movie, layers stacked) | P0 |
| [x] | `play.*` | Play/pause, first, last, previous, next frame, loop, ping-pong; 1–60 fps | P1 |
| [x] |  | Live preview updates while dragging dots, curves, camera, effects | P0 |
| [x] |  | Checkerboard behind transparent preview areas | P1 |

---

## 10. Face tools (P2 block)

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `face.extract` | Auto-detect the face in every source, crop to a shared template for consistent framing; adjustable template box; manual fallback; apply to all or one | P2 |
| [x] | `face.locate` | Auto-place Inbetween's standard face dot set; apply/append to all or one; multiple faces per image by appending | P2 |
| [x] | `face.locate.smartDrag` | "Intelligent" drag: moving one landmark re-fits the rest; modifier toggles plain drag | P2 |
| [x] | `face.detect` | Report the faces found in each source (score, box, standard-model landmarks; optional full mesh); read-only | P2 |
| [x] | `face.locateAll` | Place the model on every face found in both images, left to right (`face.locate` with `all_faces`) | P2 |
| [x] | `face.model` | Edit which landmark dots are used, highlight important ones, edit guide lines; presets; save/load | P2 |
| [x] | `face.mix` | Blend N faces into one still; per-image feature % and shape % with locks, shape-follows-feature lock, average, random, candidate gallery, save/load | P2 |
| [x] | `face.library` | Folder-organized store of images with dots; drag into a project to reuse | P2 |

Built on an open landmark model (MediaPipe face mesh, Apache-2.0; see D52). Inbetween's face dot model is its own design.

*Status (M13, M19): all six tools are commands (`face.model`, `face.locate`, `face.locate.smartDrag`, `face.extract`, `face.mix`, `face.library`, plus `image.saveToLibrary`), saved in the project (the model) or in `.inbetweenface`, `.inbetweenlandmarks` and `.inbetweenmix` files. M19 added the bundled detector (D52): `face.detect`, `face.locateAll`, `face.locate` with `source` detect (the default), `face` and `all_faces`, a `mesh478` landmarks layout for other tools, and detection inside `face.extract`. The shell has all of it (D62): smart drag on the source panels (`face.option.smartDrag`, Alt flips it for one drag), the Face Model window (landmark canvas, important highlights, guide lines, presets, save and load), the Mix Faces window (per-face sliders, average and random modes, a candidate gallery, recipes) and the Face Library browser (thumbnails, drag a picture onto Image 1 or 2). Hairline landmarks are approximate (the mesh stops at the forehead).*

---

## 11. Export

*Status: `export.animation` (GIF, APNG, animated WebP) and `export.video` (MP4, WebM, ProRes through an installed ffmpeg) take `fps`, `first`/`last`, `reverse` and `drop_duplicates`. `sequence` picks one sequence, `export.frame` takes `quality` and `dpi` (PNG and sequences take `dpi`), and `Session::cancel_handle` stops a running export (`Cancelled`). Video encoding stays with ffmpeg (D59).*

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `export.frame` | Current frame to an image file | P0 |
| [x] | `export.sequence` | Image sequence (PNG with alpha first) | P0 |
| [x] | `export.range` | All sequences, current one, or a range | P1 |
| [x] | `export.video` | Modern targets: MP4/H.264, WebM (with alpha), ProRes 4444 optional | P1 |
| [x] | `export.gif` | Animated GIF, loop or once, opaque or transparent (also APNG/animated WebP) | P1 |
| [x] | `export.options` | fps 1–60 (plus 29.97), JPEG quality, DPI, reverse frames, drop duplicate frames at sequence joins, cancellable | P1 |
| [ ] | `export.videoSource` | When a source is a video, include its clip before/after the morph | P2 |
| [x] | `export.afterSave/afterOpen` | Auto-save project before export; open the result afterwards | P2 |
| [x] |  | Output size not capped by the GPU (tile or fall back to CPU) | P1 |

**Video sources (P2):** import a video as Image 1 or 2; morph from the last frame of clip 1 to the first frame of clip 2.

---

## 12. Workspace and UI

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `view.mode.full/edit/preview/frames/dual/fullscreen` | Six layouts (F5–F9, F11); dual-monitor split of editor and preview (dual opens the preview in a second window; multi-monitor use is a manual check) | P1 |
| [x] | `view.split` | Resizable split between Image 1, Image 2, Preview | P1 |
| [x] | `view.zoom` | 12.5%–800% via tools, up to 1600% via wheel; best fit, actual size, fit width/height | P1 |
| [x] | `view.syncSources` / `view.syncPreview` | Zoom/pan both sources together, and the preview with them | P1 |
| [x] | `view.frameStrip` | Frame Sequence mode: thumbnail grid of every frame; thumb size adjustable | P1 |
| [x] | `edit.undo/redo` | Unlimited (a byte budget, at least 200 steps), covering images, dots, sequences, curves | P0 |
| [x] | `export.pdf` | Contact sheet of sources, preview or all frames, with captions | P2 |
| [x] | `tools.benchmark` | Measure preview and export throughput | P2 |
| [x] | `tools.sequencePacker` | Pack an arbitrary image list into a movie | P2 |

### Default keymap

| Action | Key | Action | Key |
|---|---|---|---|
| Sequence list | F2 | Swap images | F3 |
| View modes | F5 F6 F7 F8 F9 | Full screen | F11 / Alt+Enter |
| Grid / dots / lines / triangles / track | Ctrl+G / D / L / T / M | Edit / Add / Delete tool | E / A / D |
| Hand (hold) | Space | Zoom in / out tool | T / W (Alt toggles) |
| Magnifier power −/+ | N / M | Zoom out / in / fit / 100% | Ctrl+- / Ctrl+= / Ctrl+0 / Alt+Ctrl+0 (also , . / *) |
| Fit width / height | Alt+Right / Alt+Down | Feature / Shape / Master / Track curve | Shift+Ctrl+F / S / M / T |
| Camera / sequence effect | Shift+Ctrl+C / E | Movie effects | Ctrl+E |
| Scale / rotate / skew selection | Ctrl+H / R / K | Add / duplicate sequence | Ctrl+Ins / Ctrl+J |
| First / last sequence | Ctrl+Home / End | Prev / next sequence | PgUp / PgDn |
| Layer to top / bottom | Shift+Ctrl+] / [ | Layer up / down | Ctrl+] / [ |
| Play/pause | Ctrl+P | First / last frame | Home / End |
| Prev / next frame | Ctrl+B / Ctrl+F | Nudge dots | Arrows (Ctrl = 10 px) |

---

## Beyond the core

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `cli.*` / `mcp.*` | Every command drivable headlessly from the CLI (`ib run`) and over MCP (`ib mcp`, one tool per command, `frame.preview` returns a picture) | P1 |
| [x] |  | Deterministic rendering: same project and frame always give the same bytes | P0 |
| [x] |  | GPU and CPU renderers cross-checked on every test scene | P1 |
| [x] | `dots.autoLandmarks` | Landmark-based auto-dots for non-face subjects | P2 |
