//! What each morph type must do at its ends and in between, and how hold frames behave.

use inbetween_doc::{DotPair, DotSet, MorphType, Sequence};
use inbetween_geom::Point;
use inbetween_raster::{over, scale, Image, Pixel};
use inbetween_render_cpu::{RenderError, Renderer};
use inbetween_testkit::{alpha_gradient, checkerboard, gradient, labelled_grid};
use proptest::prelude::*;

const W: u32 = 48;
const H: u32 = 32;

type Xy = (f64, f64);

fn pt(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn sequence(pairs: &[(Xy, Xy)], morph: MorphType, frames: u32) -> Sequence {
    Sequence {
        dots: DotSet::new(
            pairs
                .iter()
                .map(|&((a, b), (c, d))| DotPair::new(pt(a, b), pt(c, d)))
                .collect(),
        ),
        morph,
        frames,
        ..Sequence::default()
    }
}

fn moved() -> Vec<(Xy, Xy)> {
    vec![
        ((12.0, 8.0), (15.0, 10.0)),
        ((36.0, 8.0), (33.0, 11.0)),
        ((24.0, 16.0), (26.0, 14.0)),
        ((12.0, 24.0), (10.0, 22.0)),
        ((36.0, 24.0), (38.0, 26.0)),
    ]
}

/// The same dots, not moved: every warp is then the identity.
fn pinned() -> Vec<(Xy, Xy)> {
    moved().into_iter().map(|(a, _)| (a, a)).collect()
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
        assert!(px.iter().all(|v| v.is_finite()), "non-finite {px:?}");
        assert!((0.0..=1.0).contains(&px[3]), "alpha {px:?}");
        assert!(
            px[..3].iter().all(|c| *c >= 0.0 && *c <= px[3] + 1e-6),
            "{px:?}"
        );
    }
}

fn opaque() -> (Image, Image) {
    (gradient(W, H).unwrap(), labelled_grid(W, H, 8).unwrap())
}

fn translucent() -> (Image, Image) {
    (
        alpha_gradient(W, H).unwrap(),
        checkerboard(W, H, 6, [200, 30, 30, 255], [30, 30, 200, 0]).unwrap(),
    )
}

fn px(img: &Image, x: u32, y: u32) -> Pixel {
    img.get(x, y).unwrap()
}

fn close(a: Pixel, b: Pixel, tol: f32) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tol)
}

// ----------------------------------------------------------------------------- Warp 2

#[test]
fn warp2_shows_only_image_two_and_ends_on_it() {
    let (a, b) = opaque();
    let seq = sequence(&moved(), MorphType::Warp2, 7);
    let r = renderer(&seq, &a, &b);
    assert!(
        max_diff(&r.render_frame(6).unwrap(), &b) < 1e-4,
        "last frame is Image 2"
    );
    // Image 1 never leaks in, even with no Image 1 colour at all to compare against.
    let blank = Image::new(W, H).unwrap();
    let with_blank = renderer(&seq, &blank, &b);
    for i in 0..7 {
        assert!(
            max_diff(
                &r.render_frame(i).unwrap(),
                &with_blank.render_frame(i).unwrap()
            ) < 1e-6
        );
    }
}

#[test]
fn warp2_with_unmoved_dots_is_the_identity_on_image_two_every_frame() {
    let (a, b) = opaque();
    let r = renderer(&sequence(&pinned(), MorphType::Warp2, 6), &a, &b);
    for i in 0..6 {
        assert!(
            max_diff(&r.render_frame(i).unwrap(), &b) < 1e-4,
            "frame {i}"
        );
    }
}

#[test]
fn warp2_starts_with_image_twos_colours_in_image_ones_shape() {
    // Drag one dot far on Image 2. At t = 0 the mesh is Image 1's, so Image 2's content around
    // that dot is pulled back to where the dot is on Image 1.
    let a = gradient(W, H).unwrap();
    let b = labelled_grid(W, H, 8).unwrap();
    let dots = [
        ((8.0, 4.0), (8.0, 4.0)),
        ((40.0, 4.0), (40.0, 4.0)),
        ((8.0, 28.0), (8.0, 28.0)),
        ((40.0, 28.0), (40.0, 28.0)),
        ((16.0, 16.0), (32.0, 16.0)),
    ];
    let r = renderer(&sequence(&dots, MorphType::Warp2, 3), &a, &b);
    let first = r.render_frame(0).unwrap();
    assert!(
        max_diff(&first, &b) > 0.05,
        "the shape differs from Image 2"
    );
    // Image 2's pixel at the dot (32, 16) now appears at Image 1's dot (16, 16).
    let (want, got) = (px(&b, 32, 16), px(&first, 16, 16));
    assert!(close(want, got, 0.35), "{want:?} vs {got:?}");
}

// ----------------------------------------------------------------------- fades and direct

#[test]
fn fade_out_scales_colour_keeps_alpha_and_ends_black() {
    let (a, b) = translucent();
    let seq = sequence(&moved(), MorphType::FadeOut1, 5);
    let r = Renderer::new(&seq, (W, H), &a, None).unwrap(); // Image 2 is not needed
    assert_eq!(r.frame_count(), 5);
    assert!(
        max_diff(&r.render_frame(0).unwrap(), &a) < 1e-5,
        "starts as Image 1"
    );
    for (i, t) in [(1u32, 0.25f32), (2, 0.5), (3, 0.75)] {
        let f = r.render_frame(i).unwrap();
        for (x, y) in [(3, 3), (20, 10), (44, 30)] {
            let src = px(&a, x, y);
            let want = [
                src[0] * (1.0 - t),
                src[1] * (1.0 - t),
                src[2] * (1.0 - t),
                src[3],
            ];
            assert!(close(px(&f, x, y), want, 1e-5), "frame {i} at ({x},{y})");
        }
    }
    let last = r.render_frame(4).unwrap();
    for x in 0..W {
        let p = px(&last, x, 5);
        assert_eq!(&p[..3], &[0.0, 0.0, 0.0], "black");
        assert!((p[3] - px(&a, x, 5)[3]).abs() < 1e-6, "alpha kept");
    }
    // Image 2 is never read, so a different one changes nothing.
    let with_b = renderer(&seq, &a, &b);
    assert!(
        max_diff(
            &with_b.render_frame(2).unwrap(),
            &r.render_frame(2).unwrap()
        ) < 1e-7
    );
}

#[test]
fn fade_in_starts_black_and_ends_on_image_two() {
    let (a, b) = opaque();
    let seq = sequence(&moved(), MorphType::FadeIn2, 5);
    let r = renderer(&seq, &a, &b);
    let first = r.render_frame(0).unwrap();
    for p in first.as_slice().chunks_exact(4) {
        assert_eq!(&p[..3], &[0.0, 0.0, 0.0]);
        assert_eq!(p[3], 1.0);
    }
    assert!(
        max_diff(&r.render_frame(4).unwrap(), &b) < 1e-5,
        "ends as Image 2"
    );
    let mid = r.render_frame(2).unwrap();
    let (src, got) = (px(&b, 10, 10), px(&mid, 10, 10));
    assert!(close(
        got,
        [src[0] * 0.5, src[1] * 0.5, src[2] * 0.5, src[3]],
        1e-5
    ));
}

#[test]
fn direct_cuts_from_image_one_to_image_two_on_the_last_frame_only() {
    let (a, b) = opaque();
    let seq = sequence(&moved(), MorphType::Direct, 6);
    let r = renderer(&seq, &a, &b);
    for i in 0..5 {
        assert!(
            max_diff(&r.render_frame(i).unwrap(), &a) < 1e-6,
            "frame {i} is Image 1"
        );
    }
    assert!(
        max_diff(&r.render_frame(5).unwrap(), &b) < 1e-6,
        "last frame is Image 2"
    );
}

#[test]
fn fades_and_direct_ignore_bad_dots() {
    let (a, b) = opaque();
    let dup = [((5.0, 5.0), (6.0, 6.0)), ((5.0, 5.0), (9.0, 9.0))];
    for morph in [MorphType::FadeOut1, MorphType::FadeIn2, MorphType::Direct] {
        let r = renderer(&sequence(&dup, morph, 3), &a, &b);
        assert!(r.render_frame(1).is_ok(), "{morph:?}");
    }
    assert!(matches!(
        Renderer::new(&sequence(&dup, MorphType::Morph, 3), (W, H), &a, Some(&b)),
        Err(RenderError::Morph(_))
    ));
}

// -------------------------------------------------------------------------- Morph Hold

#[test]
fn morph_hold_lays_image_two_over_a_steady_image_one() {
    let (a, b) = translucent();
    let seq = sequence(&pinned(), MorphType::MorphHold, 5);
    let r = renderer(&seq, &a, &b);
    assert!(
        max_diff(&r.render_frame(0).unwrap(), &a) < 1e-6,
        "starts as Image 1"
    );
    for (i, t) in [(1u32, 0.25f32), (2, 0.5), (4, 1.0)] {
        let f = r.render_frame(i).unwrap();
        for (x, y) in [(2, 2), (8, 2), (20, 14), (44, 28)] {
            let want = over(scale(px(&b, x, y), t), px(&a, x, y));
            assert!(close(px(&f, x, y), want, 1e-4), "frame {i} at ({x},{y})");
        }
    }
}

#[test]
fn morph_hold_keeps_image_one_where_image_two_is_transparent_but_morph_does_not() {
    let (a, b) = translucent();
    // Checker cells of Image 2 alternate opaque red and fully transparent blue.
    let hold = renderer(&sequence(&pinned(), MorphType::MorphHold, 5), &a, &b);
    let plain = renderer(&sequence(&pinned(), MorphType::Morph, 5), &a, &b);
    let (hold_last, plain_last) = (
        hold.render_frame(4).unwrap(),
        plain.render_frame(4).unwrap(),
    );
    // Cell (1, 0) spans x 6..12: transparent in Image 2.
    let (x, y) = (9, 3);
    assert_eq!(px(&b, x, y)[3], 0.0);
    assert!(
        close(px(&hold_last, x, y), px(&a, x, y), 1e-5),
        "Image 1 stays visible"
    );
    assert_eq!(
        px(&plain_last, x, y)[3],
        0.0,
        "a plain morph has faded Image 1 away"
    );
}

// ------------------------------------------------------------------------- missing images

#[test]
fn types_that_need_image_two_say_so_and_the_others_do_not() {
    let (a, _) = opaque();
    for morph in MorphType::ALL {
        let seq = sequence(&moved(), morph, 4);
        let r = Renderer::new(&seq, (W, H), &a, None);
        if morph.uses_image2() {
            assert!(matches!(r, Err(RenderError::MissingImage2)), "{morph:?}");
        } else {
            assert!(r.unwrap().render_frame(1).is_ok(), "{morph:?}");
        }
    }
}

// --------------------------------------------------------------------------- hold frames

#[test]
fn hold_frames_repeat_the_first_and_last_morph_frames() {
    let (a, b) = opaque();
    let plain = sequence(&moved(), MorphType::Morph, 5);
    let held = Sequence {
        hold_start: 3,
        hold_end: 2,
        ..plain.clone()
    };
    let (rp, rh) = (renderer(&plain, &a, &b), renderer(&held, &a, &b));
    assert_eq!((rp.frame_count(), rh.frame_count()), (5, 10));
    for i in 0..3 {
        assert_eq!(
            rh.render_frame(i).unwrap(),
            rp.render_frame(0).unwrap(),
            "start hold {i}"
        );
    }
    for i in 0..5 {
        assert_eq!(
            rh.render_frame(3 + i).unwrap(),
            rp.render_frame(i).unwrap(),
            "morph {i}"
        );
    }
    for i in 8..10 {
        assert_eq!(
            rh.render_frame(i).unwrap(),
            rp.render_frame(4).unwrap(),
            "end hold {i}"
        );
    }
    assert_eq!(
        rh.render_frame(9).unwrap(),
        rh.render_frame(u32::MAX).unwrap()
    );
    // The held ends are exactly the source images.
    assert!(max_diff(&rh.render_frame(0).unwrap(), &a) < 1e-4);
    assert!(max_diff(&rh.render_frame(9).unwrap(), &b) < 1e-4);
}

#[test]
fn holds_work_for_every_morph_type() {
    let (a, b) = translucent();
    for morph in MorphType::ALL {
        let seq = Sequence {
            hold_start: 2,
            hold_end: 2,
            ..sequence(&moved(), morph, 4)
        };
        let r = renderer(&seq, &a, &b);
        assert_eq!(r.frame_count(), 8);
        assert_eq!(
            r.render_frame(0).unwrap(),
            r.render_frame(2).unwrap(),
            "{morph:?}"
        );
        assert_eq!(
            r.render_frame(5).unwrap(),
            r.render_frame(7).unwrap(),
            "{morph:?}"
        );
    }
}

#[test]
fn a_hold_longer_than_the_limit_is_an_error() {
    let (a, b) = opaque();
    let seq = Sequence {
        hold_start: 1000,
        ..sequence(&[], MorphType::Morph, 4)
    };
    assert!(matches!(
        Renderer::new(&seq, (W, H), &a, Some(&b)),
        Err(RenderError::Morph(_))
    ));
}

// -------------------------------------------------------------------------------- general

fn coord() -> impl Strategy<Value = f64> {
    (-30i32..=90).prop_map(|v| f64::from(v) * 0.5)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    #[test]
    fn every_type_always_gives_valid_output(
        raw in prop::collection::vec((coord(), coord(), coord(), coord()), 0..8),
        kind in 0..7usize,
        hold_start in 0..3u32,
        hold_end in 0..3u32,
    ) {
        let pairs: Vec<(Xy, Xy)> = raw.iter().map(|&(a, b, c, d)| ((a, b), (c, d))).collect();
        let mut seq = sequence(&pairs, MorphType::ALL[kind], 4);
        seq.hold_start = hold_start;
        seq.hold_end = hold_end;
        let (a, b) = translucent();
        match Renderer::new(&seq, (W, H), &a, Some(&b)) {
            Err(RenderError::Morph(_)) => {} // duplicate dots on a warping type
            Err(e) => prop_assert!(false, "unexpected error {e}"),
            Ok(r) => {
                for i in 0..r.frame_count() {
                    let img = r.render_frame(i).expect("frame renders");
                    assert_valid(&img);
                    prop_assert_eq!(&img, &r.render_frame(i).expect("again"));
                }
            }
        }
    }
}

#[test]
fn every_type_is_deterministic_across_thread_counts() {
    let (a, b) = translucent();
    for morph in MorphType::ALL {
        let seq = sequence(&moved(), morph, 5);
        let r = renderer(&seq, &a, &b);
        let reference = r.render_frame(2).unwrap().to_rgba8();
        for threads in [1, 3] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            let got = pool.install(|| r.render_frame(2).unwrap().to_rgba8());
            assert_eq!(got, reference, "{morph:?} on {threads} threads");
        }
    }
}
