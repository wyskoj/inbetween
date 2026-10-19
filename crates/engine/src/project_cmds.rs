//! Project-level commands: `project.info`, `project.new`, `project.open`, `project.save`.

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
                "hold_start": s.hold_start,
                "hold_end": s.hold_end,
                "total_frames": s.total_frames(),
                "morph": s.morph,
                "fit": s.fit,
                "mesh": s.mesh,
                "visible": s.visible,
                "feature_curve": !s.feature_curve.is_identity(),
                "shape_curve": !s.shape_curve.is_identity(),
                "master_curve": !s.master_curve.is_opaque(),
                "camera": s.camera.is_active(),
                "track": s.track.is_active(),
                "dots": s.dots.len(),
                "image1": s.image1.0,
                "image2": s.image2.0,
            })
        })
        .collect();
    Ok(json!({
        "path": session.path().map(|p| p.display().to_string()),
        "kind": p.kind.id(),
        "output": { "width": p.output.width, "height": p.output.height },
        "total_frames": p.total_frames(),
        "dirty": session.is_dirty(),
        "sequences": sequences,
    }))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    #[allow(unused_mut)]
    let mut specs = vec![CommandSpec {
        id: "project.info",
        label: "Project Info",
        menu: "",
        shortcut: None,
        description: "Report the output size and every sequence's frames, dots and images.",
        params: &[],
        enabled: always,
        disabled_reason: "",
        edits: false,
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
    use inbetween_doc::{ImageRef, OutputSettings, Project, Sequence};
    use std::path::{Path, PathBuf};

    use inbetween_codecs::IMAGE_EXTENSIONS;

    const NEW_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "image1",
            ParamKind::Path,
            true,
            "The first image (PNG, JPEG, BMP, TIFF, TGA, GIF or PSD)",
        )
        .open_file(IMAGE_EXTENSIONS),
        ParamSpec::new(
            "image2",
            ParamKind::Path,
            true,
            "The second image (PNG, JPEG, BMP, TIFF, TGA, GIF or PSD)",
        )
        .open_file(IMAGE_EXTENSIONS),
        ParamSpec::new(
            "name",
            ParamKind::String,
            false,
            "A name for the first sequence",
        )
        .default_is("both file names"),
    ];

    const OPEN_PARAMS: &[ParamSpec] = &[ParamSpec::new(
        "path",
        ParamKind::Path,
        true,
        "The .inbetween project file to open",
    )
    .open_file(&["inbetween"])];

    const SAVE_PARAMS: &[ParamSpec] = &[ParamSpec::new(
        "path",
        ParamKind::Path,
        false,
        "Where to save; omit to save over the file the project was opened from",
    )
    .save_file(&["inbetween"])
    .default_is("the current project file")];

    /// An absolute path as a string, whether or not the file exists yet.
    fn absolute(path: &str) -> Result<String, CommandError> {
        std::path::absolute(path)
            .map(|p| p.display().to_string())
            .map_err(|e| CommandError::Io(format!("{path}: {e}")))
    }

    fn stem(path: &str) -> String {
        Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn new(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let (one, two) = (
            absolute(params.string("image1").unwrap_or_default())?,
            absolute(params.string("image2").unwrap_or_default())?,
        );
        let a = inbetween_codecs::read_file(Path::new(&one))?;
        inbetween_codecs::read_file(Path::new(&two))?; // fail early on an unreadable second image
        let name = match params.string("name") {
            Some(n) => n.to_string(),
            None => format!("{} to {}", stem(&one), stem(&two)),
        };
        let project = Project {
            output: OutputSettings {
                width: a.width,
                height: a.height,
            },
            sequences: vec![Sequence {
                name,
                image1: ImageRef::new(one),
                image2: ImageRef::new(two),
                ..Sequence::default()
            }],
            ..Project::default()
        };
        project.validate()?;
        session.set_project(project, None);
        Ok(json!({ "width": a.width, "height": a.height }))
    }

    fn open(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let path = PathBuf::from(params.string("path").unwrap_or_default());
        let project = inbetween_format::read_project(&path)?;
        let count = project.sequences.len();
        session.set_project(project, Some(path.clone()));
        Ok(json!({ "path": path.display().to_string(), "sequences": count }))
    }

    /// The project with absolute image paths rewritten relative to `dir`, so the saved file
    /// still finds its images when the folder moves. Relative references are kept.
    fn relative_to(project: &Project, dir: &Path) -> Result<Project, CommandError> {
        let dir = std::path::absolute(dir)
            .map_err(|e| CommandError::Io(format!("{}: {e}", dir.display())))?
            .display()
            .to_string();
        let fix = |r: &ImageRef| -> Result<ImageRef, CommandError> {
            if !Path::new(&r.0).is_absolute() {
                return Ok(r.clone());
            }
            inbetween_format::make_relative(&dir, &r.0).map(ImageRef).ok_or_else(|| {
                CommandError::BadParams(format!(
                    "cannot store `{}` relative to the project file (it is on a different drive); save the project next to its images or on the same drive",
                    r.0
                ))
            })
        };
        let mut out = project.clone();
        for seq in &mut out.sequences {
            seq.image1 = fix(&seq.image1)?;
            seq.image2 = fix(&seq.image2)?;
            for image in seq.effects.all_images_mut() {
                *image = fix(image)?;
            }
        }
        Ok(out)
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
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let relative = relative_to(session.project(), dir)?;
        inbetween_format::write_project(&path, &relative)?;
        // Keep the in-memory references in step with the file, so reopening finds the same images.
        *session.project_mut() = relative;
        session.set_path(path.clone());
        session.mark_saved();
        Ok(json!({ "path": path.display().to_string() }))
    }

    pub(super) fn specs() -> Vec<CommandSpec> {
        vec![
            CommandSpec {
                id: "project.new",
                label: "New From Two Images",
                menu: "File/New From Two Images...",
                shortcut: Some("Ctrl+N"),
                description: "Start a project that morphs one image into another. The output size is Image 1's size.",
                params: NEW_PARAMS,
                enabled: always,
                disabled_reason: "",
                edits: false,
                run: new,
            },
            CommandSpec {
                id: "project.open",
                label: "Open Project",
                menu: "File/Open...",
                shortcut: Some("Ctrl+O"),
                description: "Load a .inbetween project file, replacing the current project.",
                params: OPEN_PARAMS,
                enabled: always,
                disabled_reason: "",
                edits: false,
                run: open,
            },
            CommandSpec {
                id: "project.save",
                label: "Save Project",
                menu: "File/Save...",
                shortcut: Some("Ctrl+S"),
                description: "Write the project to a .inbetween file. Nothing is written if the project is invalid.",
                params: SAVE_PARAMS,
                enabled: always,
                disabled_reason: "",
                edits: false,
                run: save,
            },
        ]
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::Registry;

        fn temp(tag: &str) -> PathBuf {
            let dir =
                std::env::temp_dir().join(format!("inbetween-pc-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }

        fn write_png(path: &Path, w: u32, h: u32) {
            let rgba = vec![200u8; (w * h * 4) as usize];
            std::fs::write(path, inbetween_codecs::encode_png(w, h, &rgba).unwrap()).unwrap();
        }

        #[test]
        fn new_save_open_round_trip_keeps_images_findable() {
            let dir = temp("new");
            let (a, b) = (dir.join("imgs/a.png"), dir.join("imgs/b.png"));
            std::fs::create_dir_all(dir.join("imgs")).unwrap();
            write_png(&a, 40, 30);
            write_png(&b, 20, 10);
            let r = Registry::standard();
            let mut s = Session::new();
            let out = r
                .run(
                    &mut s,
                    "project.new",
                    &json!({"image1": a.to_str(), "image2": b.to_str()}),
                )
                .unwrap();
            assert_eq!(
                (out["width"].as_u64(), out["height"].as_u64()),
                (Some(40), Some(30))
            );
            assert_eq!(s.project().sequences[0].name, "a to b");
            assert!(
                s.prepare(0).is_ok(),
                "absolute references load in an unsaved project"
            );
            assert!(!s.is_dirty());

            r.run(&mut s, "dots.add", &json!({"x": 5, "y": 5})).unwrap();
            assert!(s.is_dirty());
            let file = dir.join("proj/movie.inbetween");
            std::fs::create_dir_all(dir.join("proj")).unwrap();
            r.run(&mut s, "project.save", &json!({"path": file.to_str()}))
                .unwrap();
            assert!(!s.is_dirty());
            // Stored relative, with forward slashes, so the folder can move.
            let saved = inbetween_format::read_project(&file).unwrap();
            assert_eq!(saved.sequences[0].image1.0, "../imgs/a.png");
            assert_eq!(saved.sequences[0].image2.0, "../imgs/b.png");
            assert!(s.prepare(0).is_ok());

            let mut fresh = Session::new();
            r.run(&mut fresh, "project.open", &json!({"path": file.to_str()}))
                .unwrap();
            assert_eq!(fresh.project(), s.project());
            assert!(fresh.prepare(0).is_ok());
            // Saving again with no path overwrites the same file.
            r.run(&mut fresh, "project.save", &json!({})).unwrap();
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn new_rejects_missing_and_corrupt_images() {
            let dir = temp("bad");
            let good = dir.join("g.png");
            write_png(&good, 8, 8);
            let junk = dir.join("junk.png");
            std::fs::write(&junk, b"nope").unwrap();
            let r = Registry::standard();
            let mut s = Session::new();
            for (a, b) in [
                (good.clone(), dir.join("missing.png")),
                (dir.join("missing.png"), good.clone()),
                (junk.clone(), good.clone()),
                (good.clone(), junk),
            ] {
                let e = r.run(
                    &mut s,
                    "project.new",
                    &json!({"image1": a.to_str(), "image2": b.to_str()}),
                );
                assert!(e.is_err());
                assert!(
                    !s.has_sequences(),
                    "a failed new must not replace the project"
                );
            }
            assert!(r.run(&mut s, "project.save", &json!({})).is_err()); // no path yet
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
