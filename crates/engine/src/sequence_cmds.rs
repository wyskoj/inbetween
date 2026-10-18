//! Sequence commands: `sequence.length`, `sequence.image`.

use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use serde_json::{json, Value};

const SEQUENCE: ParamSpec = ParamSpec::new(
    "sequence",
    ParamKind::Integer,
    false,
    "Sequence number, starting at 0",
)
.range(0.0, 1_000_000.0)
.default_is("0");

const LENGTH_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new(
        "frames",
        ParamKind::Integer,
        false,
        "New number of frames, including the first and last; omit to just report the current length",
    )
    .range(2.0, 999.0),
];

fn length(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    if let Some(frames) = params.integer("frames") {
        let seq = session.sequence_mut(index)?;
        let before = seq.frames;
        seq.frames = frames as u32;
        if let Err(e) = seq.validate() {
            seq.frames = before;
            return Err(e.into());
        }
    }
    Ok(json!({ "sequence": index, "frames": session.sequence(index)?.frames }))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    #[allow(unused_mut)]
    let mut specs = vec![CommandSpec {
        id: "sequence.length",
        label: "Sequence Length",
        menu: "Sequence/Length...",
        shortcut: None,
        description: "Set (or report) how many frames a sequence renders, from 2 to 999.",
        params: LENGTH_PARAMS,
        enabled: Session::has_sequences,
        disabled_reason: "the project has no sequences",
        edits: true,
        run: length,
    }];
    #[cfg(not(target_arch = "wasm32"))]
    specs.extend(files::specs());
    specs
}

#[cfg(not(target_arch = "wasm32"))]
mod files {
    use super::*;

    const IMAGE_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "image",
            ParamKind::Integer,
            true,
            "Which image to replace: 1 or 2",
        )
        .range(1.0, 2.0),
        ParamSpec::new("path", ParamKind::Path, true, "The new image (PNG or JPEG)")
            .open_file(&["png", "jpg", "jpeg"]),
        SEQUENCE,
    ];

    fn image(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let index = session.sequence_index(params)?;
        let which = params.integer("image").unwrap_or(1);
        let path = std::path::absolute(params.string("path").unwrap_or_default())
            .map_err(|e| CommandError::Io(e.to_string()))?
            .display()
            .to_string();
        // Load it now so a bad file is refused here, not at the next render.
        let loaded = session.load_image(&path, which as u8, index)?;
        let seq = session.sequence_mut(index)?;
        if which == 1 {
            seq.image1 = inbetween_doc::ImageRef(path.clone());
        } else {
            seq.image2 = inbetween_doc::ImageRef(path.clone());
        }
        Ok(json!({
            "sequence": index,
            "image": which,
            "path": path,
            "width": loaded.width(),
            "height": loaded.height(),
        }))
    }

    pub(super) fn specs() -> Vec<CommandSpec> {
        vec![CommandSpec {
            id: "sequence.image",
            label: "Set Image",
            menu: "Sequence/Set Image...",
            shortcut: None,
            description: "Replace Image 1 or Image 2 of a sequence with another PNG or JPEG file.",
            params: IMAGE_PARAMS,
            enabled: Session::has_sequences,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: image,
        }]
    }
}
