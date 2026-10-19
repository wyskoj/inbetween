//! The camera pass: move, scale, flip and rotate a rendered morph frame.
//!
//! Scale and rotation turn about the frame centre; the offset is in frame widths and heights.
//! Output pixels the moved picture does not cover are transparent.

use crate::RenderError;
use inbetween_doc::{Camera, CameraKey};
use inbetween_raster::{Image, TRANSPARENT};
use rayon::prelude::*;

/// Slack at the picture's edge so a picture moved by whole pixels keeps its border pixels.
const EDGE_EPS: f64 = 1e-9;

/// Frame `frame` seen through `pose`.
pub fn apply_pose(pose: &CameraKey, frame: &Image) -> Result<Image, RenderError> {
    let (width, height) = frame.size();
    let mut out = Image::new(width, height)?;
    let (w, h) = (f64::from(width), f64::from(height));
    let (cx, cy) = (w / 2.0, h / 2.0);
    let (tx, ty) = (pose.x * w, pose.y * h);
    let (sin, cos) = pose.rotation.to_radians().sin_cos();
    // A scale of zero is refused by validation; guard anyway so a hostile pose cannot divide
    // by it.
    let inv = |s: f64| if s.abs() > 1e-9 { 1.0 / s } else { 0.0 };
    let (isx, isy) = (inv(pose.scale_x), inv(pose.scale_y));
    out.as_mut_slice()
        .par_chunks_mut(width as usize * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let (dx, dy) = (x as f64 + 0.5 - cx - tx, y as f64 + 0.5 - cy - ty);
                // Undo the rotation (clockwise on screen with y down), then the scale.
                let (ux, uy) = (dx * cos + dy * sin, -dx * sin + dy * cos);
                let (sx, sy) = (ux * isx + cx, uy * isy + cy);
                let inside =
                    sx >= -EDGE_EPS && sy >= -EDGE_EPS && sx <= w + EDGE_EPS && sy <= h + EDGE_EPS;
                let colour = if inside && sx.is_finite() && sy.is_finite() {
                    frame.sample_bilinear(sx as f32, sy as f32)
                } else {
                    TRANSPARENT
                };
                px.copy_from_slice(&colour);
            }
        });
    Ok(out)
}

/// Frame `frame` seen through the camera at morph time `t`. A camera that is off, or whose
/// pose at `t` is the identity, returns the frame untouched.
pub fn apply(camera: &Camera, t: f64, frame: Image) -> Result<Image, RenderError> {
    if !camera.enabled {
        return Ok(frame);
    }
    let pose = camera.pose(t);
    if pose.is_identity() {
        return Ok(frame);
    }
    apply_pose(&pose, &frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::CameraKey;

    fn ramp() -> Image {
        let mut img = Image::new(8, 6).unwrap();
        for y in 0..6 {
            for x in 0..8 {
                img.set(x, y, [x as f32 / 8.0, y as f32 / 6.0, 0.5, 1.0]);
            }
        }
        img
    }

    #[test]
    fn identity_pose_is_a_copy() {
        let img = ramp();
        let out = apply_pose(&CameraKey::identity(0.0), &img).unwrap();
        assert_eq!(out, img);
    }

    #[test]
    fn whole_pixel_offsets_shift_exactly() {
        let img = ramp();
        let pose = CameraKey {
            x: 2.0 / 8.0,
            ..CameraKey::identity(0.0)
        };
        let out = apply_pose(&pose, &img).unwrap();
        assert_eq!(out.get(5, 3), img.get(3, 3));
        assert_eq!(out.get(0, 3), Some(TRANSPARENT));
        assert_eq!(out.get(1, 3), Some(TRANSPARENT));
        assert_eq!(out.get(2, 3), img.get(0, 3));
    }

    #[test]
    fn negative_scale_flips() {
        let img = ramp();
        let pose = CameraKey {
            scale_x: -1.0,
            ..CameraKey::identity(0.0)
        };
        let out = apply_pose(&pose, &img).unwrap();
        for x in 0..8 {
            assert_eq!(out.get(x, 2), img.get(7 - x, 2), "x = {x}");
        }
    }

    #[test]
    fn a_half_turn_rotates_the_picture_around_the_centre() {
        let img = ramp();
        let pose = CameraKey {
            rotation: 180.0,
            ..CameraKey::identity(0.0)
        };
        let out = apply_pose(&pose, &img).unwrap();
        for (x, y) in [(0, 0), (3, 2), (7, 5)] {
            let (a, b) = (out.get(x, y).unwrap(), img.get(7 - x, 5 - y).unwrap());
            for c in 0..4 {
                assert!((a[c] - b[c]).abs() < 1e-4, "({x}, {y}) channel {c}");
            }
        }
    }

    #[test]
    fn off_or_identity_camera_returns_the_frame_untouched() {
        let img = ramp();
        let mut cam = Camera::default();
        cam.set_key(CameraKey {
            x: 0.5,
            ..CameraKey::identity(1.0)
        })
        .unwrap();
        assert_eq!(apply(&cam, 1.0, img.clone()).unwrap(), img);
        cam.enabled = true;
        // At t = 0 the pose is the first key held, which moves the picture.
        assert_ne!(apply(&cam, 0.0, img.clone()).unwrap(), img);
        cam.keys.clear();
        assert_eq!(apply(&cam, 0.5, img.clone()).unwrap(), img);
    }

    #[test]
    fn hostile_poses_do_not_panic_or_make_nan() {
        let img = ramp();
        for pose in [
            CameraKey {
                scale_x: 0.0,
                ..CameraKey::default()
            },
            CameraKey {
                x: f64::NAN,
                ..CameraKey::default()
            },
            CameraKey {
                rotation: f64::INFINITY,
                ..CameraKey::default()
            },
            CameraKey {
                scale_x: 1e300,
                scale_y: -1e-300,
                ..CameraKey::default()
            },
        ] {
            let out = apply_pose(&pose, &img).unwrap();
            assert!(out.as_slice().iter().all(|v| v.is_finite()), "{pose:?}");
        }
    }
}
