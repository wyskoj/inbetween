use crate::blend::{lerp, Pixel, TRANSPARENT};
use crate::RasterError;

/// Largest image accepted, in pixels. At 16 bytes per pixel this is 1.6 GB of f32 storage.
pub const MAX_PIXELS: u64 = 100_000_000;

/// An RGBA image, premultiplied f32, rows top to bottom.
///
/// Pixel `(x, y)` covers the square `[x, x+1) x [y, y+1)`; its centre is `(x + 0.5, y + 0.5)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    width: u32,
    height: u32,
    data: Vec<f32>,
}

fn checked_len(width: u32, height: u32) -> Result<usize, RasterError> {
    let err = RasterError::InvalidSize { width, height };
    let pixels = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || pixels > MAX_PIXELS {
        return Err(err);
    }
    usize::try_from(pixels)
        .ok()
        .and_then(|p| p.checked_mul(4))
        .ok_or(err)
}

impl Image {
    /// A fully transparent image.
    pub fn new(width: u32, height: u32) -> Result<Self, RasterError> {
        let len = checked_len(width, height)?;
        Ok(Self {
            width,
            height,
            data: vec![0.0; len],
        })
    }

    /// An image filled with one premultiplied pixel.
    pub fn filled(width: u32, height: u32, px: Pixel) -> Result<Self, RasterError> {
        let len = checked_len(width, height)?;
        let mut data = Vec::with_capacity(len);
        for _ in 0..len / 4 {
            data.extend_from_slice(&px);
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Build from straight (non-premultiplied) 8-bit sRGB RGBA, e.g. a decoded PNG.
    pub fn from_rgba8(width: u32, height: u32, rgba: &[u8]) -> Result<Self, RasterError> {
        let len = checked_len(width, height)?;
        if rgba.len() != len {
            return Err(RasterError::BufferMismatch {
                expected: len,
                got: rgba.len(),
            });
        }
        let mut data = Vec::with_capacity(len);
        for px in rgba.chunks_exact(4) {
            let a = f32::from(px[3]) / 255.0;
            data.push(f32::from(px[0]) / 255.0 * a);
            data.push(f32::from(px[1]) / 255.0 * a);
            data.push(f32::from(px[2]) / 255.0 * a);
            data.push(a);
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Convert to straight 8-bit sRGB RGBA, rounding to nearest. Out-of-range values are
    /// clamped and NaN becomes 0, so the result is always valid. Fully transparent pixels
    /// come out as `[0, 0, 0, 0]`.
    pub fn to_rgba8(&self) -> Vec<u8> {
        let to_u8 = |v: f32| (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8; // NaN casts to 0
        let mut out = Vec::with_capacity(self.data.len());
        for px in self.data.chunks_exact(4) {
            let a = if px[3].is_nan() {
                0.0
            } else {
                px[3].clamp(0.0, 1.0)
            };
            if a <= 0.0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            out.push(to_u8(px[0] / a));
            out.push(to_u8(px[1] / a));
            out.push(to_u8(px[2] / a));
            out.push(to_u8(a));
        }
        out
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Raw premultiplied f32 values, four per pixel.
    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }

    /// Mutable access to the raw premultiplied f32 values, four per pixel, row by row. For
    /// renderers that fill whole rows in parallel.
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.data
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize)
            .checked_mul(self.width as usize)?
            .checked_add(x as usize)?;
        i.checked_mul(4)
    }

    /// The pixel at `(x, y)`, or `None` outside the image.
    pub fn get(&self, x: u32, y: u32) -> Option<Pixel> {
        let i = self.index(x, y)?;
        let s = self.data.get(i..i + 4)?;
        Some([s[0], s[1], s[2], s[3]])
    }

    /// Set a pixel. Returns `false` (and changes nothing) outside the image.
    pub fn set(&mut self, x: u32, y: u32, px: Pixel) -> bool {
        let Some(i) = self.index(x, y) else {
            return false;
        };
        match self.data.get_mut(i..i + 4) {
            Some(s) => {
                s.copy_from_slice(&px);
                true
            }
            None => false,
        }
    }

    /// Pixel at signed coordinates with edge clamp: anything outside reads the nearest edge pixel.
    pub fn get_clamped(&self, x: i64, y: i64) -> Pixel {
        let cx = x.clamp(0, i64::from(self.width) - 1) as u32;
        let cy = y.clamp(0, i64::from(self.height) - 1) as u32;
        self.get(cx, cy).unwrap_or(TRANSPARENT)
    }

    /// Bilinear sample at continuous position `(x, y)`, where pixel centres sit at `+ 0.5`.
    /// Positions outside the image clamp to the edge; NaN reads as 0. Sampling exactly at a
    /// pixel centre returns that pixel.
    pub fn sample_bilinear(&self, x: f32, y: f32) -> Pixel {
        // Keep within a range where the f32 -> i64 steps below are exact and cannot overflow.
        let limit = 1.0e9_f32;
        let fx = if x.is_nan() {
            0.0
        } else {
            x.clamp(-limit, limit)
        } - 0.5;
        let fy = if y.is_nan() {
            0.0
        } else {
            y.clamp(-limit, limit)
        } - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let (x0, y0) = (x0 as i64, y0 as i64);
        let top = lerp(self.get_clamped(x0, y0), self.get_clamped(x0 + 1, y0), tx);
        let bottom = lerp(
            self.get_clamped(x0, y0 + 1),
            self.get_clamped(x0 + 1, y0 + 1),
            tx,
        );
        lerp(top, bottom, ty)
    }

    /// Cross-dissolve two same-sized images: `a` at `t = 0`, `b` at `t = 1`.
    pub fn lerp(a: &Self, b: &Self, t: f32) -> Result<Self, RasterError> {
        if a.size() != b.size() {
            return Err(RasterError::SizeMismatch {
                left: a.size(),
                right: b.size(),
            });
        }
        let mut out = a.clone();
        for (o, (pa, pb)) in out
            .data
            .chunks_exact_mut(4)
            .zip(a.data.chunks_exact(4).zip(b.data.chunks_exact(4)))
        {
            let p = lerp(
                [pa[0], pa[1], pa[2], pa[3]],
                [pb[0], pb[1], pb[2], pb[3]],
                t,
            );
            o.copy_from_slice(&p);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_by_two() -> Image {
        // Row 0: black, white. Row 1: red, transparent.
        Image::from_rgba8(
            2,
            2,
            &[0, 0, 0, 255, 255, 255, 255, 255, 255, 0, 0, 255, 0, 0, 0, 0],
        )
        .unwrap()
    }

    #[test]
    fn rejects_bad_sizes_and_buffers() {
        assert!(matches!(
            Image::new(0, 5),
            Err(RasterError::InvalidSize { .. })
        ));
        assert!(matches!(
            Image::new(5, 0),
            Err(RasterError::InvalidSize { .. })
        ));
        assert!(matches!(
            Image::new(u32::MAX, u32::MAX),
            Err(RasterError::InvalidSize { .. })
        ));
        assert!(matches!(
            Image::new(20_000, 20_000),
            Err(RasterError::InvalidSize { .. })
        ));
        assert_eq!(
            Image::from_rgba8(2, 2, &[0; 15]),
            Err(RasterError::BufferMismatch {
                expected: 16,
                got: 15
            })
        );
    }

    #[test]
    fn get_and_set_are_bounds_checked() {
        let mut img = Image::new(3, 2).unwrap();
        assert!(img.set(2, 1, [0.1, 0.2, 0.3, 0.5]));
        assert_eq!(img.get(2, 1), Some([0.1, 0.2, 0.3, 0.5]));
        assert!(!img.set(3, 0, [1.0; 4]));
        assert!(!img.set(0, 2, [1.0; 4]));
        assert_eq!(img.get(3, 0), None);
        assert_eq!(img.get(u32::MAX, u32::MAX), None);
    }

    #[test]
    fn premultiplies_on_the_way_in() {
        let img = Image::from_rgba8(1, 1, &[255, 128, 0, 128]).unwrap();
        let p = img.get(0, 0).unwrap();
        let a = 128.0 / 255.0;
        assert!((p[3] - a).abs() < 1e-6);
        assert!((p[0] - a).abs() < 1e-6);
        assert!((p[1] - a * 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(p[2], 0.0);
    }

    #[test]
    fn rgba8_round_trips_exactly_for_every_colour_and_alpha() {
        // Every channel value at every non-zero alpha.
        for a in 1..=255u8 {
            let mut buf = Vec::new();
            for c in 0..=255u8 {
                buf.extend_from_slice(&[c, 255 - c, c / 2, a]);
            }
            let img = Image::from_rgba8(256, 1, &buf).unwrap();
            assert_eq!(img.to_rgba8(), buf, "alpha {a}");
        }
    }

    #[test]
    fn transparent_pixels_normalise_to_zero() {
        let img = Image::from_rgba8(1, 1, &[200, 100, 50, 0]).unwrap();
        assert_eq!(img.to_rgba8(), vec![0, 0, 0, 0]);
    }

    #[test]
    fn to_rgba8_survives_garbage_values() {
        let mut img = Image::new(2, 1).unwrap();
        img.set(0, 0, [f32::NAN, f32::INFINITY, -4.0, 0.5]);
        img.set(1, 0, [1.0, 1.0, 1.0, f32::NAN]);
        let out = img.to_rgba8();
        assert_eq!(out.len(), 8);
        assert_eq!(&out[4..], &[0, 0, 0, 0]);
    }

    #[test]
    fn bilinear_at_pixel_centres_returns_the_pixel() {
        let img = two_by_two();
        for y in 0..2 {
            for x in 0..2 {
                let want = img.get(x, y).unwrap();
                let got = img.sample_bilinear(x as f32 + 0.5, y as f32 + 0.5);
                assert_eq!(got, want);
            }
        }
    }

    #[test]
    fn bilinear_midpoints_average() {
        let img = two_by_two();
        // Halfway between black and white on the top row.
        let p = img.sample_bilinear(1.0, 0.5);
        assert!((p[0] - 0.5).abs() < 1e-6 && (p[3] - 1.0).abs() < 1e-6);
        // Centre of the 2x2: the mean of all four premultiplied pixels.
        let c = img.sample_bilinear(1.0, 1.0);
        assert!((c[0] - 0.5).abs() < 1e-6, "{c:?}");
        assert!((c[3] - 0.75).abs() < 1e-6, "{c:?}");
    }

    #[test]
    fn bilinear_clamps_at_the_edges() {
        let img = two_by_two();
        assert_eq!(img.sample_bilinear(-50.0, 0.5), img.get(0, 0).unwrap());
        assert_eq!(img.sample_bilinear(50.0, 0.5), img.get(1, 0).unwrap());
        assert_eq!(img.sample_bilinear(0.5, 99.0), img.get(0, 1).unwrap());
        assert_eq!(img.sample_bilinear(-1e30, 1e30), img.get(0, 1).unwrap());
    }

    #[test]
    fn bilinear_handles_non_finite_positions() {
        let img = two_by_two();
        for (x, y) in [
            (f32::NAN, 0.5),
            (0.5, f32::NAN),
            (f32::INFINITY, f32::NEG_INFINITY),
        ] {
            let p = img.sample_bilinear(x, y);
            assert!(p.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn bilinear_interpolation_is_alpha_correct() {
        // Opaque red next to transparent: the midpoint is half-alpha pure red, not dark red.
        let img = Image::from_rgba8(2, 1, &[255, 0, 0, 255, 0, 0, 0, 0]).unwrap();
        let p = img.sample_bilinear(1.0, 0.5);
        assert!((p[3] - 0.5).abs() < 1e-6);
        assert!((p[0] / p[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn image_lerp_dissolves_and_checks_sizes() {
        let a = Image::filled(2, 2, [0.0, 0.0, 0.0, 1.0]).unwrap();
        let b = Image::filled(2, 2, [1.0, 1.0, 1.0, 1.0]).unwrap();
        assert_eq!(Image::lerp(&a, &b, 0.0).unwrap(), a);
        assert_eq!(Image::lerp(&a, &b, 1.0).unwrap(), b);
        assert_eq!(
            Image::lerp(&a, &b, 0.5).unwrap().get(1, 1),
            Some([0.5, 0.5, 0.5, 1.0])
        );
        let c = Image::new(3, 2).unwrap();
        assert!(matches!(
            Image::lerp(&a, &c, 0.5),
            Err(RasterError::SizeMismatch { .. })
        ));
    }
}
