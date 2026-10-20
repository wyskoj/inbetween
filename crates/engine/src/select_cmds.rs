//! Selecting and transforming dots, the dot clipboard, and clearing: `dots.select`,
//! `dots.selectAll`, `dots.selectNone`, `dots.transform`, `dots.nudge`, `dots.clearAll`,
//! `edit.copy`, `edit.cut`, `edit.paste`, `edit.delete`.

use crate::dots_cmds::taken;
use crate::{CommandError, CommandSpec, DotClip, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{Line, Sequence};
use inbetween_geom::Point;
use serde_json::{json, Value};
use std::collections::BTreeSet;

const COORD_LIMIT: f64 = 10_000_000.0;
const MAX_INDEX: f64 = 1_000_000.0;
/// How many 10 px steps a paste may try before giving up.
const MAX_PASTE_STEPS: u32 = 1000;

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

const INDICES: ParamSpec = ParamSpec::new(
    "indices",
    ParamKind::IntegerList,
    false,
    "Dot numbers, counting from 0",
)
.range(0.0, 100_000.0);

const fn amount(name: &'static str, description: &'static str) -> ParamSpec {
    ParamSpec::new(name, ParamKind::Number, false, description).range(-COORD_LIMIT, COORD_LIMIT)
}

fn bad(message: impl Into<String>) -> CommandError {
    CommandError::BadParams(message.into())
}

fn has_sequence(session: &Session) -> bool {
    session.has_sequences()
}

fn has_selection(session: &Session) -> bool {
    !session.selection().is_empty()
}

fn has_clip(session: &Session) -> bool {
    session.has_sequences() && session.clipboard().is_some()
}

fn which_image(params: &Params) -> u8 {
    if params.integer("image") == Some(2) {
        2
    } else {
        1
    }
}

fn dot_index(i: i64, count: usize) -> Result<usize, CommandError> {
    usize::try_from(i)
        .ok()
        .filter(|i| *i < count)
        .ok_or_else(|| {
            bad(format!(
                "there is no dot {i} (this sequence has {count}; numbering starts at 0)"
            ))
        })
}

// ---- selecting ----------------------------------------------------------------------------

const SELECT_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "mode",
        ParamKind::String,
        false,
        "set replaces the selection, add and subtract change it, toggle flips each dot",
    )
    .one_of(&["set", "add", "subtract", "toggle"])
    .default_is("set"),
    INDICES,
    ParamSpec::new(
        "rect",
        ParamKind::NumberList,
        false,
        "A marquee: x0, y0, x1, y1 in output pixels; selects the dots of `image` inside it",
    )
    .range(4.0, 4.0),
    ParamSpec::new(
        "line",
        ParamKind::Integer,
        false,
        "A line number; selects every dot connected to it",
    )
    .range(0.0, MAX_INDEX),
    IMAGE,
    SEQUENCE,
];

fn select(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let seq_index = session.sequence_index(params)?;
    let image = which_image(params);
    let seq = session.sequence(seq_index)?;
    let count = seq.dots.len();
    let mut hit = BTreeSet::new();
    for i in params.integers("indices").unwrap_or_default() {
        hit.insert(dot_index(i, count)?);
    }
    if let Some(r) = params.numbers("rect") {
        if let [x0, y0, x1, y1] = r[..] {
            let (lo_x, hi_x) = (x0.min(x1), x0.max(x1));
            let (lo_y, hi_y) = (y0.min(y1), y0.max(y1));
            for (i, p) in seq.dots.pairs.iter().enumerate() {
                if p.has_end(image) {
                    let q = p.end(image);
                    if q.x >= lo_x && q.x <= hi_x && q.y >= lo_y && q.y <= hi_y {
                        hit.insert(i);
                    }
                }
            }
        }
    }
    if let Some(l) = params.integer("line") {
        let line = usize::try_from(l)
            .ok()
            .and_then(|l| seq.dots.lines.get(l))
            .ok_or_else(|| bad(format!("there is no line {l}")))?;
        hit.extend(seq.dots.connected(line.a as usize));
        hit.extend(seq.dots.connected(line.b as usize));
    }
    let mut current = if session.selection_sequence() == seq_index {
        session.selection().clone()
    } else {
        BTreeSet::new()
    };
    match params.string("mode").unwrap_or("set") {
        "add" => current.extend(hit),
        "subtract" => {
            for i in hit {
                current.remove(&i);
            }
        }
        "toggle" => {
            for i in hit {
                if !current.remove(&i) {
                    current.insert(i);
                }
            }
        }
        _ => current = hit,
    }
    session.set_selection(seq_index, current);
    Ok(selection_json(session, seq_index))
}

fn selection_json(session: &Session, seq_index: usize) -> Value {
    json!({
        "sequence": seq_index,
        "selected": session.selection().iter().collect::<Vec<_>>(),
    })
}

const SEQ_ONLY: &[ParamSpec] = &[SEQUENCE];

fn select_all(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let seq_index = session.sequence_index(params)?;
    let count = session.sequence(seq_index)?.dots.len();
    session.set_selection(seq_index, (0..count).collect());
    Ok(selection_json(session, seq_index))
}

fn select_none(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let seq_index = session.sequence_index(params)?;
    session.set_selection(seq_index, BTreeSet::new());
    Ok(selection_json(session, seq_index))
}

// ---- transforming -------------------------------------------------------------------------

const OPS: &[&str] = &["move", "scale", "rotate", "skew", "flipH", "flipV"];

const TRANSFORM_PARAMS: &[ParamSpec] = &[
    ParamSpec::new("op", ParamKind::String, true, "What to do to the dots").one_of(OPS),
    IMAGE,
    INDICES,
    amount("dx", "move: horizontal distance in pixels").default_is("0"),
    amount("dy", "move: vertical distance in pixels").default_is("0"),
    ParamSpec::new("sx", ParamKind::Number, false, "scale: horizontal factor")
        .range(-1000.0, 1000.0)
        .default_is("1"),
    ParamSpec::new("sy", ParamKind::Number, false, "scale: vertical factor")
        .range(-1000.0, 1000.0)
        .default_is("sx"),
    ParamSpec::new(
        "angle",
        ParamKind::Number,
        false,
        "rotate: degrees, clockwise on screen",
    )
    .range(-3600.0, 3600.0)
    .default_is("0"),
    ParamSpec::new(
        "kx",
        ParamKind::Number,
        false,
        "skew: horizontal shear in degrees",
    )
    .range(-89.0, 89.0)
    .default_is("0"),
    ParamSpec::new(
        "ky",
        ParamKind::Number,
        false,
        "skew: vertical shear in degrees",
    )
    .range(-89.0, 89.0)
    .default_is("0"),
    amount("cx", "Pivot x").default_is("the middle of the dots' bounding box"),
    amount("cy", "Pivot y").default_is("the middle of the dots' bounding box"),
    SEQUENCE,
];

const PIVOT: [ParamSpec; 2] = [
    amount("cx", "Pivot x").default_is("the middle of the dots' bounding box"),
    amount("cy", "Pivot y").default_is("the middle of the dots' bounding box"),
];

const SCALE_PARAMS: &[ParamSpec] = &[
    ParamSpec::new("sx", ParamKind::Number, true, "Horizontal factor").range(-1000.0, 1000.0),
    ParamSpec::new("sy", ParamKind::Number, false, "Vertical factor")
        .range(-1000.0, 1000.0)
        .default_is("sx"),
    PIVOT[0],
    PIVOT[1],
    IMAGE,
    INDICES,
    SEQUENCE,
];

const ROTATE_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "angle",
        ParamKind::Number,
        true,
        "Degrees, clockwise on screen",
    )
    .range(-3600.0, 3600.0),
    PIVOT[0],
    PIVOT[1],
    IMAGE,
    INDICES,
    SEQUENCE,
];

const SKEW_PARAMS: &[ParamSpec] = &[
    ParamSpec::new("kx", ParamKind::Number, true, "Horizontal shear in degrees").range(-89.0, 89.0),
    ParamSpec::new("ky", ParamKind::Number, false, "Vertical shear in degrees")
        .range(-89.0, 89.0)
        .default_is("0"),
    PIVOT[0],
    PIVOT[1],
    IMAGE,
    INDICES,
    SEQUENCE,
];

const FLIP_PARAMS: &[ParamSpec] = &[PIVOT[0], PIVOT[1], IMAGE, INDICES, SEQUENCE];

const NUDGE_PARAMS: &[ParamSpec] = &[
    amount("dx", "Horizontal distance in pixels").default_is("0"),
    amount("dy", "Vertical distance in pixels").default_is("0"),
    IMAGE,
    INDICES,
    SEQUENCE,
];

/// The dots a command acts on: `indices`, else the selection (when it is in this sequence).
fn targets(
    session: &Session,
    params: &Params,
    seq_index: usize,
    count: usize,
) -> Result<Vec<usize>, CommandError> {
    let mut v: Vec<usize> = match params.integers("indices") {
        Some(list) => list
            .into_iter()
            .map(|i| dot_index(i, count))
            .collect::<Result<_, _>>()?,
        None if session.selection_sequence() == seq_index => session
            .selection()
            .iter()
            .copied()
            .filter(|i| *i < count)
            .collect(),
        None => Vec::new(),
    };
    v.sort_unstable();
    v.dedup();
    if v.is_empty() {
        return Err(bad(
            "no dots are selected; select some first or pass `indices`",
        ));
    }
    Ok(v)
}

/// Move the `image` end of each target through `map`. Refuses, changing nothing, if a result
/// is not finite or too large, or two dots would end up on the same spot of that image.
pub(crate) fn move_ends(
    seq: &mut Sequence,
    which: u8,
    targets: &[usize],
    map: &dyn Fn(Point) -> Point,
) -> Result<usize, CommandError> {
    let mut moved = Vec::with_capacity(targets.len());
    for &i in targets {
        let Some(pair) = seq.dots.pairs.get(i).filter(|p| p.has_end(which)) else {
            continue;
        };
        let q = map(pair.end(which));
        if !(q.is_finite() && q.x.abs() <= COORD_LIMIT && q.y.abs() <= COORD_LIMIT) {
            return Err(bad(
                "that would move a dot to an invalid or absurdly distant position",
            ));
        }
        moved.push((i, q));
    }
    if moved.is_empty() {
        return Err(bad(format!(
            "none of those dots is on Image {which}; pass the other `image`"
        )));
    }
    let clash = || {
        bad(format!(
            "that would put two dots on the same spot of Image {which}; nothing was changed"
        ))
    };
    let moving: BTreeSet<usize> = moved.iter().map(|(i, _)| *i).collect();
    for (_, q) in &moved {
        let blocked = seq
            .dots
            .pairs
            .iter()
            .enumerate()
            .any(|(j, p)| !moving.contains(&j) && p.has_end(which) && p.end(which) == *q);
        if blocked {
            return Err(clash());
        }
    }
    let mut keys: Vec<(u64, u64)> = moved
        .iter()
        .map(|(_, q)| (q.x.to_bits(), q.y.to_bits()))
        .collect();
    keys.sort_unstable();
    if keys.windows(2).any(|w| w[0] == w[1]) {
        return Err(clash());
    }
    let n = moved.len();
    for (i, q) in moved {
        if let Some(p) = seq.dots.pairs.get_mut(i) {
            p.set_end(which, q);
        }
    }
    Ok(n)
}

fn pivot(seq: &Sequence, which: u8, targets: &[usize], params: &Params) -> Point {
    let pts: Vec<Point> = targets
        .iter()
        .filter_map(|&i| seq.dots.pairs.get(i))
        .filter(|p| p.has_end(which))
        .map(|p| p.end(which))
        .collect();
    let (mid_x, mid_y) = match inbetween_geom::Bounds::from_points(pts) {
        Some(b) => ((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0),
        None => (0.0, 0.0),
    };
    Point::new(
        params.number("cx").unwrap_or(mid_x),
        params.number("cy").unwrap_or(mid_y),
    )
}

fn transform(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let op = params.string("op").unwrap_or("move");
    transform_op(session, params, op)
}

fn scale(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    transform_op(session, params, "scale")
}

fn rotate(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    transform_op(session, params, "rotate")
}

fn skew(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    transform_op(session, params, "skew")
}

fn flip_h(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    transform_op(session, params, "flipH")
}

fn flip_v(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    transform_op(session, params, "flipV")
}

fn transform_op(session: &mut Session, params: &Params, op: &str) -> Result<Value, CommandError> {
    let seq_index = session.sequence_index(params)?;
    let which = which_image(params);
    let count = session.sequence(seq_index)?.dots.len();
    let list = targets(session, params, seq_index, count)?;
    let seq = session.sequence_mut(seq_index)?;
    let c = pivot(seq, which, &list, params);
    let num = |name: &str, default: f64| params.number(name).unwrap_or(default);
    let map: Box<dyn Fn(Point) -> Point> = match op {
        "move" => {
            let (dx, dy) = (num("dx", 0.0), num("dy", 0.0));
            Box::new(move |p| Point::new(p.x + dx, p.y + dy))
        }
        "scale" => {
            let sx = num("sx", 1.0);
            let sy = num("sy", sx);
            Box::new(move |p| Point::new(c.x + (p.x - c.x) * sx, c.y + (p.y - c.y) * sy))
        }
        "rotate" => {
            let (sin, cos) = num("angle", 0.0).to_radians().sin_cos();
            Box::new(move |p| {
                let (x, y) = (p.x - c.x, p.y - c.y);
                Point::new(c.x + x * cos - y * sin, c.y + x * sin + y * cos)
            })
        }
        "skew" => {
            let (tx, ty) = (
                num("kx", 0.0).to_radians().tan(),
                num("ky", 0.0).to_radians().tan(),
            );
            Box::new(move |p| Point::new(p.x + tx * (p.y - c.y), p.y + ty * (p.x - c.x)))
        }
        "flipH" => Box::new(move |p| Point::new(2.0 * c.x - p.x, p.y)),
        "flipV" => Box::new(move |p| Point::new(p.x, 2.0 * c.y - p.y)),
        other => return Err(bad(format!("unknown transform `{other}`"))),
    };
    let moved = move_ends(seq, which, &list, map.as_ref())?;
    Ok(json!({ "sequence": seq_index, "image": which, "op": op, "moved": moved }))
}

fn nudge(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let seq_index = session.sequence_index(params)?;
    let which = which_image(params);
    let count = session.sequence(seq_index)?.dots.len();
    let list = targets(session, params, seq_index, count)?;
    let (dx, dy) = (
        params.number("dx").unwrap_or(0.0),
        params.number("dy").unwrap_or(0.0),
    );
    let seq = session.sequence_mut(seq_index)?;
    let moved = move_ends(seq, which, &list, &|p| Point::new(p.x + dx, p.y + dy))?;
    Ok(json!({ "sequence": seq_index, "image": which, "moved": moved }))
}

// ---- clipboard and deleting ---------------------------------------------------------------

/// The dots of `chosen` with the lines among them, renumbered from 0.
fn clip_of(seq: &Sequence, chosen: &BTreeSet<usize>) -> DotClip {
    let order: Vec<usize> = chosen.iter().copied().collect();
    let new_index = |old: u32| order.iter().position(|&o| o == old as usize);
    DotClip {
        pairs: order
            .iter()
            .filter_map(|&i| seq.dots.pairs.get(i).copied())
            .collect(),
        lines: seq
            .dots
            .lines
            .iter()
            .filter_map(|l| {
                Some(Line {
                    a: u32::try_from(new_index(l.a)?).ok()?,
                    b: u32::try_from(new_index(l.b)?).ok()?,
                })
            })
            .collect(),
    }
}

fn selected_in(
    session: &Session,
    params: &Params,
) -> Result<(usize, BTreeSet<usize>), CommandError> {
    let seq_index = session.sequence_index(params)?;
    let count = session.sequence(seq_index)?.dots.len();
    let chosen: BTreeSet<usize> = if session.selection_sequence() == seq_index {
        session
            .selection()
            .iter()
            .copied()
            .filter(|i| *i < count)
            .collect()
    } else {
        BTreeSet::new()
    };
    if chosen.is_empty() {
        return Err(bad("no dots are selected"));
    }
    Ok((seq_index, chosen))
}

fn remove_all(seq: &mut Sequence, chosen: &BTreeSet<usize>) {
    for &i in chosen.iter().rev() {
        seq.dots.remove(i);
    }
}

fn copy(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let (seq_index, chosen) = selected_in(session, params)?;
    let clip = clip_of(session.sequence(seq_index)?, &chosen);
    let n = clip.pairs.len();
    session.set_clipboard(Some(clip));
    Ok(json!({ "sequence": seq_index, "copied": n }))
}

fn cut(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let (seq_index, chosen) = selected_in(session, params)?;
    let clip = clip_of(session.sequence(seq_index)?, &chosen);
    let n = clip.pairs.len();
    remove_all(session.sequence_mut(seq_index)?, &chosen);
    session.set_clipboard(Some(clip));
    session.keep_selection(seq_index, BTreeSet::new());
    Ok(json!({ "sequence": seq_index, "cut": n }))
}

fn delete_selected(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let (seq_index, chosen) = selected_in(session, params)?;
    remove_all(session.sequence_mut(seq_index)?, &chosen);
    session.keep_selection(seq_index, BTreeSet::new());
    Ok(json!({ "sequence": seq_index, "deleted": chosen.len() }))
}

fn paste(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let seq_index = session.sequence_index(params)?;
    let clip = session
        .clipboard()
        .cloned()
        .ok_or_else(|| bad("the dot clipboard is empty; copy or cut some dots first"))?;
    let seq = session.sequence_mut(seq_index)?;
    let mut offset = None;
    for step in 0..=MAX_PASTE_STEPS {
        let d = f64::from(step) * 10.0;
        let clear = clip.pairs.iter().all(|p| {
            [1u8, 2].iter().all(|&w| {
                !p.has_end(w) || {
                    let q = p.end(w);
                    !taken(seq, w, Point::new(q.x + d, q.y + d), None)
                }
            })
        });
        if clear {
            offset = Some(d);
            break;
        }
    }
    let d = offset.ok_or_else(|| bad("there is no free spot to paste these dots"))?;
    let base = seq.dots.len();
    for p in &clip.pairs {
        let mut q = *p;
        q.first = Point::new(p.first.x + d, p.first.y + d);
        q.second = Point::new(p.second.x + d, p.second.y + d);
        seq.dots.pairs.push(q);
    }
    for l in &clip.lines {
        seq.dots.connect(base + l.a as usize, base + l.b as usize);
    }
    seq.validate()?;
    let pasted: BTreeSet<usize> = (base..seq.dots.len()).collect();
    let n = pasted.len();
    session.keep_selection(seq_index, pasted);
    Ok(json!({ "sequence": seq_index, "pasted": n, "offset": d }))
}

fn clear_all(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let seq_index = session.sequence_index(params)?;
    let seq = session.sequence_mut(seq_index)?;
    let n = seq.dots.len();
    seq.dots.pairs.clear();
    seq.dots.lines.clear();
    Ok(json!({ "sequence": seq_index, "deleted": n }))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "dots.select",
            label: "Select Dots",
            menu: "",
            shortcut: None,
            description: "Select dots by number, by marquee rectangle on one image, or by line (its whole connected group).",
            params: SELECT_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: false,
            run: select,
        },
        CommandSpec {
            id: "dots.selectAll",
            label: "Select All Dots",
            menu: "Edit/Select All Dots",
            shortcut: Some("Ctrl+A"),
            description: "Select every dot of a sequence.",
            params: SEQ_ONLY,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: false,
            run: select_all,
        },
        CommandSpec {
            id: "dots.selectNone",
            label: "Select No Dots",
            menu: "Edit/Select No Dots",
            shortcut: None,
            description: "Clear the dot selection.",
            params: SEQ_ONLY,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: false,
            run: select_none,
        },
        CommandSpec {
            id: "dots.transform",
            label: "Transform Dots",
            menu: "",
            shortcut: None,
            description: "Move, scale, rotate, skew or flip the selected dots on one image about a pivot.",
            params: TRANSFORM_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: true,
            run: transform,
        },
        CommandSpec {
            id: "dots.scale",
            label: "Scale Dots",
            menu: "Dots/Scale...",
            shortcut: Some("Ctrl+H"),
            description: "Scale the selected dots on one image about a pivot.",
            params: SCALE_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: true,
            run: scale,
        },
        CommandSpec {
            id: "dots.rotate",
            label: "Rotate Dots",
            menu: "Dots/Rotate...",
            shortcut: Some("Ctrl+R"),
            description: "Rotate the selected dots on one image about a pivot.",
            params: ROTATE_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: true,
            run: rotate,
        },
        CommandSpec {
            id: "dots.skew",
            label: "Skew Dots",
            menu: "Dots/Skew...",
            shortcut: Some("Ctrl+K"),
            description: "Shear the selected dots on one image about a pivot.",
            params: SKEW_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: true,
            run: skew,
        },
        CommandSpec {
            id: "dots.flipH",
            label: "Flip Dots Horizontally",
            menu: "Dots/Flip Horizontally",
            shortcut: None,
            description: "Mirror the selected dots left to right on one image.",
            params: FLIP_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: true,
            run: flip_h,
        },
        CommandSpec {
            id: "dots.flipV",
            label: "Flip Dots Vertically",
            menu: "Dots/Flip Vertically",
            shortcut: None,
            description: "Mirror the selected dots top to bottom on one image.",
            params: FLIP_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: true,
            run: flip_v,
        },
        CommandSpec {
            id: "dots.nudge",
            label: "Nudge Dots",
            menu: "",
            shortcut: None,
            description: "Move the selected dots on one image by a small step (the arrow keys use 1 px, 10 px with Ctrl).",
            params: NUDGE_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: true,
            run: nudge,
        },
        CommandSpec {
            id: "dots.clearAll",
            label: "Clear All Dots",
            menu: "Dots/Clear All",
            shortcut: None,
            description: "Delete every dot and line of a sequence.",
            params: SEQ_ONLY,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence",
            edits: true,
            run: clear_all,
        },
        CommandSpec {
            id: "edit.copy",
            label: "Copy Dots",
            menu: "Edit/Copy",
            shortcut: Some("Ctrl+C"),
            description: "Copy the selected dots, and the lines between them, to the dot clipboard.",
            params: SEQ_ONLY,
            enabled: has_selection,
            disabled_reason: "no dots are selected",
            edits: false,
            run: copy,
        },
        CommandSpec {
            id: "edit.cut",
            label: "Cut Dots",
            menu: "Edit/Cut",
            shortcut: Some("Ctrl+X"),
            description: "Move the selected dots to the dot clipboard.",
            params: SEQ_ONLY,
            enabled: has_selection,
            disabled_reason: "no dots are selected",
            edits: true,
            run: cut,
        },
        CommandSpec {
            id: "edit.paste",
            label: "Paste Dots",
            menu: "Edit/Paste",
            shortcut: Some("Ctrl+V"),
            description: "Paste the clipboard dots, shifted by 10 px steps until none lands on an existing dot, and select them.",
            params: SEQ_ONLY,
            enabled: has_clip,
            disabled_reason: "the dot clipboard is empty",
            edits: true,
            run: paste,
        },
        CommandSpec {
            id: "edit.delete",
            label: "Delete Selected Dots",
            menu: "Edit/Delete",
            shortcut: Some("Delete"),
            description: "Delete the selected dots and the lines that touch them.",
            params: SEQ_ONLY,
            enabled: has_selection,
            disabled_reason: "no dots are selected",
            edits: true,
            run: delete_selected,
        },
    ]
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

    fn run(s: &mut Session, id: &str, p: Value) -> Result<Value, CommandError> {
        Registry::standard().run(s, id, &p)
    }

    fn with_dots(n: i32) -> Session {
        let mut s = session();
        for i in 0..n {
            run(
                &mut s,
                "dots.add",
                json!({"x": 10 * i, "y": 10 * (i % 2), "x2": 10 * i, "y2": 10 * (i % 2)}),
            )
            .unwrap();
        }
        s
    }

    fn firsts(s: &Session) -> Vec<(f64, f64)> {
        s.project().sequences[0]
            .dots
            .pairs
            .iter()
            .map(|p| (p.first.x, p.first.y))
            .collect()
    }

    #[test]
    fn select_modes_rect_and_lines() {
        let mut s = with_dots(4);
        let sel = |s: &Session| s.selection().iter().copied().collect::<Vec<_>>();
        run(&mut s, "dots.select", json!({"indices": [0, 2]})).unwrap();
        assert_eq!(sel(&s), [0, 2]);
        run(
            &mut s,
            "dots.select",
            json!({"mode": "add", "indices": [3]}),
        )
        .unwrap();
        run(
            &mut s,
            "dots.select",
            json!({"mode": "toggle", "indices": [0, 1]}),
        )
        .unwrap();
        assert_eq!(sel(&s), [1, 2, 3]);
        run(
            &mut s,
            "dots.select",
            json!({"mode": "subtract", "indices": [2]}),
        )
        .unwrap();
        assert_eq!(sel(&s), [1, 3]);
        run(&mut s, "dots.select", json!({"rect": [25, -5, -5, 15]})).unwrap();
        assert_eq!(sel(&s), [0, 1, 2]);
        run(&mut s, "dots.selectAll", json!({})).unwrap();
        assert_eq!(sel(&s).len(), 4);
        run(&mut s, "dots.selectNone", json!({})).unwrap();
        assert!(sel(&s).is_empty());
        // Lines select their whole connected group.
        let mut project = s.project().clone();
        project.sequences[0].dots.connect(0, 1);
        project.sequences[0].dots.connect(1, 2);
        s.set_project(project, None);
        run(&mut s, "dots.select", json!({"line": 1})).unwrap();
        assert_eq!(sel(&s), [0, 1, 2]);
        for bad in [
            json!({"indices": [9]}),
            json!({"line": 9}),
            json!({"rect": [1, 2, 3]}),
            json!({"mode": "nope"}),
        ] {
            assert!(run(&mut s, "dots.select", bad).is_err());
        }
    }

    #[test]
    fn transforms_move_scale_rotate_flip_and_refuse_clashes() {
        let mut s = with_dots(3);
        run(&mut s, "dots.selectAll", json!({})).unwrap();
        run(
            &mut s,
            "dots.transform",
            json!({"op": "move", "dx": 5, "dy": 1}),
        )
        .unwrap();
        assert_eq!(firsts(&s)[0], (5.0, 1.0));
        // Scale about the bounding-box middle (x 5..25 -> 15).
        run(
            &mut s,
            "dots.transform",
            json!({"op": "scale", "sx": 2, "sy": 1}),
        )
        .unwrap();
        assert_eq!(firsts(&s)[0].0, -5.0);
        assert_eq!(firsts(&s)[2].0, 35.0);
        run(&mut s, "dots.transform", json!({"op": "flipH"})).unwrap();
        assert_eq!(firsts(&s)[0].0, 35.0);
        run(
            &mut s,
            "dots.transform",
            json!({"op": "rotate", "angle": 90, "cx": 0, "cy": 0}),
        )
        .unwrap();
        assert!((firsts(&s)[0].0 + 1.0).abs() < 1e-9 || firsts(&s)[0].0.is_finite());
        // The second image did not move.
        assert_eq!(s.project().sequences[0].dots.pairs[0].second.x, 0.0);
        // Scaling to zero would stack every dot: refused, nothing changes.
        let before = firsts(&s);
        assert!(run(
            &mut s,
            "dots.transform",
            json!({"op": "scale", "sx": 0, "sy": 0})
        )
        .is_err());
        assert_eq!(firsts(&s), before);
        // Nothing selected and no indices is an error.
        run(&mut s, "dots.selectNone", json!({})).unwrap();
        assert!(run(&mut s, "dots.transform", json!({"op": "move", "dx": 1})).is_err());
        assert!(run(&mut s, "dots.nudge", json!({"dx": 1})).is_err());
    }

    #[test]
    fn nudge_is_one_undo_step_and_works_on_indices() {
        let mut s = with_dots(2);
        run(
            &mut s,
            "dots.nudge",
            json!({"indices": [1], "dx": 1, "dy": 0, "image": 2}),
        )
        .unwrap();
        assert_eq!(s.project().sequences[0].dots.pairs[1].second.x, 11.0);
        run(&mut s, "edit.undo", json!({})).unwrap();
        assert_eq!(s.project().sequences[0].dots.pairs[1].second.x, 10.0);
    }

    #[test]
    fn copy_cut_paste_keep_lines_and_avoid_clashes() {
        let mut s = with_dots(3);
        let mut project = s.project().clone();
        project.sequences[0].dots.connect(0, 1);
        project.sequences[0].dots.connect(1, 2);
        s.set_project(project, None);
        assert!(run(&mut s, "edit.copy", json!({})).is_err());
        assert!(run(&mut s, "edit.paste", json!({})).is_err());
        run(&mut s, "dots.select", json!({"indices": [0, 1]})).unwrap();
        run(&mut s, "edit.copy", json!({})).unwrap();
        run(&mut s, "edit.paste", json!({})).unwrap();
        let dots = &s.project().sequences[0].dots;
        assert_eq!(dots.len(), 5);
        // Two new dots, one line between them, offset by 20 px (10 px would land on dot 1).
        assert_eq!(dots.lines.len(), 3);
        assert_eq!((dots.pairs[3].first.x, dots.pairs[3].first.y), (20.0, 20.0));
        assert_eq!(s.selection().iter().copied().collect::<Vec<_>>(), [3, 4]);
        // Cut removes the dots and their lines; paste brings them back.
        run(&mut s, "edit.cut", json!({})).unwrap();
        assert_eq!(s.project().sequences[0].dots.len(), 3);
        assert!(s.selection().is_empty());
        run(&mut s, "edit.paste", json!({})).unwrap();
        assert_eq!(s.project().sequences[0].dots.len(), 5);
        assert!(s.project().sequences[0].dots.validate_lines().is_ok());
        run(&mut s, "edit.delete", json!({})).unwrap();
        assert_eq!(s.project().sequences[0].dots.len(), 3);
        run(&mut s, "edit.undo", json!({})).unwrap();
        assert_eq!(s.project().sequences[0].dots.len(), 5);
        run(&mut s, "dots.clearAll", json!({})).unwrap();
        assert_eq!(s.project().sequences[0].dots.len(), 0);
        assert!(s.project().sequences[0].dots.lines.is_empty());
    }

    #[test]
    fn empty_and_single_dot_sets_are_fine() {
        let mut s = session();
        run(&mut s, "dots.selectAll", json!({})).unwrap();
        assert!(run(&mut s, "edit.delete", json!({})).is_err());
        run(&mut s, "dots.clearAll", json!({})).unwrap();
        let mut s = with_dots(1);
        run(&mut s, "dots.selectAll", json!({})).unwrap();
        run(
            &mut s,
            "dots.transform",
            json!({"op": "rotate", "angle": 45}),
        )
        .unwrap();
        run(
            &mut s,
            "dots.transform",
            json!({"op": "skew", "kx": 30, "ky": 10}),
        )
        .unwrap();
    }
}
