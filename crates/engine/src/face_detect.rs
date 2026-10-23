//! Automatic face finding: the bundled detector behind `face.detect`, `face.locate` (source
//! `detect`) and `face.extract`.
//!
//! The detector runs on each image's processed source (turns, crops and adjustments
//! applied), and its points are mapped from source pixels to output pixels through the
//! sequence's fit, so they sit exactly where dots do.

use crate::face_cmds::SEQUENCE;
use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_geom::Point;
use serde_json::{json, Value};

/// Whether this build carries the face detector.
pub(crate) const AVAILABLE: bool = cfg!(feature = "face-detect");

/// One face found in an image, in output pixels.
pub(crate) struct Found {
    pub score: f32,
    /// Left, top, width and height of the face.
    pub bounds: [f64; 4],
    /// One position per enabled landmark of the face model, in the model's order.
    pub landmarks: Vec<Point>,
    /// All 468 mesh points.
    pub mesh: Vec<Point>,
}

#[cfg(feature = "face-detect")]
mod imp {
    use super::*;
    use inbetween_detect::{mesh_map, Detector, Face};
    use inbetween_morph::fit_transform;
    use inbetween_raster::Image;
    use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

    /// Detections kept so repeating a command (after undo, say) runs no inference.
    const CACHE_SIZE: usize = 8;
    type Entry = (Weak<Image>, Arc<Vec<Face>>);
    static CACHE: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

    fn detector() -> Result<&'static Detector, CommandError> {
        static DETECTOR: OnceLock<Result<Detector, String>> = OnceLock::new();
        match DETECTOR.get_or_init(|| Detector::new().map_err(|e| e.to_string())) {
            Ok(d) => Ok(d),
            Err(e) => Err(CommandError::Internal(e.clone())),
        }
    }

    fn faces_of(img: &Arc<Image>) -> Result<Arc<Vec<Face>>, CommandError> {
        {
            let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
            cache.retain(|(w, _)| w.strong_count() > 0);
            let hit = cache
                .iter()
                .find(|(w, _)| w.upgrade().is_some_and(|i| Arc::ptr_eq(&i, img)));
            if let Some((_, faces)) = hit {
                return Ok(Arc::clone(faces));
            }
        }
        let faces = Arc::new(
            detector()?
                .faces(img)
                .map_err(|e| CommandError::BadParams(e.to_string()))?,
        );
        let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
        if cache.len() >= CACHE_SIZE {
            cache.remove(0);
        }
        cache.push((Arc::downgrade(img), Arc::clone(&faces)));
        Ok(faces)
    }

    pub(crate) fn find(
        session: &mut Session,
        seq: usize,
        which: u8,
    ) -> Result<Vec<Found>, CommandError> {
        let source = session.source(seq, which)?;
        let faces = faces_of(&source)?;
        let model = session.project().face.clone();
        let output = session.project().output;
        let fit = session.sequence(seq)?.fit;
        let to_output = fit_transform(
            source.width(),
            source.height(),
            output.width,
            output.height,
            fit,
        )
        .map_err(crate::face_cmds::morph_error)?
        .inverse()
        .map_err(|e| CommandError::BadParams(e.to_string()))?;
        let names: Vec<(&str, f64, f64)> = model
            .active()
            .iter()
            .filter_map(|&i| model.dots.get(i))
            .map(|d| (d.name.as_str(), d.x, d.y))
            .collect();
        let mut out = Vec::new();
        for face in faces.iter() {
            let Some(placed) = mesh_map::place(&names, &face.mesh) else {
                return Err(CommandError::BadParams(
                    "the face model's landmarks are not known to the detector and too few match to fit them; use the standard model or a box".into(),
                ));
            };
            let map = |pts: &[Point]| pts.iter().map(|p| to_output.apply(*p)).collect::<Vec<_>>();
            let (a, b) = (
                to_output.apply(Point::new(face.bounds.x0, face.bounds.y0)),
                to_output.apply(Point::new(face.bounds.x1, face.bounds.y1)),
            );
            out.push(Found {
                score: face.score,
                bounds: [
                    a.x.min(b.x),
                    a.y.min(b.y),
                    (b.x - a.x).abs(),
                    (b.y - a.y).abs(),
                ],
                landmarks: map(&placed),
                mesh: map(&face.mesh),
            });
        }
        Ok(out)
    }

    /// Place the model's landmarks on a mesh given as 468 points (the detector's layout) or
    /// 478 (with iris centres at 468 and 473, which then place the pupils).
    pub(crate) fn from_mesh(
        points: &[Point],
        model: &inbetween_doc::FaceModel,
    ) -> Result<Vec<Point>, CommandError> {
        if points.len() != 468 && points.len() != 478 {
            return Err(CommandError::BadParams(format!(
                "a mesh478 landmarks file has 468 or 478 positions, not {}",
                points.len()
            )));
        }
        let active = model.active();
        let names: Vec<(&str, f64, f64)> = active
            .iter()
            .filter_map(|&i| model.dots.get(i))
            .map(|d| (d.name.as_str(), d.x, d.y))
            .collect();
        let mut placed = mesh_map::place(&names, points).ok_or_else(|| {
            CommandError::BadParams(
                "the landmarks do not fit the face model (too few known landmark names)".into(),
            )
        })?;
        if points.len() == 478 {
            for ((name, _, _), p) in names.iter().zip(placed.iter_mut()) {
                let iris = match *name {
                    "pupil_right" => points.get(468),
                    "pupil_left" => points.get(473),
                    _ => None,
                };
                if let Some(q) = iris {
                    *p = *q;
                }
            }
        }
        Ok(placed)
    }
}

#[cfg(not(feature = "face-detect"))]
mod imp {
    use super::*;

    fn missing() -> CommandError {
        CommandError::BadParams(
            "this build of Inbetween does not include the face detector; give a face box or a landmarks file".into(),
        )
    }

    pub(crate) fn find(_: &mut Session, _: usize, _: u8) -> Result<Vec<Found>, CommandError> {
        Err(missing())
    }

    pub(crate) fn from_mesh(
        _: &[Point],
        _: &inbetween_doc::FaceModel,
    ) -> Result<Vec<Point>, CommandError> {
        Err(missing())
    }
}

pub(crate) use imp::{find, from_mesh};

fn no_face(which: u8, seq: usize) -> CommandError {
    CommandError::BadParams(format!(
        "no face found in image {which} of sequence {seq}; give a box (box_x, box_y, box_w, box_h) or a landmarks file"
    ))
}

/// Which faces to place: one (by number, best first) or all, left to right.
#[derive(Clone, Copy)]
pub(crate) enum Pick {
    One(usize),
    All,
}

/// The landmark positions of the chosen faces, one run per face, for each wanted image
/// (`image` 1 or 2, or both when `None`). The two images' lists are in the same order and
/// of the same length when both are wanted.
pub(crate) fn runs(
    session: &mut Session,
    seq: usize,
    image: Option<i64>,
    pick: Pick,
) -> Result<[Vec<Vec<Point>>; 2], CommandError> {
    let mut out: [Vec<Vec<Point>>; 2] = [Vec::new(), Vec::new()];
    for which in [1u8, 2] {
        if image.is_some_and(|i| i != i64::from(which)) {
            continue;
        }
        let mut found = find(session, seq, which)?;
        if found.is_empty() {
            return Err(no_face(which, seq));
        }
        let chosen = match pick {
            Pick::One(n) => {
                let total = found.len();
                if n >= total {
                    return Err(CommandError::BadParams(format!(
                        "image {which} of sequence {seq} has {total} face(s), so face {n} does not exist (numbering starts at 0, best first)"
                    )));
                }
                vec![found.swap_remove(n)]
            }
            Pick::All => {
                found.sort_by(|a, b| a.bounds[0].total_cmp(&b.bounds[0]));
                found
            }
        };
        if let Some(slot) = out.get_mut(usize::from(which) - 1) {
            *slot = chosen.into_iter().map(|f| f.landmarks).collect();
        }
    }
    if image.is_none() && out[0].len() != out[1].len() {
        return Err(CommandError::BadParams(format!(
            "image 1 of sequence {seq} has {} face(s) but image 2 has {}; place them one image at a time with `image`, or give a box",
            out[0].len(),
            out[1].len()
        )));
    }
    Ok(out)
}

// --------------------------------------------------------------- face.detect

const DETECT_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new(
        "image",
        ParamKind::Integer,
        false,
        "Which image to look at: 1 or 2",
    )
    .range(1.0, 2.0)
    .default_is("both"),
    ParamSpec::new(
        "mesh",
        ParamKind::Bool,
        false,
        "Also report all 468 mesh points of each face",
    ),
];

fn xy(p: &Point) -> Value {
    json!([p.x, p.y])
}

fn detect(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let seq = session.sequence_index(params)?;
    let image = params.integer("image");
    let with_mesh = params.boolean("mesh").unwrap_or(false);
    let model = session.project().face.clone();
    let names: Vec<String> = model
        .active()
        .iter()
        .filter_map(|&i| model.dots.get(i))
        .map(|d| d.name.clone())
        .collect();
    let mut images = Vec::new();
    for which in [1u8, 2] {
        if image.is_some_and(|i| i != i64::from(which)) {
            continue;
        }
        let faces: Vec<Value> = find(session, seq, which)?
            .iter()
            .map(|f| {
                let mut v = json!({
                    "score": f.score,
                    "box": { "x": f.bounds[0], "y": f.bounds[1], "w": f.bounds[2], "h": f.bounds[3] },
                    "landmarks": names.iter().zip(&f.landmarks)
                        .map(|(n, p)| json!({ "name": n, "x": p.x, "y": p.y }))
                        .collect::<Vec<_>>(),
                });
                if with_mesh {
                    v["mesh"] = Value::Array(f.mesh.iter().map(xy).collect());
                }
                v
            })
            .collect();
        images.push(json!({ "image": which, "faces": faces }));
    }
    Ok(json!({ "sequence": seq, "images": images }))
}

// ----------------------------------------------------------- face.locateAll

const LOCATE_ALL_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new(
        "mode",
        ParamKind::String,
        false,
        "replace: the sequence's dots become the faces found; append: add them after the existing dots",
    )
    .one_of(crate::face_cmds::LOCATE_MODES)
    .default_is("replace"),
];

fn locate_all(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let mut p = serde_json::Map::new();
    p.insert("source".into(), json!("detect"));
    p.insert("all_faces".into(), json!(true));
    if let Some(n) = params.integer("sequence") {
        p.insert("sequence".into(), json!(n));
    }
    if let Some(m) = params.string("mode") {
        p.insert("mode".into(), json!(m));
    }
    let params = Params::parse(crate::face_cmds::LOCATE_PARAMS, &Value::Object(p))?;
    crate::face_cmds::locate(session, &params)
}

const LOCATE_SEQUENCES_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "mode",
        ParamKind::String,
        false,
        "replace: each sequence's dots become the face found; append: add it after the existing dots",
    )
    .one_of(crate::face_cmds::LOCATE_MODES)
    .default_is("replace"),
    ParamSpec::new(
        "all_faces",
        ParamKind::Bool,
        false,
        "Place every face found, left to right, instead of the best one (both images need the same number of faces)",
    ),
];

fn locate_sequences(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let mut p = serde_json::Map::new();
    p.insert("source".into(), json!("detect"));
    p.insert("all_sequences".into(), json!(true));
    if let Some(m) = params.string("mode") {
        p.insert("mode".into(), json!(m));
    }
    if let Some(a) = params.boolean("all_faces") {
        p.insert("all_faces".into(), json!(a));
    }
    let params = Params::parse(crate::face_cmds::LOCATE_PARAMS, &Value::Object(p))?;
    crate::face_cmds::locate(session, &params)
}

fn has_sequence(session: &Session) -> bool {
    session.has_sequences()
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "face.locateSequences",
            label: "Mark Face in All Sequences",
            menu: "Face/Mark Face in All Sequences",
            shortcut: None,
            description: "Put dots on the face in both images of every sequence. A sequence without a usable face is skipped and listed under `skipped`; it is an error only if every sequence fails (face.locate with all_sequences).",
            params: LOCATE_SEQUENCES_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence to put a face in",
            edits: true,
            run: locate_sequences,
        },
        CommandSpec {
            id: "face.locateAll",
            label: "Mark Several Faces",
            menu: "Face/Mark Several Faces",
            shortcut: None,
            description: "Put dots on every face in both images, left to right; both images need the same number of faces. Uses the bundled detector and places the face model on each (face.locate with all_faces).",
            params: LOCATE_ALL_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence to put faces in",
            edits: true,
            run: locate_all,
        },
        CommandSpec {
        id: "face.detect",
        label: "Detect Faces",
        menu: "",
        shortcut: None,
        description: "Report the faces (command line and MCP only; not in the menu). Inbetween's bundled detector finds in a sequence's images: a score, the face box and the face model's landmark positions for each face, in output pixels (so they are the positions face.locate would place dots at). `mesh` adds the full 468-point mesh. Nothing is changed.",
        params: DETECT_PARAMS,
        enabled: has_sequence,
        disabled_reason: "the project has no sequence to look at",
        edits: false,
        run: detect,
    }]
}
