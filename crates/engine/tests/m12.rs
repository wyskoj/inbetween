//! Layer projects, camera, track and master curves, and the transition, light, trail and
//! radial effects through the command registry: set, render, export, save, reopen, refuse.

use inbetween_engine::{CommandError, Registry, Session};
use inbetween_testkit::demo;
use serde_json::{json, Value};
use std::path::PathBuf;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("inbetween-m12-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (sun, moon) = demo::pair(24, 18).unwrap();
        let mut gray = vec![0u8; 24 * 18 * 4];
        for (i, px) in gray.chunks_mut(4).enumerate() {
            let v = if i % 24 < 12 { 0 } else { 255 };
            px.copy_from_slice(&[v, v, v, 255]);
        }
        for (name, rgba) in [
            ("sun.png", sun.to_rgba8()),
            ("moon.png", moon.to_rgba8()),
            ("wipe.png", gray),
        ] {
            let png = inbetween_codecs::encode_png(24, 18, &rgba).unwrap();
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

fn session(d: &Dir, frames: u32) -> Session {
    let mut s = Session::new();
    run(
        &mut s,
        "project.new",
        json!({"image1": d.s("sun.png"), "image2": d.s("moon.png")}),
    );
    run(&mut s, "sequence.length", json!({"frames": frames}));
    s
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

fn pixel(rgba: &[u8], x: usize, y: usize) -> [u8; 4] {
    let i = (y * 24 + x) * 4;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

fn png_pixels(path: PathBuf) -> Vec<u8> {
    inbetween_codecs::read_file(&path).unwrap().data
}

#[test]
fn layer_projects_stack_with_the_first_on_top_and_hold_short_layers() {
    let d = Dir::new("layers");
    let mut s = session(&d, 3);
    run(&mut s, "sequence.duplicate", json!({"sequence": 0}));
    run(
        &mut s,
        "sequence.length",
        json!({"sequence": 1, "frames": 5}),
    );
    // Sequence mode: 3 + 5 frames. Layer mode: the longer one.
    assert_eq!(run(&mut s, "project.info", json!({}))["total_frames"], 8);
    let v = run(&mut s, "project.type", json!({"kind": "layer"}));
    assert_eq!(v["total_frames"], 5);
    assert_eq!(run(&mut s, "project.info", json!({}))["kind"], "layer");

    // The top layer (sequence 0) is opaque, so it hides the one below; once it has played its
    // 3 frames it holds its last frame for the rest of the movie.
    let out = d.0.join("movie");
    let v = run(
        &mut s,
        "export.movie",
        json!({"out_dir": out.display().to_string()}),
    );
    assert_eq!(v["frames"], 5);
    assert!(!out.join("frame_0006.png").exists());
    let top_last = frame(&mut s, 0, 2);
    for n in [3, 4, 5] {
        assert_eq!(
            png_pixels(out.join(format!("frame_{n:04}.png"))),
            top_last,
            "frame {n}"
        );
    }

    // Fade the top layer out with a master curve: at the end only the bottom layer shows.
    run(
        &mut s,
        "curve.master",
        json!({"sequence": 0, "op": "move", "index": 1, "y": 0.0}),
    );
    let out2 = d.0.join("movie2");
    run(
        &mut s,
        "export.movie",
        json!({"out_dir": out2.display().to_string()}),
    );
    let bottom_last = frame(&mut s, 1, 4);
    assert_eq!(png_pixels(out2.join("frame_0005.png")), bottom_last);
    // The first frame is the top layer at full opacity.
    assert_eq!(png_pixels(out2.join("frame_0001.png")), frame(&mut s, 0, 0));

    // Hidden layers drop out; switching back restores the sequential movie.
    run(
        &mut s,
        "sequence.visibility",
        json!({"sequence": 0, "visible": false}),
    );
    assert_eq!(run(&mut s, "project.info", json!({}))["total_frames"], 5);
    run(&mut s, "project.type", json!({"kind": "sequence"}));
    assert_eq!(run(&mut s, "project.info", json!({}))["total_frames"], 5);
    run(
        &mut s,
        "sequence.visibility",
        json!({"sequence": 0, "visible": true}),
    );
    assert_eq!(run(&mut s, "project.info", json!({}))["total_frames"], 8);
}

#[test]
fn camera_moves_the_picture_and_leaves_the_first_frame_alone() {
    let d = Dir::new("camera");
    let mut s = session(&d, 3);
    let plain_first = frame(&mut s, 0, 0);
    let plain_last = frame(&mut s, 0, 2);
    run(
        &mut s,
        "camera.keyframes",
        json!({"op": "set", "t": 1.0, "x": 0.5}),
    );
    // At t = 0 the camera is at its first keyframe's pose: here that is also the last one, so
    // the whole movie is shifted. Add an identity key at 0 so it pans.
    run(
        &mut s,
        "camera.keyframes",
        json!({"op": "set", "t": 0.0, "x": 0.0}),
    );
    assert_eq!(frame(&mut s, 0, 0), plain_first);
    let last = frame(&mut s, 0, 2);
    assert!(last != plain_last);
    // Half the picture has moved off to the right: the left 12 columns are transparent.
    assert_eq!(pixel(&last, 3, 9)[3], 0);
    assert_eq!(pixel(&last, 20, 9), pixel(&plain_last, 8, 9));
    // Turning it off restores the plain frame; the keyframes stay.
    run(&mut s, "camera.keyframes", json!({"enabled": false}));
    assert_eq!(frame(&mut s, 0, 2), plain_last);
    let v = run(&mut s, "camera.keyframes", json!({}));
    assert_eq!(v["keyframes"].as_array().map(Vec::len), Some(2));
}

#[test]
fn track_bends_the_middle_but_not_the_ends() {
    let d = Dir::new("track");
    let mut s = session(&d, 5);
    // Dots that really move: the track has nothing to bend without them.
    for (x, y, x2, y2) in [(6, 5, 12, 9), (18, 6, 10, 4), (12, 14, 16, 13)] {
        run(
            &mut s,
            "dots.add",
            json!({"x": x, "y": y, "x2": x2, "y2": y2}),
        );
    }
    let straight: Vec<Vec<u8>> = (0..5).map(|i| frame(&mut s, 0, i)).collect();
    run(
        &mut s,
        "curve.track",
        json!({"op": "preset", "name": "arc", "tension": 20}),
    );
    let bent: Vec<Vec<u8>> = (0..5).map(|i| frame(&mut s, 0, i)).collect();
    assert_eq!(bent[0], straight[0]);
    assert_eq!(bent[4], straight[4]);
    assert!(bent[2] != straight[2]);
    // Tension 0 is the straight path again.
    run(&mut s, "curve.track", json!({"tension": 0}));
    assert_eq!(frame(&mut s, 0, 2), straight[2]);
}

#[test]
fn transition_light_trail_and_radial_render_and_keep_the_ends() {
    let d = Dir::new("effects");
    let mut s = session(&d, 5);
    let plain: Vec<Vec<u8>> = (0..5).map(|i| frame(&mut s, 0, i)).collect();

    // Transition: dark (left) half dissolves first.
    let v = run(
        &mut s,
        "effect.transition",
        json!({"image": d.s("wipe.png"), "softness": 0}),
    );
    assert_eq!(v["enabled"], true);
    assert_eq!(frame(&mut s, 0, 0), plain[0]);
    assert_eq!(frame(&mut s, 0, 4), plain[4]);
    let mid = frame(&mut s, 0, 2);
    // Left side already shows Image 2, right side still Image 1.
    assert_eq!(pixel(&mid, 3, 9), pixel(&plain[4], 3, 9));
    assert_eq!(pixel(&mid, 20, 9), pixel(&plain[0], 20, 9));
    run(&mut s, "effect.transition", json!({"clear": true}));
    assert_eq!(frame(&mut s, 0, 2), plain[2]);

    // Light: bright half brightens, so some pixel gets lighter and none darker on that side.
    run(
        &mut s,
        "effect.light",
        json!({"image": d.s("wipe.png"), "bright": 100, "dark": 100}),
    );
    let lit = frame(&mut s, 0, 2);
    assert!(lit != plain[2]);
    assert!(lit[..].len() == plain[2].len());
    run(&mut s, "effect.light", json!({"enabled": false}));
    assert_eq!(frame(&mut s, 0, 2), plain[2]);

    // Trail: blur leaves the first frame alone and changes later ones.
    run(
        &mut s,
        "effect.trail",
        json!({"overlay": true, "intensity": 100}),
    );
    assert_eq!(frame(&mut s, 0, 0), plain[0]);
    assert!(frame(&mut s, 0, 3) != plain[3]);
    run(&mut s, "effect.trail", json!({"enabled": false}));

    // Radial: rays over the picture.
    let v = run(
        &mut s,
        "effect.radial",
        json!({"color": "#ff0000", "intensity": 100}),
    );
    assert_eq!(v["color"], "#ff0000ff");
    assert!(frame(&mut s, 0, 1) != plain[1]);
    run(&mut s, "effect.radial", json!({"clear": true}));
    assert_eq!(frame(&mut s, 0, 1), plain[1]);
    assert!(!s.project().sequences[0].effects.any_active());
}

#[test]
fn everything_survives_save_open_and_undo() {
    let d = Dir::new("save");
    let mut s = session(&d, 4);
    run(
        &mut s,
        "effect.transition",
        json!({"image": d.s("wipe.png"), "softness": 35, "invert": true}),
    );
    run(
        &mut s,
        "effect.light",
        json!({"image": d.s("wipe.png"), "bright": 20, "dark": 80}),
    );
    run(
        &mut s,
        "effect.trail",
        json!({"overlay": false, "color": "#336699", "intensity": 70}),
    );
    run(
        &mut s,
        "effect.radial",
        json!({"overlay": false, "intensity": 12.5}),
    );
    run(
        &mut s,
        "camera.keyframes",
        json!({"op": "set", "t": 1.0, "x": 0.25, "scale_x": -1.5, "rotation": 12}),
    );
    run(
        &mut s,
        "camera.curve",
        json!({"op": "add", "x": 0.5, "y": 0.2}),
    );
    run(
        &mut s,
        "curve.track",
        json!({"op": "preset", "name": "wave", "tension": 2, "flip_h": true}),
    );
    run(
        &mut s,
        "curve.master",
        json!({"op": "move", "index": 0, "y": 0.5}),
    );
    run(&mut s, "project.type", json!({"kind": "layer"}));
    let before = s.project().clone();
    let file = d.s("p.inbetween");
    run(&mut s, "project.save", json!({"path": file}));

    let mut t = Session::new();
    run(&mut t, "project.open", json!({"path": file}));
    let after = t.project();
    assert_eq!(after.kind, before.kind);
    let (a, b) = (&after.sequences[0], &before.sequences[0]);
    assert_eq!(a.camera, b.camera);
    assert_eq!(a.track, b.track);
    assert_eq!(a.master_curve, b.master_curve);
    assert_eq!(a.effects.trail, b.effects.trail);
    assert_eq!(a.effects.radial, b.effects.radial);
    assert_eq!(a.effects.transition.softness, 35.0);
    assert!(a.effects.transition.invert && a.effects.transition.enabled);
    // Images were saved relative to the project file and still load.
    assert_eq!(a.effects.transition.image.0, "wipe.png");
    assert_eq!(a.effects.light.image.0, "wipe.png");
    assert!(t.prepare(0).is_ok());
    assert_eq!(frame(&mut t, 0, 1), frame(&mut s, 0, 1));

    // Undo walks back through the edits.
    for _ in 0..3 {
        run(&mut s, "edit.undo", json!({}));
    }
    assert_eq!(run(&mut s, "project.info", json!({}))["kind"], "sequence");
}

#[test]
fn bad_input_is_refused_and_changes_nothing() {
    let d = Dir::new("bad");
    let mut s = session(&d, 3);
    let rev = s.revision();
    let cases = [
        ("effect.transition", json!({"enabled": true})), // no image
        ("effect.transition", json!({"image": d.s("missing.png")})),
        ("effect.transition", json!({"softness": 101})),
        ("effect.light", json!({"enabled": true})),
        ("effect.light", json!({"bright": -1})),
        ("effect.trail", json!({"color": "green"})),
        ("effect.trail", json!({"intensity": 200})),
        ("effect.radial", json!({"color": "#12"})),
        (
            "camera.keyframes",
            json!({"op": "set", "t": 0.5, "scale_x": 0}),
        ),
        ("camera.keyframes", json!({"op": "set", "t": 2})),
        ("camera.keyframes", json!({"op": "remove", "index": 3})),
        (
            "camera.curve",
            json!({"op": "move", "index": 0, "x": 0.1, "y": 0.1}),
        ),
        ("curve.master", json!({"op": "add", "x": 0.5})),
        ("curve.master", json!({"op": "remove", "index": 0})),
        (
            "curve.track",
            json!({"op": "move", "index": 0, "u": 1, "v": 1}),
        ),
        ("curve.track", json!({"tension": 99})),
        ("project.type", json!({"kind": "pile"})),
        ("camera.keyframes", json!({"sequence": 9, "op": "reset"})),
    ];
    for (id, p) in cases {
        assert!(try_run(&mut s, id, p.clone()).is_err(), "{id} {p}");
    }
    assert_eq!(s.revision(), rev);
    let seq = &s.project().sequences[0];
    assert!(!seq.needs_post_pass() && !seq.track.is_active());
}

#[test]
fn deleted_effect_image_is_an_error_not_a_crash() {
    let d = Dir::new("gone");
    let mut s = session(&d, 3);
    run(
        &mut s,
        "effect.transition",
        json!({"image": d.s("wipe.png")}),
    );
    std::fs::remove_file(d.0.join("wipe.png")).unwrap();
    // The cached image may still be in memory; a fresh session must report it cleanly.
    let file = d.s("p.inbetween");
    run(&mut s, "project.save", json!({"path": file}));
    let mut t = Session::new();
    run(&mut t, "project.open", json!({"path": file}));
    let e = t.prepare(0).err().expect("missing image must be an error");
    assert!(e.to_string().contains("transition"), "{e}");
}
