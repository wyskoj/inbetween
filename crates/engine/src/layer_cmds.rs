//! `project.type`: switch between sequence and layer projects.
//!
//! In a sequence project the visible sequences play one after another. In a layer project they
//! all play at once, stacked with the first in the list on top; the movie is as long as the
//! longest visible sequence and a shorter one holds its last frame. Switching changes no
//! sequence, so it can be undone and switched back at any time.

use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::ProjectType;
use serde_json::{json, Value};

const KINDS: &[&str] = &["sequence", "layer"];

const PARAMS: &[ParamSpec] = &[ParamSpec::new(
    "kind",
    ParamKind::String,
    false,
    "sequence (play one after another) or layer (play at once, first on top); omit to just report",
)
.one_of(KINDS)];

fn run(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    if let Some(kind) = params.string("kind").and_then(ProjectType::from_id) {
        session.project_mut().kind = kind;
    }
    let p = session.project();
    Ok(json!({
        "kind": p.kind.id(),
        "total_frames": p.total_frames(),
        "visible_sequences": p.sequences.iter().filter(|s| s.visible).count(),
    }))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "project.type",
        label: "Project Type",
        menu: "Project/Type...",
        shortcut: None,
        description: "Choose how sequences combine: one after another (sequence project) or all at once as stacked layers, the first in the list on top (layer project).",
        params: PARAMS,
        enabled: |_| true,
        disabled_reason: "",
        edits: true,
        run,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Registry;
    use inbetween_doc::{Project, Sequence};

    fn go(s: &mut Session, p: Value) -> Result<Value, CommandError> {
        Registry::standard().run(s, "project.type", &p)
    }

    #[test]
    fn switching_changes_the_movie_length_and_undoes() {
        let mut s = Session::new();
        let seq = |frames| Sequence {
            frames,
            ..Sequence::default()
        };
        s.set_project(
            Project {
                sequences: vec![seq(10), seq(4)],
                ..Project::default()
            },
            None,
        );
        let v = go(&mut s, json!({})).unwrap();
        assert_eq!(
            (v["kind"].as_str(), v["total_frames"].as_u64()),
            (Some("sequence"), Some(14))
        );
        let v = go(&mut s, json!({"kind": "layer"})).unwrap();
        assert_eq!(
            (v["kind"].as_str(), v["total_frames"].as_u64()),
            (Some("layer"), Some(10))
        );
        assert!(go(&mut s, json!({"kind": "stack"})).is_err());
        Registry::standard()
            .run(&mut s, "edit.undo", &json!({}))
            .unwrap();
        assert_eq!(go(&mut s, json!({})).unwrap()["kind"], "sequence");
    }

    #[test]
    fn works_on_an_empty_project() {
        let mut s = Session::new();
        let v = go(&mut s, json!({"kind": "layer"})).unwrap();
        assert_eq!(v["total_frames"], 0);
    }
}
