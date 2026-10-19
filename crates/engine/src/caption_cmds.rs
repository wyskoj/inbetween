//! Captions at three scopes: `image.caption` (one source image, so it morphs with the picture),
//! `caption.sequence` (every frame of a sequence) and `caption.movie` (every frame of the
//! movie).
//!
//! The three share one set of fields, all optional and merged into the existing caption. A
//! call with no field changes the caption's settings and reports them; `clear` removes it.
//! Giving `text` for the first time creates the caption with the default look.

use crate::effect_cmds::{parse_color, SEQUENCE};
use crate::image_cmds::{run_edit, IMAGE};
use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{Align, Caption, ImageRef};
use serde_json::{json, Value};

macro_rules! caption_fields {
    ($($lead:expr),*) => {
        &[
            $($lead,)*
            ParamSpec::new("text", ParamKind::String, false, "The caption text; \\n starts a new line"),
            ParamSpec::new(
                "font",
                ParamKind::Path,
                false,
                "A .ttf or .otf font file; an empty path goes back to the bundled font",
            )
            .open_file(&["ttf", "otf"]),
            ParamSpec::new("size", ParamKind::Number, false, "Font size in pixels of the frame, 1 to 4000")
                .range(1.0, 4000.0)
                .default_is("48"),
            ParamSpec::new("bold", ParamKind::Bool, false, "Bold"),
            ParamSpec::new("italic", ParamKind::Bool, false, "Italic"),
            ParamSpec::new("color", ParamKind::String, false, "Text colour as #rrggbb or #rrggbbaa")
                .default_is("#ffffff"),
            ParamSpec::new("border_size", ParamKind::Number, false, "Outline width in pixels; 0 for none")
                .range(0.0, 4000.0)
                .default_is("0"),
            ParamSpec::new("border_color", ParamKind::String, false, "Outline colour as #rrggbb or #rrggbbaa")
                .default_is("#000000"),
            ParamSpec::new(
                "align",
                ParamKind::String,
                false,
                "How the text sits on its anchor: left, center or right",
            )
            .one_of(&["left", "center", "right"])
            .default_is("center"),
            ParamSpec::new(
                "x",
                ParamKind::Number,
                false,
                "Anchor across the frame, 0 (left) to 1 (right); may lie outside",
            )
            .range(-10.0, 10.0)
            .default_is("0.5"),
            ParamSpec::new(
                "y",
                ParamKind::Number,
                false,
                "Anchor down the frame, 0 (top) to 1 (bottom): where the middle of the text sits",
            )
            .range(-10.0, 10.0)
            .default_is("0.9"),
            ParamSpec::new(
                "shadow_size",
                ParamKind::Number,
                false,
                "Shadow blur in pixels; 0 draws no shadow",
            )
            .range(0.0, 4000.0)
            .default_is("0"),
            ParamSpec::new(
                "shadow_distance",
                ParamKind::Number,
                false,
                "How far the shadow is offset, in pixels",
            )
            .range(-4000.0, 4000.0)
            .default_is("4"),
            ParamSpec::new(
                "shadow_angle",
                ParamKind::Number,
                false,
                "Direction the shadow falls, in degrees counter-clockwise from the right",
            )
            .range(-3600.0, 3600.0)
            .default_is("-45"),
            ParamSpec::new("shadow_color", ParamKind::String, false, "Shadow colour as #rrggbb or #rrggbbaa")
                .default_is("#000000a0"),
            ParamSpec::new("enabled", ParamKind::Bool, false, "Show or hide the caption without losing its settings"),
            ParamSpec::new("clear", ParamKind::Bool, false, "Remove the caption"),
        ]
    };
}

const IMAGE_PARAMS: &[ParamSpec] = caption_fields!(SEQUENCE, IMAGE);
const SEQUENCE_PARAMS: &[ParamSpec] = caption_fields!(SEQUENCE);
const MOVIE_PARAMS: &[ParamSpec] = caption_fields!();

const FIELD_NAMES: &[&str] = &[
    "text",
    "font",
    "size",
    "bold",
    "italic",
    "color",
    "border_size",
    "border_color",
    "align",
    "x",
    "y",
    "shadow_size",
    "shadow_distance",
    "shadow_angle",
    "shadow_color",
    "enabled",
];

/// The absolute path of the `font` parameter (empty stays empty: the bundled font).
fn font_path(params: &Params) -> Result<Option<String>, CommandError> {
    let Some(raw) = params.string("font") else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Ok(Some(String::new()));
    }
    std::path::absolute(raw)
        .map(|p| Some(p.display().to_string()))
        .map_err(|e| CommandError::Io(format!("{raw}: {e}")))
}

/// Merge the given fields into `slot` (creating the caption if needed), or clear it.
fn apply(
    slot: &mut Option<Caption>,
    params: &Params,
    font: Option<String>,
) -> Result<(), CommandError> {
    if params.boolean("clear").unwrap_or(false) {
        *slot = None;
        return Ok(());
    }
    if !FIELD_NAMES.iter().any(|n| params.has(n)) {
        return Ok(());
    }
    let c = slot.get_or_insert_with(Caption::default);
    if let Some(v) = params.string("text") {
        c.text = v.to_string();
    }
    if let Some(f) = font {
        c.font = (!f.is_empty()).then_some(ImageRef(f));
    }
    if let Some(v) = params.number("size") {
        c.size = v;
    }
    if let Some(v) = params.boolean("bold") {
        c.bold = v;
    }
    if let Some(v) = params.boolean("italic") {
        c.italic = v;
    }
    if let Some(v) = params.string("color") {
        c.color = parse_color(v)?;
    }
    if let Some(v) = params.number("border_size") {
        c.border_size = v;
    }
    if let Some(v) = params.string("border_color") {
        c.border_color = parse_color(v)?;
    }
    if let Some(v) = params.string("align") {
        c.align = Align::from_id(v).ok_or_else(|| {
            CommandError::BadParams(format!("unknown align `{v}` (left, center or right)"))
        })?;
    }
    if let Some(v) = params.number("x") {
        c.x = v;
    }
    if let Some(v) = params.number("y") {
        c.y = v;
    }
    if let Some(v) = params.number("shadow_size") {
        c.shadow.size = v;
    }
    if let Some(v) = params.number("shadow_distance") {
        c.shadow.distance = v;
    }
    if let Some(v) = params.number("shadow_angle") {
        c.shadow.angle = v;
    }
    if let Some(v) = params.string("shadow_color") {
        c.shadow.color = parse_color(v)?;
    }
    if let Some(v) = params.boolean("enabled") {
        c.enabled = v;
    }
    c.validate().map_err(CommandError::Doc)
}

/// Fonts are files, which this build cannot read.
#[cfg(target_arch = "wasm32")]
fn check_font(_: &mut Session, font: &Option<String>) -> Result<(), CommandError> {
    if font.as_ref().is_some_and(|p| !p.is_empty()) {
        return Err(CommandError::Image(
            "font files cannot be read in this build".into(),
        ));
    }
    Ok(())
}

/// Refuse a font that cannot be read now, not at the next render.
#[cfg(not(target_arch = "wasm32"))]
fn check_font(session: &mut Session, font: &Option<String>) -> Result<(), CommandError> {
    if let Some(path) = font.as_ref().filter(|p| !p.is_empty()) {
        let probe = Caption {
            font: Some(ImageRef(path.clone())),
            ..Caption::default()
        };
        session.caption_font(&probe)?;
    }
    Ok(())
}

fn report(caption: &Option<Caption>) -> Result<Value, CommandError> {
    serde_json::to_value(caption).map_err(|e| CommandError::Internal(e.to_string()))
}

fn image_caption(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let font = font_path(params)?;
    check_font(session, &font)?;
    run_edit(session, params, false, |e, _| {
        apply(&mut e.caption, params, font)
    })
}

fn sequence_caption(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let font = font_path(params)?;
    check_font(session, &font)?;
    let seq = session.sequence_mut(index)?;
    apply(&mut seq.caption, params, font)?;
    Ok(json!({ "sequence": index, "caption": report(&seq.caption)? }))
}

fn movie_caption(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let font = font_path(params)?;
    check_font(session, &font)?;
    let project = session.project_mut();
    apply(&mut project.caption, params, font)?;
    Ok(json!({ "caption": report(&project.caption)? }))
}

fn always(_: &Session) -> bool {
    true
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    let base =
        |id, label, menu, description, params, enabled: fn(&Session) -> bool, run| CommandSpec {
            id,
            label,
            menu,
            shortcut: None,
            description,
            params,
            enabled,
            disabled_reason: "the project has no sequences",
            edits: true,
            run,
        };
    vec![
        base(
            "image.caption",
            "Image Caption",
            "Image/Caption...",
            "Text drawn on one source image; it moves and morphs with the picture.",
            IMAGE_PARAMS,
            Session::has_sequences,
            image_caption,
        ),
        base(
            "caption.sequence",
            "Sequence Caption",
            "Sequence/Caption...",
            "Text drawn over every frame of a sequence.",
            SEQUENCE_PARAMS,
            Session::has_sequences,
            sequence_caption,
        ),
        base(
            "caption.movie",
            "Movie Caption",
            "Movie/Caption...",
            "Text drawn over every frame of the movie.",
            MOVIE_PARAMS,
            always,
            movie_caption,
        ),
    ]
}
