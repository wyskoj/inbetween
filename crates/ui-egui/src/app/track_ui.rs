//! The Track Curve window: draw the path every dot travels as a curve in chord space.
//!
//! The horizontal axis runs along each dot's straight line (0 at its start on Image 1, 1 at its
//! end on Image 2) and the vertical axis is the sideways distance, both in chord lengths. The two
//! ends are pinned (so the first and last frame still match the images); drag an inner point to
//! bend the path, click empty space to add one, right-click a point to remove it. Pulling a
//! point past the end (u above 1) makes dots overshoot: the caricature effect. Everything runs
//! `curve.track`; a drag is one undo step.

use super::*;
use egui::{Color32, Sense, Stroke};
use inbetween_doc::Track;

/// What the window remembers between frames.
#[derive(Default)]
pub(super) struct TrackUi {
    pub drag: Option<usize>,
    pub group_open: bool,
    pub canvas: Option<Rect>,
}

/// The part of chord space the canvas shows: along from -0.5 to 1.5, sideways from -1 to 1.
const U_RANGE: (f64, f64) = (-0.5, 1.5);
const V_RANGE: (f64, f64) = (-1.0, 1.0);
const CANVAS: Vec2 = Vec2::new(360.0, 240.0);
const REACH: f32 = 9.0;

/// Screen position of chord-space point `(u, v)` in `rect`.
pub(crate) fn to_screen(rect: Rect, u: f64, v: f64) -> Pos2 {
    let x = (u - U_RANGE.0) / (U_RANGE.1 - U_RANGE.0);
    let y = (v - V_RANGE.0) / (V_RANGE.1 - V_RANGE.0);
    Pos2::new(
        rect.min.x + rect.width() * x as f32,
        rect.min.y + rect.height() * y as f32,
    )
}

/// Chord-space point under screen position `p` (the inverse of [`to_screen`]).
pub(crate) fn from_screen(rect: Rect, p: Pos2) -> (f64, f64) {
    let x = f64::from((p.x - rect.min.x) / rect.width().max(1.0));
    let y = f64::from((p.y - rect.min.y) / rect.height().max(1.0));
    (
        U_RANGE.0 + x * (U_RANGE.1 - U_RANGE.0),
        V_RANGE.0 + y * (V_RANGE.1 - V_RANGE.0),
    )
}

/// The inner control point near `at`, if any (the pinned ends are not grabbable).
pub(crate) fn inner_point_near(points: &[Pos2], at: Pos2, reach: f32) -> Option<usize> {
    let last = points.len().checked_sub(1)?;
    points
        .iter()
        .enumerate()
        .filter(|(i, _)| *i > 0 && *i < last)
        .map(|(i, p)| (i, p.distance(at)))
        .filter(|(_, d)| *d <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Where a new point at chord-space `(u, v)` goes in the point list: between the two neighbours
/// whose segment is nearest (an index from 1 to `len - 1`).
pub(crate) fn insert_index(points: &[(f64, f64)], u: f64, v: f64) -> usize {
    let mut best = (f64::MAX, 1usize);
    for (i, w) in points.windows(2).enumerate() {
        let ((ax, ay), (bx, by)) = (w[0], w[1]);
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 {
            (((u - ax) * dx + (v - ay) * dy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let d = (ax + t * dx - u).hypot(ay + t * dy - v);
        if d < best.0 {
            best = (d, i + 1);
        }
    }
    best.1.clamp(1, points.len().saturating_sub(1).max(1))
}

impl InbetweenApp {
    /// Where the Track Curve window drew its canvas last frame (for tests).
    pub fn track_canvas(&self) -> Option<Rect> {
        self.track_ui.canvas
    }

    pub fn open_track(&mut self) {
        if self.state.track_window {
            return;
        }
        self.state.track_window = true;
        self.track_ui.group_open = self.run_command("edit.beginGroup", &json!({})).is_ok();
    }

    pub fn close_track(&mut self) {
        self.track_ui.drag = None;
        if self.track_ui.group_open {
            let _ = self.run_command("edit.endGroup", &json!({}));
            self.track_ui.group_open = false;
        }
        self.state.track_window = false;
    }

    fn track_now(&self) -> Option<Track> {
        self.session
            .project()
            .sequences
            .get(self.state.sequence)
            .map(|s| s.track.clone())
    }

    pub(super) fn track_window(&mut self, ctx: &Context) {
        if !self.state.track_window {
            return;
        }
        let Some(track) = self.track_now() else {
            self.close_track();
            return;
        };
        let mut open = true;
        let mut ops: Vec<Value> = Vec::new();
        let (mut tension, mut flip_h, mut flip_v, mut enabled) =
            (track.tension, track.flip_h, track.flip_v, track.enabled);
        egui::Window::new("Track Curve")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos([280.0, 90.0])
            .show(ctx, |ui| {
                ui.checkbox(&mut enabled, "Dots follow this curve");
                let (rect, resp) = ui.allocate_exact_size(CANVAS, Sense::click_and_drag());
                self.track_ui.canvas = Some(rect);
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 2.0, Color32::from_gray(28));
                // The chord (the straight path) and the unit marks at its ends.
                let chord = Stroke::new(1.0_f32, Color32::from_gray(90));
                painter.line_segment([to_screen(rect, U_RANGE.0, 0.0), to_screen(rect, U_RANGE.1, 0.0)], chord);
                for u in [0.0, 1.0] {
                    painter.line_segment(
                        [to_screen(rect, u, -0.08), to_screen(rect, u, 0.08)],
                        Stroke::new(1.5_f32, Color32::from_gray(150)),
                    );
                }
                painter.text(
                    to_screen(rect, 0.0, 0.14),
                    egui::Align2::CENTER_TOP,
                    "Image 1",
                    egui::FontId::proportional(11.0),
                    Color32::from_gray(150),
                );
                painter.text(
                    to_screen(rect, 1.0, 0.14),
                    egui::Align2::CENTER_TOP,
                    "Image 2",
                    egui::FontId::proportional(11.0),
                    Color32::from_gray(150),
                );
                // The path a dot takes, with tension and flips applied.
                let path: Vec<Pos2> = (0..=64)
                    .map(|i| {
                        let s = f64::from(i) / 64.0;
                        let (u, v) = track.offset(s);
                        to_screen(rect, u, v)
                    })
                    .collect();
                let colour = if track.enabled {
                    Color32::from_rgb(0, 200, 255)
                } else {
                    Color32::from_gray(110)
                };
                painter.add(egui::Shape::line(path, Stroke::new(2.0_f32, colour)));
                let screen_points: Vec<Pos2> = track
                    .points
                    .iter()
                    .map(|p| to_screen(rect, p.u, p.v))
                    .collect();
                let last = screen_points.len().saturating_sub(1);
                for (i, p) in screen_points.iter().enumerate() {
                    if i == 0 || i == last {
                        painter.circle_stroke(*p, 5.0, Stroke::new(1.5_f32, Color32::from_gray(200)));
                    } else {
                        painter.circle_filled(*p, 5.0, Color32::from_rgb(255, 170, 40));
                    }
                }

                // Pick on press so a drag starts on what was under the pointer.
                if resp.drag_started() {
                    let pressed = ui
                        .input(|i| i.pointer.press_origin())
                        .or_else(|| resp.interact_pointer_pos());
                    if let Some(i) = pressed.and_then(|p| inner_point_near(&screen_points, p, REACH)) {
                        self.track_ui.drag = Some(i);
                    }
                }
                if let (Some(i), true, Some(p)) =
                    (self.track_ui.drag, resp.dragged(), resp.interact_pointer_pos())
                {
                    let (u, v) = from_screen(rect, p);
                    ops.push(json!({ "op": "move", "index": i, "u": u, "v": v }));
                }
                if resp.drag_stopped() {
                    self.track_ui.drag = None;
                }
                if resp.clicked() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        if inner_point_near(&screen_points, p, REACH).is_none() {
                            let (u, v) = from_screen(rect, p);
                            let pts: Vec<(f64, f64)> =
                                track.points.iter().map(|q| (q.u, q.v)).collect();
                            let at = insert_index(&pts, u, v);
                            ops.push(json!({ "op": "add", "index": at, "u": u, "v": v }));
                        }
                    }
                }
                if resp.secondary_clicked() {
                    if let Some(i) = resp
                        .interact_pointer_pos()
                        .and_then(|p| inner_point_near(&screen_points, p, REACH))
                    {
                        ops.push(json!({ "op": "remove", "index": i }));
                    }
                }
                ui.weak("Drag a point to bend the path; click to add one; right-click to remove. Pull a point past the end (the right mark) to overshoot.");
                ui.horizontal(|ui| {
                    ui.label("Tension");
                    ui.add(egui::Slider::new(&mut tension, 0.0..=20.0));
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut flip_h, "Flip horizontally");
                    ui.checkbox(&mut flip_v, "Flip vertically");
                });
                ui.horizontal(|ui| {
                    for name in ["arc", "wave", "overshoot"] {
                        if ui.button(name).clicked() {
                            ops.push(json!({ "op": "preset", "name": name }));
                        }
                    }
                    if ui.button("Reset").clicked() {
                        ops.push(json!({ "op": "reset" }));
                    }
                    if ui.button("Apply to all sequences").clicked() {
                        ops.push(json!({ "op": "apply_all" }));
                    }
                });
            });

        for op in ops {
            let _ = self.run_command("curve.track", &op);
        }
        let mut changes = serde_json::Map::new();
        if tension != track.tension {
            changes.insert("tension".into(), json!(tension));
        }
        if flip_h != track.flip_h {
            changes.insert("flip_h".into(), json!(flip_h));
        }
        if flip_v != track.flip_v {
            changes.insert("flip_v".into(), json!(flip_v));
        }
        if enabled != track.enabled {
            changes.insert("enabled".into(), json!(enabled));
        }
        if !changes.is_empty() {
            let _ = self.run_command("curve.track", &Value::Object(changes));
        }
        if !open {
            self.close_track();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_and_chord_space_round_trip() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), CANVAS);
        for (u, v) in [(0.0, 0.0), (1.0, 0.0), (0.37, -0.6), (1.4, 0.9)] {
            let (a, b) = from_screen(rect, to_screen(rect, u, v));
            assert!((a - u).abs() < 1e-5 && (b - v).abs() < 1e-5, "{u} {v}");
        }
        // The chord's two ends sit where the marks are drawn.
        let start = to_screen(rect, 0.0, 0.0);
        let end = to_screen(rect, 1.0, 0.0);
        assert!(end.x > start.x && (end.y - start.y).abs() < 1e-4);
    }

    #[test]
    fn only_inner_points_can_be_grabbed() {
        let pts = [
            Pos2::new(0.0, 0.0),
            Pos2::new(50.0, 20.0),
            Pos2::new(100.0, 0.0),
        ];
        assert_eq!(inner_point_near(&pts, Pos2::new(52.0, 22.0), 9.0), Some(1));
        assert_eq!(
            inner_point_near(&pts, Pos2::new(1.0, 1.0), 9.0),
            None,
            "the start is pinned"
        );
        assert_eq!(
            inner_point_near(&pts, Pos2::new(99.0, 1.0), 9.0),
            None,
            "the end is pinned"
        );
        assert_eq!(inner_point_near(&[], Pos2::ZERO, 9.0), None);
        assert_eq!(inner_point_near(&pts[..1], Pos2::ZERO, 9.0), None);
    }

    #[test]
    fn a_new_point_goes_between_the_nearest_neighbours() {
        let pts = [(0.0, 0.0), (0.3, 0.2), (0.7, -0.2), (1.0, 0.0)];
        assert_eq!(insert_index(&pts, 0.15, 0.1), 1);
        assert_eq!(insert_index(&pts, 0.5, 0.0), 2);
        assert_eq!(insert_index(&pts, 0.9, -0.05), 3);
        // Degenerate lists still give a valid index.
        assert_eq!(insert_index(&[(0.0, 0.0), (1.0, 0.0)], 0.5, 0.3), 1);
        assert_eq!(insert_index(&[], 0.5, 0.3), 1);
        assert_eq!(insert_index(&[(0.0, 0.0)], 0.5, 0.3), 1);
    }
}
