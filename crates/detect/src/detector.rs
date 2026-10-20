//! The two-stage detector: a short-range face detector finds boxes and eye keypoints, then a
//! landmark network places a 468-point mesh inside a rotated square crop around each face.

use crate::anchors::{self, Anchor, Detection, INPUT};
use crate::geometry::{self, Roi};
use crate::DetectError;
use inbetween_geom::Point;
use inbetween_raster::transform::resample;
use inbetween_raster::Image;
use tract_onnx::prelude::*;

/// Longest image side the pipeline works at; bigger inputs are box-averaged down first.
pub const MAX_SIDE: u32 = 4096;
/// Longest side of the copy the box detector looks at.
const DETECT_SIDE: u32 = 1024;
/// Side of the landmark network's crop.
const MESH_SIDE: u32 = 192;
/// Detections below this score are ignored.
pub const MIN_SCORE: f32 = 0.5;
/// Most faces reported for one image.
pub const MAX_FACES: usize = 16;
/// Number of mesh points the landmark network returns.
pub const MESH_POINTS: usize = 468;

#[cfg(feature = "bundled-models")]
const BLAZEFACE: &[u8] = include_bytes!("../models/blazeface.onnx");
#[cfg(feature = "bundled-models")]
const LANDMARKS: &[u8] = include_bytes!("../models/face_landmarks.onnx");

type Plan = Arc<TypedRunnableModel>;

/// An axis-aligned box in image pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// One detected face.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub score: f32,
    /// Box around the mesh, in image pixels.
    pub bounds: Bounds,
    /// The 468 mesh points in image pixels (pixel `(0, 0)` has its corner at `(0, 0)`).
    pub mesh: Vec<Point>,
}

pub struct Detector {
    blaze: Plan,
    mesh: Plan,
    anchors: Vec<Anchor>,
}

/// The windows `(x, y, w, h)` the box detector looks at: the whole image, plus squares along
/// the long side of a wide or tall image (evenly spaced, each overlapping the next, together
/// covering the image) so small faces are not squeezed into a thin letterboxed strip.
fn windows(w: u32, h: u32) -> Vec<(u32, u32, u32, u32)> {
    let mut out = vec![(0, 0, w, h)];
    let (long, short) = (w.max(h), w.min(h));
    if short == 0 || f64::from(long) <= f64::from(short) * 1.25 {
        return out;
    }
    let spare = long - short;
    let gaps = spare.div_ceil(short).max(1);
    for i in 0..=gaps {
        let pos = (u64::from(spare) * u64::from(i) / u64::from(gaps)) as u32;
        out.push(if w >= h {
            (pos, 0, short, short)
        } else {
            (0, pos, short, short)
        });
    }
    out
}

fn model_err(e: impl std::fmt::Display) -> DetectError {
    DetectError::Model(e.to_string())
}

fn load(bytes: &[u8], side: usize) -> Result<Plan, DetectError> {
    let mut cursor = std::io::Cursor::new(bytes);
    let model = tract_onnx::onnx()
        .model_for_read(&mut cursor)
        .map_err(model_err)?;
    let model = model
        .with_input_fact(0, f32::fact([1, side, side, 3]).into())
        .map_err(model_err)?;
    model
        .into_optimized()
        .map_err(model_err)?
        .into_runnable()
        .map_err(model_err)
}

/// RGB values of `img` as an NHWC tensor, mapped by `(v * scale) + offset` (premultiplied
/// colour, i.e. transparent areas are black).
fn to_tensor(img: &Image, scale: f32, offset: f32) -> Result<Tensor, DetectError> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut data = Vec::with_capacity(w * h * 3);
    for px in img.as_slice().chunks_exact(4) {
        for v in px.iter().take(3) {
            let v = if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            };
            data.push(v * scale + offset);
        }
    }
    let arr = tract_ndarray::Array4::from_shape_vec((1, h, w, 3), data).map_err(model_err)?;
    Ok(arr.into())
}

fn floats(t: &TValue) -> Result<Vec<f32>, DetectError> {
    Ok(t.to_plain_array_view::<f32>()
        .map_err(model_err)?
        .iter()
        .copied()
        .collect())
}

impl Detector {
    /// The detector with the bundled models. Parsing and optimising takes about 100 ms, so
    /// callers keep one around.
    #[cfg(feature = "bundled-models")]
    pub fn new() -> Result<Self, DetectError> {
        Self::from_bytes(BLAZEFACE, LANDMARKS)
    }

    #[cfg(not(feature = "bundled-models"))]
    pub fn new() -> Result<Self, DetectError> {
        Err(DetectError::Model(
            "this build does not include the face detector models".into(),
        ))
    }

    /// A detector from two ONNX files: the box detector and the 468-point landmark network.
    pub fn from_bytes(blazeface: &[u8], landmarks: &[u8]) -> Result<Self, DetectError> {
        Ok(Self {
            blaze: load(blazeface, INPUT)?,
            mesh: load(landmarks, MESH_SIDE as usize)?,
            anchors: anchors::generate(),
        })
    }

    /// Find the faces in `img`, best first. No face is `Ok(vec![])`.
    pub fn faces(&self, img: &Image) -> Result<Vec<Face>, DetectError> {
        let ow = img.width();
        // Borrow the image when it is already small enough; shrinking clones it otherwise.
        let shrunk;
        let work: &Image = if img.width().max(img.height()) <= MAX_SIDE {
            img
        } else {
            shrunk = img.downscale_to_fit(MAX_SIDE).map_err(model_err)?;
            &shrunk
        };
        let shrunk_base;
        let base: &Image = if work.width().max(work.height()) <= DETECT_SIDE {
            work
        } else {
            shrunk_base = work.downscale_to_fit(DETECT_SIDE).map_err(model_err)?;
            &shrunk_base
        };
        // A NaN or infinity anywhere in `img` survives the box averages into `base`, so the
        // scan covers the small copy only.
        if base.as_slice().iter().any(|v| !v.is_finite()) {
            return Err(DetectError::BadInput(
                "the image contains non-finite pixel values".into(),
            ));
        }
        let wins = windows(base.width(), base.height());
        let results: Vec<Result<Vec<Detection>, DetectError>> = std::thread::scope(|s| {
            // A window whose thread cannot start (no threads on this platform) runs inline.
            let handles: Vec<_> = wins
                .iter()
                .map(|&(x, y, w, h)| {
                    std::thread::Builder::new()
                        .spawn_scoped(s, move || self.detect_window(base, x, y, w, h))
                        .map_err(|_| (x, y, w, h))
                })
                .collect();
            handles
                .into_iter()
                .map(|h| match h {
                    Ok(handle) => handle.join().unwrap_or_else(|_| {
                        Err(DetectError::Model("a detector thread stopped".into()))
                    }),
                    Err((x, y, w, h)) => self.detect_window(base, x, y, w, h),
                })
                .collect()
        });
        let mut dets = Vec::new();
        for r in results {
            dets.extend(r?);
        }
        let dets = anchors::weighted_nms(dets, 0.3);
        let k_work = f64::from(work.width()) / f64::from(base.width());
        let k_orig = f64::from(ow) / f64::from(work.width());
        let mut faces = Vec::new();
        for d in dets.iter().take(MAX_FACES) {
            if let Some(face) = self.mesh_for(work, d, k_work, k_orig)? {
                faces.push(face);
            }
        }
        faces.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then(a.bounds.x0.total_cmp(&b.bounds.x0))
        });
        Ok(faces)
    }

    /// Run the box detector on the `w` x `h` window of `base` at `(x, y)`, letterboxed into the
    /// network's square. Detections come back in `base` pixels.
    fn detect_window(
        &self,
        base: &Image,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    ) -> Result<Vec<Detection>, DetectError> {
        let lb = geometry::letterbox_to_image(w, h, INPUT as u32)
            .ok_or_else(|| DetectError::BadInput("the image has no size".into()))?;
        let map = [
            lb[0],
            lb[1],
            lb[2] + f64::from(x),
            lb[3],
            lb[4],
            lb[5] + f64::from(y),
        ];
        let boxed = resample(base, INPUT as u32, INPUT as u32, map).map_err(model_err)?;
        let out = self
            .blaze
            .run(tvec!(to_tensor(&boxed, 2.0, -1.0)?.into()))
            .map_err(model_err)?;
        let (raw, scores) = match (out.first(), out.get(1)) {
            (Some(a), Some(b)) => (floats(a)?, floats(b)?),
            _ => {
                return Err(DetectError::Model(
                    "detector returned too few outputs".into(),
                ))
            }
        };
        if raw.len() != anchors::ANCHOR_COUNT * anchors::REGRESSORS
            || scores.len() != anchors::ANCHOR_COUNT
        {
            return Err(DetectError::Model(
                "detector output has an unexpected shape".into(),
            ));
        }
        let side = INPUT as f64;
        let to_base = |u: f32, v: f32| {
            let (px, py) = geometry::apply(&map, f64::from(u) * side, f64::from(v) * side);
            (px as f32, py as f32)
        };
        let k = (lb[0] * side) as f32;
        let dets = anchors::decode(&raw, &scores, &self.anchors, MIN_SCORE)
            .into_iter()
            .map(|d| {
                let (cx, cy) = to_base(d.cx, d.cy);
                let mut keypoints = d.keypoints;
                for kp in &mut keypoints {
                    *kp = to_base(kp.0, kp.1);
                }
                Detection {
                    score: d.score,
                    cx,
                    cy,
                    w: d.w * k,
                    h: d.h * k,
                    keypoints,
                }
            })
            .collect();
        Ok(dets)
    }

    /// `d` is in `base` pixels; `k_work` takes those to `work` pixels.
    fn mesh_for(
        &self,
        work: &Image,
        d: &Detection,
        k_work: f64,
        k_orig: f64,
    ) -> Result<Option<Face>, DetectError> {
        let to_work = |x: f32, y: f32| (f64::from(x) * k_work, f64::from(y) * k_work);
        let (cx, cy) = to_work(d.cx, d.cy);
        let box_size = f64::from(d.w.max(d.h)) * k_work;
        let right_eye = to_work(d.keypoints[0].0, d.keypoints[0].1);
        let left_eye = to_work(d.keypoints[1].0, d.keypoints[1].1);
        let Some(roi) = Roi::from_eyes(right_eye, left_eye, (cx, cy), box_size, 1.5) else {
            return Ok(None);
        };
        let map = roi.crop_to_image(MESH_SIDE);
        let crop = resample(work, MESH_SIDE, MESH_SIDE, map).map_err(model_err)?;
        let out = self
            .mesh
            .run(tvec!(to_tensor(&crop, 1.0, 0.0)?.into()))
            .map_err(model_err)?;
        let (pts, score) = match (out.first(), out.get(1)) {
            (Some(a), Some(b)) => (floats(a)?, floats(b)?),
            _ => {
                return Err(DetectError::Model(
                    "landmark network returned too few outputs".into(),
                ))
            }
        };
        let presence = score.first().copied().unwrap_or(f32::MIN);
        if pts.len() != MESH_POINTS * 3 || presence.is_nan() {
            return Err(DetectError::Model(
                "landmark output has an unexpected shape".into(),
            ));
        }
        let presence = 1.0 / (1.0 + (-presence.clamp(-80.0, 80.0)).exp());
        if presence < MIN_SCORE {
            return Ok(None);
        }
        let mut mesh = Vec::with_capacity(MESH_POINTS);
        for p in pts.chunks_exact(3) {
            let (x, y) = geometry::apply(&map, f64::from(p[0]), f64::from(p[1]));
            let pt = Point::new(x * k_orig, y * k_orig);
            if !pt.is_finite() {
                return Ok(None);
            }
            mesh.push(pt);
        }
        let bounds = mesh.iter().fold(
            Bounds {
                x0: f64::MAX,
                y0: f64::MAX,
                x1: f64::MIN,
                y1: f64::MIN,
            },
            |b, p| Bounds {
                x0: b.x0.min(p.x),
                y0: b.y0.min(p.y),
                x1: b.x1.max(p.x),
                y1: b.y1.max(p.y),
            },
        );
        Ok(Some(Face {
            score: d.score.min(presence).max(0.0),
            bounds,
            mesh,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::windows;

    #[test]
    fn square_images_get_one_window() {
        assert_eq!(windows(500, 500), vec![(0, 0, 500, 500)]);
        assert_eq!(windows(500, 400), vec![(0, 0, 500, 400)]);
    }

    #[test]
    fn wide_and_tall_images_get_overlapping_squares_that_cover_them() {
        let w = windows(1024, 640);
        assert_eq!(w[0], (0, 0, 1024, 640));
        assert!(w.len() > 2);
        assert!(w[1..]
            .iter()
            .all(|&(x, y, a, b)| y == 0 && a == 640 && b == 640 && x + a <= 1024));
        assert_eq!(w.last().map(|t| t.0 + t.2), Some(1024));
        let t = windows(300, 1200);
        assert!(t[1..]
            .iter()
            .all(|&(x, y, a, b)| x == 0 && a == 300 && b == 300 && y + b <= 1200));
        assert_eq!(t.last().map(|v| v.1 + v.3), Some(1200));
    }

    #[test]
    fn degenerate_sizes_do_not_loop() {
        assert_eq!(windows(1, 1).len(), 1);
        assert_eq!(windows(9000, 1).len(), 1 + windows(9000, 1).len() - 1);
        assert!(windows(4096, 3).len() < 10_000);
    }
}

/// The bundled networks against the original TFLite models they were converted from.
///
/// The reference outputs in `tests/data/reference/` were produced by the TFLite interpreter
/// from the exact input tensors this test builds (`INBETWEEN_DUMP_REF=<dir>` writes those
/// inputs; see `models/README.md`). The test runs the converted ONNX files in tract on the
/// same inputs and requires the outputs to agree.
#[cfg(all(test, feature = "bundled-models"))]
mod reference {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn portrait() -> Image {
        let path = format!("{}/tests/data/portrait_a.png", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(path).expect("portrait");
        let d = inbetween_codecs::decode(&bytes).expect("decodes");
        Image::from_rgba8(d.width, d.height, &d.data).expect("image")
    }

    fn floats_of(t: &Tensor) -> Vec<f32> {
        t.to_plain_array_view::<f32>()
            .expect("f32")
            .iter()
            .copied()
            .collect()
    }

    fn read_ref(name: &str) -> Option<Vec<f32>> {
        let path = format!(
            "{}/tests/data/reference/{name}.bin",
            env!("CARGO_MANIFEST_DIR")
        );
        let bytes = std::fs::read(path).ok()?;
        Some(
            bytes
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect(),
        )
    }

    fn write_bin(dir: &std::path::Path, name: &str, v: &[f32]) {
        let bytes: Vec<u8> = v.iter().flat_map(|f| f.to_le_bytes()).collect();
        std::fs::create_dir_all(dir).expect("dir");
        std::fs::write(dir.join(format!("{name}.bin")), bytes).expect("write");
    }

    fn max_diff(a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len());
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f32::max)
    }

    #[test]
    fn converted_models_match_the_tflite_reference() {
        let det = Detector::new().expect("loads");
        let img = portrait();
        // Detector input: the whole portrait letterboxed into 128 x 128.
        let lb = geometry::letterbox_to_image(img.width(), img.height(), INPUT as u32).expect("lb");
        let boxed = resample(&img, INPUT as u32, INPUT as u32, lb).expect("resample");
        let blaze_in = to_tensor(&boxed, 2.0, -1.0).expect("tensor");
        // Mesh input: a fixed, slightly rotated square around the face.
        let roi = Roi {
            cx: 317.0,
            cy: 170.0,
            size: 170.0,
            angle: 0.05,
        };
        let crop =
            resample(&img, MESH_SIDE, MESH_SIDE, roi.crop_to_image(MESH_SIDE)).expect("crop");
        let mesh_in = to_tensor(&crop, 1.0, 0.0).expect("tensor");
        if let Some(dir) = std::env::var_os("INBETWEEN_DUMP_REF") {
            let dir = std::path::PathBuf::from(dir);
            write_bin(&dir, "blaze_input", &floats_of(&blaze_in));
            write_bin(&dir, "mesh_input", &floats_of(&mesh_in));
        }
        let blaze = det.blaze.run(tvec!(blaze_in.into())).expect("run");
        let mesh = det.mesh.run(tvec!(mesh_in.into())).expect("run");
        let (regressors, scores) = (floats_of(&blaze[0]), floats_of(&blaze[1]));
        let (points, presence) = (floats_of(&mesh[0]), floats_of(&mesh[1]));
        let cases = [
            ("blaze_regressors", &regressors, 5e-3f32),
            ("blaze_scores", &scores, 5e-3),
            ("mesh_points", &points, 5e-3),
            ("mesh_presence", &presence, 1e-3),
        ];
        for (name, got, tolerance) in cases {
            let want = read_ref(name).unwrap_or_else(|| panic!("missing reference {name}"));
            let diff = max_diff(got, &want);
            eprintln!("{name}: {} values, max difference {diff:.6}", got.len());
            assert!(
                diff <= tolerance,
                "{name}: max difference {diff} over {tolerance}"
            );
        }
    }
}
