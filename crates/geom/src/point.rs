use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul, Sub};

/// A 2D point or vector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Linear interpolation: `self` at `t = 0`, `other` at `t = 1`.
    pub fn lerp(self, other: Self, t: f64) -> Self {
        self + (other - self) * t
    }

    pub fn distance(self, other: Self) -> f64 {
        let d = other - self;
        d.x.hypot(d.y)
    }

    /// Z component of the 2D cross product.
    pub fn cross(self, other: Self) -> f64 {
        self.x * other.y - self.y * other.x
    }
}

impl Add for Point {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Point {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_hits_endpoints_and_midpoint() {
        let a = Point::new(0.0, 10.0);
        let b = Point::new(4.0, 20.0);
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        assert_eq!(a.lerp(b, 0.5), Point::new(2.0, 15.0));
    }

    #[test]
    fn distance_and_cross() {
        assert_eq!(Point::new(0.0, 0.0).distance(Point::new(3.0, 4.0)), 5.0);
        assert_eq!(Point::new(1.0, 0.0).cross(Point::new(0.0, 1.0)), 1.0);
    }

    #[test]
    fn non_finite_is_detected() {
        assert!(!Point::new(f64::NAN, 0.0).is_finite());
        assert!(!Point::new(0.0, f64::INFINITY).is_finite());
        assert!(Point::new(1.0, -2.0).is_finite());
    }
}
