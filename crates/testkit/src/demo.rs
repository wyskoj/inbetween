//! A generated demo pair with hand-placed dots: a round sun face and a boxy moon face.
//!
//! Both images are drawn from formulas (no photographs, nothing third-party), so they are
//! free of any licence. [`dot_pairs`] returns dots that match the same features on both:
//! eight points around each head, both eyes, and three points on the mouth, plus twelve
//! pinned dots around the frame edge so the background stays put.

use inbetween_raster::{Image, RasterError};

type Rgb = [f32; 3];

/// One face, with every size a fraction of the image height (and centre of the width).
struct Face {
    /// Head shape exponent: 2 is a circle, larger is boxier.
    n: f64,
    /// Head half-width and half-height, as fractions of the image height.
    a: f64,
    b: f64,
    /// Vertical centre of the head as a fraction of the image height.
    cy: f64,
    /// Eye offset from the head centre: (sideways, vertical), and radius.
    eye: (f64, f64, f64),
    /// Mouth centre offset (vertical), half-width and half-height.
    mouth: (f64, f64, f64),
    smile_only: bool,
    bg: (Rgb, Rgb),
    head: (Rgb, Rgb),
    ink: Rgb,
    mouth_ink: Rgb,
}

fn sun() -> Face {
    Face {
        n: 2.0,
        a: 0.36,
        b: 0.36,
        cy: 0.50,
        eye: (0.14, -0.08, 0.05),
        mouth: (0.12, 0.17, 0.10),
        smile_only: true,
        bg: ([0.47, 0.75, 1.0], [1.0, 0.89, 0.67]),
        head: ([1.0, 0.84, 0.25], [0.94, 0.59, 0.08]),
        ink: [0.20, 0.12, 0.08],
        mouth_ink: [0.59, 0.12, 0.12],
    }
}

fn moon() -> Face {
    Face {
        n: 5.0,
        a: 0.46,
        b: 0.34,
        cy: 0.53,
        eye: (0.20, -0.02, 0.065),
        mouth: (0.15, 0.10, 0.10),
        smile_only: false,
        bg: ([0.27, 0.20, 0.47], [0.59, 0.35, 0.63]),
        head: ([0.47, 0.90, 0.75], [0.16, 0.59, 0.59]),
        ink: [0.08, 0.12, 0.20],
        mouth_ink: [0.10, 0.20, 0.30],
    }
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

impl Face {
    fn colour_at(&self, x: f64, y: f64, w: f64, h: f64) -> Rgb {
        let (cx, cy) = (0.5 * w, self.cy * h);
        let (dx, dy) = (x - cx, y - cy);
        let mut c = mix(self.bg.0, self.bg.1, (y / h) as f32);

        let d = (dx.abs() / (self.a * h)).powf(self.n) + (dy.abs() / (self.b * h)).powf(self.n);
        if d <= 1.0 {
            // Darker toward the rim for a little roundness.
            c = mix(self.head.0, self.head.1, (d * d) as f32);
            let (ex, ey, er) = (self.eye.0 * h, self.eye.1 * h, self.eye.2 * h);
            for side in [-1.0, 1.0] {
                let (ox, oy) = (dx - side * ex, dy - ey);
                if ox * ox + oy * oy <= er * er {
                    c = self.ink;
                }
            }
            let (my, rx, ry) = (self.mouth.0 * h, self.mouth.1 * h, self.mouth.2 * h);
            let q = ((dx / rx).powi(2) + ((dy - my) / ry).powi(2)).sqrt();
            if (q - 1.0).abs() <= 0.16 && (!self.smile_only || dy >= my) {
                c = self.mouth_ink;
            }
        }
        c
    }

    fn render(&self, width: u32, height: u32) -> Result<Image, RasterError> {
        Image::new(width, height)?;
        let (w, h) = (f64::from(width), f64::from(height));
        let mut buf = Vec::with_capacity(width as usize * height as usize * 4);
        for py in 0..height {
            for px in 0..width {
                // 3x3 supersampling for smooth edges.
                let mut acc = [0.0_f32; 3];
                for sy in 0..3 {
                    for sx in 0..3 {
                        let x = f64::from(px) + (f64::from(sx) + 0.5) / 3.0;
                        let y = f64::from(py) + (f64::from(sy) + 0.5) / 3.0;
                        let c = self.colour_at(x, y, w, h);
                        for (a, v) in acc.iter_mut().zip(c) {
                            *a += v / 9.0;
                        }
                    }
                }
                buf.extend(
                    acc.iter()
                        .map(|v| (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8),
                );
                buf.push(255);
            }
        }
        Image::from_rgba8(width, height, &buf)
    }

    /// Landmark positions in pixels, in a fixed order shared by every face.
    fn anchors(&self, width: u32, height: u32) -> Vec<(f64, f64)> {
        let (w, h) = (f64::from(width), f64::from(height));
        let (cx, cy) = (0.5 * w, self.cy * h);
        let mut out = Vec::new();
        // Eight points around the head, every 45 degrees, starting at the right.
        for k in 0..8 {
            let angle = f64::from(k) * std::f64::consts::FRAC_PI_4;
            let (ux, uy) = (angle.cos(), angle.sin());
            let norm = (ux.abs().powf(self.n) + uy.abs().powf(self.n)).powf(1.0 / self.n);
            out.push((cx + self.a * h * ux / norm, cy + self.b * h * uy / norm));
        }
        let (ex, ey) = (self.eye.0 * h, self.eye.1 * h);
        out.push((cx - ex, cy + ey));
        out.push((cx + ex, cy + ey));
        let (my, rx, ry) = (self.mouth.0 * h, self.mouth.1 * h, self.mouth.2 * h);
        out.push((cx - rx, cy + my));
        out.push((cx + rx, cy + my));
        out.push((cx, cy + my + ry));
        out
    }
}

/// The two demo images at the given size.
pub fn pair(width: u32, height: u32) -> Result<(Image, Image), RasterError> {
    Ok((sun().render(width, height)?, moon().render(width, height)?))
}

/// Hand-placed dot pairs `((x on Image 1, y on Image 1), (x on Image 2, y on Image 2))`.
pub fn dot_pairs(width: u32, height: u32) -> Vec<((f64, f64), (f64, f64))> {
    let (w, h) = (f64::from(width), f64::from(height));
    let mut pairs: Vec<_> = sun()
        .anchors(width, height)
        .into_iter()
        .zip(moon().anchors(width, height))
        .collect();
    for p in [
        (0.25 * w, 0.0),
        (0.5 * w, 0.0),
        (0.75 * w, 0.0),
        (0.25 * w, h),
        (0.5 * w, h),
        (0.75 * w, h),
        (0.0, 0.25 * h),
        (0.0, 0.5 * h),
        (0.0, 0.75 * h),
        (w, 0.25 * h),
        (w, 0.5 * h),
        (w, 0.75 * h),
    ] {
        pairs.push((p, p));
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_are_opaque_and_different() {
        let (a, b) = pair(120, 90).unwrap();
        assert_eq!(a.size(), (120, 90));
        assert!(a.as_slice().chunks_exact(4).all(|p| p[3] == 1.0));
        assert_ne!(a, b);
        // The sun's face centre is yellow-ish, the moon's is green-ish.
        let (ca, cb) = (a.get(60, 40).unwrap(), b.get(60, 40).unwrap());
        assert!(ca[0] > ca[2] && cb[1] > cb[0]);
    }

    #[test]
    fn dots_sit_on_the_features_and_are_distinct() {
        let (w, h) = (480, 360);
        let pairs = dot_pairs(w, h);
        assert_eq!(pairs.len(), 8 + 2 + 3 + 12);
        for side in 0..2 {
            let pts: Vec<(f64, f64)> = pairs
                .iter()
                .map(|p| if side == 0 { p.0 } else { p.1 })
                .collect();
            for (i, p) in pts.iter().enumerate() {
                assert!(
                    p.0 >= 0.0 && p.0 <= 480.0 && p.1 >= 0.0 && p.1 <= 360.0,
                    "{p:?}"
                );
                assert!(pts[i + 1..].iter().all(|q| q != p), "duplicate dot {p:?}");
            }
        }
        // Eye dots land on dark eye pixels in both images.
        let (a, b) = pair(w, h).unwrap();
        let on = |img: &Image, p: (f64, f64)| img.get(p.0 as u32, p.1 as u32).unwrap();
        for i in [8, 9] {
            assert!(on(&a, pairs[i].0)[0] < 0.3, "sun eye {i}");
            assert!(on(&b, pairs[i].1)[0] < 0.3, "moon eye {i}");
        }
    }
}
