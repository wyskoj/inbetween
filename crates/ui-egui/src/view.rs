//! Zoom and pan: the map between frame coordinates (output pixels) and screen points.

use egui::{Pos2, Rect, Vec2};
use serde::{Deserialize, Serialize};

pub const MIN_ZOOM: f32 = 0.02;
pub const MAX_ZOOM: f32 = 16.0;
/// The zoom levels the zoom keys and menu step through (12.5% to 800%); the wheel goes on to
/// [`MAX_ZOOM`] (1600%).
pub const ZOOM_PRESETS: [f32; 7] = [0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0];
/// Space left around a fitted frame, in screen points.
const FIT_MARGIN: f32 = 10.0;

/// One panel's view of the frame. `offset` is where frame (0, 0) sits relative to the panel's
/// top-left corner, in screen points.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct View {
    pub zoom: f32,
    pub offset: [f32; 2],
    /// While true the frame is re-fitted whenever the panel resizes. Any manual zoom or pan
    /// turns it off; "Fit" turns it back on.
    pub auto_fit: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset: [0.0, 0.0],
            auto_fit: true,
        }
    }
}

fn finite(v: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        fallback
    }
}

impl View {
    fn offset_vec(&self) -> Vec2 {
        Vec2::new(finite(self.offset[0], 0.0), finite(self.offset[1], 0.0))
    }

    pub fn zoom(&self) -> f32 {
        finite(self.zoom, 1.0).clamp(MIN_ZOOM, MAX_ZOOM)
    }

    /// Frame point to screen point.
    pub fn to_screen(&self, panel: Rect, p: Pos2) -> Pos2 {
        panel.min + self.offset_vec() + p.to_vec2() * self.zoom()
    }

    /// Screen point to frame point.
    pub fn to_frame(&self, panel: Rect, s: Pos2) -> Pos2 {
        ((s - panel.min - self.offset_vec()) / self.zoom()).to_pos2()
    }

    /// The screen rectangle covered by a `frame`-sized image.
    pub fn frame_rect(&self, panel: Rect, frame: Vec2) -> Rect {
        Rect::from_min_max(
            self.to_screen(panel, Pos2::ZERO),
            self.to_screen(panel, frame.to_pos2()),
        )
    }

    /// Centre the whole frame in `panel` at the largest zoom that fits.
    pub fn fit(frame: Vec2, panel: Rect) -> Self {
        let avail = (panel.size() - Vec2::splat(2.0 * FIT_MARGIN)).max(Vec2::splat(1.0));
        let zoom = if frame.x > 0.0 && frame.y > 0.0 {
            (avail.x / frame.x).min(avail.y / frame.y)
        } else {
            1.0
        };
        let zoom = finite(zoom, 1.0).clamp(MIN_ZOOM, MAX_ZOOM);
        let offset = (panel.size() - frame * zoom) / 2.0;
        Self {
            zoom,
            offset: [offset.x, offset.y],
            auto_fit: true,
        }
    }

    /// Show the frame at 100%, centred.
    pub fn actual_size(frame: Vec2, panel: Rect) -> Self {
        let offset = (panel.size() - frame) / 2.0;
        Self {
            zoom: 1.0,
            offset: [offset.x, offset.y],
            auto_fit: false,
        }
    }

    /// Multiply the zoom by `factor`, keeping the frame point under `cursor` where it is.
    pub fn zoom_at(&mut self, panel: Rect, cursor: Pos2, factor: f32) {
        let old = self.zoom();
        let new = (old * finite(factor, 1.0)).clamp(MIN_ZOOM, MAX_ZOOM);
        let anchor = cursor - panel.min;
        let offset = anchor - (anchor - self.offset_vec()) * (new / old);
        self.zoom = new;
        self.offset = [offset.x, offset.y];
        self.auto_fit = false;
    }

    /// Fit the frame's width to the panel; the top of the frame stays in view when it is taller
    /// than the panel.
    pub fn fit_width(frame: Vec2, panel: Rect) -> Self {
        let avail = (panel.size().x - 2.0 * FIT_MARGIN).max(1.0);
        let zoom = if frame.x > 0.0 { avail / frame.x } else { 1.0 };
        let zoom = finite(zoom, 1.0).clamp(MIN_ZOOM, MAX_ZOOM);
        let x = (panel.size().x - frame.x * zoom) / 2.0;
        let slack = panel.size().y - frame.y * zoom;
        let y = if slack >= 0.0 {
            slack / 2.0
        } else {
            FIT_MARGIN
        };
        Self {
            zoom,
            offset: [x, y],
            auto_fit: false,
        }
    }

    /// Fit the frame's height to the panel; the left of the frame stays in view when it is wider
    /// than the panel.
    pub fn fit_height(frame: Vec2, panel: Rect) -> Self {
        let avail = (panel.size().y - 2.0 * FIT_MARGIN).max(1.0);
        let zoom = if frame.y > 0.0 { avail / frame.y } else { 1.0 };
        let zoom = finite(zoom, 1.0).clamp(MIN_ZOOM, MAX_ZOOM);
        let y = (panel.size().y - frame.y * zoom) / 2.0;
        let slack = panel.size().x - frame.x * zoom;
        let x = if slack >= 0.0 {
            slack / 2.0
        } else {
            FIT_MARGIN
        };
        Self {
            zoom,
            offset: [x, y],
            auto_fit: false,
        }
    }

    /// Move to the next zoom preset above (`zoom_in`) or below the current zoom, keeping the
    /// middle of the panel where it is.
    pub fn step_preset(&mut self, panel: Rect, zoom_in: bool) {
        let now = self.zoom();
        let target = if zoom_in {
            ZOOM_PRESETS.iter().copied().find(|z| *z > now * 1.001)
        } else {
            ZOOM_PRESETS
                .iter()
                .rev()
                .copied()
                .find(|z| *z < now / 1.001)
        };
        if let Some(target) = target {
            self.zoom_at(panel, panel.center(), target / now);
            // Land exactly on the preset, not a rounding error away from it.
            self.zoom = target;
        }
    }

    pub fn pan(&mut self, delta: Vec2) {
        let o = self.offset_vec() + delta;
        self.offset = [o.x, o.y];
        self.auto_fit = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> Rect {
        Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(400.0, 300.0))
    }

    fn close(a: Pos2, b: Pos2) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn screen_and_frame_round_trip() {
        let v = View {
            zoom: 2.5,
            offset: [13.0, -7.0],
            auto_fit: false,
        };
        for p in [
            Pos2::new(0.0, 0.0),
            Pos2::new(123.4, 56.7),
            Pos2::new(-30.0, 900.0),
        ] {
            assert!(close(v.to_frame(panel(), v.to_screen(panel(), p)), p));
        }
    }

    #[test]
    fn fit_centres_and_contains_the_frame() {
        let frame = Vec2::new(480.0, 360.0);
        let v = View::fit(frame, panel());
        let r = v.frame_rect(panel(), frame);
        assert!(panel().contains_rect(r), "{r:?} not inside {:?}", panel());
        assert!((r.center() - panel().center()).length() < 1e-3);
        assert!(v.auto_fit);
        // The limiting side touches the margin.
        let width_limited = ((r.width() + 2.0 * FIT_MARGIN) - panel().width()).abs() < 1e-3;
        let height_limited = ((r.height() + 2.0 * FIT_MARGIN) - panel().height()).abs() < 1e-3;
        assert!(width_limited || height_limited);
    }

    #[test]
    fn fit_handles_degenerate_sizes() {
        let tiny = Rect::from_min_size(Pos2::ZERO, Vec2::new(5.0, 5.0));
        for frame in [
            Vec2::ZERO,
            Vec2::new(0.0, 10.0),
            Vec2::new(1e9, 1e9),
            Vec2::new(f32::NAN, 3.0),
        ] {
            let v = View::fit(frame, tiny);
            assert!(v.zoom().is_finite() && (MIN_ZOOM..=MAX_ZOOM).contains(&v.zoom()));
            assert!(v.offset.iter().all(|o| o.is_finite() || o.is_nan()));
        }
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor_fixed() {
        let mut v = View::fit(Vec2::new(480.0, 360.0), panel());
        let cursor = Pos2::new(220.0, 140.0);
        let before = v.to_frame(panel(), cursor);
        v.zoom_at(panel(), cursor, 1.7);
        assert!(close(v.to_frame(panel(), cursor), before));
        v.zoom_at(panel(), cursor, 0.3);
        assert!(close(v.to_frame(panel(), cursor), before));
        assert!(!v.auto_fit);
    }

    #[test]
    fn zoom_is_clamped_and_survives_nonsense() {
        let mut v = View::default();
        for _ in 0..200 {
            v.zoom_at(panel(), Pos2::new(200.0, 100.0), 2.0);
        }
        assert_eq!(v.zoom(), MAX_ZOOM);
        for _ in 0..400 {
            v.zoom_at(panel(), Pos2::new(200.0, 100.0), 0.5);
        }
        assert_eq!(v.zoom(), MIN_ZOOM);
        v.zoom_at(panel(), Pos2::new(200.0, 100.0), f32::NAN);
        v.zoom_at(panel(), Pos2::new(200.0, 100.0), f32::INFINITY);
        assert!(v.zoom().is_finite());
        let broken = View {
            zoom: f32::NAN,
            offset: [f32::NAN, f32::INFINITY],
            auto_fit: false,
        };
        assert!(broken.to_screen(panel(), Pos2::new(1.0, 1.0)).x.is_finite());
    }

    #[test]
    fn pan_moves_by_the_drag_and_stops_auto_fit() {
        let mut v = View::fit(Vec2::new(100.0, 100.0), panel());
        let before = v.to_screen(panel(), Pos2::new(10.0, 10.0));
        v.pan(Vec2::new(30.0, -12.0));
        assert!(close(
            v.to_screen(panel(), Pos2::new(10.0, 10.0)),
            before + Vec2::new(30.0, -12.0)
        ));
        assert!(!v.auto_fit);
    }

    #[test]
    fn actual_size_is_one_to_one_and_centred() {
        let v = View::actual_size(Vec2::new(100.0, 80.0), panel());
        assert_eq!(v.zoom(), 1.0);
        let r = v.frame_rect(panel(), Vec2::new(100.0, 80.0));
        assert!((r.center() - panel().center()).length() < 1e-3);
        assert_eq!(r.size(), Vec2::new(100.0, 80.0));
    }

    #[test]
    fn fit_width_and_height_fill_one_axis() {
        let frame = Vec2::new(200.0, 100.0);
        let w = View::fit_width(frame, panel());
        assert!((w.frame_rect(panel(), frame).width() - 380.0).abs() < 1e-3);
        assert!(!w.auto_fit);
        let h = View::fit_height(frame, panel());
        assert!((h.frame_rect(panel(), frame).height() - 280.0).abs() < 1e-3);
        // A frame taller than the panel keeps its top in view under fit-width.
        let tall = View::fit_width(Vec2::new(100.0, 1000.0), panel());
        assert!((tall.offset[1] - FIT_MARGIN).abs() < 1e-3);
        // Degenerate frames still give a finite view.
        assert!(View::fit_width(Vec2::ZERO, panel()).zoom().is_finite());
        assert!(View::fit_height(Vec2::ZERO, panel()).zoom().is_finite());
    }

    #[test]
    fn presets_step_through_the_ladder_and_stop_at_the_ends() {
        let mut v = View::actual_size(Vec2::new(100.0, 80.0), panel());
        v.step_preset(panel(), true);
        assert_eq!(v.zoom(), 2.0);
        for _ in 0..10 {
            v.step_preset(panel(), true);
        }
        assert_eq!(v.zoom(), 8.0);
        for _ in 0..10 {
            v.step_preset(panel(), false);
        }
        assert_eq!(v.zoom(), 0.125);
        // From between presets it goes to the next one in that direction.
        v.zoom = 1.3;
        v.step_preset(panel(), true);
        assert_eq!(v.zoom(), 2.0);
        v.zoom = 1.3;
        v.step_preset(panel(), false);
        assert_eq!(v.zoom(), 1.0);
        // The wheel limit is 1600%.
        let mut w = View::default();
        w.zoom_at(panel(), panel().center(), 1000.0);
        assert_eq!(w.zoom(), MAX_ZOOM);
    }
}
