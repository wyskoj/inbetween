//! Movie-level settings: `movie.size` and `movie.fps`.
//!
//! The output size is shared by every sequence. Dots are stored in output-frame pixels (D12), so
//! changing the size moves every dot through the old and new fit of its image, which keeps each
//! dot on the feature it marked. Sequence and movie captions scale with the frame height; image
//! captions are drawn on the picture itself and are untouched.

use crate::effect_cmds::SEQUENCE;
use crate::image_cmds::{matrix, remap_side, source_size};
use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{affine_inv, affine_mul, Caption, FitMode, MAX_SIDE};
use inbetween_morph::fit_transform;
use serde_json::{json, Value};

const PRESETS: &[(&str, u32, u32)] = &[
    ("720p", 1280, 720),
    ("1080p", 1920, 1080),
    ("1440p", 2560, 1440),
    ("4k", 3840, 2160),
    ("square1080", 1080, 1080),
    ("vertical1080", 1080, 1920),
];

const PRESET_IDS: &[&str] = &["720p", "1080p", "1440p", "4k", "square1080", "vertical1080"];

const SIZE_PARAMS: &[ParamSpec] = &[
    ParamSpec::new("width", ParamKind::Integer, false, "Output width in pixels")
        .range(1.0, MAX_SIDE as f64),
    ParamSpec::new(
        "height",
        ParamKind::Integer,
        false,
        "Output height in pixels",
    )
    .range(1.0, MAX_SIDE as f64),
    ParamSpec::new(
        "preset",
        ParamKind::String,
        false,
        "A named size instead of width and height",
    )
    .one_of(PRESET_IDS),
    ParamSpec::new(
        "match_image",
        ParamKind::Integer,
        false,
        "Match the size of this image (1 or 2) of `sequence`, after its edits",
    )
    .range(1.0, 2.0),
    SEQUENCE,
    ParamSpec::new(
        "keep_dots",
        ParamKind::Bool,
        false,
        "Leave the dots where they are instead of moving them with their pictures",
    ),
];

const FPS_PARAMS: &[ParamSpec] = &[ParamSpec::new(
    "fps",
    ParamKind::Number,
    false,
    "Frames per second, 1 to 60 (24, 25, 30, 29.97 and 60 are typical)",
)
.range(1.0, 60.0)];

/// The map taking a dot in the output frame of size `old` to the same point of its picture in
/// the frame of size `new`. `None` when the sequence has no dot on that image.
fn resize_map(
    session: &mut Session,
    index: usize,
    which: u8,
    old: (u32, u32),
    new: (u32, u32),
) -> Result<Option<inbetween_doc::Affine>, CommandError> {
    let seq = session.sequence(index)?;
    if !seq.dots.pairs.iter().any(|p| p.has_end(which)) {
        return Ok(None);
    }
    let mode = seq.fit;
    let edit = seq.edit(which).clone();
    // Stretch scales each axis by new/old whatever the picture is; the other modes need it.
    let size = if mode == FitMode::Stretch {
        (1, 1)
    } else {
        let src = source_size(session, index, which).map_err(|e| {
            CommandError::BadParams(format!(
                "Image {which} of sequence {index} cannot be read to move its dots ({e}); \
                 fix the file or pass keep_dots"
            ))
        })?;
        let g = edit.geometry(src.0, src.1).map_err(CommandError::Doc)?;
        (g.width, g.height)
    };
    let fit = |out: (u32, u32)| {
        fit_transform(size.0, size.1, out.0, out.1, mode)
            .map(|a| matrix(&a))
            .map_err(|e| CommandError::Render(e.into()))
    };
    let (fit_old, fit_new) = (fit(old)?, fit(new)?);
    let back = affine_inv(&fit_new).ok_or_else(|| {
        CommandError::BadParams(format!("Image {which} has no area at the new size"))
    })?;
    Ok(Some(affine_mul(&back, &fit_old)))
}

fn scale_caption(c: &mut Caption, k: f64) {
    c.size *= k;
    c.border_size *= k;
    c.shadow.distance *= k;
    c.shadow.size *= k;
}

fn size(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let old_out = session.project().output;
    let old = (old_out.width, old_out.height);
    let modes = [
        params.string("preset").is_some(),
        params.integer("match_image").is_some(),
        params.integer("width").is_some() || params.integer("height").is_some(),
    ];
    if modes.iter().filter(|m| **m).count() > 1 {
        return Err(CommandError::BadParams(
            "use one of: width and height, `preset`, or `match_image`".into(),
        ));
    }
    let mut new = old;
    if let Some(id) = params.string("preset") {
        let found = PRESETS.iter().find(|(name, _, _)| *name == id);
        let Some((_, w, h)) = found else {
            return Err(CommandError::BadParams(format!("unknown preset `{id}`")));
        };
        new = (*w, *h);
    } else if let Some(which) = params.integer("match_image") {
        let which = if which == 2 { 2 } else { 1 };
        let index = session.sequence_index(params)?;
        let edit = session.sequence(index)?.edit(which).clone();
        let src = source_size(session, index, which)?;
        let g = edit.geometry(src.0, src.1).map_err(CommandError::Doc)?;
        new = (g.width, g.height);
    } else if modes[2] {
        let side = |name: &str, current: u32| {
            params
                .integer(name)
                .map_or(current, |n| n.clamp(1, i64::from(MAX_SIDE)) as u32)
        };
        new = (side("width", old.0), side("height", old.1));
    }
    if new.0 == 0 || new.1 == 0 || new.0 > MAX_SIDE || new.1 > MAX_SIDE {
        return Err(CommandError::BadParams(format!(
            "output size {} x {} is not allowed (each side 1 to {MAX_SIDE})",
            new.0, new.1
        )));
    }
    let mut moved = false;
    if new != old {
        let keep = params.boolean("keep_dots").unwrap_or(false);
        if !keep {
            for index in 0..session.project().sequences.len() {
                for which in [1u8, 2u8] {
                    if let Some(map) = resize_map(session, index, which, old, new)? {
                        remap_side(session, index, which, &map)?;
                        moved = true;
                    }
                }
            }
        }
        let k = f64::from(new.1) / f64::from(old.1.max(1));
        let project = session.project_mut();
        project.output.width = new.0;
        project.output.height = new.1;
        if let Some(c) = project.caption.as_mut() {
            scale_caption(c, k);
        }
        for seq in &mut project.sequences {
            if let Some(c) = seq.caption.as_mut() {
                scale_caption(c, k);
            }
        }
        project.validate().map_err(CommandError::Doc)?;
    }
    Ok(json!({
        "width": new.0,
        "height": new.1,
        "previous": [old.0, old.1],
        "dots_moved": moved,
    }))
}

fn fps(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    if let Some(n) = params.number("fps") {
        if !(n.is_finite() && (1.0..=60.0).contains(&n)) {
            return Err(CommandError::BadParams(
                "fps must be a number from 1 to 60".into(),
            ));
        }
        session.project_mut().fps = n;
    }
    Ok(json!({ "fps": session.project().fps }))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "movie.size",
            label: "Movie Size",
            menu: "Movie/Movie Size...",
            shortcut: None,
            description: "Set (or report) the output frame size: width and height, a preset, or the size of an image. Dots move with their pictures, and sequence and movie captions scale with the height.",
            params: SIZE_PARAMS,
            enabled: |_| true,
            disabled_reason: "",
            edits: true,
            run: size,
        },
        CommandSpec {
            id: "movie.fps",
            label: "Movie Frame Rate",
            menu: "Movie/Frame Rate...",
            shortcut: None,
            description: "Set (or report) the movie's frames per second, 1 to 60. Playback and exports default to it, and sequence.length converts seconds with it.",
            params: FPS_PARAMS,
            enabled: |_| true,
            disabled_reason: "",
            edits: true,
            run: fps,
        },
    ]
}
