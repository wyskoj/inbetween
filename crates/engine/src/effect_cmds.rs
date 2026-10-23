//! Effect commands: `effect.backColor`, `effect.background`, `effect.mask`, `effect.foreground`.
//!
//! Each reports its settings when called with no setting parameters. Giving an `image` loads
//! it (a bad file is refused now, not at the next render) and switches the effect on unless
//! `enabled` says otherwise. `clear` resets the effect to its defaults and removes the image.

use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{BackColor, Effects, ImageLayer, ImageRef, Mask};
use serde_json::{json, Value};

pub(crate) const SEQUENCE: ParamSpec = ParamSpec::new(
    "sequence",
    ParamKind::Integer,
    false,
    "Sequence number, starting at 0",
)
.range(0.0, 1_000_000.0)
.default_is("0");

pub(crate) const SCOPE: ParamSpec = ParamSpec::new(
    "scope",
    ParamKind::String,
    false,
    "`sequence` for one sequence's effect (see `sequence`), `movie` for an effect over the whole movie",
)
.one_of(&["sequence", "movie"])
.default_is("sequence");

pub(crate) const ENABLED: ParamSpec = ParamSpec::new(
    "enabled",
    ParamKind::Bool,
    false,
    "Switch the effect on or off; omit to leave it as it is",
);

pub(crate) const CLEAR: ParamSpec = ParamSpec::new(
    "clear",
    ParamKind::Bool,
    false,
    "Reset the effect to its defaults and remove its image",
);

pub(crate) use inbetween_codecs::IMAGE_EXTENSIONS;

const BACK_COLOR_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    SCOPE,
    ParamSpec::new(
        "color",
        ParamKind::String,
        false,
        "Colour as #rrggbb or #rrggbbaa; setting it switches the effect on",
    ),
    ENABLED,
    CLEAR,
];

const BACKGROUND_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    SCOPE,
    ParamSpec::new(
        "image",
        ParamKind::Path,
        false,
        "Image shown behind the morph (PNG, JPEG, BMP, TIFF, TGA, GIF or PSD); setting it switches the effect on",
    )
    .open_file(IMAGE_EXTENSIONS),
    ParamSpec::new("opacity", ParamKind::Number, false, "Opacity, 0 to 100")
        .range(0.0, 100.0)
        .default_is("100"),
    ENABLED,
    CLEAR,
];

const FOREGROUND_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    SCOPE,
    ParamSpec::new(
        "image",
        ParamKind::Path,
        false,
        "Overlay drawn over the morph (PNG with alpha, or JPEG); setting it switches the effect on",
    )
    .open_file(IMAGE_EXTENSIONS),
    ParamSpec::new("opacity", ParamKind::Number, false, "Opacity, 0 to 100")
        .range(0.0, 100.0)
        .default_is("100"),
    ENABLED,
    CLEAR,
];

const MASK_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    SCOPE,
    ParamSpec::new(
        "image",
        ParamKind::Path,
        false,
        "Grayscale image: light areas show the morph, dark areas hide it (PNG, JPEG, BMP, TIFF, TGA, GIF or PSD); setting it switches the effect on",
    )
    .open_file(IMAGE_EXTENSIONS),
    ParamSpec::new(
        "intensity",
        ParamKind::Number,
        false,
        "-100 to 100. Positive hides the morph under dark areas, negative under light areas",
    )
    .range(-100.0, 100.0)
    .default_is("100"),
    ParamSpec::new(
        "invert",
        ParamKind::Bool,
        false,
        "Swap light and dark in the mask",
    ),
    ENABLED,
    CLEAR,
];

/// Parse `#rrggbb` or `#rrggbbaa`.
pub(crate) fn parse_color(text: &str) -> Result<[u8; 4], CommandError> {
    let bad = || {
        CommandError::BadParams(format!(
            "`color` must look like #rrggbb or #rrggbbaa, not `{text}`"
        ))
    };
    let hex = text.strip_prefix('#').ok_or_else(bad)?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad());
    }
    let byte =
        |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("ff"), 16).map_err(|_| bad());
    Ok([
        byte(0)?,
        byte(2)?,
        byte(4)?,
        if hex.len() == 8 { byte(6)? } else { 255 },
    ])
}

pub(crate) fn hex(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
}

/// What an effect command changes: one sequence's effects or the movie's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Sequence(usize),
    Movie,
}

impl Target {
    /// The sequence number, or `None` for the movie.
    pub(crate) fn index(self) -> Option<usize> {
        match self {
            Self::Sequence(i) => Some(i),
            Self::Movie => None,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Sequence(_) => "sequence",
            Self::Movie => "movie",
        }
    }
}

/// Read `scope` (and `sequence` for the sequence scope). With `scope=movie` the `sequence`
/// parameter is ignored, so a shell may always send it.
pub(crate) fn target(session: &Session, params: &Params) -> Result<Target, CommandError> {
    if params.string("scope") == Some("movie") {
        Ok(Target::Movie)
    } else {
        session.sequence_index(params).map(Target::Sequence)
    }
}

/// Load the `image` parameter (so a bad file is refused now) and return its absolute path.
pub(crate) fn take_image(
    session: &mut Session,
    params: &Params,
    target: &Target,
    name: &str,
) -> Result<Option<ImageRef>, CommandError> {
    let Some(raw) = params.string("image") else {
        return Ok(None);
    };
    let path = std::path::absolute(raw)
        .map_err(|e| CommandError::Io(e.to_string()))?
        .display()
        .to_string();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let loaded = match target {
            Target::Sequence(index) => session.load_image(&path, 0, *index),
            Target::Movie => session.load_effect_path(&path),
        };
        loaded.map_err(|e| match e {
            CommandError::Image(m) => CommandError::Image(format!("{name} effect: {m}")),
            other => other,
        })?;
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (session, target, name);
    Ok(Some(ImageRef(path)))
}

/// Refuse to leave an effect switched on without its image.
pub(crate) fn need_image(on: bool, image: &ImageRef, name: &str) -> Result<(), CommandError> {
    if on && image.0.is_empty() {
        return Err(CommandError::BadParams(format!(
            "the {name} effect needs an `image` before it can be switched on"
        )));
    }
    Ok(())
}

pub(crate) fn target_effects<'a>(
    session: &'a mut Session,
    target: &Target,
) -> Result<&'a mut Effects, CommandError> {
    match target {
        Target::Sequence(index) => Ok(&mut session.sequence_mut(*index)?.effects),
        Target::Movie => Ok(&mut session.project_mut().effects),
    }
}

fn back_color(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let target = target(session, params)?;
    let fx = target_effects(session, &target)?;
    if params.boolean("clear") == Some(true) {
        fx.back_color = BackColor::default();
    }
    if let Some(text) = params.string("color") {
        fx.back_color.rgba = parse_color(text)?;
        fx.back_color.enabled = true;
    }
    if let Some(on) = params.boolean("enabled") {
        fx.back_color.enabled = on;
    }
    Ok(json!({
        "sequence": target.index(),
        "scope": target.name(),
        "enabled": fx.back_color.enabled,
        "color": hex(fx.back_color.rgba),
    }))
}

fn image_layer(
    session: &mut Session,
    params: &Params,
    name: &'static str,
    pick: fn(&mut Effects) -> &mut ImageLayer,
) -> Result<Value, CommandError> {
    let target = target(session, params)?;
    let image = take_image(session, params, &target, name)?;
    let layer = pick(target_effects(session, &target)?);
    if params.boolean("clear") == Some(true) {
        *layer = ImageLayer::default();
    }
    if let Some(image) = image {
        layer.image = image;
        layer.enabled = true;
    }
    if let Some(opacity) = params.number("opacity") {
        layer.opacity = opacity;
    }
    if let Some(on) = params.boolean("enabled") {
        layer.enabled = on;
    }
    need_image(layer.enabled, &layer.image, name)?;
    Ok(json!({
        "sequence": target.index(),
        "scope": target.name(),
        "enabled": layer.enabled,
        "image": layer.image.0,
        "opacity": layer.opacity,
    }))
}

fn background(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    image_layer(session, params, "background", |fx| &mut fx.background)
}

fn foreground(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    image_layer(session, params, "foreground", |fx| &mut fx.foreground)
}

fn mask(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let target = target(session, params)?;
    let image = take_image(session, params, &target, "mask")?;
    let m = &mut target_effects(session, &target)?.mask;
    if params.boolean("clear") == Some(true) {
        *m = Mask::default();
    }
    if let Some(image) = image {
        m.image = image;
        m.enabled = true;
    }
    if let Some(v) = params.number("intensity") {
        m.intensity = v;
    }
    if let Some(v) = params.boolean("invert") {
        m.invert = v;
    }
    if let Some(on) = params.boolean("enabled") {
        m.enabled = on;
    }
    need_image(m.enabled, &m.image, "mask")?;
    Ok(json!({
        "sequence": target.index(),
        "scope": target.name(),
        "enabled": m.enabled,
        "image": m.image.0,
        "intensity": m.intensity,
        "invert": m.invert,
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
            "effect.backColor",
            "Background Color",
            "Effects/Background Color...",
            "Solid colour behind the morph (and behind the background image).",
            BACK_COLOR_PARAMS,
            back_color,
        ),
        spec(
            "effect.background",
            "Background Image",
            "Effects/Background Image...",
            "Image behind the morph, visible through transparent areas and the mask.",
            BACKGROUND_PARAMS,
            background,
        ),
        spec(
            "effect.mask",
            "Mask",
            "Effects/Mask...",
            "Grayscale image that controls the morph's opacity.",
            MASK_PARAMS,
            mask,
        ),
        spec(
            "effect.foreground",
            "Foreground Image",
            "Effects/Foreground Image...",
            "Image laid over the morph, such as a frame or graphic.",
            FOREGROUND_PARAMS,
            foreground,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse_and_reject() {
        assert_eq!(parse_color("#ff8000").ok(), Some([255, 128, 0, 255]));
        assert_eq!(parse_color("#00000080").ok(), Some([0, 0, 0, 128]));
        for bad in [
            "",
            "#",
            "ff8000",
            "#ff80",
            "#gg0000",
            "#ff80000",
            "#ff8000ffaa",
            "#\u{e9}\u{e9}\u{e9}",
        ] {
            assert!(parse_color(bad).is_err(), "{bad}");
        }
    }
}
