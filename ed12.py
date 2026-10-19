def sub(p, pairs):
    s = open(p, encoding='utf-8').read()
    for a, b in pairs:
        assert a in s, (p, a[:60])
        s = s.replace(a, b, 1)
    open(p, 'w', encoding='utf-8').write(s)

p = 'crates/engine/src/file_cmds.rs'
sub(p, [
('//! File-level commands that work across projects: `file.appendProject`.', '//! File-level commands that work across projects: `file.appendProject`, `file.wizard`.'),
('use inbetween_doc::{ImageRef, Project, Sequence};', 'use inbetween_doc::{\n    ImageRef, MorphType, OutputSettings, Project, ProjectType, Sequence, MAX_SIDE,\n};'),
('''fn always(_: &Session) -> bool {
    true
}
''', '''fn always(_: &Session) -> bool {
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
'''),
('''pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "file.appendProject",''', '''const WIZARD_MORPHS: &[&str] = &[
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
        id: "file.appendProject",'''),
])

# wizard tests, appended before the final closing of tests mod
s = open(p, encoding='utf-8').read()
tests = '''
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
        r.run(&mut s, "file.wizard", &json!({"images": list, "loop": true}))
            .unwrap();
        assert_eq!(s.project().sequences.len(), 3);
        for i in 0..3 {
            assert!(s.prepare(i).is_ok());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
'''
i = s.rindex('}')
s = s[:i] + tests + '}\n'
open(p, 'w', encoding='utf-8').write(s)
