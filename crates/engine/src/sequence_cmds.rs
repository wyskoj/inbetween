//! Sequence commands: `sequence.length`, `sequence.morphType`, `sequence.fit`,
//! `morph.option.retriangulate`, `morph.option.dropCornerTris`, `sequence.image`.

use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{FitMode, MeshPolicy, MorphType};
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
        "New number of morph frames, including the first and last; omit to leave it as it is",
    )
    .range(2.0, 999.0),
    ParamSpec::new(
        "hold_start",
        ParamKind::Integer,
        false,
        "Extra frames before the morph that repeat Image 1; omit to leave it as it is",
    )
    .range(0.0, 999.0),
    ParamSpec::new(
        "hold_end",
        ParamKind::Integer,
        false,
        "Extra frames after the morph that repeat Image 2; omit to leave it as it is",
    )
    .range(0.0, 999.0),
];

fn length(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let seq = session.sequence_mut(index)?;
    let before = (seq.frames, seq.hold_start, seq.hold_end);
    if let Some(n) = params.integer("frames") {
        seq.frames = n as u32;
    }
    if let Some(n) = params.integer("hold_start") {
        seq.hold_start = n as u32;
    }
    if let Some(n) = params.integer("hold_end") {
        seq.hold_end = n as u32;
    }
    if let Err(e) = seq.validate() {
        (seq.frames, seq.hold_start, seq.hold_end) = before;
        return Err(e.into());
    }
    Ok(json!({
        "sequence": index,
        "frames": seq.frames,
        "hold_start": seq.hold_start,
        "hold_end": seq.hold_end,
        "total_frames": seq.total_frames(),
    }))
}

const MORPH_IDS: &[&str] = &[
    "morph",
    "morph_hold",
    "warp1",
    "warp2",
    "fade_out1",
    "fade_in2",
    "direct",
];
const FIT_IDS: &[&str] = &["stretch", "contain", "cover"];

const MORPH_TYPE_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new(
        "type",
        ParamKind::String,
        false,
        "morph, morph_hold, warp1, warp2, fade_out1, fade_in2 or direct; omit to just report it",
    )
    .one_of(MORPH_IDS),
];

const FIT_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new(
        "fit",
        ParamKind::String,
        false,
        "How a source of another size fits the frame: stretch, contain or cover; omit to just report it",
    )
    .one_of(FIT_IDS),
];

const RETRIANGULATE_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new(
        "on",
        ParamKind::Bool,
        false,
        "true re-triangulates the mesh on every frame, false only once from Image 1's dots; omit to just report it",
    ),
];

const DROP_CORNER_PARAMS: &[ParamSpec] = &[
    SEQUENCE,
    ParamSpec::new(
        "on",
        ParamKind::Bool,
        false,
        "true discards the triangles that touch the frame corners (the rest of the frame turns transparent); omit to just report it",
    ),
];

fn morph_type(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    if let Some(id) = params.string("type") {
        let t = MorphType::from_id(id)
            .ok_or_else(|| CommandError::BadParams(format!("unknown morph type `{id}`")))?;
        session.sequence_mut(index)?.morph = t;
    }
    let t = session.sequence(index)?.morph;
    Ok(json!({
        "sequence": index,
        "type": t.id(),
        "needs_image2": t.uses_image2(),
        "uses_dots": t.warps(),
    }))
}

fn fit(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    if let Some(id) = params.string("fit") {
        let f = FitMode::from_id(id)
            .ok_or_else(|| CommandError::BadParams(format!("unknown fit mode `{id}`")))?;
        session.sequence_mut(index)?.fit = f;
    }
    Ok(json!({ "sequence": index, "fit": session.sequence(index)?.fit.id() }))
}

fn retriangulate(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    if let Some(on) = params.boolean("on") {
        session.sequence_mut(index)?.mesh = if on {
            MeshPolicy::PerFrame
        } else {
            MeshPolicy::Frame0
        };
    }
    let on = session.sequence(index)?.mesh == MeshPolicy::PerFrame;
    Ok(json!({ "sequence": index, "on": on, "mesh": session.sequence(index)?.mesh.id() }))
}

fn drop_corner_tris(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    if let Some(on) = params.boolean("on") {
        session.sequence_mut(index)?.drop_corner_tris = on;
    }
    Ok(json!({ "sequence": index, "on": session.sequence(index)?.drop_corner_tris }))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    #[allow(unused_mut)]
    let mut specs = vec![
        CommandSpec {
            id: "sequence.length",
            label: "Sequence Length",
            menu: "Sequence/Length...",
            shortcut: None,
            description: "Set (or report) a sequence's morph frames (2 to 999) and its hold frames (0 to 999) at each end.",
            params: LENGTH_PARAMS,
            enabled: Session::has_sequences,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: length,
        },
        CommandSpec {
            id: "sequence.morphType",
            label: "Morph Type",
            menu: "Sequence/Morph Type...",
            shortcut: None,
            description: "Set (or report) how a sequence combines its images: morph, morph_hold, warp1, warp2, fade_out1, fade_in2 or direct.",
            params: MORPH_TYPE_PARAMS,
            enabled: Session::has_sequences,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: morph_type,
        },
        CommandSpec {
            id: "sequence.fit",
            label: "Image Fit",
            menu: "Sequence/Image Fit...",
            shortcut: None,
            description: "Set (or report) how sources of another size fit the output frame: stretch, contain or cover.",
            params: FIT_PARAMS,
            enabled: Session::has_sequences,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: fit,
        },
        CommandSpec {
            id: "morph.option.retriangulate",
            label: "Re-triangulate Every Frame",
            menu: "Sequence/Re-triangulate Every Frame",
            shortcut: None,
            description: "Set (or report) whether the mesh is re-triangulated for every intermediate frame (on) or only once from Image 1's dots (off).",
            params: RETRIANGULATE_PARAMS,
            enabled: Session::has_sequences,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: retriangulate,
        },
        CommandSpec {
            id: "morph.option.dropCornerTris",
            label: "Drop Corner Triangles",
            menu: "Sequence/Drop Corner Triangles",
            shortcut: None,
            description: "Set (or report) whether triangles touching the four frame corners are discarded, leaving the area outside the dots' outline transparent so a backdrop or back colour shows through.",
            params: DROP_CORNER_PARAMS,
            enabled: Session::has_sequences,
            disabled_reason: "the project has no sequences",
            edits: true,
            run: drop_corner_tris,
        },
    ];
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
