//! Key-dot foundation: unpaired dots and lines never change a frame, 2048 pairs work end to
//! end, and the selection follows edits.

use inbetween_doc::{DotPair, Side};
use inbetween_engine::{Registry, Session};
use inbetween_geom::Point;
use inbetween_testkit::demo;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("inbetween-kd-{tag}-{}", std::process::id()));
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

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    Registry::standard()
        .run(s, id, &p)
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn open(d: &Dir) -> Session {
    let mut s = Session::new();
    run(
        &mut s,
        "project.new",
        json!({"image1": d.s("sun.png"), "image2": d.s("moon.png")}),
    );
    s
}

fn frame_bytes(s: &mut Session, d: &Dir, name: &str, t: f64) -> Vec<u8> {
    run(s, "export.frame", json!({"out": d.s(name), "t": t}));
    std::fs::read(d.0.join(name)).unwrap()
}

#[test]
fn unpaired_dots_and_lines_never_change_a_frame() {
    let d = Dir::new("unpaired");
    let mut s = open(&d);
    for (x, y, x2, y2) in [(2, 2, 3, 4), (20, 3, 18, 5), (11, 15, 12, 13)] {
        run(
            &mut s,
            "dots.add",
            json!({"x": x, "y": y, "x2": x2, "y2": y2}),
        );
    }
    let before: Vec<Vec<u8>> = [0.0, 0.5, 1.0]
        .iter()
        .enumerate()
        .map(|(i, t)| frame_bytes(&mut s, &d, &format!("a{i}.png"), *t))
        .collect();

    let mut project = s.project().clone();
    let dots = &mut project.sequences[0].dots;
    dots.pairs
        .push(DotPair::unpaired(Point::new(7.0, 7.0), Side::Only1));
    dots.pairs
        .push(DotPair::unpaired(Point::new(9.0, 8.0), Side::Only2));
    assert!(dots.connect(0, 3) && dots.connect(3, 4) && dots.connect(1, 2));
    s.set_project(project, None);

    for (i, t) in [0.0, 0.5, 1.0].iter().enumerate() {
        let after = frame_bytes(&mut s, &d, &format!("b{i}.png"), *t);
        assert_eq!(before[i], after, "frame at t={t} changed");
    }
}

#[test]
fn deleting_a_dot_drops_and_renumbers_its_lines() {
    let d = Dir::new("lines");
    let mut s = open(&d);
    for x in [2, 6, 10] {
        run(&mut s, "dots.add", json!({"x": x, "y": 5}));
    }
    let mut project = s.project().clone();
    let dots = &mut project.sequences[0].dots;
    assert!(dots.connect(0, 1) && dots.connect(1, 2));
    s.set_project(project, None);
    run(&mut s, "dots.delete", json!({"index": 0}));
    let dots = &s.project().sequences[0].dots;
    assert_eq!(dots.len(), 2);
    assert_eq!(dots.lines.len(), 1);
    assert!(dots.validate_lines().is_ok());
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.project().sequences[0].dots.lines.len(), 2);
}

#[test]
fn selection_is_clamped_and_cleared_when_dots_are_renumbered() {
    let d = Dir::new("selection");
    let mut s = open(&d);
    for x in 0..4 {
        run(&mut s, "dots.add", json!({"x": 2 * x, "y": 5}));
    }
    s.set_selection(0, BTreeSet::from([1, 3, 99]));
    assert_eq!(s.selection().iter().copied().collect::<Vec<_>>(), [1, 3]);
    // An edit that keeps the dot count keeps the selection.
    run(
        &mut s,
        "dots.move",
        json!({"index": 0, "image": 1, "x": 1, "y": 1}),
    );
    assert_eq!(s.selection().len(), 2);
    // Deleting renumbers, so the selection is cleared.
    run(&mut s, "dots.delete", json!({"index": 0}));
    assert!(s.selection().is_empty());
    s.set_selection(0, BTreeSet::from([0, 2]));
    run(&mut s, "edit.undo", json!({}));
    assert!(s.selection().is_empty());
    // A selection in a sequence that does not exist is dropped.
    s.set_selection(7, BTreeSet::from([0]));
    assert!(s.selection().is_empty());
}

#[test]
fn moving_a_missing_end_is_refused_and_the_present_end_follows() {
    let d = Dir::new("ends");
    let mut s = open(&d);
    run(&mut s, "dots.add", json!({"x": 4, "y": 4}));
    let mut project = s.project().clone();
    project.sequences[0].dots.pairs[0] = DotPair::unpaired(Point::new(4.0, 4.0), Side::Only1);
    s.set_project(project, None);
    let r = Registry::standard().run(
        &mut s,
        "dots.move",
        &json!({"index": 0, "image": 2, "x": 5, "y": 5}),
    );
    assert!(r.is_err());
    run(
        &mut s,
        "dots.move",
        json!({"index": 0, "image": 1, "x": 5, "y": 6}),
    );
    let p = s.project().sequences[0].dots.pairs[0];
    assert_eq!(
        (p.first, p.second),
        (Point::new(5.0, 6.0), Point::new(5.0, 6.0))
    );
}

#[test]
fn two_thousand_and_forty_eight_pairs_save_load_and_render() {
    let d = Dir::new("many");
    let mut s = open(&d);
    for i in 0..2048u32 {
        let (x, y) = (f64::from(i % 64) * 3.0, f64::from(i / 64) * 3.0);
        run(
            &mut s,
            "dots.add",
            json!({"x": x, "y": y, "x2": x + 0.25 * f64::from(i % 3), "y2": y}),
        );
    }
    assert_eq!(s.project().sequences[0].dots.len(), 2048);
    let file = d.s("many.inbetween");
    run(&mut s, "project.saveAs", json!({"path": file}));
    let mut loaded = Session::new();
    run(&mut loaded, "project.open", json!({"path": file}));
    assert_eq!(loaded.project().sequences[0].dots.len(), 2048);
    let start = Instant::now();
    let a = frame_bytes(&mut loaded, &d, "many.png", 0.5);
    let ms = start.elapsed().as_millis();
    eprintln!("2048 pairs: first frame in {ms} ms");
    assert!(!a.is_empty());
    assert_eq!(a, frame_bytes(&mut loaded, &d, "many2.png", 0.5));
}

#[test]
fn dots_wait_for_their_partner_and_auto_match_finds_it() {
    let root = std::env::temp_dir().join(format!("inbetween-kd-match-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let a = inbetween_testkit::labelled_grid(64, 48, 8).unwrap();
    let mut b = inbetween_raster::Image::new(64, 48).unwrap();
    for y in 0..48u32 {
        for x in 0..64u32 {
            b.set(x, y, a.get_clamped(i64::from(x) - 3, i64::from(y) - 2));
        }
    }
    for (name, img) in [("a.png", &a), ("b.png", &b)] {
        let png = inbetween_codecs::encode_png(64, 48, &img.to_rgba8()).unwrap();
        std::fs::write(root.join(name), png).unwrap();
    }
    let mut s = Session::new();
    run(
        &mut s,
        "project.new",
        json!({"image1": root.join("a.png").display().to_string(),
               "image2": root.join("b.png").display().to_string()}),
    );
    run(&mut s, "dots.option.autoPartner", json!({"on": false}));
    for (x, y) in [(0, 0), (63, 0), (63, 47), (0, 47)] {
        run(&mut s, "dots.add", json!({"x": x, "y": y, "image": 1}));
    }
    assert!(s.project().sequences[0]
        .dots
        .pairs
        .iter()
        .all(|p| !p.is_paired()));
    // Placing on image 2 completes the oldest waiting dots first.
    for (x, y) in [(0, 0), (63, 0), (63, 47), (0, 47)] {
        run(&mut s, "dots.add", json!({"x": x, "y": y, "image": 2}));
    }
    assert_eq!(s.project().sequences[0].dots.len(), 4);
    assert!(s.project().sequences[0]
        .dots
        .pairs
        .iter()
        .all(|p| p.is_paired()));
    run(&mut s, "dots.add", json!({"x": 30, "y": 24, "image": 1}));
    let v = run(&mut s, "dots.autoMatch", json!({}));
    assert_eq!(v["completed"], 1);
    let p = s.project().sequences[0].dots.pairs[4];
    assert!(p.is_paired());
    assert!(
        p.second.distance(Point::new(33.0, 26.0)) <= 1.0,
        "{:?}",
        p.second
    );
    run(&mut s, "edit.undo", json!({}));
    assert!(!s.project().sequences[0].dots.pairs[4].is_paired());
    let _ = std::fs::remove_dir_all(&root);
}
