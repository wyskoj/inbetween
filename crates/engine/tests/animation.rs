//! `export.animation` and `export.video` through the command registry.

use inbetween_engine::{Registry, Session};
use inbetween_testkit::demo;
use serde_json::{json, Value};
use std::path::PathBuf;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("inbetween-an-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (sun, moon) = demo::pair(24, 18).unwrap();
        for (name, img) in [("sun.png", sun), ("moon.png", moon)] {
            let png = inbetween_codecs::encode_png(24, 18, &img.to_rgba8()).unwrap();
            std::fs::write(root.join(name), png).unwrap();
        }
        Self(root)
    }

    fn s(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn try_run(s: &mut Session, id: &str, p: Value) -> Result<Value, inbetween_engine::CommandError> {
    Registry::standard().run(s, id, &p)
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    try_run(s, id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// Sun to moon (4 frames), then moon to sun (4 frames): 8 movie frames.
fn session(d: &Dir) -> Session {
    let mut s = Session::new();
    run(
        &mut s,
        "project.new",
        json!({"image1": d.s("sun.png"), "image2": d.s("moon.png")}),
    );
    run(&mut s, "sequence.length", json!({"frames": 4}));
    run(&mut s, "sequence.add", json!({"image2": d.s("sun.png")}));
    s
}

fn ffmpeg_available() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[test]
fn gif_apng_and_webp_hold_every_frame() {
    let d = Dir::new("formats");
    let mut s = session(&d);
    for (file, format, magic) in [
        ("a.gif", "gif", &b"GIF89a"[..]),
        ("a.png", "apng", &b"\x89PNG"[..]),
        ("a.webp", "webp", &b"RIFF"[..]),
    ] {
        let v = run(
            &mut s,
            "export.animation",
            json!({"out": d.s(file), "fps": 12}),
        );
        assert_eq!(v["format"], format);
        assert_eq!(v["frames"], 8);
        let bytes = std::fs::read(d.0.join(file)).unwrap();
        assert!(bytes.starts_with(magic), "{file}");
    }
    // The animated PNG really has 8 frames.
    let reader = png::Decoder::new(std::io::Cursor::new(
        std::fs::read(d.0.join("a.png")).unwrap(),
    ))
    .read_info()
    .unwrap();
    assert_eq!(reader.info().animation_control().unwrap().num_frames, 8);
}

#[test]
fn range_reverse_and_duplicate_dropping() {
    let d = Dir::new("range");
    let mut s = session(&d);
    let count = |s: &mut Session, extra: Value| {
        let mut p = json!({"out": d.s("r.png")});
        p.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        run(s, "export.animation", p)["frames"].as_u64().unwrap()
    };
    assert_eq!(count(&mut s, json!({"first": 3, "last": 6})), 4);
    assert_eq!(count(&mut s, json!({"reverse": true})), 8);
    // Sequence 2 starts on the frame sequence 1 ends on: one duplicate at the join.
    assert_eq!(count(&mut s, json!({"drop_duplicates": true})), 7);
    assert_eq!(
        count(&mut s, json!({"drop_duplicates": true, "reverse": true})),
        7
    );
}

#[test]
fn reversed_animation_starts_on_the_last_frame() {
    let d = Dir::new("reverse");
    let mut s = Session::new();
    run(
        &mut s,
        "project.new",
        json!({"image1": d.s("sun.png"), "image2": d.s("moon.png")}),
    );
    run(&mut s, "sequence.length", json!({"frames": 4}));
    let first_frame = |s: &mut Session, extra: Value| {
        let mut p = json!({"out": d.s("x.png")});
        p.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        run(s, "export.animation", p);
        let bytes = std::fs::read(d.0.join("x.png")).unwrap();
        let mut r = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut buf = vec![0; r.output_buffer_size().unwrap()];
        r.next_frame(&mut buf).unwrap();
        buf
    };
    let moon = first_frame(&mut s, json!({"first": 4, "last": 4}));
    let sun = first_frame(&mut s, json!({"first": 1, "last": 1}));
    assert_ne!(moon, sun);
    assert_eq!(first_frame(&mut s, json!({"reverse": true})), moon);
    assert_eq!(first_frame(&mut s, json!({})), sun);
}

#[test]
fn transparent_and_flattened_gifs_differ() {
    let d = Dir::new("alpha");
    let mut s = Session::new();
    run(
        &mut s,
        "project.new",
        json!({"image1": d.s("sun.png"), "image2": d.s("moon.png")}),
    );
    run(&mut s, "sequence.fit", json!({"fit": "contain"}));
    let a = run(
        &mut s,
        "export.animation",
        json!({"out": d.s("t.gif"), "transparent": true}),
    );
    let b = run(
        &mut s,
        "export.animation",
        json!({"out": d.s("o.gif"), "transparent": false, "matte": "#ff0000"}),
    );
    assert_eq!(
        (a["frames"].clone(), b["frames"].clone()),
        (json!(30), json!(30))
    );
    assert_ne!(
        std::fs::read(d.0.join("t.gif")).unwrap(),
        std::fs::read(d.0.join("o.gif")).unwrap()
    );
}

#[test]
fn bad_requests_are_actionable_errors_and_write_nothing() {
    let d = Dir::new("bad");
    let mut s = session(&d);
    let cases = [
        (json!({"out": d.s("x.bmp")}), "format"),
        (json!({"out": d.s("x.gif"), "fps": 0}), "fps"),
        (json!({"out": d.s("x.gif"), "fps": 100}), "fps"),
        (json!({"out": d.s("x.gif"), "matte": "white"}), "color"),
        (json!({"out": d.s("x.gif"), "first": 5, "last": 2}), "first"),
        (json!({"out": d.s("x.gif"), "format": "avi"}), "format"),
    ];
    for (p, word) in cases {
        let e = try_run(&mut s, "export.animation", p.clone())
            .unwrap_err()
            .to_string();
        assert!(e.to_lowercase().contains(word), "{p}: {e}");
    }
    assert!(!d.0.join("x.gif").exists() && !d.0.join("x.bmp").exists());
    // No sequences at all: the command is disabled, not a crash.
    let mut empty = Session::new();
    assert!(try_run(&mut empty, "export.animation", json!({"out": d.s("e.gif")})).is_err());
    assert!(try_run(&mut empty, "export.video", json!({"out": d.s("e.mp4")})).is_err());
}

#[test]
fn video_without_ffmpeg_says_how_to_fix_it() {
    let d = Dir::new("noffmpeg");
    let mut s = session(&d);
    let e = try_run(
        &mut s,
        "export.video",
        json!({"out": d.s("v.mp4"), "ffmpeg": d.s("no-such-ffmpeg")}),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("ffmpeg") && e.contains("install"), "{e}");
}

#[test]
fn video_formats_encode_with_ffmpeg() {
    if !ffmpeg_available() {
        eprintln!("SKIP: ffmpeg is not installed");
        return;
    }
    let d = Dir::new("video");
    let mut s = session(&d);
    for file in ["v.mp4", "v.webm", "v.mov"] {
        let v = run(&mut s, "export.video", json!({"out": d.s(file), "fps": 24}));
        assert_eq!(v["frames"], 8, "{file}");
        let size = std::fs::metadata(d.0.join(file)).unwrap().len();
        assert!(size > 200, "{file} is {size} bytes");
    }
    // Odd output sizes still make a valid H.264 file (the encoder needs even sides).
    let (a, b) = demo::pair(25, 19).unwrap();
    for (name, img) in [("odd1.png", a), ("odd2.png", b)] {
        let png = inbetween_codecs::encode_png(25, 19, &img.to_rgba8()).unwrap();
        std::fs::write(d.0.join(name), png).unwrap();
    }
    let mut odd = Session::new();
    run(
        &mut odd,
        "project.new",
        json!({"image1": d.s("odd1.png"), "image2": d.s("odd2.png")}),
    );
    run(&mut odd, "sequence.length", json!({"frames": 3}));
    run(&mut odd, "export.video", json!({"out": d.s("odd.mp4")}));
    assert!(d.0.join("odd.mp4").exists());
}
