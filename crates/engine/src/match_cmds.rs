//! `dots.autoMatch`: place the missing partner of unpaired dots automatically.

use crate::dots_cmds::taken;
use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::Side;
use inbetween_geom::Point;
use inbetween_render_cpu::Renderer;
use serde_json::{json, Value};

const PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "indices",
        ParamKind::IntegerList,
        false,
        "Unpaired dots to match, counting from 0",
    )
    .range(0.0, 100_000.0)
    .default_is("every unpaired dot"),
    ParamSpec::new(
        "sequence",
        ParamKind::Integer,
        false,
        "Sequence number, starting at 0",
    )
    .range(0.0, 1_000_000.0)
    .default_is("0"),
];

fn has_sequence(session: &Session) -> bool {
    session.has_sequences()
}

fn auto_match(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let prepared = session.prepare(index)?;
    let image2 = prepared.image2.clone().ok_or_else(|| {
        CommandError::BadParams(
            "this sequence's morph type does not use Image 2, so there is nothing to match".into(),
        )
    })?;
    let count = prepared.sequence.dots.len();
    let wanted: Vec<usize> = match params.integers("indices") {
        Some(list) => {
            let mut v = Vec::new();
            for i in list {
                let i = usize::try_from(i)
                    .ok()
                    .filter(|i| *i < count)
                    .ok_or_else(|| {
                        CommandError::BadParams(format!(
                        "there is no dot {i} (this sequence has {count}; numbering starts at 0)"
                    ))
                    })?;
                v.push(i);
            }
            v.sort_unstable();
            v.dedup();
            v
        }
        None => prepared
            .sequence
            .dots
            .pairs
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.is_paired())
            .map(|(i, _)| i)
            .collect(),
    };
    let todo: Vec<(usize, u8, Point)> = wanted
        .iter()
        .filter_map(|&i| {
            let p = prepared.sequence.dots.pairs.get(i)?;
            match p.side {
                Side::Only1 => Some((i, 1, p.first)),
                Side::Only2 => Some((i, 2, p.second)),
                Side::Both => None,
            }
        })
        .collect();
    if todo.is_empty() {
        return Ok(json!({ "sequence": index, "completed": 0, "skipped": 0 }));
    }
    // At t=0 and t=1 the frame is the image itself at output size, which is what the matcher needs.
    let mut plain = prepared.sequence.clone();
    plain.effects = Default::default();
    plain.caption = None;
    plain.camera = Default::default();
    let renderer = Renderer::new(&plain, prepared.output, &prepared.image1, Some(&image2))
        .map_err(CommandError::Render)?;
    let frame1 = renderer.render_t(0.0).map_err(CommandError::Render)?;
    let frame2 = renderer.render_t(1.0).map_err(CommandError::Render)?;
    let dots: Vec<(Point, u8)> = todo.iter().map(|&(_, from, at)| (at, from)).collect();
    let found = inbetween_morph::automatch::auto_match(&plain, &frame1, &frame2, &dots);
    let seq = session.sequence_mut(index)?;
    let (mut completed, mut skipped) = (0usize, 0usize);
    for (&(i, from, _), q) in todo.iter().zip(found) {
        let to = 3 - from;
        if !q.is_finite() || taken(seq, to, q, Some(i)) {
            skipped += 1;
            continue;
        }
        if let Some(p) = seq.dots.pairs.get_mut(i) {
            p.side = Side::Both;
            if to == 1 {
                p.first = q;
            } else {
                p.second = q;
            }
            completed += 1;
        }
    }
    seq.validate()?;
    Ok(json!({ "sequence": index, "completed": completed, "skipped": skipped }))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "dots.autoMatch",
        label: "Auto-Match Dots",
        menu: "Dots/Auto-Match",
        shortcut: None,
        description: "Fill in the partner of unpaired dots: guess from the paired dots, then refine by comparing small patches of the two images.",
        params: PARAMS,
        enabled: has_sequence,
        disabled_reason: "the project has no sequence",
        edits: true,
        run: auto_match,
    }]
}
