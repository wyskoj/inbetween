//! File-level commands that work across projects: `file.appendProject`.

use crate::session::resolve_against;
use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{ImageRef, Project, Sequence};
use inbetween_format::MAX_SEQUENCES;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn always(_: &Session) -> bool {
    true
}

/// The sequences of `other` with every image reference made absolute against the folder
/// of `other_file`, so they keep finding their images inside another project. The next
/// save turns them relative to the new file again.
fn rebased_sequences(other: Project, other_file: &Path) -> Result<Vec<Sequence>, CommandError> {
    let fix = |r: &mut ImageRef| -> Result<(), CommandError> {
        if r.0.is_empty() {
            return Ok(());
        }
        let full = resolve_against(Some(other_file), &r.0);
        let full = std::path::absolute(&full)
            .map_err(|e| CommandError::Io(format!("{}: {e}", full.display())))?;
        r.0 = full.display().to_string();
        Ok(())
    };
    let mut sequences = other.sequences;
    for seq in &mut sequences {
        fix(&mut seq.image1)?;
        fix(&mut seq.image2)?;
        for image in seq.effects.all_images_mut() {
            fix(image)?;
        }
    }
    Ok(sequences)
}

const APPEND_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "path",
        ParamKind::Path,
        true,
        "The .inbetween project whose sequences to add",
    )
    .open_file(&["inbetween"]),
    ParamSpec::new(
        "after",
        ParamKind::Integer,
        false,
        "Insert after this sequence number",
    )
    .range(0.0, 1_000_000.0)
    .default_is("the end of the list"),
];

/// Add the sequences of another project file to this one. The output size, project kind
/// and face model stay this project's.
fn append_project(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let path = PathBuf::from(params.string("path").unwrap_or_default());
    let other = inbetween_format::read_project(&path)?;
    if other.kind != session.project().kind {
        return Err(CommandError::BadParams(format!(
            "cannot append a {} project to a {} project; both must be the same kind",
            other.kind.id(),
            session.project().kind.id()
        )));
    }
    let count = session.project().sequences.len();
    let adding = other.sequences.len();
    if count.saturating_add(adding) > MAX_SEQUENCES {
        return Err(CommandError::BadParams(format!(
            "appending {adding} sequences to {count} would pass the limit of {MAX_SEQUENCES}"
        )));
    }
    let at = match params.integer("after") {
        Some(n) => {
            let n = n.max(0) as usize;
            if n >= count {
                return Err(CommandError::NoSuchSequence { index: n, count });
            }
            n + 1
        }
        None => count,
    };
    let sequences = rebased_sequences(other, &path)?;
    session.project_mut().sequences.splice(at..at, sequences);
    Ok(json!({
        "first": at,
        "added": adding,
        "count": session.project().sequences.len(),
    }))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "file.appendProject",
        label: "Append Project",
        menu: "File/Append Project...",
        shortcut: None,
        description: "Add the sequences of another .inbetween project to this one. Output size, project kind and face model stay this project's; appending a Sequence project to a Layer project (or the reverse) is refused.",
        params: APPEND_PARAMS,
        enabled: always,
        disabled_reason: "",
        edits: true,
        run: append_project,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Registry;
    use inbetween_doc::ProjectType;

    pub(crate) fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("inbetween-fc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub(crate) fn write_png(path: &Path, w: u32, h: u32, grey: u8) {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        let rgba = [grey, grey, grey, 255].repeat((w * h) as usize);
        std::fs::write(path, inbetween_codecs::encode_png(w, h, &rgba).unwrap()).unwrap();
    }

    /// A saved two-image project in `dir/<name>/`, its images in `dir/<name>/imgs/`.
    fn make_project(r: &Registry, dir: &Path, name: &str, grey: u8) -> PathBuf {
        let (a, b) = (
            dir.join(name).join("imgs/a.png"),
            dir.join(name).join("imgs/b.png"),
        );
        write_png(&a, 32, 24, grey);
        write_png(&b, 32, 24, grey.saturating_add(40));
        let mut s = Session::new();
        r.run(
            &mut s,
            "project.new",
            &json!({"image1": a.to_str(), "image2": b.to_str(), "name": name}),
        )
        .unwrap();
        let file = dir.join(name).join("p.inbetween");
        r.run(&mut s, "project.save", &json!({"path": file.to_str()}))
            .unwrap();
        file
    }

    #[test]
    fn appended_images_resolve_from_another_folder() {
        let dir = temp("append");
        let r = Registry::standard();
        let one = make_project(&r, &dir, "one", 10);
        let two = make_project(&r, &dir, "two", 90);

        let mut s = Session::new();
        r.run(&mut s, "project.open", &json!({"path": one.to_str()}))
            .unwrap();
        let out = r
            .run(&mut s, "file.appendProject", &json!({"path": two.to_str()}))
            .unwrap();
        assert_eq!(
            (out["first"].as_u64(), out["added"].as_u64()),
            (Some(1), Some(1))
        );
        assert_eq!(s.project().sequences.len(), 2);
        assert!(s.is_dirty());
        // Both sequences render: the appended one finds its images in the other folder.
        assert!(s.prepare(0).is_ok());
        assert!(s.prepare(1).is_ok());

        // Save next to the first project, reopen, render again.
        let saved = dir.join("one/merged.inbetween");
        r.run(&mut s, "project.save", &json!({"path": saved.to_str()}))
            .unwrap();
        let on_disk = inbetween_format::read_project(&saved).unwrap();
        assert_eq!(on_disk.sequences[0].image1.0, "imgs/a.png");
        assert_eq!(on_disk.sequences[1].image1.0, "../two/imgs/a.png");
        let mut fresh = Session::new();
        r.run(&mut fresh, "project.open", &json!({"path": saved.to_str()}))
            .unwrap();
        assert!(fresh.prepare(1).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_inserts_after_a_chosen_sequence_and_undoes() {
        let dir = temp("append-after");
        let r = Registry::standard();
        let one = make_project(&r, &dir, "one", 10);
        let two = make_project(&r, &dir, "two", 90);
        let mut s = Session::new();
        r.run(&mut s, "project.open", &json!({"path": one.to_str()}))
            .unwrap();
        r.run(&mut s, "sequence.duplicate", &json!({"sequence": 0}))
            .unwrap();
        let before = s.project().clone();
        r.run(
            &mut s,
            "file.appendProject",
            &json!({"path": two.to_str(), "after": 0}),
        )
        .unwrap();
        assert_eq!(s.project().sequences.len(), 3);
        assert_eq!(s.project().sequences[1].name, "two");
        assert_eq!(s.project().sequences[2], before.sequences[1]);
        r.run(&mut s, "edit.undo", &json!({})).unwrap();
        assert_eq!(s.project(), &before);
        // After a missing sequence is refused.
        assert!(r
            .run(
                &mut s,
                "file.appendProject",
                &json!({"path": two.to_str(), "after": 7})
            )
            .is_err());
        assert_eq!(s.project(), &before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_current_project_keeps_its_output_and_face_model() {
        let dir = temp("append-keep");
        let r = Registry::standard();
        let two = make_project(&r, &dir, "two", 90);
        let mut s = Session::new();
        r.run(&mut s, "file.new", &json!({"width": 640, "height": 360}))
            .unwrap();
        r.run(&mut s, "file.appendProject", &json!({"path": two.to_str()}))
            .unwrap();
        assert_eq!(
            (s.project().output.width, s.project().output.height),
            (640, 360)
        );
        assert_eq!(s.project().sequences.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mismatched_kind_is_refused_and_changes_nothing() {
        let dir = temp("append-kind");
        let r = Registry::standard();
        let two = make_project(&r, &dir, "two", 90);
        let mut s = Session::new();
        r.run(&mut s, "file.new", &json!({"kind": "layer"}))
            .unwrap();
        let before = s.project().clone();
        let e = r
            .run(&mut s, "file.appendProject", &json!({"path": two.to_str()}))
            .unwrap_err();
        let text = e.to_string();
        assert!(
            text.contains("sequence") && text.contains("layer"),
            "{text}"
        );
        assert_eq!(s.project(), &before);
        assert_eq!(s.project().kind, ProjectType::Layer);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_files_and_the_sequence_cap_are_refused() {
        let dir = temp("append-bad");
        let r = Registry::standard();
        let mut s = Session::new();
        let junk = dir.join("junk.inbetween");
        std::fs::write(&junk, b"not a project").unwrap();
        for p in [junk, dir.join("missing.inbetween")] {
            assert!(r
                .run(&mut s, "file.appendProject", &json!({"path": p.to_str()}))
                .is_err());
        }
        // A project that fills the cap cannot take one more.
        let big = Project {
            sequences: vec![Sequence::default(); MAX_SEQUENCES],
            ..Project::default()
        };
        let file = dir.join("big.inbetween");
        inbetween_format::write_project(&file, &big).unwrap();
        r.run(&mut s, "file.new", &json!({})).unwrap();
        let before = s.project().clone();
        let e = r
            .run(
                &mut s,
                "file.appendProject",
                &json!({"path": file.to_str()}),
            )
            .unwrap_err();
        assert!(e.to_string().contains("limit"), "{e}");
        assert_eq!(s.project(), &before);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
