//! Landmark detection for subjects that are not faces: well-spread corner features.
//!
//! Corners are found with the minimum eigenvalue of the local structure tensor (the
//! Shi-Tomasi measure, a published method): where the picture changes in two directions at
//! once the response is high. The strongest ones are picked greedily with a minimum spacing, so
//! the dots spread over the subject instead of piling up on one busy spot. Everything is
//! deterministic: ties break by position.

use inbetween_geom::Point;
use inbetween_raster::{luminance, Image};

/// Detection runs on a copy no larger than this on its longest side.
const WORK_SIDE: u32 = 1024;
/// Half-width of the window the structure tensor sums over.
const WINDOW: i64 = 2;
/// A corner must beat every response within this many pixels (on the working copy).
const NEIGHBOURHOOD: i64 = 3;

/// Settings for [`detect_corners`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CornerOptions {
    /// Most corners to return.
    pub max_corners: usize,
    /// A corner's response must be at least this fraction (0 to 1) of the strongest one's.
    pub quality: f64,
    /// Smallest distance between two corners, in pixels of `img`.
    pub min_distance: f64,
}

impl Default for CornerOptions {
    fn default() -> Self {
        Self {
            max_corners: 40,
            quality: 0.05,
            min_distance: 12.0,
        }
    }
}

/// Grey levels of `img` as a flat row-major array, premultiplied (transparent pixels are dark).
fn grey(img: &Image) -> Vec<f32> {
    img.as_slice()
        .chunks_exact(4)
        .map(|p| luminance([p[0], p[1], p[2], p[3]]))
        .collect()
}

/// A 3x3 binomial blur with clamped edges.
fn blur(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let at = |x: i64, y: i64| -> f32 {
        let x = x.clamp(0, w as i64 - 1) as usize;
        let y = y.clamp(0, h as i64 - 1) as usize;
        src.get(y * w + x).copied().unwrap_or(0.0)
    };
    const K: [f32; 3] = [1.0, 2.0, 1.0];
    let mut out = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            for (j, ky) in K.iter().enumerate() {
                for (i, kx) in K.iter().enumerate() {
                    sum += ky * kx * at(x as i64 + i as i64 - 1, y as i64 + j as i64 - 1);
                }
            }
            if let Some(o) = out.get_mut(y * w + x) {
                *o = sum / 16.0;
            }
        }
    }
    out
}

/// The corner response (smaller eigenvalue of the structure tensor) at every pixel.
fn response(grey: &[f32], w: usize, h: usize) -> Vec<f32> {
    let smooth = blur(grey, w, h);
    let at = |x: i64, y: i64| -> f32 {
        let x = x.clamp(0, w as i64 - 1) as usize;
        let y = y.clamp(0, h as i64 - 1) as usize;
        smooth.get(y * w + x).copied().unwrap_or(0.0)
    };
    // Sobel gradients and their products.
    let mut ixx = vec![0.0f32; w * h];
    let mut iyy = vec![0.0f32; w * h];
    let mut ixy = vec![0.0f32; w * h];
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
            let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
            let i = y as usize * w + x as usize;
            ixx[i] = gx * gx;
            iyy[i] = gy * gy;
            ixy[i] = gx * gy;
        }
    }
    let sum = |a: &[f32], x: i64, y: i64| -> f32 {
        let mut s = 0.0;
        for j in -WINDOW..=WINDOW {
            for i in -WINDOW..=WINDOW {
                let xx = (x + i).clamp(0, w as i64 - 1) as usize;
                let yy = (y + j).clamp(0, h as i64 - 1) as usize;
                s += a.get(yy * w + xx).copied().unwrap_or(0.0);
            }
        }
        s
    };
    let mut out = vec![0.0f32; w * h];
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let (a, b, c) = (sum(&ixx, x, y), sum(&iyy, x, y), sum(&ixy, x, y));
            let trace = a + b;
            let det = (a - b) * (a - b) + 4.0 * c * c;
            let r = (trace - det.max(0.0).sqrt()) / 2.0;
            if let Some(o) = out.get_mut(y as usize * w + x as usize) {
                *o = if r.is_finite() { r.max(0.0) } else { 0.0 };
            }
        }
    }
    out
}

/// The strongest, well-spread corners of `img`, strongest first, as pixel positions (the
/// centre of a pixel is `x + 0.5`). Empty for a flat or tiny picture.
pub fn detect_corners(img: &Image, opts: &CornerOptions) -> Vec<Point> {
    let (iw, ih) = img.size();
    if iw < 8 || ih < 8 || opts.max_corners == 0 {
        return Vec::new();
    }
    // Work on a reduced copy so a huge frame costs no more than a modest one.
    let (work, scale) = match img.downscale_to_fit(WORK_SIDE) {
        Ok(small) if small.size() != img.size() => {
            let k = f64::from(iw) / f64::from(small.width().max(1));
            (small, k)
        }
        _ => (img.clone(), 1.0),
    };
    let (w, h) = (work.width() as usize, work.height() as usize);
    let g = grey(&work);
    let r = response(&g, w, h);
    let max = r.iter().copied().fold(0.0f32, f32::max);
    if max.is_nan() || max <= 1e-9 {
        return Vec::new();
    }
    let quality = if opts.quality.is_finite() {
        opts.quality.clamp(0.0, 1.0)
    } else {
        0.05
    };
    let floor = (f64::from(max) * quality).max(1e-9) as f32;
    // Local maxima over the neighbourhood; equal responses go to the first in reading order.
    let mut candidates: Vec<(f32, usize, usize)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let v = r[y * w + x];
            if v < floor {
                continue;
            }
            let mut is_max = true;
            'scan: for j in -NEIGHBOURHOOD..=NEIGHBOURHOOD {
                for i in -NEIGHBOURHOOD..=NEIGHBOURHOOD {
                    if i == 0 && j == 0 {
                        continue;
                    }
                    let (xx, yy) = (x as i64 + i, y as i64 + j);
                    if xx < 0 || yy < 0 || xx >= w as i64 || yy >= h as i64 {
                        continue;
                    }
                    let o = r[yy as usize * w + xx as usize];
                    let earlier = (yy, xx) < (y as i64, x as i64);
                    if o > v || (o == v && earlier) {
                        is_max = false;
                        break 'scan;
                    }
                }
            }
            if is_max {
                candidates.push((v, x, y));
            }
        }
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.2.cmp(&b.2)).then(a.1.cmp(&b.1)));
    let spacing = if opts.min_distance.is_finite() {
        (opts.min_distance.max(0.0) / scale).max(1.0)
    } else {
        1.0
    };
    let mut picked: Vec<(f64, f64)> = Vec::new();
    for (_, x, y) in candidates {
        if picked.len() >= opts.max_corners {
            break;
        }
        let p = (x as f64 + 0.5, y as f64 + 0.5);
        if picked
            .iter()
            .all(|q| (q.0 - p.0).hypot(q.1 - p.1) >= spacing)
        {
            picked.push(p);
        }
    }
    picked
        .into_iter()
        .map(|(x, y)| Point::new(x * scale, y * scale))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bright square on a dark ground: four sharp corners.
    fn square(w: u32, h: u32, x0: u32, y0: u32, x1: u32, y1: u32) -> Image {
        let mut img = Image::new(w, h).unwrap();
        for y in 0..h {
            for x in 0..w {
                let v = if (x0..x1).contains(&x) && (y0..y1).contains(&y) {
                    0.9
                } else {
                    0.1
                };
                img.set(x, y, [v, v, v, 1.0]);
            }
        }
        img
    }

    #[test]
    fn a_square_gives_its_four_corners() {
        let img = square(96, 72, 24, 18, 72, 54);
        let found = detect_corners(
            &img,
            &CornerOptions {
                max_corners: 4,
                ..CornerOptions::default()
            },
        );
        assert_eq!(found.len(), 4, "{found:?}");
        for want in [(24.0, 18.0), (72.0, 18.0), (72.0, 54.0), (24.0, 54.0)] {
            assert!(
                found
                    .iter()
                    .any(|p| p.distance(Point::new(want.0, want.1)) < 3.0),
                "no corner near {want:?} in {found:?}"
            );
        }
    }

    #[test]
    fn corners_are_spread_by_the_minimum_distance_and_capped() {
        let img = inbetween_testkit::labelled_grid(160, 120, 10).unwrap();
        let opts = CornerOptions {
            max_corners: 25,
            quality: 0.01,
            min_distance: 20.0,
        };
        let found = detect_corners(&img, &opts);
        assert!(found.len() <= 25);
        assert!(found.len() >= 5, "{}", found.len());
        for (i, a) in found.iter().enumerate() {
            for b in &found[i + 1..] {
                assert!(a.distance(*b) >= 19.0, "{a:?} {b:?}");
            }
            assert!(a.x >= 0.0 && a.x <= 160.0 && a.y >= 0.0 && a.y <= 120.0);
        }
        // Deterministic.
        assert_eq!(found, detect_corners(&img, &opts));
    }

    #[test]
    fn flat_tiny_and_hostile_inputs_give_nothing_and_do_not_panic() {
        let flat = Image::filled(64, 64, [0.5, 0.5, 0.5, 1.0]).unwrap();
        assert!(detect_corners(&flat, &CornerOptions::default()).is_empty());
        let tiny = Image::new(4, 4).unwrap();
        assert!(detect_corners(&tiny, &CornerOptions::default()).is_empty());
        let img = square(64, 64, 10, 10, 50, 50);
        for opts in [
            CornerOptions {
                max_corners: 0,
                ..CornerOptions::default()
            },
            CornerOptions {
                quality: f64::NAN,
                ..CornerOptions::default()
            },
            CornerOptions {
                quality: -3.0,
                ..CornerOptions::default()
            },
            CornerOptions {
                quality: 9.0,
                ..CornerOptions::default()
            },
            CornerOptions {
                min_distance: f64::NAN,
                ..CornerOptions::default()
            },
            CornerOptions {
                min_distance: -5.0,
                ..CornerOptions::default()
            },
            CornerOptions {
                min_distance: 1e30,
                ..CornerOptions::default()
            },
            CornerOptions {
                max_corners: usize::MAX,
                ..CornerOptions::default()
            },
        ] {
            let found = detect_corners(&img, &opts);
            assert!(found.iter().all(|p| p.is_finite()), "{opts:?}");
        }
    }

    #[test]
    fn a_big_frame_is_detected_on_a_reduced_copy_and_mapped_back() {
        let img = square(2048, 1536, 512, 384, 1536, 1152);
        let found = detect_corners(
            &img,
            &CornerOptions {
                max_corners: 4,
                quality: 0.1,
                min_distance: 100.0,
            },
        );
        assert_eq!(found.len(), 4, "{found:?}");
        for want in [
            (512.0, 384.0),
            (1536.0, 384.0),
            (1536.0, 1152.0),
            (512.0, 1152.0),
        ] {
            assert!(
                found
                    .iter()
                    .any(|p| p.distance(Point::new(want.0, want.1)) < 8.0),
                "no corner near {want:?} in {found:?}"
            );
        }
    }
}
