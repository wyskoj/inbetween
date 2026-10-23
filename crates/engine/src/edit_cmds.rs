//! History commands: `edit.undo`, `edit.redo`, `edit.beginGroup`, `edit.endGroup`.

use crate::{CommandError, CommandSpec, Params, Session};
use serde_json::{json, Value};

fn can_undo(session: &Session) -> bool {
    session.can_undo()
}

fn can_redo(session: &Session) -> bool {
    session.can_redo()
}

fn always(_: &Session) -> bool {
    true
}

fn undo(session: &mut Session, _: &Params) -> Result<Value, CommandError> {
    let done = session.undo();
    Ok(json!({ "done": done, "can_undo": session.can_undo(), "can_redo": session.can_redo() }))
}

fn redo(session: &mut Session, _: &Params) -> Result<Value, CommandError> {
    let done = session.redo();
    Ok(json!({ "done": done, "can_undo": session.can_undo(), "can_redo": session.can_redo() }))
}

fn begin_group(session: &mut Session, _: &Params) -> Result<Value, CommandError> {
    session.begin_group();
    Ok(Value::Null)
}

fn end_group(session: &mut Session, _: &Params) -> Result<Value, CommandError> {
    session.end_group();
    Ok(Value::Null)
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "edit.undo",
            label: "Undo",
            menu: "Edit/Undo",
            shortcut: Some("Ctrl+Z"),
            description: "Undo the last edit (or the last group of edits).",
            params: &[],
            enabled: can_undo,
            disabled_reason: "there is nothing to undo",
            edits: false,
            run: undo,
        },
        CommandSpec {
            id: "edit.redo",
            label: "Redo",
            menu: "Edit/Redo",
            shortcut: Some("Ctrl+Y"),
            description: "Redo the edit that was just undone.",
            params: &[],
            enabled: can_redo,
            disabled_reason: "there is nothing to redo",
            edits: false,
            run: redo,
        },
        CommandSpec {
            id: "edit.beginGroup",
            label: "Begin Edit Group",
            menu: "",
            shortcut: None,
            description: "Start merging the following edits into a single undo step, such as every move of a drag. Pair with edit.endGroup.",
            params: &[],
            enabled: always,
            disabled_reason: "",
            edits: false,
            run: begin_group,
        },
        CommandSpec {
            id: "edit.endGroup",
            label: "End Edit Group",
            menu: "",
            shortcut: None,
            description: "Stop merging edits into one undo step.",
            params: &[],
            enabled: always,
            disabled_reason: "",
            edits: false,
            run: end_group,
        },
    ]
}
