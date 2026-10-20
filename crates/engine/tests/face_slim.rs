//! A build without the bundled detector (`--no-default-features`): `face.locate` falls back to
//! a box and asking for detection is an actionable error.
#![cfg(not(feature = "face-detect"))]

use inbetween_engine::{Registry, Session};
use inbetween_testkit::demo;
use serde_json::json;

fn session() -> Session {
    let dir = std::env::temp_dir().join(format!("inbetween-slim-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b) = demo::pair(60, 45).unwrap();
    let mut paths = Vec::new();
    for (name, img) in [("a.png", a), ("b.png", b)] {
        let png = inbetween_codecs::encode_png(60, 45, &img.to_rgba8()).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, png).unwrap();
        paths.push(p.display().to_string());
    }
    let mut s = Session::new();
    Registry::standard()
        .run(
            &mut s,
            "project.new",
            &json!({"image1": paths[0], "image2": paths[1]}),
        )
        .unwrap();
    s
}

#[test]
fn locate_defaults_to_a_box_and_detect_is_refused_with_advice() {
    let r = Registry::standard();
    let mut s = session();
    let out = r.run(&mut s, "face.locate", &json!({})).unwrap();
    assert_eq!(out["source"], "box");
    assert_eq!(s.project().sequences[0].dots.len(), 38);

    let before = s.project().clone();
    for (id, p) in [
        ("face.locate", json!({"source": "detect"})),
        ("face.locateAll", json!({})),
        (
            "face.locate",
            json!({"landmarks": "x.inbetweenlandmarks", "layout": "mesh478"}),
        ),
    ] {
        let e = r.run(&mut s, id, &p).unwrap_err();
        assert!(!e.to_string().is_empty(), "{id}");
    }
    let e = r.run(&mut s, "face.detect", &json!({})).unwrap_err();
    assert!(
        e.to_string().contains("does not include the face detector"),
        "{e}"
    );
    assert_eq!(*s.project(), before);
}
