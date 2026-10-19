//! Track curve command: `curve.track`.
//!
//! The track bends the path every dot travels between its place on Image 1 and its place on
//! Image 2. The curve lives in *chord space*: `u` runs along the straight line from a dot's
//! start to its end (0 to 1) and `v` is the sideways distance, both in chord lengths. Every dot
//! follows the same curve, scaled and turned onto its own chord. The ends stay at (0, 0) and
//! (1, 0), so the first and last frames still match the images; inner points that stray outside
//! the chord make dots overshoot (caricature).

use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{Track, TrackPoint};
use serde_json::{json, Value};

const OPS: &[&str] = &["add", "move", "remove", "reset", "apply_all", "preset"];
const PRESETS: &[&str] = &["arc", "wave", "overshoot"];

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
        "add (a control point at u, v; at `index` or before the end), move (point `index` to u, v), remove (point `index`), reset (back to a straight path), preset (one of the named shapes), apply_all (copy this track to every sequence); omit to just report the track",
    )
    .one_of(OPS),
    ParamSpec::new(
        "u",
        ParamKind::Number,
        false,
        "Position along the chord in chord lengths (0 is the dot's start, 1 its end)",
    )
    .range(-100.0, 100.0),
    ParamSpec::new(
        "v",
        ParamKind::Number,
        false,
        "Sideways distance from the chord in chord lengths",
    )
    .range(-100.0, 100.0),
    ParamSpec::new(
        "index",
        ParamKind::Integer,
        false,
        "Which control point, counting from 0 (the first and last cannot move or be removed)",
    )
    .range(0.0, 64.0),
    ParamSpec::new(
        "name",
        ParamKind::String,
        false,
        "For `preset`: arc (one gentle bow), wave (an S bend) or overshoot (swings past the end first)",
    )
    .one_of(PRESETS),
    ParamSpec::new(
        "tension",
        ParamKind::Number,
        false,
        "0 keeps the straight line, 1 follows the curve as drawn, up to 20 exaggerates it",
    )
    .range(0.0, 20.0)
    .default_is("1"),
    ParamSpec::new(
        "flip_h",
        ParamKind::Bool,
        false,
        "Mirror the along-the-chord part of the bend",
    ),
    ParamSpec::new(
        "flip_v",
        ParamKind::Bool,
        false,
        "Mirror the sideways part of the bend (bow to the other side)",
    ),
    ParamSpec::new(
        "enabled",
        ParamKind::Bool,
        false,
        "Switch the track on or off; changing the points switches it on",
    ),
];

fn preset(name: &str) -> Option<Vec<TrackPoint>> {
    let p = |u, v| TrackPoint { u, v };
    match name {
        "arc" => Some(vec![p(0.0, 0.0), p(0.5, 0.25), p(1.0, 0.0)]),
        "wave" => Some(vec![p(0.0, 0.0), p(0.25, 0.2), p(0.75, -0.2), p(1.0, 0.0)]),
        "overshoot" => Some(vec![p(0.0, 0.0), p(1.25, 0.1), p(1.0, 0.0)]),
        _ => None,
    }
}

fn need(params: &Params, name: &str, op: &str) -> Result<f64, CommandError> {
    params
        .number(name)
        .ok_or_else(|| CommandError::BadParams(format!("`{op}` needs the `{name}` parameter")))
}

fn need_inner(params: &Params, op: &str, len: usize) -> Result<usize, CommandError> {
    let i = params
        .integer("index")
        .map(|n| n.max(0) as usize)
        .ok_or_else(|| CommandError::BadParams(format!("`{op}` needs the `index` parameter")))?;
    if i == 0 || i + 1 >= len {
        return Err(CommandError::BadParams(format!(
            "no inner control point {i}: the track has {len} points and the first and last are fixed"
        )));
    }
    Ok(i)
}

fn report(index: usize, t: &Track) -> Value {
    json!({
        "sequence": index,
        "enabled": t.enabled,
        "active": t.is_active(),
        "tension": t.tension,
        "flip_h": t.flip_h,
        "flip_v": t.flip_v,
        "points": t.points.iter().map(|p| json!([p.u, p.v])).collect::<Vec<_>>(),
    })
}

fn run(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let mut touched_points = false;
    match params.string("op") {
        None => {}
        Some(op @ "add") => {
            let (u, v) = (need(params, "u", op)?, need(params, "v", op)?);
            let track = &mut session.sequence_mut(index)?.track;
            let len = track.points.len();
            let at = params
                .integer("index")
                .map_or(len - 1, |n| (n.max(1) as usize).min(len - 1));
            track.points.insert(at, TrackPoint { u, v });
            touched_points = true;
        }
        Some(op @ "move") => {
            let (u, v) = (need(params, "u", op)?, need(params, "v", op)?);
            let track = &mut session.sequence_mut(index)?.track;
            let i = need_inner(params, op, track.points.len())?;
            if let Some(p) = track.points.get_mut(i) {
                *p = TrackPoint { u, v };
            }
            touched_points = true;
        }
        Some(op @ "remove") => {
            let track = &mut session.sequence_mut(index)?.track;
            let i = need_inner(params, op, track.points.len())?;
            track.points.remove(i);
            touched_points = true;
        }
        Some("reset") => {
            let seq = session.sequence_mut(index)?;
            seq.track = Track {
                tension: seq.track.tension,
                flip_h: seq.track.flip_h,
                flip_v: seq.track.flip_v,
                ..Track::default()
            };
        }
        Some(op @ "preset") => {
            let name = params.string("name").ok_or_else(|| {
                CommandError::BadParams(format!("`{op}` needs the `name` parameter"))
            })?;
            let points = preset(name)
                .ok_or_else(|| CommandError::BadParams(format!("unknown preset `{name}`")))?;
            session.sequence_mut(index)?.track.points = points;
            touched_points = true;
        }
        Some("apply_all") => {
            let track = session.sequence_mut(index)?.track.clone();
            for seq in &mut session.project_mut().sequences {
                seq.track = track.clone();
            }
        }
        Some(other) => return Err(CommandError::BadParams(format!("unknown op `{other}`"))),
    }
    let track = &mut session.sequence_mut(index)?.track;
    if let Some(v) = params.number("tension") {
        track.tension = v;
    }
    if let Some(v) = params.boolean("flip_h") {
        track.flip_h = v;
    }
    if let Some(v) = params.boolean("flip_v") {
        track.flip_v = v;
    }
    if touched_points {
        track.enabled = true;
    }
    if let Some(on) = params.boolean("enabled") {
        track.enabled = on;
    }
    track.validate()?;
    Ok(report(index, track))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "curve.track",
        label: "Track Curve",
        menu: "Sequence/Track Curve",
        shortcut: Some("Shift+Ctrl+T"),
        description: "Edit (or report) the track curve: the path every dot travels from Image 1 to Image 2, bent by control points in chord space (u along the straight line, v sideways). Tension 0 to 20 scales the bend; flips mirror it. The ends stay fixed, so the first and last frames still match the images.",
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
        Registry::standard().run(s, "curve.track", &p)
    }

    #[test]
    fn points_are_added_moved_and_removed() {
        let mut s = session();
        let v = go(&mut s, json!({})).unwrap();
        assert_eq!(v["active"], false);
        assert_eq!(v["points"], json!([[0.0, 0.0], [1.0, 0.0]]));
        let v = go(&mut s, json!({"op": "add", "u": 0.5, "v": 0.3})).unwrap();
        assert_eq!(v["points"], json!([[0.0, 0.0], [0.5, 0.3], [1.0, 0.0]]));
        assert_eq!(v["enabled"], true);
        assert_eq!(v["active"], true);
        let v = go(
            &mut s,
            json!({"op": "move", "index": 1, "u": 0.6, "v": -0.1}),
        )
        .unwrap();
        assert_eq!(v["points"][1], json!([0.6, -0.1]));
        assert!(go(
            &mut s,
            json!({"op": "move", "index": 0, "u": 0.1, "v": 0.0})
        )
        .is_err());
        assert!(go(&mut s, json!({"op": "remove", "index": 2})).is_err());
        assert!(go(&mut s, json!({"op": "remove"})).is_err());
        assert!(go(&mut s, json!({"op": "add", "u": 0.5})).is_err());
        let v = go(&mut s, json!({"op": "remove", "index": 1})).unwrap();
        assert_eq!(v["points"], json!([[0.0, 0.0], [1.0, 0.0]]));
        assert_eq!(v["active"], false);
    }

    #[test]
    fn tension_flips_presets_and_reset() {
        let mut s = session();
        let v = go(
            &mut s,
            json!({"op": "preset", "name": "wave", "tension": 3.5, "flip_v": true}),
        )
        .unwrap();
        assert_eq!(v["points"].as_array().map(Vec::len), Some(4));
        assert_eq!(v["tension"], 3.5);
        assert_eq!(v["flip_v"], true);
        assert!(go(&mut s, json!({"op": "preset"})).is_err());
        let v = go(&mut s, json!({"op": "reset"})).unwrap();
        assert_eq!(v["points"], json!([[0.0, 0.0], [1.0, 0.0]]));
        assert_eq!(v["tension"], 3.5, "reset only straightens the path");
        let v = go(&mut s, json!({"enabled": false})).unwrap();
        assert_eq!(v["enabled"], false);
    }

    #[test]
    fn add_inserts_at_the_given_place_and_clamps_it() {
        let mut s = session();
        go(&mut s, json!({"op": "add", "u": 0.3, "v": 0.1})).unwrap();
        go(&mut s, json!({"op": "add", "u": 0.1, "v": 0.2, "index": 1})).unwrap();
        let v = go(
            &mut s,
            json!({"op": "add", "u": 0.9, "v": 0.2, "index": 50}),
        )
        .unwrap();
        assert_eq!(v["points"][1], json!([0.1, 0.2]));
        assert_eq!(v["points"][3], json!([0.9, 0.2]));
        assert_eq!(v["points"][4], json!([1.0, 0.0]));
    }

    #[test]
    fn at_most_64_points_and_bad_tension_are_refused() {
        let mut s = session();
        for i in 0..62 {
            go(
                &mut s,
                json!({"op": "add", "u": 0.5, "v": f64::from(i) / 100.0}),
            )
            .unwrap();
        }
        assert!(go(&mut s, json!({"op": "add", "u": 0.5, "v": 0.5})).is_err());
        assert!(go(&mut s, json!({"tension": 21})).is_err());
        let v = go(&mut s, json!({})).unwrap();
        assert_eq!(v["points"].as_array().map(Vec::len), Some(64));
    }

    #[test]
    fn apply_all_copies_the_track() {
        let mut s = session();
        go(&mut s, json!({"op": "preset", "name": "arc"})).unwrap();
        go(&mut s, json!({"op": "apply_all"})).unwrap();
        let v = go(&mut s, json!({"sequence": 1})).unwrap();
        assert_eq!(v["active"], true);
    }
}
