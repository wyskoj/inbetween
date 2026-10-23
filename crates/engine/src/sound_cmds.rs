//! `effect.sound`: the sound under the movie.
//!
//! Called with no setting parameters it reports. Giving a `path` probes the file now (a bad
//! file is refused at once) and switches the sound on unless `enabled` says otherwise.

use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{ImageRef, Sound};
use serde_json::{json, Value};

pub(crate) const SOUND_EXTENSIONS: &[&str] = &["wav", "mp3"];

const PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "path",
        ParamKind::Path,
        false,
        "Sound file under the movie (WAV or MP3); setting it switches the sound on",
    )
    .open_file(SOUND_EXTENSIONS),
    ParamSpec::new(
        "enabled",
        ParamKind::Bool,
        false,
        "Switch the sound on or off; omit to leave it as it is",
    ),
    ParamSpec::new(
        "loop",
        ParamKind::Bool,
        false,
        "Repeat the sound when it is shorter than the movie",
    ),
    ParamSpec::new(
        "cut",
        ParamKind::Bool,
        false,
        "End the sound where the movie ends when it is longer",
    )
    .default_is("true"),
    ParamSpec::new(
        "fade_in",
        ParamKind::Number,
        false,
        "Fade-in in seconds, 0 to 60",
    )
    .range(0.0, 60.0)
    .default_is("0"),
    ParamSpec::new(
        "fade_out",
        ParamKind::Number,
        false,
        "Fade-out in seconds, 0 to 60",
    )
    .range(0.0, 60.0)
    .default_is("0"),
    ParamSpec::new(
        "clear",
        ParamKind::Bool,
        false,
        "Remove the sound from the project",
    ),
];

/// Length, rate and channel count of the sound file at `reference`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn probe(
    session: &Session,
    reference: &str,
) -> Result<inbetween_audio::Probe, CommandError> {
    inbetween_format::check_image_path(reference)?;
    let full = session.resolve_image_path(reference);
    inbetween_audio::probe_file(&full)
        .map_err(|e| CommandError::Image(format!("the sound {}: {e}", full.display())))
}

fn sound(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let mut sound = if params.boolean("clear") == Some(true) {
        None
    } else {
        session.project().sound.clone()
    };
    if let Some(raw) = params.string("path") {
        let path = std::path::absolute(raw)
            .map_err(|e| CommandError::Io(e.to_string()))?
            .display()
            .to_string();
        #[cfg(not(target_arch = "wasm32"))]
        probe(session, &path)?;
        let s = sound.get_or_insert_with(Sound::default);
        s.path = ImageRef(path);
        s.enabled = true;
    }
    let touched = ["enabled", "loop", "cut", "fade_in", "fade_out"]
        .iter()
        .any(|n| params.has(n));
    if touched && sound.is_none() {
        return Err(CommandError::BadParams(
            "there is no sound yet; give a `path` first".into(),
        ));
    }
    if let Some(s) = sound.as_mut() {
        if let Some(v) = params.boolean("enabled") {
            s.enabled = v;
        }
        if let Some(v) = params.boolean("loop") {
            s.looped = v;
        }
        if let Some(v) = params.boolean("cut") {
            s.cut_at_end = v;
        }
        if let Some(v) = params.number("fade_in") {
            s.fade_in = v;
        }
        if let Some(v) = params.number("fade_out") {
            s.fade_out = v;
        }
        s.validate().map_err(CommandError::Doc)?;
        if s.enabled && s.path.0.is_empty() {
            return Err(CommandError::BadParams(
                "the sound needs a `path` before it can be switched on".into(),
            ));
        }
    }
    session.project_mut().sound = sound.clone();
    let Some(s) = sound else {
        return Ok(json!({"sound": false}));
    };
    #[cfg(not(target_arch = "wasm32"))]
    let probed = if s.path.0.is_empty() {
        None
    } else {
        probe(session, &s.path.0).ok()
    };
    #[cfg(not(target_arch = "wasm32"))]
    let (duration, rate, channels) = probed.map_or((json!(null), json!(null), json!(null)), |p| {
        (json!(p.duration), json!(p.rate), json!(p.channels))
    });
    #[cfg(target_arch = "wasm32")]
    let (duration, rate, channels) = (json!(null), json!(null), json!(null));
    Ok(json!({
        "sound": true,
        "enabled": s.enabled,
        "path": s.path.0,
        "loop": s.looped,
        "cut": s.cut_at_end,
        "fade_in": s.fade_in,
        "fade_out": s.fade_out,
        "duration": duration,
        "rate": rate,
        "channels": channels,
    }))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "effect.sound",
        label: "Sound",
        menu: "Movie/Sound...",
        shortcut: None,
        description: "A WAV or MP3 file played under the movie, with looping, cutting and fades. Video exports mux it in.",
        params: PARAMS,
        enabled: Session::has_sequences,
        disabled_reason: "the project has no sequences",
        edits: true,
        run: sound,
    }]
}
