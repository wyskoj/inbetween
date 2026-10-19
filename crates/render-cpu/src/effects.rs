//! The effects pass: light, mask, back colour, background, radial rays and foreground around
//! a rendered morph. (The transition changes how the morph itself is mixed, so it lives in the
//! renderer; the trail combines several frames, so it does too.)
//!
//! Effect images are stretched to the output size and sampled bilinearly at pixel centres.
//! Layer order, bottom to top: back colour, background, rays (when under), the lit and masked
//! morph, rays (when over), foreground.

use crate::RenderError;
use inbetween_doc::Effects;
use inbetween_raster::{clamp_valid, over, scale, Image, Pixel, TRANSPARENT};
use rayon::prelude::*;

/// The loaded images for the effects that are switched on.
#[derive(Clone, Copy, Debug, Default)]
pub struct EffectImages<'a> {
    pub background: Option<&'a Image>,
    pub mask: Option<&'a Image>,
    pub foreground: Option<&'a Image>,
    pub transition: Option<&'a Image>,
    pub light: Option<&'a Image>,
}

/// Brightness of a premultiplied pixel on a black backdrop, in `0..=1`.
pub(crate) fn luminance(p: Pixel) -> f32 {
    (0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]).clamp(0.0, 1.0)
}

/// How much of the morph a mask pixel of brightness `light` lets through.
pub fn mask_factor(light: f32, intensity: f64, invert: bool) -> f32 {
    let light = if light.is_nan() {
        0.0
    } else {
        light.clamp(0.0, 1.0)
    };
    let light = if invert { 1.0 - light } else { light };
    let i = if intensity.is_nan() {
        0.0
    } else {
        (intensity / 100.0).clamp(-1.0, 1.0) as f32
    };
    if i >= 0.0 {
        1.0 - i * (1.0 - light)
    } else {
        1.0 + i * light
    }
}

/// How far (0 to 1) a pixel whose transition image has brightness `light` has dissolved from
/// Image 1 to Image 2 at colour-dissolve time `t`. Dark pixels go first (light ones when
/// `invert`); `softness` (0 to 100) is how much of the timeline each pixel takes to dissolve.
/// Always 0 at `t = 0` and 1 at `t = 1`, whatever the image.
pub fn transition_amount(light: f32, t: f64, softness: f64, invert: bool) -> f64 {
    let light = if light.is_nan() {
        0.0
    } else {
        f64::from(light.clamp(0.0, 1.0))
    };
    let light = if invert { 1.0 - light } else { light };
    let soft = if softness.is_nan() {
        0.0
    } else {
        (softness / 100.0).clamp(0.0, 1.0)
    };
    // A hard edge still takes a sliver of time, so the ends stay exact.
    let soft = soft.max(1e-3);
    let start = light * (1.0 - soft);
    let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
    if t >= 1.0 {
        return 1.0;
    }
    ((t - start) / soft).clamp(0.0, 1.0)
}

/// Brighten or darken a premultiplied pixel by a light image brightness `level`: above mid
/// gray pushes towards white by up to `bright` percent, below pushes towards black by up to
/// `dark` percent. Mid gray, and a pixel with no alpha, are unchanged; alpha never changes.
pub fn light_pixel(p: Pixel, level: f32, bright: f64, dark: f64) -> Pixel {
    let level = if level.is_nan() {
        0.5
    } else {
        level.clamp(0.0, 1.0)
    };
    let d = (level - 0.5) * 2.0;
    let pct = |v: f64| {
        if v.is_nan() {
            0.0
        } else {
            (v / 100.0).clamp(0.0, 1.0) as f32
        }
    };
    let a = p[3];
    if d >= 0.0 {
        let k = d * pct(bright);
        [
            p[0] + (a - p[0]) * k,
            p[1] + (a - p[1]) * k,
            p[2] + (a - p[2]) * k,
            a,
        ]
    } else {
        let k = 1.0 - (-d) * pct(dark);
        [p[0] * k, p[1] * k, p[2] * k, a]
    }
}

/// Rays: how much (0 to 1) of the ray colour covers output pixel centre `(x, y)` of a
/// `width` x `height` frame at morph time `t`. 24 rays from the centre fade towards the
/// corners and turn slowly as `t` goes from 0 to 1.
pub fn ray_strength(x: f32, y: f32, width: u32, height: u32, t: f64) -> f32 {
    const RAYS: f32 = 24.0;
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    let (dx, dy) = (x - w / 2.0, y - h / 2.0);
    let reach = 0.5 * (w * w + h * h).sqrt();
    let r = ((dx * dx + dy * dy).sqrt() / reach).clamp(0.0, 1.0);
    let turn = if t.is_nan() {
        0.0
    } else {
        t.clamp(0.0, 1.0) as f32
    };
    let angle = dy.atan2(dx) * RAYS + turn * std::f32::consts::PI;
    let ray = 0.5 + 0.5 * angle.cos();
    let strength = ray * ray * ray * (1.0 - 0.8 * r);
    if strength.is_finite() {
        strength.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Whether every enabled effect has the image it needs.
pub fn check(effects: &Effects, images: &EffectImages) -> Result<(), RenderError> {
    let needs = [
        (effects.background.enabled, images.background, "background"),
        (effects.mask.enabled, images.mask, "mask"),
        (effects.foreground.enabled, images.foreground, "foreground"),
        (effects.transition.enabled, images.transition, "transition"),
        (effects.light.enabled, images.light, "light"),
    ];
    match needs.iter().find(|(on, img, _)| *on && img.is_none()) {
        Some((_, _, name)) => Err(RenderError::MissingEffectImage(name)),
        None => Ok(()),
    }
}

/// Stretch-sample `image` at output pixel `(x, y)` of a `width` x `height` frame.
pub(crate) fn sample(image: &Image, x: u32, y: u32, width: u32, height: u32) -> Pixel {
    let u = (f64::from(x) + 0.5) * f64::from(image.width()) / f64::from(width.max(1));
    let v = (f64::from(y) + 0.5) * f64::from(image.height()) / f64::from(height.max(1));
    image.sample_bilinear(u as f32, v as f32)
}

/// Apply the enabled effects to a rendered morph frame at morph time `t`, in place.
pub fn apply(
    effects: &Effects,
    images: &EffectImages,
    frame: &mut Image,
    t: f64,
) -> Result<(), RenderError> {
    if !(effects.back_color.enabled
        || effects.background.enabled
        || effects.mask.enabled
        || effects.foreground.enabled
        || effects.light.enabled
        || effects.radial.enabled)
    {
        return Ok(());
    }
    check(effects, images)?;
    let (width, height) = frame.size();
    let back = effects.back_color.enabled.then(|| {
        let [r, g, b, a] = effects.back_color.rgba.map(|c| f32::from(c) / 255.0);
        [r * a, g * a, b * a, a]
    });
    let bg_opacity = (effects.background.opacity / 100.0).clamp(0.0, 1.0) as f32;
    let fg_opacity = (effects.foreground.opacity / 100.0).clamp(0.0, 1.0) as f32;
    let background = images.background.filter(|_| effects.background.enabled);
    let foreground = images.foreground.filter(|_| effects.foreground.enabled);
    let mask = images.mask.filter(|_| effects.mask.enabled);
    let lit = images.light.filter(|_| effects.light.enabled);
    let rays = effects.radial.enabled.then(|| {
        let [r, g, b, a] = effects.radial.rgba.map(|c| f32::from(c) / 255.0);
        let gain = (effects.radial.intensity / 100.0).clamp(0.0, 1.0) as f32 * a;
        ([r, g, b], gain)
    });
    let ray_px = |x: u32, y: u32| -> Option<Pixel> {
        rays.map(|([r, g, b], gain)| {
            let k = ray_strength(x as f32 + 0.5, y as f32 + 0.5, width, height, t) * gain;
            [r * k, g * k, b * k, k]
        })
    };

    frame
        .as_mut_slice()
        .par_chunks_mut(width as usize * 4)
        .enumerate()
        .for_each(|(y, row)| {
            let y = y as u32;
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let x = x as u32;
                let mut morph: Pixel = [px[0], px[1], px[2], px[3]];
                if let Some(l) = lit {
                    let level = luminance(sample(l, x, y, width, height));
                    morph = light_pixel(morph, level, effects.light.bright, effects.light.dark);
                }
                if let Some(m) = mask {
                    let light = luminance(sample(m, x, y, width, height));
                    morph = scale(
                        morph,
                        mask_factor(light, effects.mask.intensity, effects.mask.invert),
                    );
                }
                let mut base = back.unwrap_or(TRANSPARENT);
                if let Some(bg) = background {
                    base = over(scale(sample(bg, x, y, width, height), bg_opacity), base);
                }
                let ray = ray_px(x, y);
                if let Some(r) = ray.filter(|_| !effects.radial.overlay) {
                    base = over(r, base);
                }
                let mut out = over(morph, base);
                if let Some(r) = ray.filter(|_| effects.radial.overlay) {
                    out = over(r, out);
                }
                if let Some(fg) = foreground {
                    out = over(scale(sample(fg, x, y, width, height), fg_opacity), out);
                }
                px.copy_from_slice(&clamp_valid(out));
            }
        });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_runs_dark_first_and_keeps_the_ends() {
        for soft in [0.0, 20.0, 100.0, f64::NAN] {
            for light in [0.0, 0.3, 1.0, f32::NAN] {
                for invert in [false, true] {
                    assert_eq!(transition_amount(light, 0.0, soft, invert), 0.0);
                    assert_eq!(transition_amount(light, 1.0, soft, invert), 1.0);
                }
            }
        }
        // Hard edge: a dark pixel has gone over by t = 0.2, a light one has not.
        assert_eq!(transition_amount(0.0, 0.2, 0.0, false), 1.0);
        assert_eq!(transition_amount(1.0, 0.2, 0.0, false), 0.0);
        // Inverted swaps them.
        assert_eq!(transition_amount(1.0, 0.2, 0.0, true), 1.0);
        // Soft: halfway through its own window a pixel is halfway dissolved.
        let a = transition_amount(0.0, 0.1, 20.0, false);
        assert!((a - 0.5).abs() < 1e-9, "{a}");
        // Monotone in t.
        let mut last = 0.0;
        for i in 0..=100 {
            let v = transition_amount(0.6, f64::from(i) / 100.0, 30.0, false);
            assert!(v >= last);
            last = v;
        }
    }

    #[test]
    fn light_brightens_above_mid_gray_and_darkens_below() {
        let p = [0.2, 0.4, 0.6, 0.8];
        assert_eq!(light_pixel(p, 0.5, 100.0, 100.0), p);
        let up = light_pixel(p, 1.0, 100.0, 0.0);
        assert_eq!(up, [0.8, 0.8, 0.8, 0.8]);
        let down = light_pixel(p, 0.0, 0.0, 100.0);
        assert_eq!(down, [0.0, 0.0, 0.0, 0.8]);
        // Zero intensity does nothing; empty pixels stay empty.
        assert_eq!(light_pixel(p, 1.0, 0.0, 0.0), p);
        assert_eq!(light_pixel(TRANSPARENT, 1.0, 100.0, 100.0), TRANSPARENT);
        for lv in [f32::NAN, -3.0, 9.0] {
            let q = light_pixel(p, lv, f64::NAN, 1e9);
            assert!(q.iter().all(|c| c.is_finite() && *c <= q[3] + 1e-6));
        }
    }

    #[test]
    fn rays_stay_in_range_and_turn_with_time() {
        let mut seen = false;
        for y in 0..20 {
            for x in 0..20 {
                let a = ray_strength(x as f32, y as f32, 20, 20, 0.0);
                let b = ray_strength(x as f32, y as f32, 20, 20, 1.0);
                assert!((0.0..=1.0).contains(&a) && (0.0..=1.0).contains(&b));
                seen |= a != b;
            }
        }
        assert!(seen, "rays do not move");
        assert_eq!(ray_strength(f32::NAN, 0.0, 0, 0, f64::NAN), 0.0);
    }

    #[test]
    fn mask_factor_cases() {
        // Full intensity: white keeps, black hides.
        assert_eq!(mask_factor(1.0, 100.0, false), 1.0);
        assert_eq!(mask_factor(0.0, 100.0, false), 0.0);
        // Inverted swaps them.
        assert_eq!(mask_factor(1.0, 100.0, true), 0.0);
        // Negative intensity: white hides.
        assert_eq!(mask_factor(1.0, -100.0, false), 0.0);
        assert_eq!(mask_factor(0.0, -100.0, false), 1.0);
        // Zero does nothing; half intensity halves the effect.
        assert_eq!(mask_factor(0.0, 0.0, false), 1.0);
        assert_eq!(mask_factor(0.0, 50.0, false), 0.5);
        // Hostile input stays in range.
        for l in [f32::NAN, -5.0, 9.0] {
            let f = mask_factor(l, 1e9, false);
            assert!((0.0..=1.0).contains(&f));
        }
        assert!((0.0..=1.0).contains(&mask_factor(0.5, f64::NAN, false)));
    }
}
