//! Automatic face placement through the command registry, on public-domain portraits.
#![cfg(feature = "face-detect")]

use inbetween_engine::{CommandError, Registry, Session};
use inbetween_geom::Point;
use inbetween_raster::Image;
use serde_json::{json, Value};
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../detect/tests/data")
        .join(name)
}

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("inbetween-detect-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn s(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }

    fn write(&self, name: &str, img: &Image) -> String {
        let png = inbetween_codecs::encode_png(img.width(), img.height(), &img.to_rgba8()).unwrap();
        std::fs::write(self.0.join(name), png).unwrap();
        self.s(name)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn load(name: &str) -> Image {
    let bytes = std::fs::read(fixture(name)).unwrap();
    let d = inbetween_codecs::decode(&bytes).unwrap();
    Image::from_rgba8(d.width, d.height, &d.data).unwrap()
}

fn try_run(s: &mut Session, id: &str, p: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, id, &p)
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    try_run(s, id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn portraits() -> Session {
    let mut s = Session::new();
    run(
        &mut s,
        "project.new",
        json!({
            "image1": fixture("portrait_a.png").display().to_string(),
            "image2": fixture("portrait_b.png").display().to_string(),
        }),
    );
    s
}

fn count(s: &Session) -> usize {
    s.project().sequences[0].dots.len()
}

#[test]
fn locate_detects_each_image_separately() {
    let mut s = portraits();
    let before = s.project().clone();
    let out = run(&mut s, "face.locate", json!({}));
    assert_eq!(out["source"], "detect");
    assert_eq!(count(&s), 38);
    let dots = &s.project().sequences[0].dots;
    let differing = dots
        .pairs
        .iter()
        .filter(|p| p.first.distance(p.second) > 1.0)
        .count();
    assert!(differing > 30, "ends should differ per image: {differing}");
    // Subject's right eye is at image left on both images.
    let names: Vec<_> = s
        .project()
        .face
        .dots
        .iter()
        .map(|d| d.name.clone())
        .collect();
    let at = |name: &str| names.iter().position(|n| n == name).unwrap();
    let (re, le) = (at("pupil_right"), at("pupil_left"));
    for which in [1u8, 2] {
        assert!(dots.pairs[re].end(which).x < dots.pairs[le].end(which).x);
    }
    // Undo restores the project.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(*s.project(), before);
}

#[test]
fn detect_reports_faces_with_landmarks_and_optional_mesh() {
    let mut s = portraits();
    let out = run(&mut s, "face.detect", json!({}));
    let images = out["images"].as_array().unwrap();
    assert_eq!(images.len(), 2);
    for img in images {
        let faces = img["faces"].as_array().unwrap();
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0]["landmarks"].as_array().unwrap().len(), 38);
        assert!(faces[0]["score"].as_f64().unwrap() >= 0.5);
        assert!(faces[0].get("mesh").is_none());
    }
    let one = run(&mut s, "face.detect", json!({"image": 2, "mesh": true}));
    let faces = one["images"][0]["faces"].as_array().unwrap();
    assert_eq!(one["images"].as_array().unwrap().len(), 1);
    assert_eq!(faces[0]["mesh"].as_array().unwrap().len(), 468);
    // Read-only: no edit to undo.
    assert!(s.project().sequences[0].dots.is_empty());
}

#[test]
fn an_image_without_a_face_gives_a_clear_error_and_changes_nothing() {
    let d = Dir::new("noface");
    let mut blank = Image::new(200, 200).unwrap();
    for y in 0..200 {
        for x in 0..200 {
            let g = x as f32 / 200.0;
            blank.set(x, y, [g, 0.3, 1.0 - g, 1.0]);
        }
    }
    let flat = d.write("flat.png", &blank);
    let mut s = Session::new();
    run(
        &mut s,
        "project.new",
        json!({"image1": fixture("portrait_a.png").display().to_string(), "image2": flat}),
    );
    let before = s.project().clone();
    let err = try_run(&mut s, "face.locate", json!({})).unwrap_err();
    assert!(
        err.to_string()
            .contains("no face found in image 2 of sequence 0"),
        "{err}"
    );
    assert_eq!(*s.project(), before);
    // A box still works.
    run(&mut s, "face.locate", json!({"source": "box"}));
    assert_eq!(count(&s), 38);
    // A face number that does not exist is refused.
    let mut s = portraits();
    assert!(try_run(&mut s, "face.locate", json!({"face": 3})).is_err());
    assert!(s.project().sequences[0].dots.is_empty());
}

#[test]
fn all_faces_places_every_face_left_to_right() {
    let d = Dir::new("two");
    let (a, b) = (load("portrait_a.png"), load("portrait_b.png"));
    let mut both = Image::new(a.width() + b.width(), a.height()).unwrap();
    for (img, ox) in [(&a, 0), (&b, a.width())] {
        for y in 0..img.height() {
            for x in 0..img.width() {
                both.set(x + ox, y, img.get(x, y).unwrap());
            }
        }
    }
    let path = d.write("both.png", &both);
    let mut s = Session::new();
    run(
        &mut s,
        "project.new",
        json!({"image1": path, "image2": path}),
    );
    run(&mut s, "face.locate", json!({"all_faces": true}));
    assert_eq!(count(&s), 76);
    let dots = &s.project().sequences[0].dots;
    // The first run (portrait A) sits left of the second.
    let (x0, x1) = (dots.pairs[28].first.x, dots.pairs[38 + 28].first.x);
    assert!(x0 < x1);
    // Appending a face after the existing run.
    run(&mut s, "face.locate", json!({"mode": "replace"}));
    assert_eq!(count(&s), 38);
    run(&mut s, "face.locate", json!({"mode": "append", "face": 1}));
    assert_eq!(count(&s), 76);
}

#[test]
fn extract_detects_when_no_face_is_placed() {
    let d = Dir::new("extract");
    let mut s = portraits();
    let out = d.s("framed");
    run(&mut s, "face.extract", json!({"out_dir": out}));
    assert_eq!(count(&s), 38);
    assert!(std::path::Path::new(&out)
        .join("sequence1_image1.png")
        .exists());
    assert!(std::path::Path::new(&out)
        .join("sequence1_image2.png")
        .exists());
}

#[test]
fn a_mesh_file_places_the_model_like_detection_does() {
    let d = Dir::new("mesh");
    let mut s = portraits();
    let report = run(&mut s, "face.detect", json!({"image": 1, "mesh": true}));
    let mesh: Vec<Point> = report["images"][0]["faces"][0]["mesh"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| Point::new(p[0].as_f64().unwrap(), p[1].as_f64().unwrap()))
        .collect();
    let file = d.s("mesh.inbetweenlandmarks");
    inbetween_format::write_landmarks(std::path::Path::new(&file), &mesh).unwrap();
    run(
        &mut s,
        "face.locate",
        json!({"landmarks": file, "layout": "mesh478"}),
    );
    let placed = s.project().sequences[0].dots.first_positions();
    run(&mut s, "face.locate", json!({"image": 1}));
    let detected = s.project().sequences[0].dots.first_positions();
    assert_eq!(placed.len(), 38);
    for (p, q) in placed.iter().zip(&detected) {
        assert!(p.distance(*q) < 1e-6);
    }
    // A wrong-sized file is refused.
    let bad = d.s("bad.inbetweenlandmarks");
    inbetween_format::write_landmarks(std::path::Path::new(&bad), &mesh[..100]).unwrap();
    assert!(try_run(
        &mut s,
        "face.locate",
        json!({"landmarks": bad, "layout": "mesh478"})
    )
    .is_err());
}

#[test]
fn conflicting_sources_are_refused() {
    let mut s = portraits();
    for p in [
        json!({"source": "detect", "box_x": 1, "box_y": 1, "box_w": 5, "box_h": 5}),
        json!({"source": "box", "landmarks": "x.inbetweenlandmarks"}),
        json!({"source": "landmarks"}),
    ] {
        assert!(try_run(&mut s, "face.locate", p.clone()).is_err(), "{p}");
    }
    assert!(s.project().sequences[0].dots.is_empty());
}

#[test]
fn hostile_images_and_parameters_never_panic() {
    let d = Dir::new("hostile");
    let tiny = d.write(
        "tiny.png",
        &Image::filled(1, 1, [0.5, 0.5, 0.5, 1.0]).unwrap(),
    );
    let clear = d.write("clear.png", &Image::new(64, 48).unwrap());
    let sliver = d.write(
        "sliver.png",
        &Image::filled(3000, 2, [0.4, 0.4, 0.4, 1.0]).unwrap(),
    );
    let garbage = d.s("garbage.png");
    std::fs::write(&garbage, b"not an image").unwrap();
    for (a, b) in [(&tiny, &clear), (&sliver, &tiny), (&garbage, &clear)] {
        let mut s = Session::new();
        if try_run(&mut s, "project.new", json!({"image1": a, "image2": b})).is_err() {
            continue;
        }
        for (id, p) in [
            ("face.detect", json!({})),
            ("face.detect", json!({"mesh": true, "image": 1})),
            ("face.locate", json!({})),
            ("face.locate", json!({"all_faces": true, "face": 15})),
            (
                "face.locate",
                json!({"landmarks": garbage, "layout": "mesh478"}),
            ),
            ("face.extract", json!({"out_dir": d.s("out")})),
        ] {
            let before = s.project().clone();
            let r = Registry::standard().run_caught(&mut s, id, &p);
            if let Err(CommandError::Internal(m)) = &r {
                panic!("{id} {p}: internal error {m}");
            }
            if r.is_err() {
                assert_eq!(*s.project(), before, "{id} {p}");
            }
        }
    }
}

#[test]
fn locate_all_is_locate_with_all_faces() {
    let mut s = portraits();
    let out = run(&mut s, "face.locateAll", json!({}));
    assert_eq!(out["source"], "detect");
    assert_eq!(count(&s), 38);
    // The same face again would land on its own dots, which is refused and changes nothing.
    assert!(try_run(&mut s, "face.locateAll", json!({"mode": "append"})).is_err());
    assert_eq!(count(&s), 38);
}

fn flat_image(d: &Dir, name: &str) -> String {
    let mut img = Image::new(200, 200).unwrap();
    for y in 0..200 {
        for x in 0..200 {
            let g = x as f32 / 200.0;
            img.set(x, y, [g, 0.3, 1.0 - g, 1.0]);
        }
    }
    d.write(name, &img)
}

#[test]
fn all_sequences_skips_a_sequence_without_a_face_and_reports_it() {
    let d = Dir::new("skip");
    let mut s = portraits();
    // The second sequence continues from portrait_b to a picture with no face.
    run(
        &mut s,
        "sequence.add",
        json!({"image2": flat_image(&d, "flat.png")}),
    );
    let out = run(&mut s, "face.locateSequences", json!({}));
    assert_eq!(out["sequences"], json!([0]));
    assert_eq!(out["skipped"][0]["sequence"], 1);
    let reason = out["skipped"][0]["reason"].as_str().unwrap();
    assert!(
        reason.contains("no face found in image 2 of sequence 1"),
        "{reason}"
    );
    assert_eq!(s.project().sequences[0].dots.len(), 38);
    assert!(s.project().sequences[1].dots.is_empty());
    // One undo step for the whole run.
    run(&mut s, "edit.undo", json!({}));
    assert!(s.project().sequences[0].dots.is_empty());
}

#[test]
fn all_sequences_is_an_error_only_when_every_sequence_fails() {
    let d = Dir::new("allfail");
    let mut s = Session::new();
    run(
        &mut s,
        "project.new",
        json!({"image1": flat_image(&d, "a.png"), "image2": flat_image(&d, "b.png")}),
    );
    run(
        &mut s,
        "sequence.add",
        json!({"image2": flat_image(&d, "c.png")}),
    );
    let before = s.project().clone();
    let err = try_run(&mut s, "face.locateSequences", json!({})).unwrap_err();
    assert!(err.to_string().contains("no face found"), "{err}");
    assert_eq!(*s.project(), before);
}

#[test]
fn locate_sequences_is_locate_with_all_sequences() {
    let mut a = portraits();
    let mut b = portraits();
    let x = run(&mut a, "face.locateSequences", json!({}));
    let y = run(&mut b, "face.locate", json!({"all_sequences": true}));
    assert_eq!(x["sequences"], y["sequences"]);
    assert_eq!(a.project().sequences[0].dots, b.project().sequences[0].dots);
}
