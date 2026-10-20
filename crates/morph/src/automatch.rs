//! Auto-match: guess where a lone dot's partner belongs, then refine the guess by comparing
//! small patches of the two images.

use inbetween_doc::Sequence;
use inbetween_geom::{triangulate, Point, Triangulation};
use inbetween_raster::Image;
use rayon::prelude::*;

/// Half-width of the compared patch (the patch is `2 * PATCH + 1` pixels square).
const PATCH: i64 = 6;
/// How far from the guess the refinement looks, in pixels.
const SEARCH: i64 = 12;
/// A match weaker than this normalised correlation keeps the guess.
const MIN_SCORE: f64 = 0.6;
/// Patches flatter than this (sum of squared deviations) carry no information.
const MIN_ENERGY: f64 = 1e-6;

/// Maps a point of one image to the other through the mesh of the paired dots (plus the four
/// frame corners). Built once, used for many points.
pub struct Transfer {
    mesh: Option<Triangulation>,
    target: Vec<Point>,
}

impl Transfer {
    /// `from_image` is 1 or 2: the image the points to map are on. With fewer than three
    /// paired dots, or dots the triangulator refuses, the map is the identity.
    pub fn new(seq: &Sequence, from_image: u8, width: u32, height: u32) -> Self {
        let (from, to) = if from_image == 1 {
            (seq.dots.first_positions(), seq.dots.second_positions())
        } else {
            (seq.dots.second_positions(), seq.dots.first_positions())
        };
        let (w, h) = (f64::from(width), f64::from(height));
        let mesh = if from.len() >= 3 {
            triangulate(&from, w, h).ok()
        } else {
            None
        };
        let mut target = to;
        target.extend([
            Point::new(0.0, 0.0),
            Point::new(w, 0.0),
            Point::new(w, h),
            Point::new(0.0, h),
        ]);
        Self { mesh, target }
    }

    /// The partner position for `p`. Outside the mesh the nearest vertex's displacement is used.
    pub fn apply(&self, p: Point) -> Point {
        let Some(mesh) = &self.mesh else {
            return p;
        };
        if !p.is_finite() {
            return p;
        }
        for (i, idx) in mesh.triangles.iter().enumerate() {
            let (Some(tri), Some(t0), Some(t1), Some(t2)) = (
                mesh.triangle(i),
                self.target.get(idx[0]),
                self.target.get(idx[1]),
                self.target.get(idx[2]),
            ) else {
                continue;
            };
            if let Some(w) = tri.barycentric(p) {
                if w.iter().all(|&v| v >= -1e-9) {
                    let q = *t0 * w[0] + *t1 * w[1] + *t2 * w[2];
                    if q.is_finite() {
                        return q;
                    }
                }
            }
        }
        // Outside the hull: follow the nearest vertex.
        let nearest = mesh
            .vertices
            .iter()
            .zip(&self.target)
            .min_by(|a, b| a.0.distance(p).total_cmp(&b.0.distance(p)));
        match nearest {
            Some((v, t)) => {
                let q = p + (*t - *v);
                if q.is_finite() {
                    q
                } else {
                    p
                }
            }
            None => p,
        }
    }
}

/// Map one point of image `from_image` to the other image through the paired dots of `seq`.
pub fn transfer_point(seq: &Sequence, p: Point, from_image: u8, width: u32, height: u32) -> Point {
    Transfer::new(seq, from_image, width, height).apply(p)
}

fn luma(img: &Image, x: i64, y: i64) -> f64 {
    // Premultiplied, so the colour channels already carry the alpha.
    let px = img.get_clamped(x, y);
    let v = 0.299 * f64::from(px[0]) + 0.587 * f64::from(px[1]) + 0.114 * f64::from(px[2]);
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

fn patch(img: &Image, cx: i64, cy: i64) -> Vec<f64> {
    let n = (2 * PATCH + 1) as usize;
    let mut v = Vec::with_capacity(n * n);
    for dy in -PATCH..=PATCH {
        for dx in -PATCH..=PATCH {
            v.push(luma(img, cx.saturating_add(dx), cy.saturating_add(dy)));
        }
    }
    v
}

/// Zero-mean version of a patch and its energy.
fn centred(v: &[f64]) -> (Vec<f64>, f64) {
    let mean = v.iter().sum::<f64>() / v.len().max(1) as f64;
    let c: Vec<f64> = v.iter().map(|x| x - mean).collect();
    let e = c.iter().map(|x| x * x).sum();
    (c, e)
}

fn round(v: f64) -> i64 {
    if v.is_finite() {
        v.round().clamp(-1.0e9, 1.0e9) as i64
    } else {
        0
    }
}

/// Refine `guess` (a position on `other`) for the dot at `at` on `from`.
fn refine(from: &Image, other: &Image, at: Point, guess: Point) -> Point {
    refine_scored(from, other, at, guess).0
}

/// Like [`refine`], and the correlation of the match when the patches really matched (`None`
/// when the guess was kept because the patch was flat or nothing correlated well enough).
fn refine_scored(from: &Image, other: &Image, at: Point, guess: Point) -> (Point, Option<f64>) {
    let (ax, ay) = (round(at.x), round(at.y));
    let (gx, gy) = (round(guess.x), round(guess.y));
    let (tpl, te) = centred(&patch(from, ax, ay));
    if te < MIN_ENERGY {
        return (guess, None);
    }
    let mut best = (f64::MIN, gx, gy);
    for dy in -SEARCH..=SEARCH {
        for dx in -SEARCH..=SEARCH {
            let (cx, cy) = (gx + dx, gy + dy);
            let (cand, ce) = centred(&patch(other, cx, cy));
            if ce < MIN_ENERGY {
                continue;
            }
            let dot: f64 = tpl.iter().zip(&cand).map(|(a, b)| a * b).sum();
            let score = dot / (te * ce).sqrt();
            // Strictly greater: the first of equal scores wins, so the result is deterministic.
            if score > best.0 {
                best = (score, cx, cy);
            }
        }
    }
    if best.0 >= MIN_SCORE {
        (Point::new(best.1 as f64, best.2 as f64), Some(best.0))
    } else {
        (guess, None)
    }
}

/// For each `(dot position, from_image)` in `dots`, the refined partner position on the other
/// image. `img1` and `img2` must be the two images at output size (frame 0 and the last frame
/// of the morph give exactly that). `seq` supplies the paired dots for the first guess.
pub fn auto_match(seq: &Sequence, img1: &Image, img2: &Image, dots: &[(Point, u8)]) -> Vec<Point> {
    let (w, h) = img1.size();
    let to2 = Transfer::new(seq, 1, w, h);
    let to1 = Transfer::new(seq, 2, w, h);
    dots.par_iter()
        .map(|&(at, from)| {
            if from == 1 {
                refine(img1, img2, at, to2.apply(at))
            } else {
                refine(img2, img1, at, to1.apply(at))
            }
        })
        .collect()
}

/// Like [`auto_match`], with each position's correlation: `Some` when the patches matched,
/// `None` when the position is only the first guess.
pub fn auto_match_scored(
    seq: &Sequence,
    img1: &Image,
    img2: &Image,
    dots: &[(Point, u8)],
) -> Vec<(Point, Option<f64>)> {
    let (w, h) = img1.size();
    let to2 = Transfer::new(seq, 1, w, h);
    let to1 = Transfer::new(seq, 2, w, h);
    dots.par_iter()
        .map(|&(at, from)| {
            if from == 1 {
                refine_scored(img1, img2, at, to2.apply(at))
            } else {
                refine_scored(img2, img1, at, to1.apply(at))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::DotPair;
    use inbetween_testkit::labelled_grid;

    fn shifted(img: &Image, dx: i64, dy: i64) -> Image {
        let (w, h) = img.size();
        let mut out = Image::new(w, h).unwrap();
        for y in 0..h {
            for x in 0..w {
                out.set(x, y, img.get_clamped(i64::from(x) - dx, i64::from(y) - dy));
            }
        }
        out
    }

    fn corners(n: usize) -> Sequence {
        let mut s = Sequence::default();
        for (x, y) in [(0.0, 0.0), (63.0, 0.0), (63.0, 47.0), (0.0, 47.0)]
            .into_iter()
            .take(n)
        {
            s.dots.pairs.push(DotPair::fixed(Point::new(x, y)));
        }
        s
    }

    #[test]
    fn recovers_a_known_shift() {
        let a = labelled_grid(64, 48, 8).unwrap();
        let b = shifted(&a, 4, -3);
        let seq = corners(4);
        let at = Point::new(30.0, 24.0);
        let got = auto_match(&seq, &a, &b, &[(at, 1)]);
        assert!(
            got[0].distance(Point::new(34.0, 21.0)) <= 1.0,
            "{:?}",
            got[0]
        );
        let back = auto_match(&seq, &a, &b, &[(Point::new(34.0, 21.0), 2)]);
        assert!(back[0].distance(at) <= 1.0, "{:?}", back[0]);
        assert_eq!(got, auto_match(&seq, &a, &b, &[(at, 1)]));
    }

    #[test]
    fn transfer_follows_the_mesh_and_degenerates_to_identity() {
        let mut seq = corners(4);
        let p = Point::new(10.0, 10.0);
        assert_eq!(transfer_point(&corners(2), p, 1, 64, 48), p);
        assert_eq!(transfer_point(&Sequence::default(), p, 2, 64, 48), p);
        for pair in &mut seq.dots.pairs {
            pair.second.x += 5.0;
        }
        let q = transfer_point(&seq, p, 1, 64, 48);
        assert!(
            (q.x - 15.0).abs() < 1e-6 && (q.y - 10.0).abs() < 1e-6,
            "{q:?}"
        );
        let far = transfer_point(&seq, Point::new(1e12, -1e12), 1, 64, 48);
        assert!(far.is_finite());
        assert!(transfer_point(&seq, Point::new(f64::NAN, 0.0), 1, 64, 48)
            .x
            .is_nan());
    }

    #[test]
    fn blank_and_hostile_inputs_keep_the_guess() {
        let blank = Image::new(32, 32).unwrap();
        let seq = Sequence::default();
        let at = Point::new(5.0, 5.0);
        assert_eq!(auto_match(&seq, &blank, &blank, &[(at, 1)]), vec![at]);
        let out = auto_match(
            &seq,
            &blank,
            &blank,
            &[
                (Point::new(f64::NAN, 1e300), 1),
                (Point::new(-1e300, f64::INFINITY), 2),
            ],
        );
        assert_eq!(out.len(), 2);
        assert!(auto_match(&seq, &blank, &blank, &[]).is_empty());
    }
}
