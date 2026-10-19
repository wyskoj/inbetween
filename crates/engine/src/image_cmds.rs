//! Source image editing: `image.rotate`, `image.flip`, `image.crop`, `image.adjust`,
//! `image.reset` and `image.swap`.
//!
//! Each image of a sequence carries one [`ImageEdit`]; every command replaces only its own part
//! of it and leaves the file on disk alone. Commands that change the geometry also move that
//! image's side of every dot pair, so each dot stays on the feature it marked (a dot may end up
//! outside the frame). `keep_dots` leaves the dots where they are instead.
//!
//! Called with nothing to change, a command reports the image's current edit.

use crate::effect_cmds::SEQUENCE;
use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{affine_apply, affine_inv, affine_mul, Affine, Crop, ImageEdit};
use inbetween_geom::Point;
use inbetween_morph::fit_transform;
use serde_json::{json, Value};

pub(crate) const IMAGE: ParamSpec = ParamSpec::new(
    "image",
    ParamKind::Integer,
    false,
    "Which image of the sequence: 1 or 2",
)
.range(1.0, 2.0)
.default_is("1");

pub(crate) const KEEP_DOTS: ParamSpec = ParamSpec::new(
    "keep_dots",
    ParamKind::Bool,
    false,
    "Leave the dots where they are instead of moving them with the image",
);

/// The sequence and image (1 or 2) a command is aimed at.
pub(crate) fn target(session: &Session, params: &Params) -> Result<(usize, u8), CommandError> {
    let index = session.sequence_index(params)?;
    let which = if params.integer("image") == Some(2) {
        2
    } else {
        1
    };
    Ok((index, which))
}

/// Size of the source file of an image.
#[cfg(target_arch = "wasm32")]
fn source_size(_: &mut Session, _: usize, _: u8) -> Result<(u32, u32), CommandError> {
    Err(CommandError::Image(
        "image files cannot be read in this build".into(),
    ))
}

/// Size of the source file of an image.
#[cfg(not(target_arch = "wasm32"))]
fn source_size(session: &mut Session, index: usize, which: u8) -> Result<(u32, u32), CommandError> {
    let seq = session.sequence(index)?;
    let reference = if which == 1 { &seq.image1 } else { &seq.image2 }.0.clone();
    let img = session.load_image(&reference, which, index)?;
    Ok(img.size())
}

fn geometry_changed(a: &ImageEdit, b: &ImageEdit) -> bool {
    (
        a.quarter_turns,
        a.flip_h,
        a.flip_v,
        a.crop,
        a.angle,
        a.expand,
    ) != (
        b.quarter_turns,
        b.flip_h,
        b.flip_v,
        b.crop,
        b.angle,
        b.expand,
    )
}

fn matrix(a: &inbetween_geom::Affine) -> Affine {
    [a.a, a.b, a.tx, a.c, a.d, a.ty]
}

fn singular(which: u8) -> CommandError {
    CommandError::BadParams(format!(
        "the edit leaves Image {which} with no area to map its dots through"
    ))
}

/// The map taking a position in the output frame, with the image shown through `old`, to the
/// position of the same point of the picture once it is shown through `new`.
fn dot_map(
    session: &Session,
    index: usize,
    which: u8,
    old: &ImageEdit,
    new: &ImageEdit,
    src: (u32, u32),
) -> Result<Affine, CommandError> {
    let seq = session.sequence(index)?;
    let out = session.project().output;
    let fit = |w: u32, h: u32| -> Result<Affine, CommandError> {
        fit_transform(w, h, out.width, out.height, seq.fit)
            .map(|a| matrix(&a))
            .map_err(|e| CommandError::Render(e.into()))
    };
    let og = old.geometry(src.0, src.1).map_err(CommandError::Doc)?;
    let ng = new.geometry(src.0, src.1).map_err(CommandError::Doc)?;
    let fit_old = fit(og.width, og.height)?;
    let fit_new = fit(ng.width, ng.height)?;
    let back = affine_inv(&fit_new).ok_or_else(|| singular(which))?;
    let into_new = ng.source_to_edited().ok_or_else(|| singular(which))?;
    Ok(affine_mul(
        &back,
        &affine_mul(&into_new, &affine_mul(&og.edited_to_source, &fit_old)),
    ))
}

/// Move one side of every dot pair through `map`. Refuses (changing nothing) if two dots
/// would end up on the same spot or a position stops being finite.
fn remap_side(
    session: &mut Session,
    index: usize,
    which: u8,
    map: &Affine,
) -> Result<(), CommandError> {
    let seq = session.sequence_mut(index)?;
    let mut moved = Vec::with_capacity(seq.dots.pairs.len());
    for pair in &seq.dots.pairs {
        let p = if which == 1 { pair.first } else { pair.second };
        let (x, y) = affine_apply(map, p.x, p.y);
        if !(x.is_finite() && y.is_finite()) {
            return Err(CommandError::BadParams(format!(
                "that edit would move a dot of Image {which} to an invalid position"
            )));
        }
        moved.push(Point::new(x, y));
    }
    let mut keys: Vec<(u64, u64)> = moved
        .iter()
        .map(|p| (p.x.to_bits(), p.y.to_bits()))
        .collect();
    keys.sort_unstable();
    if keys.windows(2).any(|w| w[0] == w[1]) {
        return Err(CommandError::BadParams(format!(
            "that edit would put two dots on the same spot of Image {which}; move one first, or pass keep_dots"
        )));
    }
    for (pair, p) in seq.dots.pairs.iter_mut().zip(moved) {
        if which == 1 {
            pair.first = p;
        } else {
            pair.second = p;
        }
    }
    Ok(())
}

/// Apply `change` to the edit of the targeted image, move the dots if the geometry changed,
/// store the result and report it. `geometric` commands need the source file (its size).
pub(crate) fn run_edit(
    session: &mut Session,
    params: &Params,
    geometric: bool,
    change: impl FnOnce(&mut ImageEdit, (f64, f64)) -> Result<(), CommandError>,
) -> Result<Value, CommandError> {
    let (index, which) = target(session, params)?;
    let old = session.sequence(index)?.edit(which).clone();
    let src = if geometric {
        Some(source_size(session, index, which)?)
    } else {
        None
    };
    let turned = src.map_or((0.0, 0.0), |(w, h)| {
        if old.quarter_turns % 2 == 1 {
            (f64::from(h), f64::from(w))
        } else {
            (f64::from(w), f64::from(h))
        }
    });
    let mut new = old.clone();
    change(&mut new, turned)?;
    new.validate().map_err(CommandError::Doc)?;
    let mut dots_moved = false;
    let mut size = None;
    if let Some(src) = src {
        size = new
            .geometry(src.0, src.1)
            .ok()
            .map(|g| json!([g.width, g.height]));
        if geometry_changed(&old, &new) && !params.boolean("keep_dots").unwrap_or(false) {
            let map = dot_map(session, index, which, &old, &new, src)?;
            remap_side(session, index, which, &map)?;
            dots_moved = true;
        }
    }
    *session.sequence_mut(index)?.edit_mut(which) = new.clone();
    Ok(json!({
        "sequence": index,
        "image": which,
        "edit": serde_json::to_value(&new).map_err(|e| CommandError::Internal(e.to_string()))?,
        "size": size,
        "dots_moved": dots_moved,
    }))
}

/// Re-express a crop after the whole picture is turned a quarter clockwise.
fn crop_turn_cw(c: Crop, th: f64) -> Crop {
    Crop {
        x: th - c.y - c.h,
        y: c.x,
        w: c.h,
        h: c.w,
    }
}

fn crop_turn_ccw(c: Crop, tw: f64) -> Crop {
    Crop {
        x: c.y,
        y: tw - c.x - c.w,
        w: c.h,
        h: c.w,
    }
}

const ROTATE_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    ParamSpec::new(
        "turn",
        ParamKind::String,
        false,
        "Turn the picture: cw (90 degrees clockwise), ccw or 180",
    )
    .one_of(&["cw", "ccw", "180"]),
    ParamSpec::new(
        "angle",
        ParamKind::Number,
        false,
        "Rotate the content by this many degrees clockwise (-360 to 360), set absolutely, about the crop centre",
    )
    .range(-360.0, 360.0),
    ParamSpec::new(
        "expand",
        ParamKind::Bool,
        false,
        "Grow the canvas so the rotated picture fits entirely",
    ),
    KEEP_DOTS,
];

fn rotate(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    run_edit(session, params, true, |e, (tw, th)| {
        match params.string("turn") {
            Some(t @ ("cw" | "ccw")) => {
                let cw = t == "cw";
                e.quarter_turns = (e.quarter_turns + if cw { 1 } else { 3 }) % 4;
                std::mem::swap(&mut e.flip_h, &mut e.flip_v);
                e.crop = e.crop.map(|c| {
                    if cw {
                        crop_turn_cw(c, th)
                    } else {
                        crop_turn_ccw(c, tw)
                    }
                });
            }
            Some("180") => {
                e.quarter_turns = (e.quarter_turns + 2) % 4;
                e.crop = e.crop.map(|c| Crop {
                    x: tw - c.x - c.w,
                    y: th - c.y - c.h,
                    ..c
                });
            }
            Some(other) => {
                return Err(CommandError::BadParams(format!(
                    "unknown turn `{other}` (cw, ccw or 180)"
                )))
            }
            None => {}
        }
        if let Some(a) = params.number("angle") {
            e.angle = a;
        }
        if let Some(x) = params.boolean("expand") {
            e.expand = x;
        }
        Ok(())
    })
}

const FLIP_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    ParamSpec::new(
        "axis",
        ParamKind::String,
        true,
        "horizontal (mirror left to right) or vertical (top to bottom); applying it again undoes it",
    )
    .one_of(&["horizontal", "vertical"]),
    KEEP_DOTS,
];

fn flip(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    run_edit(session, params, true, |e, (tw, th)| {
        match params.string("axis") {
            Some("horizontal") => {
                e.flip_h = !e.flip_h;
                e.crop = e.crop.map(|c| Crop {
                    x: tw - c.x - c.w,
                    ..c
                });
            }
            Some("vertical") => {
                e.flip_v = !e.flip_v;
                e.crop = e.crop.map(|c| Crop {
                    y: th - c.y - c.h,
                    ..c
                });
            }
            _ => {
                return Err(CommandError::BadParams(
                    "`axis` must be horizontal or vertical".into(),
                ))
            }
        }
        // A mirror reverses the direction a rotation turns.
        e.angle = -e.angle;
        Ok(())
    })
}

const CROP_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    ParamSpec::new(
        "x",
        ParamKind::Number,
        false,
        "Left edge, in pixels of the turned and flipped image (may be negative)",
    ),
    ParamSpec::new("y", ParamKind::Number, false, "Top edge, in pixels"),
    ParamSpec::new(
        "w",
        ParamKind::Number,
        false,
        "Width in pixels; the crop may extend past the image",
    ),
    ParamSpec::new("h", ParamKind::Number, false, "Height in pixels"),
    ParamSpec::new(
        "angle",
        ParamKind::Number,
        false,
        "Rotate the content inside the crop by this many degrees clockwise (-360 to 360)",
    )
    .range(-360.0, 360.0),
    ParamSpec::new(
        "aspect",
        ParamKind::String,
        false,
        "Lock the shape: free, original, output, 1:1, 4:3, 16:9 or w:h. Snaps h to w (or w to h when only h is given)",
    )
    .default_is("free"),
    ParamSpec::new(
        "clear",
        ParamKind::Bool,
        false,
        "Remove the crop and its angle",
    ),
    KEEP_DOTS,
];

fn parse_ratio(
    text: &str,
    turned: (f64, f64),
    output: (f64, f64),
) -> Result<Option<f64>, CommandError> {
    let bad = || {
        CommandError::BadParams(format!(
            "`aspect` must be free, original, output or w:h like 4:3, not `{text}`"
        ))
    };
    let ratio = match text {
        "free" => return Ok(None),
        "original" => turned.0 / turned.1,
        "output" => output.0 / output.1,
        other => {
            let (a, b) = other.split_once(':').ok_or_else(bad)?;
            let a: f64 = a.trim().parse().map_err(|_| bad())?;
            let b: f64 = b.trim().parse().map_err(|_| bad())?;
            a / b
        }
    };
    if ratio.is_finite() && ratio > 0.0 {
        Ok(Some(ratio))
    } else {
        Err(bad())
    }
}

fn crop(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let out = session.project().output;
    let output = (f64::from(out.width), f64::from(out.height));
    run_edit(session, params, true, |e, turned| {
        if params.boolean("clear").unwrap_or(false) {
            e.crop = None;
            e.angle = 0.0;
            e.expand = false;
            return Ok(());
        }
        let touched = ["x", "y", "w", "h", "aspect"].iter().any(|n| params.has(n));
        if touched {
            let base = e.crop.unwrap_or(Crop {
                x: 0.0,
                y: 0.0,
                w: turned.0,
                h: turned.1,
            });
            let mut c = Crop {
                x: params.number("x").unwrap_or(base.x),
                y: params.number("y").unwrap_or(base.y),
                w: params.number("w").unwrap_or(base.w),
                h: params.number("h").unwrap_or(base.h),
            };
            if let Some(ratio) =
                parse_ratio(params.string("aspect").unwrap_or("free"), turned, output)?
            {
                if params.has("h") && !params.has("w") {
                    c.w = c.h * ratio;
                } else {
                    c.h = c.w / ratio;
                }
            }
            e.crop = Some(c);
        }
        if let Some(a) = params.number("angle") {
            e.angle = a;
        }
        let full = e
            .crop
            .is_some_and(|c| c.x == 0.0 && c.y == 0.0 && c.w == turned.0 && c.h == turned.1);
        if full && e.angle == 0.0 {
            e.crop = None;
            e.expand = false;
        }
        Ok(())
    })
}

const ADJUST_FIELDS: [(&str, &str); 7] = [
    ("brightness", "Brightness, -100 to 100"),
    ("contrast", "Contrast, -100 to 100"),
    ("red", "Red channel, -100 to 100"),
    ("green", "Green channel, -100 to 100"),
    ("blue", "Blue channel, -100 to 100"),
    ("hue", "Hue shift in degrees, -180 to 180"),
    ("saturation", "Saturation, -100 to 100"),
];

const ADJUST_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    ParamSpec::new("brightness", ParamKind::Number, false, ADJUST_FIELDS[0].1).range(-100.0, 100.0),
    ParamSpec::new("contrast", ParamKind::Number, false, ADJUST_FIELDS[1].1).range(-100.0, 100.0),
    ParamSpec::new("red", ParamKind::Number, false, ADJUST_FIELDS[2].1).range(-100.0, 100.0),
    ParamSpec::new("green", ParamKind::Number, false, ADJUST_FIELDS[3].1).range(-100.0, 100.0),
    ParamSpec::new("blue", ParamKind::Number, false, ADJUST_FIELDS[4].1).range(-100.0, 100.0),
    ParamSpec::new("hue", ParamKind::Number, false, ADJUST_FIELDS[5].1).range(-180.0, 180.0),
    ParamSpec::new("saturation", ParamKind::Number, false, ADJUST_FIELDS[6].1).range(-100.0, 100.0),
    ParamSpec::new("negative", ParamKind::Bool, false, "Invert the colours"),
    ParamSpec::new("grayscale", ParamKind::Bool, false, "Remove all colour"),
    ParamSpec::new(
        "reset",
        ParamKind::Bool,
        false,
        "Set every adjustment back to zero first",
    ),
];

fn adjust(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    run_edit(session, params, false, |e, _| {
        let a = &mut e.adjust;
        if params.boolean("reset").unwrap_or(false) {
            *a = Default::default();
        }
        let set = |name: &str, slot: &mut f64| {
            if let Some(v) = params.number(name) {
                *slot = v;
            }
        };
        set("brightness", &mut a.brightness);
        set("contrast", &mut a.contrast);
        set("red", &mut a.red);
        set("green", &mut a.green);
        set("blue", &mut a.blue);
        set("hue", &mut a.hue);
        set("saturation", &mut a.saturation);
        if let Some(v) = params.boolean("negative") {
            a.negative = v;
        }
        if let Some(v) = params.boolean("grayscale") {
            a.grayscale = v;
        }
        Ok(())
    })
}

const RESET_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    ParamSpec::new(
        "part",
        ParamKind::String,
        false,
        "What to reset: all, geometry (turns, flips, crop, angle), adjust, filters or caption",
    )
    .one_of(&["all", "geometry", "adjust", "filters", "caption"])
    .default_is("all"),
    KEEP_DOTS,
];

fn reset(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let part = params.string("part").unwrap_or("all");
    let geometric = matches!(part, "all" | "geometry");
    run_edit(session, params, geometric, |e, _| {
        match part {
            "all" => *e = ImageEdit::default(),
            "geometry" => {
                let keep = ImageEdit {
                    adjust: e.adjust,
                    filters: std::mem::take(&mut e.filters),
                    caption: e.caption.take(),
                    ..ImageEdit::default()
                };
                *e = keep;
            }
            "adjust" => e.adjust = Default::default(),
            "filters" => e.filters.clear(),
            "caption" => e.caption = None,
            other => {
                return Err(CommandError::BadParams(format!(
                    "unknown part `{other}` (all, geometry, adjust, filters or caption)"
                )))
            }
        }
        Ok(())
    })
}

const SWAP_PARAMS: &[ParamSpec] = &[SEQUENCE];

fn swap(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let seq = session.sequence_mut(index)?;
    std::mem::swap(&mut seq.image1, &mut seq.image2);
    std::mem::swap(&mut seq.edit1, &mut seq.edit2);
    for pair in &mut seq.dots.pairs {
        std::mem::swap(&mut pair.first, &mut pair.second);
    }
    Ok(json!({
        "sequence": index,
        "image1": seq.image1.0,
        "image2": seq.image2.0,
    }))
}

type Run = fn(&mut Session, &Params) -> Result<Value, CommandError>;

fn spec(
    id: &'static str,
    label: &'static str,
    menu: &'static str,
    description: &'static str,
    params: &'static [ParamSpec],
    run: Run,
) -> CommandSpec {
    CommandSpec {
        id,
        label,
        menu,
        shortcut: None,
        description,
        params,
        enabled: Session::has_sequences,
        disabled_reason: "the project has no sequences",
        edits: true,
        run,
    }
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    let mut swap_spec = spec(
        "image.swap",
        "Swap Images",
        "Image/Swap Images",
        "Swap Image 1 and Image 2 of a sequence, with their edits and the two sides of every dot pair.",
        SWAP_PARAMS,
        swap,
    );
    swap_spec.shortcut = Some("F3");
    vec![
        spec(
            "image.rotate",
            "Rotate",
            "Image/Rotate...",
            "Turn an image a quarter or half turn, or rotate its content by any angle. The dots move with it.",
            ROTATE_PARAMS,
            rotate,
        ),
        spec(
            "image.flip",
            "Flip",
            "Image/Flip...",
            "Mirror an image left to right or top to bottom. The dots move with it.",
            FLIP_PARAMS,
            flip,
        ),
        spec(
            "image.crop",
            "Crop",
            "Image/Crop...",
            "Crop an image to a rectangle that may extend past it, optionally rotating the content inside. The dots move with it.",
            CROP_PARAMS,
            crop,
        ),
        spec(
            "image.adjust",
            "Adjust",
            "Image/Adjust...",
            "Brightness, contrast, red/green/blue balance, hue, saturation, negative and grayscale.",
            ADJUST_PARAMS,
            adjust,
        ),
        spec(
            "image.reset",
            "Reset Edits",
            "Image/Reset Edits...",
            "Remove all of an image's edits, or just one part of them.",
            RESET_PARAMS,
            reset,
        ),
        swap_spec,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::{DotPair, DotSet, Project, Sequence};

    fn session() -> Session {
        let mut s = Session::new();
        s.set_project(
            Project {
                sequences: vec![Sequence {
                    dots: DotSet::new(vec![
                        DotPair::fixed(Point::new(1.0, 2.0)),
                        DotPair::fixed(Point::new(5.0, 6.0)),
                    ]),
                    ..Sequence::default()
                }],
                ..Project::default()
            },
            None,
        );
        s
    }

    #[test]
    fn remap_moves_one_side_through_the_map() {
        let mut s = session();
        remap_side(&mut s, 0, 2, &[1.0, 0.0, 10.0, 0.0, 1.0, -1.0]).unwrap();
        let pairs = &s.sequence(0).unwrap().dots.pairs;
        assert_eq!(pairs[0].first, Point::new(1.0, 2.0));
        assert_eq!(pairs[0].second, Point::new(11.0, 1.0));
    }

    #[test]
    fn a_map_that_merges_two_dots_is_refused_and_changes_nothing() {
        let mut s = session();
        let before = s.sequence(0).unwrap().clone();
        let err = remap_side(&mut s, 0, 1, &[0.0, 0.0, 3.0, 0.0, 0.0, 3.0])
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("same spot") && err.contains("Image 1"),
            "{err}"
        );
        assert_eq!(*s.sequence(0).unwrap(), before);
        let err = remap_side(&mut s, 0, 1, &[f64::NAN, 0.0, 0.0, 0.0, 1.0, 0.0]);
        assert!(err.is_err());
        assert_eq!(*s.sequence(0).unwrap(), before);
    }
}
