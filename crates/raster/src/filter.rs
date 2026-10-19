//! Image filters. Each takes an image and bounded parameters and returns a new image of the
//! same size. Parameters outside their documented range are clamped and NaN counts as the
//! neutral value, so no input panics or produces NaN. A neutral parameter (zero amount or
//! radius) returns the input unchanged. Centres are normalised: `(0.5, 0.5)` is the middle.
//!
//! Output is deterministic: the same input and parameters always give the same bytes.

use crate::par::{build, map};
use crate::{clamp_valid, luminance, Image, Pixel, RasterError};
use rayon::prelude::*;

fn num(v: f64, lo: f64, hi: f64, neutral: f64) -> f64 {
    if v.is_nan() {
        neutral
    } else {
        v.clamp(lo, hi)
    }
}

fn dims(img: &Image) -> (usize, usize) {
    (img.width() as usize, img.height() as usize)
}

// ---- Separable passes -------------------------------------------------------------------

/// Columns handled per task by the vertical pass.
const STRIP: usize = 32;

/// Run a row pass over the rows, then over the columns. The vertical pass works on strips of
/// columns, transposed into a small local buffer so the row pass can be reused.
fn separable(
    img: &Image,
    pass: impl Fn(&[f32], usize) -> Vec<f32> + Sync,
) -> Result<Image, RasterError> {
    let (w, h) = dims(img);
    let horizontal = pass(img.as_slice(), w);
    let strips: Vec<(usize, Vec<f32>)> = (0..w.div_ceil(STRIP))
        .into_par_iter()
        .map(|s| {
            let x0 = s * STRIP;
            let sw = STRIP.min(w - x0);
            let mut local = vec![0.0f32; sw * h * 4];
            for y in 0..h {
                let from = (y * w + x0) * 4;
                if let Some(src) = horizontal.get(from..from + sw * 4) {
                    for (c, px) in src.chunks_exact(4).enumerate() {
                        let d = (c * h + y) * 4;
                        if let Some(dst) = local.get_mut(d..d + 4) {
                            dst.copy_from_slice(px);
                        }
                    }
                }
            }
            (sw, pass(&local, h))
        })
        .collect();
    let mut out = vec![0.0f32; horizontal.len()];
    out.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
        let mut x = 0;
        for (sw, local) in &strips {
            for c in 0..*sw {
                let s = (c * h + y) * 4;
                if let (Some(src), Some(dst)) = (local.get(s..s + 4), row.get_mut(x * 4..x * 4 + 4))
                {
                    dst.copy_from_slice(src);
                }
                x += 1;
            }
        }
    });
    Image::from_premultiplied(img.width(), img.height(), out)
}

/// Clamp-edged box blur of every row of `data` (rows `w` pixels wide) by `radius`.
fn box_rows(data: &[f32], w: usize, radius: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; data.len()];
    if w == 0 {
        return out;
    }
    let n = (2 * radius + 1) as f64;
    out.par_chunks_mut(w * 4)
        .zip(data.par_chunks(w * 4))
        .for_each(|(dst, row)| {
            let at = |x: i64, c: usize| -> f64 {
                let i = x.clamp(0, w as i64 - 1) as usize;
                f64::from(row.get(i * 4 + c).copied().unwrap_or(0.0))
            };
            for c in 0..4 {
                let r = radius as i64;
                let mut sum = 0.0f64;
                for k in -r..=r {
                    sum += at(k, c);
                }
                for x in 0..w {
                    if let Some(d) = dst.get_mut(x * 4 + c) {
                        *d = (sum / n) as f32;
                    }
                    let xi = x as i64;
                    sum += at(xi + r + 1, c) - at(xi - r, c);
                }
            }
        });
    out
}

/// Clamp-edged weighted row convolution with a symmetric kernel `k` (centre at `k.len() / 2`).
fn kernel_rows(data: &[f32], w: usize, k: &[f32]) -> Vec<f32> {
    let mut out = vec![0.0f32; data.len()];
    if w == 0 {
        return out;
    }
    let half = (k.len() / 2) as i64;
    out.par_chunks_mut(w * 4)
        .zip(data.par_chunks(w * 4))
        .for_each(|(dst, row)| {
            for x in 0..w {
                let mut acc = [0.0f32; 4];
                for (j, wt) in k.iter().enumerate() {
                    let sx = (x as i64 + j as i64 - half).clamp(0, w as i64 - 1) as usize;
                    for (c, a) in acc.iter_mut().enumerate() {
                        *a += wt * row.get(sx * 4 + c).copied().unwrap_or(0.0);
                    }
                }
                if let Some(d) = dst.get_mut(x * 4..x * 4 + 4) {
                    d.copy_from_slice(&acc);
                }
            }
        });
    out
}

/// A 3x3 weighted mean (1-2-1 kernel) blended with the original by `strength` (`0..=1`).
pub fn smooth(img: &Image, strength: f64) -> Result<Image, RasterError> {
    let s = num(strength, 0.0, 1.0, 0.0) as f32;
    if s == 0.0 {
        return Ok(img.clone());
    }
    let blurred = separable(img, |d, w| kernel_rows(d, w, &[0.25, 0.5, 0.25]))?;
    let mut out = img.clone();
    for (o, b) in out
        .as_mut_slice()
        .iter_mut()
        .zip(blurred.as_slice().iter())
    {
        *o += (*b - *o) * s;
    }
    Ok(out)
}

/// Box blur with the given radius in pixels (`1..=100`; 0 is the identity).
pub fn box_blur(img: &Image, radius: u32) -> Result<Image, RasterError> {
    if radius == 0 {
        return Ok(img.clone());
    }
    let r = radius.min(100) as usize;
    separable(img, |d, w| box_rows(d, w, r))
}

/// Gaussian blur with standard deviation `sigma` pixels (`0.3..=100`; 0 or less is the
/// identity). Small sigmas use a true kernel clipped at 3 sigma; larger ones use three
/// successive box blurs, which match a Gaussian closely and cost the same at any size.
pub fn gaussian(img: &Image, sigma: f64) -> Result<Image, RasterError> {
    let sigma = num(sigma, 0.0, 100.0, 0.0);
    if sigma <= 0.0 {
        return Ok(img.clone());
    }
    let sigma = sigma.max(0.3);
    if sigma < 2.0 {
        let radius = (sigma * 3.0).ceil() as i64;
        let mut k: Vec<f32> = (-radius..=radius)
            .map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp() as f32)
            .collect();
        let total: f32 = k.iter().sum();
        for v in &mut k {
            *v /= total;
        }
        return separable(img, |d, w| kernel_rows(d, w, &k));
    }
    // Box widths whose three-fold repeat approximates the Gaussian.
    let n = 3.0;
    let ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = ideal.floor() as i64;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wl = wl.max(1);
    let wu = wl + 2;
    let wlf = wl as f64;
    let m = ((12.0 * sigma * sigma - n * wlf * wlf - 4.0 * n * wlf - 3.0 * n) / (-4.0 * wlf - 4.0))
        .round() as i64;
    let mut cur = img.clone();
    for i in 0..3 {
        let width = if i < m { wl } else { wu };
        let r = ((width - 1) / 2).max(0) as usize;
        if r > 0 {
            cur = separable(&cur, |d, w| box_rows(d, w, r))?;
        }
    }
    Ok(cur)
}

// ---- Sampling-based filters --------------------------------------------------------------

const BLUR_SAMPLES: usize = 24;

/// Blur along lines toward the centre, `amount` (`0..=1`) of the way to it.
pub fn zoom_blur(img: &Image, cx: f64, cy: f64, amount: f64) -> Result<Image, RasterError> {
    let amount = num(amount, 0.0, 1.0, 0.0);
    if amount == 0.0 {
        return Ok(img.clone());
    }
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let (cx, cy) = (num(cx, -1.0, 2.0, 0.5) * w, num(cy, -1.0, 2.0, 0.5) * h);
    build(img.width(), img.height(), |x, y| {
        let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
        let mut acc = [0.0f32; 4];
        for k in 0..BLUR_SAMPLES {
            let f = amount * k as f64 / (BLUR_SAMPLES - 1) as f64;
            let p = img.sample_bilinear((px + (cx - px) * f) as f32, (py + (cy - py) * f) as f32);
            for (a, v) in acc.iter_mut().zip(p) {
                *a += v;
            }
        }
        acc.map(|v| v / BLUR_SAMPLES as f32)
    })
}

/// Blur along arcs around the centre, spreading over `angle` degrees (`0..=360`).
pub fn radial_blur(img: &Image, cx: f64, cy: f64, angle: f64) -> Result<Image, RasterError> {
    let angle = num(angle, 0.0, 360.0, 0.0).to_radians();
    if angle == 0.0 {
        return Ok(img.clone());
    }
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let (cx, cy) = (num(cx, -1.0, 2.0, 0.5) * w, num(cy, -1.0, 2.0, 0.5) * h);
    build(img.width(), img.height(), |x, y| {
        let (dx, dy) = (f64::from(x) + 0.5 - cx, f64::from(y) + 0.5 - cy);
        let mut acc = [0.0f32; 4];
        for k in 0..BLUR_SAMPLES {
            let a = angle * (k as f64 / (BLUR_SAMPLES - 1) as f64 - 0.5);
            let (s, c) = a.sin_cos();
            let p = img.sample_bilinear(
                (cx + dx * c - dy * s) as f32,
                (cy + dx * s + dy * c) as f32,
            );
            for (a, v) in acc.iter_mut().zip(p) {
                *a += v;
            }
        }
        acc.map(|v| v / BLUR_SAMPLES as f32)
    })
}

/// Unsharp mask: add `amount` (`0..=5`) times the difference from a Gaussian blur of the
/// given `radius` (`0.3..=100`).
pub fn sharpen(img: &Image, amount: f64, radius: f64) -> Result<Image, RasterError> {
    let amount = num(amount, 0.0, 5.0, 0.0) as f32;
    if amount == 0.0 {
        return Ok(img.clone());
    }
    let blurred = gaussian(img, num(radius, 0.3, 100.0, 1.0))?;
    let mut out = img.clone();
    for (o, b) in out
        .as_mut_slice()
        .chunks_exact_mut(4)
        .zip(blurred.as_slice().chunks_exact(4))
    {
        let p = [
            o[0] + amount * (o[0] - b[0]),
            o[1] + amount * (o[1] - b[1]),
            o[2] + amount * (o[2] - b[2]),
            o[3],
        ];
        o.copy_from_slice(&clamp_valid(p));
    }
    Ok(out)
}

/// Relief: the brightness difference across one pixel along `angle` degrees, scaled by
/// `depth` (`0..=10`) around a grey `base` (`0..=1`), with the image's own alpha.
pub fn emboss(img: &Image, angle: f64, depth: f64, base: f64) -> Result<Image, RasterError> {
    let a = num(angle, -360.0, 360.0, 0.0).to_radians();
    let depth = num(depth, 0.0, 10.0, 1.0) as f32;
    let base = num(base, 0.0, 1.0, 0.5) as f32;
    let (dx, dy) = (a.cos() as f32, -a.sin() as f32);
    build(img.width(), img.height(), |x, y| {
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let front = luminance(img.sample_bilinear(px + dx, py + dy));
        let back = luminance(img.sample_bilinear(px - dx, py - dy));
        let alpha = img.get(x, y).map_or(0.0, |p| p[3]);
        let v = (base + depth * (front - back)).clamp(0.0, 1.0) * alpha;
        [v, v, v, alpha]
    })
}

/// Replace every `cell x cell` block (`2..=512`, anchored at the top left) by its average.
pub fn mosaic(img: &Image, cell: u32) -> Result<Image, RasterError> {
    let cell = cell.clamp(2, 512) as usize;
    let (w, h) = dims(img);
    let cols = w.div_ceil(cell);
    let rows = h.div_ceil(cell);
    let data = img.as_slice();
    let averages: Vec<[f32; 4]> = (0..rows * cols)
        .into_par_iter()
        .map(|i| {
            let (cr, cc) = (i / cols, i % cols);
            let (x0, y0) = (cc * cell, cr * cell);
            let (x1, y1) = ((x0 + cell).min(w), (y0 + cell).min(h));
            let mut sum = [0.0f64; 4];
            for y in y0..y1 {
                for x in x0..x1 {
                    if let Some(p) = data.get((y * w + x) * 4..(y * w + x) * 4 + 4) {
                        for (s, v) in sum.iter_mut().zip(p) {
                            *s += f64::from(*v);
                        }
                    }
                }
            }
            let n = ((x1 - x0) * (y1 - y0)).max(1) as f64;
            sum.map(|s| (s / n) as f32)
        })
        .collect();
    build(img.width(), img.height(), |x, y| {
        let i = (y as usize / cell) * cols + x as usize / cell;
        averages.get(i).copied().unwrap_or_default()
    })
}

/// Oil paint: each pixel takes the average colour of the most common brightness level in
/// its `(2 radius + 1)` square window. `radius` is `1..=16`, `levels` `8..=256`.
pub fn oil_paint(img: &Image, radius: u32, levels: u32) -> Result<Image, RasterError> {
    if radius == 0 {
        return Ok(img.clone());
    }
    let r = radius.min(16) as i64;
    let levels = levels.clamp(8, 256) as usize;
    let (w, h) = (i64::from(img.width()), i64::from(img.height()));
    let mut out = Image::new(img.width(), img.height())?;
    let row_len = (w as usize) * 4;
    out.as_mut_slice()
        .par_chunks_mut(row_len)
        .enumerate()
        .for_each(|(y, row)| {
            let y = y as i64;
            let mut count = vec![0u32; levels];
            let mut sums = vec![[0.0f64; 4]; levels];
            let level_of = |p: Pixel| ((luminance(p) * (levels - 1) as f32) + 0.5) as usize;
            let column = |x: i64, count: &mut [u32], sums: &mut [[f64; 4]], sign: f64| {
                for yy in (y - r)..=(y + r) {
                    let p = img.get_clamped(x, yy.clamp(0, h - 1));
                    let l = level_of(p).min(levels - 1);
                    if sign > 0.0 {
                        count[l] += 1;
                    } else {
                        count[l] = count[l].saturating_sub(1);
                    }
                    for (s, v) in sums[l].iter_mut().zip(p) {
                        *s += sign * f64::from(v);
                    }
                }
            };
            for k in -r..=r {
                column(k.clamp(0, w - 1), &mut count, &mut sums, 1.0);
            }
            for x in 0..w {
                let mut best = 0usize;
                for (l, c) in count.iter().enumerate() {
                    if *c > count[best] {
                        best = l;
                    }
                }
                let n = f64::from(count[best].max(1));
                let p = clamp_valid(sums[best].map(|s| (s / n) as f32));
                if let Some(d) = row.get_mut(x as usize * 4..x as usize * 4 + 4) {
                    d.copy_from_slice(&p);
                }
                column((x - r).clamp(0, w - 1), &mut count, &mut sums, -1.0);
                column((x + r + 1).clamp(0, w - 1), &mut count, &mut sums, 1.0);
            }
        });
    Ok(out)
}

fn hash(x: u32, y: u32, seed: u64) -> u64 {
    let mut z = seed
        ^ (u64::from(x).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        ^ (u64::from(y).wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Scatter: every pixel reads a pseudo-random neighbour within `radius` pixels (`0..=100`).
/// The scatter is a hash of `(x, y, seed)`, so it is repeatable.
pub fn splash(img: &Image, radius: u32, seed: u64) -> Result<Image, RasterError> {
    if radius == 0 {
        return Ok(img.clone());
    }
    let r = i64::from(radius.min(100));
    let span = (2 * r + 1) as u64;
    build(img.width(), img.height(), |x, y| {
        let hv = hash(x, y, seed);
        let dx = (hv % span) as i64 - r;
        let dy = ((hv >> 32) % span) as i64 - r;
        img.get_clamped(i64::from(x) + dx, i64::from(y) + dy)
    })
}

/// Bend the image around a cylinder along one axis (`vertical` selects the axis the
/// cylinder stands on). Positive `amount` (`-1..=1`) bulges the middle outward; negative
/// pinches it.
pub fn cylinder(img: &Image, vertical: bool, amount: f64) -> Result<Image, RasterError> {
    let amount = num(amount, -1.0, 1.0, 0.0);
    if amount == 0.0 {
        return Ok(img.clone());
    }
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let a = amount.abs();
    let warp = |u: f64| -> f64 {
        if amount > 0.0 {
            (1.0 - a) * u + a * u.asin() * 2.0 / std::f64::consts::PI
        } else {
            (1.0 - a) * u + a * (u * std::f64::consts::FRAC_PI_2).sin()
        }
    };
    build(img.width(), img.height(), |x, y| {
        let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
        if vertical {
            let u = (py / h * 2.0 - 1.0).clamp(-1.0, 1.0);
            img.sample_bilinear(px as f32, ((warp(u) + 1.0) * 0.5 * h) as f32)
        } else {
            let u = (px / w * 2.0 - 1.0).clamp(-1.0, 1.0);
            img.sample_bilinear(((warp(u) + 1.0) * 0.5 * w) as f32, py as f32)
        }
    })
}

/// Twist the area within `radius` pixels of the centre by up to `angle` degrees
/// (`-720..=720`), strongest at the centre.
pub fn whirl(img: &Image, cx: f64, cy: f64, radius: f64, angle: f64) -> Result<Image, RasterError> {
    let radius = num(radius, 0.0, 100_000.0, 0.0);
    let angle = num(angle, -720.0, 720.0, 0.0).to_radians();
    if radius <= 0.0 || angle == 0.0 {
        return Ok(img.clone());
    }
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let (cx, cy) = (num(cx, -1.0, 2.0, 0.5) * w, num(cy, -1.0, 2.0, 0.5) * h);
    build(img.width(), img.height(), |x, y| {
        let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
        let (dx, dy) = (px - cx, py - cy);
        let d = (dx * dx + dy * dy).sqrt();
        if d >= radius {
            return img.get(x, y).unwrap_or_default();
        }
        let f = 1.0 - d / radius;
        let a = angle * f * f;
        let (s, c) = a.sin_cos();
        img.sample_bilinear((cx + dx * c - dy * s) as f32, (cy + dx * s + dy * c) as f32)
    })
}

/// Pinch (`amount` in `0..=1`) or bulge (`-1..=0`) the area within `radius` pixels of the
/// centre.
pub fn pinch(img: &Image, cx: f64, cy: f64, radius: f64, amount: f64) -> Result<Image, RasterError> {
    let radius = num(radius, 0.0, 100_000.0, 0.0);
    let amount = num(amount, -1.0, 1.0, 0.0);
    if radius <= 0.0 || amount == 0.0 {
        return Ok(img.clone());
    }
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let (cx, cy) = (num(cx, -1.0, 2.0, 0.5) * w, num(cy, -1.0, 2.0, 0.5) * h);
    build(img.width(), img.height(), |x, y| {
        let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
        let (dx, dy) = (px - cx, py - cy);
        let d = (dx * dx + dy * dy).sqrt();
        if d >= radius || d == 0.0 {
            return img.get(x, y).unwrap_or_default();
        }
        let s = (std::f64::consts::FRAC_PI_2 * d / radius).sin().powf(-amount);
        img.sample_bilinear((cx + dx * s) as f32, (cy + dy * s) as f32)
    })
}

/// Apply `f` to every pixel. Re-exported helper for callers that build their own ops.
pub fn map_pixels(img: &Image, f: impl Fn(Pixel) -> Pixel + Sync) -> Result<Image, RasterError> {
    map(img, f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dot(w: u32, h: u32) -> Image {
        let mut img = Image::new(w, h).unwrap();
        img.set(w / 2, h / 2, [1.0, 1.0, 1.0, 1.0]);
        img
    }

    fn ramp(w: u32, h: u32) -> Image {
        let mut img = Image::new(w, h).unwrap();
        for y in 0..h {
            for x in 0..w {
                let v = (x + y * w) as f32 / (w * h) as f32;
                img.set(x, y, [v, 1.0 - v, v * 0.5, 1.0]);
            }
        }
        img
    }

    fn valid(img: &Image) -> bool {
        img.as_slice().chunks_exact(4).all(|p| {
            p.iter().all(|v| v.is_finite()) && (0.0..=1.0).contains(&p[3])
        })
    }

    fn all(img: &Image) -> Vec<Image> {
        vec![
            smooth(img, 1.0).unwrap(),
            box_blur(img, 3).unwrap(),
            gaussian(img, 1.0).unwrap(),
            gaussian(img, 6.0).unwrap(),
            zoom_blur(img, 0.5, 0.5, 0.5).unwrap(),
            radial_blur(img, 0.5, 0.5, 30.0).unwrap(),
            sharpen(img, 1.0, 2.0).unwrap(),
            emboss(img, 45.0, 1.0, 0.5).unwrap(),
            mosaic(img, 3).unwrap(),
            oil_paint(img, 2, 32).unwrap(),
            splash(img, 4, 7).unwrap(),
            cylinder(img, false, 0.8).unwrap(),
            cylinder(img, true, -0.8).unwrap(),
            whirl(img, 0.5, 0.5, 20.0, 90.0).unwrap(),
            pinch(img, 0.5, 0.5, 20.0, 0.7).unwrap(),
            pinch(img, 0.5, 0.5, 20.0, -0.7).unwrap(),
        ]
    }

    #[test]
    fn every_filter_keeps_size_and_validity_on_odd_sizes() {
        for (w, h) in [(1, 1), (1, 7), (9, 1), (5, 3), (13, 11)] {
            for img in [dot(w, h), ramp(w, h)] {
                for out in all(&img) {
                    assert_eq!(out.size(), (w, h));
                    assert!(valid(&out), "{w}x{h}");
                }
            }
        }
    }

    #[test]
    fn deterministic() {
        let img = ramp(17, 9);
        let a = all(&img);
        let b = all(&img);
        assert_eq!(a, b);
    }

    #[test]
    fn neutral_parameters_are_exact_identity() {
        let img = ramp(7, 5);
        assert_eq!(smooth(&img, 0.0).unwrap(), img);
        assert_eq!(box_blur(&img, 0).unwrap(), img);
        assert_eq!(gaussian(&img, 0.0).unwrap(), img);
        assert_eq!(zoom_blur(&img, 0.5, 0.5, 0.0).unwrap(), img);
        assert_eq!(radial_blur(&img, 0.5, 0.5, 0.0).unwrap(), img);
        assert_eq!(sharpen(&img, 0.0, 3.0).unwrap(), img);
        assert_eq!(oil_paint(&img, 0, 64).unwrap(), img);
        assert_eq!(splash(&img, 0, 1).unwrap(), img);
        assert_eq!(cylinder(&img, false, 0.0).unwrap(), img);
        assert_eq!(whirl(&img, 0.5, 0.5, 0.0, 90.0).unwrap(), img);
        assert_eq!(pinch(&img, 0.5, 0.5, 10.0, 0.0).unwrap(), img);
    }

    #[test]
    fn nan_parameters_are_neutral() {
        let img = ramp(6, 6);
        assert_eq!(gaussian(&img, f64::NAN).unwrap(), img);
        assert_eq!(whirl(&img, f64::NAN, f64::NAN, f64::NAN, f64::NAN).unwrap(), img);
        assert!(valid(&emboss(&img, f64::NAN, f64::NAN, f64::NAN).unwrap()));
        assert!(valid(&zoom_blur(&img, f64::NAN, f64::NAN, 1.0).unwrap()));
    }

    #[test]
    fn blur_spreads_a_bright_pixel_and_keeps_the_total() {
        let img = dot(9, 9);
        for out in [
            box_blur(&img, 1).unwrap(),
            gaussian(&img, 1.0).unwrap(),
            smooth(&img, 1.0).unwrap(),
        ] {
            let centre = out.get(4, 4).unwrap()[0];
            let side = out.get(5, 4).unwrap()[0];
            assert!(centre < 1.0 && side > 0.0 && centre >= side);
            let total: f32 = out.as_slice().chunks_exact(4).map(|p| p[0]).sum();
            assert!((total - 1.0).abs() < 1e-4, "{total}");
        }
        let b = box_blur(&img, 1).unwrap();
        assert!((b.get(4, 4).unwrap()[0] - 1.0 / 9.0).abs() < 1e-6);
    }

    #[test]
    fn large_gaussian_spreads_and_keeps_flat_images_flat() {
        let flat = Image::filled(20, 20, [0.4, 0.4, 0.4, 1.0]).unwrap();
        let out = gaussian(&flat, 8.0).unwrap();
        assert!(out.as_slice().iter().zip(flat.as_slice()).all(|(a, b)| (a - b).abs() < 1e-5));
        let out = gaussian(&dot(41, 41), 5.0).unwrap();
        assert!(out.get(20, 20).unwrap()[0] < 0.1);
        assert!(out.get(25, 20).unwrap()[0] > 0.0);
    }

    #[test]
    fn mosaic_is_the_cell_average() {
        let mut img = Image::new(4, 4).unwrap();
        img.set(0, 0, [1.0, 1.0, 1.0, 1.0]);
        let out = mosaic(&img, 2).unwrap();
        assert_eq!(out.get(1, 1), Some([0.25, 0.25, 0.25, 0.25]));
        assert_eq!(out.get(2, 2), Some([0.0; 4]));
    }

    #[test]
    fn sharpen_increases_edge_contrast() {
        let mut img = Image::filled(8, 1, [0.2, 0.2, 0.2, 1.0]).unwrap();
        for x in 4..8 {
            img.set(x, 0, [0.8, 0.8, 0.8, 1.0]);
        }
        let out = sharpen(&img, 1.0, 1.0).unwrap();
        assert!(out.get(3, 0).unwrap()[0] < 0.2);
        assert!(out.get(4, 0).unwrap()[0] > 0.8);
    }

    #[test]
    fn emboss_of_flat_image_is_base_grey() {
        let flat = Image::filled(5, 5, [0.3, 0.6, 0.9, 1.0]).unwrap();
        let out = emboss(&flat, 45.0, 2.0, 0.5).unwrap();
        assert_eq!(out.get(2, 2), Some([0.5, 0.5, 0.5, 1.0]));
    }

    #[test]
    fn oil_paint_picks_the_dominant_colour() {
        let mut img = Image::filled(5, 5, [0.0, 0.0, 0.0, 1.0]).unwrap();
        img.set(2, 2, [1.0, 1.0, 1.0, 1.0]);
        let out = oil_paint(&img, 1, 16).unwrap();
        assert_eq!(out.get(2, 2), Some([0.0, 0.0, 0.0, 1.0]));
    }

    #[test]
    fn splash_depends_on_seed_but_stays_in_range() {
        let img = ramp(16, 16);
        let a = splash(&img, 5, 1).unwrap();
        let b = splash(&img, 5, 2).unwrap();
        assert_ne!(a, b);
        assert_eq!(a, splash(&img, 5, 1).unwrap());
    }

    #[test]
    fn whirl_and_pinch_leave_the_far_field_alone() {
        let img = ramp(21, 21);
        let w = whirl(&img, 0.5, 0.5, 5.0, 180.0).unwrap();
        assert_eq!(w.get(0, 0), img.get(0, 0));
        assert_ne!(w.get(12, 10), img.get(12, 10));
        let p = pinch(&img, 0.5, 0.5, 5.0, 0.5).unwrap();
        assert_eq!(p.get(20, 20), img.get(20, 20));
    }

    #[test]
    fn pinch_pulls_content_in_and_bulge_pushes_out() {
        // A bright centre dot: pinching makes it look thinner (less bright area), bulging
        // makes it larger.
        let mut img = Image::new(41, 41).unwrap();
        for y in 18..23 {
            for x in 18..23 {
                img.set(x, y, [1.0, 1.0, 1.0, 1.0]);
            }
        }
        let area = |i: &Image| -> f32 { i.as_slice().chunks_exact(4).map(|p| p[0]).sum() };
        let base = area(&img);
        assert!(area(&pinch(&img, 0.5, 0.5, 18.0, 0.8).unwrap()) < base);
        assert!(area(&pinch(&img, 0.5, 0.5, 18.0, -0.8).unwrap()) > base);
    }
}
