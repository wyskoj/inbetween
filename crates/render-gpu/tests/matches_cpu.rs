//! The GPU renderer must match the CPU oracle on every scene (tolerance: docs D24).
//!
//! No GPU is not a failure locally (the tests print SKIP). Set `INBETWEEN_REQUIRE_GPU=1`,
//! as CI does with a software Vulkan driver, to make a missing adapter an error.

use inbetween_doc::{DotPair, DotSet, FitMode, MeshPolicy, MorphType, Sequence};
use inbetween_geom::Point;
use inbetween_raster::Image;
use inbetween_render_cpu::Renderer;
use inbetween_render_gpu::{GpuContext, GpuRenderer};
use inbetween_testkit::{alpha_gradient, checkerboard, demo, gradient, labelled_grid};
use std::sync::OnceLock;

/// How closely the GPU must match the CPU oracle (docs/decisions.md, D24). Values are
/// premultiplied channels in 0..1, so 0.01 is about 2.5 of the 255 levels of an 8-bit image.
struct Tolerance {
    /// Largest allowed difference on any channel of any pixel.
    max: f32,
    /// A pixel differs "visibly" above this.
    visible: f32,
    /// Most pixels allowed to differ visibly, as a fraction of the frame (at least one pixel
    /// is always allowed on frames under 1,000 pixels).
    visible_fraction: f64,
    /// Mean absolute difference over every channel of every pixel.
    mean: f64,
}

/// Ordinary meshes. GPU rasterisers snap vertices to a sub-pixel grid, so sampling positions
/// differ by about 0.004 px; in practice the worst pixel is under 2 levels.
const STRICT: Tolerance = Tolerance {
    max: 0.01,
    visible: 0.01,
    visible_fraction: 0.0,
    mean: 5e-4,
};

/// Folded meshes and slivers, where overlapping triangles meet. A pixel centre almost on a
/// shared edge can be claimed by a different triangle on each renderer, so a handful of
/// isolated pixels may differ by more.
const LOOSE: Tolerance = Tolerance {
    max: 0.5,
    visible: 0.01,
    visible_fraction: 0.003,
    mean: 1e-3,
};

/// Meshes with the corner triangles dropped. A pixel centre exactly on the outline's edge can
/// be claimed by the CPU and not the GPU (or the reverse), which flips it between clear and
/// drawn, so a few edge pixels may differ completely. Everything else must still match.
const OUTLINE: Tolerance = Tolerance {
    max: 1.0,
    visible: 0.01,
    visible_fraction: 0.01,
    mean: 3e-3,
};

/// CI sets `INBETWEEN_REQUIRE_GPU=1` where a software driver is installed.
fn require_gpu() -> bool {
    matches!(std::env::var("INBETWEEN_REQUIRE_GPU"), Ok(v) if !v.is_empty() && v != "0")
}

fn context() -> Option<&'static GpuContext> {
    static CONTEXT: OnceLock<Option<GpuContext>> = OnceLock::new();
    let ctx = CONTEXT.get_or_init(|| match GpuContext::new() {
        Ok(ctx) => {
            println!("GPU: {} ({})", ctx.adapter_name(), ctx.backend());
            Some(ctx)
        }
        Err(e) => {
            assert!(
                !require_gpu(),
                "INBETWEEN_REQUIRE_GPU is set but there is no GPU: {e}"
            );
            println!("SKIP: {e}");
            None
        }
    });
    ctx.as_ref()
}

struct Diff {
    max: f32,
    mean: f64,
    visible: usize,
}

fn diff(cpu: &Image, gpu: &Image, visible_over: f32) -> Diff {
    assert_eq!(cpu.size(), gpu.size());
    let (mut max, mut visible, mut sum) = (0f32, 0usize, 0f64);
    for (a, b) in cpu
        .as_slice()
        .chunks_exact(4)
        .zip(gpu.as_slice().chunks_exact(4))
    {
        let d = a
            .iter()
            .zip(b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f32::max);
        max = max.max(d);
        sum += a
            .iter()
            .zip(b)
            .map(|(x, y)| f64::from((x - y).abs()))
            .sum::<f64>();
        if d > visible_over {
            visible += 1;
        }
    }
    let pixels = f64::from(cpu.width()) * f64::from(cpu.height());
    Diff {
        max,
        mean: sum / (pixels * 4.0),
        visible,
    }
}
type Xy = (f64, f64);

fn sequence(pairs: &[(Xy, Xy)], frames: u32) -> Sequence {
    let pairs = pairs
        .iter()
        .map(|&((a, b), (c, d))| DotPair::new(Point::new(a, b), Point::new(c, d)))
        .collect();
    Sequence {
        dots: DotSet::new(pairs),
        frames,
        ..Sequence::default()
    }
}

/// Render `frames` of `seq` on both renderers and require them to agree within `tol`.
fn check(
    name: &str,
    tol: &Tolerance,
    seq: &Sequence,
    output: (u32, u32),
    a: &Image,
    b: &Image,
    frames: &[u32],
) {
    let Some(ctx) = context() else { return };
    let cpu = Renderer::new(seq, output, a, Some(b)).expect("cpu renderer");
    let mut gpu = GpuRenderer::new(ctx, seq, output, a, Some(b)).expect("gpu renderer");
    let pixels = f64::from(output.0) * f64::from(output.1);
    let allowed = (tol.visible_fraction * pixels)
        .ceil()
        .max(if pixels < 1000.0 { 1.0 } else { 0.0 });
    for &i in frames {
        let want = cpu.render_frame(i).expect("cpu frame");
        let got = gpu.render_frame(i).expect("gpu frame");
        let d = diff(&want, &got, tol.visible);
        println!(
            "{name} frame {i}: max {:.5}, mean {:.7}, visible pixels {}",
            d.max, d.mean, d.visible
        );
        assert!(
            d.max <= tol.max,
            "{name} frame {i}: max difference {} > {}",
            d.max,
            tol.max
        );
        assert!(
            d.mean <= tol.mean,
            "{name} frame {i}: mean difference {} > {}",
            d.mean,
            tol.mean
        );
        assert!(
            (d.visible as f64) <= allowed,
            "{name} frame {i}: {} pixels differ visibly (allowed {allowed})",
            d.visible
        );
    }
}

fn compare(name: &str, seq: &Sequence, output: (u32, u32), a: &Image, b: &Image, frames: &[u32]) {
    check(name, &STRICT, seq, output, a, b, frames);
}

fn compare_loose(
    name: &str,
    seq: &Sequence,
    output: (u32, u32),
    a: &Image,
    b: &Image,
    frames: &[u32],
) {
    check(name, &LOOSE, seq, output, a, b, frames);
}
const ALL: &[u32] = &[0, 1, 4, 7, 10, 11];

fn moved() -> Vec<(Xy, Xy)> {
    vec![
        ((12.0, 8.0), (15.0, 10.0)),
        ((36.0, 8.0), (33.0, 11.0)),
        ((24.0, 16.0), (26.0, 14.0)),
        ((12.0, 24.0), (10.0, 22.0)),
        ((36.0, 24.0), (38.0, 26.0)),
    ]
}

#[test]
fn demo_pair() {
    let (w, h) = (480, 360);
    let (a, b) = demo::pair(w, h).unwrap();
    let pairs = demo::dot_pairs(w, h);
    compare("demo", &sequence(&pairs, 12), (w, h), &a, &b, ALL);
}

#[test]
fn gradient_and_grid_with_modest_moves() {
    let (a, b) = (gradient(48, 32).unwrap(), labelled_grid(48, 32, 8).unwrap());
    compare(
        "gradient/grid",
        &sequence(&moved(), 12),
        (48, 32),
        &a,
        &b,
        ALL,
    );
}

#[test]
fn alpha_images() {
    let a = alpha_gradient(48, 32).unwrap();
    let b = checkerboard(48, 32, 6, [200, 30, 30, 255], [30, 30, 200, 90]).unwrap();
    compare("alpha", &sequence(&moved(), 12), (48, 32), &a, &b, ALL);
}

#[test]
fn unpaired_dots_and_lines_change_nothing() {
    let (a, b) = (gradient(48, 32).unwrap(), labelled_grid(48, 32, 8).unwrap());
    let plain = sequence(&moved(), 12);
    let mut extra = plain.clone();
    extra.dots.pairs.push(DotPair::unpaired(
        Point::new(7.0, 9.0),
        inbetween_doc::Side::Only1,
    ));
    extra.dots.connect(0, extra.dots.len() - 1);
    for i in [0, 6, 11] {
        let x = Renderer::new(&plain, (48, 32), &a, Some(&b)).unwrap();
        let y = Renderer::new(&extra, (48, 32), &a, Some(&b)).unwrap();
        assert_eq!(x.render_frame(i).unwrap(), y.render_frame(i).unwrap());
    }
    compare("unpaired", &extra, (48, 32), &a, &b, &[0, 6, 11]);
}

#[test]
fn no_dots_is_a_cross_dissolve() {
    let (a, b) = (gradient(64, 40).unwrap(), labelled_grid(64, 40, 8).unwrap());
    compare("no dots", &sequence(&[], 12), (64, 40), &a, &b, ALL);
}

#[test]
fn warp_one_never_shows_image_two() {
    let (a, b) = (labelled_grid(48, 32, 8).unwrap(), gradient(48, 32).unwrap());
    let mut seq = sequence(&moved(), 12);
    seq.morph = MorphType::Warp1;
    compare("warp1", &seq, (48, 32), &a, &b, ALL);

    // Warp 1 also works with no Image 2 at all.
    let Some(ctx) = context() else { return };
    let cpu = Renderer::new(&seq, (48, 32), &a, None).unwrap();
    let mut gpu = GpuRenderer::new(ctx, &seq, (48, 32), &a, None).unwrap();
    for i in [0, 5, 11] {
        let d = diff(
            &cpu.render_frame(i).unwrap(),
            &gpu.render_frame(i).unwrap(),
            STRICT.visible,
        );
        assert!(d.max <= STRICT.max && d.visible == 0, "frame {i}");
    }
}

#[test]
fn every_morph_type_matches_with_and_without_hold_frames() {
    // Translucent sources, so the over-blend of Morph Hold and the dimming of the fades show.
    let a = alpha_gradient(48, 32).unwrap();
    let b = checkerboard(48, 32, 6, [200, 30, 30, 255], [30, 30, 200, 0]).unwrap();
    for morph in MorphType::ALL {
        for (hold_start, hold_end) in [(0, 0), (2, 3)] {
            let mut seq = sequence(&moved(), 8);
            seq.morph = morph;
            seq.hold_start = hold_start;
            seq.hold_end = hold_end;
            let last = seq.total_frames() - 1;
            let name = format!("{} hold {hold_start}/{hold_end}", morph.id());
            compare(
                &name,
                &seq,
                (48, 32),
                &a,
                &b,
                &[0, 1, 3, 4, 6, last - 1, last],
            );
        }
    }
}

#[test]
fn hold_frames_count_and_repeat_the_end_frames() {
    let Some(ctx) = context() else { return };
    let (a, b) = (gradient(48, 32).unwrap(), labelled_grid(48, 32, 8).unwrap());
    let mut seq = sequence(&moved(), 5);
    seq.hold_start = 2;
    seq.hold_end = 3;
    let mut gpu = GpuRenderer::new(ctx, &seq, (48, 32), &a, Some(&b)).unwrap();
    assert_eq!(gpu.frame_count(), 10);
    let first = gpu.render_frame(0).unwrap();
    assert_eq!(diff(&first, &gpu.render_frame(2).unwrap(), 0.0).max, 0.0);
    let last = gpu.render_frame(9).unwrap();
    assert_eq!(diff(&last, &gpu.render_frame(6).unwrap(), 0.0).max, 0.0);
    // set_sequence picks up a new hold length without a rebuild.
    seq.hold_end = 0;
    gpu.set_sequence(&seq).unwrap();
    assert_eq!(gpu.frame_count(), 7);
}

#[test]
fn per_frame_mesh() {
    let (a, b) = (gradient(48, 32).unwrap(), labelled_grid(48, 32, 8).unwrap());
    let mut seq = sequence(&moved(), 12);
    seq.mesh = MeshPolicy::PerFrame;
    compare("per-frame mesh", &seq, (48, 32), &a, &b, ALL);
}

#[test]
fn dropped_corner_triangles() {
    let (a, b) = (gradient(48, 32).unwrap(), labelled_grid(48, 32, 8).unwrap());
    for policy in [MeshPolicy::Frame0, MeshPolicy::PerFrame] {
        let mut seq = sequence(&moved(), 12);
        seq.mesh = policy;
        seq.drop_corner_tris = true;
        let name = format!("drop corners {policy:?}");
        check(&name, &OUTLINE, &seq, (48, 32), &a, &b, ALL);
    }
    let mut seq = sequence(&[((20.0, 10.0), (30.0, 20.0))], 12);
    seq.drop_corner_tris = true;
    compare("drop corners, one dot", &seq, (48, 32), &a, &b, ALL);
}

#[test]
fn different_sized_sources_in_every_fit_mode() {
    let a = gradient(96, 48).unwrap(); // wider than the output
    let b = labelled_grid(40, 64, 8).unwrap(); // taller than the output
    for fit in [FitMode::Stretch, FitMode::Contain, FitMode::Cover] {
        let mut seq = sequence(&moved(), 12);
        seq.fit = fit;
        compare(&format!("fit {fit:?}"), &seq, (48, 32), &a, &b, ALL);
    }
}

#[test]
fn folded_mesh_where_dots_swap_sides() {
    let dots = [
        ((8.0, 8.0), (40.0, 24.0)),
        ((40.0, 8.0), (8.0, 24.0)),
        ((8.0, 24.0), (40.0, 8.0)),
        ((40.0, 24.0), (8.0, 8.0)),
        ((24.0, 16.0), (24.0, 16.0)),
    ];
    let (a, b) = (labelled_grid(48, 32, 8).unwrap(), gradient(48, 32).unwrap());
    // Overlapping triangles resolve by draw order on both renderers; edge pixels may differ.
    compare_loose("folded", &sequence(&dots, 12), (48, 32), &a, &b, ALL);
}

#[test]
fn degenerate_and_extreme_dot_sets() {
    let (a, b) = (labelled_grid(48, 32, 8).unwrap(), gradient(48, 32).unwrap());
    let cases: Vec<(&str, Vec<(Xy, Xy)>)> = vec![
        ("one dot", vec![((20.0, 10.0), (30.0, 20.0))]),
        (
            "collinear",
            vec![
                ((10.0, 16.0), (24.0, 5.0)),
                ((20.0, 16.0), (24.0, 15.0)),
                ((30.0, 16.0), (24.0, 25.0)),
            ],
        ),
        (
            "collinear then triangle",
            vec![
                ((10.0, 16.0), (10.0, 5.0)),
                ((20.0, 16.0), (30.0, 25.0)),
                ((30.0, 16.0), (40.0, 5.0)),
            ],
        ),
        (
            "outside the frame",
            vec![
                ((-40.0, -40.0), (-30.0, -35.0)),
                ((90.0, 70.0), (80.0, 60.0)),
            ],
        ),
        (
            "on the corners",
            vec![((0.0, 0.0), (5.0, 5.0)), ((48.0, 32.0), (40.0, 28.0))],
        ),
    ];
    for (name, dots) in cases {
        compare_loose(name, &sequence(&dots, 12), (48, 32), &a, &b, ALL);
    }
}

#[test]
fn larger_frame_with_many_dots() {
    // 1,000 dots on a 640x360 frame: many small triangles and many shared edges.
    let (w, h) = (640u32, 360u32);
    let mut pairs = Vec::new();
    let mut state = 0x1234_5678_9ABC_DEF1u64;
    let mut unit = move || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    };
    let (cols, rows) = (40u32, 25u32);
    for r in 0..rows {
        for c in 0..cols {
            let x = (f64::from(c) + 0.5 + (unit() - 0.5) * 0.5) * f64::from(w) / f64::from(cols);
            let y = (f64::from(r) + 0.5 + (unit() - 0.5) * 0.5) * f64::from(h) / f64::from(rows);
            let (dx, dy) = ((y / 40.0).sin() * 3.0, (x / 50.0).cos() * 3.0);
            pairs.push(((x, y), (x + dx, y + dy)));
        }
    }
    let (a, b) = (gradient(w, h).unwrap(), labelled_grid(w, h, 20).unwrap());
    compare(
        "1000 dots",
        &sequence(&pairs, 12),
        (w, h),
        &a,
        &b,
        &[0, 5, 11],
    );
}

#[test]
fn renderer_can_be_reused_across_frames() {
    let Some(ctx) = context() else { return };
    let (a, b) = (gradient(48, 32).unwrap(), labelled_grid(48, 32, 8).unwrap());
    let seq = sequence(&moved(), 12);
    let cpu = Renderer::new(&seq, (48, 32), &a, Some(&b)).unwrap();
    let mut gpu = GpuRenderer::new(ctx, &seq, (48, 32), &a, Some(&b)).unwrap();
    // Out of order and repeated, including the clamped last frame.
    for i in [11, 0, 5, 5, 3, u32::MAX, 0] {
        let d = diff(
            &cpu.render_frame(i).unwrap(),
            &gpu.render_frame(i).unwrap(),
            STRICT.visible,
        );
        assert!(d.max <= STRICT.max, "frame {i}");
    }
    assert_eq!(gpu.frame_count(), 12);
    assert_eq!(gpu.size(), (48, 32));
}

#[test]
fn output_is_valid() {
    let Some(ctx) = context() else { return };
    let a = alpha_gradient(48, 32).unwrap();
    let b = checkerboard(48, 32, 5, [255, 0, 0, 255], [0, 0, 255, 40]).unwrap();
    let seq = sequence(
        &[((5.0, 5.0), (60.0, 60.0)), ((-30.0, 90.0), (20.0, 10.0))],
        8,
    );
    let mut gpu = GpuRenderer::new(ctx, &seq, (48, 32), &a, Some(&b)).unwrap();
    for i in 0..8 {
        let img = gpu.render_frame(i).unwrap();
        for px in img.as_slice().chunks_exact(4) {
            assert!(px.iter().all(|v| v.is_finite()), "non-finite {px:?}");
            assert!((0.0..=1.0).contains(&px[3]), "alpha {px:?}");
            assert!(
                px[..3].iter().all(|c| *c >= 0.0 && *c <= px[3] + 1e-6),
                "colour {px:?}"
            );
        }
    }
}

#[test]
fn invalid_input_is_an_error_not_a_crash() {
    let Some(ctx) = context() else { return };
    let (a, b) = (gradient(16, 16).unwrap(), gradient(16, 16).unwrap());
    let seq = sequence(&[], 4);
    assert!(GpuRenderer::new(ctx, &seq, (0, 10), &a, Some(&b)).is_err());
    assert!(GpuRenderer::new(ctx, &seq, (u32::MAX, u32::MAX), &a, Some(&b)).is_err());
    assert!(GpuRenderer::new(ctx, &seq, (16, 16), &a, None).is_err()); // Morph needs Image 2
    let dup = sequence(&[((5.0, 5.0), (6.0, 6.0)), ((5.0, 5.0), (9.0, 9.0))], 4);
    assert!(GpuRenderer::new(ctx, &dup, (16, 16), &a, Some(&b)).is_err());
    let nan = sequence(&[((f64::NAN, 5.0), (6.0, 6.0))], 4);
    assert!(GpuRenderer::new(ctx, &nan, (16, 16), &a, Some(&b)).is_err());
    let zero = Sequence {
        frames: 0,
        ..Sequence::default()
    };
    assert!(GpuRenderer::new(ctx, &zero, (16, 16), &a, Some(&b)).is_err());
}

#[test]
fn the_1080p_perf_scene_matches() {
    // The scene `cargo xtask perf` times: 1,000 dots on a 1920x1080 frame. Dots move far
    // enough that the mesh overlaps locally, so use the loose tolerance.
    use inbetween_testkit::perf;
    let pairs = perf::dot_pairs();
    let (a, b) = (
        gradient(perf::WIDTH, perf::HEIGHT).unwrap(),
        labelled_grid(perf::WIDTH, perf::HEIGHT, 60).unwrap(),
    );
    compare_loose(
        "perf scene",
        &sequence(&pairs, 30),
        (perf::WIDTH, perf::HEIGHT),
        &a,
        &b,
        &[0, 15, 29],
    );
}

#[test]
fn set_sequence_matches_a_fresh_renderer_without_re_uploading() {
    let Some(ctx) = context() else { return };
    let (a, b) = (gradient(48, 32).unwrap(), labelled_grid(48, 32, 8).unwrap());
    let before = sequence(&moved(), 12);
    let mut after = sequence(&moved(), 20);
    after
        .dots
        .pairs
        .push(DotPair::new(Point::new(30.0, 30.0), Point::new(31.0, 29.0)));
    after.dots.pairs[0].second = Point::new(20.0, 12.0);

    let mut reused = GpuRenderer::new(ctx, &before, (48, 32), &a, Some(&b)).unwrap();
    let _ = reused.render_frame(5).unwrap();
    reused.set_sequence(&after).unwrap();
    assert_eq!(reused.frame_count(), 20);
    let mut fresh = GpuRenderer::new(ctx, &after, (48, 32), &a, Some(&b)).unwrap();
    for i in [0, 7, 19] {
        let (x, y) = (
            reused.render_frame(i).unwrap(),
            fresh.render_frame(i).unwrap(),
        );
        assert_eq!(x, y, "frame {i}");
    }

    // A sequence the planner refuses leaves the renderer exactly as it was.
    let mut bad = after.clone();
    bad.dots
        .pairs
        .push(DotPair::new(Point::new(30.0, 30.0), Point::new(1.0, 1.0))); // duplicate
    assert!(reused.set_sequence(&bad).is_err());
    assert_eq!(reused.frame_count(), 20);
    assert_eq!(
        reused.render_frame(7).unwrap(),
        fresh.render_frame(7).unwrap()
    );

    // Fit modes can change in place, too.
    let mut contain = after.clone();
    contain.fit = FitMode::Contain;
    reused.set_sequence(&contain).unwrap();
    let cpu = Renderer::new(&contain, (48, 32), &a, Some(&b)).unwrap();
    let d = diff(
        &cpu.render_frame(7).unwrap(),
        &reused.render_frame(7).unwrap(),
        STRICT.visible,
    );
    assert!(d.max <= STRICT.max, "{}", d.max);
}

#[test]
fn preview_target_can_be_drawn_and_waited_on() {
    let Some(ctx) = context() else { return };
    let (a, b) = (gradient(64, 48).unwrap(), labelled_grid(64, 48, 8).unwrap());
    let seq = sequence(&moved(), 12);
    let mut gpu = GpuRenderer::new(ctx, &seq, (64, 48), &a, Some(&b)).unwrap();
    for t in [0.0, 0.5, 1.0] {
        gpu.draw_preview(t).unwrap();
    }
    gpu.wait();
    let tex = gpu.preview_texture();
    assert_eq!((tex.width(), tex.height()), (64, 48));
    assert_eq!(tex.format(), wgpu::TextureFormat::Rgba8Unorm);
}

#[test]
fn edited_sources_match() {
    use inbetween_doc::{AdjustSettings, Crop, Filter, ImageEdit};
    use inbetween_render_cpu::source::apply_edit;
    // Turn + crop + angle + adjust + gaussian on both images, then morph the results.
    let (a, b) = (gradient(48, 32).unwrap(), labelled_grid(48, 32, 8).unwrap());
    let edit = ImageEdit {
        quarter_turns: 1,
        flip_h: true,
        crop: Some(Crop {
            x: 2.0,
            y: 3.0,
            w: 26.0,
            h: 40.0,
        }),
        angle: 12.0,
        adjust: AdjustSettings {
            brightness: 15.0,
            saturation: -20.0,
            ..AdjustSettings::default()
        },
        filters: vec![Filter::Gaussian { sigma: 1.5 }],
        ..ImageEdit::default()
    };
    let a2 = apply_edit(&a, &edit, None).unwrap().unwrap();
    let b2 = apply_edit(&b, &edit, None).unwrap().unwrap();
    assert_eq!(a2.size(), (26, 40));
    let pairs: Vec<(Xy, Xy)> = vec![
        ((6.0, 6.0), (8.0, 7.0)),
        ((20.0, 6.0), (18.0, 9.0)),
        ((13.0, 20.0), (15.0, 18.0)),
        ((6.0, 33.0), (5.0, 31.0)),
        ((20.0, 33.0), (21.0, 35.0)),
    ];
    compare("edited", &sequence(&pairs, 12), (26, 40), &a2, &b2, ALL);
}
