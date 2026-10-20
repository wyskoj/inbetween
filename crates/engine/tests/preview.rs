//! `frame.preview`: a small base64 PNG of any frame, nothing written to disk.

use base64::Engine as _;
use inbetween_doc::{DotPair, DotSet, ImageRef, OutputSettings, Project, Sequence};
use inbetween_engine::{CommandError, Registry, Session};
use inbetween_geom::Point;
use serde_json::{json, Value};
use std::path::PathBuf;

const W: u32 = 64;
const H: u32 = 40;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("inbetween-pv-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
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

fn session(dir: &Dir) -> Session {
    for (name, k) in [("a.png", 1u32), ("b.png", 3)] {
        let mut rgba = Vec::new();
        for y in 0..H {
            for x in 0..W {
                rgba.extend_from_slice(&[(x * 4 * k) as u8, (y * 6) as u8, (50 * k) as u8, 255]);
            }
        }
        std::fs::write(
            dir.0.join(name),
            inbetween_codecs::encode_png(W, H, &rgba).unwrap(),
        )
        .unwrap();
    }
    let p = |x: f64, y: f64| Point::new(x, y);
    let seq = Sequence {
        image1: ImageRef::new(dir.s("a.png")),
        image2: ImageRef::new(dir.s("b.png")),
        dots: DotSet::new(vec![
            DotPair::new(p(16.0, 10.0), p(20.0, 12.0)),
            DotPair::new(p(48.0, 30.0), p(44.0, 28.0)),
        ]),
        frames: 5,
        ..Sequence::default()
    };
    let mut s = Session::new();
    s.set_project(
        Project {
            output: OutputSettings {
                width: W,
                height: H,
            },
            sequences: vec![seq],
            ..Project::default()
        },
        None,
    );
    s
}

fn run(s: &mut Session, params: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, "frame.preview", &params)
}

fn decode(v: &Value) -> inbetween_codecs::Rgba8Image {
    let b64 = v["png_base64"].as_str().unwrap();
    let png = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    inbetween_codecs::decode(&png).unwrap()
}

#[test]
fn small_frames_match_export_frame_exactly() {
    let dir = Dir::new("same");
    let mut s = session(&dir);
    let v = run(&mut s, json!({"t": 0.5, "max_size": 4096})).unwrap();
    let out = dir.s("f.png");
    Registry::standard()
        .run(&mut s, "export.frame", &json!({"out": out, "t": 0.5}))
        .unwrap();
    let want = inbetween_codecs::decode(&std::fs::read(&out).unwrap()).unwrap();
    let got = decode(&v);
    assert_eq!((got.width, got.height), (W, H));
    assert_eq!(got.data, want.data);
}

#[test]
fn max_size_keeps_the_aspect_ratio() {
    let dir = Dir::new("size");
    let mut s = session(&dir);
    let v = run(&mut s, json!({"max_size": 32})).unwrap();
    assert_eq!(
        (v["width"].as_u64(), v["height"].as_u64()),
        (Some(32), Some(20))
    );
    let img = decode(&v);
    assert_eq!((img.width, img.height), (32, 20));
    // Default is 512, and a small frame is never enlarged.
    let v = run(&mut s, json!({})).unwrap();
    assert_eq!(
        (v["width"].as_u64(), v["height"].as_u64()),
        (Some(64), Some(40))
    );
}

#[test]
fn deterministic_and_read_only() {
    let dir = Dir::new("det");
    let mut s = session(&dir);
    let rev = s.revision();
    let a = run(&mut s, json!({"frame": 2, "max_size": 40})).unwrap();
    let b = run(&mut s, json!({"frame": 2, "max_size": 40})).unwrap();
    assert_eq!(a, b);
    assert_eq!(s.revision(), rev);
    assert!(!s.can_undo());
}

#[test]
fn bad_requests_are_errors() {
    let dir = Dir::new("bad");
    let mut s = session(&dir);
    for params in [
        json!({"t": 0.5, "frame": 1}),
        json!({"movie_frame": 1, "t": 0.5}),
        json!({"movie_frame": 1, "sequence": 0}),
        json!({"max_size": 2}),
        json!({"max_size": 100000}),
        json!({"sequence": 9}),
    ] {
        assert!(run(&mut s, params.clone()).is_err(), "{params}");
    }
    let ok = run(&mut s, json!({"movie_frame": 1, "max_size": 16})).unwrap();
    assert_eq!(ok["width"], 16);
    let mut empty = Session::new();
    assert!(matches!(
        run(&mut empty, json!({})),
        Err(CommandError::Disabled { .. })
    ));
}
