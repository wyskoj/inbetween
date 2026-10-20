//! The face tools through the command registry: the model, placing a face, smart drag,
//! framing, mixing and the library.

use inbetween_doc::FaceModel;
use inbetween_engine::{CommandError, Registry, Session};
use inbetween_geom::Point;
use inbetween_morph::face::{self, FaceBox};
use inbetween_testkit::demo;
use serde_json::{json, Value};
use std::path::PathBuf;

const W: u32 = 120;
const H: u32 = 90;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("inbetween-face-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (sun, moon) = demo::pair(W, H).unwrap();
        for (name, img) in [("sun.png", sun), ("moon.png", moon)] {
            let png = inbetween_codecs::encode_png(W, H, &img.to_rgba8()).unwrap();
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

fn try_run(s: &mut Session, id: &str, p: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, id, &p)
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    try_run(s, id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn session(d: &Dir) -> Session {
    let mut s = Session::new();
    run(
        &mut s,
        "project.new",
        json!({"image1": d.s("sun.png"), "image2": d.s("moon.png")}),
    );
    run(&mut s, "sequence.length", json!({"frames": 5}));
    s
}

fn first_positions(s: &Session, seq: usize) -> Vec<Point> {
    s.project().sequences[seq].dots.first_positions()
}

fn second_positions(s: &Session, seq: usize) -> Vec<Point> {
    s.project().sequences[seq].dots.second_positions()
}

fn close(a: &[Point], b: &[Point]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p.distance(*q) < 1e-6)
}

fn frame(s: &mut Session, sequence: usize, index: u32) -> Vec<u8> {
    let prepared = s.prepare(sequence).unwrap();
    prepared
        .renderer()
        .unwrap()
        .render_frame(index)
        .unwrap()
        .to_rgba8()
}

fn locate(s: &mut Session, image: Option<u8>, b: (f64, f64, f64, f64)) -> Value {
    let mut p = json!({"box_x": b.0, "box_y": b.1, "box_w": b.2, "box_h": b.3});
    if let Some(i) = image {
        p["image"] = json!(i);
    }
    run(s, "face.locate", p)
}

#[test]
fn model_edits_report_save_load_and_undo() {
    let d = Dir::new("model");
    let mut s = session(&d);
    let v = run(&mut s, "face.model", json!({}));
    assert_eq!(v["landmarks"], 38);
    assert_eq!(v["enabled"], 38);

    let v = run(
        &mut s,
        "face.model",
        json!({"op": "preset", "preset": "minimal"}),
    );
    let minimal = v["enabled"].as_u64().unwrap();
    assert!(minimal < 15 && minimal > 5);

    run(
        &mut s,
        "face.model",
        json!({"op": "enable", "index": 0, "on": true}),
    );
    assert_eq!(s.project().face.active_count() as u64, minimal + 1);
    run(
        &mut s,
        "face.model",
        json!({"op": "important", "index": 0, "on": true}),
    );
    assert!(s.project().face.dots[0].important);
    run(
        &mut s,
        "face.model",
        json!({"op": "move", "index": 0, "x": 0.1, "y": 0.2}),
    );
    assert_eq!(s.project().face.dots[0].x, 0.1);
    run(
        &mut s,
        "face.model",
        json!({"op": "rename", "index": 0, "name": "left jaw"}),
    );
    run(
        &mut s,
        "face.model",
        json!({"op": "rename", "name": "mine"}),
    );
    assert_eq!(s.project().face.name, "mine");
    assert_eq!(s.project().face.dots[0].name, "left jaw");
    let guides = s.project().face.guides.len();
    run(
        &mut s,
        "face.model",
        json!({"op": "guide_add", "index": 0, "to": 4}),
    );
    assert_eq!(s.project().face.guides.len(), guides + 1);
    run(
        &mut s,
        "face.model",
        json!({"op": "guide_remove", "guide": 0}),
    );
    assert_eq!(s.project().face.guides.len(), guides);

    // Save, change, load: the saved model comes back.
    let file = d.s("mine.inbetweenface");
    let saved = s.project().face.clone();
    run(&mut s, "face.model", json!({"op": "save", "path": file}));
    run(
        &mut s,
        "face.model",
        json!({"op": "preset", "preset": "standard"}),
    );
    assert_ne!(s.project().face, saved);
    run(&mut s, "face.model", json!({"op": "load", "path": file}));
    assert_eq!(s.project().face, saved);

    // The model is part of the project: it survives save and open.
    let project = d.s("p.inbetween");
    run(&mut s, "project.save", json!({"path": project}));
    let mut t = Session::new();
    run(&mut t, "project.open", json!({"path": project}));
    assert_eq!(t.project().face, saved);

    // Every edit is an undo step: undoing the load returns the standard preset.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.project().face, FaceModel::standard());
}

#[test]
fn bad_model_edits_are_refused_and_change_nothing() {
    let d = Dir::new("badmodel");
    let mut s = session(&d);
    let before = s.project().face.clone();
    let revision = s.revision();
    for p in [
        json!({"op": "enable", "index": 99}),
        json!({"op": "enable", "on": true}),
        json!({"op": "enable", "index": 999, "on": true}),
        json!({"op": "move", "index": 0, "x": 1}),
        json!({"op": "move", "index": 0, "x": 1e9, "y": 0}),
        json!({"op": "preset"}),
        json!({"op": "preset", "preset": "nope"}),
        json!({"op": "rename"}),
        json!({"op": "rename", "index": 50, "name": "x"}),
        json!({"op": "guide_add", "index": 0}),
        json!({"op": "guide_add", "index": 0, "to": 0}),
        json!({"op": "guide_add", "index": 0, "to": 999}),
        json!({"op": "guide_remove", "guide": 999}),
        json!({"op": "save"}),
        json!({"op": "load", "path": d.s("missing.inbetweenface")}),
        json!({"op": "explode"}),
    ] {
        assert!(try_run(&mut s, "face.model", p.clone()).is_err(), "{p}");
    }
    // A file that is some other kind of Inbetween file is refused with a clear message.
    run(
        &mut s,
        "dots.export",
        json!({"path": d.s("set.inbetweendots")}),
    );
    let e = try_run(
        &mut s,
        "face.model",
        json!({"op": "load", "path": d.s("set.inbetweendots")}),
    )
    .unwrap_err();
    assert!(e.to_string().contains("inbetween-dots"), "{e}");
    assert_eq!(s.project().face, before);
    assert_eq!(s.revision(), revision);
}

#[test]
fn locate_places_the_model_on_a_box() {
    let d = Dir::new("locate");
    let mut s = session(&d);
    let model = s.project().face.clone();
    let b = FaceBox::new(30.0, 10.0, 50.0, 70.0).unwrap();
    let v = locate(&mut s, None, (30.0, 10.0, 50.0, 70.0));
    assert_eq!(v["dots_per_face"], 38);
    let expected = face::place(&model, &b);
    assert!(close(&first_positions(&s, 0), &expected));
    assert!(close(&second_positions(&s, 0), &expected));

    // Locating only Image 2 moves only its positions.
    let b2 = FaceBox::new(10.0, 5.0, 60.0, 80.0).unwrap();
    locate(&mut s, Some(2), (10.0, 5.0, 60.0, 80.0));
    assert!(close(&first_positions(&s, 0), &expected));
    assert!(close(&second_positions(&s, 0), &face::place(&model, &b2)));

    // The morph invariants still hold for a face-fitted pair: the ends are the sources.
    let first = frame(&mut s, 0, 0);
    let last = frame(&mut s, 0, 4);
    let (sun, moon) = demo::pair(W, H).unwrap();
    let close_enough = |a: &[u8], b: &[u8]| a.iter().zip(b).all(|(x, y)| x.abs_diff(*y) <= 2);
    assert!(close_enough(&first, &sun.to_rgba8()));
    assert!(close_enough(&last, &moon.to_rgba8()));

    // Appending a second face keeps the first and adds another 38 dots.
    run(
        &mut s,
        "face.locate",
        json!({"mode": "append", "box_x": 85, "box_y": 20, "box_w": 30, "box_h": 40}),
    );
    assert_eq!(s.project().sequences[0].dots.len(), 76);
    assert!(close(&first_positions(&s, 0)[..38], &expected));

    // Undo takes the appended face away again.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.project().sequences[0].dots.len(), 38);
}

#[test]
fn locate_defaults_to_a_centred_box_and_can_fill_every_sequence() {
    let d = Dir::new("locate-all");
    let mut s = session(&d);
    run(&mut s, "sequence.duplicate", json!({}));
    run(
        &mut s,
        "face.locate",
        json!({"all_sequences": true, "source": "box"}),
    );
    let centred = FaceBox::centered(W, H).unwrap();
    let expected = face::place(&s.project().face, &centred);
    for seq in 0..2 {
        assert!(close(&first_positions(&s, seq), &expected));
    }
    // One sequence only.
    run(
        &mut s,
        "face.model",
        json!({"op": "preset", "preset": "minimal"}),
    );
    run(
        &mut s,
        "face.locate",
        json!({"sequence": 1, "source": "box"}),
    );
    assert_eq!(
        s.project().sequences[1].dots.len(),
        s.project().face.active_count()
    );
    assert_eq!(s.project().sequences[0].dots.len(), 38);
}

#[test]
fn locate_reads_landmark_files_and_checks_them() {
    let d = Dir::new("landmarks");
    let mut s = session(&d);
    let model = s.project().face.clone();
    let pts = face::place(&model, &FaceBox::new(20.0, 10.0, 70.0, 75.0).unwrap());
    let file = d.s("face.inbetweenlandmarks");
    inbetween_format::write_landmarks(std::path::Path::new(&file), &pts).unwrap();
    run(&mut s, "face.locate", json!({"landmarks": file}));
    assert!(close(&first_positions(&s, 0), &pts));

    // Wrong count, both sources, a partial box, a wrong kind of file, a missing file.
    inbetween_format::write_landmarks(
        std::path::Path::new(&d.s("few.inbetweenlandmarks")),
        &pts[..10],
    )
    .unwrap();
    let before = s.project().clone();
    for p in [
        json!({"landmarks": d.s("few.inbetweenlandmarks")}),
        json!({"landmarks": file, "box_x": 1, "box_y": 1, "box_w": 5, "box_h": 5}),
        json!({"box_x": 1, "box_y": 1}),
        json!({"box_x": 1, "box_y": 1, "box_w": 0, "box_h": 5}),
        json!({"landmarks": d.s("sun.png")}),
        json!({"landmarks": d.s("missing.inbetweenlandmarks")}),
        json!({"sequence": 3}),
    ] {
        assert!(try_run(&mut s, "face.locate", p.clone()).is_err(), "{p}");
    }
    assert_eq!(*s.project(), before);

    // A model with every landmark off cannot place a face.
    for i in 0..38 {
        run(
            &mut s,
            "face.model",
            json!({"op": "enable", "index": i, "on": false}),
        );
    }
    assert!(try_run(&mut s, "face.locate", json!({})).is_err());
}

#[test]
fn locating_on_top_of_existing_dots_cannot_make_two_share_a_spot() {
    let d = Dir::new("clash");
    let mut s = session(&d);
    locate(&mut s, None, (30.0, 10.0, 50.0, 70.0));
    let before = s.project().clone();
    // The same face appended again would put every dot on top of an existing one.
    let e = try_run(
        &mut s,
        "face.locate",
        json!({"mode": "append", "box_x": 30, "box_y": 10, "box_w": 50, "box_h": 70}),
    )
    .unwrap_err();
    assert!(e.to_string().contains("share a spot"), "{e}");
    assert_eq!(*s.project(), before);
}

#[test]
fn smart_drag_pulls_the_neighbours_and_only_on_one_image() {
    let d = Dir::new("drag");
    let mut s = session(&d);
    locate(&mut s, None, (20.0, 5.0, 80.0, 80.0));
    let nose = s
        .project()
        .face
        .dots
        .iter()
        .position(|d| d.name == "nose_tip")
        .unwrap();
    let nostril = s
        .project()
        .face
        .dots
        .iter()
        .position(|d| d.name == "nostril_right")
        .unwrap();
    let chin = s
        .project()
        .face
        .dots
        .iter()
        .position(|d| d.name == "chin")
        .unwrap();
    let before = first_positions(&s, 0);
    let target = before[nose] + Point::new(8.0, 3.0);
    let v = run(
        &mut s,
        "face.locate.smartDrag",
        json!({"index": nose, "image": 1, "x": target.x, "y": target.y, "radius": 0.2}),
    );
    assert!(v["moved"].as_u64().unwrap() > 1);
    let after = first_positions(&s, 0);
    assert!(after[nose].distance(target) < 1e-9);
    let pulled = after[nostril].x - before[nostril].x;
    assert!(pulled > 1.0 && pulled < 8.0, "{pulled}");
    assert!(after[chin].distance(before[chin]) < 0.5);
    // Image 2 is untouched.
    assert!(close(&second_positions(&s, 0), &before));

    // A plain move changes exactly one dot.
    run(
        &mut s,
        "dots.move",
        json!({"index": chin, "image": 1, "x": before[chin].x, "y": before[chin].y + 1.0}),
    );
    // Dragging on image 2 works the same way.
    run(
        &mut s,
        "face.locate.smartDrag",
        json!({"index": nose, "image": 2, "x": 70, "y": 50}),
    );
    assert_eq!(second_positions(&s, 0)[nose], Point::new(70.0, 50.0));

    // The first dot of a second face is addressed with `first`.
    run(
        &mut s,
        "face.locate",
        json!({"mode": "append", "box_x": 90, "box_y": 10, "box_w": 25, "box_h": 30}),
    );
    let second_nose = 38 + nose;
    let at = first_positions(&s, 0)[second_nose];
    run(
        &mut s,
        "face.locate.smartDrag",
        json!({"index": second_nose, "first": 38, "image": 1, "x": at.x + 1.0, "y": at.y}),
    );
    assert_eq!(first_positions(&s, 0)[nose], after[nose]);
}

#[test]
fn bad_smart_drags_are_refused() {
    let d = Dir::new("baddrag");
    let mut s = session(&d);
    // No face yet.
    assert!(try_run(
        &mut s,
        "face.locate.smartDrag",
        json!({"index": 0, "image": 1, "x": 1, "y": 1})
    )
    .is_err());
    locate(&mut s, None, (20.0, 5.0, 80.0, 80.0));
    let before = s.project().clone();
    for p in [
        json!({"index": 38, "image": 1, "x": 1, "y": 1}),
        json!({"index": 0, "first": 1000, "image": 1, "x": 1, "y": 1}),
        json!({"index": 0, "image": 1, "x": 1, "y": 1, "radius": 0}),
        json!({"index": 0, "image": 3, "x": 1, "y": 1}),
        json!({"index": 0, "image": 1, "x": 1}),
        json!({"index": 0, "image": 1, "x": 1e12, "y": 1}),
        json!({"index": 0, "image": 1, "x": 1, "y": 1, "sequence": 5}),
    ] {
        assert!(
            try_run(&mut s, "face.locate.smartDrag", p.clone()).is_err(),
            "{p}"
        );
    }
    // Dragging a dot exactly onto another dot of the same image is refused.
    let other = first_positions(&s, 0)[5];
    let e = try_run(
        &mut s,
        "face.locate.smartDrag",
        json!({"index": 4, "image": 1, "x": other.x, "y": other.y, "radius": 0.01}),
    );
    assert!(e.is_err());
    assert_eq!(*s.project(), before);
}

#[test]
fn extract_frames_both_faces_on_the_same_template() {
    let d = Dir::new("extract");
    let mut s = session(&d);
    // Face boxes in different places and sizes in the two pictures.
    locate(&mut s, Some(1), (10.0, 10.0, 50.0, 60.0));
    locate(&mut s, Some(2), (50.0, 5.0, 60.0, 80.0));
    let model = s.project().face.clone();
    let template = FaceBox::new(30.0, 8.0, 60.0, 74.0).unwrap();
    let out = d.s("framed");
    run(
        &mut s,
        "face.extract",
        json!({"out_dir": out, "box_x": 30, "box_y": 8, "box_w": 60, "box_h": 74}),
    );
    // Both faces now sit exactly on the template, in both pictures.
    let expected = face::place(&model, &template);
    assert!(close(&first_positions(&s, 0), &expected));
    assert!(close(&second_positions(&s, 0), &expected));

    // The pictures were rewritten at the output size and the sequence points at them.
    let seq = &s.project().sequences[0];
    for image in [&seq.image1.0, &seq.image2.0] {
        assert!(image.contains("framed"), "{image}");
        let bytes = std::fs::read(image).unwrap();
        assert_eq!(inbetween_codecs::read_size(&bytes).unwrap(), (W, H));
    }

    // The content moved with the dots: the middle of the template shows what was in the
    // middle of the source face box.
    let framed = inbetween_codecs::read_file(std::path::Path::new(&seq.image1.0)).unwrap();
    let (sun, _) = demo::pair(W, H).unwrap();
    let px = |data: &[u8], x: u32, y: u32| {
        let i = ((y * W + x) * 4) as usize;
        data[i..i + 4].to_vec()
    };
    let sun8 = sun.to_rgba8();
    // Template centre (60, 45) came from the source box centre (35, 40).
    let got = px(&framed.data, 60, 45);
    let want = px(&sun8, 35, 40);
    assert!(
        got.iter().zip(&want).all(|(a, b)| a.abs_diff(*b) <= 40),
        "{got:?} vs {want:?}"
    );

    // Undo restores the dots and the old pictures.
    run(&mut s, "edit.undo", json!({}));
    assert!(s.project().sequences[0].image1.0.ends_with("sun.png"));
    assert!(close(
        &first_positions(&s, 0),
        &face::place(&model, &FaceBox::new(10.0, 10.0, 50.0, 60.0).unwrap())
    ));
}

#[test]
fn extract_needs_a_face_and_a_usable_template() {
    let d = Dir::new("badextract");
    let mut s = session(&d);
    let out = d.s("framed");
    assert!(try_run(&mut s, "face.extract", json!({"out_dir": out})).is_err());
    locate(&mut s, None, (10.0, 10.0, 50.0, 60.0));
    let before = s.project().clone();
    for p in [
        json!({"out_dir": out, "box_x": 1}),
        json!({"out_dir": out, "box_x": 1, "box_y": 1, "box_w": -5, "box_h": 5}),
        json!({"out_dir": out, "first": 50}),
        json!({"out_dir": out, "sequence": 9}),
    ] {
        assert!(try_run(&mut s, "face.extract", p.clone()).is_err(), "{p}");
    }
    assert_eq!(*s.project(), before);
    // Dots that all sit on one spot do not span a face.
    let mut flat = s.project().clone();
    for pair in &mut flat.sequences[0].dots.pairs {
        pair.first = Point::new(5.0, 5.0);
    }
    s.set_project(flat, None);
    assert!(try_run(&mut s, "face.extract", json!({"out_dir": out})).is_err());
}

fn two_faces(d: &Dir) -> Session {
    let mut s = session(d);
    run(&mut s, "sequence.duplicate", json!({}));
    run(
        &mut s,
        "sequence.image",
        json!({"sequence": 1, "image": 1, "path": d.s("moon.png")}),
    );
    locate_in(&mut s, 0, (20.0, 5.0, 70.0, 80.0));
    locate_in(&mut s, 1, (35.0, 15.0, 50.0, 60.0));
    s
}

fn locate_in(s: &mut Session, sequence: usize, b: (f64, f64, f64, f64)) {
    run(
        s,
        "face.locate",
        json!({"sequence": sequence, "box_x": b.0, "box_y": b.1, "box_w": b.2, "box_h": b.3}),
    );
}

fn read_png(path: &str) -> Vec<u8> {
    inbetween_codecs::read_file(std::path::Path::new(path))
        .unwrap()
        .data
}

#[test]
fn mixing_one_hundred_percent_of_a_face_gives_that_face_back() {
    let d = Dir::new("mix-identity");
    let mut s = two_faces(&d);
    let out = d.s("mix.png");
    let revision = s.revision();
    let v = run(
        &mut s,
        "face.mix",
        json!({"out": out, "features": "100,0", "shapes": "100,0"}),
    );
    assert_eq!(v["files"].as_array().unwrap().len(), 1);
    let (sun, _) = demo::pair(W, H).unwrap();
    let mixed = read_png(&out);
    let want = sun.to_rgba8();
    assert_eq!(mixed.len(), want.len());
    assert!(mixed.iter().zip(&want).all(|(a, b)| a.abs_diff(*b) <= 2));
    // `mix` writes a file but never edits the project.
    assert_eq!(s.revision(), revision);
}

#[test]
fn average_mix_sits_between_the_faces_and_is_deterministic() {
    let d = Dir::new("mix-average");
    let mut s = two_faces(&d);
    let a = d.s("a.png");
    let b = d.s("b.png");
    run(&mut s, "face.mix", json!({"out": a, "mode": "average"}));
    run(&mut s, "face.mix", json!({"out": b, "mode": "average"}));
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    // Defaults (no percentages) mean the same as average.
    let c = d.s("c.png");
    run(&mut s, "face.mix", json!({"out": c}));
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&c).unwrap());
    // And it differs from either 100% mix.
    let one = d.s("one.png");
    run(&mut s, "face.mix", json!({"out": one, "features": "100,0"}));
    assert_ne!(read_png(&a), read_png(&one));
    // Feature-only mixing: shapes follow the features when not given, and `lock` forces it.
    let locked = d.s("locked.png");
    run(
        &mut s,
        "face.mix",
        json!({"out": locked, "features": "70,30", "shapes": "10,90", "lock": true}),
    );
    let follow = d.s("follow.png");
    run(
        &mut s,
        "face.mix",
        json!({"out": follow, "features": "70,30"}),
    );
    assert_eq!(read_png(&locked), read_png(&follow));
    let unlocked = d.s("unlocked.png");
    run(
        &mut s,
        "face.mix",
        json!({"out": unlocked, "features": "70,30", "shapes": "10,90"}),
    );
    assert_ne!(read_png(&locked), read_png(&unlocked));
}

#[test]
fn random_candidates_are_reproducible_and_recipes_replay_them() {
    let d = Dir::new("mix-random");
    let mut s = two_faces(&d);
    let gallery = d.s("gallery.png");
    let v = run(
        &mut s,
        "face.mix",
        json!({"out": gallery, "mode": "random", "seed": 5, "count": 3}),
    );
    let files: Vec<String> = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap().to_string())
        .collect();
    assert_eq!(files.len(), 3);
    assert!(files[0].ends_with("gallery-001.png") && files[2].ends_with("gallery-003.png"));
    assert_ne!(read_png(&files[0]), read_png(&files[1]));

    // The same seed gives the same first candidate as a single mix.
    let single = d.s("single.png");
    run(
        &mut s,
        "face.mix",
        json!({"out": single, "mode": "random", "seed": 5}),
    );
    assert_eq!(read_png(&single), read_png(&files[0]));

    // A saved recipe replays the mix exactly, with no random mode involved.
    let recipe = d.s("r.inbetweenmix");
    let again = d.s("again.png");
    run(
        &mut s,
        "face.mix",
        json!({"out": again, "mode": "random", "seed": 5, "recipe_save": recipe}),
    );
    let replay = d.s("replay.png");
    run(
        &mut s,
        "face.mix",
        json!({"out": replay, "recipe_load": recipe}),
    );
    assert_eq!(read_png(&replay), read_png(&single));
}

#[test]
fn bad_mixes_are_refused() {
    let d = Dir::new("mix-bad");
    let mut s = two_faces(&d);
    let out = d.s("x.png");
    for p in [
        json!({"out": out, "features": "50"}),
        json!({"out": out, "features": "0,0"}),
        json!({"out": out, "features": "a,b"}),
        json!({"out": out, "features": "-1,2"}),
        json!({"out": out, "shapes": "1,2,3"}),
        json!({"out": out, "sequences": "0,7"}),
        json!({"out": out, "sequences": "x"}),
        json!({"out": out, "sequences": ","}),
        json!({"out": out, "count": 3}),
        json!({"out": out, "mode": "random", "count": 2, "recipe_save": d.s("r.inbetweenmix")}),
        json!({"out": out, "recipe_load": d.s("missing.inbetweenmix")}),
        json!({"out": out, "first": 60}),
    ] {
        assert!(try_run(&mut s, "face.mix", p.clone()).is_err(), "{p}");
    }
    assert!(!PathBuf::from(&out).exists());
    // Faces with different dot counts cannot be mixed: sequence 1 has a second face.
    run(
        &mut s,
        "face.locate",
        json!({"sequence": 1, "mode": "append", "box_x": 85, "box_y": 20, "box_w": 30, "box_h": 40}),
    );
    assert!(try_run(&mut s, "face.mix", json!({"out": out})).is_ok());
}

#[test]
fn the_library_stores_lists_reuses_and_removes_entries() {
    let d = Dir::new("library");
    let mut s = session(&d);
    locate(&mut s, None, (30.0, 10.0, 50.0, 70.0));
    let dir = d.s("lib");
    run(
        &mut s,
        "face.library",
        json!({"dir": dir, "op": "add", "name": "sun face", "folder": "suns"}),
    );
    run(
        &mut s,
        "image.saveToLibrary",
        json!({"dir": dir, "name": "moon face", "image": 2}),
    );
    let list = run(&mut s, "face.library", json!({"dir": dir}));
    let entries = list["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["folder"], "");
    assert_eq!(entries[0]["name"], "moon face");
    assert_eq!(entries[1]["folder"], "suns");
    assert_eq!(entries[1]["dots"], 38);

    // Adding again needs overwrite.
    assert!(try_run(
        &mut s,
        "face.library",
        json!({"dir": dir, "op": "add", "name": "sun face", "folder": "suns"})
    )
    .is_err());
    run(
        &mut s,
        "face.library",
        json!({"dir": dir, "op": "add", "name": "sun face", "folder": "suns", "overwrite": true}),
    );

    // Use the entry in a fresh project: the picture and the dots arrive where they were.
    let wanted = first_positions(&s, 0);
    let mut t = session(&d);
    let v = run(
        &mut t,
        "face.library",
        json!({"dir": dir, "op": "use", "name": "sun face", "folder": "suns", "image": 2}),
    );
    assert_eq!(v["dots"], 38);
    assert!(t.project().sequences[0].image2.0.contains("sun face"));
    assert!(close(&second_positions(&t, 0), &wanted));
    assert!(t.prepare(0).is_ok());

    // Remove.
    run(
        &mut s,
        "face.library",
        json!({"dir": dir, "op": "remove", "name": "sun face", "folder": "suns"}),
    );
    let list = run(&mut s, "face.library", json!({"dir": dir}));
    assert_eq!(list["entries"].as_array().unwrap().len(), 1);
    assert!(try_run(
        &mut s,
        "face.library",
        json!({"dir": dir, "op": "remove", "name": "sun face", "folder": "suns"})
    )
    .is_err());

    // An empty or missing library lists nothing.
    let empty = run(&mut s, "face.library", json!({"dir": d.s("nowhere")}));
    assert_eq!(empty["entries"].as_array().unwrap().len(), 0);
}

#[test]
fn library_names_cannot_leave_the_library() {
    let d = Dir::new("library-names");
    let mut s = session(&d);
    let dir = d.s("lib");
    for (name, folder) in [
        ("..", None),
        ("../escape", None),
        ("a/b", None),
        ("a\\b", None),
        (".hidden", None),
        ("", None),
        ("ok", Some("..")),
        ("ok", Some("a/b")),
        ("ok", Some(".git")),
    ] {
        let mut p = json!({"dir": dir, "op": "add", "name": name});
        if let Some(f) = folder {
            p["folder"] = json!(f);
        }
        assert!(try_run(&mut s, "face.library", p.clone()).is_err(), "{p}");
    }
    assert!(try_run(&mut s, "face.library", json!({"dir": dir, "op": "add"})).is_err());
    assert!(!PathBuf::from(d.s("escape.png")).exists());
    assert!(!PathBuf::from(&dir).join("..").join("escape.png").exists());
    // Using a missing entry is an error, and changes nothing.
    let before = s.project().clone();
    assert!(try_run(
        &mut s,
        "face.library",
        json!({"dir": dir, "op": "use", "name": "ghost"})
    )
    .is_err());
    assert_eq!(*s.project(), before);
}

#[test]
fn library_dots_follow_the_picture_when_the_frame_differs() {
    let d = Dir::new("library-fit");
    let mut s = session(&d);
    locate(&mut s, None, (30.0, 10.0, 50.0, 70.0));
    let dir = d.s("lib");
    run(
        &mut s,
        "face.library",
        json!({"dir": dir, "op": "add", "name": "a"}),
    );
    // A project with a bigger frame fits the picture to it: the dots scale with the picture.
    let (sun, moon) = demo::pair(240, 180).unwrap();
    for (name, img) in [("big_sun.png", sun), ("big_moon.png", moon)] {
        let png = inbetween_codecs::encode_png(240, 180, &img.to_rgba8()).unwrap();
        std::fs::write(d.0.join(name), png).unwrap();
    }
    let mut t = Session::new();
    run(
        &mut t,
        "project.new",
        json!({"image1": d.s("big_sun.png"), "image2": d.s("big_moon.png")}),
    );
    run(
        &mut t,
        "face.library",
        json!({"dir": dir, "op": "use", "name": "a"}),
    );
    let small = first_positions(&s, 0);
    let big = first_positions(&t, 0);
    assert_eq!(small.len(), big.len());
    assert!(small
        .iter()
        .zip(&big)
        .all(|(a, b)| (a.x * 2.0 - b.x).abs() < 1e-6 && (a.y * 2.0 - b.y).abs() < 1e-6));
}
