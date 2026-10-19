//! Face commands that edit dots and the face model: `face.model`, `face.locate` and
//! `face.locate.smartDrag`. (Extract, mix and the library are in `face_media`.)
//!
//! Inbetween does not bundle a face detector. A face is placed from a **face box** (given, or
//! a centred starting box to adjust) or from **landmark positions** that any open landmark
//! model wrote to a `.inbetweenlandmarks` file, in the order of the face model's enabled
//! landmarks. Either way the dots come out in the model's order, so smart drag, mixing and
//! the library all know which dot is which.

use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{DotPair, FaceModel, Sequence, FACE_PRESETS};
use inbetween_geom::Point;
use inbetween_morph::face::{self, FaceBox};
use inbetween_morph::MorphError;
use serde_json::{json, Value};
use std::collections::HashSet;

pub(crate) const SEQUENCE: ParamSpec = ParamSpec::new(
    "sequence",
    ParamKind::Integer,
    false,
    "Sequence number, starting at 0",
)
.range(0.0, 1_000_000.0)
.default_is("0");

pub(crate) const FIRST: ParamSpec = ParamSpec::new(
    "first",
    ParamKind::Integer,
    false,
    "Number of the face's first dot, counting from 0 (a sequence can hold several faces one after another; each takes as many dots as the face model has enabled landmarks)",
)
.range(0.0, 100_000.0)
.default_is("0");

const fn box_param(name: &'static str, description: &'static str) -> ParamSpec {
    ParamSpec::new(name, ParamKind::Number, false, description).range(-10_000_000.0, 10_000_000.0)
}

pub(crate) const BOX_PARAMS: [ParamSpec; 4] = [
    box_param("box_x", "Left edge of the face box, in output pixels"),
    box_param("box_y", "Top edge of the face box, in output pixels"),
    box_param("box_w", "Width of the face box, in output pixels"),
    box_param("box_h", "Height of the face box, in output pixels"),
];

pub(crate) fn morph_error(e: MorphError) -> CommandError {
    CommandError::BadParams(e.to_string())
}

/// The face box in `params` (all four of `box_x`, `box_y`, `box_w`, `box_h`), or `None` when
/// none of them is given.
pub(crate) fn explicit_box(params: &Params) -> Result<Option<FaceBox>, CommandError> {
    let names = ["box_x", "box_y", "box_w", "box_h"];
    let given = names.iter().filter(|n| params.has(n)).count();
    match given {
        0 => Ok(None),
        4 => {
            let n = |name: &str| params.number(name).unwrap_or(0.0);
            FaceBox::new(n("box_x"), n("box_y"), n("box_w"), n("box_h"))
                .map(Some)
                .map_err(morph_error)
        }
        _ => Err(CommandError::BadParams(
            "a face box needs all four of box_x, box_y, box_w and box_h".into(),
        )),
    }
}

/// The positions of one image's copy of a face: `count` dots from `first`.
pub(crate) fn face_points(
    seq: &Sequence,
    first: usize,
    count: usize,
    image: i64,
) -> Result<Vec<Point>, CommandError> {
    let end = first.saturating_add(count);
    let pairs = seq.dots.pairs.get(first..end).ok_or_else(|| {
        CommandError::BadParams(format!(
            "the face model needs {count} dots starting at dot {first}, but this sequence has only {} dots; place a face with face.locate first",
            seq.dots.len()
        ))
    })?;
    Ok(pairs
        .iter()
        .map(|p| if image == 2 { p.second } else { p.first })
        .collect())
}

/// Two dots may not share a spot on one image.
pub(crate) fn check_distinct(seq: &Sequence) -> Result<(), CommandError> {
    for image in [1u8, 2] {
        let mut seen = HashSet::with_capacity(seq.dots.len());
        for (i, p) in seq.dots.pairs.iter().enumerate() {
            let at = if image == 1 { p.first } else { p.second };
            // Compare exact bit patterns (and fold -0.0 into 0.0).
            let key = ((at.x + 0.0).to_bits(), (at.y + 0.0).to_bits());
            if !seen.insert(key) {
                return Err(CommandError::BadParams(format!(
                    "dot {} would share a spot with another dot on Image {image}; move the face a little",
                    i + 1
                )));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- face.model

const MODEL_OPS: &[&str] = &[
    "report",
    "preset",
    "enable",
    "important",
    "move",
    "rename",
    "guide_add",
    "guide_remove",
    "save",
    "load",
];

const MODEL_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "op",
        ParamKind::String,
        false,
        "report (default); preset (switch to the preset named by `preset`); enable / important (set landmark `index` on or off with `on`); move (landmark `index` to `x`, `y` on the unit face box); rename (the model, or landmark `index` when given, to `name`); guide_add (a line from landmark `index` to landmark `to`); guide_remove (guide number `guide`); save / load (the model file at `path`)",
    )
    .one_of(MODEL_OPS),
    ParamSpec::new("preset", ParamKind::String, false, "Preset name, for `preset`")
        .one_of(FACE_PRESETS),
    ParamSpec::new(
        "index",
        ParamKind::Integer,
        false,
        "Landmark number in the model, counting from 0 (all landmarks, enabled or not)",
    )
    .range(0.0, 100_000.0),
    ParamSpec::new("to", ParamKind::Integer, false, "The second landmark of a guide line")
        .range(0.0, 100_000.0),
    ParamSpec::new("guide", ParamKind::Integer, false, "Guide line number, counting from 0")
        .range(0.0, 100_000.0),
    ParamSpec::new("on", ParamKind::Bool, false, "On or off, for `enable` and `important`"),
    ParamSpec::new("name", ParamKind::String, false, "A new name, for `rename`"),
    ParamSpec::new(
        "x",
        ParamKind::Number,
        false,
        "Horizontal position on the unit face box (0 left edge, 1 right edge), for `move`",
    )
    .range(-100.0, 100.0),
    ParamSpec::new(
        "y",
        ParamKind::Number,
        false,
        "Vertical position on the unit face box (0 top edge, 1 bottom edge), for `move`",
    )
    .range(-100.0, 100.0),
    ParamSpec::new(
        "path",
        ParamKind::Path,
        false,
        "A .inbetweenface file, for `save` and `load`",
    )
    .save_file(&["inbetweenface"]),
];

fn landmark_index(params: &Params, model: &FaceModel) -> Result<usize, CommandError> {
    let index = params
        .integer("index")
        .ok_or_else(|| CommandError::BadParams("this operation needs `index`".into()))?;
    usize::try_from(index)
        .ok()
        .filter(|i| *i < model.dots.len())
        .ok_or_else(|| {
            CommandError::BadParams(format!(
                "there is no landmark {index} (the model has {}; numbering starts at 0)",
                model.dots.len()
            ))
        })
}

fn model_report(model: &FaceModel) -> Value {
    json!({
        "name": model.name,
        "landmarks": model.dots.len(),
        "enabled": model.active_count(),
        "dots": model.dots.iter().enumerate().map(|(i, d)| json!({
            "index": i, "name": d.name, "x": d.x, "y": d.y,
            "enabled": d.enabled, "important": d.important,
        })).collect::<Vec<_>>(),
        "guides": model.guides,
    })
}

fn model(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let mut edited = session.project().face.clone();
    let on = || {
        params
            .boolean("on")
            .ok_or_else(|| CommandError::BadParams("this operation needs `on`".into()))
    };
    match params.string("op").unwrap_or("report") {
        "report" => {}
        "preset" => {
            let name = params.string("preset").ok_or_else(|| {
                CommandError::BadParams("`preset` needs the `preset` name".into())
            })?;
            edited = FaceModel::preset(name)
                .ok_or_else(|| CommandError::BadParams(format!("no preset named `{name}`")))?;
        }
        "enable" => {
            let i = landmark_index(params, &edited)?;
            let on = on()?;
            if let Some(d) = edited.dots.get_mut(i) {
                d.enabled = on;
            }
        }
        "important" => {
            let i = landmark_index(params, &edited)?;
            let on = on()?;
            if let Some(d) = edited.dots.get_mut(i) {
                d.important = on;
            }
        }
        "move" => {
            let i = landmark_index(params, &edited)?;
            let (Some(x), Some(y)) = (params.number("x"), params.number("y")) else {
                return Err(CommandError::BadParams("`move` needs `x` and `y`".into()));
            };
            if let Some(d) = edited.dots.get_mut(i) {
                d.x = x;
                d.y = y;
            }
        }
        "rename" => {
            let name = params
                .string("name")
                .ok_or_else(|| CommandError::BadParams("`rename` needs `name`".into()))?
                .to_string();
            if params.has("index") {
                let i = landmark_index(params, &edited)?;
                if let Some(d) = edited.dots.get_mut(i) {
                    d.name = name;
                }
            } else {
                edited.name = name;
            }
        }
        "guide_add" => {
            let a = landmark_index(params, &edited)?;
            let b = params
                .integer("to")
                .and_then(|b| usize::try_from(b).ok())
                .ok_or_else(|| CommandError::BadParams("`guide_add` needs `to`".into()))?;
            edited.guides.push([a, b]);
        }
        "guide_remove" => {
            let g = params
                .integer("guide")
                .and_then(|g| usize::try_from(g).ok())
                .filter(|g| *g < edited.guides.len())
                .ok_or_else(|| {
                    CommandError::BadParams(format!(
                        "there is no guide line with that number (the model has {})",
                        edited.guides.len()
                    ))
                })?;
            edited.guides.remove(g);
        }
        "save" => {
            let path = params
                .string("path")
                .ok_or_else(|| CommandError::BadParams("`save` needs `path`".into()))?;
            save_model(path, &edited)?;
        }
        "load" => {
            let path = params
                .string("path")
                .ok_or_else(|| CommandError::BadParams("`load` needs `path`".into()))?;
            edited = load_model(path)?;
        }
        other => return Err(CommandError::BadParams(format!("unknown op `{other}`"))),
    }
    edited.validate()?;
    session.project_mut().face = edited;
    Ok(model_report(&session.project().face))
}

#[cfg(not(target_arch = "wasm32"))]
fn save_model(path: &str, model: &FaceModel) -> Result<(), CommandError> {
    inbetween_format::write_face_model(std::path::Path::new(path), model)?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn load_model(path: &str) -> Result<FaceModel, CommandError> {
    Ok(inbetween_format::read_face_model(std::path::Path::new(
        path,
    ))?)
}

#[cfg(target_arch = "wasm32")]
fn save_model(_: &str, _: &FaceModel) -> Result<(), CommandError> {
    Err(CommandError::BadParams(
        "model files are not available in this build".into(),
    ))
}

#[cfg(target_arch = "wasm32")]
fn load_model(_: &str) -> Result<FaceModel, CommandError> {
    Err(CommandError::BadParams(
        "model files are not available in this build".into(),
    ))
}

// --------------------------------------------------------------- face.locate

const LOCATE_MODES: &[&str] = &["replace", "append"];

const LOCATE_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new(
        "all_sequences",
        ParamKind::Bool,
        false,
        "Place the face in every sequence instead of just one",
    ),
    ParamSpec::new(
        "image",
        ParamKind::Integer,
        false,
        "Which image's dots to place: 1 or 2",
    )
    .range(1.0, 2.0)
    .default_is("both"),
    ParamSpec::new(
        "mode",
        ParamKind::String,
        false,
        "replace: with both images the dot set becomes the face, with one image only that image's positions of the first dots change; append: add the face after the existing dots (for several faces in one image)",
    )
    .one_of(LOCATE_MODES)
    .default_is("replace"),
    BOX_PARAMS[0],
    BOX_PARAMS[1],
    BOX_PARAMS[2],
    BOX_PARAMS[3],
    ParamSpec::new(
        "landmarks",
        ParamKind::Path,
        false,
        "A .inbetweenlandmarks file with the face's landmark positions in the model's order (from any landmark detector), instead of a box",
    )
    .open_file(&["inbetweenlandmarks"]),
];

#[cfg(not(target_arch = "wasm32"))]
fn read_landmarks(path: &str) -> Result<Vec<Point>, CommandError> {
    Ok(inbetween_format::read_landmarks(std::path::Path::new(
        path,
    ))?)
}

#[cfg(target_arch = "wasm32")]
fn read_landmarks(_: &str) -> Result<Vec<Point>, CommandError> {
    Err(CommandError::BadParams(
        "landmark files are not available in this build".into(),
    ))
}

fn locate(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let targets: Vec<usize> = if params.boolean("all_sequences").unwrap_or(false) {
        (0..session.project().sequences.len()).collect()
    } else {
        vec![session.sequence_index(params)?]
    };
    let model = session.project().face.clone();
    let count = model.active_count();
    if count == 0 {
        return Err(CommandError::BadParams(
            "the face model has no enabled landmarks; enable some with face.model".into(),
        ));
    }
    let output = session.project().output;
    let placed = match params.string("landmarks") {
        Some(path) => {
            if explicit_box(params)?.is_some() {
                return Err(CommandError::BadParams(
                    "give a face box or a landmarks file, not both".into(),
                ));
            }
            let points = read_landmarks(path)?;
            if points.len() != count {
                return Err(CommandError::BadParams(format!(
                    "the landmarks file has {} positions but the face model has {count} enabled landmarks",
                    points.len()
                )));
            }
            points
        }
        None => {
            let face_box = match explicit_box(params)? {
                Some(b) => b,
                None => FaceBox::centered(output.width, output.height).map_err(morph_error)?,
            };
            face::place(&model, &face_box)
        }
    };
    let image = params.integer("image");
    let append = params.string("mode") == Some("append");
    for &index in &targets {
        let seq = session.sequence_mut(index)?;
        if append {
            seq.dots
                .pairs
                .extend(placed.iter().map(|p| DotPair::fixed(*p)));
        } else if image.is_none() {
            seq.dots.pairs = placed.iter().map(|p| DotPair::fixed(*p)).collect();
        } else {
            for (k, p) in placed.iter().enumerate() {
                match seq.dots.pairs.get_mut(k) {
                    Some(pair) if image == Some(1) => pair.first = *p,
                    Some(pair) => pair.second = *p,
                    None => seq.dots.pairs.push(DotPair::fixed(*p)),
                }
            }
        }
        seq.validate()?;
        check_distinct(seq)?;
    }
    Ok(json!({
        "sequences": targets,
        "dots_per_face": count,
        "dots": targets.iter().map(|&i| session.project().sequences.get(i).map_or(0, |s| s.dots.len())).collect::<Vec<_>>(),
    }))
}

// ------------------------------------------------------ face.locate.smartDrag

const DRAG_PARAMS: &[ParamSpec] = &[
    ParamSpec::new("index", ParamKind::Integer, true, "Which dot to drag, counting from 0")
        .range(0.0, 100_000.0),
    ParamSpec::new(
        "image",
        ParamKind::Integer,
        true,
        "Which image's copy of the face to drag: 1 or 2",
    )
    .range(1.0, 2.0),
    ParamSpec::new("x", ParamKind::Number, true, "New horizontal position, in output pixels")
        .range(-10_000_000.0, 10_000_000.0),
    ParamSpec::new("y", ParamKind::Number, true, "New vertical position, in output pixels")
        .range(-10_000_000.0, 10_000_000.0),
    ParamSpec::new(
        "radius",
        ParamKind::Number,
        false,
        "How far the pull reaches, in face-box widths: dots on the face model nearer than this follow the dragged dot",
    )
    .range(0.01, 10.0)
    .default_is("0.35"),
    FIRST,
    SEQUENCE,
];

fn smart_drag(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let seq_index = session.sequence_index(params)?;
    let model = session.project().face.clone();
    let count = model.active_count();
    let image = params.integer("image").unwrap_or(1);
    let first = params
        .integer("first")
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0);
    let index = params
        .integer("index")
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(usize::MAX);
    let local = index
        .checked_sub(first)
        .filter(|l| *l < count)
        .ok_or_else(|| {
            CommandError::BadParams(format!(
                "dot {index} is not part of the face that starts at dot {first} ({count} dots); smart drag moves only face dots"
            ))
        })?;
    let to = Point::new(
        params.number("x").unwrap_or(0.0),
        params.number("y").unwrap_or(0.0),
    );
    let seq = session.sequence_mut(seq_index)?;
    let points = face_points(seq, first, count, image)?;
    let moved = face::smart_drag(
        &model,
        &points,
        local,
        to,
        params.number("radius").unwrap_or(0.35),
    )
    .map_err(morph_error)?;
    for (pair, p) in seq.dots.pairs.iter_mut().skip(first).zip(&moved) {
        if image == 2 {
            pair.second = *p;
        } else {
            pair.first = *p;
        }
    }
    seq.validate()?;
    check_distinct(seq)?;
    let changed = moved.iter().zip(&points).filter(|(a, b)| a != b).count();
    Ok(json!({ "sequence": seq_index, "image": image, "moved": changed }))
}

fn has_sequence(session: &Session) -> bool {
    session.has_sequences()
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "face.model",
            label: "Face Model",
            menu: "Face/Model...",
            shortcut: None,
            description: "Edit or report Inbetween's face dot model: which landmarks a face has, which are enabled or important, their place on the unit face box and the guide lines between them. Presets (standard, minimal) and model files (.inbetweenface) included.",
            params: MODEL_PARAMS,
            enabled: |_| true,
            disabled_reason: "",
            edits: true,
            run: model,
        },
        CommandSpec {
            id: "face.locate",
            label: "Locate Face",
            menu: "Face/Locate Face...",
            shortcut: None,
            description: "Place the face model's dots on a face: in a face box you give (or a centred starting box), or at landmark positions a detector wrote to a .inbetweenlandmarks file. Replace the sequence's dots or append another face; apply to one sequence or all. Inbetween does not detect faces itself.",
            params: LOCATE_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence to put a face in",
            edits: true,
            run: locate,
        },
        CommandSpec {
            id: "face.locate.smartDrag",
            label: "Smart Drag Face Dot",
            menu: "",
            shortcut: None,
            description: "Move one face dot on one image and let the nearby face dots follow (important landmarks follow less), so a face can be fitted in a few drags. Plain `dots.move` moves a single dot.",
            params: DRAG_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no face dots to drag",
            edits: true,
            run: smart_drag,
        },
    ]
}
