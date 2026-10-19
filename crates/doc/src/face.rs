//! Inbetween's face dot model: which landmarks a face has and where they sit on a unit face
//! box. Pure data; the placement and fitting maths live in `morph::face`.
//!
//! The model is Inbetween's own design. It is deliberately independent of any detector: a
//! detector (or a person) supplies a face box or landmark positions in the model's order, and
//! the model says what each position means.

use crate::DocError;
use serde::{Deserialize, Serialize};

/// Most landmarks a model may hold.
pub const MAX_FACE_DOTS: usize = 512;
/// Most guide lines a model may hold.
pub const MAX_FACE_GUIDES: usize = 1024;
/// Longest landmark or model name, in bytes.
pub const MAX_FACE_NAME: usize = 64;

/// One landmark of a face model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FaceDot {
    pub name: String,
    /// Position on the unit face box: x to the right, y down, both 0 to 1 for a point inside
    /// the face.
    pub x: f64,
    pub y: f64,
    /// Whether the landmark is used. Switched-off landmarks are skipped everywhere.
    pub enabled: bool,
    /// Highlighted in the editor and held stiffer by smart drag.
    pub important: bool,
}

impl Default for FaceDot {
    fn default() -> Self {
        Self {
            name: String::new(),
            x: 0.5,
            y: 0.5,
            enabled: true,
            important: false,
        }
    }
}

/// A set of landmarks plus guide lines (pairs of landmark indexes drawn as lines).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FaceModel {
    pub name: String,
    pub dots: Vec<FaceDot>,
    /// Guide lines between landmarks, as indexes into `dots`.
    pub guides: Vec<[usize; 2]>,
}

/// Names of the built-in presets.
pub const FACE_PRESETS: &[&str] = &["standard", "minimal"];

impl Default for FaceModel {
    fn default() -> Self {
        Self::standard()
    }
}

/// (name, x, y, important) for the standard model.
const STANDARD: &[(&str, f64, f64, bool)] = &[
    ("jaw_right_top", 0.04, 0.38, false),
    ("jaw_right_mid", 0.07, 0.58, false),
    ("jaw_right_low", 0.17, 0.78, false),
    ("jaw_right_chin", 0.32, 0.92, false),
    ("chin", 0.50, 0.98, true),
    ("jaw_left_chin", 0.68, 0.92, false),
    ("jaw_left_low", 0.83, 0.78, false),
    ("jaw_left_mid", 0.93, 0.58, false),
    ("jaw_left_top", 0.96, 0.38, false),
    ("hairline_right", 0.20, 0.12, false),
    ("hairline_mid", 0.50, 0.04, false),
    ("hairline_left", 0.80, 0.12, false),
    ("brow_right_outer", 0.18, 0.32, false),
    ("brow_right_peak", 0.30, 0.27, false),
    ("brow_right_inner", 0.43, 0.32, false),
    ("brow_left_inner", 0.57, 0.32, false),
    ("brow_left_peak", 0.70, 0.27, false),
    ("brow_left_outer", 0.82, 0.32, false),
    ("eye_right_outer", 0.20, 0.42, true),
    ("eye_right_top", 0.30, 0.39, false),
    ("eye_right_inner", 0.41, 0.43, true),
    ("eye_right_bottom", 0.30, 0.46, false),
    ("pupil_right", 0.30, 0.425, false),
    ("eye_left_inner", 0.59, 0.43, true),
    ("eye_left_top", 0.70, 0.39, false),
    ("eye_left_outer", 0.80, 0.42, true),
    ("eye_left_bottom", 0.70, 0.46, false),
    ("pupil_left", 0.70, 0.425, false),
    ("nose_bridge", 0.50, 0.45, false),
    ("nose_tip", 0.50, 0.64, true),
    ("nostril_right", 0.42, 0.66, false),
    ("nostril_left", 0.58, 0.66, false),
    ("mouth_right_corner", 0.34, 0.78, true),
    ("lip_top_right", 0.42, 0.755, false),
    ("lip_top_mid", 0.50, 0.74, false),
    ("lip_top_left", 0.58, 0.755, false),
    ("mouth_left_corner", 0.66, 0.78, true),
    ("lip_bottom_mid", 0.50, 0.84, false),
];

impl FaceModel {
    /// The standard model: outline, hairline, brows, eyes, nose and mouth (38 landmarks).
    pub fn standard() -> Self {
        let dots = STANDARD
            .iter()
            .map(|&(name, x, y, important)| FaceDot {
                name: name.to_string(),
                x,
                y,
                enabled: true,
                important,
            })
            .collect();
        let chain = |a: usize, b: usize| (a..b).map(|i| [i, i + 1]).collect::<Vec<_>>();
        let mut guides = chain(0, 8);
        guides.extend(chain(12, 14));
        guides.extend(chain(15, 17));
        guides.extend([[18, 19], [19, 20], [20, 21], [21, 18]]);
        guides.extend([[23, 24], [24, 25], [25, 26], [26, 23]]);
        guides.extend([[32, 33], [33, 34], [34, 35], [35, 36], [36, 37], [37, 32]]);
        Self {
            name: "standard".into(),
            dots,
            guides,
        }
    }

    /// The minimal model: only the landmarks marked important, enough to line up the main
    /// features. Same landmark order as the standard model, so files stay compatible.
    pub fn minimal() -> Self {
        let mut model = Self::standard();
        for dot in &mut model.dots {
            dot.enabled = dot.important;
        }
        model.name = "minimal".into();
        model
    }

    pub fn preset(name: &str) -> Option<Self> {
        match name {
            "standard" => Some(Self::standard()),
            "minimal" => Some(Self::minimal()),
            _ => None,
        }
    }

    /// Indexes (into `dots`) of the enabled landmarks, in order. These are the landmarks a
    /// face puts on an image.
    pub fn active(&self) -> Vec<usize> {
        self.dots
            .iter()
            .enumerate()
            .filter(|(_, d)| d.enabled)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn active_count(&self) -> usize {
        self.dots.iter().filter(|d| d.enabled).count()
    }

    pub fn validate(&self) -> Result<(), DocError> {
        let bad = |why| Err(DocError::BadFace(why));
        if self.dots.len() > MAX_FACE_DOTS {
            return bad("too many landmarks");
        }
        if self.guides.len() > MAX_FACE_GUIDES {
            return bad("too many guide lines");
        }
        if self.name.len() > MAX_FACE_NAME || self.dots.iter().any(|d| d.name.len() > MAX_FACE_NAME)
        {
            return bad("a name is too long");
        }
        if self
            .dots
            .iter()
            .any(|d| !d.x.is_finite() || !d.y.is_finite() || d.x.abs() > 100.0 || d.y.abs() > 100.0)
        {
            return bad("a landmark position is not a sensible number");
        }
        if self
            .guides
            .iter()
            .any(|g| g[0] >= self.dots.len() || g[1] >= self.dots.len() || g[0] == g[1])
        {
            return bad("a guide line joins a missing landmark, or a landmark to itself");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_valid_and_share_an_order() {
        let s = FaceModel::standard();
        let m = FaceModel::minimal();
        s.validate().unwrap();
        m.validate().unwrap();
        assert_eq!(s.dots.len(), 38);
        assert_eq!(m.dots.len(), 38);
        assert_eq!(s.active_count(), 38);
        assert!(m.active_count() < 15 && m.active_count() > 5);
        assert!(m.dots.iter().all(|d| d.enabled == d.important));
        assert!(FaceModel::preset("nope").is_none());
        for p in FACE_PRESETS {
            assert!(FaceModel::preset(p).is_some());
        }
    }

    #[test]
    fn standard_landmarks_sit_inside_the_box_and_do_not_coincide() {
        let s = FaceModel::standard();
        for (i, a) in s.dots.iter().enumerate() {
            assert!(
                (0.0..=1.0).contains(&a.x) && (0.0..=1.0).contains(&a.y),
                "{}",
                a.name
            );
            for b in &s.dots[i + 1..] {
                assert!(
                    (a.x - b.x).hypot(a.y - b.y) > 0.01,
                    "{} / {}",
                    a.name,
                    b.name
                );
            }
        }
    }

    #[test]
    fn hostile_models_are_refused() {
        let mut m = FaceModel::standard();
        m.dots[0].x = f64::NAN;
        assert!(m.validate().is_err());
        let mut m = FaceModel::standard();
        m.guides.push([0, 999]);
        assert!(m.validate().is_err());
        let mut m = FaceModel::standard();
        m.guides.push([3, 3]);
        assert!(m.validate().is_err());
        let mut m = FaceModel::standard();
        m.dots = vec![FaceDot::default(); MAX_FACE_DOTS + 1];
        assert!(m.validate().is_err());
        let mut m = FaceModel::standard();
        m.name = "x".repeat(MAX_FACE_NAME + 1);
        assert!(m.validate().is_err());
    }
}
