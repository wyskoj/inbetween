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

/// Distance from `p` to the segment `a`-`b`, or infinity when anything is not finite.
fn segment_distance(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    let (ab, ap) = (b - a, p - a);
    let len2 = ab.x * ab.x + ab.y * ab.y;
    let t = if len2 > 0.0 {
        ((ap.x * ab.x + ap.y * ab.y) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let d = (a + ab * t - p).length();
    if d.is_finite() {
        d
    } else {
        f32::INFINITY
    }
}

/// The segment nearest to `pointer` within `radius` screen points, if any. Ties go to the
/// later segment (drawn on top).
pub fn pick_segment(segments: &[(Pos2, Pos2)], pointer: Pos2, radius: f32) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, (a, b)) in segments.iter().enumerate() {
        let d = segment_distance(*a, *b, pointer);
        if d <= radius && best.is_none_or(|(_, bd)| d <= bd) {
            best = Some((i, d));
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

    #[test]
    fn picks_the_nearest_segment() {
        let segs = [
            (Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0)),
            (Pos2::new(0.0, 10.0), Pos2::new(100.0, 10.0)),
        ];
        assert_eq!(pick_segment(&segs, Pos2::new(50.0, 3.0), 5.0), Some(0));
        assert_eq!(pick_segment(&segs, Pos2::new(50.0, 8.0), 5.0), Some(1));
        assert_eq!(pick_segment(&segs, Pos2::new(50.0, 50.0), 5.0), None);
        // Beyond the end the distance is to the end point.
        assert_eq!(pick_segment(&segs, Pos2::new(104.0, 0.0), 5.0), Some(0));
        assert_eq!(pick_segment(&segs, Pos2::new(110.0, 0.0), 5.0), None);
    }

    #[test]
    fn degenerate_and_nan_segments_never_panic_or_match() {
        let dot = (Pos2::new(5.0, 5.0), Pos2::new(5.0, 5.0));
        assert_eq!(pick_segment(&[dot], Pos2::new(6.0, 5.0), 3.0), Some(0));
        let nan = (Pos2::new(f32::NAN, 0.0), Pos2::new(5.0, 5.0));
        assert_eq!(pick_segment(&[nan], Pos2::new(5.0, 5.0), 3.0), None);
        assert_eq!(pick_segment(&[dot], Pos2::new(f32::NAN, 0.0), 3.0), None);
        assert_eq!(pick_segment(&[], Pos2::ZERO, 3.0), None);
    }
}
