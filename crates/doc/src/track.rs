//! The track curve: the path every dot travels from its place on Image 1 to its place on
//! Image 2. The default is the straight line; a track bends it.

use crate::DocError;
use serde::{Deserialize, Serialize};

/// Most control points on a track, ends included.
pub const MAX_TRACK_POINTS: usize = 64;
/// Largest tension.
pub const MAX_TENSION: f64 = 20.0;
/// Control points may stray this far from the chord (in chord lengths).
pub const MAX_TRACK_REACH: f64 = 100.0;

/// A control point in *chord space*: `u` runs along the line from a dot's start to its end
/// (0 at the start, 1 at the end), `v` is the sideways distance, both in chord lengths.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackPoint {
    pub u: f64,
    pub v: f64,
}

/// A bent path for the dots, shared by all of them: each dot follows the same curve scaled,
/// turned and moved onto its own start and end.
///
/// The ends are pinned at (0, 0) and (1, 0), so frame 0 and the last frame still match the
/// images. Inner points that stray outside the chord make a dot overshoot (caricature).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Track {
    pub enabled: bool,
    /// 2 to 64 points. The first is (0, 0) and the last (1, 0).
    pub points: Vec<TrackPoint>,
    /// 0 keeps the dots on their straight line, 1 follows the drawn curve, up to 20
    /// exaggerates it.
    pub tension: f64,
    /// Mirror the along-the-chord part of the bend (a path that lags starts ahead instead).
    pub flip_h: bool,
    /// Mirror the sideways part of the bend (swap which side of the line the path bows to).
    pub flip_v: bool,
}

impl Default for Track {
    fn default() -> Self {
        Self {
            enabled: false,
            points: vec![TrackPoint { u: 0.0, v: 0.0 }, TrackPoint { u: 1.0, v: 0.0 }],
            tension: 1.0,
            flip_h: false,
            flip_v: false,
        }
    }
}

impl Track {
    /// Whether the track bends the dots' paths at all.
    pub fn is_active(&self) -> bool {
        let n = self.points.len();
        self.enabled
            && self.tension != 0.0
            && self.points.iter().enumerate().any(|(i, p)| {
                p.v != 0.0 || (n >= 2 && (p.u - i as f64 / (n - 1) as f64).abs() > 1e-12)
            })
    }

    pub fn validate(&self) -> Result<(), DocError> {
        let bad = |why: &'static str| Err(DocError::BadTrack(why));
        if self.points.len() < 2 || self.points.len() > MAX_TRACK_POINTS {
            return bad("a track needs 2 to 64 points");
        }
        if !(self.tension.is_finite() && (0.0..=MAX_TENSION).contains(&self.tension)) {
            return bad("tension must be between 0 and 20");
        }
        if self.points.iter().any(|p| {
            !(p.u.is_finite() && p.v.is_finite())
                || p.u.abs() > MAX_TRACK_REACH
                || p.v.abs() > MAX_TRACK_REACH
        }) {
            return bad("track points must be finite and within 100 chord lengths");
        }
        match (self.points.first(), self.points.last()) {
            (Some(a), Some(b)) if (a.u, a.v) == (0.0, 0.0) && (b.u, b.v) == (1.0, 0.0) => Ok(()),
            _ => bad("a track must start at (0, 0) and end at (1, 0)"),
        }
    }

    /// Where a dot is at progress `s` (0 to 1): `(along, sideways)` in chord lengths. A
    /// straight path gives `(s, 0)`. Always finite; exactly `(0, 0)` at `s = 0` and `(1, 0)`
    /// at `s = 1`.
    pub fn offset(&self, s: f64) -> (f64, f64) {
        let s = if s.is_nan() { 0.0 } else { s.clamp(0.0, 1.0) };
        if !self.is_active() || s <= 0.0 || s >= 1.0 {
            return (s, 0.0);
        }
        let Some(last) = self.points.len().checked_sub(1).filter(|&l| l >= 1) else {
            return (s, 0.0);
        };
        // Uniform Catmull-Rom through the points, parameterised by index.
        let x = s * last as f64;
        let i = (x.floor() as usize).min(last - 1);
        let f = x - i as f64;
        let at = |k: isize| -> TrackPoint {
            let k = k.clamp(0, last as isize) as usize;
            self.points
                .get(k)
                .copied()
                .unwrap_or(TrackPoint { u: 0.0, v: 0.0 })
        };
        let i = i as isize;
        let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
        let cr = |a: f64, b: f64, c: f64, d: f64| {
            0.5 * (2.0 * b
                + (c - a) * f
                + (2.0 * a - 5.0 * b + 4.0 * c - d) * f * f
                + (3.0 * b - a - 3.0 * c + d) * f * f * f)
        };
        let (u, v) = (cr(p0.u, p1.u, p2.u, p3.u), cr(p0.v, p1.v, p2.v, p3.v));
        let k = self.tension;
        // The drawn path minus the straight line is the bend; tension scales the bend.
        let du = (u - s) * k * if self.flip_h { -1.0 } else { 1.0 };
        let dv = v * k * if self.flip_v { -1.0 } else { 1.0 };
        let out = (s + du, dv);
        if out.0.is_finite() && out.1.is_finite() {
            out
        } else {
            (s, 0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bowed() -> Track {
        Track {
            enabled: true,
            points: vec![
                TrackPoint { u: 0.0, v: 0.0 },
                TrackPoint { u: 0.5, v: 0.4 },
                TrackPoint { u: 1.0, v: 0.0 },
            ],
            ..Track::default()
        }
    }

    #[test]
    fn default_is_straight_and_inactive() {
        let t = Track::default();
        assert!(!t.is_active());
        assert!(t.validate().is_ok());
        assert_eq!(t.offset(0.3), (0.3, 0.0));
    }

    #[test]
    fn evenly_spaced_straight_points_stay_inactive() {
        let mut t = bowed();
        t.points[1].v = 0.0;
        assert!(!t.is_active());
    }

    #[test]
    fn ends_are_exact_and_the_middle_bows() {
        let t = bowed();
        assert!(t.is_active());
        assert_eq!(t.offset(0.0), (0.0, 0.0));
        assert_eq!(t.offset(1.0), (1.0, 0.0));
        let (u, v) = t.offset(0.5);
        assert!((u - 0.5).abs() < 1e-9 && (v - 0.4).abs() < 1e-9, "{u} {v}");
    }

    #[test]
    fn tension_scales_the_bend_and_flips_mirror_it() {
        let mut t = bowed();
        t.tension = 0.0;
        assert_eq!(t.offset(0.5), (0.5, 0.0));
        t.tension = 2.0;
        assert!((t.offset(0.5).1 - 0.8).abs() < 1e-9);
        t.flip_v = true;
        assert!((t.offset(0.5).1 + 0.8).abs() < 1e-9);
    }

    #[test]
    fn hostile_tracks_are_refused_and_offsets_stay_finite() {
        let mut t = bowed();
        t.tension = f64::NAN;
        assert!(t.validate().is_err());
        t.tension = 21.0;
        assert!(t.validate().is_err());
        t = bowed();
        t.points[0].u = 0.1;
        assert!(t.validate().is_err());
        t = bowed();
        t.points[1].v = f64::INFINITY;
        assert!(t.validate().is_err());
        t.points.truncate(1);
        assert!(t.validate().is_err());
        assert_eq!(t.offset(0.5), (0.5, 0.0));
        let t = bowed();
        for s in [f64::NAN, -1.0, 2.0, f64::INFINITY] {
            let (u, v) = t.offset(s);
            assert!(u.is_finite() && v.is_finite());
        }
    }
}
