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

/// What the system's file manager should be asked to show for `path`: Windows selects the file
/// with `explorer /select,`, macOS reveals it with `open -R`, and elsewhere (or for a folder)
/// the folder is opened.
pub fn reveal_command(path: &Path) -> (&'static str, Vec<std::ffi::OsString>) {
    let file = path.is_file();
    if cfg!(target_os = "windows") && file {
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path.as_os_str());
        ("explorer", vec![arg])
    } else if cfg!(target_os = "macos") && file {
        ("open", vec!["-R".into(), path.as_os_str().to_owned()])
    } else {
        let target = if file {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let program = if cfg!(target_os = "windows") {
            "explorer"
        } else if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        (program, vec![target.as_os_str().to_owned()])
    }
}

/// Show `path` in the system's file manager, selecting it when it is a file.
pub fn reveal(path: &Path) -> Result<(), String> {
    let (program, args) = reveal_command(path);
    std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not show {}: {e}", path.display()))
}

/// Run before rendering: save the project if `save_first` asks and there is something to save.
/// Returns the path written, if any.
pub(crate) fn before(
    session: &mut Session,
    params: &Params,
) -> Result<Option<String>, CommandError> {
    // Every export starts here: a stop requested for an earlier one does not carry over.
    session.begin_cancellable();
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

#[cfg(test)]
mod reveal_tests {
    use super::*;

    #[test]
    fn a_file_is_selected_where_the_system_can_and_a_folder_is_opened() {
        let dir = std::env::temp_dir().join(format!("inbetween-reveal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.png");
        std::fs::write(&file, b"x").unwrap();
        let (_, args) = reveal_command(&file);
        let first = args[0].to_string_lossy().into_owned();
        if cfg!(target_os = "windows") {
            assert!(first.starts_with("/select,") && first.ends_with("a.png"));
        } else if cfg!(target_os = "macos") {
            assert_eq!(first, "-R");
        } else {
            assert_eq!(args[0], dir.as_os_str());
        }
        let (_, args) = reveal_command(&dir);
        assert_eq!(args, vec![dir.as_os_str().to_owned()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
