//! The morph invariants from AGENTS.md section 3, plus behaviour checks for fit modes,
//! errors and determinism.

use inbetween_doc::{DotPair, DotSet, FitMode, MeshPolicy, MorphType, Sequence};
use inbetween_geom::Point;
use inbetween_morph::MorphError;
use inbetween_raster::Image;
use inbetween_render_cpu::{RenderError, Renderer};
use inbetween_testkit::{alpha_gradient, checkerboard, gradient, labelled_grid};
use proptest::prelude::*;

const W: u32 = 48;
const H: u32 = 32;

fn pt(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn sequence(pairs: &[(Point, Point)], morph: MorphType, frames: u32) -> Sequence {
    Sequence {
        dots: DotSet::new(pairs.iter().map(|&(a, b)| DotPair::new(a, b)).collect()),
        morph,
        frames,
        ..Sequence::default()
    }
}

fn renderer<'a>(seq: &Sequence, a: &'a Image, b: &'a Image) -> Renderer<'a> {
    Renderer::new(seq, (W, H), a, Some(b)).unwrap()
}

fn max_diff(a: &Image, b: &Image) -> f32 {
    assert_eq!(a.size(), b.size());
    a.as_slice()
        .iter()
        .zip(b.as_slice())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

fn assert_valid(img: &Image) {
    for px in img.as_slice().chunks_exact(4) {
        assert!(px.iter().all(|v| v.is_finite()), "non-finite pixel {px:?}");
        assert!((0.0..=1.0).contains(&px[3]), "alpha out of range {px:?}");
        assert!(
            px[..3].iter().all(|c| *c >= 0.0 && *c <= px[3] + 1e-6),
            "bad colour {px:?}"
        );
    }
}

/// A handful of dot pairs, all inside the frame, moved by modest amounts.
fn moved_dots() -> Vec<(Point, Point)> {
    vec![
        (pt(12.0, 8.0), pt(15.0, 10.0)),
        (pt(36.0, 8.0), pt(33.0, 11.0)),
        (pt(24.0, 16.0), pt(26.0, 14.0)),
        (pt(12.0, 24.0), pt(10.0, 22.0)),
        (pt(36.0, 24.0), pt(38.0, 26.0)),
    ]
}

fn scenes() -> Vec<(&'static str, Image, Image)> {
    vec![
        (
            "gradient / grid",
            gradient(W, H).unwrap(),
            labelled_grid(W, H, 16).unwrap(),
        ),
        (
            "alpha / checker",
            alpha_gradient(W, H).unwrap(),
            checkerboard(W, H, 6, [200, 30, 30, 255], [30, 30, 200, 90]).unwrap(),
        ),
    ]
}

// ---- Invariant 1: first frame is Image 1, last frame is Image 2 --------------------------

#[test]
fn first_frame_is_image_1_and_last_frame_is_image_2() {
    for (name, a, b) in scenes() {
        for policy in [MeshPolicy::Frame0, MeshPolicy::PerFrame] {
            for morph in [MorphType::Morph, MorphType::Warp1] {
                let mut seq = sequence(&moved_dots(), morph, 9);
                seq.mesh = policy;
                let r = renderer(&seq, &a, &b);
                let first = r.render_frame(0).unwrap();
                assert!(
                    max_diff(&first, &a) < 1e-4,
                    "{name} {policy:?} {morph:?}: first frame"
                );
                if morph == MorphType::Morph {
                    let last = r.render_frame(8).unwrap();
                    assert!(max_diff(&last, &b) < 1e-4, "{name} {policy:?}: last frame");
                }
            }
        }
    }
}

#[test]
fn endpoints_hold_even_when_dots_move_far_and_fold_the_mesh() {
    // Dots swap sides, which flips triangles in the shared mesh.
    let dots = [
        (pt(8.0, 8.0), pt(40.0, 24.0)),
        (pt(40.0, 8.0), pt(8.0, 24.0)),
        (pt(8.0, 24.0), pt(40.0, 8.0)),
        (pt(40.0, 24.0), pt(8.0, 8.0)),
        (pt(24.0, 16.0), pt(24.0, 16.0)),
    ];
    let a = labelled_grid(W, H, 8).unwrap();
    let b = gradient(W, H).unwrap();
    let seq = sequence(&dots, MorphType::Morph, 5);
    let r = renderer(&seq, &a, &b);
    assert!(max_diff(&r.render_frame(0).unwrap(), &a) < 1e-4);
    assert!(max_diff(&r.render_frame(4).unwrap(), &b) < 1e-4);
    for i in 0..5 {
        assert_valid(&r.render_frame(i).unwrap());
    }
}

#[test]
fn single_frame_sequence_is_image_1() {
    let (a, b) = (gradient(W, H).unwrap(), labelled_grid(W, H, 8).unwrap());
    let seq = sequence(&moved_dots(), MorphType::Morph, 1);
    let r = renderer(&seq, &a, &b);
    assert!(max_diff(&r.render_frame(0).unwrap(), &a) < 1e-4);
}

// ---- Invariant 2: identical dots reduce to a pure cross-dissolve -------------------------

#[test]
fn identical_dot_sets_are_a_pure_cross_dissolve() {
    let fixed: Vec<(Point, Point)> = moved_dots()
        .iter()
        .map(|&(a, _)| (a, a))
        .chain([(pt(-6.0, 40.0), pt(-6.0, 40.0))])
        .collect();
    for (name, a, b) in scenes() {
        let seq = sequence(&fixed, MorphType::Morph, 6);
        let r = renderer(&seq, &a, &b);
        for i in 0..6 {
            let t = i as f32 / 5.0;
            let want = Image::lerp(&a, &b, t).unwrap();
            let got = r.render_frame(i).unwrap();
            assert!(max_diff(&got, &want) < 1e-4, "{name} frame {i}");
        }
    }
}

#[test]
fn no_dots_at_all_is_also_a_cross_dissolve() {
    let (a, b) = (gradient(W, H).unwrap(), labelled_grid(W, H, 8).unwrap());
    let seq = sequence(&[], MorphType::Morph, 5);
    let r = renderer(&seq, &a, &b);
    let want = Image::lerp(&a, &b, 0.5).unwrap();
    assert!(max_diff(&r.render_frame(2).unwrap(), &want) < 1e-4);
}

// ---- Invariant 3: warp with unmoved dots is the identity ---------------------------------

#[test]
fn warp_with_unmoved_dots_is_the_identity_on_every_frame() {
    let fixed: Vec<(Point, Point)> = moved_dots().iter().map(|&(a, _)| (a, a)).collect();
    let (a, b) = (labelled_grid(W, H, 8).unwrap(), gradient(W, H).unwrap());
    let seq = sequence(&fixed, MorphType::Warp1, 7);
    // With Image 2 present, and with it absent: Image 2 must never leak in.
    for image2 in [Some(&b), None] {
        let r = Renderer::new(&seq, (W, H), &a, image2).unwrap();
        for i in 0..7 {
            assert!(
                max_diff(&r.render_frame(i).unwrap(), &a) < 1e-4,
                "frame {i}"
            );
        }
    }
}

// ---- Invariant 4: no NaN, alpha in [0, 1] ------------------------------------------------

fn coord() -> impl Strategy<Value = f64> {
    (-30i32..=90).prop_map(|v| f64::from(v) * 0.5)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn output_is_always_valid_for_arbitrary_dots(
        raw in prop::collection::vec((coord(), coord(), coord(), coord()), 0..10),
        warp in any::<bool>(),
        per_frame in any::<bool>(),
        fit in 0..3u8,
    ) {
        let pairs: Vec<(Point, Point)> =
            raw.iter().map(|&(a, b, c, d)| (pt(a, b), pt(c, d))).collect();
        let mut seq = sequence(&pairs, if warp { MorphType::Warp1 } else { MorphType::Morph }, 4);
        if per_frame { seq.mesh = MeshPolicy::PerFrame; }
        seq.fit = [FitMode::Stretch, FitMode::Contain, FitMode::Cover][fit as usize];
        let a = alpha_gradient(40, 40).unwrap();
        let b = checkerboard(W, H, 5, [255, 0, 0, 255], [0, 0, 255, 60]).unwrap();
        match Renderer::new(&seq, (W, H), &a, Some(&b)) {
            Err(RenderError::Morph(MorphError::DuplicateDot { .. })) => {}
            Err(e) => prop_assert!(false, "unexpected error: {e}"),
            Ok(r) => {
                for i in 0..4 {
                    let img = r.render_frame(i).expect("frame renders");
                    assert_valid(&img);
                }
            }
        }
    }
}

// ---- Invariant 5: determinism ------------------------------------------------------------

#[test]
fn rendering_is_deterministic_across_runs_and_thread_counts() {
    let (a, b) = (labelled_grid(W, H, 8).unwrap(), gradient(W, H).unwrap());
    let seq = sequence(&moved_dots(), MorphType::Morph, 5);
    let r = renderer(&seq, &a, &b);
    let reference = r.render_frame(2).unwrap().to_rgba8();
    assert_eq!(r.render_frame(2).unwrap().to_rgba8(), reference);
    for threads in [1, 2, 7] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let got = pool.install(|| r.render_frame(2).unwrap().to_rgba8());
        assert_eq!(got, reference, "{threads} threads");
    }
    // A freshly built renderer gives the same bytes too.
    assert_eq!(
        renderer(&seq, &a, &b).render_frame(2).unwrap().to_rgba8(),
        reference
    );
}

// ---- Behaviour ---------------------------------------------------------------------------

#[test]
fn warp_moves_image_content_to_the_new_dot_position() {
    // Image 1's red ramp is 0 at the left edge and 255 at the right. Drag the centre dot
    // from x = 16 to x = 32; surrounding dots are pinned. At t = 1 the pixel at the new
    // dot position must show what Image 1 had at the old one.
    let a = gradient(W, H).unwrap();
    let pinned = [(8.0, 4.0), (40.0, 4.0), (8.0, 28.0), (40.0, 28.0)];
    let mut pairs: Vec<(Point, Point)> =
        pinned.iter().map(|&(x, y)| (pt(x, y), pt(x, y))).collect();
    pairs.push((pt(16.0, 16.0), pt(32.0, 16.0)));
    let seq = sequence(&pairs, MorphType::Warp1, 3);
    let r = Renderer::new(&seq, (W, H), &a, None).unwrap();
    let last = r.render_frame(2).unwrap();
    let at_new = last.get(32, 16).unwrap()[0];
    let want = a.get(16, 16).unwrap()[0];
    assert!(
        (at_new - want).abs() < 0.06,
        "got {at_new}, want about {want}"
    );
    // And it really moved: the undisturbed image has a much larger red value at x = 32.
    assert!(a.get(32, 16).unwrap()[0] - want > 0.2);
}

#[test]
fn different_sized_sources_are_stretched_to_the_output() {
    let a = gradient(W, H).unwrap();
    let b = gradient(96, 64).unwrap();
    let seq = sequence(&[], MorphType::Morph, 2);
    let r = renderer(&seq, &a, &b);
    let last = r.render_frame(1).unwrap();
    let mut worst = 0.0_f32;
    for y in 0..H {
        for x in 0..W {
            let want = b.sample_bilinear((x as f32 + 0.5) * 2.0, (y as f32 + 0.5) * 2.0);
            let got = last.get(x, y).unwrap();
            worst = worst.max(
                got.iter()
                    .zip(want)
                    .map(|(g, w)| (g - w).abs())
                    .fold(0.0, f32::max),
            );
        }
    }
    assert!(worst < 1e-4, "worst {worst}");
}

#[test]
fn contain_leaves_transparent_bars_and_cover_fills_the_frame() {
    let wide = checkerboard(96, 16, 4, [255, 255, 255, 255], [0, 0, 0, 255]).unwrap();
    let mut seq = sequence(&[], MorphType::Warp1, 1);

    seq.fit = FitMode::Contain;
    let img = Renderer::new(&seq, (W, H), &wide, None)
        .unwrap()
        .render_frame(0)
        .unwrap();
    // Scale 0.5 -> content 48x8, centred: bars on rows 0..12 and 20..32.
    assert_eq!(img.get(10, 2).unwrap()[3], 0.0);
    assert_eq!(img.get(10, 29).unwrap()[3], 0.0);
    assert_eq!(img.get(10, 16).unwrap()[3], 1.0);

    seq.fit = FitMode::Cover;
    let img = Renderer::new(&seq, (W, H), &wide, None)
        .unwrap()
        .render_frame(0)
        .unwrap();
    assert!(img.as_slice().chunks_exact(4).all(|p| p[3] == 1.0));
}

#[test]
fn per_frame_and_shared_meshes_agree_when_nothing_moves() {
    let fixed: Vec<(Point, Point)> = moved_dots().iter().map(|&(a, _)| (a, a)).collect();
    let (a, b) = (labelled_grid(W, H, 8).unwrap(), gradient(W, H).unwrap());
    let mut seq = sequence(&fixed, MorphType::Morph, 5);
    let shared = renderer(&seq, &a, &b).render_frame(2).unwrap();
    seq.mesh = MeshPolicy::PerFrame;
    let per = renderer(&seq, &a, &b).render_frame(2).unwrap();
    assert!(max_diff(&shared, &per) < 1e-4);
}

#[test]
fn degenerate_dot_sets_render_without_error() {
    let a = labelled_grid(W, H, 8).unwrap();
    let b = gradient(W, H).unwrap();
    let cases: Vec<Vec<(Point, Point)>> = vec![
        // One dot.
        vec![(pt(20.0, 10.0), pt(30.0, 20.0))],
        // Collinear on Image 1, collinear on a different line on Image 2.
        vec![
            (pt(10.0, 16.0), pt(24.0, 5.0)),
            (pt(20.0, 16.0), pt(24.0, 15.0)),
            (pt(30.0, 16.0), pt(24.0, 25.0)),
        ],
        // Collinear on Image 1 only becomes a triangle on Image 2.
        vec![
            (pt(10.0, 16.0), pt(10.0, 5.0)),
            (pt(20.0, 16.0), pt(30.0, 25.0)),
            (pt(30.0, 16.0), pt(40.0, 5.0)),
        ],
        // Dots well outside the frame.
        vec![
            (pt(-40.0, -40.0), pt(-30.0, -35.0)),
            (pt(90.0, 70.0), pt(80.0, 60.0)),
        ],
        // Dots exactly on the corners.
        vec![
            (pt(0.0, 0.0), pt(5.0, 5.0)),
            (pt(48.0, 32.0), pt(40.0, 28.0)),
        ],
    ];
    for (n, pairs) in cases.iter().enumerate() {
        let seq = sequence(pairs, MorphType::Morph, 5);
        let r = renderer(&seq, &a, &b);
        for i in 0..5 {
            assert_valid(
                &r.render_frame(i)
                    .unwrap_or_else(|e| panic!("case {n} frame {i}: {e}")),
            );
        }
        assert!(
            max_diff(&r.render_frame(0).unwrap(), &a) < 1e-4,
            "case {n} first frame"
        );
        assert!(
            max_diff(&r.render_frame(4).unwrap(), &b) < 1e-4,
            "case {n} last frame"
        );
    }
}

// ---- Errors, not panics ------------------------------------------------------------------

#[test]
fn bad_input_is_reported() {
    let (a, b) = (gradient(W, H).unwrap(), gradient(W, H).unwrap());
    let seq = sequence(&[], MorphType::Morph, 3);

    assert!(matches!(
        Renderer::new(&seq, (W, H), &a, None),
        Err(RenderError::MissingImage2)
    ));
    assert!(matches!(
        Renderer::new(&seq, (0, H), &a, Some(&b)),
        Err(RenderError::Raster(_))
    ));
    assert!(matches!(
        Renderer::new(&seq, (u32::MAX, u32::MAX), &a, Some(&b)),
        Err(RenderError::Raster(_))
    ));

    let dup = sequence(
        &[(pt(5.0, 5.0), pt(6.0, 6.0)), (pt(5.0, 5.0), pt(9.0, 9.0))],
        MorphType::Morph,
        3,
    );
    assert!(matches!(
        Renderer::new(&dup, (W, H), &a, Some(&b)),
        Err(RenderError::Morph(MorphError::DuplicateDot {
            image: 1,
            ..
        }))
    ));

    let nan = sequence(&[(pt(f64::NAN, 5.0), pt(6.0, 6.0))], MorphType::Morph, 3);
    assert!(matches!(
        Renderer::new(&nan, (W, H), &a, Some(&b)),
        Err(RenderError::Morph(_))
    ));

    let zero_frames = Sequence {
        frames: 0,
        ..Sequence::default()
    };
    assert!(matches!(
        Renderer::new(&zero_frames, (W, H), &a, Some(&b)),
        Err(RenderError::Morph(_))
    ));
}

#[test]
fn out_of_range_frame_indexes_clamp_to_the_last_frame() {
    let (a, b) = (gradient(W, H).unwrap(), labelled_grid(W, H, 8).unwrap());
    let seq = sequence(&moved_dots(), MorphType::Morph, 4);
    let r = renderer(&seq, &a, &b);
    assert_eq!(r.frame_count(), 4);
    assert_eq!(
        r.render_frame(3).unwrap(),
        r.render_frame(u32::MAX).unwrap()
    );
    assert!(r.render_t(f64::NAN).is_ok());
    assert!(r.render_t(f64::INFINITY).is_ok());
}
