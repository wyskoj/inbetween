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
| [~] | `engine.triangulate` | Triangulate paired dots, auto-adding the 4 image corners | P0 |
| [~] | `engine.warp` | Per-triangle affine warp of each source toward the interpolated dot positions | P0 |
| [~] | `engine.dissolve` | Cross-dissolve the two warped images by the feature amount | P0 |
| [~] | `engine.alpha` | Full alpha pipeline: 32-bit RGBA sources in, RGBA frames out | P0 |
| [ ] | `morph.option.dropCornerTris` | Option: discard triangles touching the 4 corners (backdrop effect on non-alpha images) | P1 |
| [~] | `morph.option.retriangulate` | Option: re-triangulate every intermediate frame instead of only frame 0 | P1 |
| [~] | `engine.outsideDots` | Dots may sit outside the image bounds | P1 |

### Morph types (per sequence)

| | Id | Behaviour | Pri |
|---|---|---|---|
| [~] | `morph.type.morph` | Image 1 morphs into Image 2 (default) | P0 |
| [ ] | `morph.type.morphHold` | Morph while Image 1 never fades (partial morph using an alpha Image 2) | P1 |
| [~] | `morph.type.warp1` | Image 1 deforms into Image 2's shape; Image 2's colours never appear | P0 |
| [ ] | `morph.type.warp2` | Image 2 deforms from Image 1's shape; Image 1's colours never appear | P1 |
| [ ] | `morph.type.fadeOut1` | Image 1 fades to black | P1 |
| [ ] | `morph.type.fadeIn2` | Image 2 fades in from black | P1 |
| [ ] | `morph.type.direct` | No morph; Image 2 cuts in at the end (slideshow when paired with a Transition effect) | P1 |

**Oracle tests:** frame 0 equals Image 1; last frame equals Image 2; identical dot sets reduce to a pure cross-dissolve; Warp 1 with identical images and unmoved dots is the identity.

---

## 1. Projects and files

| | Id | Behaviour | Pri |
|---|---|---|---|
| [~] | `file.new` / `project.open` / `project.save` | `.inbetween` project file | P0 |
| [ ] | `file.openVisual` | Open dialog that shows project thumbnails | P2 |
| [ ] | `file.appendProject` | Merge another project's sequences into this one | P1 |
| [ ] | `file.wizard` | New-project wizard: pick type, pick an ordered image list, build sequences | P1 |
| [x] | `dots.export` / `dots.import` | Export/import a dot set on its own (`.inbetweendots`) | P1 |
| [ ] | `frame.exportEdit` | Export a single frame together with its dot set | P2 |
| [ ] | `file.dragDrop` | Drop images onto either source panel or effect slots | P1 |

**Image import:** BMP, JPEG, TIFF, PNG, PSD (flattened), TGA, GIF; keep alpha for BMP/TIFF/PNG/PSD/TGA. P0 for PNG/JPEG, P1 for the rest. *Status: PNG and JPEG codecs done (`codecs`); the import command arrives with the engine.*

---

## 2. Source image editing (per image)

| | Id | Behaviour | Pri |
|---|---|---|---|
| [ ] | `image.crop` | Drag crop; free or locked aspect; crop may extend past the image; arbitrary-angle rotate inside crop | P1 |
| [ ] | `image.rotate` | 90° CW, 90° CCW, 180°, arbitrary angle | P1 |
| [ ] | `image.flip` | Horizontal, vertical | P1 |
| [ ] | `image.adjust` | Brightness, contrast, R/G/B, hue, saturation, negative, grayscale | P1 |
| [ ] | `image.filter.*` | Smooth, blur, Gaussian, zoom blur, radial blur, sharpen, emboss, mosaic, oil paint, splash, cylinder, whirl, pinch | P2 |
| [ ] | `image.caption` | Per-image caption (see §8) | P2 |
| [ ] | `image.swap` | Swap Image 1 and Image 2 (F3) | P1 |
| [ ] | `image.saveToLibrary` | Store image plus dots in the face library (§10) | P2 |

All source edits are undoable and non-destructive (dots stay attached to the edited image).

---

## 3. Key dots

### Editing

| | Id | Behaviour | Pri |
|---|---|---|---|
| [ ] | `dots.add` | Click to add; partner dot auto-created on the other image | P0 |
| [ ] | `dots.option.autoPartner` | Toggle auto-creating the partner dot; when off, dots are added per image and shown as unpaired | P1 |
| [ ] | `dots.addSamePos` | Modifier (Alt) puts the partner at identical coordinates (boundary/anchor dots for warps) | P1 |
| [ ] | `dots.option.autoJump` | Toggle moving the cursor to the partner position after adding | P2 |
| [ ] | `dots.move` | Drag a dot; preview updates live | P0 |
| [ ] | `dots.delete` | Delete tool, or right-click any dot with any tool | P0 |
| [ ] | `dots.select` | Marquee select; Shift adds, Alt subtracts; clicking a line selects its connected dots | P1 |
| [ ] | `dots.transform` | Move, scale, rotate, skew, flip a selection | P1 |
| [ ] | `dots.nudge` | Arrow keys move selection 1 px; Ctrl+arrow 10 px | P1 |
| [ ] | `edit.cut/copy/paste/delete` | Clipboard ops on dots, keeping connected lines | P1 |
| [ ] | `dots.selectAll` / `dots.clearAll` | | P1 |
| [ ] | `dots.matching` | Paired vs unpaired dots shown in different colours; partner blinks when hovering an unpaired dot | P1 |
| [ ] | `dots.autoMatch` | Generate partner dots at plausible positions automatically | P2 |
| [ ] |  | Limit: at least 2048 dot pairs | P0 |

### Lines (organizational only; never affect the morph)

| | Id | Behaviour | Pri |
|---|---|---|---|
| [ ] | `lines.addPolyline` | Click repeatedly to drop dots joined by segments; one modifier finishes with a final dot, another (or right-click) ends without one | P1 |
| [ ] | `lines.connect` | Connect any two existing dots | P1 |
| [ ] | `lines.insertDot` | Clicking on a segment inserts a dot into it | P1 |
| [ ] | `lines.delete` | Remove a segment, keep its dots; deleting a dot removes its segments | P1 |
| [ ] | `lines.drag` | Drag a segment to move it with all connected dots and lines | P1 |

### Display

| | Id | Behaviour | Pri |
|---|---|---|---|
| [ ] | `view.toggle.dots/lines/triangles/track/grid` | Show/hide dots, lines, triangle mesh, morph track, grid | P1 |
| [ ] | `prefs.dotStyle` | Several dot styles plus highlighted variants | P2 |
| [ ] | `prefs.dotColor` | Single colour, or cycle 8 colours per pair; dot opacity 64–255; antialiased | P2 |
| [ ] |  | Hover highlights a dot pair; hovering a line highlights everything connected | P1 |
| [ ] | `view.magnifier` | Loupe around the cursor, 1×–4× | P2 |

---

## 4. Sequences and project types

| | Id | Behaviour | Pri |
|---|---|---|---|
| [~] | `sequence.length` | 2–999 morph frames, plus 0–999 hold frames on each end, set separately; enter as frames or seconds; optional apply-to-all | P0 |
| [ ] | `sequence.add` | Add a continued sequence (previous Image 2 becomes new Image 1) | P1 |
| [ ] | `sequence.addBlank` | Add an empty sequence | P1 |
| [ ] | `sequence.delete/duplicate/rename` | | P1 |
| [ ] | `sequence.visibility` | Hide/show; modifier-click toggles solo | P1 |
| [ ] | `sequence.reorder` | Up, down, to top, to bottom, drag in list | P1 |
| [ ] | `sequence.link` | Make Image 1 equal the previous sequence's Image 2 (seamless join) | P1 |
| [ ] | `sequence.copyFrame` | Copy a source image with its dots into another sequence (drag between image icons) | P1 |
| [ ] | `project.type.sequence` | Sequences play one after another; movie length is the sum | P1 |
| [ ] | `project.type.layer` | All sequences play at once as stacked layers (first = top); all share one length; switchable at any time | P2 |
| [ ] | `view.sequenceList` | Panel listing sequences with both thumbnails, name, visibility (F2) | P1 |

---

## 5. Curves

All curves are smooth splines through ≥2 control points, with per-frame tick marks, live-editable during playback, and can be applied to all sequences. A **curve library** saves and loads named curves.

| | Id | Behaviour | Pri |
|---|---|---|---|
| [ ] | `curve.feature` | Remaps the colour-dissolve amount over time | P1 |
| [ ] | `curve.shape` | Remaps the shape-interpolation amount over time | P1 |
| [ ] | `curve.master` | Controls the sequence's overall opacity (fade in/out) | P2 |
| [ ] | `curve.track` | Dots travel along a 2D curve instead of a straight line; adjustable tension 0–20; flip H/V; endpoints movable (enables caricature/anti-face extrapolation) | P2 |
| [ ] | `curve.edit` | Click to add point, drag to move, right-click to delete, linear toggle, reset | P1 |
| [ ] | `curve.library` | Save, load, delete library curves | P2 |

---

## 6. Camera (per sequence, and per effect)

| | Id | Behaviour | Pri |
|---|---|---|---|
| [ ] | `camera.keyframes` | Keyframes of offset/scale/rotation along a timeline; frames between are interpolated smoothly | P2 |
| [ ] | `camera.handles` | On-canvas frame: drag to move, square handles to scale, circle handle to rotate; numeric entry; negative scale flips | P2 |
| [ ] | `camera.aspect` | Lock keyframe proportion to movie, Image 1 or Image 2, or free | P2 |
| [ ] | `camera.curve` | Optional easing curve for the camera path | P2 |
| [ ] | `camera.copyPaste/reset/applyAll/load/save` | | P2 |
| [ ] | `effect.camera` | Same camera controls drive an individual effect layer (animates a still effect) | P2 |

---

## 7. Effects

Movie-level effects cover the whole frame; sequence-level effects apply only inside that sequence's camera zone. Each effect has an on/off switch, a thumbnail slot, and a clear action.

| | Id | Behaviour | Pri |
|---|---|---|---|
| [ ] | `effect.backColor` | Solid colour behind the morph; can tint a background image | P1 |
| [ ] | `effect.background` | Image behind the morph, opacity 0–100 (visible through alpha or a mask) | P1 |
| [ ] | `effect.mask` | Grayscale image controls morph opacity; intensity −100..100; invert option | P1 |
| [ ] | `effect.foreground` | RGBA overlay (frames, graphics), opacity 0–100 | P1 |
| [ ] | `effect.transition` | Grayscale image controls *when* each pixel dissolves (dark first); border softness 0–100; invert | P2 |
| [ ] | `effect.light` | Grayscale image brightens/darkens the morph; separate bright and dark intensity | P2 |
| [ ] | `effect.trail` | Motion blur (overlay on) or motion trail (overlay off); tint colour; intensity | P2 |
| [ ] | `effect.radial` | Radiating rays over or under the morph; tint colour; intensity | P2 |
| [ ] | `effect.sound` | WAV/MP3 track starting at frame 0; cut at movie end, loop, fade in/out lengths | P2 |
| [ ] | `movie.adjust` / `movie.filter.*` | Same adjustments and filters as §2, applied to the output (filters also per sequence) | P2 |

---

## 8. Captions

| | Id | Behaviour | Pri |
|---|---|---|---|
| [ ] | `caption.*` | Captions at three scopes: per source image, per sequence, whole movie | P2 |
| [ ] |  | Multi-line text; font, size, style, colour, border size and colour, alignment | P2 |
| [ ] |  | Shadow with angle, distance, colour, size (size 0 = none) | P2 |
| [ ] |  | Position by drag or by normalized numeric coordinates | P2 |

---

## 9. Movie, preview and playback

| | Id | Behaviour | Pri |
|---|---|---|---|
| [ ] | `movie.size` | Match source size, presets, or custom | P0 |
| [ ] | `preview.scrub` | Scrub 0–100% in 1% steps; mouse wheel steps one frame; preview one sequence or all | P0 |
| [ ] | `play.*` | Play/pause, first, last, previous, next frame, loop, ping-pong; 1–60 fps | P1 |
| [ ] |  | Live preview updates while dragging dots, curves, camera, effects | P0 |
| [ ] |  | Checkerboard behind transparent preview areas | P1 |

---

## 10. Face tools (P2 block)

| | Id | Behaviour | Pri |
|---|---|---|---|
| [ ] | `face.extract` | Auto-detect the face in every source, crop to a shared template for consistent framing; adjustable template box; manual fallback; apply to all or one | P2 |
| [ ] | `face.locate` | Auto-place Inbetween's standard face dot set; apply/append to all or one; multiple faces per image by appending | P2 |
| [ ] | `face.locate.smartDrag` | "Intelligent" drag: moving one landmark re-fits the rest; modifier toggles plain drag | P2 |
| [ ] | `face.model` | Edit which landmark dots are used, highlight important ones, edit guide lines; presets; save/load | P2 |
| [ ] | `face.mix` | Blend N faces into one still; per-image feature % and shape % with locks, shape-follows-feature lock, average, random, candidate gallery, save/load | P2 |
| [ ] | `face.library` | Folder-organized store of images with dots; drag into a project to reuse | P2 |

Built on open landmark models (e.g. MediaPipe Face Mesh, dlib). Inbetween's face dot model is its own design.

---

## 11. Export

| | Id | Behaviour | Pri |
|---|---|---|---|
| [x] | `export.frame` | Current frame to an image file | P0 |
| [x] | `export.sequence` | Image sequence (PNG with alpha first) | P0 |
| [ ] | `export.range` | All sequences, current one, or a range | P1 |
| [ ] | `export.video` | Modern targets: MP4/H.264, WebM (with alpha), ProRes 4444 optional | P1 |
| [ ] | `export.gif` | Animated GIF, loop or once, opaque or transparent (also APNG/animated WebP) | P1 |
| [ ] | `export.options` | fps 1–60 (plus 29.97), JPEG quality, DPI, reverse frames, drop duplicate frames at sequence joins, cancellable | P1 |
| [ ] | `export.videoSource` | When a source is a video, include its clip before/after the morph | P2 |
| [ ] | `export.afterSave/afterOpen` | Auto-save project before export; open the result afterwards | P2 |
| [ ] |  | Output size not capped by the GPU (tile or fall back to CPU) | P1 |

**Video sources (P2):** import a video as Image 1 or 2; morph from the last frame of clip 1 to the first frame of clip 2.

---

## 12. Workspace and UI

| | Id | Behaviour | Pri |
|---|---|---|---|
| [ ] | `view.mode.full/edit/preview/frames/dual/fullscreen` | Six layouts (F5–F9, F11); dual-monitor split of editor and preview | P1 |
| [ ] | `view.split` | Resizable split between Image 1, Image 2, Preview | P1 |
| [ ] | `view.zoom` | 12.5%–800% via tools, up to 1600% via wheel; best fit, actual size, fit width/height | P1 |
| [ ] | `view.syncSources` / `view.syncPreview` | Zoom/pan both sources together, and the preview with them | P1 |
| [ ] | `view.frameStrip` | Frame Sequence mode: thumbnail grid of every frame; thumb size adjustable | P1 |
| [ ] | `edit.undo/redo` | Unlimited, covering images, dots, sequences, curves | P0 |
| [ ] | `export.pdf` | Contact sheet of sources, preview or all frames, with captions | P2 |
| [ ] | `tools.benchmark` | Measure preview and export throughput | P2 |
| [ ] | `tools.sequencePacker` | Pack an arbitrary image list into a movie | P2 |

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
| [ ] | `cli.*` / `mcp.*` | Every command drivable headlessly from the CLI and over MCP | P1 |
| [ ] |  | Deterministic rendering: same project and frame always give the same bytes | P0 |
| [ ] |  | GPU and CPU renderers cross-checked on every test scene | P1 |
| [ ] | `dots.autoLandmarks` | Landmark-based auto-dots for non-face subjects | P2 |
