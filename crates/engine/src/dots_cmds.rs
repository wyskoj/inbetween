//! Dot commands: `dots.add`, `dots.move`, `dots.delete`, `dots.import`, `dots.export`.

use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{DotPair, Sequence, Side};
use inbetween_geom::Point;
use serde_json::{json, Value};

/// Dot coordinates are output-frame pixels; this keeps absurd values out of the model.
const COORD_LIMIT: f64 = 10_000_000.0;
const MAX_INDEX: f64 = 1_000_000.0;

const SEQUENCE: ParamSpec = ParamSpec::new(
    "sequence",
    ParamKind::Integer,
    false,
    "Sequence number, starting at 0",
)
.range(0.0, 1_000_000.0)
.default_is("0");

const fn coord(name: &'static str, description: &'static str, required: bool) -> ParamSpec {
    ParamSpec::new(name, ParamKind::Number, required, description).range(-COORD_LIMIT, COORD_LIMIT)
}

const ADD_PARAMS: &[ParamSpec] = &[
    coord(
        "x",
        "Horizontal position on Image 1, in output pixels",
        true,
    ),
    coord("y", "Vertical position on Image 1, in output pixels", true),
    coord("x2", "Horizontal position on Image 2", false)
        .default_is("where the dots already placed carry (x, y)"),
    coord("y2", "Vertical position on Image 2", false)
        .default_is("where the dots already placed carry (x, y)"),
    ParamSpec::new(
        "image",
        ParamKind::Integer,
        false,
        "Which image (1 or 2) x and y are on when x2 and y2 are left out",
    )
    .range(1.0, 2.0)
    .default_is("1"),
    SEQUENCE,
];

const ADD_SAME_PARAMS: &[ParamSpec] = &[
    coord("x", "Horizontal position, in output pixels", true),
    coord("y", "Vertical position, in output pixels", true),
    SEQUENCE,
];

const MOVE_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "index",
        ParamKind::Integer,
        true,
        "Which dot, counting from 0",
    )
    .range(0.0, MAX_INDEX),
    ParamSpec::new(
        "image",
        ParamKind::Integer,
        true,
        "Which image's copy of the dot: 1 or 2",
    )
    .range(1.0, 2.0),
    coord("x", "New horizontal position, in output pixels", true),
    coord("y", "New vertical position, in output pixels", true),
    SEQUENCE,
];

const DELETE_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "index",
        ParamKind::Integer,
        true,
        "Which dot, counting from 0",
    )
    .range(0.0, MAX_INDEX),
    SEQUENCE,
];

fn has_sequence(session: &Session) -> bool {
    session.has_sequences()
}

fn number(params: &Params, name: &str) -> Result<f64, CommandError> {
    params
        .number(name)
        .ok_or_else(|| CommandError::BadParams(format!("missing parameter `{name}`")))
}

fn index_of(params: &Params, count: usize) -> Result<usize, CommandError> {
    let index = params.integer("index").unwrap_or(-1);
    usize::try_from(index)
        .ok()
        .filter(|i| *i < count)
        .ok_or_else(|| {
            CommandError::BadParams(format!(
                "there is no dot {index} (this sequence has {count}; numbering starts at 0)"
            ))
        })
}

fn occupied(which: u8) -> CommandError {
    CommandError::BadParams(format!(
        "another dot is already at that position on Image {which}; two dots cannot share a spot"
    ))
}

/// Whether some dot other than `skip` already sits at `at` on image `which`.
pub(crate) fn taken(seq: &Sequence, which: u8, at: Point, skip: Option<usize>) -> bool {
    seq.dots
        .pairs
        .iter()
        .enumerate()
        .any(|(i, p)| Some(i) != skip && p.has_end(which) && p.end(which) == at)
}

#[derive(Clone, Copy, PartialEq)]
enum Partner {
    /// Map the point through the paired mesh, or leave the partner out, per the preference.
    Auto,
    /// Put the partner at identical coordinates.
    Same,
}

/// Put a dot at `at` on image `image` (1 or 2): a full pair when `second` is given, `same` is
/// set or `auto_partner` is on, otherwise a dot waiting for its partner (or completing the
/// oldest one that is). Returns the dot number.
#[allow(clippy::too_many_arguments)]
pub(crate) fn place_dot(
    seq: &mut Sequence,
    size: (u32, u32),
    auto_partner: bool,
    image: u8,
    at: Point,
    second: Option<Point>,
    same: bool,
) -> Result<usize, CommandError> {
    let pair = if let Some(second) = second {
        Some(DotPair::new(at, second))
    } else if same {
        Some(DotPair::fixed(at))
    } else if auto_partner {
        let mapped = inbetween_morph::automatch::transfer_point(seq, at, image, size.0, size.1);
        Some(if image == 1 {
            DotPair::new(at, mapped)
        } else {
            DotPair::new(mapped, at)
        })
    } else {
        None
    };
    let dot = match pair {
        Some(pair) => {
            if taken(seq, 1, pair.first, None) {
                return Err(occupied(1));
            }
            if taken(seq, 2, pair.second, None) {
                return Err(occupied(2));
            }
            seq.dots.pairs.push(pair);
            seq.dots.len() - 1
        }
        None => {
            if taken(seq, image, at, None) {
                return Err(occupied(image));
            }
            // A dot that is still missing this image's end is completed first, oldest first.
            let waiting = seq.dots.pairs.iter().position(|p| !p.has_end(image));
            match waiting.and_then(|i| seq.dots.pairs.get_mut(i).map(|p| (i, p))) {
                Some((i, p)) => {
                    p.side = Side::Both;
                    if image == 1 {
                        p.first = at;
                    } else {
                        p.second = at;
                    }
                    i
                }
                None => {
                    let side = if image == 1 { Side::Only1 } else { Side::Only2 };
                    seq.dots.pairs.push(DotPair::unpaired(at, side));
                    seq.dots.len() - 1
                }
            }
        }
    };
    seq.validate()?;
    Ok(dot)
}

fn add_dot(
    session: &mut Session,
    params: &Params,
    partner: Partner,
) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let image: u8 = if params.integer("image") == Some(2) {
        2
    } else {
        1
    };
    let at = Point::new(number(params, "x")?, number(params, "y")?);
    let size = (
        session.project().output.width,
        session.project().output.height,
    );
    let auto_partner = session.prefs().auto_partner;
    let second = (params.has("x2") || params.has("y2")).then(|| {
        Point::new(
            params.number("x2").unwrap_or(at.x),
            params.number("y2").unwrap_or(at.y),
        )
    });
    let seq = session.sequence_mut(index)?;
    let same = partner == Partner::Same;
    let dot = place_dot(seq, size, auto_partner, image, at, second, same)?;
    let paired = seq.dots.pairs.get(dot).is_some_and(DotPair::is_paired);
    Ok(json!({ "sequence": index, "index": dot, "dots": seq.dots.len(), "paired": paired }))
}

fn add(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    add_dot(session, params, Partner::Auto)
}

fn add_same_pos(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    add_dot(session, params, Partner::Same)
}

fn move_dot(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let seq_index = session.sequence_index(params)?;
    let to = Point::new(number(params, "x")?, number(params, "y")?);
    let image = params.integer("image").unwrap_or(1);
    let seq = session.sequence_mut(seq_index)?;
    let dot = index_of(params, seq.dots.len())?;
    let which = if image == 1 { 1 } else { 2 };
    if !seq.dots.pairs.get(dot).is_some_and(|p| p.has_end(which)) {
        return Err(CommandError::BadParams(format!(
            "dot {dot} is not on Image {which}"
        )));
    }
    let clash = seq
        .dots
        .pairs
        .iter()
        .enumerate()
        .any(|(i, p)| i != dot && p.has_end(which) && p.end(which) == to);
    if clash {
        return Err(occupied(if image == 1 { 1 } else { 2 }));
    }
    if let Some(pair) = seq.dots.pairs.get_mut(dot) {
        pair.set_end(which, to);
    }
    Ok(json!({ "sequence": seq_index, "index": dot, "image": image, "x": to.x, "y": to.y }))
}

fn delete(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let seq_index = session.sequence_index(params)?;
    let seq = session.sequence_mut(seq_index)?;
    let dot = index_of(params, seq.dots.len())?;
    seq.dots.remove(dot);
    Ok(json!({ "sequence": seq_index, "deleted": dot, "dots": seq.dots.len() }))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    #[allow(unused_mut)]
    let mut specs = vec![
        CommandSpec {
            id: "dots.add",
            label: "Add Dot",
            menu: "",
            shortcut: None,
            description: "Add a dot. With auto-partner on (the default) the partner is placed where the existing dots carry that point, or at x2 and y2; with it off the dot waits on one image for its partner.",
            params: ADD_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence to put dots in",
            edits: true,
            run: add,
        },
        CommandSpec {
            id: "dots.addSamePos",
            label: "Add Dot At Same Position",
            menu: "",
            shortcut: None,
            description: "Add a dot pair with the partner at identical coordinates, for boundary and anchor dots.",
            params: ADD_SAME_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence to put dots in",
            edits: true,
            run: add_same_pos,
        },
        CommandSpec {
            id: "dots.move",
            label: "Move Dot",
            menu: "",
            shortcut: None,
            description: "Move one dot on one image; its partner on the other image stays put.",
            params: MOVE_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no dots to move",
            edits: true,
            run: move_dot,
        },
        CommandSpec {
            id: "dots.delete",
            label: "Delete Dot",
            menu: "",
            shortcut: None,
            description: "Delete a dot pair from both images.",
            params: DELETE_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no dots to delete",
            edits: true,
            run: delete,
        },
    ];
    #[cfg(not(target_arch = "wasm32"))]
    specs.extend(files::specs());
    specs
}

#[cfg(not(target_arch = "wasm32"))]
mod files {
    use super::*;
    use std::path::Path;

    const IMPORT_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "path",
            ParamKind::Path,
            true,
            "The .inbetweendots file to read",
        )
        .open_file(&["inbetweendots"]),
        SEQUENCE,
    ];
    const EXPORT_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "path",
            ParamKind::Path,
            true,
            "The .inbetweendots file to write",
        )
        .save_file(&["inbetweendots"]),
        SEQUENCE,
    ];

    fn import(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let index = session.sequence_index(params)?;
        let dots =
            inbetween_format::read_dots(Path::new(params.string("path").unwrap_or_default()))?;
        let count = dots.len();
        session.sequence_mut(index)?.dots = dots;
        Ok(json!({ "sequence": index, "dots": count }))
    }

    fn export(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let index = session.sequence_index(params)?;
        let dots = &session.sequence(index)?.dots;
        inbetween_format::write_dots(Path::new(params.string("path").unwrap_or_default()), dots)?;
        Ok(json!({ "sequence": index, "dots": dots.len() }))
    }

    pub(super) fn specs() -> Vec<CommandSpec> {
        vec![
            CommandSpec {
                id: "dots.import",
                label: "Import Dots",
                menu: "Dots/Import...",
                shortcut: None,
                description: "Replace a sequence's dots with those from a .inbetweendots file.",
                params: IMPORT_PARAMS,
                enabled: has_sequence,
                disabled_reason: "the project has no sequence to put dots in",
                edits: true,
                run: import,
            },
            CommandSpec {
                id: "dots.export",
                label: "Export Dots",
                menu: "Dots/Export...",
                shortcut: None,
                description: "Write a sequence's dots to a .inbetweendots file.",
                params: EXPORT_PARAMS,
                enabled: has_sequence,
                disabled_reason: "the project has no sequence to export dots from",
                edits: false,
                run: export,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use crate::{CommandError, Registry, Session};
    use inbetween_doc::{ImageRef, Project, Sequence};
    use serde_json::{json, Value};

    fn session() -> Session {
        let mut s = Session::new();
        let seq = Sequence::new(ImageRef::new("a.png"), ImageRef::new("b.png"));
        s.set_project(
            Project {
                sequences: vec![seq],
                ..Project::default()
            },
            None,
        );
        s
    }

    fn run(r: &Registry, s: &mut Session, id: &str, p: Value) -> Result<Value, CommandError> {
        r.run(s, id, &p)
    }

    #[test]
    fn add_move_delete_round_trip_with_undo() {
        let r = Registry::standard();
        let mut s = session();
        let out = run(&r, &mut s, "dots.add", json!({"x": 10, "y": 20})).unwrap();
        assert_eq!(out["index"], 0);
        run(
            &r,
            &mut s,
            "dots.add",
            json!({"x": 50, "y": 60, "x2": 55.5, "y2": 61}),
        )
        .unwrap();
        let pairs = &s.project().sequences[0].dots.pairs;
        assert_eq!((pairs[0].first.x, pairs[0].second.x), (10.0, 10.0));
        assert_eq!((pairs[1].second.x, pairs[1].second.y), (55.5, 61.0));

        run(
            &r,
            &mut s,
            "dots.move",
            json!({"index": 0, "image": 2, "x": 12, "y": 22}),
        )
        .unwrap();
        let p0 = s.project().sequences[0].dots.pairs[0];
        assert_eq!((p0.first.x, p0.second.x, p0.second.y), (10.0, 12.0, 22.0));

        run(&r, &mut s, "dots.delete", json!({"index": 0})).unwrap();
        assert_eq!(s.project().sequences[0].dots.len(), 1);

        // Each edit was one undo step, newest first.
        for expected in [2, 2, 1, 0] {
            run(&r, &mut s, "edit.undo", json!({})).unwrap();
            assert_eq!(s.project().sequences[0].dots.len(), expected);
        }
        assert!(!s.can_undo());
        assert!(s.can_redo());
    }

    #[test]
    fn two_dots_cannot_share_a_spot() {
        let r = Registry::standard();
        let mut s = session();
        run(
            &r,
            &mut s,
            "dots.add",
            json!({"x": 10, "y": 10, "x2": 30, "y2": 30}),
        )
        .unwrap();
        let e = run(
            &r,
            &mut s,
            "dots.add",
            json!({"x": 10, "y": 10, "x2": 99, "y2": 99}),
        )
        .unwrap_err();
        assert!(e.to_string().contains("Image 1"), "{e}");
        let e = run(
            &r,
            &mut s,
            "dots.add",
            json!({"x": 70, "y": 70, "x2": 30, "y2": 30}),
        )
        .unwrap_err();
        assert!(e.to_string().contains("Image 2"), "{e}");
        run(&r, &mut s, "dots.add", json!({"x": 70, "y": 70})).unwrap();
        let revision = s.revision();
        let e = run(
            &r,
            &mut s,
            "dots.move",
            json!({"index": 1, "image": 1, "x": 10, "y": 10}),
        )
        .unwrap_err();
        assert!(matches!(e, CommandError::BadParams(_)));
        // Failed edits change nothing: no revision bump, no undo step.
        assert_eq!(s.revision(), revision);
        assert_eq!(s.project().sequences[0].dots.len(), 2);
        // Moving a dot onto its own position is fine.
        run(
            &r,
            &mut s,
            "dots.move",
            json!({"index": 0, "image": 1, "x": 10, "y": 10}),
        )
        .unwrap();
    }

    #[test]
    fn bad_indexes_and_sequences_are_errors() {
        let r = Registry::standard();
        let mut s = session();
        run(&r, &mut s, "dots.add", json!({"x": 1, "y": 1})).unwrap();
        for (id, p) in [
            ("dots.move", json!({"index": 5, "image": 1, "x": 0, "y": 0})),
            ("dots.delete", json!({"index": 1})),
            ("dots.delete", json!({"index": 0, "sequence": 3})),
            ("dots.add", json!({"x": 1, "y": 1, "sequence": 9})),
        ] {
            assert!(run(&r, &mut s, id, p.clone()).is_err(), "{id} {p}");
        }
        assert_eq!(s.project().sequences[0].dots.len(), 1);
    }

    #[test]
    fn drag_gestures_merge_into_one_undo_step() {
        let r = Registry::standard();
        let mut s = session();
        run(&r, &mut s, "dots.add", json!({"x": 10, "y": 10})).unwrap();
        run(&r, &mut s, "edit.beginGroup", json!({})).unwrap();
        for x in [11, 12, 13, 14] {
            run(
                &r,
                &mut s,
                "dots.move",
                json!({"index": 0, "image": 1, "x": x, "y": 10}),
            )
            .unwrap();
        }
        run(&r, &mut s, "edit.endGroup", json!({})).unwrap();
        assert_eq!(s.project().sequences[0].dots.pairs[0].first.x, 14.0);
        run(&r, &mut s, "edit.undo", json!({})).unwrap();
        assert_eq!(s.project().sequences[0].dots.pairs[0].first.x, 10.0);
        run(&r, &mut s, "edit.redo", json!({})).unwrap();
        assert_eq!(s.project().sequences[0].dots.pairs[0].first.x, 14.0);
        // Outside a group every edit is its own step again.
        run(
            &r,
            &mut s,
            "dots.move",
            json!({"index": 0, "image": 1, "x": 15, "y": 10}),
        )
        .unwrap();
        run(
            &r,
            &mut s,
            "dots.move",
            json!({"index": 0, "image": 1, "x": 16, "y": 10}),
        )
        .unwrap();
        run(&r, &mut s, "edit.undo", json!({})).unwrap();
        assert_eq!(s.project().sequences[0].dots.pairs[0].first.x, 15.0);
    }
}
