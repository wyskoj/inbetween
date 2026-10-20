//! Camera commands: `camera.keyframes` (a sequence's camera) and `effect.camera` (the camera of
//! one still-image effect). (The camera's easing curve is `camera.curve`, in `curve_cmds`.)
//!
//! A camera moves a rendered picture over time: offset, scale (negative flips) and rotation,
//! as keyframes at morph times from 0 to 1. Frames between keyframes ease smoothly. A
//! sequence's camera moves the whole composite; an effect's camera moves only that effect's
//! picture, which animates a still image.

use crate::effect_cmds::{target as effect_target, target_effects, Target, SCOPE};
use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{Camera, CameraAspect, CameraKey, CAMERA_EFFECTS};
use serde_json::{json, Value};

const OPS: &[&str] = &[
    "set",
    "remove",
    "reset",
    "apply_all",
    "copy",
    "paste",
    "save",
    "load",
];
const ASPECTS: &[&str] = &["free", "movie", "locked", "image1", "image2"];

/// The parameters both camera commands share, followed by `$extra`.
macro_rules! camera_params {
    ($($extra:expr),*) => {
        &[
            ParamSpec::new(
                "sequence",
                ParamKind::Integer,
                false,
                "Sequence number, starting at 0",
            )
            .range(0.0, 1_000_000.0)
            .default_is("0"),
            ParamSpec::new(
                "op",
                ParamKind::String,
                false,
                "set (a keyframe at `t`; values you leave out keep what the camera shows there), remove (keyframe `index`), reset (no keyframes), apply_all (copy this camera to every sequence), copy and paste (through a clipboard that holds one camera), save and load (a .inbetweencamera file at `path`); omit to just report the camera",
            )
            .one_of(OPS),
            ParamSpec::new("t", ParamKind::Number, false, "Morph time of the keyframe, 0 to 1")
                .range(0.0, 1.0),
            ParamSpec::new(
                "x",
                ParamKind::Number,
                false,
                "Horizontal offset in frame widths (0.1 moves the picture right by a tenth of the frame)",
            )
            .range(-100.0, 100.0),
            ParamSpec::new(
                "y",
                ParamKind::Number,
                false,
                "Vertical offset in frame heights (positive moves the picture down)",
            )
            .range(-100.0, 100.0),
            ParamSpec::new(
                "scale_x",
                ParamKind::Number,
                false,
                "Horizontal scale about the frame centre, 0.01 to 100 in size; negative flips left to right",
            )
            .range(-100.0, 100.0),
            ParamSpec::new(
                "scale_y",
                ParamKind::Number,
                false,
                "Vertical scale about the frame centre, 0.01 to 100 in size; negative flips top to bottom",
            )
            .range(-100.0, 100.0),
            ParamSpec::new(
                "rotation",
                ParamKind::Number,
                false,
                "Rotation in degrees about the frame centre, clockwise",
            )
            .range(-360_000.0, 360_000.0),
            ParamSpec::new(
                "index",
                ParamKind::Integer,
                false,
                "Which keyframe, counting from 0 (for `remove`)",
            )
            .range(0.0, 64.0),
            ParamSpec::new(
                "enabled",
                ParamKind::Bool,
                false,
                "Switch the camera on or off; setting a keyframe switches it on",
            ),
            ParamSpec::new(
                "aspect",
                ParamKind::String,
                false,
                "Keeps the camera frame's proportions when you set one scale: free (the scales are independent), movie (the frame is the movie's shape, so both scales match), image1 or image2 (the frame is shaped like that image)",
            )
            .one_of(ASPECTS),
            ParamSpec::new(
                "path",
                ParamKind::Path,
                false,
                "The .inbetweencamera file for `save` and `load`",
            )
            .save_file(&["inbetweencamera"]),
            $($extra),*
        ]
    };
}

const PARAMS: &[ParamSpec] = camera_params!();

const EFFECT: ParamSpec = ParamSpec::new(
    "effect",
    ParamKind::String,
    true,
    "Which still-image effect the camera moves",
)
.one_of(&CAMERA_EFFECTS);

const EFFECT_PARAMS: &[ParamSpec] = camera_params!(EFFECT, SCOPE);

fn key_json(k: &CameraKey) -> Value {
    json!({
        "t": k.t, "x": k.x, "y": k.y,
        "scale_x": k.scale_x, "scale_y": k.scale_y, "rotation": k.rotation,
    })
}

fn report(c: &Camera) -> Value {
    json!({
        "enabled": c.enabled,
        "active": c.is_active(),
        "aspect": c.aspect.id(),
        "easing_identity": c.easing.is_identity(),
        "keyframes": c.keys.iter().map(key_json).collect::<Vec<_>>(),
    })
}

/// Build the keyframe a `set` describes: omitted values come from the pose already shown at
/// `t`. `ratio` is the width-to-height ratio of the two scales the aspect lock keeps (1 for the
/// movie's shape), or `None` when the scales are free.
fn keyframe(
    camera: &Camera,
    params: &Params,
    ratio: Option<f64>,
) -> Result<CameraKey, CommandError> {
    let t = params
        .number("t")
        .ok_or_else(|| CommandError::BadParams("`set` needs the `t` parameter".into()))?;
    // The pose before any easing, so a tweak of one value does not bake the easing in.
    let shown = Camera {
        easing: inbetween_doc::Curve::default(),
        ..camera.clone()
    }
    .pose(t);
    let mut k = CameraKey {
        t,
        x: params.number("x").unwrap_or(shown.x),
        y: params.number("y").unwrap_or(shown.y),
        scale_x: params.number("scale_x").unwrap_or(shown.scale_x),
        scale_y: params.number("scale_y").unwrap_or(shown.scale_y),
        rotation: params.number("rotation").unwrap_or(shown.rotation),
    };
    if let Some(r) = ratio.filter(|r| r.is_finite() && *r > 0.0) {
        // The scale that was given drives the other; the other keeps its own flip.
        match (params.number("scale_x"), params.number("scale_y")) {
            (Some(_), None) => k.scale_y = (k.scale_x.abs() / r).copysign(shown.scale_y),
            (None, Some(_)) => k.scale_x = (k.scale_y.abs() * r).copysign(shown.scale_x),
            _ => {}
        }
    }
    Ok(k)
}

/// What a camera command edits.
#[derive(Clone, Copy)]
enum Subject {
    Sequence(usize),
    Effect(Target, &'static str),
}

impl Subject {
    fn camera(self, session: &mut Session) -> Result<&mut Camera, CommandError> {
        match self {
            Self::Sequence(i) => Ok(&mut session.sequence_mut(i)?.camera),
            Self::Effect(target, name) => target_effects(session, &target)?
                .cameras
                .get_mut(name)
                .ok_or_else(|| CommandError::BadParams(format!("no effect called `{name}`"))),
        }
    }

    /// The sequence whose images give `image1` and `image2` their shape.
    fn sequence(self) -> Option<usize> {
        match self {
            Self::Sequence(i) => Some(i),
            Self::Effect(target, _) => target.index(),
        }
    }
}

/// The size of image `which` of sequence `index` as the morph sees it (edits applied).
#[cfg(not(target_arch = "wasm32"))]
fn image_size(session: &mut Session, index: usize, which: u8) -> Result<(u32, u32), CommandError> {
    Ok(session.source(index, which)?.size())
}

#[cfg(target_arch = "wasm32")]
fn image_size(session: &mut Session, index: usize, which: u8) -> Result<(u32, u32), CommandError> {
    crate::image_cmds::source_size(session, index, which)
}

/// The ratio of the two scales that keeps the camera frame shaped like `aspect` says.
fn lock_ratio(
    session: &mut Session,
    subject: Subject,
    aspect: CameraAspect,
) -> Result<Option<f64>, CommandError> {
    let which = match aspect {
        CameraAspect::Free => return Ok(None),
        CameraAspect::Locked => return Ok(Some(1.0)),
        CameraAspect::Image1 => 1,
        CameraAspect::Image2 => 2,
    };
    let Some(index) = subject.sequence() else {
        return Err(CommandError::BadParams(
            "`aspect` image1 and image2 need a sequence; the movie has no images of its own".into(),
        ));
    };
    let (iw, ih) = image_size(session, index, which)?;
    let out = session.project().output;
    let (iw, ih) = (f64::from(iw), f64::from(ih));
    let (w, h) = (f64::from(out.width), f64::from(out.height));
    // The frame is (|sx| W) by (|sy| H) pixels; it has the image's shape when that is iw : ih.
    Ok(Some(iw / ih.max(1.0) * h / w.max(1.0)))
}

fn edit(session: &mut Session, params: &Params, subject: Subject) -> Result<Value, CommandError> {
    if let Some(aspect) = params.string("aspect").and_then(CameraAspect::from_id) {
        subject.camera(session)?.aspect = aspect;
    }
    let aspect = subject.camera(session)?.aspect;
    match params.string("op") {
        None => {}
        Some("set") => {
            let ratio = lock_ratio(session, subject, aspect)?;
            let camera = subject.camera(session)?;
            let key = keyframe(camera, params, ratio)?;
            camera.set_key(key)?;
            camera.enabled = true;
        }
        Some("remove") => {
            let i = params
                .integer("index")
                .map(|n| n.max(0) as usize)
                .ok_or_else(|| {
                    CommandError::BadParams("`remove` needs the `index` parameter".into())
                })?;
            subject.camera(session)?.remove_key(i)?;
        }
        Some("reset") => {
            let camera = subject.camera(session)?;
            *camera = Camera {
                aspect: camera.aspect,
                ..Camera::default()
            };
        }
        Some("apply_all") => {
            let camera = subject.camera(session)?.clone();
            let name = match subject {
                Subject::Sequence(_) => None,
                Subject::Effect(_, name) => Some(name),
            };
            for seq in &mut session.project_mut().sequences {
                match name.and_then(|n| seq.effects.cameras.get_mut(n)) {
                    Some(slot) => *slot = camera.clone(),
                    None if name.is_none() => seq.camera = camera.clone(),
                    None => {}
                }
            }
        }
        Some("copy") => {
            let camera = subject.camera(session)?.clone();
            session.set_camera_clip(camera);
        }
        Some("paste") => {
            let Some(clip) = session.camera_clip().cloned() else {
                return Err(CommandError::BadParams(
                    "there is no copied camera to paste; use `op` copy first".into(),
                ));
            };
            *subject.camera(session)? = clip;
        }
        Some(op @ ("save" | "load")) => file_op(session, params, subject, op)?,
        Some(other) => return Err(CommandError::BadParams(format!("unknown op `{other}`"))),
    }
    let camera = subject.camera(session)?;
    if let Some(on) = params.boolean("enabled") {
        camera.enabled = on;
    }
    camera.validate()?;
    let mut out = report(camera);
    if let Some(o) = out.as_object_mut() {
        match subject {
            Subject::Sequence(i) => {
                o.insert("sequence".into(), json!(i));
            }
            Subject::Effect(target, name) => {
                o.insert("sequence".into(), json!(target.index()));
                o.insert("scope".into(), json!(target.name()));
                o.insert("effect".into(), json!(name));
            }
        }
    }
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn file_op(
    session: &mut Session,
    params: &Params,
    subject: Subject,
    op: &str,
) -> Result<(), CommandError> {
    let path = params
        .string("path")
        .ok_or_else(|| CommandError::BadParams(format!("`{op}` needs the `path` parameter")))?;
    let path = std::path::Path::new(path);
    if op == "save" {
        let camera = subject.camera(session)?.clone();
        inbetween_format::write_camera(path, &camera)?;
    } else {
        let loaded = inbetween_format::read_camera(path)?;
        *subject.camera(session)? = loaded;
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn file_op(_: &mut Session, _: &Params, _: Subject, op: &str) -> Result<(), CommandError> {
    Err(CommandError::BadParams(format!(
        "`{op}` reads or writes a file, which this build cannot do"
    )))
}

fn run(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    edit(session, params, Subject::Sequence(index))
}

fn effect_camera(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let name = params.string("effect").unwrap_or_default();
    let Some(name) = CAMERA_EFFECTS.iter().copied().find(|n| *n == name) else {
        return Err(CommandError::BadParams(format!(
            "`effect` must be one of {}",
            CAMERA_EFFECTS.join(", ")
        )));
    };
    let target = effect_target(session, params)?;
    if target == Target::Movie && name == "transition" {
        return Err(CommandError::BadParams(
            "a transition works on a sequence, not on the whole movie".into(),
        ));
    }
    edit(session, params, Subject::Effect(target, name))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "camera.keyframes",
            label: "Camera",
            menu: "Sequence/Camera...",
            shortcut: Some("Shift+Ctrl+C"),
            description: "Edit (or report) the sequence's camera: keyframes of offset, scale and rotation over morph time that move the rendered morph. Frames between keyframes ease smoothly; uncovered areas are transparent. Also copy, paste, save, load, reset and apply to all sequences.",
            params: PARAMS,
            enabled: Session::has_sequences,
            disabled_reason: "the project has no sequences",
            edits: true,
            run,
        },
        CommandSpec {
            id: "effect.camera",
            label: "Effect Camera",
            menu: "Sequence/Effect Camera...",
            shortcut: None,
            description: "Edit (or report) the camera of one still-image effect (background, mask, foreground, transition or light): the same keyframes as the sequence camera, but moving only that effect's picture. A moving mask or light sweeps over a still morph. Takes `scope` movie for movie-wide effects.",
            params: EFFECT_PARAMS,
            enabled: Session::has_sequences,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: effect_camera,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Registry;
    use inbetween_doc::{Project, Sequence};

    fn session() -> Session {
        let mut s = Session::new();
        s.set_project(
            Project {
                sequences: vec![Sequence::default(), Sequence::default()],
                ..Project::default()
            },
            None,
        );
        s
    }

    fn go(s: &mut Session, p: Value) -> Result<Value, CommandError> {
        Registry::standard().run(s, "camera.keyframes", &p)
    }

    fn fx(s: &mut Session, p: Value) -> Result<Value, CommandError> {
        Registry::standard().run(s, "effect.camera", &p)
    }

    #[test]
    fn keyframes_are_set_reported_and_removed() {
        let mut s = session();
        let v = go(&mut s, json!({})).unwrap();
        assert_eq!(v["enabled"], false);
        assert_eq!(v["keyframes"], json!([]));
        let v = go(
            &mut s,
            json!({"op": "set", "t": 1.0, "x": 0.25, "rotation": 90}),
        )
        .unwrap();
        assert_eq!(v["enabled"], true);
        assert_eq!(v["active"], true);
        assert_eq!(v["keyframes"][0]["x"], 0.25);
        // Values left out keep what the camera shows at that time.
        let v = go(&mut s, json!({"op": "set", "t": 0.5})).unwrap();
        assert_eq!(v["keyframes"][0]["t"], 0.5);
        assert_eq!(v["keyframes"][0]["x"], 0.25);
        assert_eq!(v["keyframes"].as_array().map(Vec::len), Some(2));
        let v = go(&mut s, json!({"op": "remove", "index": 0})).unwrap();
        assert_eq!(v["keyframes"].as_array().map(Vec::len), Some(1));
        assert!(go(&mut s, json!({"op": "remove", "index": 9})).is_err());
        assert!(go(&mut s, json!({"op": "remove"})).is_err());
        assert!(go(&mut s, json!({"op": "set"})).is_err());
        let v = go(&mut s, json!({"op": "reset"})).unwrap();
        assert_eq!(v["keyframes"], json!([]));
    }

    #[test]
    fn bad_keyframes_are_refused_and_leave_the_camera_alone() {
        let mut s = session();
        go(&mut s, json!({"op": "set", "t": 0.5, "x": 1})).unwrap();
        for bad in [
            json!({"op": "set", "t": 0.5, "scale_x": 0.0}),
            json!({"op": "set", "t": 0.5, "scale_y": 0.001}),
            json!({"op": "set", "t": 0.5, "scale_x": 500}),
        ] {
            assert!(go(&mut s, bad).is_err());
        }
        let v = go(&mut s, json!({})).unwrap();
        assert_eq!(v["keyframes"][0]["x"], 1.0);
        assert_eq!(v["keyframes"][0]["scale_x"], 1.0);
    }

    #[test]
    fn movie_aspect_makes_the_scales_follow_each_other() {
        let mut s = session();
        go(&mut s, json!({"aspect": "movie"})).unwrap();
        let v = go(&mut s, json!({"op": "set", "t": 1.0, "scale_x": 2.0})).unwrap();
        assert_eq!(v["keyframes"][0]["scale_y"], 2.0);
        assert_eq!(v["aspect"], "locked");
        let v = go(&mut s, json!({"op": "set", "t": 1.0, "scale_y": -3.0})).unwrap();
        assert_eq!(v["keyframes"][0]["scale_x"], 3.0);
        assert_eq!(v["keyframes"][0]["scale_y"], -3.0);
        go(&mut s, json!({"aspect": "free"})).unwrap();
        let v = go(&mut s, json!({"op": "set", "t": 1.0, "scale_x": 1.0})).unwrap();
        assert_eq!(v["keyframes"][0]["scale_y"], -3.0);
    }

    #[test]
    fn apply_all_and_enabled_toggle() {
        let mut s = session();
        go(&mut s, json!({"op": "set", "t": 1.0, "y": 0.5})).unwrap();
        go(&mut s, json!({"op": "apply_all"})).unwrap();
        let other = go(&mut s, json!({"sequence": 1})).unwrap();
        assert_eq!(other["keyframes"][0]["y"], 0.5);
        let off = go(&mut s, json!({"enabled": false})).unwrap();
        assert_eq!(off["active"], false);
        assert_eq!(off["keyframes"].as_array().map(Vec::len), Some(1));
        assert!(go(&mut s, json!({"sequence": 7})).is_err());
    }

    #[test]
    fn edits_undo() {
        let mut s = session();
        go(&mut s, json!({"op": "set", "t": 1.0, "x": 0.5})).unwrap();
        assert!(s.can_undo());
        Registry::standard()
            .run(&mut s, "edit.undo", &json!({}))
            .unwrap();
        assert!(s.project().sequences[0].camera.keys.is_empty());
    }

    #[test]
    fn copy_and_paste_move_a_camera_between_sequences() {
        let mut s = session();
        assert!(go(&mut s, json!({"op": "paste"})).is_err());
        go(&mut s, json!({"op": "set", "t": 1.0, "rotation": 30})).unwrap();
        go(&mut s, json!({"op": "copy"})).unwrap();
        let v = go(&mut s, json!({"sequence": 1, "op": "paste"})).unwrap();
        assert_eq!(v["keyframes"][0]["rotation"], 30.0);
        assert_eq!(v["active"], true);
    }

    #[test]
    fn save_and_load_round_trip_through_a_file() {
        let dir = std::env::temp_dir().join(format!("inbetween-cam-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pan.inbetweencamera").display().to_string();
        let mut s = session();
        go(&mut s, json!({"op": "set", "t": 0.0})).unwrap();
        go(
            &mut s,
            json!({"op": "set", "t": 1.0, "x": 0.4, "scale_x": -2.0}),
        )
        .unwrap();
        go(&mut s, json!({"op": "save", "path": path})).unwrap();
        let v = go(&mut s, json!({"sequence": 1, "op": "load", "path": path})).unwrap();
        assert_eq!(v["keyframes"][1]["x"], 0.4);
        assert_eq!(v["keyframes"][1]["scale_x"], -2.0);
        assert!(go(&mut s, json!({"op": "load"})).is_err());
        std::fs::write(&path, b"{\"format\":\"nope\"}").unwrap();
        assert!(go(&mut s, json!({"op": "load", "path": path})).is_err());
        assert!(go(
            &mut s,
            json!({"op": "load", "path": dir.join("none").display().to_string()})
        )
        .is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn effect_cameras_are_separate_from_the_sequence_camera() {
        let mut s = session();
        let v = fx(
            &mut s,
            json!({"effect": "mask", "op": "set", "t": 1.0, "x": 0.5}),
        )
        .unwrap();
        assert_eq!(v["effect"], "mask");
        assert_eq!(v["scope"], "sequence");
        assert_eq!(v["active"], true);
        assert!(s.project().sequences[0].camera.keys.is_empty());
        assert_eq!(s.project().sequences[0].effects.cameras.mask.keys.len(), 1);
        assert!(s.project().sequences[0]
            .effects
            .cameras
            .light
            .keys
            .is_empty());
        // The movie has its own set, and no transition.
        fx(
            &mut s,
            json!({"effect": "light", "scope": "movie", "op": "set", "t": 1.0, "y": 0.1}),
        )
        .unwrap();
        assert_eq!(s.project().effects.cameras.light.keys.len(), 1);
        assert!(fx(&mut s, json!({"effect": "transition", "scope": "movie"})).is_err());
        assert!(fx(&mut s, json!({"effect": "nope"})).is_err());
        fx(&mut s, json!({"effect": "mask", "op": "apply_all"})).unwrap();
        assert_eq!(s.project().sequences[1].effects.cameras.mask.keys.len(), 1);
        Registry::standard()
            .run(&mut s, "edit.undo", &json!({}))
            .unwrap();
        assert!(s.project().sequences[1]
            .effects
            .cameras
            .mask
            .keys
            .is_empty());
    }
}
