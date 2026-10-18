//! Picking dots with the pointer.

use egui::Pos2;

/// The dot nearest to `pointer` within `radius` screen points, if any. When two dots are
/// equally near, the later one wins (it is drawn on top).
pub fn pick(dots: &[Pos2], pointer: Pos2, radius: f32) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, d) in dots.iter().enumerate() {
        let dist = (*d - pointer).length();
        if dist.is_finite() && dist <= radius && best.is_none_or(|(_, b)| dist <= b) {
            best = Some((i, dist));
        }
    }
    best.map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dots() -> Vec<Pos2> {
        vec![
            Pos2::new(10.0, 10.0),
            Pos2::new(40.0, 10.0),
            Pos2::new(42.0, 10.0),
        ]
    }

    #[test]
    fn picks_the_nearest_dot_in_range() {
        assert_eq!(pick(&dots(), Pos2::new(12.0, 11.0), 8.0), Some(0));
        assert_eq!(pick(&dots(), Pos2::new(41.0, 10.0), 8.0), Some(2)); // tie goes to the later dot
        assert_eq!(pick(&dots(), Pos2::new(39.0, 10.0), 8.0), Some(1));
    }

    #[test]
    fn misses_outside_the_radius() {
        assert_eq!(pick(&dots(), Pos2::new(100.0, 100.0), 8.0), None);
        assert_eq!(pick(&[], Pos2::new(0.0, 0.0), 8.0), None);
        assert_eq!(pick(&dots(), Pos2::new(10.0, 19.0), 8.0), None);
    }

    #[test]
    fn nan_positions_never_match() {
        let dots = vec![Pos2::new(f32::NAN, 0.0), Pos2::new(5.0, 5.0)];
        assert_eq!(pick(&dots, Pos2::new(5.0, 5.0), 3.0), Some(1));
        assert_eq!(pick(&dots, Pos2::new(f32::NAN, 0.0), 3.0), None);
    }
}
