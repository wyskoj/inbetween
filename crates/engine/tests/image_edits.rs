//! `image.rotate`, `image.flip`, `image.crop`, `image.adjust`, `image.reset`, `image.swap`:
//! dots must stay on their features, and every edit must undo.

use inbetween_doc::{DotPair, DotSet, ImageRef, OutputSettings, Project, Sequence};
use inbetween_engine::{CommandError, Registry, Session};
use inbetween_geom::Point;
use inbetween_morph::fit_transform;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("inbetween-ie-{tag}-{}", std::process::id()));
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

const W: u32 = 64;
const H: u32 = 48;

/// A smooth background with a magenta 7 x 7 marker whose centre is at `(mx, my)`, and a cyan
/// one at `(cx, cy)`.
fn write_png(path: &Path, marker: (u32, u32), other: (u32, u32)) {
    let mut rgba = Vec::new();
    for y in 0..H {
        for x in 0..W {
            let near = |c: (u32, u32)| x.abs_diff(c.0) <= 3 && y.abs_diff(c.1) <= 3;
            let px = if near(marker) {
                [255, 0, 255, 255]
            } else if near(other) {
                [0, 255, 255, 255]
            } else {
                [(x * 3) as u8 / 2, (y * 4) as u8 / 2, 90, 255]
            };
            rgba.extend_from_slice(&px);
        }
    }
    std::fs::write(path, inbetween_codecs::encode_png(W, H, &rgba).unwrap()).unwrap();
}

const MARK1: (u32, u32) = (20, 14);
const MARK2: (u32, u32) = (45, 33);

fn session(dir: &Dir) -> Session {
    write_png(&dir.0.join("a.png"), MARK1, MARK2);
    write_png(&dir.0.join("b.png"), MARK2, MARK1);
    let mut s = Session::new();
    s.set_project(
        Project {
            output: OutputSettings {
                width: W,
                height: H,
            },
            sequences: vec![Sequence {
                image1: ImageRef::new(dir.s("a.png")),
                image2: ImageRef::new(dir.s("b.png")),
                // Dot 0 marks the magenta marker on Image 1 and the cyan one on Image 2.
                dots: DotSet::new(vec![
                    DotPair::new(
                        Point::new(f64::from(MARK1.0) + 0.5, f64::from(MARK1.1) + 0.5),
                        Point::new(f64::from(MARK1.0) + 0.5, f64::from(MARK1.1) + 0.5),
                    ),
                    DotPair::new(
                        Point::new(f64::from(MARK2.0) + 0.5, f64::from(MARK2.1) + 0.5),
                        Point::new(f64::from(MARK2.0) + 0.5, f64::from(MARK2.1) + 0.5),
                    ),
                ]),
                frames: 5,
                ..Sequence::default()
            }],
            ..Project::default()
        },
        None,
    );
    s
}

fn run(s: &mut Session, id: &str, params: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, id, &params)
}

/// The colour of the processed image under dot `dot` of Image `which`.
fn under(s: &mut Session, dot: usize, which: u8) -> [f32; 4] {
    let image = s.source(0, which).unwrap();
    let seq = s.sequence(0).unwrap();
    let out = s.project().output;
    let fit = fit_transform(
        image.width(),
        image.height(),
        out.width,
        out.height,
        seq.fit,
    )
    .unwrap();
    let pair = seq.dots.pairs[dot];
    let p = fit.apply(if which == 1 { pair.first } else { pair.second });
    image.sample_bilinear(p.x as f32, p.y as f32)
}

fn is_magenta(p: [f32; 4]) -> bool {
    p[0] > 0.9 && p[1] < 0.1 && p[2] > 0.9
}

fn is_cyan(p: [f32; 4]) -> bool {
    p[0] < 0.1 && p[1] > 0.9 && p[2] > 0.9
}

/// Both of Image 1's dots must sit on their markers (dot 0 on magenta, dot 1 on cyan).
fn on_features(s: &mut Session, step: &str) {
    let (a, b) = (under(s, 0, 1), under(s, 1, 1));
    assert!(is_magenta(a), "{step}: dot 0 is on {a:?}");
    assert!(is_cyan(b), "{step}: dot 1 is on {b:?}");
}

#[test]
fn dots_stay_on_their_features_through_every_geometry_edit() {
    let d = Dir::new("features");
    let mut s = session(&d);
    on_features(&mut s, "start");
    let steps: Vec<(&str, &str, Value)> = vec![
        ("flip h", "image.flip", json!({"axis": "horizontal"})),
        ("flip v", "image.flip", json!({"axis": "vertical"})),
        ("turn cw", "image.rotate", json!({"turn": "cw"})),
        ("turn ccw", "image.rotate", json!({"turn": "ccw"})),
        ("turn 180", "image.rotate", json!({"turn": "180"})),
        ("turn cw again", "image.rotate", json!({"turn": "cw"})),
        // A crop that keeps both markers (in the turned image the size is 48 x 64).
        (
            "crop",
            "image.crop",
            json!({"x": 2, "y": 3, "w": 40, "h": 55}),
        ),
        ("crop angle", "image.crop", json!({"angle": 17})),
        (
            "flip with crop",
            "image.flip",
            json!({"axis": "horizontal"}),
        ),
        ("turn with crop", "image.rotate", json!({"turn": "ccw"})),
        // A crop that extends well past the image on every side.
        (
            "crop past",
            "image.crop",
            json!({"x": -20, "y": -15, "w": 120, "h": 100, "angle": -33}),
        ),
        ("rotate angle", "image.rotate", json!({"angle": 71})),
        ("expand", "image.rotate", json!({"expand": true})),
        ("clear", "image.crop", json!({"clear": true})),
        ("reset geometry", "image.reset", json!({"part": "geometry"})),
    ];
    for (name, id, params) in steps {
        run(&mut s, id, params).unwrap_or_else(|e| panic!("{name}: {e}"));
        on_features(&mut s, name);
    }
    // Everything undone leaves no edit at all.
    assert!(s.sequence(0).unwrap().edit1.is_identity());
}

#[test]
fn image_two_edits_move_the_second_side_only() {
    let d = Dir::new("second");
    let mut s = session(&d);
    let before: Vec<Point> = s.sequence(0).unwrap().dots.first_positions();
    run(
        &mut s,
        "image.flip",
        json!({"axis": "horizontal", "image": 2}),
    )
    .unwrap();
    run(&mut s, "image.rotate", json!({"turn": "cw", "image": 2})).unwrap();
    assert_eq!(s.sequence(0).unwrap().dots.first_positions(), before);
    // On Image 2, dot 0 sits on the cyan marker and dot 1 on the magenta one.
    assert!(is_cyan(under(&mut s, 0, 2)));
    assert!(is_magenta(under(&mut s, 1, 2)));
}

#[test]
fn cw_then_ccw_leaves_the_dots_where_they_were() {
    let d = Dir::new("roundtrip");
    let mut s = session(&d);
    let before = s.sequence(0).unwrap().dots.clone();
    run(&mut s, "image.rotate", json!({"turn": "cw"})).unwrap();
    assert_ne!(s.sequence(0).unwrap().dots, before);
    run(&mut s, "image.rotate", json!({"turn": "ccw"})).unwrap();
    let after = &s.sequence(0).unwrap().dots;
    for (a, b) in before.pairs.iter().zip(&after.pairs) {
        assert!((a.first.x - b.first.x).abs() < 1e-9 && (a.first.y - b.first.y).abs() < 1e-9);
        assert_eq!(a.second, b.second, "Image 2 is untouched");
    }
    assert!(s.sequence(0).unwrap().edit1.is_identity());
}

#[test]
fn turning_a_crop_keeps_it_on_the_same_content() {
    let d = Dir::new("cropturn");
    let mut s = session(&d);
    run(
        &mut s,
        "image.crop",
        json!({"x": 10, "y": 5, "w": 30, "h": 20}),
    )
    .unwrap();
    let before = s.source(0, 1).unwrap();
    run(&mut s, "image.rotate", json!({"turn": "cw"})).unwrap();
    let turned = s.source(0, 1).unwrap();
    assert_eq!(turned.size(), (20, 30));
    let want = inbetween_raster::transform::rotate_quarter(&before, 1).unwrap();
    assert_eq!(*turned, want);
    run(&mut s, "image.flip", json!({"axis": "vertical"})).unwrap();
    let flipped = s.source(0, 1).unwrap();
    assert_eq!(
        *flipped,
        inbetween_raster::transform::flip(&want, false, true).unwrap()
    );
}

#[test]
fn crop_aspect_snaps_the_height() {
    let d = Dir::new("aspect");
    let mut s = session(&d);
    let r = run(
        &mut s,
        "image.crop",
        json!({"x": 0, "y": 0, "w": 32, "aspect": "4:3"}),
    )
    .unwrap();
    assert_eq!(r["size"], json!([32, 24]));
    let r = run(&mut s, "image.crop", json!({"h": 30, "aspect": "1:1"})).unwrap();
    assert_eq!(r["size"], json!([30, 30]));
    let r = run(&mut s, "image.crop", json!({"w": 40, "aspect": "output"})).unwrap();
    assert_eq!(r["size"], json!([40, 30]));
    assert!(run(&mut s, "image.crop", json!({"aspect": "wide"})).is_err());
    assert!(run(&mut s, "image.crop", json!({"aspect": "0:3"})).is_err());
    // Cropping to everything again is no crop.
    run(
        &mut s,
        "image.crop",
        json!({"x": 0, "y": 0, "w": 64, "h": 48}),
    )
    .unwrap();
    assert!(s.sequence(0).unwrap().edit1.is_identity());
}

#[test]
fn adjust_merges_and_resets() {
    let d = Dir::new("adjust");
    let mut s = session(&d);
    run(&mut s, "image.adjust", json!({"brightness": 20})).unwrap();
    run(
        &mut s,
        "image.adjust",
        json!({"contrast": -10, "negative": true}),
    )
    .unwrap();
    let a = s.sequence(0).unwrap().edit1.adjust;
    assert_eq!((a.brightness, a.contrast, a.negative), (20.0, -10.0, true));
    // Adjusting does not move dots or touch the other image.
    assert!(s.sequence(0).unwrap().edit2.is_identity());
    let r = run(&mut s, "image.adjust", json!({})).unwrap();
    assert_eq!(r["dots_moved"], false);
    assert_eq!(r["edit"]["adjust"]["brightness"], 20.0);
    run(&mut s, "image.adjust", json!({"reset": true, "hue": 30})).unwrap();
    let a = s.sequence(0).unwrap().edit1.adjust;
    assert_eq!((a.brightness, a.hue, a.negative), (0.0, 30.0, false));
    assert!(run(&mut s, "image.adjust", json!({"brightness": 500})).is_err());
    run(&mut s, "image.reset", json!({"part": "adjust"})).unwrap();
    assert!(s.sequence(0).unwrap().edit1.is_identity());
}

#[test]
fn swap_swaps_images_edits_and_dots_and_twice_is_identity() {
    let d = Dir::new("swap");
    let mut s = session(&d);
    run(&mut s, "image.adjust", json!({"brightness": 10})).unwrap();
    run(
        &mut s,
        "image.flip",
        json!({"axis": "vertical", "image": 2}),
    )
    .unwrap();
    let before = s.sequence(0).unwrap().clone();
    run(&mut s, "image.swap", json!({})).unwrap();
    let swapped = s.sequence(0).unwrap().clone();
    assert_eq!(swapped.image1, before.image2);
    assert_eq!(swapped.image2, before.image1);
    assert_eq!(swapped.edit1, before.edit2);
    assert_eq!(swapped.edit2, before.edit1);
    assert_eq!(swapped.dots.pairs[0].first, before.dots.pairs[0].second);
    assert_eq!(swapped.dots.pairs[1].second, before.dots.pairs[1].first);
    run(&mut s, "image.swap", json!({})).unwrap();
    assert_eq!(*s.sequence(0).unwrap(), before);
    assert_eq!(
        Registry::standard().get("image.swap").unwrap().shortcut,
        Some("F3")
    );
}

#[test]
fn undo_restores_pixels_and_dots() {
    let d = Dir::new("undo");
    let mut s = session(&d);
    let original = s.source(0, 1).unwrap();
    let dots = s.sequence(0).unwrap().dots.clone();
    run(&mut s, "image.rotate", json!({"turn": "cw"})).unwrap();
    run(
        &mut s,
        "image.crop",
        json!({"x": 1, "y": 2, "w": 44, "h": 60, "angle": 9}),
    )
    .unwrap();
    on_features(&mut s, "edited");
    let edited = s.source(0, 1).unwrap();
    run(&mut s, "edit.undo", json!({})).unwrap();
    run(&mut s, "edit.undo", json!({})).unwrap();
    assert!(std::sync::Arc::ptr_eq(&original, &s.source(0, 1).unwrap()));
    assert_eq!(s.sequence(0).unwrap().dots, dots);
    run(&mut s, "edit.redo", json!({})).unwrap();
    run(&mut s, "edit.redo", json!({})).unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&edited, &s.source(0, 1).unwrap()),
        "redo reuses the cached edited source"
    );
    on_features(&mut s, "redone");
}

#[test]
fn keep_dots_leaves_them_alone() {
    let d = Dir::new("keep");
    let mut s = session(&d);
    let before = s.sequence(0).unwrap().dots.clone();
    let r = run(
        &mut s,
        "image.flip",
        json!({"axis": "horizontal", "keep_dots": true}),
    )
    .unwrap();
    assert_eq!(r["dots_moved"], false);
    assert_eq!(s.sequence(0).unwrap().dots, before);
}

#[test]
fn a_missing_source_refuses_geometry_and_changes_nothing() {
    let d = Dir::new("missing");
    let mut s = session(&d);
    std::fs::remove_file(d.0.join("a.png")).unwrap();
    let before = s.sequence(0).unwrap().clone();
    let err = run(&mut s, "image.flip", json!({"axis": "horizontal"}))
        .unwrap_err()
        .to_string();
    assert!(err.contains("Image 1"), "{err}");
    assert_eq!(*s.sequence(0).unwrap(), before);
    // Colour edits need no pixels, so they still work.
    run(&mut s, "image.adjust", json!({"brightness": 5})).unwrap();
}

#[test]
fn bad_requests_are_refused() {
    let d = Dir::new("bad");
    let mut s = session(&d);
    let before = s.sequence(0).unwrap().clone();
    for (id, params) in [
        ("image.rotate", json!({"turn": "sideways"})),
        ("image.rotate", json!({"angle": 1000})),
        ("image.flip", json!({})),
        ("image.flip", json!({"axis": "diagonal"})),
        ("image.crop", json!({"w": 0})),
        ("image.crop", json!({"w": 1e9, "h": 1e9})),
        ("image.crop", json!({"x": 1e12})),
        ("image.reset", json!({"part": "nonsense"})),
        ("image.rotate", json!({"sequence": 9, "turn": "cw"})),
        ("image.rotate", json!({"image": 3})),
    ] {
        assert!(run(&mut s, id, params.clone()).is_err(), "{id} {params}");
    }
    assert_eq!(*s.sequence(0).unwrap(), before);
}

#[test]
fn continued_sequences_carry_the_edit_with_the_image() {
    let d = Dir::new("carry");
    let mut s = session(&d);
    run(
        &mut s,
        "image.adjust",
        json!({"image": 2, "saturation": -40}),
    )
    .unwrap();
    write_png(&d.0.join("c.png"), MARK1, MARK2);
    run(&mut s, "sequence.add", json!({"image2": d.s("c.png")})).unwrap();
    let seqs = &s.project().sequences;
    assert_eq!(seqs[1].edit1, seqs[0].edit2);
    assert!(!seqs[1].edit1.is_identity());
    assert!(seqs[1].edit2.is_identity());
}
