//! Multi-sequence projects and curves, end to end through the command registry.

use inbetween_engine::{Registry, Session};
use inbetween_testkit::demo;
use serde_json::json;
use std::path::PathBuf;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("inbetween-ms-{tag}-{}", std::process::id()));
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

fn run(s: &mut Session, id: &str, p: serde_json::Value) -> serde_json::Value {
    Registry::standard()
        .run(s, id, &p)
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn frames_in(dir: &PathBuf) -> usize {
    std::fs::read_dir(dir).map_or(0, |d| d.count())
}

#[test]
fn continued_sequences_export_as_one_movie() {
    let d = Dir::new("movie");
    let mut s = Session::new();
    run(
        &mut s,
        "project.new",
        json!({"image1": d.s("sun.png"), "image2": d.s("moon.png")}),
    );
    run(&mut s, "sequence.length", json!({"frames": 4}));
    // Sun -> moon, then moon -> sun: the second sequence starts where the first ends.
    let v = run(
        &mut s,
        "sequence.add",
        json!({"image2": d.s("sun.png"), "name": "back"}),
    );
    assert_eq!(v["sequence"], 1);
    let seq1 = &s.project().sequences[1];
    assert_eq!(seq1.image1, s.project().sequences[0].image2);
    assert_eq!((seq1.frames, seq1.name.as_str()), (4, "back"));
    run(
        &mut s,
        "sequence.length",
        json!({"sequence": 1, "frames": 3}),
    );
    let info = run(&mut s, "project.info", json!({}));
    assert_eq!(info["total_frames"], 7);

    let out = d.0.join("movie");
    let v = run(
        &mut s,
        "export.movie",
        json!({"out_dir": out.display().to_string()}),
    );
    assert_eq!(
        (v["frames"].as_u64(), v["total_frames"].as_u64()),
        (Some(7), Some(7))
    );
    assert_eq!(frames_in(&out), 7);
    assert!(out.join("frame_0007.png").exists());

    // Hiding a sequence drops it from the movie; a range writes just those frames.
    run(
        &mut s,
        "sequence.visibility",
        json!({"sequence": 0, "visible": false}),
    );
    let out2 = d.0.join("movie2");
    let v = run(
        &mut s,
        "export.movie",
        json!({"out_dir": out2.display().to_string()}),
    );
    assert_eq!(v["frames"], 3);
    let out3 = d.0.join("movie3");
    run(
        &mut s,
        "sequence.visibility",
        json!({"sequence": 0, "visible": true}),
    );
    let v = run(
        &mut s,
        "export.movie",
        json!({"out_dir": out3.display().to_string(), "first": 4, "last": 5}),
    );
    assert_eq!(v["frames"], 2);
    assert!(out3.join("frame_0004.png").exists() && !out3.join("frame_0003.png").exists());

    // Joining frames: last of sequence 0 and first of sequence 1 show the same picture.
    let last0 = std::fs::read(out.join("frame_0004.png")).unwrap();
    let first1 = std::fs::read(out.join("frame_0005.png")).unwrap();
    assert_eq!(last0, first1);
}

#[test]
fn movie_export_refuses_when_everything_is_hidden() {
    let d = Dir::new("hidden");
    let mut s = Session::new();
    run(
        &mut s,
        "project.new",
        json!({"image1": d.s("sun.png"), "image2": d.s("moon.png")}),
    );
    run(&mut s, "sequence.visibility", json!({"visible": false}));
    let r = Registry::standard().run(&mut s, "export.movie", &json!({"out_dir": d.s("never")}));
    assert!(r.is_err());
    assert!(!d.0.join("never").exists());
}

#[test]
fn curves_change_the_middle_frame_but_not_the_ends() {
    let d = Dir::new("curve");
    let mut s = Session::new();
    run(
        &mut s,
        "project.new",
        json!({"image1": d.s("sun.png"), "image2": d.s("moon.png")}),
    );
    let (sun, moon) = demo::pair(24, 18).unwrap();
    let render = |s: &mut Session, name: &str, t: f64| {
        let out = d.s(name);
        run(s, "export.frame", json!({"out": out, "t": t}));
        std::fs::read(out).unwrap()
    };
    let plain_mid = render(&mut s, "plain.png", 0.5);
    run(
        &mut s,
        "curve.feature",
        json!({"op": "add", "x": 0.5, "y": 0.9}),
    );
    let curved_mid = render(&mut s, "curved.png", 0.5);
    assert_ne!(plain_mid, curved_mid);
    // The curve is saved with the project.
    run(&mut s, "project.save", json!({"path": d.s("p.inbetween")}));
    let mut again = Session::new();
    run(
        &mut again,
        "project.open",
        json!({"path": d.s("p.inbetween")}),
    );
    assert_eq!(
        again.project().sequences[0].feature_curve,
        s.project().sequences[0].feature_curve
    );
    // Frame 0 and the last frame still match the sources.
    let first = render(&mut s, "first.png", 0.0);
    let last = render(&mut s, "last.png", 1.0);
    let decode = |bytes: &[u8]| inbetween_codecs::decode(bytes).unwrap().data;
    assert_eq!(decode(&first), sun.to_rgba8());
    assert_eq!(decode(&last), moon.to_rgba8());
}
