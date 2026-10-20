//! End-to-end detector tests on public-domain portraits and generated images.
#![cfg(feature = "bundled-models")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use inbetween_detect::{mesh_map, Detector, Face, MESH_POINTS};
use inbetween_raster::Image;
use std::sync::OnceLock;

fn detector() -> &'static Detector {
    static D: OnceLock<Detector> = OnceLock::new();
    D.get_or_init(|| Detector::new().expect("bundled models load"))
}

fn load(name: &str) -> Image {
    let path = format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(path).expect("fixture");
    let img = inbetween_codecs::decode(&bytes).expect("decodes");
    Image::from_rgba8(img.width, img.height, &img.data).expect("image")
}

fn one_face(name: &str) -> Face {
    let faces = detector().faces(&load(name)).expect("detects");
    assert_eq!(faces.len(), 1, "{name}: {faces:?}");
    faces.into_iter().next().expect("one face")
}

fn mid(f: &Face, name: &str) -> (f64, f64) {
    let p = mesh_map::point(name, &f.mesh).expect("mapped");
    (p.x, p.y)
}

#[test]
fn each_portrait_has_one_face_in_its_window() {
    // Hand-measured windows on the 512 x 640 fixtures (x0, y0, x1, y1).
    for (name, win) in [
        ("portrait_a.png", (240.0, 80.0, 390.0, 260.0)),
        ("portrait_b.png", (225.0, 100.0, 380.0, 290.0)),
    ] {
        let f = one_face(name);
        let b = f.bounds;
        assert!(
            b.x0 >= win.0 && b.y0 >= win.1 && b.x1 <= win.2 && b.y1 <= win.3,
            "{name}: {b:?} outside {win:?}"
        );
        assert!(f.score >= 0.5 && f.score <= 1.0);
    }
}

#[test]
fn subjects_right_eye_is_at_image_left_and_features_are_in_order() {
    for name in ["portrait_a.png", "portrait_b.png"] {
        let f = one_face(name);
        let (re, le) = (mid(&f, "pupil_right"), mid(&f, "pupil_left"));
        let (nose, mouth) = (mid(&f, "nose_tip"), mid(&f, "lip_top_mid"));
        let chin = mid(&f, "chin");
        assert!(re.0 < le.0, "{name}");
        assert!(
            re.1 < nose.1 && nose.1 < mouth.1 && mouth.1 < chin.1,
            "{name}"
        );
        let (jr, jl) = (mid(&f, "jaw_right_mid"), mid(&f, "jaw_left_mid"));
        assert!(jr.0 < re.0 && le.0 < jl.0, "{name}");
    }
}

#[test]
fn all_mesh_points_are_finite_and_near_the_face() {
    for name in ["portrait_a.png", "portrait_b.png"] {
        let f = one_face(name);
        assert_eq!(f.mesh.len(), MESH_POINTS);
        let w = f.bounds.x1 - f.bounds.x0;
        for p in &f.mesh {
            assert!(p.is_finite());
            assert!(p.x > f.bounds.x0 - w && p.x < f.bounds.x1 + w);
        }
    }
}

#[test]
fn detection_is_deterministic() {
    let img = load("portrait_a.png");
    let a = detector().faces(&img).expect("detects");
    let b = detector().faces(&img).expect("detects");
    assert_eq!(a, b);
}

#[test]
fn images_without_a_face_give_an_empty_list() {
    let (w, h) = (256u32, 256u32);
    let mut grad = Image::new(w, h).expect("image");
    let mut check = Image::new(w, h).expect("image");
    for y in 0..h {
        for x in 0..w {
            let g = x as f32 / w as f32;
            grad.set(x, y, [g, g * 0.5, 1.0 - g, 1.0]);
            let c = if (x / 16 + y / 16) % 2 == 0 { 1.0 } else { 0.0 };
            check.set(x, y, [c, c, c, 1.0]);
        }
    }
    let black = Image::filled(64, 64, [0.0, 0.0, 0.0, 1.0]).expect("image");
    for img in [grad, check, black] {
        assert!(detector().faces(&img).expect("ok").is_empty());
    }
}

#[test]
fn hostile_inputs_never_panic() {
    let d = detector();
    let one = Image::filled(1, 1, [0.5, 0.5, 0.5, 1.0]).expect("image");
    assert!(d.faces(&one).map(|f| f.is_empty()).unwrap_or(true));
    let clear = Image::new(100, 50).expect("transparent");
    assert!(d.faces(&clear).map(|f| f.is_empty()).unwrap_or(true));
    let mut nan = Image::new(32, 32).expect("image");
    nan.as_mut_slice().fill(f32::NAN);
    assert!(d.faces(&nan).is_err());
    let wide = Image::filled(9000, 12, [0.2, 0.4, 0.6, 1.0]).expect("image");
    assert!(d.faces(&wide).map(|f| f.is_empty()).unwrap_or(true));
    let big = Image::filled(5000, 5000, [0.2, 0.4, 0.6, 1.0]).expect("image");
    assert!(d.faces(&big).map(|f| f.is_empty()).unwrap_or(true));
}

#[test]
fn a_face_is_found_after_scaling_the_portrait_down_and_up() {
    let img = load("portrait_b.png");
    let small = img.downscale_to_fit(320).expect("downscale");
    let faces = detector().faces(&small).expect("detects");
    assert_eq!(faces.len(), 1);
    let k = 320.0 / 640.0;
    let b = faces[0].bounds;
    assert!(b.x0 > 225.0 * k && b.x1 < 380.0 * k, "{b:?}");
}

/// Timing by hand; `cargo xtask perf` runs the `perf_detect` example with a budget.
#[test]
#[ignore = "timing only"]
fn timing() {
    let t = std::time::Instant::now();
    let d = Detector::new().expect("loads");
    eprintln!("load: {:?}", t.elapsed());
    // A 1080p frame holding the portrait at full height.
    let p = load("portrait_a.png");
    let mut frame = Image::new(1920, 1080).expect("image");
    let (ow, oh) = (p.width(), p.height());
    for y in 0..1080u32 {
        for x in 0..1920u32 {
            let (sx, sy) = (
                (x as f32 - 700.0) * ow as f32 / 864.0,
                y as f32 * oh as f32 / 1080.0,
            );
            if sx >= 0.0 && (sx as u32) < ow {
                frame.set(x, y, p.sample_bilinear(sx, sy));
            }
        }
    }
    let _ = d.faces(&frame).expect("warm up");
    let runs = 10;
    let t = std::time::Instant::now();
    let mut n = 0;
    for _ in 0..runs {
        n = d.faces(&frame).expect("detects").len();
    }
    eprintln!("1080p: {:?} per image, {n} face(s)", t.elapsed() / runs);
    let t = std::time::Instant::now();
    for _ in 0..runs {
        let _ = d.faces(&p).expect("detects");
    }
    eprintln!("512x640: {:?} per image", t.elapsed() / runs);
}
