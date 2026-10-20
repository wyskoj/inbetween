//! `dots.autoLandmarks`: dot pairs for subjects that are not faces.

use inbetween_engine::{CommandError, Registry, Session};
use inbetween_raster::Image;
use serde_json::{json, Value};
use std::path::PathBuf;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let d = std::env::temp_dir().join(format!("inbetween-land-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Self(d)
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

const W: u32 = 192;
const H: u32 = 144;
const SHIFT: (i64, i64) = (5, -3);

fn run(s: &mut Session, id: &str, p: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, id, &p)
}

fn write(path: &str, img: &Image) {
    std::fs::write(
        path,
        inbetween_codecs::encode_png(img.width(), img.height(), &img.to_rgba8()).unwrap(),
    )
    .unwrap();
}

/// A textured scene and the same scene moved by SHIFT.
fn project(d: &Dir) -> Session {
    let a = inbetween_testkit::labelled_grid(W, H, 12).unwrap();
    let mut b = Image::new(W, H).unwrap();
    for y in 0..H {
        for x in 0..W {
            b.set(
                x,
                y,
                a.get_clamped(i64::from(x) - SHIFT.0, i64::from(y) - SHIFT.1),
            );
        }
    }
    write(&d.s("a.png"), &a);
    write(&d.s("b.png"), &b);
    let mut s = Session::new();
    run(
        &mut s,
        "tools.sequencePacker",
        json!({"images": [d.s("a.png"), d.s("b.png")], "frames": 3}),
    )
    .unwrap();
    s
}

fn pairs(s: &Session) -> Vec<((f64, f64), (f64, f64))> {
    s.project().sequences[0]
        .dots
        .pairs
        .iter()
        .map(|p| ((p.first.x, p.first.y), (p.second.x, p.second.y)))
        .collect()
}

#[test]
fn pairs_follow_the_real_shift_and_respect_the_options() {
    let d = Dir::new("a");
    let mut s = project(&d);
    let r = run(
        &mut s,
        "dots.autoLandmarks",
        json!({"count": 30, "quality": 0.02}),
    )
    .unwrap();
    let added = r["added"].as_u64().unwrap() as usize;
    assert!((5..=30).contains(&added), "{r}");
    let list = pairs(&s);
    assert_eq!(list.len(), added);
    for (a, b) in &list {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        assert!(
            (dx - SHIFT.0 as f64).abs() <= 2.0 && (dy - SHIFT.1 as f64).abs() <= 2.0,
            "pair {a:?} -> {b:?} is not the shift"
        );
    }
    // The spacing holds.
    let spacing = 0.04 * f64::from(H);
    for (i, (a, _)) in list.iter().enumerate() {
        for (b, _) in &list[i + 1..] {
            assert!((a.0 - b.0).hypot(a.1 - b.1) >= spacing - 1.0, "{a:?} {b:?}");
        }
    }
    // The sequence still renders, and the ends are still the images.
    let p = s.prepare(0).unwrap();
    assert!(p.renderer().unwrap().render_frame(1).is_ok());
    // Another run adds more without touching what is there, and leaves a gap from old dots.
    let before = list.len();
    run(
        &mut s,
        "dots.autoLandmarks",
        json!({"count": 10, "min_distance": 30}),
    )
    .unwrap();
    assert!(pairs(&s).len() >= before);
    assert_eq!(&pairs(&s)[..before], &list[..]);
    // replace starts over; border adds eight fixed dots.
    let r = run(
        &mut s,
        "dots.autoLandmarks",
        json!({"count": 6, "replace": true, "border": true}),
    )
    .unwrap();
    let list = pairs(&s);
    assert_eq!(list.len() as u64, r["total"].as_u64().unwrap());
    let fixed = list.iter().filter(|(a, b)| a == b).count();
    assert_eq!(fixed, 8);
    assert!(list.iter().any(|(a, _)| *a == (0.0, 0.0)));
    // One undo takes the whole thing back.
    run(&mut s, "edit.undo", json!({})).unwrap();
    assert!(pairs(&s).len() > 8);
}

#[test]
fn flat_pictures_and_bad_requests_add_nothing() {
    let d = Dir::new("b");
    let flat = Image::filled(64, 48, [0.4, 0.4, 0.4, 1.0]).unwrap();
    write(&d.s("f1.png"), &flat);
    write(&d.s("f2.png"), &flat);
    let mut s = Session::new();
    run(
        &mut s,
        "tools.sequencePacker",
        json!({"images": [d.s("f1.png"), d.s("f2.png")]}),
    )
    .unwrap();
    let r = run(&mut s, "dots.autoLandmarks", json!({})).unwrap();
    assert_eq!(r["added"], 0);
    assert!(pairs(&s).is_empty());
    assert!(run(&mut s, "dots.autoLandmarks", json!({"sequence": 5})).is_err());
    assert!(run(&mut s, "dots.autoLandmarks", json!({"count": 0})).is_err());
    assert!(run(&mut s, "dots.autoLandmarks", json!({"quality": 2})).is_err());
    let mut empty = Session::new();
    assert!(run(&mut empty, "dots.autoLandmarks", json!({})).is_err());
}
