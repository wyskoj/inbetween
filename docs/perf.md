# Performance

`cargo xtask perf` times the CPU renderer (the reference oracle), then the GPU renderer if an adapter exists, in release builds:

- Scene: one 1920x1080 frame, 1,000 dot pairs on a jittered 40x25 grid (2,002 mesh triangles), Image 1 a gradient, Image 2 a labelled grid, Morph type, shared mesh, frame spread across a 30-frame sequence.
- Default is 4 worker threads, the reference machine for the budget; `--threads N` overrides and `--quick` uses fewer frames.
- Reports setup (two triangulations), plan time, and the median/min/max render time over 21 frames, plus a checksum of frame 15 so an optimisation can prove it changed no pixels.
- **Budget: 100 ms per frame at 4 threads.** The command fails when the median is over budget (`INBETWEEN_PERF_BUDGET_MS` overrides it). Tighten it as the renderer improves.

## Baselines

Machine: Intel Core Ultra 9 275HX (24 cores), 31 GB RAM, Windows 11, Rust 1.99.

| Date | Change | 1 thread | 2 | 4 | 8 | 24 | Checksum (frame 15) |
|---|---|---|---|---|---|---|---|
| 2026-10-18 | M3 renderer: per-triangle buffers merged in order | 127.7 ms | | 52.4 ms | | 36.3 ms | `83f960a821976570` |
| 2026-10-18 | Row bands with a per-band owner map (no serial merge) | 103.4 ms | 52.7 ms | 28.4 ms | 16.2 ms | 9.9 ms | `83f960a821976570` |

Setup (two Delaunay triangulations of 1,000 dots) takes about 0.8 ms and planning a frame about 0.15 ms, so rendering is essentially all of the cost. The identical checksum across both renderers and every thread count shows the optimisation changed no output and that results do not depend on the thread count.

The old renderer scaled poorly (4 to 24 threads gained only 1.4x) because per-triangle results were merged serially. The band renderer scales almost linearly up to 24 threads.

## Where the time goes next

Roughly 50 ns per pixel on one thread: two bilinear samples (four fetches each) plus an f64 inverse-affine map and an edge test per pixel. The obvious next steps, when the budget needs tightening: an f32 incremental affine instead of recomputing it per pixel, skipping the second sample when the dissolve weight is 0 or 1, and sampling the source in whole-row runs where the map is axis-aligned. The GPU renderer (M7) is the real answer for interactive speed.

## GPU renderer (M7)

Same scene, `cargo xtask perf` second half (`crates/render-gpu/examples/perf_gpu.rs`). **Budget: 16.7 ms per frame** (60 fps real-time preview), measured as plan + vertex upload + draw + wait, with no readback. The harness also times a draw plus copy-back into a CPU `Image` (what an export pays) and prints how the frame compares with the CPU oracle.

Machine: NVIDIA GeForce RTX 5060 Laptop GPU (Vulkan), Windows 11.

| Date | Metric | Result |
|---|---|---|
| 2026-10-18 | Setup (pipeline, two 1080p source uploads, targets) | about 10 ms |
| 2026-10-18 | Draw, median (min to max) | **0.70 ms** (0.41 to 0.87) |
| 2026-10-18 | Draw plus readback to a CPU image | about 10 ms |
| 2026-10-18 | Versus CPU on frame 15 | max difference 0.278 on 1 pixel of 2,073,600; mean 2.6e-5 |

That is about 40 times faster than the CPU renderer at 4 threads (28 ms) before readback, with room for roughly 1,400 frames per second against the 60 fps budget. After readback the GPU is still about 3 times faster than the CPU at 4 threads, and the copy of 33 MB of f32 pixels dominates the 10 ms. A live preview should sample `GpuRenderer::texture()` directly and skip the readback. Not measured: a low-end integrated GPU, or lavapipe (software), where the numbers will be much worse.
## Source image operations (M16)

`crates/raster/examples/filters.rs`: every pixel operation on a 1920x1080 image, mean of 5 runs after one warm-up, `RAYON_NUM_THREADS=4`.

| Date | Operation | Time |
|---|---|---|
| 2026-10-19 | rotate 90, flip | 6.6 ms, 5.8 ms |
| 2026-10-19 | resample (rotated affine) | 11.5 ms |
| 2026-10-19 | adjust (brightness, contrast, hue, saturation) | 27 ms |
| 2026-10-19 | smooth, box r=10, gaussian s=10, sharpen | 55 ms, 34 ms, 124 ms, 174 ms |
| 2026-10-19 | zoom blur, radial blur | 514 ms, 1135 ms |
| 2026-10-19 | emboss, mosaic 16, oil r=4, splash 8 | 53 ms, 9 ms, 218 ms, 18 ms |
| 2026-10-19 | cylinder, whirl, pinch | 47 ms, 29 ms, 43 ms |

The adjust (15 ms) and gaussian (60 ms) targets are not met yet; both are memory-bound on a 33 MB image (a plain flip costs 6 ms). Edits are processed once per source and cached by the session, so this cost is paid on edit, not per frame. Zoom and radial blur take 24 bilinear taps per pixel and are the slowest; they are the first candidates if interactive dragging feels slow.
