//! Sequence commands: `sequence.length`.

use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use serde_json::{json, Value};

const PARAMS: &[ParamSpec] = &[
    ParamSpec::new("sequence", ParamKind::Integer, false, "Sequence number, starting at 0")
        .range(0.0, 1_000_000.0)
        .default_is("0"),
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
    vec![CommandSpec {
        id: "sequence.length",
        label: "Sequence Length",
        menu: "Sequence/Length...",
        shortcut: None,
        description: "Set (or report) how many frames a sequence renders, from 2 to 999.",
        params: PARAMS,
        enabled: Session::has_sequences,
        disabled_reason: "the project has no sequences",
        run: length,
    }]
}
