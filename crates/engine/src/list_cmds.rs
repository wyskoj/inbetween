//! The sequence list: `sequence.addBlank`, `sequence.add`, `sequence.delete`,
//! `sequence.duplicate`, `sequence.rename`, `sequence.visibility`, `sequence.reorder`,
//! `sequence.link`, `sequence.copyFrame`.
//!
//! In a sequence-type project the visible sequences play one after another, in list order.

use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{DotPair, DotSet, Sequence};
use inbetween_format::MAX_SEQUENCES;
use serde_json::{json, Value};

const SEQUENCE: ParamSpec = ParamSpec::new(
    "sequence",
    ParamKind::Integer,
    false,
    "Sequence number, starting at 0",
)
.range(0.0, 1_000_000.0)
.default_is("0");

const SEQUENCE_REQUIRED: ParamSpec = ParamSpec::new(
    "sequence",
    ParamKind::Integer,
    true,
    "Sequence number, starting at 0",
)
.range(0.0, 1_000_000.0);

const AFTER: ParamSpec = ParamSpec::new(
    "after",
    ParamKind::Integer,
    false,
    "Insert after this sequence number",
)
.range(0.0, 1_000_000.0)
.default_is("the end of the list");

const NAME: ParamSpec = ParamSpec::new("name", ParamKind::String, false, "A name for the sequence");

fn has_sequence(session: &Session) -> bool {
    session.has_sequences()
}

fn always(_: &Session) -> bool {
    true
}

fn can_link(session: &Session) -> bool {
    session.project().sequences.len() > 1
}

/// Put `seq` into the list after `after` (or at the end) and return its new index.
fn insert(session: &mut Session, after: Option<i64>, seq: Sequence) -> Result<usize, CommandError> {
    let count = session.project().sequences.len();
    if count >= MAX_SEQUENCES {
        return Err(CommandError::BadParams(format!(
            "a project holds at most {MAX_SEQUENCES} sequences"
        )));
    }
    let at = match after {
        Some(n) => {
            let n = n.max(0) as usize;
            if n >= count {
                return Err(CommandError::NoSuchSequence { index: n, count });
            }
            n + 1
        }
        None => count,
    };
    session.project_mut().sequences.insert(at, seq);
    Ok(at)
}

fn add_blank(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let seq = Sequence {
        name: params.string("name").unwrap_or_default().to_string(),
        ..Sequence::default()
    };
    let at = insert(session, params.integer("after"), seq)?;
    Ok(json!({ "sequence": at, "count": session.project().sequences.len() }))
}

fn delete(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let removed = session.project_mut().sequences.remove(index);
    Ok(json!({
        "deleted": index,
        "name": removed.name,
        "count": session.project().sequences.len(),
    }))
}

fn duplicate(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let mut copy = session.sequence(index)?.clone();
    if !copy.name.is_empty() {
        copy.name.push_str(" copy");
    }
    let at = insert(session, Some(index as i64), copy)?;
    Ok(json!({ "sequence": at, "count": session.project().sequences.len() }))
}

fn rename(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let name = params.string("name").unwrap_or_default().to_string();
    session.sequence_mut(index)?.name = name.clone();
    Ok(json!({ "sequence": index, "name": name }))
}

fn visibility(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    if params.boolean("solo").unwrap_or(false) {
        // Solo this sequence; soloing it again brings the others back.
        let only_this = session
            .project()
            .sequences
            .iter()
            .enumerate()
            .all(|(i, s)| s.visible == (i == index));
        for (i, s) in session.project_mut().sequences.iter_mut().enumerate() {
            s.visible = only_this || i == index;
        }
    } else {
        let seq = session.sequence_mut(index)?;
        seq.visible = params.boolean("visible").unwrap_or(!seq.visible);
    }
    let visible: Vec<bool> = session
        .project()
        .sequences
        .iter()
        .map(|s| s.visible)
        .collect();
    Ok(json!({ "sequence": index, "visible": visible }))
}

const MOVES: &[&str] = &["up", "down", "top", "bottom"];

fn reorder(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let last = session.project().sequences.len().saturating_sub(1);
    let target = match (params.string("move"), params.integer("to")) {
        (Some(_), Some(_)) => {
            return Err(CommandError::BadParams(
                "give either `move` or `to`, not both".into(),
            ))
        }
        (Some("up"), None) => index.saturating_sub(1),
        (Some("down"), None) => (index + 1).min(last),
        (Some("top"), None) => 0,
        (Some("bottom"), None) => last,
        (Some(other), None) => {
            return Err(CommandError::BadParams(format!(
                "unknown move `{other}` (up, down, top or bottom)"
            )))
        }
        (None, Some(n)) => {
            let n = n.max(0) as usize;
            if n > last {
                return Err(CommandError::NoSuchSequence {
                    index: n,
                    count: last + 1,
                });
            }
            n
        }
        (None, None) => {
            return Err(CommandError::BadParams(
                "give `move` (up, down, top, bottom) or `to` (a position)".into(),
            ))
        }
    };
    let seqs = &mut session.project_mut().sequences;
    let item = seqs.remove(index);
    seqs.insert(target, item);
    Ok(json!({ "from": index, "sequence": target }))
}

fn link(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let Some(previous) = index.checked_sub(1) else {
        return Err(CommandError::BadParams(
            "the first sequence has no previous sequence to link to".into(),
        ));
    };
    let (image, edit) = {
        let p = session.sequence(previous)?;
        (p.image2.clone(), p.edit2.clone())
    };
    let target = session.sequence_mut(index)?;
    target.image1 = image.clone();
    target.edit1 = edit;
    Ok(json!({ "sequence": index, "image1": image.0 }))
}

fn copy_frame(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let (from, to) = (
        params.integer("from_sequence").unwrap_or(0).max(0) as usize,
        params.integer("to_sequence").unwrap_or(0).max(0) as usize,
    );
    let (from_image, to_image) = (
        params.integer("from_image").unwrap_or(1),
        params.integer("to_image").unwrap_or(1),
    );
    let source = session.sequence(from)?;
    let (image, edit, positions) = if from_image == 1 {
        (
            source.image1.clone(),
            source.edit1.clone(),
            source.dots.first_positions(),
        )
    } else {
        (
            source.image2.clone(),
            source.edit2.clone(),
            source.dots.second_positions(),
        )
    };
    let target = session.sequence_mut(to)?;
    // The copied dots sit on the chosen image; the other image starts with them unmoved, to be
    // dragged into place.
    target.dots = DotSet::new(positions.into_iter().map(DotPair::fixed).collect());
    if to_image == 1 {
        target.image1 = image;
        target.edit1 = edit;
    } else {
        target.image2 = image;
        target.edit2 = edit;
    }
    Ok(json!({
        "from_sequence": from,
        "to_sequence": to,
        "to_image": to_image,
        "dots": target.dots.len(),
    }))
}

const BLANK_PARAMS: &[ParamSpec] = &[AFTER, NAME];

const DELETE_PARAMS: &[ParamSpec] = &[SEQUENCE_REQUIRED];

const DUPLICATE_PARAMS: &[ParamSpec] = &[SEQUENCE];

const RENAME_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new("name", ParamKind::String, true, "The new name"),
];

const VISIBILITY_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new(
        "visible",
        ParamKind::Bool,
        false,
        "true to show, false to hide; omit to toggle",
    ),
    ParamSpec::new(
        "solo",
        ParamKind::Bool,
        false,
        "Show only this sequence (toggle)",
    ),
];

const REORDER_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new("move", ParamKind::String, false, "up, down, top or bottom").one_of(MOVES),
    ParamSpec::new(
        "to",
        ParamKind::Integer,
        false,
        "The position to move it to, starting at 0 (instead of `move`)",
    )
    .range(0.0, 1_000_000.0),
];

const LINK_PARAMS: &[ParamSpec] = &[SEQUENCE_REQUIRED];

const COPY_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "from_sequence",
        ParamKind::Integer,
        true,
        "Sequence to copy from",
    )
    .range(0.0, 1_000_000.0),
    ParamSpec::new(
        "from_image",
        ParamKind::Integer,
        true,
        "Image to copy: 1 or 2",
    )
    .range(1.0, 2.0),
    ParamSpec::new(
        "to_sequence",
        ParamKind::Integer,
        true,
        "Sequence to copy into",
    )
    .range(0.0, 1_000_000.0),
    ParamSpec::new(
        "to_image",
        ParamKind::Integer,
        true,
        "Image slot to fill: 1 or 2",
    )
    .range(1.0, 2.0),
];

pub(crate) fn specs() -> Vec<CommandSpec> {
    #[allow(unused_mut)]
    let mut specs = vec![
        CommandSpec {
            id: "sequence.addBlank",
            label: "Add Blank Sequence",
            menu: "Sequence/Add Blank Sequence",
            shortcut: None,
            description: "Add an empty sequence (no images yet; set them with sequence.image).",
            params: BLANK_PARAMS,
            enabled: always,
            disabled_reason: "",
            edits: true,
            run: add_blank,
        },
        CommandSpec {
            id: "sequence.delete",
            label: "Delete Sequence",
            menu: "Sequence/Delete Sequence",
            shortcut: None,
            description: "Remove a sequence from the project.",
            params: DELETE_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: delete,
        },
        CommandSpec {
            id: "sequence.duplicate",
            label: "Duplicate Sequence",
            menu: "Sequence/Duplicate Sequence",
            shortcut: None,
            description: "Copy a sequence, with its dots and settings, to just after itself.",
            params: DUPLICATE_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: duplicate,
        },
        CommandSpec {
            id: "sequence.rename",
            label: "Rename Sequence",
            menu: "Sequence/Rename Sequence...",
            shortcut: None,
            description: "Give a sequence a name.",
            params: RENAME_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: rename,
        },
        CommandSpec {
            id: "sequence.visibility",
            label: "Show or Hide Sequence",
            menu: "Sequence/Show or Hide Sequence",
            shortcut: None,
            description: "Show or hide a sequence (hidden ones are skipped by playback and export). With `solo`, hide every other sequence; solo again to show them all.",
            params: VISIBILITY_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: visibility,
        },
        CommandSpec {
            id: "sequence.reorder",
            label: "Move Sequence",
            menu: "Sequence/Move Sequence",
            shortcut: None,
            description: "Move a sequence up, down, to the top or to the bottom of the list, or to a position.",
            params: REORDER_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: reorder,
        },
        CommandSpec {
            id: "sequence.link",
            label: "Link to Previous Sequence",
            menu: "Sequence/Link to Previous",
            shortcut: None,
            description: "Make Image 1 of a sequence the same file as the previous sequence's Image 2, for a seamless join.",
            params: LINK_PARAMS,
            enabled: can_link,
            disabled_reason: "the project needs at least two sequences",
            edits: true,
            run: link,
        },
        CommandSpec {
            id: "sequence.copyFrame",
            label: "Copy Image With Dots",
            menu: "",
            shortcut: None,
            description: "Copy a source image and its dots from one sequence into an image slot of another. The other image's dots start unmoved.",
            params: COPY_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: copy_frame,
        },
    ];
    #[cfg(not(target_arch = "wasm32"))]
    specs.extend(files::specs());
    specs
}

#[cfg(not(target_arch = "wasm32"))]
mod files {
    use super::*;
    use inbetween_doc::ImageRef;

    const ADD_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "image2",
            ParamKind::Path,
            true,
            "The new sequence's Image 2 (PNG, JPEG, BMP, TIFF, TGA, GIF or PSD)",
        )
        .open_file(inbetween_codecs::IMAGE_EXTENSIONS),
        AFTER,
        NAME,
    ];

    /// A continued sequence: the one it follows ends where this one starts.
    fn add(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let count = session.project().sequences.len();
        let after = match params.integer("after") {
            Some(n) => n.max(0) as usize,
            None => count.saturating_sub(1),
        };
        let previous = if count == 0 {
            None
        } else {
            Some(session.sequence(after)?.clone())
        };
        let path = std::path::absolute(params.string("image2").unwrap_or_default())
            .map_err(|e| CommandError::Io(e.to_string()))?
            .display()
            .to_string();
        session.load_image(&path, 2, count)?; // refuse a bad file now
        let mut seq = match previous {
            Some(p) => Sequence {
                image1: p.image2.clone(),
                edit1: p.edit2.clone(),
                frames: p.frames,
                morph: p.morph,
                fit: p.fit,
                mesh: p.mesh,
                drop_corner_tris: p.drop_corner_tris,
                ..Sequence::default()
            },
            None => Sequence::default(),
        };
        seq.image2 = ImageRef(path);
        seq.name = params.string("name").unwrap_or_default().to_string();
        let at = insert(session, (count > 0).then_some(after as i64), seq)?;
        Ok(json!({ "sequence": at, "count": session.project().sequences.len() }))
    }

    pub(super) fn specs() -> Vec<CommandSpec> {
        vec![CommandSpec {
            id: "sequence.add",
            label: "Add Continued Sequence",
            menu: "Sequence/Add Continued Sequence...",
            shortcut: None,
            description: "Add a sequence that continues the previous one: its Image 1 is the previous sequence's Image 2, and `image2` is the new target.",
            params: ADD_PARAMS,
            enabled: always,
            disabled_reason: "",
            edits: true,
            run: add,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Registry;
    use inbetween_doc::{ImageRef, Project};

    fn session(n: usize) -> Session {
        let mut s = Session::new();
        let sequences = (0..n)
            .map(|i| Sequence {
                name: format!("s{i}"),
                image1: ImageRef(format!("a{i}.png")),
                image2: ImageRef(format!("b{i}.png")),
                ..Sequence::default()
            })
            .collect();
        s.set_project(
            Project {
                sequences,
                ..Project::default()
            },
            None,
        );
        s
    }

    fn names(s: &Session) -> Vec<String> {
        s.project()
            .sequences
            .iter()
            .map(|q| q.name.clone())
            .collect()
    }

    fn run(s: &mut Session, id: &str, p: Value) -> Result<Value, CommandError> {
        Registry::standard().run(s, id, &p)
    }

    #[test]
    fn blank_duplicate_rename_delete() {
        let mut s = session(2);
        let v = run(
            &mut s,
            "sequence.addBlank",
            json!({"after": 0, "name": "mid"}),
        )
        .unwrap();
        assert_eq!(v["sequence"], 1);
        assert_eq!(names(&s), ["s0", "mid", "s1"]);
        run(&mut s, "sequence.duplicate", json!({"sequence": 2})).unwrap();
        assert_eq!(names(&s), ["s0", "mid", "s1", "s1 copy"]);
        run(
            &mut s,
            "sequence.rename",
            json!({"sequence": 0, "name": "first"}),
        )
        .unwrap();
        run(&mut s, "sequence.delete", json!({"sequence": 1})).unwrap();
        assert_eq!(names(&s), ["first", "s1", "s1 copy"]);
        assert!(run(&mut s, "sequence.delete", json!({"sequence": 3})).is_err());
        assert!(run(&mut s, "sequence.delete", json!({})).is_err()); // must say which
        assert!(run(&mut s, "sequence.addBlank", json!({"after": 9})).is_err());
        // Everything above is undoable.
        assert!(s.can_undo());
    }

    #[test]
    fn a_project_can_be_emptied_and_refilled() {
        let mut s = session(1);
        run(&mut s, "sequence.delete", json!({"sequence": 0})).unwrap();
        assert!(!s.has_sequences());
        run(&mut s, "sequence.addBlank", json!({})).unwrap();
        assert_eq!(s.project().sequences.len(), 1);
    }

    #[test]
    fn reorder_moves_and_rejects_nonsense() {
        let mut s = session(4);
        run(
            &mut s,
            "sequence.reorder",
            json!({"sequence": 0, "move": "down"}),
        )
        .unwrap();
        assert_eq!(names(&s), ["s1", "s0", "s2", "s3"]);
        run(
            &mut s,
            "sequence.reorder",
            json!({"sequence": 3, "move": "top"}),
        )
        .unwrap();
        assert_eq!(names(&s), ["s3", "s1", "s0", "s2"]);
        run(
            &mut s,
            "sequence.reorder",
            json!({"sequence": 0, "move": "bottom"}),
        )
        .unwrap();
        run(
            &mut s,
            "sequence.reorder",
            json!({"sequence": 0, "move": "up"}),
        )
        .unwrap();
        assert_eq!(names(&s), ["s1", "s0", "s2", "s3"]);
        run(&mut s, "sequence.reorder", json!({"sequence": 3, "to": 1})).unwrap();
        assert_eq!(names(&s), ["s1", "s3", "s0", "s2"]);
        assert!(run(&mut s, "sequence.reorder", json!({"sequence": 0, "to": 4})).is_err());
        assert!(run(&mut s, "sequence.reorder", json!({"sequence": 0})).is_err());
        assert!(run(
            &mut s,
            "sequence.reorder",
            json!({"sequence": 0, "to": 1, "move": "up"})
        )
        .is_err());
        assert_eq!(names(&s), ["s1", "s3", "s0", "s2"]); // failed edits change nothing
    }

    #[test]
    fn visibility_toggles_and_solos() {
        let mut s = session(3);
        let vis = |s: &Session| -> Vec<bool> {
            s.project().sequences.iter().map(|q| q.visible).collect()
        };
        run(&mut s, "sequence.visibility", json!({"sequence": 1})).unwrap();
        assert_eq!(vis(&s), [true, false, true]);
        run(
            &mut s,
            "sequence.visibility",
            json!({"sequence": 1, "visible": true}),
        )
        .unwrap();
        run(
            &mut s,
            "sequence.visibility",
            json!({"sequence": 2, "solo": true}),
        )
        .unwrap();
        assert_eq!(vis(&s), [false, false, true]);
        run(
            &mut s,
            "sequence.visibility",
            json!({"sequence": 2, "solo": true}),
        )
        .unwrap();
        assert_eq!(vis(&s), [true, true, true]);
        assert_eq!(s.project().total_frames(), 90);
        run(
            &mut s,
            "sequence.visibility",
            json!({"sequence": 0, "visible": false}),
        )
        .unwrap();
        assert_eq!(s.project().total_frames(), 60);
    }

    #[test]
    fn link_joins_images_and_the_first_cannot_link() {
        let mut s = session(3);
        run(&mut s, "sequence.link", json!({"sequence": 2})).unwrap();
        assert_eq!(s.project().sequences[2].image1.0, "b1.png");
        assert!(run(&mut s, "sequence.link", json!({"sequence": 0})).is_err());
        let mut one = session(1);
        assert!(run(&mut one, "sequence.link", json!({"sequence": 0})).is_err());
    }

    #[test]
    fn copy_frame_carries_the_image_and_its_dots() {
        let mut s = session(2);
        s.project_mut().sequences[0].dots = DotSet::new(vec![DotPair::new(
            inbetween_geom::Point::new(1.0, 2.0),
            inbetween_geom::Point::new(3.0, 4.0),
        )]);
        let p = json!({"from_sequence": 0, "from_image": 2, "to_sequence": 1, "to_image": 1});
        run(&mut s, "sequence.copyFrame", p).unwrap();
        let t = &s.project().sequences[1];
        assert_eq!(t.image1.0, "b0.png");
        assert_eq!(t.dots.pairs[0].first, inbetween_geom::Point::new(3.0, 4.0));
        assert_eq!(t.dots.pairs[0].second, t.dots.pairs[0].first);
        let bad = json!({"from_sequence": 5, "from_image": 1, "to_sequence": 0, "to_image": 1});
        assert!(run(&mut s, "sequence.copyFrame", bad).is_err());
    }

    #[test]
    fn list_commands_need_sequences_where_it_makes_sense() {
        let mut empty = Session::new();
        let r = Registry::standard();
        assert!(r.is_enabled("sequence.addBlank", &empty));
        assert!(!r.is_enabled("sequence.delete", &empty));
        assert!(run(&mut empty, "sequence.delete", json!({"sequence": 0})).is_err());
        assert!(!r.is_enabled("sequence.link", &session(1)));
        assert!(r.is_enabled("sequence.link", &session(2)));
    }
}
