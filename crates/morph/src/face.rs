//! Face maths on top of `doc::FaceModel`: placing the model on a face box, fitting a box to
//! landmark positions, "smart" dragging one landmark with the rest following, and the weight
//! helpers behind face mixing. All pure functions on points; nothing here touches pixels.

use crate::MorphError;
use inbetween_doc::FaceModel;
use inbetween_geom::Point;

/// Largest face box side or coordinate, in pixels. Keeps hostile input finite.
pub const MAX_BOX: f64 = 10_000_000.0;
/// Most images a face mix blends.
pub const MAX_MIX_INPUTS: usize = 64;

fn face_error(message: impl Into<String>) -> MorphError {
    MorphError::Face(message.into())
}

/// The rectangle a face occupies on an image, in output pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl FaceBox {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Result<Self, MorphError> {
        let ok = [x, y, w, h]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= MAX_BOX)
            && w > 0.0
            && h > 0.0;
        if !ok {
            return Err(face_error(
                "a face box needs finite numbers and a width and height above zero",
            ));
        }
        Ok(Self { x, y, w, h })
    }

    /// A box centred in a frame, 80% of its height and four fifths as wide as it is tall (and
    /// no wider than the frame): a sensible starting point to adjust by hand.
    pub fn centered(width: u32, height: u32) -> Result<Self, MorphError> {
        let (fw, fh) = (f64::from(width), f64::from(height));
        let h = fh * 0.8;
        let w = (h * 0.8).min(fw * 0.9);
        Self::new((fw - w) / 2.0, (fh - h) / 2.0, w, h)
    }

    /// Where a point of the unit face box lands on this box.
    pub fn map(&self, u: f64, v: f64) -> Point {
        Point::new(self.x + u * self.w, self.y + v * self.h)
    }
}

/// The template positions (unit face box) of the model's enabled landmarks, in order.
pub fn template(model: &FaceModel) -> Vec<Point> {
    model
        .dots
        .iter()
        .filter(|d| d.enabled)
        .map(|d| Point::new(d.x, d.y))
        .collect()
}

/// Place the model's enabled landmarks on a face box.
pub fn place(model: &FaceModel, face: &FaceBox) -> Vec<Point> {
    template(model)
        .into_iter()
        .map(|p| face.map(p.x, p.y))
        .collect()
}

/// The face box that best explains `points` (the enabled landmarks, in model order): a least
/// squares fit of one scale and one offset per axis. Needs at least two landmarks that differ
/// in each direction.
pub fn fit_box(model: &FaceModel, points: &[Point]) -> Result<FaceBox, MorphError> {
    let t = template(model);
    if t.len() != points.len() {
        return Err(face_error(format!(
            "the face model has {} enabled landmarks but {} positions were given",
            t.len(),
            points.len()
        )));
    }
    if t.len() < 2 || points.iter().any(|p| !p.is_finite()) {
        return Err(face_error(
            "fitting a face needs at least two finite landmark positions",
        ));
    }
    let axis = |u: &[f64], p: &[f64]| -> Option<(f64, f64)> {
        let n = u.len() as f64;
        let (mu, mp) = (u.iter().sum::<f64>() / n, p.iter().sum::<f64>() / n);
        let var: f64 = u.iter().map(|a| (a - mu) * (a - mu)).sum();
        if var < 1e-12 {
            return None;
        }
        let cov: f64 = u.iter().zip(p).map(|(a, b)| (a - mu) * (b - mp)).sum();
        let scale = cov / var;
        Some((scale, mp - scale * mu))
    };
    let (tx, ty): (Vec<f64>, Vec<f64>) = t.iter().map(|p| (p.x, p.y)).unzip();
    let (px, py): (Vec<f64>, Vec<f64>) = points.iter().map(|p| (p.x, p.y)).unzip();
    let (Some((w, x)), Some((h, y))) = (axis(&tx, &px), axis(&ty, &py)) else {
        return Err(face_error(
            "the model's enabled landmarks do not span a face",
        ));
    };
    FaceBox::new(x, y, w, h).map_err(|_| {
        face_error("those landmark positions do not fit the model (the face would be flipped, flat or enormous)")
    })
}

/// Move landmark `index` of `points` (the enabled landmarks, in model order) to `to`, and drag
/// the others along: each moves by the same offset scaled by a Gaussian of its distance from
/// the dragged landmark on the model (`radius` in face-box units). Landmarks marked important
/// resist (they follow at 60%), so the eyes and mouth corners stay put while the cheek moves.
pub fn smart_drag(
    model: &FaceModel,
    points: &[Point],
    index: usize,
    to: Point,
    radius: f64,
) -> Result<Vec<Point>, MorphError> {
    let enabled: Vec<_> = model.dots.iter().filter(|d| d.enabled).collect();
    if enabled.len() != points.len() {
        return Err(face_error(format!(
            "the face model has {} enabled landmarks but {} dots were given",
            enabled.len(),
            points.len()
        )));
    }
    let from = *points
        .get(index)
        .ok_or_else(|| face_error(format!("there is no landmark {index} in this face")))?;
    if !to.is_finite() || !radius.is_finite() || radius <= 0.0 {
        return Err(face_error(
            "smart drag needs a finite target and a radius above zero",
        ));
    }
    let offset = to - from;
    let anchor = enabled.get(index).map(|d| Point::new(d.x, d.y));
    let mut out = points.to_vec();
    for (i, (p, dot)) in out.iter_mut().zip(&enabled).enumerate() {
        if i == index {
            *p = to;
            continue;
        }
        let Some(anchor) = anchor else { continue };
        let d = anchor.distance(Point::new(dot.x, dot.y)) / radius;
        let stiffness = if dot.important { 0.6 } else { 1.0 };
        *p = *p + offset * ((-d * d).exp() * stiffness);
    }
    Ok(out)
}

/// Scale non-negative weights so they sum to 1. All zero (or non-finite) is an error.
pub fn normalize(weights: &[f64]) -> Result<Vec<f64>, MorphError> {
    if weights.is_empty() || weights.len() > MAX_MIX_INPUTS {
        return Err(face_error(format!(
            "a mix takes 1 to {MAX_MIX_INPUTS} inputs"
        )));
    }
    if weights.iter().any(|w| !w.is_finite() || *w < 0.0) {
        return Err(face_error(
            "mix percentages must be finite and not negative",
        ));
    }
    let sum: f64 = weights.iter().sum();
    if sum <= 0.0 {
        return Err(face_error("mix percentages must not all be zero"));
    }
    Ok(weights.iter().map(|w| w / sum).collect())
}

/// Equal weights for `n` inputs.
pub fn average_weights(n: usize) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    vec![1.0 / n as f64; n]
}

/// Deterministic pseudo-random weights summing to 1 (a splitmix64 stream seeded with `seed`).
/// Every input gets at least a small share so none vanishes entirely.
pub fn random_weights(n: usize, seed: u64) -> Vec<f64> {
    let mut state = seed;
    let raw: Vec<f64> = (0..n)
        .map(|_| {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            0.05 + (z >> 11) as f64 / (1u64 << 53) as f64
        })
        .collect();
    let sum: f64 = raw.iter().sum();
    if sum <= 0.0 {
        return average_weights(n);
    }
    raw.iter().map(|r| r / sum).collect()
}

/// The weighted mean of several landmark sets (all the same length): the shape of a mix.
pub fn blend_points(sets: &[Vec<Point>], weights: &[f64]) -> Result<Vec<Point>, MorphError> {
    let weights = normalize(weights)?;
    if sets.len() != weights.len() {
        return Err(face_error("every input needs a percentage"));
    }
    let len = sets.first().map_or(0, Vec::len);
    if sets.iter().any(|s| s.len() != len) {
        return Err(face_error(
            "every input needs the same number of face dots to be mixed",
        ));
    }
    let mut out = vec![Point::new(0.0, 0.0); len];
    for (set, w) in sets.iter().zip(&weights) {
        for (o, p) in out.iter_mut().zip(set) {
            if !p.is_finite() {
                return Err(face_error("a face dot is not a finite position"));
            }
            *o = *o + *p * *w;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> FaceModel {
        FaceModel::standard()
    }

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn placing_then_fitting_recovers_the_box() {
        let face = FaceBox::new(120.0, 80.0, 200.0, 260.0).unwrap();
        for m in [FaceModel::standard(), FaceModel::minimal()] {
            let pts = place(&m, &face);
            assert_eq!(pts.len(), m.active_count());
            let fit = fit_box(&m, &pts).unwrap();
            assert!(
                (fit.x - face.x).abs() < 1e-9
                    && (fit.y - face.y).abs() < 1e-9
                    && (fit.w - face.w).abs() < 1e-9
                    && (fit.h - face.h).abs() < 1e-9,
                "{fit:?}"
            );
        }
    }

    #[test]
    fn fitting_tolerates_noise_and_refuses_nonsense() {
        let m = model();
        let face = FaceBox::new(10.0, 20.0, 100.0, 120.0).unwrap();
        let mut pts = place(&m, &face);
        for (i, p) in pts.iter_mut().enumerate() {
            p.x += if i % 2 == 0 { 1.5 } else { -1.5 };
        }
        let fit = fit_box(&m, &pts).unwrap();
        assert!((fit.w - 100.0).abs() < 8.0 && (fit.h - 120.0).abs() < 8.0);
        assert!(fit_box(&m, &pts[..3]).is_err());
        let same = vec![Point::new(5.0, 5.0); pts.len()];
        assert!(fit_box(&m, &same).is_err());
        let mut nan = pts.clone();
        nan[0].x = f64::NAN;
        assert!(fit_box(&m, &nan).is_err());
        // A mirrored face has a negative width, which is not a box.
        let mirrored: Vec<_> = pts.iter().map(|p| Point::new(-p.x, p.y)).collect();
        assert!(fit_box(&m, &mirrored).is_err());
    }

    #[test]
    fn boxes_reject_hostile_numbers() {
        for (x, y, w, h) in [
            (f64::NAN, 0.0, 1.0, 1.0),
            (0.0, 0.0, 0.0, 1.0),
            (0.0, 0.0, 1.0, -1.0),
            (0.0, 0.0, f64::INFINITY, 1.0),
            (1e12, 0.0, 1.0, 1.0),
        ] {
            assert!(FaceBox::new(x, y, w, h).is_err());
        }
        let c = FaceBox::centered(1000, 500).unwrap();
        assert!((c.h - 400.0).abs() < 1e-9 && (c.x + c.w / 2.0 - 500.0).abs() < 1e-9);
        // A very wide frame still gives a face no wider than the frame.
        let c = FaceBox::centered(100, 1000).unwrap();
        assert!(c.w <= 90.0 + 1e-9);
        assert!(FaceBox::centered(0, 10).is_err());
    }

    #[test]
    fn smart_drag_moves_the_target_exactly_and_neighbours_partly() {
        let m = model();
        let face = FaceBox::new(0.0, 0.0, 100.0, 100.0).unwrap();
        let pts = place(&m, &face);
        let nose_tip = m.dots.iter().position(|d| d.name == "nose_tip").unwrap();
        let nostril = m
            .dots
            .iter()
            .position(|d| d.name == "nostril_right")
            .unwrap();
        let chin = m.dots.iter().position(|d| d.name == "chin").unwrap();
        let to = pts[nose_tip] + Point::new(10.0, 0.0);
        let out = smart_drag(&m, &pts, nose_tip, to, 0.15).unwrap();
        assert!(close(out[nose_tip], to));
        let nostril_move = out[nostril].x - pts[nostril].x;
        assert!(nostril_move > 5.0 && nostril_move < 10.0, "{nostril_move}");
        // The far chin barely notices.
        assert!((out[chin].x - pts[chin].x).abs() < 0.5);
        // Dragging by zero changes nothing.
        let same = smart_drag(&m, &pts, nose_tip, pts[nose_tip], 0.3).unwrap();
        assert_eq!(same, pts);
    }

    #[test]
    fn important_landmarks_resist_smart_drag() {
        let m = model();
        let face = FaceBox::new(0.0, 0.0, 100.0, 100.0).unwrap();
        let pts = place(&m, &face);
        let by_name = |n: &str| m.dots.iter().position(|d| d.name == n).unwrap();
        let (a, important, plain) = (
            by_name("lip_top_mid"),
            by_name("mouth_left_corner"),
            by_name("lip_top_left"),
        );
        let out = smart_drag(&m, &pts, a, pts[a] + Point::new(0.0, 10.0), 0.5).unwrap();
        let moved = |i: usize| (out[i].y - pts[i].y).abs();
        assert!(m.dots[important].important && !m.dots[plain].important);
        // `plain` is nearer than `important` here, so compare each against its own Gaussian.
        let base = |i: usize| {
            let d = Point::new(m.dots[a].x, m.dots[a].y)
                .distance(Point::new(m.dots[i].x, m.dots[i].y))
                / 0.5;
            10.0 * (-d * d).exp()
        };
        assert!((moved(plain) - base(plain)).abs() < 1e-9);
        assert!((moved(important) - 0.6 * base(important)).abs() < 1e-9);
    }

    #[test]
    fn smart_drag_only_sees_enabled_landmarks() {
        let m = FaceModel::minimal();
        let pts = place(&m, &FaceBox::new(0.0, 0.0, 50.0, 50.0).unwrap());
        assert_eq!(pts.len(), m.active_count());
        assert!(smart_drag(&m, &pts, 0, Point::new(1.0, 1.0), 0.3).is_ok());
        assert!(smart_drag(&m, &pts, pts.len(), Point::new(1.0, 1.0), 0.3).is_err());
        assert!(smart_drag(&m, &pts[1..], 0, Point::new(1.0, 1.0), 0.3).is_err());
        for (to, r) in [
            (Point::new(f64::NAN, 0.0), 0.3),
            (Point::new(0.0, 0.0), 0.0),
            (Point::new(0.0, 0.0), f64::NAN),
            (Point::new(0.0, 0.0), -1.0),
        ] {
            assert!(smart_drag(&m, &pts, 0, to, r).is_err());
        }
    }

    #[test]
    fn weights_normalise_average_and_randomise() {
        assert_eq!(normalize(&[1.0, 3.0]).unwrap(), vec![0.25, 0.75]);
        for bad in [&[][..], &[0.0, 0.0], &[-1.0, 2.0], &[f64::NAN, 1.0]] {
            assert!(normalize(bad).is_err());
        }
        assert!(normalize(&vec![1.0; MAX_MIX_INPUTS + 1]).is_err());
        assert_eq!(average_weights(4), vec![0.25; 4]);
        assert!(average_weights(0).is_empty());
        let a = random_weights(5, 7);
        assert_eq!(a, random_weights(5, 7));
        assert_ne!(a, random_weights(5, 8));
        assert!((a.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!(a.iter().all(|w| *w > 0.0));
    }

    #[test]
    fn blending_is_a_weighted_mean() {
        let a = vec![Point::new(0.0, 0.0), Point::new(10.0, 10.0)];
        let b = vec![Point::new(10.0, 0.0), Point::new(20.0, 30.0)];
        let mid = blend_points(&[a.clone(), b.clone()], &[1.0, 1.0]).unwrap();
        assert_eq!(mid, vec![Point::new(5.0, 0.0), Point::new(15.0, 20.0)]);
        assert_eq!(
            blend_points(&[a.clone(), b.clone()], &[1.0, 0.0]).unwrap(),
            a
        );
        assert!(blend_points(&[a.clone(), b[..1].to_vec()], &[1.0, 1.0]).is_err());
        assert!(blend_points(std::slice::from_ref(&a), &[1.0, 1.0]).is_err());
        assert!(blend_points(&[vec![Point::new(f64::NAN, 0.0)]], &[1.0]).is_err());
    }
}
