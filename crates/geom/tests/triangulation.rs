//! Property tests: triangulations tile the plane exactly and never panic.

use inbetween_geom::{triangulate, GeomError, Point, Triangulation};
use proptest::prelude::*;

const W: f64 = 64.0;
const H: f64 = 48.0;

fn area_sum(t: &Triangulation) -> f64 {
    (0..t.triangles.len())
        .filter_map(|i| t.triangle(i))
        .map(|tri| tri.area())
        .sum()
}

/// Area of the convex hull (monotone chain).
fn hull_area(points: &[Point]) -> f64 {
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup();
    if pts.len() < 3 {
        return 0.0;
    }
    let cross = |o: Point, a: Point, b: Point| (a - o).cross(b - o);
    let mut hull: Vec<Point> = Vec::new();
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &Point>> = if pass == 0 {
            Box::new(pts.iter())
        } else {
            Box::new(pts.iter().rev())
        };
        for &p in iter {
            while hull.len() >= start + 2
                && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    let mut twice = 0.0;
    for i in 0..hull.len() {
        twice += hull[i].cross(hull[(i + 1) % hull.len()]);
    }
    0.5 * twice.abs()
}

fn has_duplicates(dots: &[Point]) -> bool {
    dots.iter().enumerate().any(|(i, p)| dots[..i].contains(p))
}

/// Integer-ish coordinates make duplicates, collinear runs and corner hits common.
fn dot(lo: i32, hi: i32) -> impl Strategy<Value = Point> {
    (lo..=hi, lo..=hi).prop_map(|(x, y)| Point::new(f64::from(x), f64::from(y) * 0.75))
}

fn check_tiling(dots: &[Point], samples: &[Point]) -> Result<(), TestCaseError> {
    let t = triangulate(dots, W, H).expect("distinct finite dots must triangulate");
    let mut all = dots.to_vec();
    all.extend([
        Point::new(0.0, 0.0),
        Point::new(W, 0.0),
        Point::new(W, H),
        Point::new(0.0, H),
    ]);

    // No gaps or overlaps: the triangles' areas add up to the hull's area exactly.
    let hull = hull_area(&all);
    prop_assert!(
        (area_sum(&t) - hull).abs() <= 1e-9 * hull.max(1.0),
        "{} vs {}",
        area_sum(&t),
        hull
    );

    for i in 0..t.triangles.len() {
        let tri = t.triangle(i).expect("index in range");
        prop_assert!(
            tri.signed_area() > 0.0,
            "triangle {i} not CCW or degenerate"
        );
    }

    // Every sample in the frame lies in at least one triangle, and in exactly one if it
    // is clear of all edges.
    for &p in samples {
        let mut hits = 0;
        let mut interior = 0;
        for i in 0..t.triangles.len() {
            let tri = t.triangle(i).expect("index in range");
            if tri.contains(p, 1e-9) {
                hits += 1;
            }
            if tri.contains(p, -1e-6) {
                interior += 1;
            }
        }
        prop_assert!(hits >= 1, "{p:?} not covered");
        prop_assert!(interior <= 1, "{p:?} inside {interior} triangles");
    }
    Ok(())
}

fn samples() -> impl Strategy<Value = Vec<Point>> {
    prop::collection::vec((0.0..=W, 0.0..=H).prop_map(|(x, y)| Point::new(x, y)), 20)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn dots_inside_the_frame_tile_it_exactly(
        raw in prop::collection::vec(dot(0, 64), 0..60),
        s in samples(),
    ) {
        let mut dots = raw;
        // Keep only the first of each duplicate position.
        let mut seen: Vec<Point> = Vec::new();
        dots.retain(|p| if seen.contains(p) { false } else { seen.push(*p); true });
        let dots: Vec<Point> = dots.into_iter().filter(|p| p.y <= H).collect();
        check_tiling(&dots, &s)?;
        // With every dot inside the frame the hull is the frame.
        let t = triangulate(&dots, W, H).expect("triangulates");
        prop_assert!((area_sum(&t) - W * H).abs() < 1e-6);
    }

    #[test]
    fn dots_outside_the_frame_still_cover_it(
        raw in prop::collection::vec(dot(-40, 110), 0..60),
        s in samples(),
    ) {
        let mut seen: Vec<Point> = Vec::new();
        let dots: Vec<Point> =
            raw.into_iter().filter(|p| if seen.contains(p) { false } else { seen.push(*p); true }).collect();
        check_tiling(&dots, &s)?;
    }

    #[test]
    fn arbitrary_input_never_panics(
        raw in prop::collection::vec(
            (prop::num::f64::ANY, prop::num::f64::ANY).prop_map(|(x, y)| Point::new(x, y)),
            0..40,
        ),
        w in prop::num::f64::ANY,
        h in prop::num::f64::ANY,
    ) {
        // Any result is fine; reaching the end proves there was no panic.
        let _ = triangulate(&raw, w, h);
    }

    #[test]
    fn duplicates_are_reported_not_hidden(raw in prop::collection::vec(dot(0, 6), 0..30)) {
        match triangulate(&raw, W, H) {
            Err(GeomError::DuplicateDot { first, second }) => {
                prop_assert!(first < second);
                prop_assert_eq!(raw[first], raw[second]);
            }
            Ok(_) => prop_assert!(!has_duplicates(&raw)),
            Err(e) => prop_assert!(false, "unexpected error {e}"),
        }
    }
}
