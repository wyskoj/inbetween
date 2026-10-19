//! Export commands: `export.frame`, `export.sequence`.

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
    use crate::{CommandError, ParamKind, ParamSpec, Params, Session};
    use inbetween_codecs::Rgba8Image;
    use inbetween_doc::MAX_FRAMES;
    use inbetween_raster::Image;
    use inbetween_render_cpu::Renderer;
    use serde_json::{json, Value};
    use std::path::Path;

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
    ];

    fn has_sequence(session: &Session) -> bool {
        session.has_sequences()
    }

    fn write_image(path: &Path, image: &Image) -> Result<(), CommandError> {
        let rgba = Rgba8Image::new(image.width(), image.height(), image.to_rgba8())?;
        inbetween_codecs::write_file(path, &rgba)?;
        Ok(())
    }

    fn frame(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        if params.has("t") && params.has("frame") {
            return Err(CommandError::BadParams(
                "give either `t` or `frame`, not both".into(),
            ));
        }
        let index = session.sequence_index(params)?;
        let prepared = session.prepare(index)?;
        let renderer = Renderer::new(
            &prepared.sequence,
            prepared.output,
            &prepared.image1,
            prepared.image2.as_deref(),
        )?;
        let image = match (params.number("t"), params.integer("frame")) {
            (Some(t), _) => renderer.render_t(t)?,
            (None, Some(n)) => renderer.render_frame(n as u32)?,
            (None, None) => renderer.render_frame(0)?,
        };
        let out = params.string("out").unwrap_or_default();
        write_image(Path::new(out), &image)?;
        Ok(json!({
            "out": out,
            "width": image.width(),
            "height": image.height(),
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
        let renderer = Renderer::new(
            &prepared.sequence,
            prepared.output,
            &prepared.image1,
            prepared.image2.as_deref(),
        )?;
        for n in first..=last {
            let image = renderer.render_frame(n - 1)?;
            write_image(&dir.join(format!("{prefix}_{n:04}.png")), &image)?;
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

    pub(super) fn specs() -> Vec<CommandSpec> {
        vec![
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
