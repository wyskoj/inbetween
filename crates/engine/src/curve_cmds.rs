//! Curve commands: `curve.feature`, `curve.shape`, `curve.master`, `camera.curve`.
//!
//! All edit a sequence's time curve: the feature curve remaps the colour dissolve, the shape
//! curve remaps how far the dots have travelled, the master curve is the sequence's opacity
//! over time (its ends may sit anywhere; the others are pinned at (0, 0) and (1, 1)) and the
//! camera curve eases the camera's clock. One `op` parameter covers the editing gestures
//! (`curve.edit` in the feature spec): add a point, move one, remove one, toggle linear,
//! reset, or copy the curve to every sequence. (The track curve is `curve.track`.)

use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::Curve;
use serde_json::{json, Value};

const OPS: &[&str] = &["add", "move", "remove", "linear", "reset", "apply_all"];

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
        "add (a point at x, y), move (point `index` to x, y), remove (point `index`), linear (set the straight-segments flag), reset (back to a straight line), apply_all (copy this curve to every sequence); omit to just report the curve",
    )
    .one_of(OPS),
    ParamSpec::new(
        "x",
        ParamKind::Number,
        false,
        "Time of the point, 0 to 1",
    )
    .range(0.0, 1.0),
    ParamSpec::new(
        "y",
        ParamKind::Number,
        false,
        "Amount at that time, 0 to 1",
    )
    .range(0.0, 1.0),
    ParamSpec::new(
        "index",
        ParamKind::Integer,
        false,
        "Which point, counting from 0 (the first and last cannot be moved or removed)",
    )
    .range(0.0, 64.0),
    ParamSpec::new(
        "linear",
        ParamKind::Bool,
        false,
        "For `linear`: true for straight segments, false for a smooth curve; omit to toggle",
    ),
];

#[derive(Clone, Copy, PartialEq)]
enum Which {
    Feature,
    Shape,
    Master,
    Easing,
}

fn curve_mut(seq: &mut inbetween_doc::Sequence, which: Which) -> &mut Curve {
    match which {
        Which::Feature => &mut seq.feature_curve,
        Which::Shape => &mut seq.shape_curve,
        Which::Master => &mut seq.master_curve,
        Which::Easing => &mut seq.camera.easing,
    }
}

fn fresh(which: Which) -> Curve {
    match which {
        Which::Master => Curve::opaque(),
        _ => Curve::default(),
    }
}

fn need(params: &Params, name: &str, op: &str) -> Result<f64, CommandError> {
    params
        .number(name)
        .ok_or_else(|| CommandError::BadParams(format!("`{op}` needs the `{name}` parameter")))
}

fn need_index(params: &Params, op: &str) -> Result<usize, CommandError> {
    params
        .integer("index")
        .map(|n| n.max(0) as usize)
        .ok_or_else(|| CommandError::BadParams(format!("`{op}` needs the `index` parameter")))
}

fn run(session: &mut Session, params: &Params, which: Which) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    if let Some(op) = params.string("op") {
        match op {
            "add" => {
                let (x, y) = (need(params, "x", op)?, need(params, "y", op)?);
                curve_mut(session.sequence_mut(index)?, which).insert(x, y)?;
            }
            "move" => {
                let i = need_index(params, op)?;
                let curve = curve_mut(session.sequence_mut(index)?, which);
                let is_end = i == 0 || i + 1 == curve.points.len();
                if which == Which::Master && is_end {
                    // The master curve's ends only move up and down.
                    curve.set_end_y(i, need(params, "y", op)?)?;
                } else {
                    curve.move_point(i, need(params, "x", op)?, need(params, "y", op)?)?;
                }
            }
            "remove" => {
                let i = need_index(params, op)?;
                curve_mut(session.sequence_mut(index)?, which).remove(i)?;
            }
            "linear" => {
                let curve = curve_mut(session.sequence_mut(index)?, which);
                curve.linear = params.boolean("linear").unwrap_or(!curve.linear);
            }
            "reset" => *curve_mut(session.sequence_mut(index)?, which) = fresh(which),
            "apply_all" => {
                let curve = curve_mut(session.sequence_mut(index)?, which).clone();
                for seq in &mut session.project_mut().sequences {
                    *curve_mut(seq, which) = curve.clone();
                }
            }
            other => {
                return Err(CommandError::BadParams(format!("unknown op `{other}`")));
            }
        }
    }
    let curve = curve_mut(session.sequence_mut(index)?, which).clone();
    if which == Which::Master {
        curve.validate_free()?;
    } else {
        curve.validate()?;
    }
    Ok(json!({
        "sequence": index,
        "points": curve.points.iter().map(|p| json!([p.x, p.y])).collect::<Vec<_>>(),
        "linear": curve.linear,
        "identity": if which == Which::Master { curve.is_opaque() } else { curve.is_identity() },
    }))
}

fn feature(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    run(session, params, Which::Feature)
}

fn shape(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    run(session, params, Which::Shape)
}

fn master(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    run(session, params, Which::Master)
}

fn easing(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    run(session, params, Which::Easing)
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "curve.feature",
            label: "Feature Curve",
            menu: "Sequence/Feature Curve",
            shortcut: Some("Shift+Ctrl+F"),
            description: "Edit (or report) the feature curve, which remaps the colour-dissolve amount over time. Points run left to right; the ends stay at (0, 0) and (1, 1).",
            params: PARAMS,
            enabled: Session::has_sequences,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: feature,
        },
        CommandSpec {
            id: "curve.shape",
            label: "Shape Curve",
            menu: "Sequence/Shape Curve",
            shortcut: Some("Shift+Ctrl+S"),
            description: "Edit (or report) the shape curve, which remaps how far the dots have moved over time. Points run left to right; the ends stay at (0, 0) and (1, 1).",
            params: PARAMS,
            enabled: Session::has_sequences,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: shape,
        },
        CommandSpec {
            id: "curve.master",
            label: "Master Curve",
            menu: "Sequence/Master Curve",
            shortcut: Some("Shift+Ctrl+M"),
            description: "Edit (or report) the master curve, the sequence's overall opacity over time (a fade in or out). Starts and ends at full opacity; move the end points up or down with `move`.",
            params: PARAMS,
            enabled: Session::has_sequences,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: master,
        },
        CommandSpec {
            id: "camera.curve",
            label: "Camera Easing Curve",
            menu: "Sequence/Camera Easing Curve",
            shortcut: None,
            description: "Edit (or report) the camera's easing curve, which remaps the time the camera keyframes are read at. The ends stay at (0, 0) and (1, 1).",
            params: PARAMS,
            enabled: Session::has_sequences,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: easing,
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

    fn go(s: &mut Session, id: &str, p: Value) -> Result<Value, CommandError> {
        Registry::standard().run(s, id, &p)
    }

    #[test]
    fn edit_gestures_change_only_the_named_curve() {
        let mut s = session();
        let v = go(
            &mut s,
            "curve.feature",
            json!({"op": "add", "x": 0.5, "y": 0.2}),
        )
        .unwrap();
        assert_eq!(v["points"], json!([[0.0, 0.0], [0.5, 0.2], [1.0, 1.0]]));
        assert_eq!(v["identity"], false);
        go(
            &mut s,
            "curve.feature",
            json!({"op": "move", "index": 1, "x": 0.6, "y": 0.3}),
        )
        .unwrap();
        assert_eq!(s.project().sequences[0].feature_curve.points[1].x, 0.6);
        assert!(s.project().sequences[0].shape_curve.is_identity());
        assert!(s.project().sequences[1].feature_curve.is_identity());
        let v = go(&mut s, "curve.feature", json!({"op": "linear"})).unwrap();
        assert_eq!(v["linear"], true);
        let v = go(&mut s, "curve.feature", json!({"op": "linear"})).unwrap();
        assert_eq!(v["linear"], false);
        go(&mut s, "curve.feature", json!({"op": "remove", "index": 1})).unwrap();
        assert!(s.project().sequences[0].feature_curve.is_identity());
    }

    #[test]
    fn shape_curve_and_apply_all() {
        let mut s = session();
        go(
            &mut s,
            "curve.shape",
            json!({"op": "add", "x": 0.25, "y": 0.75}),
        )
        .unwrap();
        go(&mut s, "curve.shape", json!({"op": "apply_all"})).unwrap();
        assert_eq!(
            s.project().sequences[1].shape_curve,
            s.project().sequences[0].shape_curve
        );
        assert!(s.project().sequences[1].feature_curve.is_identity());
        go(&mut s, "curve.shape", json!({"sequence": 1, "op": "reset"})).unwrap();
        assert!(s.project().sequences[1].shape_curve.is_identity());
        assert!(!s.project().sequences[0].shape_curve.is_identity());
    }

    #[test]
    fn bad_edits_are_errors_and_leave_the_curve_alone() {
        let mut s = session();
        for p in [
            json!({"op": "add"}),
            json!({"op": "add", "x": 0.5}),
            json!({"op": "add", "x": 0.0, "y": 0.5}),
            json!({"op": "move", "index": 0, "x": 0.5, "y": 0.5}),
            json!({"op": "move", "index": 7, "x": 0.5, "y": 0.5}),
            json!({"op": "remove", "index": 1}),
            json!({"op": "remove"}),
            json!({"op": "bogus"}),
            json!({"sequence": 9}),
        ] {
            assert!(go(&mut s, "curve.shape", p.clone()).is_err(), "{p}");
        }
        assert!(s.project().sequences[0].shape_curve.is_identity());
        assert!(!s.can_undo());
    }

    #[test]
    fn edits_undo() {
        let mut s = session();
        go(
            &mut s,
            "curve.feature",
            json!({"op": "add", "x": 0.5, "y": 0.2}),
        )
        .unwrap();
        assert!(s.can_undo());
        assert!(s.undo());
        assert!(s.project().sequences[0].feature_curve.is_identity());
    }
}
