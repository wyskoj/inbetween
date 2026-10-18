use crate::{Bounds, Point};
use serde::{Deserialize, Serialize};

/// Relative area threshold below which a triangle counts as degenerate.
const DEGENERATE_REL: f64 = 1e-12;

/// A triangle given by three vertices.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Triangle {
    pub a: Point,
    pub b: Point,
    pub c: Point,
}

impl Triangle {
    pub const fn new(a: Point, b: Point, c: Point) -> Self {
        Self { a, b, c }
    }

    /// Signed area: positive when `a, b, c` run counter-clockwise (y up).
    pub fn signed_area(&self) -> f64 {
        0.5 * (self.b - self.a).cross(self.c - self.a)
    }

    pub fn area(&self) -> f64 {
        self.signed_area().abs()
    }

    /// True for zero-area, sliver, or non-finite triangles. Such a triangle has no
    /// barycentric frame and no invertible affine map.
    pub fn is_degenerate(&self) -> bool {
        let longest_sq = [self.b - self.a, self.c - self.b, self.a - self.c]
            .iter()
            .map(|e| e.x * e.x + e.y * e.y)
            .fold(0.0_f64, f64::max);
        let twice_area = 2.0 * self.area();
        // Written as a negated `>` so NaN counts as degenerate.
        !(longest_sq.is_finite() && longest_sq > 0.0 && twice_area > DEGENERATE_REL * longest_sq)
    }

    pub fn bounds(&self) -> Option<Bounds> {
        Bounds::from_points([self.a, self.b, self.c])
    }

    /// Barycentric coordinates `[wa, wb, wc]` of `p`, summing to 1.
    /// `None` if the triangle is degenerate or `p` is not finite.
    pub fn barycentric(&self, p: Point) -> Option<[f64; 3]> {
        if self.is_degenerate() || !p.is_finite() {
            return None;
        }
        let d = (self.b - self.a).cross(self.c - self.a);
        let wb = (p - self.a).cross(self.c - self.a) / d;
        let wc = (self.b - self.a).cross(p - self.a) / d;
        let w = [1.0 - wb - wc, wb, wc];
        w.iter().all(|v| v.is_finite()).then_some(w)
    }

    /// Whether `p` lies inside or on the triangle, allowing `tol` slack in each weight.
    pub fn contains(&self, p: Point, tol: f64) -> bool {
        self.barycentric(p)
            .is_some_and(|w| w.iter().all(|&v| v >= -tol))
    }

    /// Point with barycentric weights `w`.
    pub fn from_barycentric(&self, w: [f64; 3]) -> Point {
        self.a * w[0] + self.b * w[1] + self.c * w[2]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit() -> Triangle {
        Triangle::new(
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
        )
    }

    #[test]
    fn area_and_orientation() {
        assert_eq!(unit().signed_area(), 0.5);
        let cw = Triangle::new(unit().a, unit().c, unit().b);
        assert_eq!(cw.signed_area(), -0.5);
        assert_eq!(cw.area(), 0.5);
    }

    #[test]
    fn barycentric_of_vertices_and_centroid() {
        let t = unit();
        assert_eq!(t.barycentric(t.a), Some([1.0, 0.0, 0.0]));
        assert_eq!(t.barycentric(t.b), Some([0.0, 1.0, 0.0]));
        assert_eq!(t.barycentric(t.c), Some([0.0, 0.0, 1.0]));
        let w = t.barycentric(Point::new(1.0 / 3.0, 1.0 / 3.0)).unwrap();
        for v in w {
            assert!((v - 1.0 / 3.0).abs() < 1e-12);
        }
    }

    #[test]
    fn barycentric_round_trips() {
        let t = Triangle::new(
            Point::new(2.0, 1.0),
            Point::new(9.0, 3.0),
            Point::new(4.0, 8.0),
        );
        let p = Point::new(5.0, 4.0);
        let back = t.from_barycentric(t.barycentric(p).unwrap());
        assert!(back.distance(p) < 1e-12);
    }

    #[test]
    fn contains_inside_outside_edge() {
        let t = unit();
        assert!(t.contains(Point::new(0.25, 0.25), 0.0));
        assert!(t.contains(Point::new(0.5, 0.5), 1e-12));
        assert!(!t.contains(Point::new(0.6, 0.6), 1e-12));
        assert!(!t.contains(Point::new(-0.1, 0.5), 0.0));
    }

    #[test]
    fn degenerate_triangles_have_no_frame() {
        let collinear = Triangle::new(
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(2.0, 2.0),
        );
        let point = Triangle::new(
            Point::new(3.0, 3.0),
            Point::new(3.0, 3.0),
            Point::new(3.0, 3.0),
        );
        let nan = Triangle::new(
            Point::new(f64::NAN, 0.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
        );
        for t in [collinear, point, nan] {
            assert!(t.is_degenerate());
            assert!(t.barycentric(Point::new(0.5, 0.5)).is_none());
            assert!(!t.contains(Point::new(0.5, 0.5), 1.0));
        }
        assert!(!unit().is_degenerate());
    }

    #[test]
    fn non_finite_query_point_is_none() {
        assert!(unit().barycentric(Point::new(f64::NAN, 0.0)).is_none());
    }
}
