use crate::{GeomError, Point, Triangle};
use spade::{DelaunayTriangulation, Point2, Triangulation as _};

/// Upper bound on dots per triangulation; caps allocations sized by input.
pub const MAX_DOTS: usize = 100_000;

/// A Delaunay mesh over a dot set plus the four output corners.
#[derive(Clone, Debug, PartialEq)]
pub struct Triangulation {
    /// The dots in input order (indices `0..dot_count`), then the corners
    /// `(0,0)`, `(w,0)`, `(w,h)`, `(0,h)` at `dot_count..dot_count + 4`.
    pub vertices: Vec<Point>,
    /// Counter-clockwise triangles as indices into `vertices`. Every triangle has
    /// positive area. A corner that coincides with a dot is left out; the dot is used.
    pub triangles: Vec<[usize; 3]>,
    pub dot_count: usize,
}

impl Triangulation {
    /// The geometry of triangle `i`, or `None` if out of range.
    pub fn triangle(&self, i: usize) -> Option<Triangle> {
        let [a, b, c] = *self.triangles.get(i)?;
        Some(Triangle::new(
            *self.vertices.get(a)?,
            *self.vertices.get(b)?,
            *self.vertices.get(c)?,
        ))
    }
}

/// Delaunay-triangulate `dots` together with the corners of a `width` x `height` frame.
///
/// Dots may lie outside the frame; the mesh then covers their convex hull, which always
/// contains the frame. Fewer than three dots, and collinear dots, are fine because the
/// corners give the mesh area. Two dots at the same position are rejected with
/// [`GeomError::DuplicateDot`] so paired dot sets keep their indices.
pub fn triangulate(dots: &[Point], width: f64, height: f64) -> Result<Triangulation, GeomError> {
    if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
        return Err(GeomError::InvalidSize { width, height });
    }
    if dots.len() > MAX_DOTS {
        return Err(GeomError::TooManyDots {
            count: dots.len(),
            max: MAX_DOTS,
        });
    }
    if let Some(i) = dots.iter().position(|p| !p.is_finite()) {
        return Err(GeomError::NonFinite(Some(i)));
    }

    let dot_count = dots.len();
    let mut vertices = dots.to_vec();
    vertices.extend([
        Point::new(0.0, 0.0),
        Point::new(width, 0.0),
        Point::new(width, height),
        Point::new(0.0, height),
    ]);

    let mut mesh: DelaunayTriangulation<Point2<f64>> = DelaunayTriangulation::new();
    // Maps spade's vertex index (insertion order, duplicates skipped) to ours.
    let mut ours: Vec<usize> = Vec::with_capacity(vertices.len());
    for (i, p) in vertices.iter().enumerate() {
        let handle = mesh
            .insert(Point2::new(p.x, p.y))
            .map_err(|_| GeomError::OutOfRange(i))?;
        let idx = handle.index();
        match ours.get(idx) {
            None => ours.push(i),
            Some(&first) if i < dot_count => {
                return Err(GeomError::DuplicateDot { first, second: i });
            }
            // A corner on top of a dot or another corner: keep the earlier vertex.
            Some(_) => {}
        }
    }

    let mut triangles = Vec::with_capacity(mesh.num_inner_faces());
    for face in mesh.inner_faces() {
        let [a, b, c] = face.vertices();
        let (Some(&a), Some(&b), Some(&c)) = (
            ours.get(a.fix().index()),
            ours.get(b.fix().index()),
            ours.get(c.fix().index()),
        ) else {
            return Err(GeomError::Internal("vertex index out of range".into()));
        };
        triangles.push([a, b, c]);
    }
    Ok(Triangulation {
        vertices,
        triangles,
        dot_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total_area(t: &Triangulation) -> f64 {
        (0..t.triangles.len())
            .filter_map(|i| t.triangle(i))
            .map(|tri| tri.area())
            .sum()
    }

    #[test]
    fn no_dots_gives_two_triangles_covering_the_frame() {
        let t = triangulate(&[], 8.0, 6.0).unwrap();
        assert_eq!(t.triangles.len(), 2);
        assert!((total_area(&t) - 48.0).abs() < 1e-9);
    }

    #[test]
    fn single_interior_dot_makes_four_triangles() {
        let t = triangulate(&[Point::new(4.0, 3.0)], 8.0, 6.0).unwrap();
        assert_eq!(t.triangles.len(), 4);
        assert!((total_area(&t) - 48.0).abs() < 1e-9);
    }

    #[test]
    fn collinear_dots_are_accepted() {
        let dots = [
            Point::new(2.0, 3.0),
            Point::new(4.0, 3.0),
            Point::new(6.0, 3.0),
        ];
        let t = triangulate(&dots, 8.0, 6.0).unwrap();
        assert!((total_area(&t) - 48.0).abs() < 1e-9);
    }

    #[test]
    fn duplicate_dots_are_rejected_with_indices() {
        let dots = [
            Point::new(1.0, 1.0),
            Point::new(2.0, 2.0),
            Point::new(1.0, 1.0),
        ];
        assert_eq!(
            triangulate(&dots, 8.0, 6.0),
            Err(GeomError::DuplicateDot {
                first: 0,
                second: 2
            })
        );
    }

    #[test]
    fn dot_on_a_corner_replaces_the_corner() {
        let t = triangulate(&[Point::new(0.0, 0.0)], 8.0, 6.0).unwrap();
        assert!((total_area(&t) - 48.0).abs() < 1e-9);
        // The corner vertex (index dot_count) is unused; the dot takes its place.
        assert!(t.triangles.iter().all(|tri| !tri.contains(&1)));
        assert!(t.triangles.iter().any(|tri| tri.contains(&0)));
    }

    #[test]
    fn dots_outside_the_frame_extend_the_mesh() {
        let t = triangulate(&[Point::new(-5.0, 3.0)], 8.0, 6.0).unwrap();
        assert!(total_area(&t) > 48.0);
        let f = |x, y| Point::new(x, y);
        for p in [f(0.1, 0.1), f(7.9, 5.9), f(4.0, 3.0), f(0.1, 5.9)] {
            let hits = (0..t.triangles.len())
                .filter_map(|i| t.triangle(i))
                .filter(|tri| tri.contains(p, 1e-12))
                .count();
            assert!(hits >= 1, "{p:?} uncovered");
        }
    }

    #[test]
    fn triangles_are_counter_clockwise() {
        let t = triangulate(&[Point::new(4.0, 3.0), Point::new(1.0, 5.0)], 8.0, 6.0).unwrap();
        for i in 0..t.triangles.len() {
            assert!(t.triangle(i).is_some_and(|tri| tri.signed_area() > 0.0));
        }
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        let p = Point::new(1.0, 1.0);
        assert!(matches!(
            triangulate(&[p], 0.0, 6.0),
            Err(GeomError::InvalidSize { .. })
        ));
        assert!(matches!(
            triangulate(&[p], f64::NAN, 6.0),
            Err(GeomError::InvalidSize { .. })
        ));
        assert!(matches!(
            triangulate(&[p], 8.0, -1.0),
            Err(GeomError::InvalidSize { .. })
        ));
        assert_eq!(
            triangulate(&[p, Point::new(f64::NAN, 0.0)], 8.0, 6.0),
            Err(GeomError::NonFinite(Some(1)))
        );
        assert_eq!(
            triangulate(&[Point::new(f64::INFINITY, 0.0)], 8.0, 6.0),
            Err(GeomError::NonFinite(Some(0)))
        );
        assert!(triangulate(&[Point::new(1e300, 0.0)], 8.0, 6.0).is_err());
        let many = vec![p; MAX_DOTS + 1];
        assert!(matches!(
            triangulate(&many, 8.0, 6.0),
            Err(GeomError::TooManyDots { .. })
        ));
    }

    #[test]
    fn triangle_accessor_is_bounds_checked() {
        let t = triangulate(&[], 8.0, 6.0).unwrap();
        assert!(t.triangle(usize::MAX).is_none());
    }
}
