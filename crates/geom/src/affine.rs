use crate::{GeomError, Point, Triangle};
use serde::{Deserialize, Serialize};

/// A 2D affine transform: `x' = a*x + b*y + tx`, `y' = c*x + d*y + ty`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Affine {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    /// The unique transform mapping each vertex of `src` to the matching vertex of `dst`.
    ///
    /// Fails with [`GeomError::DegenerateTriangle`] if `src` has no area. A degenerate `dst`
    /// is allowed (it collapses the source) but the result is then not invertible.
    pub fn from_triangles(src: &Triangle, dst: &Triangle) -> Result<Self, GeomError> {
        if src.is_degenerate() {
            return Err(GeomError::DegenerateTriangle);
        }
        let s1 = src.b - src.a;
        let s2 = src.c - src.a;
        let d1 = dst.b - dst.a;
        let d2 = dst.c - dst.a;
        let det = s1.cross(s2);
        // dst_linear * inverse(src_linear), with columns (s1, s2) and (d1, d2).
        let a = (d1.x * s2.y - d2.x * s1.y) / det;
        let b = (d2.x * s1.x - d1.x * s2.x) / det;
        let c = (d1.y * s2.y - d2.y * s1.y) / det;
        let d = (d2.y * s1.x - d1.y * s2.x) / det;
        let tx = dst.a.x - (a * src.a.x + b * src.a.y);
        let ty = dst.a.y - (c * src.a.x + d * src.a.y);
        let m = Self { a, b, c, d, tx, ty };
        if m.is_finite() {
            Ok(m)
        } else {
            Err(GeomError::NonFinite(None))
        }
    }

    pub fn is_finite(&self) -> bool {
        [self.a, self.b, self.c, self.d, self.tx, self.ty]
            .iter()
            .all(|v| v.is_finite())
    }

    pub fn apply(&self, p: Point) -> Point {
        Point::new(
            self.a * p.x + self.b * p.y + self.tx,
            self.c * p.x + self.d * p.y + self.ty,
        )
    }

    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    /// The inverse transform, or an error if the map collapses area.
    pub fn inverse(&self) -> Result<Self, GeomError> {
        let det = self.determinant();
        if !(det.is_finite() && det.abs() > 1e-300) {
            return Err(GeomError::DegenerateTriangle);
        }
        let a = self.d / det;
        let b = -self.b / det;
        let c = -self.c / det;
        let d = self.a / det;
        let m = Self {
            a,
            b,
            c,
            d,
            tx: -(a * self.tx + b * self.ty),
            ty: -(c * self.tx + d * self.ty),
        };
        if m.is_finite() {
            Ok(m)
        } else {
            Err(GeomError::NonFinite(None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri(p: [(f64, f64); 3]) -> Triangle {
        Triangle::new(
            Point::new(p[0].0, p[0].1),
            Point::new(p[1].0, p[1].1),
            Point::new(p[2].0, p[2].1),
        )
    }

    #[test]
    fn maps_vertices_exactly() {
        let src = tri([(0.0, 0.0), (4.0, 0.0), (0.0, 3.0)]);
        let dst = tri([(10.0, 10.0), (12.0, 15.0), (7.0, 11.0)]);
        let m = Affine::from_triangles(&src, &dst).unwrap();
        for (s, d) in [(src.a, dst.a), (src.b, dst.b), (src.c, dst.c)] {
            assert!(m.apply(s).distance(d) < 1e-12);
        }
    }

    #[test]
    fn identity_when_triangles_match() {
        let t = tri([(1.0, 2.0), (5.0, 3.0), (2.0, 9.0)]);
        let m = Affine::from_triangles(&t, &t).unwrap();
        for (got, want) in [
            (m.a, 1.0),
            (m.b, 0.0),
            (m.c, 0.0),
            (m.d, 1.0),
            (m.tx, 0.0),
            (m.ty, 0.0),
        ] {
            assert!((got - want).abs() < 1e-12);
        }
    }

    #[test]
    fn degenerate_source_is_an_error() {
        let flat = tri([(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)]);
        let ok = tri([(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)]);
        assert_eq!(
            Affine::from_triangles(&flat, &ok),
            Err(GeomError::DegenerateTriangle)
        );
    }

    #[test]
    fn non_finite_input_is_an_error() {
        let nan = tri([(f64::NAN, 0.0), (1.0, 0.0), (0.0, 1.0)]);
        let ok = tri([(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)]);
        assert!(Affine::from_triangles(&nan, &ok).is_err());
        assert!(Affine::from_triangles(&ok, &nan).is_err());
    }

    #[test]
    fn inverse_round_trips() {
        let src = tri([(0.0, 0.0), (4.0, 0.0), (0.0, 3.0)]);
        let dst = tri([(10.0, 10.0), (12.0, 15.0), (7.0, 11.0)]);
        let m = Affine::from_triangles(&src, &dst).unwrap();
        let inv = m.inverse().unwrap();
        let p = Point::new(1.5, 0.7);
        assert!(inv.apply(m.apply(p)).distance(p) < 1e-12);
    }

    #[test]
    fn collapsed_destination_has_no_inverse() {
        let src = tri([(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)]);
        let flat = tri([(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)]);
        let m = Affine::from_triangles(&src, &flat).unwrap();
        assert!(m.inverse().is_err());
    }
}
