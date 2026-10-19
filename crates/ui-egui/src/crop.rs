//! The crop tool's geometry, kept apart from egui so it can be tested on its own.
//!
//! The tool edits a box over the turned and flipped picture, in that picture's pixels. The box
//! may be tilted: a box tilted clockwise by some angle crops content that the engine turns
//! counter-clockwise by the same angle, so the engine's `angle` is the negative of the tilt.
//! Dragging works in the box's own frame, so a tilted box resizes along its own edges and the
//! side opposite the handle stays where it is.

use inbetween_doc::ImageEdit;
use serde::{Deserialize, Serialize};

/// The crop being drawn on one source panel. View state only: nothing reaches the project
/// until Apply runs `image.crop`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CropTool {
    /// 1 or 2.
    pub image: u8,
    /// The box's top-left corner and size before tilting, in pixels of the turned picture.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// The engine's angle: content rotation in degrees, clockwise. The box tilts the opposite way.
    pub angle: f64,
    /// `free`, `original`, `output`, or a ratio such as `4:3`.
    pub aspect: String,
}

impl Default for CropTool {
    fn default() -> Self {
        Self {
            image: 1,
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            angle: 0.0,
            aspect: "free".to_string(),
        }
    }
}

/// What a drag grabbed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// The inside of the box: move it.
    Move,
    /// The round handle above the top edge: tilt it.
    Rotate,
    /// A corner or an edge. `(-1, -1)` is the top-left corner, `(1, 0)` the right edge.
    Edge(i8, i8),
}

/// Handles in the order they are hit-tested: the rotate handle and corners first.
pub const EDGE_HANDLES: [(i8, i8); 8] = [
    (-1, -1),
    (1, -1),
    (1, 1),
    (-1, 1),
    (0, -1),
    (1, 0),
    (0, 1),
    (-1, 0),
];

fn rot(v: (f64, f64), by: f64) -> (f64, f64) {
    let (s, c) = by.sin_cos();
    (v.0 * c - v.1 * s, v.0 * s + v.1 * c)
}

impl CropTool {
    /// A starting box for `image`: the edit's current crop, or the whole turned picture.
    pub fn from_edit(image: u8, edit: &ImageEdit, turned: (f64, f64)) -> Self {
        let c = edit.crop.unwrap_or(inbetween_doc::Crop {
            x: 0.0,
            y: 0.0,
            w: turned.0,
            h: turned.1,
        });
        Self {
            image,
            x: c.x,
            y: c.y,
            w: c.w,
            h: c.h,
            angle: edit.angle,
            aspect: "free".to_string(),
        }
    }

    pub fn centre(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// How far the box is tilted clockwise on screen, in radians.
    pub fn tilt(&self) -> f64 {
        -self.angle.to_radians()
    }

    /// Whether every number is finite and the box has some size.
    pub fn is_sane(&self) -> bool {
        [self.x, self.y, self.w, self.h, self.angle]
            .iter()
            .all(|v| v.is_finite())
            && self.w >= 1.0
            && self.h >= 1.0
    }

    /// The box corners (top-left, top-right, bottom-right, bottom-left) after tilting.
    pub fn corners(&self) -> [(f64, f64); 4] {
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(sx, sy)| self.edge_point(sx, sy))
    }

    /// The point of a handle: `(sx, sy)` as in [`Handle::Edge`].
    pub fn edge_point(&self, sx: f64, sy: f64) -> (f64, f64) {
        let (cx, cy) = self.centre();
        let (dx, dy) = rot((sx * self.w / 2.0, sy * self.h / 2.0), self.tilt());
        (cx + dx, cy + dy)
    }

    /// The rotate handle: the middle of the top edge pushed `offset` pixels outward.
    pub fn rotate_point(&self, offset: f64) -> (f64, f64) {
        let (cx, cy) = self.centre();
        let (dx, dy) = rot((0.0, -self.h / 2.0 - offset), self.tilt());
        (cx + dx, cy + dy)
    }

    /// Whether `p` (turned-picture pixels) is inside the tilted box.
    pub fn contains(&self, p: (f64, f64)) -> bool {
        let (cx, cy) = self.centre();
        let (lx, ly) = rot((p.0 - cx, p.1 - cy), -self.tilt());
        lx.abs() <= self.w / 2.0 && ly.abs() <= self.h / 2.0
    }

    /// Change the shape to `ratio` (width over height) by adjusting the height.
    pub fn snap_aspect(&mut self, ratio: f64) {
        if ratio.is_finite() && ratio > 0.0 && self.w.is_finite() {
            let (cx, cy) = self.centre();
            self.h = (self.w / ratio).max(1.0);
            self.x = cx - self.w / 2.0;
            self.y = cy - self.h / 2.0;
        }
    }

    /// The parameters `image.crop` takes to apply this box.
    pub fn params(&self) -> serde_json::Value {
        serde_json::json!({
            "x": self.x, "y": self.y, "w": self.w, "h": self.h, "angle": self.angle,
        })
    }
}

/// The ratio an `aspect` setting asks for, given the turned picture and output sizes.
pub fn ratio_of(aspect: &str, turned: (f64, f64), output: (f64, f64)) -> Option<f64> {
    let ratio = match aspect {
        "free" | "" => return None,
        "original" => turned.0 / turned.1,
        "output" => output.0 / output.1,
        other => {
            let (a, b) = other.split_once(':')?;
            a.trim().parse::<f64>().ok()? / b.trim().parse::<f64>().ok()?
        }
    };
    (ratio.is_finite() && ratio > 0.0).then_some(ratio)
}

/// Which handle (if any) is within `reach` pixels of `p`, or `Move` when inside the box.
pub fn pick(tool: &CropTool, p: (f64, f64), reach: f64, rotate_offset: f64) -> Option<Handle> {
    let near = |q: (f64, f64)| (p.0 - q.0).hypot(p.1 - q.1) <= reach;
    if near(tool.rotate_point(rotate_offset)) {
        return Some(Handle::Rotate);
    }
    for (sx, sy) in EDGE_HANDLES {
        if near(tool.edge_point(f64::from(sx), f64::from(sy))) {
            return Some(Handle::Edge(sx, sy));
        }
    }
    tool.contains(p).then_some(Handle::Move)
}

/// The box after dragging `handle` from `from` to `to` (turned-picture pixels), starting from
/// `start`. `ratio` locks the shape.
pub fn drag(
    start: &CropTool,
    handle: Handle,
    from: (f64, f64),
    to: (f64, f64),
    ratio: Option<f64>,
) -> CropTool {
    let mut out = start.clone();
    let (cx, cy) = start.centre();
    match handle {
        Handle::Move => {
            out.x += to.0 - from.0;
            out.y += to.1 - from.1;
        }
        Handle::Rotate => {
            let (dx, dy) = (to.0 - cx, to.1 - cy);
            if dx.hypot(dy) > 1e-9 {
                // Clockwise from straight up.
                let tilt = dx.atan2(-dy).to_degrees();
                out.angle = wrap(-tilt);
            }
        }
        Handle::Edge(sx, sy) => {
            let (dx, dy) = rot((to.0 - from.0, to.1 - from.1), -start.tilt());
            let (fx, fy) = (f64::from(sx), f64::from(sy));
            let mut w = if sx != 0 {
                (start.w + fx * dx).max(1.0)
            } else {
                start.w
            };
            let mut h = if sy != 0 {
                (start.h + fy * dy).max(1.0)
            } else {
                start.h
            };
            if let Some(r) = ratio {
                if sx != 0 {
                    h = (w / r).max(1.0);
                    w = h * r;
                } else {
                    w = (h * r).max(1.0);
                    h = w / r;
                }
            }
            // Slide the centre so the side opposite the handle stays put.
            let shift = rot(
                (
                    if sx != 0 {
                        fx * (w - start.w) / 2.0
                    } else {
                        0.0
                    },
                    if sy != 0 {
                        fy * (h - start.h) / 2.0
                    } else {
                        0.0
                    },
                ),
                start.tilt(),
            );
            let (ncx, ncy) = (cx + shift.0, cy + shift.1);
            out.w = w;
            out.h = h;
            out.x = ncx - w / 2.0;
            out.y = ncy - h / 2.0;
        }
    }
    out
}

/// Bring an angle into `-180..=180` degrees.
fn wrap(deg: f64) -> f64 {
    let d = deg % 360.0;
    if d > 180.0 {
        d - 360.0
    } else if d < -180.0 {
        d + 360.0
    } else {
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> CropTool {
        CropTool {
            x: 20.0,
            y: 10.0,
            w: 100.0,
            h: 60.0,
            ..CropTool::default()
        }
    }

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn from_edit_starts_with_the_whole_picture_or_the_current_crop() {
        let whole = CropTool::from_edit(2, &ImageEdit::default(), (640.0, 480.0));
        assert_eq!(
            (whole.x, whole.y, whole.w, whole.h, whole.image),
            (0.0, 0.0, 640.0, 480.0, 2)
        );
        let edit = ImageEdit {
            crop: Some(inbetween_doc::Crop {
                x: 5.0,
                y: 6.0,
                w: 70.0,
                h: 80.0,
            }),
            angle: 12.0,
            ..ImageEdit::default()
        };
        let t = CropTool::from_edit(1, &edit, (640.0, 480.0));
        assert_eq!((t.x, t.y, t.w, t.h, t.angle), (5.0, 6.0, 70.0, 80.0, 12.0));
        assert!(t.is_sane());
    }

    #[test]
    fn moving_translates_the_box() {
        let t = drag(&tool(), Handle::Move, (50.0, 30.0), (60.0, 25.0), None);
        assert_eq!((t.x, t.y, t.w, t.h), (30.0, 5.0, 100.0, 60.0));
    }

    #[test]
    fn a_corner_drag_keeps_the_opposite_corner_fixed() {
        let start = tool();
        let t = drag(
            &start,
            Handle::Edge(1, 1),
            (120.0, 70.0),
            (150.0, 90.0),
            None,
        );
        assert!(close((t.x, t.y), (20.0, 10.0)));
        assert!(close((t.w, t.h), (130.0, 80.0)));
        let t = drag(
            &start,
            Handle::Edge(-1, -1),
            (20.0, 10.0),
            (30.0, 25.0),
            None,
        );
        let c = t.corners();
        assert!(close(c[2], (120.0, 70.0)), "bottom-right stays at {c:?}");
        assert!(close((t.w, t.h), (90.0, 45.0)));
    }

    #[test]
    fn an_edge_drag_changes_one_dimension() {
        let t = drag(
            &tool(),
            Handle::Edge(1, 0),
            (120.0, 40.0),
            (140.0, 99.0),
            None,
        );
        assert_eq!((t.w, t.h), (120.0, 60.0));
        assert!(close(t.edge_point(-1.0, 0.0), (20.0, 40.0)));
        let t = drag(&tool(), Handle::Edge(0, 1), (70.0, 70.0), (0.0, 80.0), None);
        assert_eq!((t.w, t.h), (100.0, 70.0));
    }

    #[test]
    fn a_tilted_box_resizes_along_its_own_edges() {
        let start = CropTool {
            angle: -30.0, // tilted 30 degrees clockwise
            ..tool()
        };
        let far = start.edge_point(-1.0, -1.0);
        let grab = start.edge_point(1.0, 1.0);
        // Drag along the box's own x axis by 20.
        let (s, c) = start.tilt().sin_cos();
        let to = (grab.0 + 20.0 * c, grab.1 + 20.0 * s);
        let t = drag(&start, Handle::Edge(1, 1), grab, to, None);
        assert!(
            close(t.edge_point(-1.0, -1.0), far),
            "the far corner stays put"
        );
        assert!((t.w - 120.0).abs() < 1e-9 && (t.h - 60.0).abs() < 1e-9);
    }

    #[test]
    fn the_rotate_handle_sets_the_angle_and_the_box_tilts_the_opposite_way() {
        let start = tool();
        let (cx, cy) = start.centre();
        // Straight right of the centre is 90 degrees clockwise from up.
        let t = drag(
            &start,
            Handle::Rotate,
            (cx, cy - 50.0),
            (cx + 40.0, cy),
            None,
        );
        assert!((t.angle + 90.0).abs() < 1e-9, "{}", t.angle);
        assert!((t.tilt().to_degrees() - 90.0).abs() < 1e-9);
        // The rotate handle now sits to the right of the box.
        let h = t.rotate_point(10.0);
        assert!(h.0 > cx + 30.0 && (h.1 - cy).abs() < 1e-6);
        // Wrapping.
        let t = drag(
            &start,
            Handle::Rotate,
            (cx, cy - 50.0),
            (cx - 1.0, cy + 50.0),
            None,
        );
        assert!(t.angle.abs() <= 180.0);
    }

    #[test]
    fn a_locked_ratio_keeps_the_shape() {
        let start = tool();
        let r = Some(2.0);
        let t = drag(&start, Handle::Edge(1, 1), (120.0, 70.0), (160.0, 71.0), r);
        assert!((t.w / t.h - 2.0).abs() < 1e-9);
        assert!(close(t.edge_point(-1.0, -1.0), (20.0, 10.0)));
        let t = drag(&start, Handle::Edge(0, 1), (70.0, 70.0), (70.0, 100.0), r);
        assert!((t.w / t.h - 2.0).abs() < 1e-9);
        let mut s = start;
        s.snap_aspect(1.0);
        assert_eq!((s.w, s.h), (100.0, 100.0));
        assert!(close(s.centre(), (70.0, 40.0)), "snapping keeps the centre");
    }

    #[test]
    fn the_box_never_collapses_and_hostile_input_stays_finite() {
        let t = drag(
            &tool(),
            Handle::Edge(1, 1),
            (120.0, 70.0),
            (-1e9, -1e9),
            None,
        );
        assert!(t.w >= 1.0 && t.h >= 1.0 && t.is_sane());
        let bad = drag(
            &tool(),
            Handle::Edge(1, 0),
            (0.0, 0.0),
            (f64::NAN, 0.0),
            None,
        );
        assert!(!bad.is_sane() || bad.w >= 1.0);
        assert!(!CropTool {
            w: f64::NAN,
            ..CropTool::default()
        }
        .is_sane());
    }

    #[test]
    fn picking_prefers_handles_then_the_body() {
        let t = tool();
        assert_eq!(
            pick(&t, (20.5, 10.5), 3.0, 20.0),
            Some(Handle::Edge(-1, -1))
        );
        assert_eq!(pick(&t, (120.0, 40.0), 3.0, 20.0), Some(Handle::Edge(1, 0)));
        assert_eq!(pick(&t, (70.0, -10.0), 3.0, 20.0), Some(Handle::Rotate));
        assert_eq!(pick(&t, (70.0, 40.0), 3.0, 20.0), Some(Handle::Move));
        assert_eq!(pick(&t, (500.0, 500.0), 3.0, 20.0), None);
    }

    #[test]
    fn aspect_settings_parse() {
        let (turned, out) = ((400.0, 200.0), (16.0, 9.0));
        assert_eq!(ratio_of("free", turned, out), None);
        assert_eq!(ratio_of("original", turned, out), Some(2.0));
        assert_eq!(ratio_of("output", turned, out), Some(16.0 / 9.0));
        assert_eq!(ratio_of("4:3", turned, out), Some(4.0 / 3.0));
        assert_eq!(ratio_of("0:3", turned, out), None);
        assert_eq!(ratio_of("wide", turned, out), None);
    }
}
