//! Effect commands end to end: set, render, save, reopen, undo, errors.

use inbetween_engine::{Registry, Session};
use inbetween_testkit::demo;
use serde_json::{json, Value};
use std::path::PathBuf;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("inbetween-fx-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (sun, moon) = demo::pair(24, 18).unwrap();
        let mut gray = vec![0u8; 24 * 18 * 4];
        for (i, px) in gray.chunks_mut(4).enumerate() {
            let v = if i % 24 < 12 { 255 } else { 0 };
            px.copy_from_slice(&[v, v, v, 255]);
        }
        let blue: Vec<u8> = (0..24 * 18).flat_map(|_| [0, 0, 255, 255]).collect();
        for (name, rgba) in [
            ("sun.png", sun.to_rgba8()),
            ("moon.png", moon.to_rgba8()),
            ("mask.png", gray),
            ("blue.png", blue),
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

fn try_run(s: &mut Session, id: &str, p: Value) -> Result<Value, inbetween_engine::CommandError> {
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
    run(&mut s, "sequence.length", json!({"frames": 3}));
    s
}

fn frame_pixel(s: &mut Session, x: u32, y: u32) -> [u8; 4] {
    let prepared = s.prepare(0).unwrap();
    let img = prepared.renderer().unwrap().render_frame(0).unwrap();
    let rgba = img.to_rgba8();
    let i = ((y * img.width() + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

#[test]
fn mask_and_background_change_the_rendered_frame() {
    let d = Dir::new("render");
    let mut s = session(&d);
    let plain_left = frame_pixel(&mut s, 2, 1);
    let plain_right = frame_pixel(&mut s, 20, 1);
    run(
        &mut s,
        "effect.background",
        json!({"image": d.s("blue.png")}),
    );
    run(&mut s, "effect.mask", json!({"image": d.s("mask.png")}));
    // The right half of the mask is black: the morph is hidden, the blue background shows.
    assert_eq!(frame_pixel(&mut s, 20, 1), [0, 0, 255, 255]);
    // The left half keeps the morph (opaque, so the background cannot show through).
    assert_eq!(frame_pixel(&mut s, 2, 1), plain_left);
    // Switching the mask off brings the morph back on the right.
    run(&mut s, "effect.mask", json!({"enabled": false}));
    assert_eq!(frame_pixel(&mut s, 20, 1), plain_right);
}

#[test]
fn back_color_reports_and_clears() {
    let d = Dir::new("color");
    let mut s = session(&d);
    let v = run(&mut s, "effect.backColor", json!({}));
    assert_eq!(v["enabled"], false);
    let v = run(&mut s, "effect.backColor", json!({"color": "#102030"}));
    assert_eq!(
        (v["enabled"].clone(), v["color"].clone()),
        (json!(true), json!("#102030ff"))
    );
    let v = run(&mut s, "effect.backColor", json!({"clear": true}));
    assert_eq!(v["enabled"], false);
}

#[test]
fn bad_input_is_refused_and_changes_nothing() {
    let d = Dir::new("bad");
    let mut s = session(&d);
    let rev = s.revision();
    let cases = [
        ("effect.backColor", json!({"color": "red"})),
        ("effect.mask", json!({"enabled": true})), // no image yet
        ("effect.mask", json!({"image": d.s("missing.png")})),
        ("effect.foreground", json!({"opacity": 101})),
        ("effect.mask", json!({"intensity": -101})),
        ("effect.background", json!({"sequence": 9, "clear": true})),
    ];
    for (id, p) in cases {
        assert!(try_run(&mut s, id, p.clone()).is_err(), "{id} {p}");
    }
    assert_eq!(s.revision(), rev);
    assert!(!s.project().sequences[0].effects.any_active());
}

#[test]
fn enabled_effect_whose_file_vanished_is_an_error_not_a_crash() {
    let d = Dir::new("gone");
    let mut s = session(&d);
    run(
        &mut s,
        "effect.foreground",
        json!({"image": d.s("blue.png")}),
    );
    std::fs::remove_file(d.0.join("blue.png")).unwrap();
    let e = s
        .prepare(0)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(e.contains("foreground"), "{e}");
}

#[test]
fn effects_survive_save_and_open_and_undo() {
    let d = Dir::new("save");
    let mut s = session(&d);
    run(
        &mut s,
        "effect.background",
        json!({"image": d.s("blue.png"), "opacity": 40}),
    );
    run(
        &mut s,
        "effect.mask",
        json!({"image": d.s("mask.png"), "intensity": -30, "invert": true}),
    );
    run(&mut s, "effect.backColor", json!({"color": "#ff000080"}));
    let file = d.s("fx.inbetween");
    run(&mut s, "project.save", json!({"path": file}));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("\"blue.png\""),
        "image paths are saved relative:
{text}"
    );

    let mut t = Session::new();
    run(&mut t, "project.open", json!({"path": file}));
    let fx = &t.project().sequences[0].effects;
    assert!(fx.background.enabled && fx.mask.enabled && fx.back_color.enabled);
    assert_eq!(
        (fx.background.opacity, fx.mask.intensity, fx.mask.invert),
        (40.0, -30.0, true)
    );
    assert_eq!(fx.back_color.rgba, [255, 0, 0, 128]);
    assert_eq!(frame_pixel(&mut t, 20, 1), frame_pixel(&mut s, 20, 1));

    // Undo takes effects back one command at a time.
    run(&mut s, "edit.undo", json!({}));
    assert!(!s.project().sequences[0].effects.back_color.enabled);
    run(&mut s, "edit.undo", json!({}));
    assert!(!s.project().sequences[0].effects.mask.enabled);
}

#[test]
fn a_project_file_with_bad_effect_numbers_does_not_open() {
    let d = Dir::new("hostile");
    let file = d.s("bad.inbetween");
    let text = r#"{"format":"inbetween-project","version":1,"project":{"sequences":[{"effects":{"mask":{"intensity":1e9}}}]}}"#;
    std::fs::write(&file, text).unwrap();
    let mut s = Session::new();
    assert!(try_run(&mut s, "project.open", json!({"path": file})).is_err());
    let text = r#"{"format":"inbetween-project","version":1,"project":{"sequences":[{"effects":{"foreground":{"enabled":true,"image":"/abs/x.png"}}}]}}"#;
    std::fs::write(&file, text).unwrap();
    assert!(try_run(&mut s, "project.open", json!({"path": file})).is_err());
}
