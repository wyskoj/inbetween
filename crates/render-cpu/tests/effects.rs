//! Effects around the morph: layer order, mask maths, missing images, invariants.

use inbetween_doc::{ImageRef, Sequence};
use inbetween_raster::{Image, Pixel};
use inbetween_render_cpu::{EffectImages, RenderError, Renderer};

const RED: Pixel = [1.0, 0.0, 0.0, 1.0];
const BLUE: Pixel = [0.0, 0.0, 1.0, 1.0];
const CLEAR: Pixel = [0.0; 4];

fn solid(px: Pixel) -> Image {
    Image::filled(8, 6, px).unwrap()
}

fn seq() -> Sequence {
    Sequence {
        image1: ImageRef::new("a"),
        image2: ImageRef::new("b"),
        frames: 3,
        ..Sequence::default()
    }
}

fn render(seq: &Sequence, a: &Image, b: &Image, fx: EffectImages, frame: u32) -> Image {
    Renderer::new(seq, (8, 6), a, Some(b))
        .unwrap()
        .with_effect_images(fx)
        .render_frame(frame)
        .unwrap()
}

#[test]
fn effects_off_change_nothing() {
    let (a, b) = (solid(RED), solid(BLUE));
    let plain = render(&seq(), &a, &b, EffectImages::default(), 1);
    let mut s = seq();
    s.effects.mask.image = ImageRef::new("ignored.png"); // configured but not enabled
    let same = render(&s, &a, &b, EffectImages::default(), 1);
    assert_eq!(plain, same);
}

#[test]
fn back_color_shows_through_transparent_morph() {
    let (a, b) = (solid(CLEAR), solid(CLEAR));
    let mut s = seq();
    s.effects.back_color.enabled = true;
    s.effects.back_color.rgba = [0, 255, 0, 255];
    let out = render(&s, &a, &b, EffectImages::default(), 0);
    assert_eq!(out.get(3, 3), Some([0.0, 1.0, 0.0, 1.0]));
}

#[test]
fn layer_order_is_back_background_morph_foreground() {
    let half_red: Pixel = [0.5, 0.0, 0.0, 0.5];
    let (a, b) = (solid(half_red), solid(half_red));
    let bg = solid(BLUE);
    let fg = solid([0.0, 0.0, 0.0, 0.0]);
    let mut s = seq();
    s.effects.back_color.enabled = true; // opaque black
    s.effects.background.enabled = true;
    s.effects.background.opacity = 50.0;
    s.effects.foreground.enabled = true;
    let fx = EffectImages {
        background: Some(&bg),
        foreground: Some(&fg),
        mask: None,
    };
    let out = render(&s, &a, &b, fx, 0);
    // Black, then blue at 50% -> (0,0,.5,1); half-red over that -> r .5, b .25.
    let p = out.get(2, 2).unwrap();
    assert!((p[0] - 0.5).abs() < 1e-5 && (p[2] - 0.25).abs() < 1e-5 && (p[3] - 1.0).abs() < 1e-5);
}

#[test]
fn foreground_covers_the_morph() {
    let (a, b) = (solid(RED), solid(RED));
    let fg = solid([0.0, 0.5, 0.0, 0.5]);
    let mut s = seq();
    s.effects.foreground.enabled = true;
    s.effects.foreground.opacity = 100.0;
    let fx = EffectImages {
        foreground: Some(&fg),
        ..EffectImages::default()
    };
    let p = render(&s, &a, &b, fx, 0).get(0, 0).unwrap();
    assert!((p[0] - 0.5).abs() < 1e-5 && (p[1] - 0.5).abs() < 1e-5 && (p[3] - 1.0).abs() < 1e-5);
}

#[test]
fn mask_hides_dark_areas_and_invert_swaps() {
    let (a, b) = (solid(RED), solid(RED));
    // Left half white, right half black.
    let mut mask = solid([0.0, 0.0, 0.0, 1.0]);
    for y in 0..6 {
        for x in 0..4 {
            mask.set(x, y, [1.0, 1.0, 1.0, 1.0]);
        }
    }
    let mut s = seq();
    s.effects.mask.enabled = true;
    let fx = EffectImages {
        mask: Some(&mask),
        ..EffectImages::default()
    };
    let out = render(&s, &a, &b, fx, 0);
    assert_eq!(out.get(1, 1), Some(RED));
    assert_eq!(out.get(6, 1), Some(CLEAR));
    s.effects.mask.invert = true;
    let out = render(&s, &a, &b, fx, 0);
    assert_eq!(out.get(1, 1), Some(CLEAR));
    assert_eq!(out.get(6, 1), Some(RED));
    s.effects.mask.invert = false;
    s.effects.mask.intensity = 0.0;
    assert_eq!(render(&s, &a, &b, fx, 0).get(6, 1), Some(RED));
}

#[test]
fn effect_images_of_other_sizes_are_stretched() {
    let (a, b) = (solid(CLEAR), solid(CLEAR));
    let bg = Image::filled(2, 2, BLUE).unwrap();
    let mut s = seq();
    s.effects.background.enabled = true;
    let fx = EffectImages {
        background: Some(&bg),
        ..EffectImages::default()
    };
    let out = render(&s, &a, &b, fx, 0);
    assert!(out.as_slice().chunks(4).all(|p| p == BLUE));
}

#[test]
fn enabled_effect_without_image_is_an_error_not_a_skip() {
    let (a, b) = (solid(RED), solid(BLUE));
    let mut s = seq();
    s.effects.foreground.enabled = true;
    let r = Renderer::new(&s, (8, 6), &a, Some(&b)).unwrap();
    assert_eq!(
        r.render_frame(0),
        Err(RenderError::MissingEffectImage("foreground"))
    );
}

#[test]
fn output_stays_valid_with_extreme_settings() {
    let (a, b) = (solid([0.3, 0.2, 0.1, 0.4]), solid(BLUE));
    let junk = solid([0.9, 0.1, 0.5, 0.7]);
    let mut s = seq();
    s.effects.back_color.enabled = true;
    s.effects.back_color.rgba = [255, 128, 0, 7];
    s.effects.background.enabled = true;
    s.effects.background.opacity = 100.0;
    s.effects.mask.enabled = true;
    s.effects.mask.intensity = -100.0;
    s.effects.foreground.enabled = true;
    let fx = EffectImages {
        background: Some(&junk),
        mask: Some(&junk),
        foreground: Some(&junk),
    };
    for f in 0..3 {
        let out = render(&s, &a, &b, fx, f);
        for p in out.as_slice().chunks(4) {
            assert!(p.iter().all(|v| v.is_finite()));
            assert!((0.0..=1.0).contains(&p[3]));
            assert!(p[..3].iter().all(|&c| c >= 0.0 && c <= p[3] + 1e-6));
        }
    }
}

#[test]
fn rendering_with_effects_is_deterministic() {
    let (a, b) = (solid(RED), solid(BLUE));
    let m = solid([0.4, 0.4, 0.4, 1.0]);
    let mut s = seq();
    s.effects.mask.enabled = true;
    let fx = EffectImages {
        mask: Some(&m),
        ..EffectImages::default()
    };
    assert_eq!(render(&s, &a, &b, fx, 1), render(&s, &a, &b, fx, 1));
}
