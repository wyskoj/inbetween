//! Output colour and filters: `movie.adjust` and `movie.filter.*`.
//!
//! `movie.adjust` is the colour adjustment of every finished frame of the movie. The
//! `movie.filter.*` commands (one per filter, plus `remove`, `move` and `clear`) edit a filter
//! list that runs over every finished frame, either the movie's (`scope=movie`, the default)
//! or one sequence's (`scope=sequence`). Their parameters are the `image.filter.*` ones with
//! the `image` parameter replaced by `scope`.

use crate::filter_cmds::{
    build_blur, build_cylinder, build_emboss, build_gaussian, build_mosaic, build_oil_paint,
    build_pinch, build_radial_blur, build_sharpen, build_smooth, build_splash, build_whirl,
    build_zoom_blur, clear_list, move_in, put, remove_from, scope_of,
};
use crate::image_cmds::{change_adjust, ADJUST_PARAMS};
use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use serde_json::{json, Value};
use std::sync::Mutex;

const FILTER_SCOPE: ParamSpec = ParamSpec::new(
    "scope",
    ParamKind::String,
    false,
    "`movie` for the movie's filters, `sequence` for one sequence's (see `sequence`)",
)
.one_of(&["movie", "sequence"])
.default_is("movie");

/// The schema of an `image.*` command with its `image` parameter swapped for `scope`
/// (`drop_sequence` also removes `sequence`, for the movie-only commands). Built once per
/// source schema and kept for the life of the process.
fn derived(
    source: &'static [ParamSpec],
    swap_image: bool,
    drop_sequence: bool,
) -> &'static [ParamSpec] {
    type Entry = (usize, &'static [ParamSpec]);
    static CACHE: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
    let key = source.as_ptr() as usize;
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((_, hit)) = cache.iter().find(|(k, _)| *k == key) {
        return hit;
    }
    let list: Vec<ParamSpec> = source
        .iter()
        .filter(|p| !(drop_sequence && p.name == "sequence"))
        .filter_map(|p| match p.name {
            "image" if swap_image => Some(FILTER_SCOPE),
            "image" => None,
            _ => Some(*p),
        })
        .collect();
    let leaked: &'static [ParamSpec] = Box::leak(list.into_boxed_slice());
    cache.push((key, leaked));
    leaked
}

fn adjust(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let mut new = session.project().adjust;
    change_adjust(&mut new, params);
    new.validate().map_err(CommandError::Doc)?;
    session.project_mut().adjust = new;
    Ok(json!({
        "adjust": serde_json::to_value(new).map_err(|e| CommandError::Internal(e.to_string()))?,
    }))
}

type Run = fn(&mut Session, &Params) -> Result<Value, CommandError>;

macro_rules! movie_filter {
    ($($run:ident => $build:path),* $(,)?) => {
        $(fn $run(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
            put(s, p, scope_of(p), $build(p))
        })*
    };
}

movie_filter! {
    smooth => build_smooth,
    blur => build_blur,
    gaussian => build_gaussian,
    zoom_blur => build_zoom_blur,
    radial_blur => build_radial_blur,
    sharpen => build_sharpen,
    emboss => build_emboss,
    mosaic => build_mosaic,
    oil_paint => build_oil_paint,
    splash => build_splash,
    cylinder => build_cylinder,
    whirl => build_whirl,
    pinch => build_pinch,
}

fn remove(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    remove_from(s, p, scope_of(p))
}

fn move_filter(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    move_in(s, p, scope_of(p))
}

fn clear(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    clear_list(s, p, scope_of(p))
}

/// (command id, the `image.filter.*` command it follows, menu path, description, run)
type Row = (&'static str, &'static str, &'static str, &'static str, Run);

const FILTERS: &[Row] = &[
    (
        "movie.filter.smooth",
        "image.filter.smooth",
        "Movie/Filters/Smooth...",
        "Soften every finished frame with a 3 x 3 weighted mean.",
        smooth,
    ),
    (
        "movie.filter.blur",
        "image.filter.blur",
        "Movie/Filters/Blur...",
        "Box blur every finished frame by a radius in pixels.",
        blur,
    ),
    (
        "movie.filter.gaussian",
        "image.filter.gaussian",
        "Movie/Filters/Gaussian Blur...",
        "Gaussian blur of every finished frame.",
        gaussian,
    ),
    (
        "movie.filter.zoomBlur",
        "image.filter.zoomBlur",
        "Movie/Filters/Zoom Blur...",
        "Blur every finished frame along lines toward a centre point.",
        zoom_blur,
    ),
    (
        "movie.filter.radialBlur",
        "image.filter.radialBlur",
        "Movie/Filters/Radial Blur...",
        "Blur every finished frame around a centre point.",
        radial_blur,
    ),
    (
        "movie.filter.sharpen",
        "image.filter.sharpen",
        "Movie/Filters/Sharpen...",
        "Unsharp mask over every finished frame.",
        sharpen,
    ),
    (
        "movie.filter.emboss",
        "image.filter.emboss",
        "Movie/Filters/Emboss...",
        "Relief lit from an angle over every finished frame.",
        emboss,
    ),
    (
        "movie.filter.mosaic",
        "image.filter.mosaic",
        "Movie/Filters/Mosaic...",
        "Replace blocks of pixels by their average in every finished frame.",
        mosaic,
    ),
    (
        "movie.filter.oilPaint",
        "image.filter.oilPaint",
        "Movie/Filters/Oil Paint...",
        "Paint-like smoothing of every finished frame.",
        oil_paint,
    ),
    (
        "movie.filter.splash",
        "image.filter.splash",
        "Movie/Filters/Splash...",
        "Scatter pixels like spattered paint (repeatable by seed).",
        splash,
    ),
    (
        "movie.filter.cylinder",
        "image.filter.cylinder",
        "Movie/Filters/Cylinder...",
        "Bend every finished frame around a cylinder.",
        cylinder,
    ),
    (
        "movie.filter.whirl",
        "image.filter.whirl",
        "Movie/Filters/Whirl...",
        "Twist the area around a centre point of every finished frame.",
        whirl,
    ),
    (
        "movie.filter.pinch",
        "image.filter.pinch",
        "Movie/Filters/Pinch...",
        "Pinch or bulge the area around a centre point of every finished frame.",
        pinch,
    ),
    (
        "movie.filter.remove",
        "image.filter.remove",
        "",
        "Remove one filter from the movie's (or a sequence's) list.",
        remove,
    ),
    (
        "movie.filter.move",
        "image.filter.move",
        "",
        "Change the position of a filter in the movie's (or a sequence's) list.",
        move_filter,
    ),
    (
        "movie.filter.clear",
        "image.filter.clear",
        "Movie/Filters/Remove All",
        "Remove every filter from the movie (or a sequence).",
        clear,
    ),
];

pub(crate) fn specs() -> Vec<CommandSpec> {
    let base = |id, label, menu, description, params, run: Run| CommandSpec {
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
    };
    let mut out = vec![base(
        "movie.adjust",
        "Adjust",
        "Movie/Adjust...",
        "Brightness, contrast, red/green/blue balance, hue, saturation, negative and grayscale for every finished frame of the movie.",
        derived(ADJUST_PARAMS, false, true),
        adjust,
    )];
    let images = crate::filter_cmds::specs();
    for (id, follows, menu, description, run) in FILTERS {
        let Some(image) = images.iter().find(|s| s.id == *follows) else {
            continue;
        };
        out.push(base(
            id,
            image.label,
            menu,
            description,
            derived(image.params, true, false),
            *run,
        ));
    }
    out
}
