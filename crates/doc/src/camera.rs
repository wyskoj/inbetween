//! A per-sequence camera: keyframes of offset, scale and rotation over morph time.
//!
//! The camera moves the rendered morph (before the effects), so a still image can pan, zoom
//! and spin. The area the morph no longer covers is transparent.

use crate::{Curve, DocError};
use serde::{Deserialize, Serialize};

/// Most keyframes one camera may hold.
pub const MAX_CAMERA_KEYS: usize = 64;
/// Smallest and largest absolute scale a keyframe may have (the sign flips the image).
pub const MIN_SCALE: f64 = 0.01;
pub const MAX_SCALE: f64 = 100.0;
/// Largest offset, in frame widths or heights.
pub const MAX_OFFSET: f64 = 100.0;

/// One camera pose, at morph time `t` (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraKey {
    pub t: f64,
    /// Offset of the picture in frame widths (0.1 moves it right by a tenth of the frame).
    pub x: f64,
    /// Offset in frame heights (positive moves it down).
    pub y: f64,
    /// Horizontal scale about the frame centre; negative flips left to right.
    pub scale_x: f64,
    /// Vertical scale about the frame centre; negative flips top to bottom.
    pub scale_y: f64,
    /// Rotation in degrees about the frame centre, clockwise on screen.
    pub rotation: f64,
}

impl Default for CameraKey {
    fn default() -> Self {
        Self::identity(0.0)
    }
}

impl CameraKey {
    /// The pose that leaves the picture where it is.
    pub const fn identity(t: f64) -> Self {
        Self {
            t,
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
        }
    }

    /// Whether this pose leaves the picture exactly where it is.
    pub fn is_identity(&self) -> bool {
        (self.x, self.y, self.scale_x, self.scale_y, self.rotation) == (0.0, 0.0, 1.0, 1.0, 0.0)
    }

    fn mix(a: &Self, b: &Self, u: f64) -> Self {
        let l = |p: f64, q: f64| p + (q - p) * u;
        Self {
            t: l(a.t, b.t),
            x: l(a.x, b.x),
            y: l(a.y, b.y),
            scale_x: l(a.scale_x, b.scale_x),
            scale_y: l(a.scale_y, b.scale_y),
            rotation: l(a.rotation, b.rotation),
        }
    }

    fn finite(&self) -> bool {
        [
            self.t,
            self.x,
            self.y,
            self.scale_x,
            self.scale_y,
            self.rotation,
        ]
        .iter()
        .all(|v| v.is_finite())
    }
}

/// Whether the two scales of a keyframe stay equal when edited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraAspect {
    /// Width and height scale independently.
    #[default]
    Free,
    /// Width and height always scale together (the keyframe's `scale_y` follows `scale_x`).
    Locked,
}

impl CameraAspect {
    pub const ALL: [Self; 2] = [Self::Free, Self::Locked];

    pub fn id(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Locked => "locked",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }
}

/// The camera of one sequence.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Camera {
    pub enabled: bool,
    /// Keyframes sorted by strictly rising `t`. Before the first and after the last the pose
    /// holds; between two, it eases smoothly.
    pub keys: Vec<CameraKey>,
    /// Optional easing of the camera's own clock (morph time in, camera time out).
    pub easing: Curve,
    pub aspect: CameraAspect,
}

impl Camera {
    /// Whether the camera can move the picture at all.
    pub fn is_active(&self) -> bool {
        self.enabled && self.keys.iter().any(|k| !k.is_identity())
    }

    /// The pose at morph time `t` (NaN counts as 0). Identity with no keys.
    pub fn pose(&self, t: f64) -> CameraKey {
        let t = self.easing.eval(t);
        let (Some(first), Some(last)) = (self.keys.first(), self.keys.last()) else {
            return CameraKey::identity(t);
        };
        if t <= first.t {
            return CameraKey { t, ..*first };
        }
        if t >= last.t {
            return CameraKey { t, ..*last };
        }
        for w in self.keys.windows(2) {
            if let [a, b] = w {
                if t <= b.t {
                    let span = b.t - a.t;
                    let u = if span > 0.0 { (t - a.t) / span } else { 1.0 };
                    let u = u.clamp(0.0, 1.0);
                    // Smoothstep: eases in and out of every keyframe.
                    return CameraKey {
                        t,
                        ..CameraKey::mix(a, b, u * u * (3.0 - 2.0 * u))
                    };
                }
            }
        }
        CameraKey::identity(t)
    }

    /// Add a keyframe, or replace the one at the same time. Returns its index.
    pub fn set_key(&mut self, key: CameraKey) -> Result<usize, DocError> {
        check_key(&key)?;
        let at = self.keys.partition_point(|k| k.t < key.t);
        if self.keys.get(at).is_some_and(|k| k.t == key.t) {
            if let Some(slot) = self.keys.get_mut(at) {
                *slot = key;
            }
            return Ok(at);
        }
        if self.keys.len() >= MAX_CAMERA_KEYS {
            return Err(DocError::BadCamera(
                "a camera may have at most 64 keyframes",
            ));
        }
        self.keys.insert(at, key);
        Ok(at)
    }

    pub fn remove_key(&mut self, index: usize) -> Result<(), DocError> {
        if index >= self.keys.len() {
            return Err(DocError::BadCamera("no such keyframe"));
        }
        self.keys.remove(index);
        Ok(())
    }

    pub fn validate(&self) -> Result<(), DocError> {
        if self.keys.len() > MAX_CAMERA_KEYS {
            return Err(DocError::BadCamera(
                "a camera may have at most 64 keyframes",
            ));
        }
        self.keys.iter().try_for_each(check_key)?;
        if self.keys.windows(2).any(|w| w[0].t >= w[1].t) {
            return Err(DocError::BadCamera(
                "keyframes must have strictly increasing times",
            ));
        }
        self.easing.validate()
    }
}

fn check_key(k: &CameraKey) -> Result<(), DocError> {
    if !k.finite() {
        return Err(DocError::BadCamera(
            "keyframe values must be finite numbers",
        ));
    }
    if !(0.0..=1.0).contains(&k.t) {
        return Err(DocError::BadCamera("keyframe time must be between 0 and 1"));
    }
    if k.x.abs() > MAX_OFFSET || k.y.abs() > MAX_OFFSET {
        return Err(DocError::BadCamera("keyframe offset is too large"));
    }
    for s in [k.scale_x, k.scale_y] {
        if !(MIN_SCALE..=MAX_SCALE).contains(&s.abs()) {
            return Err(DocError::BadCamera(
                "keyframe scale must be between 0.01 and 100 (negative flips)",
            ));
        }
    }
    if k.rotation.abs() > 360_000.0 {
        return Err(DocError::BadCamera("keyframe rotation is too large"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(t: f64, x: f64) -> CameraKey {
        CameraKey {
            x,
            ..CameraKey::identity(t)
        }
    }

    #[test]
    fn no_keys_is_identity_and_inactive() {
        let c = Camera {
            enabled: true,
            ..Camera::default()
        };
        assert!(!c.is_active());
        assert!(c.pose(0.5).is_identity());
    }

    #[test]
    fn poses_hold_outside_and_ease_between() {
        let mut c = Camera {
            enabled: true,
            ..Camera::default()
        };
        c.set_key(key(0.25, 0.0)).unwrap();
        c.set_key(key(0.75, 1.0)).unwrap();
        assert!(c.is_active());
        assert_eq!(c.pose(0.0).x, 0.0);
        assert_eq!(c.pose(1.0).x, 1.0);
        assert_eq!(c.pose(0.25).x, 0.0);
        assert_eq!(c.pose(0.5).x, 0.5);
        assert!(c.pose(0.35).x < 0.2, "eases in");
        assert_eq!(c.pose(f64::NAN).x, 0.0);
    }

    #[test]
    fn keys_stay_sorted_and_replace_at_the_same_time() {
        let mut c = Camera::default();
        c.set_key(key(0.5, 1.0)).unwrap();
        c.set_key(key(0.1, 2.0)).unwrap();
        assert_eq!(c.set_key(key(0.5, 3.0)).unwrap(), 1);
        assert_eq!(c.keys.len(), 2);
        assert_eq!(c.keys[1].x, 3.0);
        assert!(c.validate().is_ok());
        c.remove_key(0).unwrap();
        assert!(c.remove_key(5).is_err());
    }

    #[test]
    fn hostile_keys_are_refused() {
        let mut c = Camera::default();
        let base = CameraKey::default();
        for bad in [
            CameraKey {
                x: f64::NAN,
                ..base
            },
            CameraKey { t: 1.5, ..base },
            CameraKey { t: -0.1, ..base },
            CameraKey {
                scale_x: 0.0,
                ..base
            },
            CameraKey {
                scale_y: 1e9,
                ..base
            },
            CameraKey { y: 1e9, ..base },
            CameraKey {
                rotation: f64::INFINITY,
                ..base
            },
        ] {
            assert!(c.set_key(bad).is_err(), "{bad:?}");
        }
        c.keys = vec![key(0.5, 0.0), key(0.5, 1.0)];
        assert!(c.validate().is_err());
        c.keys = (0..65).map(|i| key(f64::from(i) / 100.0, 0.0)).collect();
        assert!(c.validate().is_err());
        assert!(c.set_key(key(0.99, 0.0)).is_err());
    }

    #[test]
    fn flips_are_valid() {
        let mut c = Camera::default();
        let flipped = CameraKey {
            scale_x: -2.0,
            ..CameraKey::default()
        };
        assert!(c.set_key(flipped).is_ok());
    }
}
