//! Options every export command shares: save the project first, open the result afterwards.
//!
//! Saving writes the project to its own file when it has one and has unsaved changes. Opening
//! hands the result (a file, or the folder of a frame series) to the system's default program
//! through the session's opener, which a shell or a test can replace.

use crate::{CommandError, ParamKind, ParamSpec, Params, Session};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

pub(crate) const SAVE_FIRST: ParamSpec = ParamSpec::new(
    "save_first",
    ParamKind::Bool,
    false,
    "Save the project to its file before exporting, when it has one and has unsaved changes",
)
.default_is("false");

pub(crate) const OPEN_AFTER: ParamSpec = ParamSpec::new(
    "open_after",
    ParamKind::Bool,
    false,
    "Open the result with the system's default program when the export is done",
)
.default_is("false");

/// What opens a path: replaceable so tests and shells can decide for themselves.
pub type Opener = Arc<dyn Fn(&Path) -> Result<(), String> + Send + Sync>;

/// The system's default way to open a file or folder. Starts the program and does not wait.
pub fn system_opener(path: &Path) -> Result<(), String> {
    let mut cmd;
    if cfg!(target_os = "windows") {
        cmd = std::process::Command::new("explorer");
    } else if cfg!(target_os = "macos") {
        cmd = std::process::Command::new("open");
    } else {
        cmd = std::process::Command::new("xdg-open");
    }
    cmd.arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open {}: {e}", path.display()))
}

/// Run before rendering: save the project if `save_first` asks and there is something to save.
/// Returns the path written, if any.
pub(crate) fn before(
    session: &mut Session,
    params: &Params,
) -> Result<Option<String>, CommandError> {
    if params.boolean("save_first") != Some(true) {
        return Ok(None);
    }
    crate::project_cmds::files::save_if_dirty(session)
}

/// Run after the export is written: add what was saved and opened to the report.
pub(crate) fn after(
    session: &Session,
    params: &Params,
    saved: Option<String>,
    result: &Path,
    mut report: Value,
) -> Value {
    let opened = if params.boolean("open_after") == Some(true) {
        let open = session.opener();
        match open(result) {
            Ok(()) => json!(true),
            // The export itself worked; a missing viewer is a note, not a failure.
            Err(e) => json!({ "error": e }),
        }
    } else {
        json!(false)
    };
    if let Some(o) = report.as_object_mut() {
        if params.boolean("save_first") == Some(true) {
            o.insert("saved_project".into(), json!(saved));
        }
        if params.boolean("open_after") == Some(true) {
            o.insert("opened".into(), opened);
        }
    }
    report
}
