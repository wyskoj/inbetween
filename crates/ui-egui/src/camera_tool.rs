//! The camera handles' geometry, kept apart from egui so it can be tested on its own.
//!
//! A camera pose moves the picture: a point `s` of the frame lands at
//! `centre + offset + R(rotation) * S * (s - centre)`, with the offset in frame widths and
//! heights. The tool draws the moved picture's outline. Dragging inside it moves it, the square
//! handles on its corners and edges scale it (crossing the opposite side flips it), and the
//! round handle above the top edge rotates it. All coordinates are frame pixels.

use inbetween_doc::{CameraKey, MAX_SCALE, MIN_SCALE};

/// What a drag grabbed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    /// The inside of the picture: move it.
    Move,
    /// The round handle above the top edge.
    Rotate,
    /// A corner or an edge. `(-1, -1)` is the top-left corner, `(1, 0)` the right edge.
    Scale(i8, i8),
}

/// Handles in the order they are hit-tested: rotate, corners, then edges.
pub const SCALE_HANDLES: [(i8, i8); 8] = [
    (-1, -1),
    (1, -1),
    (1, 1),
    (-1, 1),
    (0, -1),
    (1, 0),
    (0, 1),
    (-1, 0),
];

/// How far above the top edge the rotate handle sits, in frame pixels per unit of zoom
/// (the caller passes the distance already converted).
pub fn rotate_handle_gap(reach: f64) -> f64 {
    reach * 4.0
}

fn rotate(v: (f64, f64), degrees: f64) -> (f64, f64) {
    let (s, c) = degrees.to_radians().sin_cos();
    (v.0 * c - v.1 * s, v.0 * s + v.1 * c)
}

/// Frame point for a point `local` of the picture, measured from its centre.
pub fn to_frame(pose: &CameraKey, frame: (f64, f64), local: (f64, f64)) -> (f64, f64) {
    let (w, h) = frame;
    let r = rotate(
        (local.0 * pose.scale_x, local.1 * pose.scale_y),
        pose.rotation,
    );
    (w / 2.0 + pose.x * w + r.0, h / 2.0 + pose.y * h + r.1)
}

/// The picture-local point under a frame point (the inverse of [`to_frame`]); `None` when a
/// scale is zero.
pub fn to_local(pose: &CameraKey, frame: (f64, f64), at: (f64, f64)) -> Option<(f64, f64)> {
    let (w, h) = frame;
    if pose.scale_x.abs() < 1e-9 || pose.scale_y.abs() < 1e-9 {
        return None;
    }
    let d = (at.0 - w / 2.0 - pose.x * w, at.1 - h / 2.0 - pose.y * h);
    let u = rotate(d, -pose.rotation);
    Some((u.0 / pose.scale_x, u.1 / pose.scale_y))
}

/// Where a handle sits on the frame.
pub fn handle_position(pose: &CameraKey, frame: (f64, f64), hx: i8, hy: i8) -> (f64, f64) {
    to_frame(
        pose,
        frame,
        (f64::from(hx) * frame.0 / 2.0, f64::from(hy) * frame.1 / 2.0),
    )
}

/// Where the rotate handle sits: above the top edge's middle, `gap` frame pixels out.
pub fn rotate_position(pose: &CameraKey, frame: (f64, f64), gap: f64) -> (f64, f64) {
    let top = handle_position(pose, frame, 0, -1);
    let up = rotate((0.0, -gap), pose.rotation);
    (top.0 + up.0, top.1 + up.1)
}

/// The four corners of the moved picture, clockwise from the top-left.
pub fn outline(pose: &CameraKey, frame: (f64, f64)) -> [(f64, f64); 4] {
    [(-1, -1), (1, -1), (1, 1), (-1, 1)].map(|(x, y)| handle_position(pose, frame, x, y))
}

fn near(a: (f64, f64), b: (f64, f64), reach: f64) -> bool {
    (a.0 - b.0).hypot(a.1 - b.1) <= reach
}

/// What a press at `at` grabs, if anything. `reach` is the grab radius in frame pixels.
pub fn pick(pose: &CameraKey, frame: (f64, f64), at: (f64, f64), reach: f64) -> Option<Grab> {
    if near(
        at,
        rotate_position(pose, frame, rotate_handle_gap(reach)),
        reach,
    ) {
        return Some(Grab::Rotate);
    }
    for (hx, hy) in SCALE_HANDLES {
        if near(at, handle_position(pose, frame, hx, hy), reach) {
            return Some(Grab::Scale(hx, hy));
        }
    }
    let (lx, ly) = to_local(pose, frame, at)?;
    (lx.abs() <= frame.0 / 2.0 && ly.abs() <= frame.1 / 2.0).then_some(Grab::Move)
}

fn clamp_scale(s: f64) -> f64 {
    let magnitude = s.abs().clamp(MIN_SCALE, MAX_SCALE);
    if s.is_nan() {
        1.0
    } else {
        magnitude.copysign(s)
    }
}

/// The pose after dragging `grab`, pressed at `start_at` with `start` showing, to `to`.
/// Scales follow the pointer in the picture's own frame, so the side opposite the handle stays
/// where it is and crossing it flips the picture.
pub fn drag(
    grab: Grab,
    start: &CameraKey,
    frame: (f64, f64),
    start_at: (f64, f64),
    to: (f64, f64),
) -> CameraKey {
    let (w, h) = frame;
    let mut pose = *start;
    match grab {
        Grab::Move => {
            pose.x = start.x + (to.0 - start_at.0) / w.max(1.0);
            pose.y = start.y + (to.1 - start_at.1) / h.max(1.0);
        }
        Grab::Rotate => {
            let dx = to.0 - w / 2.0 - start.x * w;
            let dy = to.1 - h / 2.0 - start.y * h;
            if dx != 0.0 || dy != 0.0 {
                pose.rotation = dx.atan2(-dy).to_degrees();
            }
        }
        Grab::Scale(hx, hy) => {
            // The pointer in the picture's own axes, unscaled.
            let d = (to.0 - w / 2.0 - start.x * w, to.1 - h / 2.0 - start.y * h);
            let u = rotate(d, -start.rotation);
            if hx != 0 {
                pose.scale_x = clamp_scale(u.0 / (f64::from(hx) * w.max(1.0) / 2.0));
            }
            if hy != 0 {
                pose.scale_y = clamp_scale(u.1 / (f64::from(hy) * h.max(1.0) / 2.0));
            }
        }
    }
    pose
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: (f64, f64) = (200.0, 100.0);

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
    }

    #[test]
    fn identity_outline_is_the_frame() {
        let o = outline(&CameraKey::identity(0.0), FRAME);
        assert!(close(o[0], (0.0, 0.0)) && close(o[2], (200.0, 100.0)));
    }

    #[test]
    fn to_local_inverts_to_frame() {
        let pose = CameraKey {
            x: 0.1,
            y: -0.2,
            scale_x: -1.5,
            scale_y: 0.7,
            rotation: 33.0,
            ..CameraKey::identity(0.0)
        };
        let p = to_frame(&pose, FRAME, (30.0, -12.0));
        assert!(close(to_local(&pose, FRAME, p).unwrap(), (30.0, -12.0)));
    }

    #[test]
    fn picks_handles_then_the_body_then_nothing() {
        let pose = CameraKey::identity(0.0);
        assert_eq!(pick(&pose, FRAME, (100.0, 50.0), 6.0), Some(Grab::Move));
        assert_eq!(
            pick(&pose, FRAME, (199.0, 99.0), 6.0),
            Some(Grab::Scale(1, 1))
        );
        assert_eq!(
            pick(&pose, FRAME, (200.0, 50.0), 6.0),
            Some(Grab::Scale(1, 0))
        );
        assert_eq!(pick(&pose, FRAME, (100.0, -24.0), 6.0), Some(Grab::Rotate));
        assert_eq!(pick(&pose, FRAME, (500.0, 500.0), 6.0), None);
        // A flattened picture still answers without dividing by zero.
        let _ = pick(
            &CameraKey {
                scale_x: 0.0,
                ..pose
            },
            FRAME,
            (150.0, 50.0),
            6.0,
        );
    }

    #[test]
    fn moving_shifts_by_whole_frame_fractions() {
        let p = drag(
            Grab::Move,
            &CameraKey::identity(0.0),
            FRAME,
            (10.0, 10.0),
            (60.0, 30.0),
        );
        assert!((p.x - 0.25).abs() < 1e-9 && (p.y - 0.2).abs() < 1e-9);
    }

    #[test]
    fn scaling_follows_the_pointer_and_flips_across_the_centre() {
        let start = CameraKey::identity(0.0);
        let p = drag(
            Grab::Scale(1, 1),
            &start,
            FRAME,
            (200.0, 100.0),
            (300.0, 150.0),
        );
        assert!((p.scale_x - 2.0).abs() < 1e-9 && (p.scale_y - 2.0).abs() < 1e-9);
        let p = drag(Grab::Scale(1, 0), &start, FRAME, (200.0, 50.0), (0.0, 50.0));
        assert!((p.scale_x + 1.0).abs() < 1e-9, "{}", p.scale_x);
        let p = drag(
            Grab::Scale(1, 0),
            &start,
            FRAME,
            (200.0, 50.0),
            (-100.0, 50.0),
        );
        assert!((p.scale_x + 2.0).abs() < 1e-9, "{}", p.scale_x);
        assert_eq!(p.scale_y, 1.0);
        let p = drag(Grab::Scale(-1, 0), &start, FRAME, (0.0, 50.0), (1e9, 50.0));
        assert!(p.scale_x <= -MIN_SCALE || p.scale_x >= MIN_SCALE);
        assert!(p.scale_x.abs() <= MAX_SCALE);
    }

    #[test]
    fn rotating_points_the_top_handle_at_the_pointer() {
        let start = CameraKey::identity(0.0);
        let p = drag(Grab::Rotate, &start, FRAME, (100.0, 0.0), (300.0, 50.0));
        assert!((p.rotation - 90.0).abs() < 1e-9, "{}", p.rotation);
        let again = drag(Grab::Rotate, &p, FRAME, (0.0, 0.0), (100.0, 0.0));
        assert!((again.rotation + 0.0).abs() < 1e-9 || again.rotation.abs() > 0.0);
        // The pointer exactly on the centre leaves the rotation alone.
        let same = drag(Grab::Rotate, &start, FRAME, (0.0, 0.0), (100.0, 50.0));
        assert_eq!(same.rotation, 0.0);
    }

    #[test]
    fn hostile_numbers_stay_finite() {
        let start = CameraKey::identity(0.0);
        for to in [(f64::NAN, 1.0), (f64::INFINITY, 0.0), (1e300, -1e300)] {
            for grab in [
                Grab::Move,
                Grab::Rotate,
                Grab::Scale(1, 1),
                Grab::Scale(0, -1),
            ] {
                let p = drag(grab, &start, FRAME, (0.0, 0.0), to);
                assert!(!p.scale_x.is_nan() && !p.scale_y.is_nan());
            }
        }
    }
}
