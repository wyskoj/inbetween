//! Geometry: exact quarter turns and flips, and affine resampling.

use crate::par::build;
use crate::{Image, Pixel, RasterError, TRANSPARENT};

/// Rotate clockwise by `turns` quarter turns (taken modulo 4). Exact: no resampling.
pub fn rotate_quarter(img: &Image, turns: u8) -> Result<Image, RasterError> {
    let (w, h) = img.size();
    match turns % 4 {
        0 => Ok(img.clone()),
        1 => build(h, w, |x, y| img.get(y, h - 1 - x).unwrap_or(TRANSPARENT)),
        2 => build(w, h, |x, y| {
            img.get(w - 1 - x, h - 1 - y).unwrap_or(TRANSPARENT)
        }),
        _ => build(h, w, |x, y| img.get(w - 1 - y, x).unwrap_or(TRANSPARENT)),
    }
}

/// Mirror left-right (`h`) and/or top-bottom (`v`). Exact.
pub fn flip(img: &Image, h: bool, v: bool) -> Result<Image, RasterError> {
    let (w, ht) = img.size();
    if !h && !v {
        return Ok(img.clone());
    }
    build(w, ht, |x, y| {
        let sx = if h { w - 1 - x } else { x };
        let sy = if v { ht - 1 - y } else { y };
        img.get(sx, sy).unwrap_or(TRANSPARENT)
    })
}

/// One tap, transparent outside the image.
fn tap(img: &Image, x: i64, y: i64) -> Pixel {
    if x < 0 || y < 0 || x > i64::from(u32::MAX) || y > i64::from(u32::MAX) {
        return TRANSPARENT;
    }
    img.get(x as u32, y as u32).unwrap_or(TRANSPARENT)
}

/// Bilinear sample at continuous `(x, y)` (pixel centres at `+ 0.5`), transparent outside.
pub fn sample_transparent(img: &Image, x: f64, y: f64) -> Pixel {
    if !x.is_finite() || !y.is_finite() || x.abs() > 1.0e9 || y.abs() > 1.0e9 {
        return TRANSPARENT;
    }
    let fx = x - 0.5;
    let fy = y - 0.5;
    let x0 = fx.floor();
    let y0 = fy.floor();
    let tx = (fx - x0) as f32;
    let ty = (fy - y0) as f32;
    let (x0, y0) = (x0 as i64, y0 as i64);
    let mix = |a: Pixel, b: Pixel, t: f32| {
        let u = 1.0 - t;
        [
            a[0] * u + b[0] * t,
            a[1] * u + b[1] * t,
            a[2] * u + b[2] * t,
            a[3] * u + b[3] * t,
        ]
    };
    let top = mix(tap(img, x0, y0), tap(img, x0 + 1, y0), tx);
    let bottom = mix(tap(img, x0, y0 + 1), tap(img, x0 + 1, y0 + 1), tx);
    mix(top, bottom, ty)
}

/// Resample into an `out_w x out_h` image. `map` is the inverse affine `[a, b, c, d, e, f]`
/// taking an output position `(x, y)` to the source position `(a x + b y + c, d x + e y + f)`.
/// Output pixels whose source falls outside the image are transparent.
pub fn resample(img: &Image, out_w: u32, out_h: u32, map: [f64; 6]) -> Result<Image, RasterError> {
    build(out_w, out_h, |x, y| {
        let px = f64::from(x) + 0.5;
        let py = f64::from(y) + 0.5;
        let sx = map[0] * px + map[1] * py + map[2];
        let sy = map[3] * px + map[4] * py + map[5];
        sample_transparent(img, sx, sy)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(w: u32, h: u32) -> Image {
        let mut img = Image::new(w, h).unwrap();
        for y in 0..h {
            for x in 0..w {
                let v = (y * w + x + 1) as f32 / (w * h + 1) as f32;
                img.set(x, y, [v, v * 0.5, v * 0.25, 1.0]);
            }
        }
        img
    }

    #[test]
    fn four_cw_turns_are_identity() {
        let img = ramp(5, 3);
        let mut cur = img.clone();
        for _ in 0..4 {
            cur = rotate_quarter(&cur, 1).unwrap();
        }
        assert_eq!(cur, img);
        assert_eq!(rotate_quarter(&img, 1).unwrap().size(), (3, 5));
    }

    #[test]
    fn cw_then_ccw_and_half_turns_compose() {
        let img = ramp(4, 7);
        let cw = rotate_quarter(&img, 1).unwrap();
        assert_eq!(rotate_quarter(&cw, 3).unwrap(), img);
        let half = rotate_quarter(&img, 2).unwrap();
        assert_eq!(rotate_quarter(&cw, 1).unwrap(), half);
        // Top-left pixel goes to top-right after a CW turn.
        assert_eq!(cw.get(6, 0), img.get(0, 0));
    }

    #[test]
    fn flips_are_involutions() {
        let img = ramp(3, 5);
        for (h, v) in [(true, false), (false, true), (true, true)] {
            assert_eq!(flip(&flip(&img, h, v).unwrap(), h, v).unwrap(), img);
        }
        assert_eq!(flip(&img, true, false).unwrap().get(0, 0), img.get(2, 0));
    }

    #[test]
    fn one_by_one_and_lines() {
        let one = ramp(1, 1);
        assert_eq!(rotate_quarter(&one, 1).unwrap(), one);
        assert_eq!(flip(&one, true, true).unwrap(), one);
        let line = ramp(1, 6);
        assert_eq!(rotate_quarter(&line, 1).unwrap().size(), (6, 1));
    }

    #[test]
    fn identity_resample_is_exact_and_translation_is_exact() {
        let img = ramp(6, 4);
        let same = resample(&img, 6, 4, [1.0, 0.0, 0.0, 0.0, 1.0, 0.0]).unwrap();
        assert_eq!(same, img);
        let shifted = resample(&img, 3, 2, [1.0, 0.0, 2.0, 0.0, 1.0, 1.0]).unwrap();
        assert_eq!(shifted.get(0, 0), img.get(2, 1));
        assert_eq!(shifted.get(2, 1), img.get(4, 2));
    }

    #[test]
    fn outside_is_transparent_and_garbage_is_safe() {
        let img = ramp(2, 2);
        let out = resample(&img, 2, 2, [1.0, 0.0, 10.0, 0.0, 1.0, 0.0]).unwrap();
        assert_eq!(out.get(0, 0), Some(TRANSPARENT));
        let bad = resample(&img, 2, 2, [f64::NAN; 6]).unwrap();
        assert!(bad.as_slice().iter().all(|v| *v == 0.0));
    }
}
