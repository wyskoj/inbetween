//! Export commands: `export.frame`, `frame.exportEdit`, `export.sequence`, `export.movie`, `frame.preview`.

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
    use crate::movie::{self, MAX_MOVIE_FRAMES};
    use crate::Prepared;
    use crate::{CommandError, ParamKind, ParamSpec, Params, Session};
    use inbetween_codecs::{Rgba8Image, WriteOptions};
    use inbetween_doc::{DotPair, DotSet, MAX_FRAMES};
    use inbetween_morph::Planner;
    use inbetween_raster::Image;
    use serde_json::{json, Value};
    use std::path::{Path, PathBuf};

    const SEQUENCE: ParamSpec = ParamSpec::new(
        "sequence",
        ParamKind::Integer,
        false,
        "Sequence number, starting at 0",
    )
    .range(0.0, 1_000_000.0)
    .default_is("0");

    const FRAME_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "out",
            ParamKind::Path,
            true,
            "Image file to write (.png or .jpg)",
        )
        .save_file(&["png", "jpg", "jpeg"]),
        ParamSpec::new(
            "t",
            ParamKind::Number,
            false,
            "Morph time: 0 is Image 1, 1 is Image 2",
        )
        .range(0.0, 1.0),
        ParamSpec::new(
            "frame",
            ParamKind::Integer,
            false,
            "Frame number, starting at 0 (instead of `t`)",
        )
        .range(0.0, MAX_FRAMES as f64),
        SEQUENCE,
        ParamSpec::new(
            "movie_frame",
            ParamKind::Integer,
            false,
            "Movie frame number, starting at 1 across all visible sequences (layer projects: the stacked layers); instead of t, frame and sequence",
        )
        .range(1.0, MAX_FRAMES as f64),
        QUALITY,
        DPI,
    ];

    const QUALITY: ParamSpec = ParamSpec::new(
        "quality",
        ParamKind::Integer,
        false,
        "JPEG quality, 1 to 100 (PNG ignores it)",
    )
    .range(1.0, 100.0)
    .default_is("92");

    const DPI: ParamSpec = ParamSpec::new(
        "dpi",
        ParamKind::Integer,
        false,
        "Resolution to record in the file, in dots per inch (the pixels are unchanged)",
    )
    .range(1.0, inbetween_codecs::MAX_DPI as f64);

    fn write_options(params: &Params) -> WriteOptions {
        WriteOptions {
            quality: params
                .integer("quality")
                .map_or(92, |q| q.clamp(1, 100) as u8),
            dpi: params
                .integer("dpi")
                .map(|d| d.clamp(1, i64::from(inbetween_codecs::MAX_DPI)) as u32),
        }
    }

    const EDIT_PARAMS: &[ParamSpec] = &[
        FRAME_PARAMS[0],
        ParamSpec::new(
            "dots_out",
            ParamKind::Path,
            false,
            "Dot file to write: the dots as they sit in the frame, on both sides",
        )
        .save_file(&["inbetweendots"])
        .default_is("the image path with .inbetweendots"),
        FRAME_PARAMS[1],
        FRAME_PARAMS[2],
        SEQUENCE,
    ];

    const PREVIEW_PARAMS: &[ParamSpec] = &[
        FRAME_PARAMS[1],
        FRAME_PARAMS[2],
        SEQUENCE,
        FRAME_PARAMS[4],
        ParamSpec::new(
            "max_size",
            ParamKind::Integer,
            false,
            "Longest side of the returned picture in pixels (smaller frames are not enlarged)",
        )
        .range(16.0, 4096.0)
        .default_is("512"),
    ];

    const SEQUENCE_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "out_dir",
            ParamKind::Path,
            true,
            "Folder for the PNG frames (created if missing)",
        )
        .folder(),
        ParamSpec::new(
            "prefix",
            ParamKind::String,
            false,
            "File name prefix: PREFIX_0001.png, ...",
        )
        .default_is("frame"),
        ParamSpec::new(
            "first",
            ParamKind::Integer,
            false,
            "First frame to write, counting from 1",
        )
        .range(1.0, MAX_FRAMES as f64)
        .default_is("1"),
        ParamSpec::new(
            "last",
            ParamKind::Integer,
            false,
            "Last frame to write, counting from 1",
        )
        .range(1.0, MAX_FRAMES as f64)
        .default_is("the last frame"),
        SEQUENCE,
        DPI,
    ];

    fn has_sequence(session: &Session) -> bool {
        session.has_sequences()
    }

    fn write_image(path: &Path, image: &Image) -> Result<(), CommandError> {
        write_image_with(path, image, WriteOptions::default())
    }

    fn write_image_with(
        path: &Path,
        image: &Image,
        options: WriteOptions,
    ) -> Result<(), CommandError> {
        let rgba = Rgba8Image::new(image.width(), image.height(), image.to_rgba8())?;
        inbetween_codecs::write_file_with(path, &rgba, options)?;
        Ok(())
    }

    /// Render the frame `t` or `frame` picks (frame 0 by default): the sequence index, the
    /// image, and the morph time it shows.
    fn render_one(
        session: &mut Session,
        params: &Params,
    ) -> Result<(usize, Image, f64, Prepared), CommandError> {
        if params.has("t") && params.has("frame") {
            return Err(CommandError::BadParams(
                "give either `t` or `frame`, not both".into(),
            ));
        }
        let index = session.sequence_index(params)?;
        let prepared = session.prepare(index)?;
        let renderer = prepared.renderer()?;
        let (image, t) = match (params.number("t"), params.integer("frame")) {
            (Some(t), _) => (renderer.render_t(t)?, t),
            (None, Some(n)) => (
                renderer.render_frame(n as u32)?,
                prepared.sequence.frame_time(n as u32),
            ),
            (None, None) => (renderer.render_frame(0)?, prepared.sequence.frame_time(0)),
        };
        drop(renderer);
        let local = params.integer("frame").map_or(0, |n| n as u32);
        let image = prepared.finish(image, local)?;
        Ok((index, image, t, prepared))
    }

    fn frame(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let image = match params.integer("movie_frame") {
            Some(n) => {
                if params.has("t") || params.has("frame") || params.has("sequence") {
                    return Err(CommandError::BadParams(
                        "`movie_frame` replaces `t`, `frame` and `sequence`; give only one way to pick the frame"
                            .into(),
                    ));
                }
                session.render_movie_frame(u64::try_from(n).unwrap_or(0))?
            }
            None => render_one(session, params)?.1,
        };
        let out = params.string("out").unwrap_or_default();
        write_image_with(Path::new(out), &image, write_options(params))?;
        Ok(json!({
            "out": out,
            "width": image.width(),
            "height": image.height(),
        }))
    }

    /// A frame as an image plus its dots, ready to be edited as a new starting point: the
    /// dots sit where the frame draws them, the same on both sides of every pair.
    fn export_edit(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let (index, image, t, prepared) = render_one(session, params)?;
        let out = params.string("out").unwrap_or_default();
        let dots_path = match params.string("dots_out") {
            Some(p) => PathBuf::from(p),
            None => Path::new(out).with_extension("inbetweendots"),
        };
        let seq = &prepared.sequence;
        let planned = Planner::new(seq, prepared.output.0, prepared.output.1)
            .map_err(|e| CommandError::Render(e.into()))?
            .dots_at(t);
        // Morph types that never warp ignore their dots: place them by the shape curve alone.
        let positions = planned.unwrap_or_else(|| seq.dots.interpolated(seq.shape_curve.eval(t)));
        let dots = DotSet::new(positions.iter().map(|p| DotPair::fixed(*p)).collect());
        // Dots first, so a bad dot path stops before an image is written; if the image then
        // fails, take back the dot file this call created.
        let dots_existed = dots_path.exists();
        inbetween_format::write_dots(&dots_path, &dots)?;
        if let Err(e) = write_image(Path::new(out), &image) {
            if !dots_existed {
                let _ = std::fs::remove_file(&dots_path);
            }
            return Err(e);
        }
        Ok(json!({
            "out": out,
            "dots_out": dots_path.display().to_string(),
            "sequence": index,
            "t": t,
            "dots": dots.len(),
            "width": image.width(),
            "height": image.height(),
        }))
    }

    /// Render a frame and return it as a base64 PNG no larger than `max_size`, for a viewer
    /// (an agent, a script) that cannot open a file.
    fn preview(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let image = match params.integer("movie_frame") {
            Some(n) => {
                if params.has("t") || params.has("frame") || params.has("sequence") {
                    return Err(CommandError::BadParams(
                        "`movie_frame` replaces `t`, `frame` and `sequence`; give only one way to pick the frame"
                            .into(),
                    ));
                }
                session.render_movie_frame(u64::try_from(n).unwrap_or(0))?
            }
            None => render_one(session, params)?.1,
        };
        let max_size = params.integer("max_size").unwrap_or(512).clamp(16, 4096) as u32;
        let small = image.downscale_to_fit(max_size)?;
        let png = inbetween_codecs::encode_png(small.width(), small.height(), &small.to_rgba8())?;
        use base64::Engine as _;
        Ok(json!({
            "width": small.width(),
            "height": small.height(),
            "png_base64": base64::engine::general_purpose::STANDARD.encode(png),
        }))
    }

    fn sequence(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let index = session.sequence_index(params)?;
        let prefix = params.string("prefix").unwrap_or("frame");
        if prefix.contains(['/', '\\', ':']) {
            return Err(CommandError::BadParams(
                "`prefix` is a file name prefix and must not contain / \\ or :".into(),
            ));
        }
        let prepared = session.prepare(index)?;
        let total = prepared.sequence.total_frames();
        let first = params.integer("first").map_or(1, |n| n as u32);
        let last = params
            .integer("last")
            .map_or(total, |n| n as u32)
            .min(total);
        if first > last {
            return Err(CommandError::BadParams(format!(
                "`first` ({first}) is after the last frame to write ({last}); the sequence has {total} frames"
            )));
        }
        let dir = Path::new(params.string("out_dir").unwrap_or_default());
        std::fs::create_dir_all(dir)
            .map_err(|e| CommandError::Io(format!("{}: {e}", dir.display())))?;
        let renderer = prepared.renderer()?;
        session.begin_cancellable();
        for n in first..=last {
            session.check_cancel()?;
            let image = renderer.render_frame(n - 1)?;
            write_image_with(
                &dir.join(format!("{prefix}_{n:04}.png")),
                &image,
                write_options(params),
            )?;
        }
        Ok(json!({
            "dir": dir.display().to_string(),
            "frames": last - first + 1,
            "first": first,
            "last": last,
            "width": prepared.output.0,
            "height": prepared.output.1,
        }))
    }

    const MOVIE_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "out_dir",
            ParamKind::Path,
            true,
            "Folder for the PNG frames (created if missing)",
        )
        .folder(),
        ParamSpec::new(
            "prefix",
            ParamKind::String,
            false,
            "File name prefix: PREFIX_0001.png, ...",
        )
        .default_is("frame"),
        ParamSpec::new(
            "first",
            ParamKind::Integer,
            false,
            "First movie frame to write, counting from 1",
        )
        .range(1.0, MAX_MOVIE_FRAMES as f64)
        .default_is("1"),
        ParamSpec::new(
            "last",
            ParamKind::Integer,
            false,
            "Last movie frame to write, counting from 1",
        )
        .range(1.0, MAX_MOVIE_FRAMES as f64)
        .default_is("the last frame"),
        ParamSpec::new(
            "reverse",
            ParamKind::Bool,
            false,
            "Write the frames in reverse order (numbering follows the movie, not the order written)",
        ),
        ParamSpec::new(
            "drop_duplicates",
            ParamKind::Bool,
            false,
            "Skip a sequence's first or last frame when it repeats the frame before it (the join of continued sequences)",
        ),
        ParamSpec::new(
            "sequence",
            ParamKind::Integer,
            false,
            "Export only this sequence (numbered from 0); `first` and `last` then count inside it",
        )
        .range(0.0, 1_000_000.0)
        .default_is("the whole movie"),
        DPI,
    ];

    /// Every visible sequence, one after another, as one continuously numbered frame series.
    fn movie(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let prefix = params.string("prefix").unwrap_or("frame");
        if prefix.contains(['/', '\\', ':']) {
            return Err(CommandError::BadParams(
                "`prefix` is a file name prefix and must not contain / \\ or :".into(),
            ));
        }
        let range = movie::range_from(session, params)?;
        let total = session.project().total_frames();
        let dir = Path::new(params.string("out_dir").unwrap_or_default());
        std::fs::create_dir_all(dir)
            .map_err(|e| CommandError::Io(format!("{}: {e}", dir.display())))?;
        let written = movie::render_movie(session, range, &mut |n, image| {
            write_image_with(
                &dir.join(format!("{prefix}_{n:04}.png")),
                image,
                write_options(params),
            )
        })?;
        let (w, h) = (
            session.project().output.width,
            session.project().output.height,
        );
        Ok(json!({
            "dir": dir.display().to_string(),
            "frames": written,
            "first": range.first,
            "last": range.last,
            "total_frames": total,
            "width": w,
            "height": h,
        }))
    }

    pub(super) fn specs() -> Vec<CommandSpec> {
        vec![
            CommandSpec {
                id: "export.movie",
                label: "Export Whole Movie",
                menu: "File/Export/Whole Movie...",
                shortcut: None,
                description: "Render every visible sequence, one after another, to continuously numbered PNG files.",
                params: MOVIE_PARAMS,
                enabled: has_sequence,
                disabled_reason: "the project has no sequences to render",
                edits: false,
                run: movie,
            },
            CommandSpec {
                id: "export.frame",
                label: "Export Frame",
                menu: "File/Export/Frame...",
                shortcut: None,
                description: "Render one frame of a sequence to an image file.",
                params: FRAME_PARAMS,
                enabled: has_sequence,
                disabled_reason: "the project has no sequences to render",
                edits: false,
                run: frame,
            },
            CommandSpec {
                id: "frame.preview",
                label: "Preview Frame",
                menu: "",
                shortcut: None,
                description: "Render one frame, scaled down to fit `max_size`, and return it as a base64 PNG (`png_base64`) with its size. Nothing is written to disk.",
                params: PREVIEW_PARAMS,
                enabled: has_sequence,
                disabled_reason: "the project has no sequences to render",
                edits: false,
                run: preview,
            },
            CommandSpec {
                id: "frame.exportEdit",
                label: "Export Frame For Editing",
                menu: "File/Export/Frame For Editing...",
                shortcut: None,
                description: "Render one frame to an image file and write the dots as they sit in that frame to a .inbetweendots file beside it, so the frame can start a new sequence.",
                params: EDIT_PARAMS,
                enabled: has_sequence,
                disabled_reason: "the project has no sequences to render",
                edits: false,
                run: export_edit,
            },
            CommandSpec {
                id: "export.sequence",
                label: "Export Image Sequence",
                menu: "File/Export/Image Sequence...",
                shortcut: None,
                description: "Render every frame of a sequence to numbered PNG files.",
                params: SEQUENCE_PARAMS,
                enabled: has_sequence,
                disabled_reason: "the project has no sequences to render",
                edits: false,
                run: sequence,
            },
        ]
    }
}
