//! Project-level commands: `project.info`, `file.new`, `project.new`, `project.open`,
//! `project.save` and `project.saveAs`.

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
pub(crate) mod files {
    use super::*;
    use crate::{ParamKind, ParamSpec};
    use inbetween_doc::{ImageRef, OutputSettings, Project, ProjectType, Sequence, MAX_SIDE};
    use std::path::{Path, PathBuf};

    use crate::source_media::SOURCE_EXTENSIONS as IMAGE_EXTENSIONS;

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

    const SAVE_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "path",
            ParamKind::Path,
            false,
            "Where to save; omit to save over the file the project was opened from",
        )
        .save_file(&["inbetween"])
        .default_is("the current project file"),
        THUMBNAIL,
    ];

    const SAVE_AS_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "path",
            ParamKind::Path,
            true,
            "The .inbetween file to save to",
        )
        .save_file(&["inbetween"]),
        THUMBNAIL,
    ];

    const THUMBNAIL: ParamSpec = ParamSpec::new(
        "thumbnail",
        ParamKind::Bool,
        false,
        "Embed a small preview of the movie's middle frame in the file",
    )
    .default_is("true");

    const FILE_NEW_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "kind",
            ParamKind::String,
            false,
            "How the sequences combine: one after another, or stacked as layers",
        )
        .one_of(&["sequence", "layer"])
        .default_is("sequence"),
        ParamSpec::new("width", ParamKind::Integer, false, "Output width in pixels")
            .range(1.0, MAX_SIDE as f64)
            .default_is("1280"),
        ParamSpec::new(
            "height",
            ParamKind::Integer,
            false,
            "Output height in pixels",
        )
        .range(1.0, MAX_SIDE as f64)
        .default_is("720"),
    ];

    /// Longest side of the embedded thumbnail, in pixels.
    const THUMBNAIL_SIDE: u32 = 256;

    use crate::file_cmds::{absolute, stem};

    fn new(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let (one, two) = (
            absolute(params.string("image1").unwrap_or_default())?,
            absolute(params.string("image2").unwrap_or_default())?,
        );
        let a = crate::source_media::read_any(Path::new(&one))?;
        crate::source_media::read_any(Path::new(&two))?; // fail early on an unreadable second image
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

    /// An empty project: one blank sequence, nothing saved yet.
    fn file_new(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let defaults = OutputSettings::default();
        let dim = |name: &str, default: u32| {
            params
                .integer(name)
                .map_or(default, |n| n.clamp(1, i64::from(MAX_SIDE)) as u32)
        };
        let project = Project {
            kind: params
                .string("kind")
                .and_then(ProjectType::from_id)
                .unwrap_or_default(),
            output: OutputSettings {
                width: dim("width", defaults.width),
                height: dim("height", defaults.height),
            },
            sequences: vec![Sequence::default()],
            ..Project::default()
        };
        project.validate()?;
        let out = json!({
            "kind": project.kind.id(),
            "width": project.output.width,
            "height": project.output.height,
        });
        session.set_project(project, None);
        Ok(out)
    }

    /// Nothing open: no sequences and no file, as when the app starts.
    fn close(session: &mut Session, _: &Params) -> Result<Value, CommandError> {
        session.set_project(Project::default(), None);
        Ok(json!({}))
    }

    fn has_project(session: &Session) -> bool {
        session.has_sequences() || session.path().is_some()
    }

    fn open(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let path = PathBuf::from(params.string("path").unwrap_or_default());
        let project = inbetween_format::read_project(&path)?;
        let count = project.sequences.len();
        session.set_project(project, Some(path.clone()));
        Ok(json!({ "path": path.display().to_string(), "sequences": count }))
    }

    /// The project with every image path rewritten relative to `new_dir`, so the saved file
    /// still finds its images when the folder moves. References that are relative today are
    /// relative to `old_dir` (where the project was loaded from); when `new_dir` is another
    /// folder they are rebased. An image with no relative path (another drive) keeps its
    /// absolute path; those are listed in the second value.
    pub(crate) fn relative_to(
        project: &Project,
        old_dir: &Path,
        new_dir: &Path,
    ) -> Result<(Project, Vec<String>), CommandError> {
        let absolute = |dir: &Path| {
            std::path::absolute(dir)
                .map(|p| p.display().to_string())
                .map_err(|e| CommandError::Io(format!("{}: {e}", dir.display())))
        };
        let (old, new) = (absolute(old_dir)?, absolute(new_dir)?);
        let same_dir = inbetween_format::make_relative(&old, &new).as_deref() == Some("");
        let mut kept: Vec<String> = Vec::new();
        let mut fix = |r: &ImageRef| -> ImageRef {
            if r.0.is_empty() || (same_dir && !Path::new(&r.0).is_absolute()) {
                return r.clone();
            }
            let full = if Path::new(&r.0).is_absolute() {
                r.0.clone()
            } else {
                format!("{old}/{}", r.0)
            };
            match inbetween_format::make_relative(&new, &full) {
                Some(rel) => ImageRef(rel),
                None => {
                    if !kept.contains(&full) {
                        kept.push(full.clone());
                    }
                    ImageRef(full)
                }
            }
        };
        let mut out = project.clone();
        for image in out.all_paths_mut() {
            *image = fix(image);
        }
        Ok((out, kept))
    }

    /// The preview embedded in a saved project: the first sequence's middle frame. Any
    /// failure (no sequences, a missing image, a render error) just means no thumbnail.
    fn make_thumbnail(session: &mut Session) -> Option<inbetween_format::Thumbnail> {
        if !session.has_sequences() {
            return None;
        }
        let prepared = session.prepare(0).ok()?;
        let frame = prepared.renderer().ok()?.render_t(0.5).ok()?;
        let small = frame.downscale_to_fit(THUMBNAIL_SIDE).ok()?;
        let png =
            inbetween_codecs::encode_png(small.width(), small.height(), &small.to_rgba8()).ok()?;
        inbetween_format::Thumbnail::from_png(small.width(), small.height(), &png).ok()
    }

    fn save_to(
        session: &mut Session,
        path: PathBuf,
        params: &Params,
    ) -> Result<Value, CommandError> {
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let old_dir = session
            .path()
            .and_then(Path::parent)
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let (relative, absolute_images) = relative_to(session.project(), &old_dir, dir)?;
        // Render before the path changes, while the images still resolve as they do now.
        let thumbnail = if params.boolean("thumbnail").unwrap_or(true) {
            make_thumbnail(session)
        } else {
            None
        };
        inbetween_format::write_project_with_thumbnail(&path, &relative, thumbnail.as_ref())?;
        // Keep the in-memory references in step with the file, so reopening finds the same images.
        *session.project_mut() = relative;
        session.set_path(path.clone());
        session.mark_saved();
        Ok(json!({
            "path": path.display().to_string(),
            "absolute_images": absolute_images,
            "thumbnail": thumbnail.is_some(),
        }))
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
        save_to(session, path, params)
    }

    fn save_as(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let path = PathBuf::from(params.string("path").unwrap_or_default());
        save_to(session, path, params)
    }

    /// Save the project to its own file when it has one and has unsaved changes. Returns the
    /// path written, or `None` when there was nothing to do.
    pub(crate) fn save_if_dirty(session: &mut Session) -> Result<Option<String>, CommandError> {
        let Some(path) = session.path().map(PathBuf::from) else {
            return Ok(None);
        };
        if !session.is_dirty() {
            return Ok(None);
        }
        let shown = path.display().to_string();
        save_to(session, path, &Params::default())?;
        Ok(Some(shown))
    }

    pub(super) fn specs() -> Vec<CommandSpec> {
        vec![
            CommandSpec {
                id: "file.new",
                label: "New Project",
                menu: "File/New Project",
                shortcut: Some("Ctrl+N"),
                description: "Start an empty project with one blank sequence, replacing the current project.",
                params: FILE_NEW_PARAMS,
                enabled: always,
                disabled_reason: "",
                edits: false,
                run: file_new,
            },
            CommandSpec {
                id: "project.new",
                label: "New From Two Images",
                menu: "File/New From Two Images...",
                shortcut: None,
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
                menu: "File/Save",
                shortcut: Some("Ctrl+S"),
                description: "Write the project to a .inbetween file. Nothing is written if the project is invalid.",
                params: SAVE_PARAMS,
                enabled: always,
                disabled_reason: "",
                edits: false,
                run: save,
            },
            CommandSpec {
                id: "project.saveAs",
                label: "Save Project As",
                menu: "File/Save As...",
                shortcut: Some("Ctrl+Shift+S"),
                description: "Save the project to a new .inbetween file; later saves go there. Images are stored relative to the new file where possible.",
                params: SAVE_AS_PARAMS,
                enabled: always,
                disabled_reason: "",
                edits: false,
                run: save_as,
            },
            CommandSpec {
                id: "file.close",
                label: "Close Project",
                menu: "File/Close Project",
                shortcut: Some("Ctrl+W"),
                description: "Close the project and return to the start screen. Unsaved changes are lost; the shell asks first.",
                params: &[],
                enabled: has_project,
                disabled_reason: "No project is open.",
                edits: false,
                run: close,
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
        fn file_close_leaves_nothing_open() {
            let r = Registry::standard();
            let mut s = Session::new();
            assert!(!r.is_enabled("file.close", &s));
            r.run(&mut s, "file.new", &json!({})).unwrap();
            r.run(&mut s, "sequence.addBlank", &json!({})).unwrap();
            assert!(s.is_dirty() && r.is_enabled("file.close", &s));
            r.run(&mut s, "file.close", &json!({})).unwrap();
            assert!(!s.has_sequences() && s.path().is_none());
            assert!(!s.is_dirty() && !s.can_undo());
            assert!(!r.is_enabled("file.close", &s));
        }

        #[test]
        fn file_new_makes_a_clean_project_with_one_blank_sequence() {
            let dir = temp("filenew");
            let (a, b) = (dir.join("a.png"), dir.join("b.png"));
            write_png(&a, 8, 8);
            write_png(&b, 8, 8);
            let r = Registry::standard();
            let mut s = Session::new();
            r.run(
                &mut s,
                "project.new",
                &json!({"image1": a.to_str(), "image2": b.to_str()}),
            )
            .unwrap();
            r.run(&mut s, "dots.add", &json!({"x": 1, "y": 1})).unwrap();
            assert!(s.is_dirty());
            let out = r
                .run(
                    &mut s,
                    "file.new",
                    &json!({"kind": "layer", "width": 320, "height": 200}),
                )
                .unwrap();
            assert_eq!(out["kind"], "layer");
            assert!(!s.is_dirty() && !s.can_undo());
            assert!(s.path().is_none());
            assert_eq!(s.project().sequences.len(), 1);
            assert_eq!(s.project().sequences[0], Sequence::default());
            assert_eq!(
                (s.project().output.width, s.project().output.height),
                (320, 200)
            );
            // Defaults, and bad parameters are refused without touching the project.
            r.run(&mut s, "file.new", &json!({})).unwrap();
            assert_eq!(s.project().output, OutputSettings::default());
            assert_eq!(s.project().kind, ProjectType::Sequence);
            for bad in [
                json!({"kind": "stack"}),
                json!({"width": 0}),
                json!({"height": 99999999}),
            ] {
                assert!(r.run(&mut s, "file.new", &bad).is_err());
            }
            let _ = std::fs::remove_dir_all(&dir);
        }

        fn two_image_session(dir: &Path) -> (Registry, Session) {
            let (a, b) = (dir.join("a.png"), dir.join("b.png"));
            write_png(&a, 40, 30);
            write_png(&b, 40, 30);
            let r = Registry::standard();
            let mut s = Session::new();
            r.run(
                &mut s,
                "project.new",
                &json!({"image1": a.to_str(), "image2": b.to_str()}),
            )
            .unwrap();
            (r, s)
        }

        #[test]
        fn save_as_switches_the_file_and_clears_dirty() {
            let dir = temp("saveas");
            let (r, mut s) = two_image_session(&dir);
            r.run(&mut s, "dots.add", &json!({"x": 5, "y": 5})).unwrap();
            let first = dir.join("one.inbetween");
            let second = dir.join("sub/two.inbetween");
            std::fs::create_dir_all(dir.join("sub")).unwrap();
            r.run(&mut s, "project.saveAs", &json!({"path": first.to_str()}))
                .unwrap();
            assert_eq!(s.path(), Some(first.as_path()));
            assert!(!s.is_dirty());
            r.run(&mut s, "dots.add", &json!({"x": 9, "y": 9})).unwrap();
            assert!(s.is_dirty());
            r.run(&mut s, "project.saveAs", &json!({"path": second.to_str()}))
                .unwrap();
            assert_eq!(s.path(), Some(second.as_path()));
            assert!(!s.is_dirty());
            // The second file is re-relativised and renders; the first is untouched.
            let saved = inbetween_format::read_project(&second).unwrap();
            assert_eq!(saved.sequences[0].image1.0, "../a.png");
            assert_eq!(
                inbetween_format::read_project(&first).unwrap().sequences[0]
                    .dots
                    .len(),
                1
            );
            assert!(s.prepare(0).is_ok());
            // `project.save` with no path now writes to the second file.
            r.run(&mut s, "project.save", &json!({})).unwrap();
            // Save As needs a path.
            assert!(r.run(&mut s, "project.saveAs", &json!({})).is_err());
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn relative_to_keeps_absolute_paths_when_no_relative_exists() {
            // A project folder on one drive and images on another cannot be related.
            if cfg!(windows) {
                let mut p = Project::default();
                p.sequences.push(Sequence {
                    image1: ImageRef::new("C:/work/imgs/a.png"),
                    image2: ImageRef::new("D:\\photos\\b.png"),
                    ..Sequence::default()
                });
                let (out, kept) =
                    relative_to(&p, Path::new("C:/work/proj"), Path::new("C:/work/proj")).unwrap();
                assert_eq!(out.sequences[0].image1.0, "../imgs/a.png");
                assert_eq!(out.sequences[0].image2.0, "D:\\photos\\b.png");
                assert_eq!(kept, ["D:\\photos\\b.png"]);
                // The result is a valid project file.
                assert!(inbetween_format::project_to_string(&out).is_ok());
            }
            // Relative references pass through on every platform.
            let mut p = Project::default();
            p.sequences.push(Sequence {
                image1: ImageRef::new("x/a.png"),
                ..Sequence::default()
            });
            let (out, kept) =
                relative_to(&p, Path::new("somewhere"), Path::new("somewhere")).unwrap();
            assert_eq!(out, p);
            assert!(kept.is_empty());
            // Moving to a sibling folder rebases the reference.
            let (out, _) = relative_to(&p, Path::new("one"), Path::new("two")).unwrap();
            assert_eq!(out.sequences[0].image1.0, "../one/x/a.png");
        }

        #[test]
        fn save_embeds_a_thumbnail_that_decodes() {
            let dir = temp("thumb");
            let (r, mut s) = two_image_session(&dir);
            let file = dir.join("t.inbetween");
            let out = r
                .run(&mut s, "project.save", &json!({"path": file.to_str()}))
                .unwrap();
            assert_eq!(out["thumbnail"], true);
            let (w, h, png) = inbetween_format::read_thumbnail(&file).unwrap().unwrap();
            let img = inbetween_codecs::decode(&png).unwrap();
            assert_eq!((img.width, img.height), (w, h));
            // 40 x 30 is below the cap, so it is kept whole.
            assert_eq!((w, h), (40, 30));
            // The thumbnail is of the real frame (the test images are 200 grey, 200 alpha).
            assert_eq!(&img.data[..4], &[200, 200, 200, 200]);
            // Opting out leaves none.
            let out = r
                .run(
                    &mut s,
                    "project.save",
                    &json!({"path": file.to_str(), "thumbnail": false}),
                )
                .unwrap();
            assert_eq!(out["thumbnail"], false);
            assert_eq!(inbetween_format::read_thumbnail(&file).unwrap(), None);
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn large_frames_get_a_small_thumbnail() {
            let dir = temp("bigthumb");
            let (a, b) = (dir.join("a.png"), dir.join("b.png"));
            write_png(&a, 1024, 512);
            write_png(&b, 1024, 512);
            let r = Registry::standard();
            let mut s = Session::new();
            r.run(
                &mut s,
                "project.new",
                &json!({"image1": a.to_str(), "image2": b.to_str()}),
            )
            .unwrap();
            let file = dir.join("t.inbetween");
            r.run(&mut s, "project.save", &json!({"path": file.to_str()}))
                .unwrap();
            let (w, h, _) = inbetween_format::read_thumbnail(&file).unwrap().unwrap();
            assert_eq!((w, h), (256, 128));
            let _ = std::fs::remove_dir_all(&dir);
        }

        /// Timing probe: `cargo test --release -p inbetween-engine thumbnail_cost -- --ignored --nocapture`.
        #[test]
        #[ignore = "timing probe"]
        fn thumbnail_cost_at_1080p() {
            let dir = temp("cost");
            let (a, b) = (dir.join("a.png"), dir.join("b.png"));
            write_png(&a, 1920, 1080);
            write_png(&b, 1920, 1080);
            let r = Registry::standard();
            let mut s = Session::new();
            r.run(
                &mut s,
                "project.new",
                &json!({"image1": a.to_str(), "image2": b.to_str()}),
            )
            .unwrap();
            for i in 0..50 {
                let (x, y) = (100 + i * 30, 100 + (i % 7) * 100);
                r.run(&mut s, "dots.add", &json!({"x": x, "y": y})).unwrap();
            }
            let file = dir.join("t.inbetween");
            let mut save = |thumb: bool| {
                let t = std::time::Instant::now();
                r.run(
                    &mut s,
                    "project.save",
                    &json!({"path": file.to_str(), "thumbnail": thumb}),
                )
                .unwrap();
                t.elapsed()
            };
            let (cold_without, cold_with) = (save(false), save(true));
            let (without, with) = (save(false), save(true));
            println!("save 1080p: cold {cold_without:?} / {cold_with:?}; warm without thumbnail {without:?}, with {with:?}");
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn save_without_renderable_images_still_succeeds() {
            let dir = temp("nothumb");
            let r = Registry::standard();
            let mut s = Session::new();
            r.run(&mut s, "file.new", &json!({})).unwrap(); // blank sequence: no images
            let file = dir.join("blank.inbetween");
            let out = r
                .run(&mut s, "project.save", &json!({"path": file.to_str()}))
                .unwrap();
            assert_eq!(out["thumbnail"], false);
            assert_eq!(inbetween_format::read_thumbnail(&file).unwrap(), None);
            assert!(inbetween_format::read_project(&file).is_ok());
            // A project whose images went missing after being set also saves.
            let (r, mut s) = two_image_session(&dir);
            std::fs::remove_file(dir.join("a.png")).unwrap();
            let out = r
                .run(&mut s, "project.save", &json!({"path": file.to_str()}))
                .unwrap();
            assert_eq!(out["thumbnail"], false);
            // And an empty project (no sequences at all).
            let mut empty = Session::new();
            r.run(&mut empty, "project.save", &json!({"path": file.to_str()}))
                .unwrap();
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
