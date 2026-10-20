//! `tools.sequencePacker` and `tools.benchmark`.

use inbetween_engine::{CommandError, Registry, Session};
use serde_json::{json, Value};
use std::path::PathBuf;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let d = std::env::temp_dir().join(format!("inbetween-tools-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Self(d)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn png(dir: &Dir, name: &str, w: u32, h: u32, shade: u8) {
    let rgba: Vec<u8> = (0..w * h).flat_map(|_| [shade, 100, 200, 255]).collect();
    std::fs::write(
        dir.0.join(name),
        inbetween_codecs::encode_png(w, h, &rgba).unwrap(),
    )
    .unwrap();
}

fn run(s: &mut Session, id: &str, p: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, id, &p)
}

fn names(s: &Session) -> Vec<String> {
    s.project()
        .sequences
        .iter()
        .map(|q| q.name.clone())
        .collect()
}

#[test]
fn a_folder_is_packed_in_natural_order_skipping_what_is_not_an_image() {
    let d = Dir::new("pack");
    png(&d, "img10.png", 40, 30, 10);
    png(&d, "img2.png", 40, 30, 20);
    png(&d, "img1.png", 40, 30, 30);
    std::fs::write(d.0.join("broken.png"), b"not a picture").unwrap();
    std::fs::write(d.0.join("notes.txt"), b"hello").unwrap();
    let mut s = Session::new();
    let r = run(
        &mut s,
        "tools.sequencePacker",
        json!({"folder": d.0.display().to_string(), "frames": 5}),
    )
    .unwrap();
    assert_eq!(names(&s), ["img1 to img2", "img2 to img10"]);
    assert_eq!(r["added"], 2);
    assert_eq!(r["skipped"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        (r["width"].as_u64(), r["height"].as_u64()),
        (Some(40), Some(30))
    );
    assert!(s.project().sequences.iter().all(|q| q.frames == 5));
    for i in 0..2 {
        assert!(s.prepare(i).is_ok());
    }
    // A second run appends, loops, and one undo takes it all back.
    let before = names(&s);
    run(
        &mut s,
        "tools.sequencePacker",
        json!({"images": [d.0.join("img1.png").display().to_string(), d.0.join("img2.png").display().to_string()], "loop": true}),
    )
    .unwrap();
    assert_eq!(names(&s).len(), before.len() + 2);
    run(&mut s, "edit.undo", json!({})).unwrap();
    assert_eq!(names(&s), before);
}

#[test]
fn order_by_name_sorts_the_whole_list_and_bad_input_is_refused() {
    let d = Dir::new("order");
    png(&d, "b.png", 8, 8, 1);
    png(&d, "a.png", 8, 8, 2);
    let (a, b) = (
        d.0.join("a.png").display().to_string(),
        d.0.join("b.png").display().to_string(),
    );
    let mut s = Session::new();
    run(
        &mut s,
        "tools.sequencePacker",
        json!({"images": [b, a], "order": "name"}),
    )
    .unwrap();
    assert_eq!(names(&s), ["a to b"]);
    let mut s = Session::new();
    assert!(run(&mut s, "tools.sequencePacker", json!({})).is_err());
    assert!(run(
        &mut s,
        "tools.sequencePacker",
        json!({"images": [a.clone()]})
    )
    .is_err());
    assert!(run(
        &mut s,
        "tools.sequencePacker",
        json!({"folder": d.0.join("none").display().to_string()})
    )
    .is_err());
    // One readable image is not enough, and a refusal leaves the project alone.
    assert!(run(
        &mut s,
        "tools.sequencePacker",
        json!({"images": [a, "missing.png"]})
    )
    .is_err());
    assert!(s.project().sequences.is_empty());
}

#[test]
fn benchmark_reports_timings_and_changes_nothing() {
    let d = Dir::new("bench");
    png(&d, "a.png", 64, 48, 5);
    png(&d, "b.png", 64, 48, 250);
    let mut s = Session::new();
    run(
        &mut s,
        "tools.sequencePacker",
        json!({"images": [d.0.join("a.png").display().to_string(), d.0.join("b.png").display().to_string()], "frames": 4}),
    )
    .unwrap();
    let project = s.project().clone();
    let r = run(&mut s, "tools.benchmark", json!({"frames": 6})).unwrap();
    assert_eq!(r["frames"], 6);
    assert_eq!(
        (r["width"].as_u64(), r["height"].as_u64()),
        (Some(64), Some(48))
    );
    assert!(r["preview"]["fps"].as_f64().unwrap() > 0.0);
    assert!(r["export"]["fps"].as_f64().unwrap() > 0.0);
    assert!(
        r["export"]["ms_per_frame"].as_f64().unwrap()
            >= r["preview"]["ms_per_frame"].as_f64().unwrap()
    );
    assert!(r["export"]["png_bytes_per_frame"].as_u64().unwrap() > 0);
    assert_eq!(*s.project(), project);
    assert!(run(&mut s, "tools.benchmark", json!({"sequence": 9})).is_err());
    assert!(run(&mut s, "tools.benchmark", json!({"frames": 0})).is_err());
}
