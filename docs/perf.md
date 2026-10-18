# Performance

`cargo xtask perf` times the CPU renderer (the reference oracle) in a release build:

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
