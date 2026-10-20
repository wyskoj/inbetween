//! Animated and video export: `export.animation` (GIF, APNG, animated WebP) and `export.video`
//! (MP4, WebM, ProRes through an installed `ffmpeg`).

use crate::CommandSpec;

pub(crate) fn specs() -> Vec<CommandSpec> {
    #[cfg(not(target_arch = "wasm32"))]
    return files::specs();
    #[cfg(target_arch = "wasm32")]
    return Vec::new();
}

#[cfg(not(target_arch = "wasm32"))]
mod files {
    use super::*;
    use crate::movie::{self, MovieRange, MAX_MOVIE_FRAMES};
    use crate::{CommandError, ParamKind, ParamSpec, Params, Session};
    use inbetween_export::video::{VideoFormat, VideoOptions, VideoWriter};
    use inbetween_export::{
        encode_apng, encode_gif, encode_webp, AnimationOptions, ExportError, Frames, Loop,
        MAX_BUFFERED_BYTES,
    };
    use serde_json::{json, Value};
    use std::path::Path;

    const FIRST: ParamSpec = ParamSpec::new(
        "first",
        ParamKind::Integer,
        false,
        "First movie frame to include, counting from 1",
    )
    .range(1.0, MAX_MOVIE_FRAMES as f64)
    .default_is("1");

    const LAST: ParamSpec = ParamSpec::new(
        "last",
        ParamKind::Integer,
        false,
        "Last movie frame to include, counting from 1",
    )
    .range(1.0, MAX_MOVIE_FRAMES as f64)
    .default_is("the last frame");

    const ONLY_SEQUENCE: ParamSpec = ParamSpec::new(
        "sequence",
        ParamKind::Integer,
        false,
        "Export only this sequence (numbered from 0); `first` and `last` then count inside it",
    )
    .range(0.0, 1_000_000.0)
    .default_is("the whole movie");

    const REVERSE: ParamSpec = ParamSpec::new(
        "reverse",
        ParamKind::Bool,
        false,
        "Play the movie backwards",
    );

    const DROP_DUPLICATES: ParamSpec = ParamSpec::new(
        "drop_duplicates",
        ParamKind::Bool,
        false,
        "Skip a sequence's first or last frame when it repeats the frame before it (the join of continued sequences)",
    );

    const MATTE: ParamSpec = ParamSpec::new(
        "matte",
        ParamKind::String,
        false,
        "Colour that transparent areas are flattened over, as #rrggbb, when the output has no alpha",
    )
    .default_is("#ffffff");

    const ANIMATION_FORMATS: &[&str] = &["gif", "apng", "webp"];
    const VIDEO_FORMATS: &[&str] = &["mp4", "webm", "prores"];

    const ANIMATION_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "out",
            ParamKind::Path,
            true,
            "File to write (.gif, .png or .apng, .webp)",
        )
        .save_file(&["gif", "png", "apng", "webp"]),
        ParamSpec::new(
            "format",
            ParamKind::String,
            false,
            "gif, apng or webp; by default taken from the file extension",
        )
        .one_of(ANIMATION_FORMATS),
        ParamSpec::new(
            "fps",
            ParamKind::Number,
            false,
            "Frames per second, 1 to 60",
        )
        .range(1.0, 60.0)
        .default_is("the movie frame rate"),
        ParamSpec::new(
            "loop",
            ParamKind::Bool,
            false,
            "Repeat forever (true) or play once (false)",
        )
        .default_is("true"),
        ParamSpec::new(
            "transparent",
            ParamKind::Bool,
            false,
            "Keep transparency (GIF keeps only on or off); false flattens over `matte`",
        )
        .default_is("true"),
        MATTE,
        REVERSE,
        DROP_DUPLICATES,
        ONLY_SEQUENCE,
        FIRST,
        LAST,
    ];

    const VIDEO_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "out",
            ParamKind::Path,
            true,
            "File to write (.mp4, .webm or .mov)",
        )
        .save_file(&["mp4", "webm", "mov"]),
        ParamSpec::new(
            "format",
            ParamKind::String,
            false,
            "mp4 (H.264), webm (VP9 with alpha) or prores (ProRes 4444 with alpha); by default taken from the file extension",
        )
        .one_of(VIDEO_FORMATS),
        ParamSpec::new("fps", ParamKind::Number, false, "Frames per second, 1 to 60")
            .range(1.0, 60.0)
            .default_is("the movie frame rate"),
        MATTE,
        ParamSpec::new(
            "ffmpeg",
            ParamKind::String,
            false,
            "Path to the ffmpeg program; by default it is found on PATH",
        ),
        ParamSpec::new(
            "audio",
            ParamKind::Bool,
            false,
            "Mux the project's sound (see effect.sound) into the video; false leaves it out",
        )
        .default_is("true"),
        REVERSE,
        DROP_DUPLICATES,
        ONLY_SEQUENCE,
        FIRST,
        LAST,
    ];

    fn has_sequence(session: &Session) -> bool {
        session.has_sequences()
    }

    fn extension_of(path: &str) -> String {
        Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
    }

    fn matte(params: &Params) -> Result<[u8; 3], CommandError> {
        match params.string("matte") {
            None => Ok([255, 255, 255]),
            Some(text) => {
                let [r, g, b, _] = crate::effect_cmds::parse_color(text)?;
                Ok([r, g, b])
            }
        }
    }

    fn fps(params: &Params, session: &Session) -> f64 {
        params.number("fps").unwrap_or(session.project().fps)
    }

    fn write_file(path: &Path, bytes: &[u8]) -> Result<(), CommandError> {
        std::fs::write(path, bytes)
            .map_err(|e| CommandError::Io(format!("{}: {e}", path.display())))
    }

    fn make_parent(path: &Path) -> Result<(), CommandError> {
        match path.parent().filter(|p| !p.as_os_str().is_empty()) {
            Some(dir) => std::fs::create_dir_all(dir)
                .map_err(|e| CommandError::Io(format!("{}: {e}", dir.display()))),
            None => Ok(()),
        }
    }

    /// How many frames `range` selects, before dedupe.
    fn selected(range: &MovieRange) -> u64 {
        range.last.saturating_sub(range.first).saturating_add(1)
    }

    fn animation(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let out = params.string("out").unwrap_or_default();
        let format = match params.string("format") {
            Some(f) => f.to_string(),
            None => match extension_of(out).as_str() {
                "gif" => "gif".into(),
                "png" | "apng" => "apng".into(),
                "webp" => "webp".into(),
                other => {
                    return Err(CommandError::BadParams(format!(
                        "cannot tell the animation format from `.{other}`; use .gif, .png, .apng or .webp, or give `format`"
                    )))
                }
            },
        };
        let opts = AnimationOptions {
            fps: fps(params, session),
            looping: if params.boolean("loop") == Some(false) {
                Loop::Once
            } else {
                Loop::Forever
            },
            transparent: params.boolean("transparent") != Some(false),
            matte: matte(params)?,
        };
        let range = movie::range_from(session, params)?;
        let (width, height) = (
            session.project().output.width,
            session.project().output.height,
        );
        // Refuse before rendering anything if the frames cannot be held in memory.
        let bytes = u64::from(width) * u64::from(height) * 4;
        if bytes.saturating_mul(selected(&range)) > MAX_BUFFERED_BYTES {
            return Err(ExportError::TooMuchData {
                frames: usize::try_from(selected(&range)).unwrap_or(usize::MAX),
                width,
                height,
            }
            .into());
        }
        inbetween_export::check_fps(opts.fps)?;

        let mut frames: Vec<Vec<u8>> = Vec::new();
        movie::render_movie(session, range, &mut |_, image| {
            frames.push(image.to_rgba8());
            Ok(())
        })?;
        let set = Frames::new(width, height, &frames)?;
        let encoded = match format.as_str() {
            "gif" => encode_gif(set, &opts)?,
            "apng" => encode_apng(set, &opts)?,
            "webp" => encode_webp(set, &opts)?,
            other => {
                return Err(CommandError::BadParams(format!(
                    "unknown animation format `{other}`"
                )))
            }
        };
        let path = Path::new(out);
        make_parent(path)?;
        write_file(path, &encoded)?;
        Ok(json!({
            "out": out,
            "format": format,
            "frames": frames.len(),
            "fps": opts.fps,
            "width": width,
            "height": height,
            "bytes": encoded.len(),
            "audio": if session.project().sound.as_ref().is_some_and(|s| s.is_active()) { json!("skipped (format has no sound)") } else { json!("none") },
        }))
    }

    fn video(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let out = params.string("out").unwrap_or_default();
        let format = match params.string("format") {
            Some(id) => VideoFormat::from_id(id),
            None => match extension_of(out).as_str() {
                "mp4" => Some(VideoFormat::Mp4),
                "webm" => Some(VideoFormat::WebM),
                "mov" => Some(VideoFormat::ProRes),
                _ => None,
            },
        }
        .ok_or_else(|| {
            CommandError::BadParams(
                "cannot tell the video format; use .mp4, .webm or .mov, or give `format`".into(),
            )
        })?;
        let opts = VideoOptions {
            format,
            fps: fps(params, session),
            matte: matte(params)?,
            ffmpeg: params.string("ffmpeg").map(Into::into),
            audio: None,
        };
        let range = movie::range_from(session, params)?;
        let mut opts = opts;
        let mut audio_report = json!("none");
        if let Some(sound) = session
            .project()
            .sound
            .clone()
            .filter(|s| s.is_active() && params.boolean("audio") != Some(false))
        {
            let probe = crate::sound_cmds::probe(session, &sound.path.0)?;
            let frames = range.last - range.first + 1;
            opts.audio = Some(inbetween_export::video::AudioMux {
                path: session.resolve_image_path(&sound.path.0),
                looped: sound.looped,
                cut_at_end: sound.cut_at_end,
                fade_in: sound.fade_in,
                fade_out: sound.fade_out,
                audio_seconds: probe.duration,
                movie_seconds: frames as f64 / opts.fps,
            });
            audio_report = json!(sound.path.0);
        }
        let (width, height) = (
            session.project().output.width,
            session.project().output.height,
        );
        let path = Path::new(out);
        make_parent(path)?;
        let mut writer = VideoWriter::start(width, height, path, &opts)?;
        let frames = movie::render_movie(session, range, &mut |_, image| {
            writer.push(&image.to_rgba8()).map_err(Into::into)
        })?;
        writer.finish()?;
        Ok(json!({
            "out": out,
            "format": format.id(),
            "frames": frames,
            "audio": audio_report,
            "fps": opts.fps,
            "width": width,
            "height": height,
        }))
    }

    pub(super) fn specs() -> Vec<CommandSpec> {
        vec![
            CommandSpec {
                id: "export.animation",
                label: "Export Animated Image",
                menu: "File/Export/Animated Image...",
                shortcut: None,
                description: "Render the whole movie to an animated GIF, APNG or WebP.",
                params: ANIMATION_PARAMS,
                enabled: has_sequence,
                disabled_reason: "the project has no sequences to render",
                edits: false,
                run: animation,
            },
            CommandSpec {
                id: "export.video",
                label: "Export Video",
                menu: "File/Export/Video...",
                shortcut: None,
                description:
                    "Render the whole movie to MP4, WebM or ProRes video. Needs ffmpeg installed.",
                params: VIDEO_PARAMS,
                enabled: has_sequence,
                disabled_reason: "the project has no sequences to render",
                edits: false,
                run: video,
            },
        ]
    }
}
