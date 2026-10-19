//! File-level commands that work across projects: `file.appendProject`, `file.wizard`,
//! `file.openVisual`.

use crate::session::resolve_against;
use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{
    ImageRef, MorphType, OutputSettings, Project, ProjectType, Sequence, MAX_SIDE,
};
use inbetween_format::MAX_SEQUENCES;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn always(_: &Session) -> bool {
    true
}

/// An absolute path as a string, whether or not the file exists yet.
pub(crate) fn absolute(path: &str) -> Result<String, CommandError> {
    std::path::absolute(path)
        .map(|p| p.display().to_string())
        .map_err(|e| CommandError::Io(format!("{path}: {e}")))
}

/// The file name without its folder or extension.
pub(crate) fn stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
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
        for image in seq.all_paths_mut() {
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

/// Most projects `file.openVisual` lists from one folder.
pub const MAX_VISUAL_ENTRIES: usize = 500;

const OPEN_VISUAL_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "folder",
        ParamKind::Path,
        false,
        "A folder whose .inbetween files to list",
    )
    .folder(),
    ParamSpec::new(
        "paths",
        ParamKind::PathList,
        false,
        "Specific .inbetween files to list (for example the recent ones)",
    )
    .open_files(&["inbetween"]),
];

fn modified_secs(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// What a project browser shows for one file. A bad file is an entry with an `error`, so one
/// broken file never hides the rest.
fn visual_entry(path: &Path) -> Value {
    use base64::Engine as _;
    let shown = path.display().to_string();
    let name = stem(&shown);
    let modified = modified_secs(path);
    let project = match inbetween_format::read_project(path) {
        Ok(p) => p,
        Err(e) => {
            return json!({
                "path": shown,
                "name": name,
                "modified": modified,
                "error": e.to_string(),
            })
        }
    };
    let thumbnail = match inbetween_format::read_thumbnail(path) {
        Ok(Some((_, _, png))) => Some(base64::engine::general_purpose::STANDARD.encode(png)),
        _ => None,
    };
    json!({
        "path": shown,
        "name": name,
        "modified": modified,
        "sequences": project.sequences.len(),
        "kind": project.kind.id(),
        "thumbnail_png_base64": thumbnail,
    })
}

/// List projects with their embedded thumbnails: those in a folder and/or those named in
/// `paths`. Only the files' own data is read; nothing is rendered.
fn open_visual(_: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let folder = params.string("folder");
    let listed = params.strings("paths");
    if folder.is_none() && listed.is_none() {
        return Err(CommandError::BadParams(
            "give a `folder` to browse or a list of `paths`".into(),
        ));
    }
    let mut files: Vec<PathBuf> = listed
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect();
    let mut truncated = false;
    if let Some(folder) = folder {
        let dir = Path::new(folder);
        let io = |e: std::io::Error| CommandError::Io(format!("{folder}: {e}"));
        if !std::fs::metadata(dir).map_err(io)?.is_dir() {
            return Err(CommandError::Io(format!("{folder}: not a folder")));
        }
        let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(io)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("inbetween"))
                    && p.is_file()
            })
            .collect();
        found.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_lowercase()));
        if found.len() > MAX_VISUAL_ENTRIES {
            found.truncate(MAX_VISUAL_ENTRIES);
            truncated = true;
        }
        files.extend(found);
    }
    let projects: Vec<Value> = files.iter().map(|p| visual_entry(p)).collect();
    Ok(json!({ "projects": projects, "truncated": truncated }))
}

const WIZARD_MORPHS: &[&str] = &[
    "morph",
    "morph_hold",
    "warp1",
    "warp2",
    "fade_out1",
    "fade_in2",
    "direct",
];

const WIZARD_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "images",
        ParamKind::PathList,
        true,
        "The images to chain, in order: each one morphs into the next",
    )
    .open_files(inbetween_codecs::IMAGE_EXTENSIONS)
    .range(2.0, 1000.0),
    ParamSpec::new(
        "kind",
        ParamKind::String,
        false,
        "How the sequences combine: one after another, or stacked as layers",
    )
    .one_of(&["sequence", "layer"])
    .default_is("sequence"),
    ParamSpec::new(
        "loop",
        ParamKind::Bool,
        false,
        "Also morph the last image back into the first",
    )
    .default_is("false"),
    ParamSpec::new(
        "morph",
        ParamKind::String,
        false,
        "Morph type of every sequence",
    )
    .one_of(WIZARD_MORPHS)
    .default_is("morph"),
    ParamSpec::new(
        "frames",
        ParamKind::Integer,
        false,
        "Morph frames in each sequence",
    )
    .range(2.0, 999.0)
    .default_is("30"),
    ParamSpec::new("width", ParamKind::Integer, false, "Output width in pixels")
        .range(1.0, MAX_SIDE as f64)
        .default_is("the first image's width"),
    ParamSpec::new(
        "height",
        ParamKind::Integer,
        false,
        "Output height in pixels",
    )
    .range(1.0, MAX_SIDE as f64)
    .default_is("the first image's height"),
];

/// Chain several images into a new project: A to B, B to C, and so on (and back to A when
/// `loop` is set). Every image is read first, so a bad file fails before anything changes.
fn wizard(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let listed = params.strings("images").unwrap_or_default();
    if listed.len() < 2 {
        return Err(CommandError::BadParams(
            "`images` needs at least two images".into(),
        ));
    }
    let images = listed
        .iter()
        .map(|p| absolute(p))
        .collect::<Result<Vec<_>, _>>()?;
    let mut first_size = (0, 0);
    for (i, path) in images.iter().enumerate() {
        let img = inbetween_codecs::read_file(Path::new(path))?;
        if i == 0 {
            first_size = (img.width, img.height);
        }
    }
    let kind = params
        .string("kind")
        .and_then(ProjectType::from_id)
        .unwrap_or_default();
    let morph = params
        .string("morph")
        .and_then(MorphType::from_id)
        .unwrap_or_default();
    let frames = params.integer("frames").map_or(30, |n| n as u32);
    let dim = |name: &str, default: u32| {
        params
            .integer(name)
            .map_or(default, |n| n.clamp(1, i64::from(MAX_SIDE)) as u32)
    };
    let mut pairs: Vec<(&String, &String)> = images.iter().zip(images.iter().skip(1)).collect();
    if params.boolean("loop").unwrap_or(false) {
        if let (Some(last), Some(first)) = (images.last(), images.first()) {
            pairs.push((last, first));
        }
    }
    if pairs.len() > MAX_SEQUENCES {
        return Err(CommandError::BadParams(format!(
            "that would make {} sequences; the limit is {MAX_SEQUENCES}",
            pairs.len()
        )));
    }
    let sequences: Vec<Sequence> = pairs
        .into_iter()
        .map(|(a, b)| Sequence {
            name: format!("{} to {}", stem(a), stem(b)),
            image1: ImageRef::new(a.as_str()),
            image2: ImageRef::new(b.as_str()),
            frames,
            morph,
            ..Sequence::default()
        })
        .collect();
    let project = Project {
        kind,
        output: OutputSettings {
            width: dim("width", first_size.0),
            height: dim("height", first_size.1),
        },
        sequences,
        ..Project::default()
    };
    project.validate()?;
    let out = json!({
        "sequences": project.sequences.len(),
        "kind": project.kind.id(),
        "width": project.output.width,
        "height": project.output.height,
    });
    session.set_project(project, None);
    Ok(out)
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "file.openVisual",
        label: "Open Visually",
        menu: "File/Open Visually...",
        shortcut: None,
        description: "List .inbetween projects (those in `folder` and the named `paths`) with their name, sequence count, kind and embedded thumbnail (base64 PNG). A project that cannot be read appears with an `error` instead.",
        params: OPEN_VISUAL_PARAMS,
        enabled: always,
        disabled_reason: "",
        edits: false,
        run: open_visual,
    },
    CommandSpec {
        id: "file.wizard",
        label: "New From Images (Wizard)",
        menu: "File/New From Images...",
        shortcut: None,
        description: "Start a project that chains several images: each morphs into the next, optionally looping back to the first. Replaces the current project. Add dots afterwards.",
        params: WIZARD_PARAMS,
        enabled: always,
        disabled_reason: "",
        edits: false,
        run: wizard,
    },
    CommandSpec {
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

    fn images(dir: &Path, names: &[&str]) -> Vec<String> {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let path = dir.join(n);
                write_png(&path, 30 + i as u32, 20, 50 * i as u8);
                path.display().to_string()
            })
            .collect()
    }

    fn pair_names(s: &Session) -> Vec<(String, String)> {
        s.project()
            .sequences
            .iter()
            .map(|q| (stem(&q.image1.0), stem(&q.image2.0)))
            .collect()
    }

    #[test]
    fn wizard_chains_images_with_and_without_a_loop() {
        let dir = temp("wizard");
        let r = Registry::standard();
        let names = ["a.png", "b.png", "c.png", "d.png"];
        let all = images(&dir, &names);
        for count in 2..=4 {
            for looped in [false, true] {
                let mut s = Session::new();
                let out = r
                    .run(
                        &mut s,
                        "file.wizard",
                        &json!({"images": &all[..count], "loop": looped, "frames": 12}),
                    )
                    .unwrap();
                let expected: Vec<(String, String)> = (0..count)
                    .map(|i| (i, (i + 1) % count))
                    .filter(|(i, j)| looped || j > i)
                    .map(|(i, j)| (stem(&all[i]), stem(&all[j])))
                    .collect();
                assert_eq!(pair_names(&s), expected, "{count} images, loop {looped}");
                assert_eq!(out["sequences"], expected.len());
                assert!(s.project().sequences.iter().all(|q| q.frames == 12));
                assert!(!s.is_dirty() && s.path().is_none());
                // The output is the first image's size.
                assert_eq!(
                    (s.project().output.width, s.project().output.height),
                    (30, 20)
                );
                // Everything renders.
                for i in 0..s.project().sequences.len() {
                    assert!(s.prepare(i).is_ok());
                }
            }
        }
        // Sequence names come from the file stems.
        let mut s = Session::new();
        r.run(&mut s, "file.wizard", &json!({"images": &all[..3]}))
            .unwrap();
        assert_eq!(s.project().sequences[1].name, "b to c");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wizard_options_apply() {
        let dir = temp("wizard-opts");
        let r = Registry::standard();
        let all = images(&dir, &["a.png", "b.png"]);
        let mut s = Session::new();
        r.run(
            &mut s,
            "file.wizard",
            &json!({"images": all, "kind": "layer", "morph": "warp1", "width": 64, "height": 48}),
        )
        .unwrap();
        assert_eq!(s.project().kind, ProjectType::Layer);
        assert_eq!(s.project().sequences[0].morph, MorphType::Warp1);
        assert_eq!(
            (s.project().output.width, s.project().output.height),
            (64, 48)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bad_wizard_request_leaves_the_project_untouched() {
        let dir = temp("wizard-bad");
        let r = Registry::standard();
        let good = images(&dir, &["a.png", "b.png"]);
        let junk = dir.join("junk.png");
        std::fs::write(&junk, b"nope").unwrap();
        let text = dir.join("notes.txt");
        std::fs::write(&text, b"hello").unwrap();
        let missing = dir.join("missing.png").display().to_string();

        let mut s = Session::new();
        r.run(&mut s, "file.new", &json!({"width": 111})).unwrap();
        r.run(&mut s, "dots.add", &json!({"x": 1, "y": 1})).unwrap();
        let before = s.project().clone();
        let dirty = s.is_dirty();
        for bad in [
            json!({"images": [good[0]]}),
            json!({"images": []}),
            json!({"images": [good[0], missing]}),
            json!({"images": [missing, good[1]]}),
            json!({"images": [good[0], junk.display().to_string()]}),
            json!({"images": [good[0], text.display().to_string()]}),
            json!({"images": [good[0], good[1]], "morph": "dissolve"}),
            json!({"images": [good[0], good[1]], "frames": 1}),
            json!({"images": "a.png"}),
            json!({}),
        ] {
            assert!(r.run(&mut s, "file.wizard", &bad).is_err(), "{bad}");
            assert_eq!(s.project(), &before, "{bad}");
            assert_eq!(s.is_dirty(), dirty);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wizard_reads_every_supported_format() {
        let dir = temp("wizard-formats");
        let r = Registry::standard();
        let data = [10u8, 20, 30, 255].repeat(6 * 4);
        let png = dir.join("a.png");
        std::fs::write(&png, inbetween_codecs::encode_png(6, 4, &data).unwrap()).unwrap();
        let mut bmp = Vec::new();
        {
            use image::ImageEncoder;
            image::codecs::bmp::BmpEncoder::new(&mut bmp)
                .write_image(&data, 6, 4, image::ExtendedColorType::Rgba8)
                .unwrap();
        }
        let bmp_path = dir.join("b.bmp");
        std::fs::write(&bmp_path, &bmp).unwrap();
        let mut tga = Vec::new();
        {
            use image::ImageEncoder;
            image::codecs::tga::TgaEncoder::new(&mut tga)
                .write_image(&data, 6, 4, image::ExtendedColorType::Rgba8)
                .unwrap();
        }
        let tga_path = dir.join("c.tga");
        std::fs::write(&tga_path, &tga).unwrap();
        let list: Vec<String> = [&png, &bmp_path, &tga_path]
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        let mut s = Session::new();
        r.run(
            &mut s,
            "file.wizard",
            &json!({"images": list, "loop": true}),
        )
        .unwrap();
        assert_eq!(s.project().sequences.len(), 3);
        for i in 0..3 {
            assert!(s.prepare(i).is_ok());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_visual_lists_a_folder_with_thumbnails_and_flags_bad_files() {
        let dir = temp("visual");
        let r = Registry::standard();
        let one = make_project(&r, &dir, "one", 10);
        let two = make_project(&r, &dir, "two", 90);
        let folder = dir.join("all");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::copy(&one, folder.join("B-one.inbetween")).unwrap();
        std::fs::copy(&two, folder.join("a-two.INBETWEEN")).unwrap();
        std::fs::write(folder.join("broken.inbetween"), b"{not json").unwrap();
        std::fs::write(folder.join("notes.txt"), b"ignore me").unwrap();
        std::fs::create_dir_all(folder.join("sub.inbetween")).unwrap();

        let mut s = Session::new();
        let out = r
            .run(
                &mut s,
                "file.openVisual",
                &json!({"folder": folder.to_str()}),
            )
            .unwrap();
        let list = out["projects"].as_array().unwrap();
        let names: Vec<&str> = list.iter().map(|e| e["name"].as_str().unwrap()).collect();
        // Sorted by name without regard to case; the text file and the folder are skipped.
        assert_eq!(names, ["a-two", "B-one", "broken"]);
        assert_eq!(out["truncated"], false);
        for e in &list[..2] {
            assert_eq!(e["sequences"], 1);
            assert_eq!(e["kind"], "sequence");
            use base64::Engine as _;
            let png = base64::engine::general_purpose::STANDARD
                .decode(e["thumbnail_png_base64"].as_str().unwrap())
                .unwrap();
            let img = inbetween_codecs::decode(&png).unwrap();
            assert_eq!((img.width, img.height), (32, 24));
            assert!(e["modified"].as_u64().is_some());
        }
        assert!(list[2]["error"].is_string() && list[2].get("sequences").is_none());
        // Listing never changes the session.
        assert!(!s.has_sequences() && !s.is_dirty());

        // `paths` works alone and together with a folder; a missing path is an error entry.
        let out = r
            .run(
                &mut s,
                "file.openVisual",
                &json!({"paths": [one.to_str(), dir.join("gone.inbetween").to_str()]}),
            )
            .unwrap();
        let list = out["projects"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert!(list[0]["error"].is_null() && list[1]["error"].is_string());
        let out = r
            .run(
                &mut s,
                "file.openVisual",
                &json!({"paths": [one.to_str()], "folder": folder.to_str()}),
            )
            .unwrap();
        assert_eq!(out["projects"].as_array().unwrap().len(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_visual_handles_projects_without_thumbnails_and_bad_requests() {
        let dir = temp("visual-bad");
        let r = Registry::standard();
        let mut s = Session::new();
        let plain = dir.join("plain.inbetween");
        inbetween_format::write_project(&plain, &Project::default()).unwrap();
        let out = r
            .run(
                &mut s,
                "file.openVisual",
                &json!({"paths": [plain.to_str()]}),
            )
            .unwrap();
        assert!(out["projects"][0]["thumbnail_png_base64"].is_null());
        assert_eq!(out["projects"][0]["sequences"], 0);
        for bad in [
            json!({}),
            json!({"folder": dir.join("missing").to_str()}),
            json!({"folder": plain.to_str()}), // a file, not a folder
        ] {
            assert!(r.run(&mut s, "file.openVisual", &bad).is_err(), "{bad}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_visual_stops_at_the_entry_cap() {
        let dir = temp("visual-cap");
        let r = Registry::standard();
        let mut s = Session::new();
        let one = dir.join("seed.inbetween");
        inbetween_format::write_project(&one, &Project::default()).unwrap();
        for i in 0..MAX_VISUAL_ENTRIES + 3 {
            std::fs::copy(&one, dir.join(format!("p{i:04}.inbetween"))).unwrap();
        }
        let out = r
            .run(&mut s, "file.openVisual", &json!({"folder": dir.to_str()}))
            .unwrap();
        assert_eq!(
            out["projects"].as_array().unwrap().len(),
            MAX_VISUAL_ENTRIES
        );
        assert_eq!(out["truncated"], true);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
