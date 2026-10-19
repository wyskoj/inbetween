//! More effect commands: `effect.transition`, `effect.light`, `effect.trail`, `effect.radial`.
//!
//! They follow the rules of the first four (`effect_cmds`): called with no setting parameters
//! they report; giving an `image` loads it and switches the effect on; `clear` resets it.

use crate::effect_cmds::{
    effects_of, hex, need_image, parse_color, take_image, CLEAR, ENABLED, IMAGE_EXTENSIONS,
    SEQUENCE,
};
use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{Light, Radial, Trail, Transition};
use serde_json::{json, Value};

const TRANSITION_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new(
        "image",
        ParamKind::Path,
        false,
        "Grayscale image that decides when each pixel dissolves: dark pixels first (PNG, JPEG, BMP, TIFF, TGA, GIF or PSD); setting it switches the effect on",
    )
    .open_file(IMAGE_EXTENSIONS),
    ParamSpec::new(
        "softness",
        ParamKind::Number,
        false,
        "Border softness, 0 (hard edge) to 100 (one long fade)",
    )
    .range(0.0, 100.0)
    .default_is("20"),
    ParamSpec::new(
        "invert",
        ParamKind::Bool,
        false,
        "Dissolve light pixels first instead of dark ones",
    ),
    ENABLED,
    CLEAR,
];

const LIGHT_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new(
        "image",
        ParamKind::Path,
        false,
        "Grayscale image: light areas brighten the morph, dark areas darken it (PNG, JPEG, BMP, TIFF, TGA, GIF or PSD); setting it switches the effect on",
    )
    .open_file(IMAGE_EXTENSIONS),
    ParamSpec::new(
        "bright",
        ParamKind::Number,
        false,
        "How strongly light areas brighten, 0 to 100",
    )
    .range(0.0, 100.0)
    .default_is("50"),
    ParamSpec::new(
        "dark",
        ParamKind::Number,
        false,
        "How strongly dark areas darken, 0 to 100",
    )
    .range(0.0, 100.0)
    .default_is("50"),
    ENABLED,
    CLEAR,
];

const TRAIL_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new(
        "overlay",
        ParamKind::Bool,
        false,
        "true: motion blur (earlier frames are averaged in); false: a motion trail (tinted copies fade out behind)",
    ),
    ParamSpec::new(
        "color",
        ParamKind::String,
        false,
        "Tint of the trail copies as #rrggbb or #rrggbbaa (a trail only)",
    ),
    ParamSpec::new(
        "intensity",
        ParamKind::Number,
        false,
        "0 to 100: how many earlier frames (up to 8) and how strong the trail is",
    )
    .range(0.0, 100.0)
    .default_is("50"),
    ENABLED,
    CLEAR,
];

const RADIAL_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new(
        "overlay",
        ParamKind::Bool,
        false,
        "true: rays over the morph; false: rays under it, showing through its transparent areas",
    ),
    ParamSpec::new(
        "color",
        ParamKind::String,
        false,
        "Ray colour as #rrggbb or #rrggbbaa",
    ),
    ParamSpec::new(
        "intensity",
        ParamKind::Number,
        false,
        "Ray strength, 0 to 100",
    )
    .range(0.0, 100.0)
    .default_is("50"),
    ENABLED,
    CLEAR,
];

fn transition(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let image = take_image(session, params, index, "transition")?;
    let t = &mut effects_of(session, index)?.transition;
    if params.boolean("clear") == Some(true) {
        *t = Transition::default();
    }
    if let Some(image) = image {
        t.image = image;
        t.enabled = true;
    }
    if let Some(v) = params.number("softness") {
        t.softness = v;
    }
    if let Some(v) = params.boolean("invert") {
        t.invert = v;
    }
    if let Some(on) = params.boolean("enabled") {
        t.enabled = on;
    }
    need_image(t.enabled, &t.image, "transition")?;
    Ok(json!({
        "sequence": index,
        "enabled": t.enabled,
        "image": t.image.0,
        "softness": t.softness,
        "invert": t.invert,
    }))
}

fn light(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let image = take_image(session, params, index, "light")?;
    let l = &mut effects_of(session, index)?.light;
    if params.boolean("clear") == Some(true) {
        *l = Light::default();
    }
    if let Some(image) = image {
        l.image = image;
        l.enabled = true;
    }
    if let Some(v) = params.number("bright") {
        l.bright = v;
    }
    if let Some(v) = params.number("dark") {
        l.dark = v;
    }
    if let Some(on) = params.boolean("enabled") {
        l.enabled = on;
    }
    need_image(l.enabled, &l.image, "light")?;
    Ok(json!({
        "sequence": index,
        "enabled": l.enabled,
        "image": l.image.0,
        "bright": l.bright,
        "dark": l.dark,
    }))
}

fn trail(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let color = params.string("color").map(parse_color).transpose()?;
    let t = &mut effects_of(session, index)?.trail;
    if params.boolean("clear") == Some(true) {
        *t = Trail::default();
    }
    if let Some(rgba) = color {
        t.rgba = rgba;
    }
    if let Some(v) = params.boolean("overlay") {
        t.overlay = v;
    }
    if let Some(v) = params.number("intensity") {
        t.intensity = v;
        // Asking for a strength is asking for the effect, unless told otherwise.
        t.enabled = true;
    }
    if let Some(on) = params.boolean("enabled") {
        t.enabled = on;
    }
    Ok(json!({
        "sequence": index,
        "enabled": t.enabled,
        "overlay": t.overlay,
        "color": hex(t.rgba),
        "intensity": t.intensity,
    }))
}

fn radial(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let color = params.string("color").map(parse_color).transpose()?;
    let r = &mut effects_of(session, index)?.radial;
    if params.boolean("clear") == Some(true) {
        *r = Radial::default();
    }
    if let Some(rgba) = color {
        r.rgba = rgba;
    }
    if let Some(v) = params.boolean("overlay") {
        r.overlay = v;
    }
    if let Some(v) = params.number("intensity") {
        r.intensity = v;
        r.enabled = true;
    }
    if let Some(on) = params.boolean("enabled") {
        r.enabled = on;
    }
    Ok(json!({
        "sequence": index,
        "enabled": r.enabled,
        "overlay": r.overlay,
        "color": hex(r.rgba),
        "intensity": r.intensity,
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
    vec![
        spec(
            "effect.transition",
            "Transition",
            "Effects/Transition...",
            "Grayscale image that controls when each pixel dissolves from Image 1 to Image 2: dark pixels first, with adjustable border softness.",
            TRANSITION_PARAMS,
            transition,
        ),
        spec(
            "effect.light",
            "Light",
            "Effects/Light...",
            "Grayscale image that brightens and darkens the morph, with separate bright and dark intensity.",
            LIGHT_PARAMS,
            light,
        ),
        spec(
            "effect.trail",
            "Trail",
            "Effects/Trail...",
            "Motion blur (overlay on) or a fading, tinted motion trail (overlay off) made from the frames just before each one.",
            TRAIL_PARAMS,
            trail,
        ),
        spec(
            "effect.radial",
            "Radial Rays",
            "Effects/Radial Rays...",
            "Rays radiating from the frame centre, over or under the morph, with a tint colour and intensity.",
            RADIAL_PARAMS,
            radial,
        ),
    ]
}
