use inbetween_geom::Point;
use serde::{Deserialize, Serialize};

/// One key dot placed on both images. Coordinates are in output (frame) pixels, y down,
/// with the origin at the top-left corner of the frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DotPair {
    /// Position on Image 1.
    pub first: Point,
    /// Matching position on Image 2.
    pub second: Point,
}

impl DotPair {
    pub const fn new(first: Point, second: Point) -> Self {
        Self { first, second }
    }

    /// A dot that stays where it is in both images.
    pub const fn fixed(at: Point) -> Self {
        Self {
            first: at,
            second: at,
        }
    }
}

/// An ordered set of paired dots. Index `i` on Image 1 corresponds to index `i` on Image 2.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DotSet {
    pub pairs: Vec<DotPair>,
}

impl DotSet {
    pub fn new(pairs: Vec<DotPair>) -> Self {
        Self { pairs }
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Dot positions on Image 1, in order.
    pub fn first_positions(&self) -> Vec<Point> {
        self.pairs.iter().map(|p| p.first).collect()
    }

    /// Dot positions on Image 2, in order.
    pub fn second_positions(&self) -> Vec<Point> {
        self.pairs.iter().map(|p| p.second).collect()
    }

    /// Positions at morph time `t` (0 = Image 1, 1 = Image 2), linearly interpolated.
    pub fn interpolated(&self, t: f64) -> Vec<Point> {
        self.pairs
            .iter()
            .map(|p| p.first.lerp(p.second, t))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> DotSet {
        DotSet::new(vec![
            DotPair::new(Point::new(0.0, 0.0), Point::new(10.0, 20.0)),
            DotPair::fixed(Point::new(5.0, 5.0)),
        ])
    }

    #[test]
    fn positions_and_interpolation() {
        let s = set();
        assert_eq!(s.len(), 2);
        assert_eq!(s.first_positions()[0], Point::new(0.0, 0.0));
        assert_eq!(s.second_positions()[0], Point::new(10.0, 20.0));
        assert_eq!(s.interpolated(0.0), s.first_positions());
        assert_eq!(s.interpolated(1.0), s.second_positions());
        assert_eq!(s.interpolated(0.5)[0], Point::new(5.0, 10.0));
        assert_eq!(s.interpolated(0.5)[1], Point::new(5.0, 5.0));
    }

    #[test]
    fn empty_set() {
        let s = DotSet::default();
        assert!(s.is_empty());
        assert!(s.interpolated(0.3).is_empty());
    }
}
