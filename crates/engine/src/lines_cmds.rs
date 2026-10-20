//! Lines between dots (organizational only, they never affect the morph): `lines.connect`,
//! `lines.addPolyline`, `lines.insertDot`, `lines.delete`, `lines.drag`.

use crate::dots_cmds::place_dot;
use crate::select_cmds::move_ends;
use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_geom::Point;
use serde_json::{json, Value};

const COORD_LIMIT: f64 = 10_000_000.0;
const MAX_INDEX: f64 = 1_000_000.0;
/// Most points one polyline command accepts (x and y each count).
const MAX_POLYLINE_VALUES: f64 = 20_000.0;

const SEQUENCE: ParamSpec = ParamSpec::new(
    "sequence",
    ParamKind::Integer,
    false,
    "Sequence number, starting at 0",
)
.range(0.0, 1_000_000.0)
.default_is("0");

const IMAGE: ParamSpec = ParamSpec::new("image", ParamKind::Integer, false, "Which image (1 or 2)")
    .range(1.0, 2.0)
    .default_is("1");

const LINE: ParamSpec =
    ParamSpec::new("line", ParamKind::Integer, true, "Line number, from 0").range(0.0, MAX_INDEX);

const fn coord(name: &'static str, description: &'static str, required: bool) -> ParamSpec {
    ParamSpec::new(name, ParamKind::Number, required, description).range(-COORD_LIMIT, COORD_LIMIT)
}

fn bad(message: impl Into<String>) -> CommandError {
    CommandError::BadParams(message.into())
}

fn has_sequence(session: &Session) -> bool {
    session.has_sequences()
}

fn which_image(params: &Params) -> u8 {
    if params.integer("image") == Some(2) {
        2
    } else {
        1
    }
}

fn size(session: &Session) -> (u32, u32) {
    (
        session.project().output.width,
        session.project().output.height,
    )
}

fn line_ends(
    seq: &inbetween_doc::Sequence,
    params: &Params,
) -> Result<(usize, usize, usize), CommandError> {
    let l = params.integer("line").unwrap_or(-1);
    let n = usize::try_from(l)
        .ok()
        .filter(|n| *n < seq.dots.lines.len())
        .ok_or_else(|| {
            bad(format!(
                "there is no line {l} (this sequence has {}; numbering starts at 0)",
                seq.dots.lines.len()
            ))
        })?;
    let line = seq.dots.lines[n];
    Ok((n, line.a as usize, line.b as usize))
}

const CONNECT_PARAMS: &[ParamSpec] = &[
    ParamSpec::new("a", ParamKind::Integer, true, "First dot, from 0").range(0.0, MAX_INDEX),
    ParamSpec::new("b", ParamKind::Integer, true, "Second dot, from 0").range(0.0, MAX_INDEX),
    SEQUENCE,
];

fn connect(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let (a, b) = (
        params.integer("a").unwrap_or(-1),
        params.integer("b").unwrap_or(-1),
    );
    let seq = session.sequence_mut(index)?;
    let ok = match (usize::try_from(a), usize::try_from(b)) {
        (Ok(a), Ok(b)) => seq.dots.connect(a, b),
        _ => false,
    };
    if !ok {
        return Err(bad(format!(
            "cannot join dots {a} and {b}: both must exist, differ, and not be joined already"
        )));
    }
    Ok(json!({ "sequence": index, "line": seq.dots.lines.len() - 1 }))
}

const POLYLINE_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "points",
        ParamKind::NumberList,
        true,
        "Corner positions on `image` as x0, y0, x1, y1, ... in output pixels",
    )
    .range(2.0, MAX_POLYLINE_VALUES),
    IMAGE,
    ParamSpec::new(
        "close",
        ParamKind::Bool,
        false,
        "Join the last point back to the first",
    ),
    SEQUENCE,
];

fn add_polyline(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let image = which_image(params);
    let values = params.numbers("points").unwrap_or_default();
    if values.len() % 2 != 0 || values.len() < 2 {
        return Err(bad("`points` needs pairs of numbers: x0, y0, x1, y1, ..."));
    }
    let (sz, auto) = (size(session), session.prefs().auto_partner);
    let seq = session.sequence_mut(index)?;
    let mut dots = Vec::new();
    for xy in values.chunks_exact(2) {
        let at = Point::new(xy[0], xy[1]);
        dots.push(place_dot(seq, sz, auto, image, at, None, false)?);
    }
    let mut lines = 0;
    for w in dots.windows(2) {
        lines += usize::from(seq.dots.connect(w[0], w[1]));
    }
    if params.boolean("close") == Some(true) && dots.len() > 2 {
        if let (Some(&first), Some(&last)) = (dots.first(), dots.last()) {
            lines += usize::from(seq.dots.connect(last, first));
        }
    }
    Ok(json!({ "sequence": index, "dots": dots, "lines": lines }))
}

const INSERT_PARAMS: &[ParamSpec] = &[
    LINE,
    coord("x", "Horizontal position on `image`", true),
    coord("y", "Vertical position on `image`", true),
    IMAGE,
    SEQUENCE,
];

fn insert_dot(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let image = which_image(params);
    let other = 3 - image;
    let at = Point::new(
        params.number("x").unwrap_or(0.0),
        params.number("y").unwrap_or(0.0),
    );
    let (sz, auto) = (size(session), session.prefs().auto_partner);
    let seq = session.sequence_mut(index)?;
    let (n, a, b) = line_ends(seq, params)?;
    let ends = |w: u8| {
        let pa = seq.dots.pairs.get(a).filter(|p| p.has_end(w))?;
        let pb = seq.dots.pairs.get(b).filter(|p| p.has_end(w))?;
        Some((pa.end(w), pb.end(w)))
    };
    // The partner sits at the same fraction along the partner segment.
    let second = match (ends(image), ends(other)) {
        (Some((p, q)), Some((r, s))) => {
            let d = q - p;
            let len2 = d.x * d.x + d.y * d.y;
            let t = if len2 > 0.0 && len2.is_finite() {
                (((at.x - p.x) * d.x + (at.y - p.y) * d.y) / len2).clamp(0.0, 1.0)
            } else {
                0.5
            };
            Some(r.lerp(s, t))
        }
        _ => None,
    };
    let pair_at = second.map(|m| if image == 1 { (at, m) } else { (m, at) });
    let new = match pair_at {
        Some((first, second)) => place_dot(seq, sz, auto, 1, first, Some(second), false)?,
        None => place_dot(seq, sz, auto, image, at, None, false)?,
    };
    seq.dots.lines.remove(n);
    seq.dots.connect(a, new);
    seq.dots.connect(new, b);
    Ok(json!({ "sequence": index, "index": new }))
}

const DELETE_PARAMS: &[ParamSpec] = &[LINE, SEQUENCE];

fn delete(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let seq = session.sequence_mut(index)?;
    let (n, _, _) = line_ends(seq, params)?;
    seq.dots.lines.remove(n);
    Ok(json!({ "sequence": index, "deleted": n, "lines": seq.dots.lines.len() }))
}

const DRAG_PARAMS: &[ParamSpec] = &[
    LINE,
    IMAGE,
    coord("dx", "Horizontal distance in pixels", true),
    coord("dy", "Vertical distance in pixels", true),
    SEQUENCE,
];

fn drag(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let image = which_image(params);
    let (dx, dy) = (
        params.number("dx").unwrap_or(0.0),
        params.number("dy").unwrap_or(0.0),
    );
    let seq = session.sequence_mut(index)?;
    let (_, a, _) = line_ends(seq, params)?;
    let group = seq.dots.connected(a);
    let moved = move_ends(seq, image, &group, &|p| Point::new(p.x + dx, p.y + dy))?;
    Ok(json!({ "sequence": index, "moved": moved }))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "lines.connect",
            label: "Connect Dots",
            menu: "",
            shortcut: None,
            description: "Join two dots with a line. Lines are for organizing and never change the morph.",
            params: CONNECT_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: true,
            run: connect,
        },
        CommandSpec {
            id: "lines.addPolyline",
            label: "Add Polyline",
            menu: "",
            shortcut: None,
            description: "Add dots along a path and join them with lines, as one undo step.",
            params: POLYLINE_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: true,
            run: add_polyline,
        },
        CommandSpec {
            id: "lines.insertDot",
            label: "Insert Dot On Line",
            menu: "",
            shortcut: None,
            description: "Split a line with a new dot; the partner goes at the same fraction along the partner line.",
            params: INSERT_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: true,
            run: insert_dot,
        },
        CommandSpec {
            id: "lines.delete",
            label: "Delete Line",
            menu: "",
            shortcut: None,
            description: "Delete one line; its dots stay.",
            params: DELETE_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: true,
            run: delete,
        },
        CommandSpec {
            id: "lines.drag",
            label: "Drag Line",
            menu: "",
            shortcut: None,
            description: "Move every dot connected to a line, on one image.",
            params: DRAG_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: true,
            run: drag,
        },
    ]
}

#[cfg(test)]
mod tests {
    use crate::{Registry, Session};
    use inbetween_doc::{ImageRef, Project, Sequence};
    use serde_json::{json, Value};

    fn session() -> Session {
        let mut s = Session::new();
        s.set_project(
            Project {
                sequences: vec![Sequence::new(
                    ImageRef::new("a.png"),
                    ImageRef::new("b.png"),
                )],
                ..Project::default()
            },
            None,
        );
        s
    }

    fn run(s: &mut Session, id: &str, p: Value) -> Result<Value, crate::CommandError> {
        Registry::standard().run(s, id, &p)
    }

    #[test]
    fn polyline_is_one_undo_step_and_closes() {
        let mut s = session();
        let v = run(
            &mut s,
            "lines.addPolyline",
            json!({"points": [0, 0, 10, 0, 10, 10], "close": true}),
        )
        .unwrap();
        assert_eq!(v["lines"], 3);
        let d = &s.project().sequences[0].dots;
        assert_eq!((d.len(), d.lines.len()), (3, 3));
        run(&mut s, "edit.undo", json!({})).unwrap();
        assert_eq!(s.project().sequences[0].dots.len(), 0);
        assert!(run(&mut s, "lines.addPolyline", json!({"points": [1, 2, 3]})).is_err());
        assert!(s.project().sequences[0].dots.is_empty());
    }

    #[test]
    fn insert_puts_the_partner_at_the_same_fraction() {
        let mut s = session();
        run(
            &mut s,
            "dots.add",
            json!({"x": 0, "y": 0, "x2": 100, "y2": 0}),
        )
        .unwrap();
        run(
            &mut s,
            "dots.add",
            json!({"x": 10, "y": 0, "x2": 200, "y2": 0}),
        )
        .unwrap();
        run(&mut s, "lines.connect", json!({"a": 0, "b": 1})).unwrap();
        run(
            &mut s,
            "lines.insertDot",
            json!({"line": 0, "x": 2.5, "y": 0}),
        )
        .unwrap();
        let d = &s.project().sequences[0].dots;
        assert_eq!(d.len(), 3);
        assert_eq!(d.pairs[2].second.x, 125.0);
        assert_eq!(d.lines.len(), 2);
        assert!(d.validate_lines().is_ok());
    }

    #[test]
    fn connect_delete_drag_and_bad_input() {
        let mut s = session();
        for x in [0, 10, 20] {
            run(&mut s, "dots.add", json!({"x": x, "y": 5})).unwrap();
        }
        run(&mut s, "lines.connect", json!({"a": 0, "b": 1})).unwrap();
        run(&mut s, "lines.connect", json!({"a": 1, "b": 2})).unwrap();
        for bad in [
            json!({"a": 0, "b": 1}),
            json!({"a": 1, "b": 1}),
            json!({"a": 0, "b": 9}),
        ] {
            assert!(run(&mut s, "lines.connect", bad).is_err());
        }
        run(
            &mut s,
            "lines.drag",
            json!({"line": 0, "dx": 1, "dy": 2, "image": 2}),
        )
        .unwrap();
        let d = &s.project().sequences[0].dots;
        assert_eq!((d.pairs[2].second.x, d.pairs[2].second.y), (21.0, 7.0));
        assert_eq!(d.pairs[2].first.x, 20.0);
        run(&mut s, "lines.delete", json!({"line": 0})).unwrap();
        assert_eq!(s.project().sequences[0].dots.lines.len(), 1);
        assert!(run(&mut s, "lines.delete", json!({"line": 5})).is_err());
        assert!(run(
            &mut s,
            "lines.insertDot",
            json!({"line": 5, "x": 1, "y": 1})
        )
        .is_err());
        // Deleting a dot takes its lines with it.
        run(&mut s, "dots.delete", json!({"index": 1})).unwrap();
        assert!(s.project().sequences[0].dots.lines.is_empty());
    }
}
