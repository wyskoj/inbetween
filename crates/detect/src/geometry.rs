//! Letterbox and rotated-ROI geometry. Affines are `[a, b, c, d, e, f]` mapping `(x, y)` to
//! `(a x + b y + c, d x + e y + f)`, matching `raster::resample`'s inverse map convention.

pub type Affine = [f64; 6];

pub fn apply(m: &Affine, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5])
}

/// Inverse of an affine; `None` when singular or non-finite.
pub fn invert(m: &Affine) -> Option<Affine> {
    let det = m[0] * m[4] - m[1] * m[3];
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let (a, b, d, e) = (m[4] / det, -m[1] / det, -m[3] / det, m[0] / det);
    let inv = [a, b, -(a * m[2] + b * m[5]), d, e, -(d * m[2] + e * m[5])];
    inv.iter().all(|v| v.is_finite()).then_some(inv)
}

/// Map from letterboxed square (side `size`, image centred, aspect kept) back to image pixels.
/// Returns `None` for a zero or non-finite image size.
pub fn letterbox_to_image(width: u32, height: u32, size: u32) -> Option<Affine> {
    if width == 0 || height == 0 || size == 0 {
        return None;
    }
    let (w, h, s) = (f64::from(width), f64::from(height), f64::from(size));
    let scale = s / w.max(h);
    let (ox, oy) = ((s - w * scale) / 2.0, (s - h * scale) / 2.0);
    let k = 1.0 / scale;
    Some([k, 0.0, -ox * k, 0.0, k, -oy * k])
}

/// A square region of interest, rotated so the line between the eyes is horizontal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roi {
    pub cx: f64,
    pub cy: f64,
    pub size: f64,
    /// Roll in radians (positive turns the face clockwise on screen).
    pub angle: f64,
}

impl Roi {
    /// ROI from the two eye positions and the detector box (all in pixels). `scale` enlarges
    /// the box (1.5 in the pipeline). `None` for non-finite input or coincident eyes.
    pub fn from_eyes(
        right_eye: (f64, f64),
        left_eye: (f64, f64),
        centre: (f64, f64),
        box_size: f64,
        scale: f64,
    ) -> Option<Roi> {
        let (dx, dy) = (left_eye.0 - right_eye.0, left_eye.1 - right_eye.1);
        let vals = [dx, dy, centre.0, centre.1, box_size, scale];
        if !vals.iter().all(|v| v.is_finite()) || box_size <= 0.0 || dx.hypot(dy) < 1e-6 {
            return None;
        }
        Some(Roi {
            cx: centre.0,
            cy: centre.1,
            size: box_size * scale,
            angle: dy.atan2(dx),
        })
    }

    /// Map from a unit-square crop `(u, v)` in `0..out` pixels (side `out`) to image pixels.
    pub fn crop_to_image(&self, out: u32) -> Affine {
        let k = self.size / f64::from(out.max(1));
        let (c, s) = (self.angle.cos(), self.angle.sin());
        let h = f64::from(out) / 2.0;
        // image = centre + R * ((p - h) * k)
        [
            c * k,
            -s * k,
            self.cx - (c * h - s * h) * k,
            s * k,
            c * k,
            self.cy - (s * h + c * h) * k,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letterbox_round_trips_corners_and_centre() {
        let m = letterbox_to_image(200, 100, 128).expect("valid");
        let (x, y) = apply(&m, 64.0, 64.0);
        assert!((x - 100.0).abs() < 1e-9 && (y - 50.0).abs() < 1e-9);
        let (x, y) = apply(&m, 0.0, 32.0);
        assert!(x.abs() < 1e-9 && y.abs() < 1e-9);
        assert!(letterbox_to_image(0, 5, 128).is_none());
    }

    #[test]
    fn roi_crop_maps_centre_to_centre_and_inverts() {
        let roi = Roi::from_eyes((40.0, 50.0), (60.0, 50.0), (50.0, 60.0), 40.0, 1.5).expect("roi");
        let m = roi.crop_to_image(192);
        let (x, y) = apply(&m, 96.0, 96.0);
        assert!((x - 50.0).abs() < 1e-9 && (y - 60.0).abs() < 1e-9);
        let inv = invert(&m).expect("invertible");
        let (u, v) = apply(&inv, x, y);
        assert!((u - 96.0).abs() < 1e-9 && (v - 96.0).abs() < 1e-9);
    }

    #[test]
    fn rotated_roi_round_trips() {
        let roi = Roi::from_eyes((40.0, 40.0), (60.0, 55.0), (50.0, 60.0), 40.0, 1.5).expect("roi");
        let m = roi.crop_to_image(192);
        let inv = invert(&m).expect("invertible");
        let (x, y) = apply(&m, 10.0, 170.0);
        let (u, v) = apply(&inv, x, y);
        assert!((u - 10.0).abs() < 1e-9 && (v - 170.0).abs() < 1e-9);
    }

    #[test]
    fn degenerate_rois_are_refused() {
        assert!(Roi::from_eyes((1.0, 1.0), (1.0, 1.0), (0.0, 0.0), 10.0, 1.5).is_none());
        assert!(Roi::from_eyes((f64::NAN, 1.0), (2.0, 1.0), (0.0, 0.0), 10.0, 1.5).is_none());
        assert!(invert(&[0.0; 6]).is_none());
    }
}
