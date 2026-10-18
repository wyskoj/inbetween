use crate::Point;
use serde::{Deserialize, Serialize};

/// Axis-aligned bounding box. Edges are inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub min: Point,
    pub max: Point,
}

impl Bounds {
    /// Smallest box holding every point, or `None` for an empty or non-finite input.
    pub fn from_points(points: impl IntoIterator<Item = Point>) -> Option<Self> {
        let mut it = points.into_iter();
        let first = it.next().filter(|p| p.is_finite())?;
        let mut b = Self {
            min: first,
            max: first,
        };
        for p in it {
            if !p.is_finite() {
                return None;
            }
            b.min = Point::new(b.min.x.min(p.x), b.min.y.min(p.y));
            b.max = Point::new(b.max.x.max(p.x), b.max.y.max(p.y));
        }
        Some(b)
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    /// Smallest box holding both `self` and `other`.
    pub fn union(&self, other: &Self) -> Self {
        Self {
            min: Point::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Point::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_points_covers_all() {
        let b = Bounds::from_points([Point::new(1.0, 5.0), Point::new(-2.0, 3.0)]).unwrap();
        assert_eq!(b.min, Point::new(-2.0, 3.0));
        assert_eq!(b.max, Point::new(1.0, 5.0));
        assert_eq!((b.width(), b.height()), (3.0, 2.0));
        assert!(b.contains(Point::new(1.0, 3.0)));
        assert!(!b.contains(Point::new(1.1, 3.0)));
    }

    #[test]
    fn empty_and_non_finite_give_none() {
        assert!(Bounds::from_points([]).is_none());
        assert!(Bounds::from_points([Point::new(f64::NAN, 0.0)]).is_none());
        assert!(
            Bounds::from_points([Point::new(0.0, 0.0), Point::new(f64::INFINITY, 0.0)]).is_none()
        );
    }

    #[test]
    fn union_merges() {
        let a = Bounds::from_points([Point::new(0.0, 0.0), Point::new(1.0, 1.0)]).unwrap();
        let b = Bounds::from_points([Point::new(2.0, -1.0), Point::new(3.0, 0.5)]).unwrap();
        let u = a.union(&b);
        assert_eq!(u.min, Point::new(0.0, -1.0));
        assert_eq!(u.max, Point::new(3.0, 1.0));
    }
}
