//! The effects pass: mask, back colour, background and foreground around a rendered morph.
//!
//! Effect images are stretched to the output size and sampled bilinearly at pixel centres.
//! Layer order, bottom to top: back colour, background, the masked morph, foreground.

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
}

/// Brightness of a premultiplied pixel on a black backdrop, in `0..=1`.
fn luminance(p: Pixel) -> f32 {
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

/// Whether every enabled effect has the image it needs.
pub fn check(effects: &Effects, images: &EffectImages) -> Result<(), RenderError> {
    let needs = [
        (effects.background.enabled, images.background, "background"),
        (effects.mask.enabled, images.mask, "mask"),
        (effects.foreground.enabled, images.foreground, "foreground"),
    ];
    match needs.iter().find(|(on, img, _)| *on && img.is_none()) {
        Some((_, _, name)) => Err(RenderError::MissingEffectImage(name)),
        None => Ok(()),
    }
}

/// Stretch-sample `image` at output pixel `(x, y)` of a `width` x `height` frame.
fn sample(image: &Image, x: u32, y: u32, width: u32, height: u32) -> Pixel {
    let u = (f64::from(x) + 0.5) * f64::from(image.width()) / f64::from(width.max(1));
    let v = (f64::from(y) + 0.5) * f64::from(image.height()) / f64::from(height.max(1));
    image.sample_bilinear(u as f32, v as f32)
}

/// Apply the enabled effects to a rendered morph frame, in place.
pub fn apply(
    effects: &Effects,
    images: &EffectImages,
    frame: &mut Image,
) -> Result<(), RenderError> {
    if !effects.any_active() {
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

    frame
        .as_mut_slice()
        .par_chunks_mut(width as usize * 4)
        .enumerate()
        .for_each(|(y, row)| {
            let y = y as u32;
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let x = x as u32;
                let mut morph: Pixel = [px[0], px[1], px[2], px[3]];
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
                let mut out = over(morph, base);
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
