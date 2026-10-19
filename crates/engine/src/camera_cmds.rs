//! Camera command: `camera.keyframes`. (The camera's easing curve is `camera.curve`, in
//! `curve_cmds`.)
//!
//! A sequence's camera moves the rendered morph over time: offset, scale (negative flips) and
//! rotation, as keyframes at morph times from 0 to 1. Frames between keyframes ease smoothly.

use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{Camera, CameraAspect, CameraKey};
use serde_json::{json, Value};

const OPS: &[&str] = &["set", "remove", "reset", "apply_all"];
const ASPECTS: &[&str] = &["free", "locked"];

const PARAMS: &[ParamSpec] = &[
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
        "set (a keyframe at `t`; values you leave out keep what the camera shows there), remove (keyframe `index`), reset (no keyframes), apply_all (copy this camera to every sequence); omit to just report the camera",
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
        "free: width and height scale separately; locked: setting one scale sets the other to match",
    )
    .one_of(ASPECTS),
];

fn key_json(k: &CameraKey) -> Value {
    json!({
        "t": k.t, "x": k.x, "y": k.y,
        "scale_x": k.scale_x, "scale_y": k.scale_y, "rotation": k.rotation,
    })
}

fn report(index: usize, c: &Camera) -> Value {
    json!({
        "sequence": index,
        "enabled": c.enabled,
        "active": c.is_active(),
        "aspect": c.aspect.id(),
        "easing_identity": c.easing.is_identity(),
        "keyframes": c.keys.iter().map(key_json).collect::<Vec<_>>(),
    })
}

/// Build the keyframe a `set` describes: omitted values come from the pose already shown at `t`.
fn keyframe(camera: &Camera, params: &Params) -> Result<CameraKey, CommandError> {
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
    if camera.aspect == CameraAspect::Locked {
        // The scale that was given drives the other; the other keeps its own flip.
        match (params.number("scale_x"), params.number("scale_y")) {
            (Some(_), None) => k.scale_y = k.scale_x.abs().copysign(shown.scale_y),
            (None, Some(_)) => k.scale_x = k.scale_y.abs().copysign(shown.scale_x),
            _ => {}
        }
    }
    Ok(k)
}

fn run(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    if let Some(aspect) = params.string("aspect").and_then(CameraAspect::from_id) {
        session.sequence_mut(index)?.camera.aspect = aspect;
    }
    match params.string("op") {
        None => {}
        Some("set") => {
            let camera = &mut session.sequence_mut(index)?.camera;
            let key = keyframe(camera, params)?;
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
            session.sequence_mut(index)?.camera.remove_key(i)?;
        }
        Some("reset") => {
            let seq = session.sequence_mut(index)?;
            seq.camera = Camera {
                aspect: seq.camera.aspect,
                ..Camera::default()
            };
        }
        Some("apply_all") => {
            let camera = session.sequence_mut(index)?.camera.clone();
            for seq in &mut session.project_mut().sequences {
                seq.camera = camera.clone();
            }
        }
        Some(other) => return Err(CommandError::BadParams(format!("unknown op `{other}`"))),
    }
    let camera = &mut session.sequence_mut(index)?.camera;
    if let Some(on) = params.boolean("enabled") {
        camera.enabled = on;
    }
    camera.validate()?;
    Ok(report(index, camera))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "camera.keyframes",
        label: "Camera",
        menu: "Sequence/Camera...",
        shortcut: Some("Shift+Ctrl+C"),
        description: "Edit (or report) the sequence's camera: keyframes of offset, scale and rotation over morph time that move the rendered morph. Frames between keyframes ease smoothly; uncovered areas are transparent.",
        params: PARAMS,
        enabled: Session::has_sequences,
        disabled_reason: "the project has no sequences",
        edits: true,
        run,
    }]
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
    fn locked_aspect_makes_the_scales_follow_each_other() {
        let mut s = session();
        go(&mut s, json!({"aspect": "locked"})).unwrap();
        let v = go(&mut s, json!({"op": "set", "t": 1.0, "scale_x": 2.0})).unwrap();
        assert_eq!(v["keyframes"][0]["scale_y"], 2.0);
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
}
