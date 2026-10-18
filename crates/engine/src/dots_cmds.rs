//! Dot-set commands: `dots.import`, `dots.export`.

use crate::{CommandSpec, Session};

#[allow(dead_code)]
fn has_sequence(session: &Session) -> bool {
    session.has_sequences()
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    #[cfg(not(target_arch = "wasm32"))]
    return files::specs();
    #[cfg(target_arch = "wasm32")]
    return Vec::new();
}

#[cfg(not(target_arch = "wasm32"))]
mod files {
    use super::*;
    use crate::{CommandError, ParamKind, ParamSpec, Params};
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

    const IMPORT_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "path",
            ParamKind::Path,
            true,
            "The .inbetweendots file to read",
        ),
        SEQUENCE,
    ];
    const EXPORT_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "path",
            ParamKind::Path,
            true,
            "The .inbetweendots file to write",
        ),
        SEQUENCE,
    ];

    fn import(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let index = session.sequence_index(params)?;
        let dots =
            inbetween_format::read_dots(Path::new(params.string("path").unwrap_or_default()))?;
        let count = dots.len();
        session.sequence_mut(index)?.dots = dots;
        Ok(json!({ "sequence": index, "dots": count }))
    }

    fn export(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let index = session.sequence_index(params)?;
        let dots = &session.sequence(index)?.dots;
        inbetween_format::write_dots(Path::new(params.string("path").unwrap_or_default()), dots)?;
        Ok(json!({ "sequence": index, "dots": dots.len() }))
    }

    pub(super) fn specs() -> Vec<CommandSpec> {
        vec![
            CommandSpec {
                id: "dots.import",
                label: "Import Dots",
                menu: "Dots/Import...",
                shortcut: None,
                description: "Replace a sequence's dots with those from a .inbetweendots file.",
                params: IMPORT_PARAMS,
                enabled: has_sequence,
                disabled_reason: "the project has no sequence to put dots in",
                run: import,
            },
            CommandSpec {
                id: "dots.export",
                label: "Export Dots",
                menu: "Dots/Export...",
                shortcut: None,
                description: "Write a sequence's dots to a .inbetweendots file.",
                params: EXPORT_PARAMS,
                enabled: has_sequence,
                disabled_reason: "the project has no sequence to export dots from",
                run: export,
            },
        ]
    }
}
