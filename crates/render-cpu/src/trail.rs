//! The trail effect: motion blur or a fading, tinted trail built from earlier frames.

use crate::RenderError;
use inbetween_doc::Trail;
use inbetween_raster::{clamp_valid, over, scale, Image, Pixel, TRANSPARENT};

/// Most earlier frames a trail uses.
pub const MAX_PAST: u32 = 8;

/// How many earlier frames an intensity (0 to 100) asks for: none at 0, up to [`MAX_PAST`].
pub fn past_frames(intensity: f64) -> u32 {
    if intensity.is_nan() || intensity <= 0.0 {
        return 0;
    }
    let k = (intensity / 100.0).clamp(0.0, 1.0);
    1 + (k * f64::from(MAX_PAST - 1)).round() as u32
}

fn pixels(image: &Image) -> impl Iterator<Item = Pixel> + '_ {
    image
        .as_slice()
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
}

/// Combine `current` with the `past` frames (oldest first, the same size as `current`). With
/// nothing in `past` the current frame comes back untouched.
pub fn combine(trail: &Trail, current: Image, past: &[Image]) -> Result<Image, RenderError> {
    let past: Vec<&Image> = past.iter().filter(|p| p.size() == current.size()).collect();
    if past.is_empty() {
        return Ok(current);
    }
    let mut out = Image::new(current.width(), current.height())?;
    if trail.overlay {
        // Motion blur: the plain average of this frame and the ones before it.
        let k = 1.0 / (past.len() + 1) as f32;
        for (i, px) in out.as_mut_slice().chunks_exact_mut(4).enumerate() {
            let mut sum = current
                .as_slice()
                .get(i * 4..i * 4 + 4)
                .map_or(TRANSPARENT, |c| [c[0], c[1], c[2], c[3]]);
            for p in &past {
                if let Some(c) = p.as_slice().get(i * 4..i * 4 + 4) {
                    for (s, v) in sum.iter_mut().zip(c) {
                        *s += v;
                    }
                }
            }
            px.copy_from_slice(&clamp_valid(scale(sum, k)));
        }
    } else {
        // Trail: tinted copies, older ones fainter, laid under the current frame.
        let tint = trail.rgba.map(|c| f32::from(c) / 255.0);
        let strength = (trail.intensity / 100.0).clamp(0.0, 1.0) as f32 * 0.6;
        let n = past.len();
        let weights: Vec<f32> = (0..n)
            .map(|i| strength * (i + 1) as f32 / (n + 1) as f32)
            .collect();
        let currents: Vec<Pixel> = pixels(&current).collect();
        let mut layers: Vec<std::vec::IntoIter<Pixel>> = Vec::new();
        for p in &past {
            layers.push(pixels(p).collect::<Vec<_>>().into_iter());
        }
        for (px, cur) in out.as_mut_slice().chunks_exact_mut(4).zip(currents) {
            let mut under = TRANSPARENT;
            for (layer, w) in layers.iter_mut().zip(&weights) {
                let Some(g) = layer.next() else { continue };
                // Blend the frame's colour towards the tint by the tint's own alpha, keeping
                // the frame's shape (its alpha).
                let a = g[3];
                let ghost = [
                    g[0] + (tint[0] * a - g[0]) * tint[3],
                    g[1] + (tint[1] * a - g[1]) * tint[3],
                    g[2] + (tint[2] * a - g[2]) * tint[3],
                    a,
                ];
                under = over(scale(ghost, *w), under);
            }
            px.copy_from_slice(&clamp_valid(over(cur, under)));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(v: f32) -> Image {
        Image::filled(3, 2, [v, v, v, 1.0]).unwrap()
    }

    #[test]
    fn intensity_picks_how_many_frames() {
        assert_eq!(past_frames(0.0), 0);
        assert_eq!(past_frames(f64::NAN), 0);
        assert_eq!(past_frames(1.0), 1);
        assert_eq!(past_frames(100.0), MAX_PAST);
        assert_eq!(past_frames(1e9), MAX_PAST);
    }

    #[test]
    fn nothing_before_means_the_current_frame_unchanged() {
        let t = Trail::default();
        assert_eq!(combine(&t, flat(0.3), &[]).unwrap(), flat(0.3));
    }

    #[test]
    fn blur_averages_the_frames() {
        let t = Trail::default();
        let out = combine(&t, flat(0.9), &[flat(0.3)]).unwrap();
        let got = out.get(1, 1).unwrap();
        assert!((got[0] - 0.6).abs() < 1e-6 && got[3] == 1.0);
    }

    #[test]
    fn a_trail_goes_under_the_current_frame() {
        let t = Trail {
            overlay: false,
            rgba: [255, 0, 0, 255],
            intensity: 100.0,
            ..Trail::default()
        };
        // An opaque current frame hides what is under it.
        let out = combine(&t, flat(0.5), &[flat(1.0)]).unwrap();
        assert_eq!(out, flat(0.5));
        // Where the current frame is empty the red ghost shows.
        let empty = Image::new(3, 2).unwrap();
        let out = combine(&t, empty, &[flat(1.0)]).unwrap();
        let got = out.get(0, 0).unwrap();
        assert!(got[0] > 0.0 && got[1] == 0.0 && got[2] == 0.0 && got[3] > 0.0);
    }
}
