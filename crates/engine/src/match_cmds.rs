//! `dots.autoMatch`: place the missing partner of unpaired dots automatically.

use crate::dots_cmds::taken;
use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{DotPair, Side};
use inbetween_geom::Point;
use inbetween_morph::landmarks::{detect_corners, CornerOptions};
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

const LANDMARK_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "count",
        ParamKind::Integer,
        false,
        "Most dot pairs to add",
    )
    .range(1.0, 2000.0)
    .default_is("40"),
    ParamSpec::new(
        "quality",
        ParamKind::Number,
        false,
        "How strong a corner must be, as a fraction of the strongest one found (0 to 1); lower finds more, fainter features",
    )
    .range(0.0, 1.0)
    .default_is("0.05"),
    ParamSpec::new(
        "min_distance",
        ParamKind::Number,
        false,
        "Smallest gap between two new dots, and between a new dot and an existing one, in output pixels",
    )
    .range(1.0, 100_000.0)
    .default_is("4% of the shorter side of the frame"),
    ParamSpec::new(
        "tolerance",
        ParamKind::Number,
        false,
        "A match is kept only when matching back from Image 2 lands within this many pixels of the start",
    )
    .range(0.0, 1000.0)
    .default_is("1% of the frame diagonal"),
    ParamSpec::new(
        "border",
        ParamKind::Bool,
        false,
        "Also pin the frame's four corners and edge midpoints (the same place on both images) so the picture's edge holds still",
    )
    .default_is("false"),
    ParamSpec::new(
        "replace",
        ParamKind::Bool,
        false,
        "Remove the sequence's existing dots first",
    )
    .default_is("false"),
    ParamSpec::new(
        "sequence",
        ParamKind::Integer,
        false,
        "Sequence number, starting at 0",
    )
    .range(0.0, 1_000_000.0)
    .default_is("0"),
];

/// Dots for subjects that are not faces: find corner features on Image 1, find the same spots
/// on Image 2 by comparing patches, and keep the pairs that match back the same way.
fn auto_landmarks(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let prepared = session.prepare(index)?;
    let image2 = prepared.image2.clone().ok_or_else(|| {
        CommandError::BadParams(
            "this sequence's morph type does not use Image 2, so there is nothing to match".into(),
        )
    })?;
    let (w, h) = prepared.output;
    let (wf, hf) = (f64::from(w), f64::from(h));
    let short = wf.min(hf);
    let diag = wf.hypot(hf);
    let count = params
        .integer("count")
        .map_or(40, |n| n.clamp(1, 2000) as usize);
    let quality = params.number("quality").unwrap_or(0.05);
    let spacing = params
        .number("min_distance")
        .unwrap_or(short * 0.04)
        .max(1.0);
    let tolerance = params.number("tolerance").unwrap_or(diag * 0.01).max(0.0);
    let border = params.boolean("border") == Some(true);
    let replace = params.boolean("replace") == Some(true);

    // The two images at output size, with nothing drawn over them.
    let mut plain = prepared.sequence.clone();
    plain.effects = Default::default();
    plain.caption = None;
    plain.camera = Default::default();
    if replace {
        plain.dots = Default::default();
    }
    let renderer = Renderer::new(&plain, prepared.output, &prepared.image1, Some(&image2))
        .map_err(CommandError::Render)?;
    let frame1 = renderer.render_t(0.0).map_err(CommandError::Render)?;
    let frame2 = renderer.render_t(1.0).map_err(CommandError::Render)?;

    // Look for twice as many corners as wanted: some will not match back.
    let corners = detect_corners(
        &frame1,
        &CornerOptions {
            max_corners: count.saturating_mul(2).min(4000),
            quality,
            min_distance: spacing,
        },
    );
    let existing: Vec<Point> = plain
        .dots
        .pairs
        .iter()
        .filter(|p| p.has_end(1))
        .map(|p| p.end(1))
        .collect();
    let corners: Vec<Point> = corners
        .into_iter()
        .filter(|c| existing.iter().all(|e| e.distance(*c) >= spacing))
        .collect();
    let forward: Vec<(Point, u8)> = corners.iter().map(|c| (*c, 1)).collect();
    let found = inbetween_morph::automatch::auto_match_scored(&plain, &frame1, &frame2, &forward);
    let back: Vec<(Point, u8)> = found.iter().map(|(q, _)| (*q, 2)).collect();
    let returned = inbetween_morph::automatch::auto_match_scored(&plain, &frame1, &frame2, &back);

    let inside = |q: Point| q.is_finite() && q.x >= 0.0 && q.y >= 0.0 && q.x <= wf && q.y <= hf;
    let mut pairs: Vec<DotPair> = Vec::new();
    let mut rejected = 0usize;
    for ((p, (q, score)), (r, back_score)) in corners.iter().zip(&found).zip(&returned) {
        if pairs.len() >= count {
            break;
        }
        // Both directions must have found real patches and agree where the dot started.
        let good = score.is_some()
            && back_score.is_some()
            && inside(*q)
            && r.distance(*p) <= tolerance
            && pairs.iter().all(|d| d.second.distance(*q) >= 1.0);
        if good {
            pairs.push(DotPair::new(*p, *q));
        } else {
            rejected += 1;
        }
    }
    if border {
        let (mx, my) = (wf / 2.0, hf / 2.0);
        for (x, y) in [
            (0.0, 0.0),
            (mx, 0.0),
            (wf, 0.0),
            (wf, my),
            (wf, hf),
            (mx, hf),
            (0.0, hf),
            (0.0, my),
        ] {
            pairs.push(DotPair::fixed(Point::new(x, y)));
        }
    }
    let seq = session.sequence_mut(index)?;
    if replace {
        seq.dots = Default::default();
    }
    let mut added = 0usize;
    for pair in pairs {
        if taken(seq, 1, pair.first, None) || taken(seq, 2, pair.second, None) {
            rejected += 1;
            continue;
        }
        seq.dots.pairs.push(pair);
        added += 1;
    }
    seq.validate()?;
    Ok(json!({
        "sequence": index,
        "added": added,
        "rejected": rejected,
        "found": corners.len(),
        "total": seq.dots.len(),
    }))
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
    },
    CommandSpec {
        id: "dots.autoLandmarks",
        label: "Auto-Landmarks",
        menu: "Dots/Auto-Landmarks...",
        shortcut: None,
        description: "Place dot pairs automatically on any subject: find the strongest, well-spread corner features on Image 1, find the same spots on Image 2 by comparing patches, and keep the pairs that match back to where they started. Optionally pin the frame's border.",
        params: LANDMARK_PARAMS,
        enabled: has_sequence,
        disabled_reason: "the project has no sequence",
        edits: true,
        run: auto_landmarks,
    }]
}
