//! The curve editor: draws a feature or shape curve and turns pointer input into edits.
//!
//! Click empty space to add a point, drag a point to move it, right-click a point to remove
//! it. The two end points are pinned at (0, 0) and (1, 1). Nothing here changes the project;
//! the app runs the returned [`CurveAction`]s as `curve.feature` / `curve.shape` commands.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use inbetween_doc::Curve;
use serde::{Deserialize, Serialize};

/// Which of a sequence's two curves the editor shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveKind {
    #[default]
    Feature,
    Shape,
    /// The sequence's opacity over time (`curve.master`).
    Master,
    /// The camera's own clock (`camera.curve`).
    Camera,
}

impl CurveKind {
    pub const ALL: [Self; 4] = [Self::Feature, Self::Shape, Self::Master, Self::Camera];

    /// The engine command that edits this curve.
    pub const fn command(self) -> &'static str {
        match self {
            Self::Feature => "curve.feature",
            Self::Shape => "curve.shape",
            Self::Master => "curve.master",
            Self::Camera => "camera.curve",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Feature => "Feature curve",
            Self::Shape => "Shape curve",
            Self::Master => "Master curve",
            Self::Camera => "Camera curve",
        }
    }

    pub const fn help(self) -> &'static str {
        match self {
            Self::Feature => "Remaps the colour dissolve over time.",
            Self::Shape => "Remaps how far the dots have moved over time.",
            Self::Master => {
                "The whole sequence's opacity over time: drag the ends down to fade in or out."
            }
            Self::Camera => "Eases the camera's clock: how fast it moves through its keyframes.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CurveAction {
    Add { x: f64, y: f64 },
    BeginDrag,
    Move { index: usize, x: f64, y: f64 },
    EndDrag,
    Remove(usize),
    SetLinear(bool),
    Reset,
    ApplyAll,
}

/// Radius, in screen points, within which the pointer grabs a control point.
const GRAB: f32 = 9.0;

/// Curve coordinates (0..1, y up) to a screen position inside `rect`.
pub fn to_screen(rect: Rect, x: f64, y: f64) -> Pos2 {
    Pos2::new(
        rect.min.x + x as f32 * rect.width(),
        rect.max.y - y as f32 * rect.height(),
    )
}

/// Screen position to curve coordinates, clamped to the plot.
pub fn from_screen(rect: Rect, p: Pos2) -> (f64, f64) {
    let unit = |v: f32, size: f32| f64::from((v / size.max(1.0)).clamp(0.0, 1.0));
    (
        unit(p.x - rect.min.x, rect.width()),
        unit(rect.max.y - p.y, rect.height()),
    )
}

/// The control point under `p`, if any.
pub fn hit_point(curve: &Curve, rect: Rect, p: Pos2) -> Option<usize> {
    curve
        .points
        .iter()
        .enumerate()
        .map(|(i, pt)| (i, to_screen(rect, pt.x, pt.y).distance(p)))
        .filter(|&(_, d)| d <= GRAB)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Whether point `index` may be moved or removed (the ends may not).
pub fn is_inner(curve: &Curve, index: usize) -> bool {
    index > 0 && index + 1 < curve.points.len()
}

pub struct CurveView<'a> {
    pub curve: &'a Curve,
    /// Time of every morph frame, for the tick marks along the bottom.
    pub ticks: &'a [f64],
    /// Time of the frame the preview shows.
    pub now: f64,
}

/// Draw the plot and return the edits the pointer asked for. `drag` holds the point being
/// dragged between frames.
pub fn curve_editor(
    ui: &mut Ui,
    view: &CurveView<'_>,
    drag: &mut Option<usize>,
) -> Vec<CurveAction> {
    let mut actions = Vec::new();
    let side = ui.available_width().clamp(160.0, 340.0);
    let (outer, response) = ui.allocate_exact_size(Vec2::splat(side), Sense::click_and_drag());
    let rect = outer.shrink(14.0);
    let painter = ui.painter_at(outer);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 2.0, visuals.extreme_bg_color);

    let faint = Stroke::new(1.0_f32, Color32::from_gray(60));
    for i in 0..=4 {
        let f = f64::from(i) / 4.0;
        painter.line_segment([to_screen(rect, f, 0.0), to_screen(rect, f, 1.0)], faint);
        painter.line_segment([to_screen(rect, 0.0, f), to_screen(rect, 1.0, f)], faint);
    }
    painter.line_segment(
        [to_screen(rect, 0.0, 0.0), to_screen(rect, 1.0, 1.0)],
        Stroke::new(1.0_f32, Color32::from_gray(90)),
    );
    // One tick per frame; a very long sequence just gets a subset.
    let skip = view.ticks.len().div_ceil(120).max(1);
    for &t in view.ticks.iter().step_by(skip) {
        let base = to_screen(rect, t, 0.0);
        painter.line_segment(
            [base, base - Vec2::new(0.0, 5.0)],
            Stroke::new(1.0_f32, Color32::from_gray(150)),
        );
    }

    let line = Stroke::new(2.0_f32, Color32::from_rgb(120, 190, 255));
    let steps = 120;
    let polyline: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let x = f64::from(i) / f64::from(steps);
            to_screen(rect, x, view.curve.eval(x))
        })
        .collect();
    painter.add(egui::Shape::line(polyline, line));

    let now = view.now.clamp(0.0, 1.0);
    painter.line_segment(
        [to_screen(rect, now, 0.0), to_screen(rect, now, 1.0)],
        Stroke::new(1.0_f32, Color32::from_rgb(255, 200, 90)),
    );
    painter.circle_filled(
        to_screen(rect, now, view.curve.eval(now)),
        3.5,
        Color32::from_rgb(255, 200, 90),
    );

    let pointer = response.interact_pointer_pos().or(response.hover_pos());
    let hovered = pointer.and_then(|p| hit_point(view.curve, rect, p));
    for (i, pt) in view.curve.points.iter().enumerate() {
        let active = Some(i) == *drag || (drag.is_none() && Some(i) == hovered);
        let color = if is_inner(view.curve, i) {
            Color32::WHITE
        } else {
            Color32::from_gray(150)
        };
        let at = to_screen(rect, pt.x, pt.y);
        painter.circle_filled(at, if active { 6.0 } else { 4.5 }, color);
        painter.circle_stroke(
            at,
            if active { 6.0 } else { 4.5 },
            Stroke::new(1.0_f32, Color32::BLACK),
        );
    }

    if let Some(p) = response.interact_pointer_pos() {
        let (x, y) = from_screen(rect, p);
        if response.drag_started() {
            if let Some(i) = hit_point(view.curve, rect, p).filter(|&i| is_inner(view.curve, i)) {
                *drag = Some(i);
                actions.push(CurveAction::BeginDrag);
            }
        }
        if let (true, Some(index)) = (response.dragged(), *drag) {
            actions.push(CurveAction::Move { index, x, y });
        }
        if response.secondary_clicked() {
            if let Some(i) = hit_point(view.curve, rect, p).filter(|&i| is_inner(view.curve, i)) {
                actions.push(CurveAction::Remove(i));
            }
        } else if response.clicked() && hit_point(view.curve, rect, p).is_none() {
            actions.push(CurveAction::Add { x, y });
        }
    }
    if response.drag_stopped() && drag.take().is_some() {
        actions.push(CurveAction::EndDrag);
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::CurvePoint;

    fn rect() -> Rect {
        Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(200.0, 100.0))
    }

    fn curve() -> Curve {
        Curve {
            points: vec![
                CurvePoint::new(0.0, 0.0),
                CurvePoint::new(0.5, 0.8),
                CurvePoint::new(1.0, 1.0),
            ],
            linear: false,
        }
    }

    #[test]
    fn screen_mapping_round_trips_and_flips_y() {
        let r = rect();
        assert_eq!(to_screen(r, 0.0, 0.0), Pos2::new(10.0, 120.0));
        assert_eq!(to_screen(r, 1.0, 1.0), Pos2::new(210.0, 20.0));
        let (x, y) = from_screen(r, to_screen(r, 0.25, 0.75));
        assert!((x - 0.25).abs() < 1e-6 && (y - 0.75).abs() < 1e-6);
        // Outside the plot clamps instead of leaving 0..1.
        assert_eq!(from_screen(r, Pos2::new(-500.0, 900.0)), (0.0, 0.0));
        assert_eq!(from_screen(r, Pos2::new(900.0, -500.0)), (1.0, 1.0));
        // A collapsed rectangle never divides by zero.
        let flat = Rect::from_min_size(Pos2::ZERO, Vec2::ZERO);
        let (x, y) = from_screen(flat, Pos2::new(3.0, 3.0));
        assert!(x.is_finite() && y.is_finite());
    }

    #[test]
    fn hit_testing_finds_the_nearest_point_within_reach() {
        let (r, c) = (rect(), curve());
        let on_mid = to_screen(r, 0.5, 0.8);
        assert_eq!(hit_point(&c, r, on_mid + Vec2::new(3.0, -2.0)), Some(1));
        assert_eq!(hit_point(&c, r, on_mid + Vec2::new(40.0, 0.0)), None);
        assert_eq!(hit_point(&c, r, to_screen(r, 0.0, 0.0)), Some(0));
        assert!(is_inner(&c, 1) && !is_inner(&c, 0) && !is_inner(&c, 2) && !is_inner(&c, 9));
    }

    #[test]
    fn kinds_name_their_commands() {
        assert_eq!(CurveKind::Feature.command(), "curve.feature");
        assert_eq!(CurveKind::Shape.command(), "curve.shape");
    }
}
