//! Image filters: `image.filter.smooth`, `.blur`, `.gaussian`, `.zoomBlur`, `.radialBlur`,
//! `.sharpen`, `.emboss`, `.mosaic`, `.oilPaint`, `.splash`, `.cylinder`, `.whirl`, `.pinch`,
//! plus `image.filter.remove`, `.move` and `.clear`.
//!
//! An image carries an ordered list of at most 16 filters, applied after the colour
//! adjustment. A filter command appends to the list, or replaces the filter at `index`.
//! Centres are fractions of the image (0.5 is the middle); radii are in pixels.

use crate::effect_cmds::SEQUENCE;
use crate::image_cmds::{run_edit, IMAGE};
use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{Filter, MAX_FILTERS};
use serde_json::Value;

const INDEX: ParamSpec = ParamSpec::new(
    "index",
    ParamKind::Integer,
    false,
    "Replace the filter at this position (starting at 0) instead of adding a new one at the end",
)
.range(0.0, 15.0);

const CX: ParamSpec = ParamSpec::new(
    "cx",
    ParamKind::Number,
    false,
    "Centre, as a fraction of the image width (0.5 is the middle)",
)
.range(-1.0, 2.0)
.default_is("0.5");

const CY: ParamSpec = ParamSpec::new(
    "cy",
    ParamKind::Number,
    false,
    "Centre, as a fraction of the image height",
)
.range(-1.0, 2.0)
.default_is("0.5");

fn num(params: &Params, name: &str, default: f64) -> f64 {
    params.number(name).unwrap_or(default)
}

fn int(params: &Params, name: &str, default: u32) -> u32 {
    params
        .integer(name)
        .map_or(default, |v| u32::try_from(v.max(0)).unwrap_or(u32::MAX))
}

fn no_filter(index: i64, count: usize) -> CommandError {
    CommandError::BadParams(format!(
        "there is no filter {index} (this image has {count}; numbering starts at 0)"
    ))
}

/// Append `filter` to the image's list, or replace the one at `index`.
fn put(session: &mut Session, params: &Params, filter: Filter) -> Result<Value, CommandError> {
    run_edit(session, params, false, |e, _| {
        match params.integer("index") {
            Some(i) => {
                let count = e.filters.len();
                let slot = usize::try_from(i)
                    .ok()
                    .and_then(|i| e.filters.get_mut(i))
                    .ok_or_else(|| no_filter(i, count))?;
                *slot = filter;
            }
            None => {
                if e.filters.len() >= MAX_FILTERS {
                    return Err(CommandError::BadParams(format!(
                        "an image holds at most {MAX_FILTERS} filters; remove one first"
                    )));
                }
                e.filters.push(filter);
            }
        }
        Ok(())
    })
}

const SMOOTH_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    INDEX,
    ParamSpec::new(
        "strength",
        ParamKind::Number,
        false,
        "How much to smooth, 0 to 1",
    )
    .range(0.0, 1.0)
    .default_is("1"),
];
fn smooth(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    put(
        s,
        p,
        Filter::Smooth {
            strength: num(p, "strength", 1.0),
        },
    )
}

const BLUR_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    INDEX,
    ParamSpec::new(
        "radius",
        ParamKind::Integer,
        false,
        "Box blur radius in pixels, 1 to 100",
    )
    .range(1.0, 100.0)
    .default_is("3"),
];
fn blur(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    put(
        s,
        p,
        Filter::Blur {
            radius: int(p, "radius", 3),
        },
    )
}

const GAUSSIAN_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    INDEX,
    ParamSpec::new(
        "sigma",
        ParamKind::Number,
        false,
        "Standard deviation in pixels, 0.3 to 100",
    )
    .range(0.3, 100.0)
    .default_is("4"),
];
fn gaussian(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    put(
        s,
        p,
        Filter::Gaussian {
            sigma: num(p, "sigma", 4.0),
        },
    )
}

const ZOOM_BLUR_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    INDEX,
    CX,
    CY,
    ParamSpec::new(
        "amount",
        ParamKind::Number,
        false,
        "How far toward the centre to blur, 0 to 1",
    )
    .range(0.0, 1.0)
    .default_is("0.3"),
];
fn zoom_blur(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    put(
        s,
        p,
        Filter::ZoomBlur {
            cx: num(p, "cx", 0.5),
            cy: num(p, "cy", 0.5),
            amount: num(p, "amount", 0.3),
        },
    )
}

const RADIAL_BLUR_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    INDEX,
    CX,
    CY,
    ParamSpec::new(
        "angle",
        ParamKind::Number,
        false,
        "Blur through this many degrees, 0 to 360",
    )
    .range(0.0, 360.0)
    .default_is("15"),
];
fn radial_blur(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    put(
        s,
        p,
        Filter::RadialBlur {
            cx: num(p, "cx", 0.5),
            cy: num(p, "cy", 0.5),
            angle: num(p, "angle", 15.0),
        },
    )
}

const SHARPEN_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    INDEX,
    ParamSpec::new("amount", ParamKind::Number, false, "Strength, 0 to 5")
        .range(0.0, 5.0)
        .default_is("1"),
    ParamSpec::new(
        "radius",
        ParamKind::Number,
        false,
        "Blur radius in pixels, 0.3 to 100",
    )
    .range(0.3, 100.0)
    .default_is("1.5"),
];
fn sharpen(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    put(
        s,
        p,
        Filter::Sharpen {
            amount: num(p, "amount", 1.0),
            radius: num(p, "radius", 1.5),
        },
    )
}

const EMBOSS_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    INDEX,
    ParamSpec::new(
        "angle",
        ParamKind::Number,
        false,
        "Light direction in degrees, -360 to 360",
    )
    .range(-360.0, 360.0)
    .default_is("45"),
    ParamSpec::new("depth", ParamKind::Number, false, "Relief depth, 0 to 10")
        .range(0.0, 10.0)
        .default_is("1"),
    ParamSpec::new(
        "base",
        ParamKind::Number,
        false,
        "Grey of flat areas, 0 to 1",
    )
    .range(0.0, 1.0)
    .default_is("0.5"),
];
fn emboss(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    put(
        s,
        p,
        Filter::Emboss {
            angle: num(p, "angle", 45.0),
            depth: num(p, "depth", 1.0),
            base: num(p, "base", 0.5),
        },
    )
}

const MOSAIC_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    INDEX,
    ParamSpec::new(
        "cell",
        ParamKind::Integer,
        false,
        "Block size in pixels, 2 to 512",
    )
    .range(2.0, 512.0)
    .default_is("8"),
];
fn mosaic(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    put(
        s,
        p,
        Filter::Mosaic {
            cell: int(p, "cell", 8),
        },
    )
}

const OIL_PAINT_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    INDEX,
    ParamSpec::new(
        "radius",
        ParamKind::Integer,
        false,
        "Brush radius in pixels, 1 to 16",
    )
    .range(1.0, 16.0)
    .default_is("3"),
    ParamSpec::new(
        "levels",
        ParamKind::Integer,
        false,
        "Brightness levels, 8 to 256",
    )
    .range(8.0, 256.0)
    .default_is("32"),
];
fn oil_paint(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    put(
        s,
        p,
        Filter::OilPaint {
            radius: int(p, "radius", 3),
            levels: int(p, "levels", 32),
        },
    )
}

const SPLASH_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    INDEX,
    ParamSpec::new(
        "radius",
        ParamKind::Integer,
        false,
        "Scatter distance in pixels, 1 to 100",
    )
    .range(1.0, 100.0)
    .default_is("5"),
    ParamSpec::new(
        "seed",
        ParamKind::Integer,
        false,
        "The same seed scatters the same way",
    )
    .range(0.0, 9.0e15)
    .default_is("0"),
];
fn splash(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    put(
        s,
        p,
        Filter::Splash {
            radius: int(p, "radius", 5),
            seed: u64::try_from(p.integer("seed").unwrap_or(0).max(0)).unwrap_or(0),
        },
    )
}

const CYLINDER_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    INDEX,
    ParamSpec::new(
        "vertical",
        ParamKind::Bool,
        false,
        "Bend along the vertical axis instead of the horizontal one",
    ),
    ParamSpec::new(
        "amount",
        ParamKind::Number,
        false,
        "-1 to 1: positive bulges the middle, negative pinches it",
    )
    .range(-1.0, 1.0)
    .default_is("0.5"),
];
fn cylinder(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    put(
        s,
        p,
        Filter::Cylinder {
            vertical: p.boolean("vertical").unwrap_or(false),
            amount: num(p, "amount", 0.5),
        },
    )
}

const WHIRL_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    INDEX,
    CX,
    CY,
    ParamSpec::new("radius", ParamKind::Number, false, "Radius in pixels")
        .range(0.0, 100_000.0)
        .default_is("200"),
    ParamSpec::new(
        "angle",
        ParamKind::Number,
        false,
        "Twist in degrees, -720 to 720",
    )
    .range(-720.0, 720.0)
    .default_is("90"),
];
fn whirl(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    put(
        s,
        p,
        Filter::Whirl {
            cx: num(p, "cx", 0.5),
            cy: num(p, "cy", 0.5),
            radius: num(p, "radius", 200.0),
            angle: num(p, "angle", 90.0),
        },
    )
}

const PINCH_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    INDEX,
    CX,
    CY,
    ParamSpec::new("radius", ParamKind::Number, false, "Radius in pixels")
        .range(0.0, 100_000.0)
        .default_is("200"),
    ParamSpec::new(
        "amount",
        ParamKind::Number,
        false,
        "-1 to 1: positive pinches, negative bulges",
    )
    .range(-1.0, 1.0)
    .default_is("0.5"),
];
fn pinch(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    put(
        s,
        p,
        Filter::Pinch {
            cx: num(p, "cx", 0.5),
            cy: num(p, "cy", 0.5),
            radius: num(p, "radius", 200.0),
            amount: num(p, "amount", 0.5),
        },
    )
}

const REMOVE_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    ParamSpec::new(
        "index",
        ParamKind::Integer,
        true,
        "Position of the filter, starting at 0",
    )
    .range(0.0, 15.0),
];
fn remove(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = params.integer("index").unwrap_or(-1);
    run_edit(session, params, false, |e, _| {
        let at = usize::try_from(index)
            .ok()
            .filter(|i| *i < e.filters.len())
            .ok_or_else(|| no_filter(index, e.filters.len()))?;
        e.filters.remove(at);
        Ok(())
    })
}

const MOVE_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    IMAGE,
    ParamSpec::new(
        "index",
        ParamKind::Integer,
        true,
        "Position of the filter, starting at 0",
    )
    .range(0.0, 15.0),
    ParamSpec::new("to", ParamKind::Integer, true, "The position to move it to").range(0.0, 15.0),
];
fn move_filter(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let (from, to) = (
        params.integer("index").unwrap_or(-1),
        params.integer("to").unwrap_or(-1),
    );
    run_edit(session, params, false, |e, _| {
        let count = e.filters.len();
        let pick = |n: i64| {
            usize::try_from(n)
                .ok()
                .filter(|i| *i < count)
                .ok_or_else(|| no_filter(n, count))
        };
        let (from, to) = (pick(from)?, pick(to)?);
        let f = e.filters.remove(from);
        e.filters.insert(to, f);
        Ok(())
    })
}

const CLEAR_PARAMS: &[ParamSpec] = &[SEQUENCE, IMAGE];
fn clear(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    run_edit(session, params, false, |e, _| {
        e.filters.clear();
        Ok(())
    })
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
    vec![
        spec(
            "image.filter.smooth",
            "Smooth",
            "Image/Filters/Smooth...",
            "Soften an image with a 3 x 3 weighted mean.",
            SMOOTH_PARAMS,
            smooth,
        ),
        spec(
            "image.filter.blur",
            "Blur",
            "Image/Filters/Blur...",
            "Box blur by a radius in pixels.",
            BLUR_PARAMS,
            blur,
        ),
        spec(
            "image.filter.gaussian",
            "Gaussian Blur",
            "Image/Filters/Gaussian Blur...",
            "Gaussian blur with a standard deviation in pixels.",
            GAUSSIAN_PARAMS,
            gaussian,
        ),
        spec(
            "image.filter.zoomBlur",
            "Zoom Blur",
            "Image/Filters/Zoom Blur...",
            "Blur along lines toward a centre point.",
            ZOOM_BLUR_PARAMS,
            zoom_blur,
        ),
        spec(
            "image.filter.radialBlur",
            "Radial Blur",
            "Image/Filters/Radial Blur...",
            "Blur around a centre point, as if the image were spinning.",
            RADIAL_BLUR_PARAMS,
            radial_blur,
        ),
        spec(
            "image.filter.sharpen",
            "Sharpen",
            "Image/Filters/Sharpen...",
            "Unsharp mask: boost the difference from a blurred copy.",
            SHARPEN_PARAMS,
            sharpen,
        ),
        spec(
            "image.filter.emboss",
            "Emboss",
            "Image/Filters/Emboss...",
            "Relief lit from an angle, over a grey base.",
            EMBOSS_PARAMS,
            emboss,
        ),
        spec(
            "image.filter.mosaic",
            "Mosaic",
            "Image/Filters/Mosaic...",
            "Replace blocks of pixels by their average.",
            MOSAIC_PARAMS,
            mosaic,
        ),
        spec(
            "image.filter.oilPaint",
            "Oil Paint",
            "Image/Filters/Oil Paint...",
            "Paint-like smoothing that keeps the dominant tone of each area.",
            OIL_PAINT_PARAMS,
            oil_paint,
        ),
        spec(
            "image.filter.splash",
            "Splash",
            "Image/Filters/Splash...",
            "Scatter pixels like spattered paint (repeatable by seed).",
            SPLASH_PARAMS,
            splash,
        ),
        spec(
            "image.filter.cylinder",
            "Cylinder",
            "Image/Filters/Cylinder...",
            "Bend the picture around a cylinder.",
            CYLINDER_PARAMS,
            cylinder,
        ),
        spec(
            "image.filter.whirl",
            "Whirl",
            "Image/Filters/Whirl...",
            "Twist the area around a centre point.",
            WHIRL_PARAMS,
            whirl,
        ),
        spec(
            "image.filter.pinch",
            "Pinch",
            "Image/Filters/Pinch...",
            "Pinch or bulge the area around a centre point.",
            PINCH_PARAMS,
            pinch,
        ),
        spec(
            "image.filter.remove",
            "Remove Filter",
            "",
            "Remove one filter from an image's list.",
            REMOVE_PARAMS,
            remove,
        ),
        spec(
            "image.filter.move",
            "Move Filter",
            "",
            "Change the position of a filter in an image's list.",
            MOVE_PARAMS,
            move_filter,
        ),
        spec(
            "image.filter.clear",
            "Remove All Filters",
            "Image/Filters/Remove All",
            "Remove every filter from an image.",
            CLEAR_PARAMS,
            clear,
        ),
    ]
}
