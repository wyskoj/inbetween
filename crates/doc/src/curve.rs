use crate::DocError;
use serde::{Deserialize, Serialize};

/// Most control points a curve may have.
pub const MAX_CURVE_POINTS: usize = 64;

/// One control point: `x` is sequence time, `y` is the remapped amount, both 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    pub x: f64,
    pub y: f64,
}

impl CurvePoint {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A time remapping: morph time in, amount out. Used for the feature (colour dissolve) and
/// shape (dot interpolation) curves.
///
/// The curve runs through its points left to right. It is smooth (a cubic that never
/// overshoots between points) unless `linear` is set. The first point is always (0, 0) and the
/// last (1, 1), so frame 0 stays Image 1 and the last frame stays Image 2.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Curve {
    pub points: Vec<CurvePoint>,
    pub linear: bool,
}

impl Default for Curve {
    /// The identity: a straight line from (0, 0) to (1, 1).
    fn default() -> Self {
        Self {
            points: vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 1.0)],
            linear: false,
        }
    }
}

impl Curve {
    /// Whether the curve leaves time unchanged.
    pub fn is_identity(&self) -> bool {
        self.points == Self::default().points
    }

    /// Check the limits that keep evaluation safe: 2 to 64 finite points, x strictly rising,
    /// everything inside 0..=1, ends pinned at (0, 0) and (1, 1).
    pub fn validate(&self) -> Result<(), DocError> {
        let bad = |why: &'static str| Err(DocError::BadCurve(why));
        let (Some(first), Some(last)) = (self.points.first(), self.points.last()) else {
            return bad("a curve needs at least 2 points");
        };
        if self.points.len() < 2 {
            return bad("a curve needs at least 2 points");
        }
        if self.points.len() > MAX_CURVE_POINTS {
            return bad("a curve may have at most 64 points");
        }
        if self
            .points
            .iter()
            .any(|p| !(0.0..=1.0).contains(&p.x) || !(0.0..=1.0).contains(&p.y))
        {
            return bad("curve points must have x and y between 0 and 1");
        }
        if self.points.windows(2).any(|w| w[0].x >= w[1].x) {
            return bad("curve points must have strictly increasing x");
        }
        if (first.x, first.y) != (0.0, 0.0) || (last.x, last.y) != (1.0, 1.0) {
            return bad("a curve must start at (0, 0) and end at (1, 1)");
        }
        Ok(())
    }

    /// The remapped amount at time `t` (clamped to 0..=1; NaN counts as 0). Always in 0..=1,
    /// even for a curve that fails [`Curve::validate`].
    pub fn eval(&self, t: f64) -> f64 {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        let pts = &self.points;
        // Find the segment [i, i + 1] holding t.
        let Some(i) = pts.windows(2).position(|w| t <= w[1].x) else {
            return t;
        };
        let (Some(&p0), Some(&p1)) = (pts.get(i), pts.get(i + 1)) else {
            return t;
        };
        let dx = p1.x - p0.x;
        if !(dx > 0.0 && dx.is_finite()) {
            return t;
        }
        let u = ((t - p0.x) / dx).clamp(0.0, 1.0);
        let y = if self.linear {
            p0.y + (p1.y - p0.y) * u
        } else {
            let (m0, m1) = (self.tangent(i), self.tangent(i + 1));
            let (u2, u3) = (u * u, u * u * u);
            (2.0 * u3 - 3.0 * u2 + 1.0) * p0.y
                + (u3 - 2.0 * u2 + u) * dx * m0
                + (-2.0 * u3 + 3.0 * u2) * p1.y
                + (u3 - u2) * dx * m1
        };
        if y.is_finite() {
            y.clamp(0.0, 1.0)
        } else {
            t
        }
    }

    /// Slope at control point `i` (harmonic mean of the neighbouring slopes, zero at a turning
    /// point, so a segment never overshoots its ends).
    fn tangent(&self, i: usize) -> f64 {
        let slope = |a: usize, b: usize| -> Option<f64> {
            let (p, q) = (self.points.get(a)?, self.points.get(b)?);
            Some((q.y - p.y) / (q.x - p.x))
        };
        let before = i.checked_sub(1).and_then(|a| slope(a, i));
        let after = slope(i, i + 1);
        match (before, after) {
            (Some(a), Some(b)) if a * b > 0.0 => 2.0 * a * b / (a + b),
            (Some(_), Some(_)) => 0.0,
            (Some(s), None) | (None, Some(s)) => s,
            (None, None) => 0.0,
        }
    }

    /// Add a point at `(x, y)`. A point at an existing x replaces that point's y (the pinned
    /// ends cannot move). Returns the point's index.
    pub fn insert(&mut self, x: f64, y: f64) -> Result<usize, DocError> {
        if !(x.is_finite() && y.is_finite()) {
            return Err(DocError::BadCurve("curve points must be finite numbers"));
        }
        let (x, y) = (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0));
        let at = self.points.partition_point(|p| p.x < x);
        if self.points.get(at).is_some_and(|p| p.x == x) {
            if at == 0 || at + 1 == self.points.len() {
                return Err(DocError::BadCurve("the end points of a curve cannot move"));
            }
            if let Some(p) = self.points.get_mut(at) {
                p.y = y;
            }
            return Ok(at);
        }
        if self.points.len() >= MAX_CURVE_POINTS {
            return Err(DocError::BadCurve("a curve may have at most 64 points"));
        }
        self.points.insert(at, CurvePoint::new(x, y));
        Ok(at)
    }

    /// Move an inner point. Its x stays strictly between its neighbours; the ends cannot move.
    pub fn move_point(&mut self, index: usize, x: f64, y: f64) -> Result<(), DocError> {
        if !(x.is_finite() && y.is_finite()) {
            return Err(DocError::BadCurve("curve points must be finite numbers"));
        }
        let inner = index >= 1 && index + 1 < self.points.len();
        let (Some(lo), Some(hi)) = (
            index.checked_sub(1).and_then(|i| self.points.get(i)),
            self.points.get(index + 1),
        ) else {
            return Err(DocError::BadCurve("no such inner curve point"));
        };
        if !inner {
            return Err(DocError::BadCurve("no such inner curve point"));
        }
        const GAP: f64 = 1e-4;
        let x = x.clamp(lo.x + GAP, hi.x - GAP);
        if let Some(p) = self.points.get_mut(index) {
            *p = CurvePoint::new(x, y.clamp(0.0, 1.0));
        }
        Ok(())
    }

    /// Remove an inner point.
    pub fn remove(&mut self, index: usize) -> Result<(), DocError> {
        if index == 0 || index + 1 >= self.points.len() {
            return Err(DocError::BadCurve("no such inner curve point"));
        }
        self.points.remove(index);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bumpy() -> Curve {
        Curve {
            points: vec![
                CurvePoint::new(0.0, 0.0),
                CurvePoint::new(0.3, 0.8),
                CurvePoint::new(0.6, 0.2),
                CurvePoint::new(1.0, 1.0),
            ],
            linear: false,
        }
    }

    #[test]
    fn default_is_identity() {
        let c = Curve::default();
        assert!(c.is_identity() && c.validate().is_ok());
        for i in 0..=10 {
            let t = f64::from(i) / 10.0;
            assert!((c.eval(t) - t).abs() < 1e-12, "{t}");
        }
        assert!(!bumpy().is_identity());
    }

    #[test]
    fn passes_through_its_points_and_stays_in_range() {
        let c = bumpy();
        assert!(c.validate().is_ok());
        for p in &c.points {
            assert!((c.eval(p.x) - p.y).abs() < 1e-12);
        }
        for i in 0..=1000 {
            let y = c.eval(f64::from(i) / 1000.0);
            assert!((0.0..=1.0).contains(&y), "{y}");
        }
    }

    #[test]
    fn smooth_curve_does_not_overshoot() {
        let c = Curve {
            points: vec![
                CurvePoint::new(0.0, 0.0),
                CurvePoint::new(0.5, 0.1),
                CurvePoint::new(1.0, 1.0),
            ],
            linear: false,
        };
        let mut last = 0.0;
        for i in 0..=1000 {
            let y = c.eval(f64::from(i) / 1000.0);
            assert!(y + 1e-12 >= last, "not monotone at {i}");
            last = y;
        }
    }

    #[test]
    fn linear_toggle_interpolates_straight() {
        let mut c = bumpy();
        c.linear = true;
        assert!((c.eval(0.15) - 0.4).abs() < 1e-12);
    }

    #[test]
    fn hostile_curves_are_rejected_and_never_panic_when_evaluated() {
        let c = bumpy();
        assert_eq!(c.eval(f64::NAN), 0.0);
        assert_eq!((c.eval(-5.0), c.eval(5.0)), (0.0, 1.0));
        let pt = CurvePoint::new;
        let cases = [
            vec![],
            vec![pt(0.0, 0.0)],
            vec![pt(0.0, 0.0), pt(f64::NAN, 0.5), pt(1.0, 1.0)],
            vec![pt(0.0, 0.2), pt(1.0, 1.0)],
            vec![pt(0.0, 0.0), pt(0.6, 0.5), pt(0.6, 0.5), pt(1.0, 1.0)],
            vec![pt(0.0, 0.0), pt(2.0, 5.0), pt(1.0, 1.0)],
            vec![pt(0.0, 0.0); 100],
        ];
        for points in cases {
            for linear in [false, true] {
                let c = Curve {
                    points: points.clone(),
                    linear,
                };
                assert!(c.validate().is_err(), "{c:?}");
                for t in [-1.0, 0.0, 0.3, 0.6, 1.0, 2.0, f64::NAN, f64::INFINITY] {
                    assert!((0.0..=1.0).contains(&c.eval(t)), "{c:?} at {t}");
                }
            }
        }
    }

    #[test]
    fn editing_keeps_the_curve_valid() {
        let mut c = Curve::default();
        assert_eq!(c.insert(0.5, 0.9).unwrap(), 1);
        assert_eq!(c.insert(0.5, 0.3).unwrap(), 1); // same x: replaces
        assert_eq!(c.points.len(), 3);
        c.move_point(1, 5.0, 2.0).unwrap(); // clamped
        assert!(c.validate().is_ok());
        assert!(c.move_point(0, 0.1, 0.1).is_err());
        assert!(c.move_point(2, 0.9, 0.1).is_err());
        assert!(c.move_point(9, 0.9, 0.1).is_err());
        assert!(c.insert(0.0, 0.5).is_err());
        assert!(c.insert(f64::NAN, 0.5).is_err());
        assert!(c.remove(0).is_err() && c.remove(2).is_err());
        c.remove(1).unwrap();
        assert!(c.is_identity());
        for k in 1..MAX_CURVE_POINTS - 2 {
            c.insert(k as f64 / 100.0, 0.5).unwrap();
        }
        assert_eq!(c.points.len(), MAX_CURVE_POINTS - 1);
        c.insert(0.995, 0.5).unwrap();
        assert!(c.insert(0.9951, 0.5).is_err());
        assert!(c.validate().is_ok());
    }

    #[test]
    fn older_files_without_curves_load_as_identity() {
        let c: Curve = serde_json::from_str("{}").unwrap();
        assert!(c.is_identity());
    }
}
