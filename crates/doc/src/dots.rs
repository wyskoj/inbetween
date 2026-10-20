use crate::DocError;
use inbetween_geom::Point;
use serde::{Deserialize, Serialize};

/// Which images a dot exists on. A dot on one image only has no partner yet and does not
/// take part in the morph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    /// On both images (a normal pair).
    #[default]
    Both,
    /// On Image 1 only.
    Only1,
    /// On Image 2 only.
    Only2,
}

impl Side {
    pub fn is_both(&self) -> bool {
        *self == Side::Both
    }
}

/// One key dot placed on both images. Coordinates are in output (frame) pixels, y down,
/// with the origin at the top-left corner of the frame.
///
/// When `side` is not `Both`, the missing end is stored equal to the present end, so a
/// reader that ignores `side` sees a harmless fixed anchor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DotPair {
    /// Position on Image 1.
    pub first: Point,
    /// Matching position on Image 2.
    pub second: Point,
    /// Which images the dot exists on.
    #[serde(skip_serializing_if = "Side::is_both")]
    pub side: Side,
}

impl DotPair {
    pub const fn new(first: Point, second: Point) -> Self {
        Self {
            first,
            second,
            side: Side::Both,
        }
    }

    /// A dot that stays where it is in both images.
    pub const fn fixed(at: Point) -> Self {
        Self::new(at, at)
    }

    /// A dot that exists on one image only.
    pub const fn unpaired(at: Point, side: Side) -> Self {
        Self {
            first: at,
            second: at,
            side,
        }
    }

    pub fn is_paired(&self) -> bool {
        self.side.is_both()
    }

    /// Whether the dot exists on image `which` (1 or 2).
    pub fn has_end(&self, which: u8) -> bool {
        match self.side {
            Side::Both => true,
            Side::Only1 => which == 1,
            Side::Only2 => which == 2,
        }
    }

    /// Position on image `which` (1 or 2; anything else reads as 2).
    pub fn end(&self, which: u8) -> Point {
        if which == 1 {
            self.first
        } else {
            self.second
        }
    }

    /// Move the dot on image `which`. Does nothing when the dot is not on that image. An
    /// unpaired dot keeps its missing end equal to the present one.
    pub fn set_end(&mut self, which: u8, p: Point) {
        if !self.has_end(which) {
            return;
        }
        if which == 1 || !self.is_paired() {
            self.first = p;
        }
        if which != 1 || !self.is_paired() {
            self.second = p;
        }
    }

    /// Swap the two ends (and which image an unpaired dot is on).
    pub fn swap_ends(&mut self) {
        std::mem::swap(&mut self.first, &mut self.second);
        self.side = match self.side {
            Side::Both => Side::Both,
            Side::Only1 => Side::Only2,
            Side::Only2 => Side::Only1,
        };
    }
}

/// A line joining two dots (pair indices). It shows on both images and never reaches the morph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Line {
    pub a: u32,
    pub b: u32,
}

/// An ordered set of key dots. Index `i` on Image 1 corresponds to index `i` on Image 2.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DotSet {
    pub pairs: Vec<DotPair>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<Line>,
}

impl DotSet {
    pub fn new(pairs: Vec<DotPair>) -> Self {
        Self {
            pairs,
            lines: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Dot positions on Image 1, in order, paired dots only.
    pub fn first_positions(&self) -> Vec<Point> {
        self.pairs
            .iter()
            .filter(|p| p.is_paired())
            .map(|p| p.first)
            .collect()
    }

    /// Dot positions on Image 2, in order, paired dots only.
    pub fn second_positions(&self) -> Vec<Point> {
        self.pairs
            .iter()
            .filter(|p| p.is_paired())
            .map(|p| p.second)
            .collect()
    }

    /// Positions at morph time `t` (0 = Image 1, 1 = Image 2), linearly interpolated,
    /// paired dots only.
    pub fn interpolated(&self, t: f64) -> Vec<Point> {
        self.pairs
            .iter()
            .filter(|p| p.is_paired())
            .map(|p| p.first.lerp(p.second, t))
            .collect()
    }

    /// Remove dot `i` and every line that touches it; later dots shift down by one and the
    /// lines are renumbered. Returns the removed dot, or `None` when `i` is out of range.
    pub fn remove(&mut self, i: usize) -> Option<DotPair> {
        if i >= self.pairs.len() {
            return None;
        }
        let removed = self.pairs.remove(i);
        let i = i as u32;
        self.lines.retain(|l| l.a != i && l.b != i);
        for l in &mut self.lines {
            if l.a > i {
                l.a -= 1;
            }
            if l.b > i {
                l.b -= 1;
            }
        }
        Some(removed)
    }

    /// Join dots `a` and `b` with a line. Returns false (and changes nothing) when an index
    /// is out of range, `a == b`, or the line already exists.
    pub fn connect(&mut self, a: usize, b: usize) -> bool {
        if a == b || a >= self.pairs.len() || b >= self.pairs.len() {
            return false;
        }
        let (Ok(a), Ok(b)) = (u32::try_from(a), u32::try_from(b)) else {
            return false;
        };
        if self
            .lines
            .iter()
            .any(|l| (l.a == a && l.b == b) || (l.a == b && l.b == a))
        {
            return false;
        }
        self.lines.push(Line { a, b });
        true
    }

    /// Every dot reachable from `start` along lines, `start` included, in ascending order.
    /// Empty when `start` is out of range.
    pub fn connected(&self, start: usize) -> Vec<usize> {
        let n = self.pairs.len();
        if start >= n {
            return Vec::new();
        }
        let mut seen = vec![false; n];
        let mut stack = vec![start];
        if let Some(s) = seen.get_mut(start) {
            *s = true;
        }
        while let Some(i) = stack.pop() {
            for l in &self.lines {
                let (a, b) = (l.a as usize, l.b as usize);
                let other = if a == i {
                    b
                } else if b == i {
                    a
                } else {
                    continue;
                };
                if let Some(s) = seen.get_mut(other) {
                    if !*s {
                        *s = true;
                        stack.push(other);
                    }
                }
            }
        }
        seen.iter()
            .enumerate()
            .filter_map(|(i, s)| s.then_some(i))
            .collect()
    }

    /// Check that every line is in range, joins two different dots and is not a duplicate.
    pub fn validate_lines(&self) -> Result<(), DocError> {
        let n = self.pairs.len();
        let mut seen = std::collections::HashSet::new();
        for l in &self.lines {
            if l.a as usize >= n || l.b as usize >= n {
                return Err(DocError::BadLine(
                    "a line points at a dot that does not exist",
                ));
            }
            if l.a == l.b {
                return Err(DocError::BadLine("a line joins a dot to itself"));
            }
            if !seen.insert((l.a.min(l.b), l.a.max(l.b))) {
                return Err(DocError::BadLine("a line is listed twice"));
            }
        }
        Ok(())
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
    fn unpaired_dots_are_left_out_of_morph_lists() {
        let mut s = set();
        s.pairs
            .push(DotPair::unpaired(Point::new(9.0, 9.0), Side::Only1));
        assert_eq!(s.len(), 3);
        assert_eq!(s.first_positions().len(), 2);
        assert_eq!(s.second_positions().len(), 2);
        assert_eq!(s.interpolated(0.5).len(), 2);
    }

    #[test]
    fn remove_renumbers_lines() {
        let mut s = DotSet::new(
            (0..4)
                .map(|i| DotPair::fixed(Point::new(i as f64, 0.0)))
                .collect(),
        );
        assert!(s.connect(0, 1) && s.connect(1, 2) && s.connect(2, 3));
        assert!(!s.connect(1, 0) && !s.connect(2, 2) && !s.connect(0, 9));
        assert_eq!(s.connected(0), vec![0, 1, 2, 3]);
        assert!(s.remove(1).is_some());
        assert_eq!(s.lines, vec![Line { a: 1, b: 2 }]);
        assert_eq!(s.connected(0), vec![0]);
        assert_eq!(s.connected(1), vec![1, 2]);
        assert!(s.remove(10).is_none());
        assert!(s.connected(10).is_empty());
        assert!(s.validate_lines().is_ok());
    }

    #[test]
    fn bad_lines_are_rejected() {
        let mut s = set();
        for l in [Line { a: 0, b: 5 }, Line { a: 1, b: 1 }] {
            s.lines = vec![l];
            assert!(s.validate_lines().is_err());
        }
        s.lines = vec![Line { a: 0, b: 1 }, Line { a: 1, b: 0 }];
        assert!(s.validate_lines().is_err());
    }

    #[test]
    fn side_and_lines_are_only_written_when_used() {
        let json = serde_json::to_string(&set()).unwrap();
        assert!(!json.contains("side") && !json.contains("lines"));
        let mut s = set();
        s.pairs[0].side = Side::Only2;
        s.connect(0, 1);
        let back: DotSet = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn empty_set() {
        let s = DotSet::default();
        assert!(s.is_empty());
        assert!(s.interpolated(0.3).is_empty());
    }
}
