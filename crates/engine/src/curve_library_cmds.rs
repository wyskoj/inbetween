//! `curve.library`: a folder of named curves. List, save a sequence's curve under a name, load
//! one back into a curve, delete one. Files are `.inbetweencurve`.

use crate::CommandSpec;

pub(crate) fn specs() -> Vec<CommandSpec> {
    #[cfg(not(target_arch = "wasm32"))]
    return files::specs();
    #[cfg(target_arch = "wasm32")]
    return Vec::new();
}

#[cfg(not(target_arch = "wasm32"))]
mod files {
    use super::*;
    use crate::curve_cmds::{curve_mut, Which};
    use crate::{CommandError, ParamKind, ParamSpec, Params, Session};
    use serde_json::{json, Value};
    use std::path::{Path, PathBuf};

    const OPS: &[&str] = &["list", "save", "load", "delete"];
    const CURVES: &[&str] = &["feature", "shape", "master", "camera"];
    const EXTENSION: &str = "inbetweencurve";
    /// Most library entries listed.
    const MAX_ENTRIES: usize = 10_000;
    const MAX_NAME: usize = 64;

    const PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "op",
            ParamKind::String,
            true,
            "list the curves in the library, save the sequence's curve as `name`, load `name` into the curve, or delete `name`",
        )
        .one_of(OPS),
        ParamSpec::new("dir", ParamKind::Path, true, "The library folder (created by `save`)")
            .folder(),
        ParamSpec::new(
            "name",
            ParamKind::String,
            false,
            "Entry name: letters, digits, spaces, - _ and . (not leading), at most 64 characters",
        ),
        ParamSpec::new(
            "curve",
            ParamKind::String,
            false,
            "Which curve of the sequence: feature, shape, master or camera",
        )
        .one_of(CURVES)
        .default_is("feature"),
        ParamSpec::new(
            "sequence",
            ParamKind::Integer,
            false,
            "Sequence number, starting at 0",
        )
        .range(0.0, 1_000_000.0)
        .default_is("0"),
    ];

    fn which(name: &str) -> Which {
        match name {
            "shape" => Which::Shape,
            "master" => Which::Master,
            "camera" => Which::Easing,
            _ => Which::Feature,
        }
    }

    /// A safe file stem for an entry name.
    fn stem(name: &str) -> Result<&str, CommandError> {
        let ok = !name.is_empty()
            && name.chars().count() <= MAX_NAME
            && !name.starts_with('.')
            && !name.ends_with([' ', '.'])
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'));
        if ok {
            Ok(name)
        } else {
            Err(CommandError::BadParams(format!(
                "`name` must be 1 to {MAX_NAME} letters, digits, spaces, - _ or . and must not start with a dot or end with a space or dot, not `{name}`"
            )))
        }
    }

    fn entry_path(dir: &Path, name: &str) -> Result<PathBuf, CommandError> {
        Ok(dir.join(format!("{}.{EXTENSION}", stem(name)?)))
    }

    fn need_name<'a>(params: &'a Params, op: &str) -> Result<&'a str, CommandError> {
        params
            .string("name")
            .ok_or_else(|| CommandError::BadParams(format!("`{op}` needs the `name` parameter")))
    }

    fn list(dir: &Path) -> Result<Vec<String>, CommandError> {
        let read = match std::fs::read_dir(dir) {
            Ok(r) => r,
            // A library that was never saved to is an empty one.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(CommandError::Io(format!("{}: {e}", dir.display()))),
        };
        let mut names: Vec<String> = read
            .filter_map(Result::ok)
            .filter_map(|e| {
                let path = e.path();
                let ext = path.extension()?.to_str()?;
                if !ext.eq_ignore_ascii_case(EXTENSION) {
                    return None;
                }
                Some(path.file_stem()?.to_str()?.to_string())
            })
            .take(MAX_ENTRIES)
            .collect();
        names.sort();
        Ok(names)
    }

    fn run(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let op = params.string("op").unwrap_or_default();
        let dir = PathBuf::from(params.string("dir").unwrap_or_default());
        match op {
            "list" => Ok(json!({ "dir": dir.display().to_string(), "curves": list(&dir)? })),
            "save" => {
                let name = need_name(params, op)?;
                let path = entry_path(&dir, name)?;
                let index = session.sequence_index(params)?;
                let target = which(params.string("curve").unwrap_or("feature"));
                let curve = curve_mut(session.sequence_mut(index)?, target).clone();
                std::fs::create_dir_all(&dir)
                    .map_err(|e| CommandError::Io(format!("{}: {e}", dir.display())))?;
                inbetween_format::write_curve(&path, &curve)?;
                Ok(json!({ "saved": name, "path": path.display().to_string() }))
            }
            "load" => {
                let name = need_name(params, op)?;
                let path = entry_path(&dir, name)?;
                let index = session.sequence_index(params)?;
                let target = which(params.string("curve").unwrap_or("feature"));
                let curve = inbetween_format::read_curve(&path)?;
                // A feature, shape or camera curve is pinned at both ends; a master curve is not.
                if target == Which::Master {
                    curve.validate_free()?;
                } else {
                    curve.validate()?;
                }
                *curve_mut(session.sequence_mut(index)?, target) = curve.clone();
                Ok(json!({
                    "loaded": name,
                    "sequence": index,
                    "points": curve.points.iter().map(|p| json!([p.x, p.y])).collect::<Vec<_>>(),
                    "linear": curve.linear,
                }))
            }
            "delete" => {
                let name = need_name(params, op)?;
                let path = entry_path(&dir, name)?;
                std::fs::remove_file(&path)
                    .map_err(|e| CommandError::Io(format!("{}: {e}", path.display())))?;
                Ok(json!({ "deleted": name }))
            }
            other => Err(CommandError::BadParams(format!("unknown op `{other}`"))),
        }
    }

    pub(super) fn specs() -> Vec<CommandSpec> {
        vec![CommandSpec {
            id: "curve.library",
            label: "Curve Library",
            menu: "Sequence/Curves/Curve Library...",
            shortcut: None,
            description: "A folder of named curves: list, save a sequence's feature, shape, master or camera curve under a name, load one back, or delete one.",
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
            Registry::standard().run(s, "curve.library", &p)
        }

        fn session() -> Session {
            let mut s = Session::new();
            s.set_project(
                Project {
                    sequences: vec![Sequence::default(), Sequence::default()],
                    ..Project::default()
                },
                None,
            );
            s
        }

        fn dir(tag: &str) -> String {
            let d = std::env::temp_dir()
                .join(format!("inbetween-curvelib-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            d.display().to_string()
        }

        fn edit(s: &mut Session, id: &str, p: Value) {
            Registry::standard().run(s, id, &p).unwrap();
        }

        #[test]
        fn save_list_load_and_delete() {
            let d = dir("a");
            let mut s = session();
            assert_eq!(
                go(&mut s, json!({"op": "list", "dir": d})).unwrap()["curves"],
                json!([])
            );
            edit(
                &mut s,
                "curve.feature",
                json!({"op": "add", "x": 0.5, "y": 0.1}),
            );
            go(
                &mut s,
                json!({"op": "save", "dir": d, "name": "slow start"}),
            )
            .unwrap();
            edit(
                &mut s,
                "curve.master",
                json!({"op": "move", "index": 0, "x": 0.0, "y": 0.3}),
            );
            go(
                &mut s,
                json!({"op": "save", "dir": d, "name": "fade in", "curve": "master"}),
            )
            .unwrap();
            let v = go(&mut s, json!({"op": "list", "dir": d})).unwrap();
            assert_eq!(v["curves"], json!(["fade in", "slow start"]));
            // Load into another sequence's shape curve.
            let v = go(&mut s, json!({"op": "load", "dir": d, "name": "slow start", "curve": "shape", "sequence": 1})).unwrap();
            assert_eq!(v["points"], json!([[0.0, 0.0], [0.5, 0.1], [1.0, 1.0]]));
            assert_eq!(s.project().sequences[1].shape_curve.points.len(), 3);
            // A master curve with a low end cannot become a feature curve.
            assert!(go(&mut s, json!({"op": "load", "dir": d, "name": "fade in"})).is_err());
            go(&mut s, json!({"op": "load", "dir": d, "name": "fade in", "curve": "master", "sequence": 1})).unwrap();
            assert!((s.project().sequences[1].master_curve.eval(0.0) - 0.3).abs() < 1e-9);
            go(
                &mut s,
                json!({"op": "delete", "dir": d, "name": "slow start"}),
            )
            .unwrap();
            assert_eq!(
                go(&mut s, json!({"op": "list", "dir": d})).unwrap()["curves"],
                json!(["fade in"])
            );
            assert!(go(
                &mut s,
                json!({"op": "delete", "dir": d, "name": "slow start"})
            )
            .is_err());
            Registry::standard()
                .run(&mut s, "edit.undo", &json!({}))
                .unwrap();
            let _ = std::fs::remove_dir_all(&d);
        }

        #[test]
        fn bad_names_and_files_are_errors() {
            let d = dir("b");
            let mut s = session();
            for bad in [
                "",
                "..",
                ".hidden",
                "a/b",
                "a\\b",
                "x:y",
                "trailing.",
                "nul\0",
                &"n".repeat(65),
            ] {
                assert!(
                    go(&mut s, json!({"op": "save", "dir": d, "name": bad})).is_err(),
                    "accepted {bad:?}"
                );
            }
            assert!(go(&mut s, json!({"op": "save", "dir": d})).is_err());
            assert!(go(&mut s, json!({"op": "load", "dir": d, "name": "missing"})).is_err());
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(Path::new(&d).join("junk.inbetweencurve"), b"not json").unwrap();
            assert!(go(&mut s, json!({"op": "load", "dir": d, "name": "junk"})).is_err());
            assert!(!Path::new(&d).join("a").exists());
            let _ = std::fs::remove_dir_all(&d);
        }
    }
}
