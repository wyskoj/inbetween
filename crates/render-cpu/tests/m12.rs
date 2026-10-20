//! Camera, master curve, transition, light, trail and radial effects through the renderer.

use inbetween_doc::{CameraKey, Curve, CurvePoint, ImageRef, Sequence};
use inbetween_raster::{Image, Pixel};
use inbetween_render_cpu::{EffectImages, Renderer};

const RED: Pixel = [1.0, 0.0, 0.0, 1.0];
const BLUE: Pixel = [0.0, 0.0, 1.0, 1.0];

fn solid(px: Pixel) -> Image {
    Image::filled(8, 6, px).unwrap()
}

fn seq(frames: u32) -> Sequence {
    Sequence {
        image1: ImageRef::new("a"),
        image2: ImageRef::new("b"),
        frames,
        ..Sequence::default()
    }
}

fn render(s: &Sequence, a: &Image, b: &Image, fx: EffectImages, frame: u32) -> Image {
    Renderer::new(s, (8, 6), a, Some(b))
        .unwrap()
        .with_effect_images(fx)
        .render_frame(frame)
        .unwrap()
}

fn plain(s: &Sequence, a: &Image, b: &Image, frame: u32) -> Image {
    render(s, a, b, EffectImages::default(), frame)
}

fn assert_valid(img: &Image) {
    for p in img.as_slice().chunks(4) {
        assert!(p.iter().all(|v| v.is_finite()), "{p:?}");
        assert!((0.0..=1.0).contains(&p[3]), "{p:?}");
        assert!(
            p[..3].iter().all(|&c| c >= -1e-6 && c <= p[3] + 1e-5),
            "{p:?}"
        );
    }
}

#[test]
fn defaults_leave_the_plain_morph_alone() {
    let (a, b) = (solid(RED), solid(BLUE));
    let s = seq(5);
    let base = plain(&s, &a, &b, 2);
    let mut t = s.clone();
    t.camera.enabled = true; // no keys
    t.effects.trail.intensity = 100.0; // configured but off
    assert_eq!(plain(&t, &a, &b, 2), base);
}

#[test]
fn camera_pans_the_morph_and_shows_transparency_behind_it() {
    let (a, b) = (solid(RED), solid(RED));
    let mut s = seq(3);
    s.camera.enabled = true;
    s.camera.set_key(CameraKey::identity(0.0)).unwrap();
    s.camera
        .set_key(CameraKey {
            x: 0.5,
            ..CameraKey::identity(1.0)
        })
        .unwrap();
    // First frame: untouched. Last: shifted right by four of the eight pixels.
    assert_eq!(plain(&s, &a, &b, 0), solid(RED));
    let last = plain(&s, &a, &b, 2);
    assert_eq!(last.get(0, 0), Some([0.0; 4]));
    assert_eq!(last.get(3, 0), Some([0.0; 4]));
    assert_eq!(last.get(4, 0), Some(RED));
    assert_valid(&last);
    // Camera zone: a back colour moves with the camera, so the uncovered side stays
    // transparent and the covered side shows the colour.
    s.effects.back_color.enabled = true;
    s.effects.back_color.rgba = [0, 0, 255, 255];
    let zoned = plain(&s, &a, &b, 2);
    assert_eq!(zoned.get(0, 0), Some([0.0; 4]));
    assert_eq!(zoned.get(4, 0), Some(RED));
    // Where the morph itself is transparent the colour shows inside the zone.
    s.drop_corner_tris = false;
    let (gap, _) = (Image::filled(8, 6, [0.0; 4]).unwrap(), 0);
    assert_eq!(plain(&s, &gap, &gap, 2).get(4, 0), Some(BLUE));
    assert_eq!(plain(&s, &gap, &gap, 2).get(0, 0), Some([0.0; 4]));
    assert_valid(&zoned);
}

#[test]
fn sequence_filters_run_after_the_camera_and_change_the_frame() {
    let mut half = Image::filled(8, 6, RED).unwrap();
    for y in 0..6 {
        for x in 4..8 {
            half.set(x, y, BLUE);
        }
    }
    let mut s = seq(2);
    let plain_frame = plain(&s, &half, &half, 0);
    s.filters = vec![inbetween_doc::Filter::Blur { radius: 2 }];
    let blurred = plain(&s, &half, &half, 0);
    assert_ne!(plain_frame, blurred);
    assert_valid(&blurred);
    assert_eq!(blurred, plain(&s, &half, &half, 0));
}

#[test]
fn master_curve_fades_the_whole_frame() {
    let (a, b) = (solid(RED), solid(RED));
    let mut s = seq(5);
    s.master_curve = Curve {
        points: vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 1.0)],
        linear: true,
    };
    assert!(s.validate().is_ok());
    let first = plain(&s, &a, &b, 0);
    assert_eq!(first.get(1, 1), Some([0.0; 4]));
    let mid = plain(&s, &a, &b, 2).get(1, 1).unwrap();
    assert!(
        (mid[3] - 0.5).abs() < 1e-5 && (mid[0] - 0.5).abs() < 1e-5,
        "{mid:?}"
    );
    assert_eq!(plain(&s, &a, &b, 4), solid(RED));
    // Fade out instead.
    s.master_curve = Curve {
        points: vec![CurvePoint::new(0.0, 1.0), CurvePoint::new(1.0, 0.0)],
        linear: true,
    };
    assert_eq!(plain(&s, &a, &b, 0), solid(RED));
    assert_eq!(plain(&s, &a, &b, 4).get(0, 0), Some([0.0; 4]));
}

fn left_dark_right_light() -> Image {
    let mut m = solid([1.0, 1.0, 1.0, 1.0]);
    for y in 0..6 {
        for x in 0..4 {
            m.set(x, y, [0.0, 0.0, 0.0, 1.0]);
        }
    }
    m
}

#[test]
fn transition_dissolves_dark_pixels_first_and_keeps_the_ends() {
    let (a, b) = (solid(RED), solid(BLUE));
    let tr = left_dark_right_light();
    let mut s = seq(11);
    s.effects.transition.enabled = true;
    s.effects.transition.softness = 0.0;
    let fx = EffectImages {
        transition: Some(&tr),
        ..EffectImages::default()
    };
    // Ends are exact images.
    assert_eq!(render(&s, &a, &b, fx, 0), solid(RED));
    assert_eq!(render(&s, &a, &b, fx, 10), solid(BLUE));
    // At t = 0.5 the dark left half has gone over and the light right half has not.
    let mid = render(&s, &a, &b, fx, 5);
    assert_eq!(mid.get(1, 1), Some(BLUE));
    assert_eq!(mid.get(6, 1), Some(RED));
    // Inverted: the other way round.
    s.effects.transition.invert = true;
    let mid = render(&s, &a, &b, fx, 5);
    assert_eq!(mid.get(1, 1), Some(RED));
    assert_eq!(mid.get(6, 1), Some(BLUE));
}

#[test]
fn transition_follows_the_feature_curve() {
    let (a, b) = (solid(RED), solid(BLUE));
    let tr = solid([0.5, 0.5, 0.5, 1.0]);
    let mut s = seq(11);
    s.effects.transition.enabled = true;
    s.effects.transition.softness = 100.0;
    s.feature_curve = Curve {
        points: vec![
            CurvePoint::new(0.0, 0.0),
            CurvePoint::new(0.5, 0.9),
            CurvePoint::new(1.0, 1.0),
        ],
        linear: true,
    };
    let fx = EffectImages {
        transition: Some(&tr),
        ..EffectImages::default()
    };
    let mid = render(&s, &a, &b, fx, 5).get(0, 0).unwrap();
    assert!(mid[2] > 0.8, "feature curve not used: {mid:?}");
}

#[test]
fn light_image_brightens_and_darkens() {
    let (a, b) = (solid([0.4, 0.4, 0.4, 1.0]), solid([0.4, 0.4, 0.4, 1.0]));
    let mut light = solid([1.0, 1.0, 1.0, 1.0]);
    for y in 0..6 {
        for x in 0..4 {
            light.set(x, y, [0.0, 0.0, 0.0, 1.0]);
        }
    }
    let mut s = seq(3);
    s.effects.light.enabled = true;
    s.effects.light.bright = 100.0;
    s.effects.light.dark = 50.0;
    let fx = EffectImages {
        light: Some(&light),
        ..EffectImages::default()
    };
    let out = render(&s, &a, &b, fx, 1);
    assert!((out.get(1, 1).unwrap()[0] - 0.2).abs() < 1e-5, "dark side");
    assert_eq!(out.get(6, 1), Some([1.0, 1.0, 1.0, 1.0]));
    assert_valid(&out);
}

#[test]
fn radial_rays_draw_over_or_under() {
    let (a, b) = (solid([0.0, 0.0, 0.0, 1.0]), solid([0.0, 0.0, 0.0, 1.0]));
    let mut s = seq(3);
    s.effects.radial.enabled = true;
    s.effects.radial.intensity = 100.0;
    let over_img = plain(&s, &a, &b, 1);
    assert!(over_img.as_slice().chunks(4).any(|p| p[0] > 0.1));
    assert_valid(&over_img);
    // Under an opaque morph the rays are hidden.
    s.effects.radial.overlay = false;
    assert_eq!(plain(&s, &a, &b, 1), solid([0.0, 0.0, 0.0, 1.0]));
    // Under a transparent morph they show.
    let clear = solid([0.0; 4]);
    assert!(plain(&s, &clear, &clear, 1)
        .as_slice()
        .chunks(4)
        .any(|p| p[3] > 0.1));
    // Rays move with time.
    s.effects.radial.overlay = true;
    assert_ne!(plain(&s, &a, &b, 0), plain(&s, &a, &b, 2));
}

#[test]
fn trail_blends_earlier_frames_but_never_touches_frame_zero() {
    let (a, b) = (solid(RED), solid(BLUE));
    let mut s = seq(5);
    s.effects.trail.enabled = true; // blur
    s.effects.trail.intensity = 100.0;
    assert_eq!(render(&s, &a, &b, EffectImages::default(), 0), solid(RED));
    let blurred = plain(&s, &a, &b, 2);
    let sharp = {
        let mut t = s.clone();
        t.effects.trail.enabled = false;
        plain(&t, &a, &b, 2)
    };
    assert_ne!(blurred, sharp);
    // Blur averages in earlier (redder) frames, so there is more red than in the sharp frame.
    assert!(blurred.get(0, 0).unwrap()[0] > sharp.get(0, 0).unwrap()[0]);
    assert_valid(&blurred);
    // Trail mode on a transparent picture leaves tinted ghosts.
    s.effects.trail.overlay = false;
    s.effects.trail.rgba = [0, 255, 0, 255];
    let ghost = plain(&s, &a, &b, 3);
    assert_valid(&ghost);
    // render_t has no frame index, so it never draws a trail.
    let r = Renderer::new(&s, (8, 6), &a, Some(&b)).unwrap();
    let mut off = s.clone();
    off.effects.trail.enabled = false;
    let r_off = Renderer::new(&off, (8, 6), &a, Some(&b)).unwrap();
    assert_eq!(r.render_t(0.5).unwrap(), r_off.render_t(0.5).unwrap());
}

#[test]
fn new_effects_are_deterministic_and_valid_under_stress() {
    let (a, b) = (solid([0.3, 0.2, 0.1, 0.4]), solid(BLUE));
    let junk = solid([0.9, 0.1, 0.5, 0.7]);
    let mut s = seq(4);
    s.effects.transition.enabled = true;
    s.effects.light.enabled = true;
    s.effects.light.bright = 100.0;
    s.effects.trail.enabled = true;
    s.effects.trail.intensity = 100.0;
    s.effects.radial.enabled = true;
    s.effects.radial.intensity = 100.0;
    s.camera.enabled = true;
    s.camera
        .set_key(CameraKey {
            rotation: 33.0,
            scale_x: -1.5,
            x: 0.1,
            ..CameraKey::identity(1.0)
        })
        .unwrap();
    s.master_curve = Curve {
        points: vec![CurvePoint::new(0.0, 0.2), CurvePoint::new(1.0, 0.9)],
        linear: false,
    };
    let fx = EffectImages {
        transition: Some(&junk),
        light: Some(&junk),
        ..EffectImages::default()
    };
    for f in 0..4 {
        let one = render(&s, &a, &b, fx, f);
        assert_valid(&one);
        assert_eq!(one, render(&s, &a, &b, fx, f));
    }
}

#[test]
fn enabled_effect_without_its_image_is_an_error() {
    let (a, b) = (solid(RED), solid(BLUE));
    for which in ["transition", "light"] {
        let mut s = seq(3);
        match which {
            "transition" => s.effects.transition.enabled = true,
            _ => s.effects.light.enabled = true,
        }
        let r = Renderer::new(&s, (8, 6), &a, Some(&b)).unwrap();
        assert!(r.render_frame(1).is_err(), "{which}");
    }
}
