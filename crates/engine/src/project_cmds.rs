//! Project-level commands: `project.info`, `project.open`, `project.save`.

use crate::{CommandError, CommandSpec, Params, Session};
use serde_json::{json, Value};

fn always(_: &Session) -> bool {
    true
}

fn info(session: &mut Session, _: &Params) -> Result<Value, CommandError> {
    let p = session.project();
    let sequences: Vec<Value> = p
        .sequences
        .iter()
        .enumerate()
        .map(|(i, s)| {
            json!({
                "index": i,
                "name": s.name,
                "frames": s.frames,
                "morph": s.morph,
                "fit": s.fit,
                "mesh": s.mesh,
                "dots": s.dots.len(),
                "image1": s.image1.0,
                "image2": s.image2.0,
            })
        })
        .collect();
    Ok(json!({
        "path": session.path().map(|p| p.display().to_string()),
        "output": { "width": p.output.width, "height": p.output.height },
        "sequences": sequences,
    }))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    #[allow(unused_mut)]
    let mut specs = vec![CommandSpec {
        id: "project.info",
        label: "Project Info",
        menu: "File/Project Info",
        shortcut: None,
        description: "Report the output size and every sequence's frames, dots and images.",
        params: &[],
        enabled: always,
        disabled_reason: "",
        run: info,
    }];
    #[cfg(not(target_arch = "wasm32"))]
    specs.extend(files::specs());
    specs
}

#[cfg(not(target_arch = "wasm32"))]
mod files {
    use super::*;
    use crate::{ParamKind, ParamSpec};
    use std::path::PathBuf;

    const OPEN_PARAMS: &[ParamSpec] = &[ParamSpec::new(
        "path",
        ParamKind::Path,
        true,
        "The .inbetween project file to open",
    )];

    const SAVE_PARAMS: &[ParamSpec] = &[ParamSpec::new(
        "path",
        ParamKind::Path,
        false,
        "Where to save; omit to save over the file the project was opened from",
    )
    .default_is("the current project file")];

    fn open(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let path = PathBuf::from(params.string("path").unwrap_or_default());
        let project = inbetween_format::read_project(&path)?;
        let count = project.sequences.len();
        session.set_project(project, Some(path.clone()));
        Ok(json!({ "path": path.display().to_string(), "sequences": count }))
    }

    fn save(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let path = match params.string("path") {
            Some(p) => PathBuf::from(p),
            None => session.path().map(PathBuf::from).ok_or_else(|| {
                CommandError::BadParams(
                    "this project has no file yet; give a `path` to save to".to_string(),
                )
            })?,
        };
        inbetween_format::write_project(&path, session.project())?;
        session.set_path(path.clone());
        Ok(json!({ "path": path.display().to_string() }))
    }

    pub(super) fn specs() -> Vec<CommandSpec> {
        vec![
            CommandSpec {
                id: "project.open",
                label: "Open Project",
                menu: "File/Open...",
                shortcut: Some("Ctrl+O"),
                description: "Load a .inbetween project file, replacing the current project.",
                params: OPEN_PARAMS,
                enabled: always,
                disabled_reason: "",
                run: open,
            },
            CommandSpec {
                id: "project.save",
                label: "Save Project",
                menu: "File/Save",
                shortcut: Some("Ctrl+S"),
                description: "Write the project to a .inbetween file. Nothing is written if the project is invalid.",
                params: SAVE_PARAMS,
                enabled: always,
                disabled_reason: "",
                run: save,
            },
        ]
    }
}
