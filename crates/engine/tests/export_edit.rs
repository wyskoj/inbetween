//! `frame.exportEdit`: a frame plus the dots as they sit in it.

use inbetween_doc::{
    Curve, CurvePoint, DotPair, DotSet, ImageRef, OutputSettings, Project, Sequence,
};
use inbetween_engine::{CommandError, Registry, Session};
use inbetween_geom::Point;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("inbetween-xe-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn s(&self, name: &str) -> String {
        self.path(name).display().to_string()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_png(path: &Path, grey: u8) {
    let rgba = [grey, grey, grey, 255].repeat(64 * 48);
    std::fs::write(path, inbetween_codecs::encode_png(64, 48, &rgba).unwrap()).unwrap();
}

fn pairs() -> Vec<DotPair> {
    vec![
        DotPair::new(Point::new(10.0, 10.0), Point::new(20.0, 14.0)),
        DotPair::new(Point::new(50.0, 10.0), Point::new(44.0, 20.0)),
        DotPair::new(Point::new(30.0, 40.0), Point::new(30.0, 30.0)),
        DotPair::fixed(Point::new(5.0, 5.0)),
    ]
}

fn session(dir: &Dir, shape: Curve) -> Session {
    write_png(&dir.path("a.png"), 40);
    write_png(&dir.path("b.png"), 200);
    let mut s = Session::new();
    s.set_project(
        Project {
            output: OutputSettings {
                width: 64,
                height: 48,
            },
            sequences: vec![Sequence {
                image1: ImageRef::new(dir.s("a.png")),
                image2: ImageRef::new(dir.s("b.png")),
                dots: DotSet::new(pairs()),
                frames: 11,
                shape_curve: shape,
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

fn positions(path: &Path) -> Vec<(f64, f64)> {
    inbetween_format::read_dots(path)
        .unwrap()
        .pairs
        .iter()
        .map(|p| {
            assert_eq!(p.first, p.second, "both sides hold the frame's position");
            (p.first.x, p.first.y)
        })
        .collect()
}

fn close(a: &[(f64, f64)], b: &[Point]) {
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b) {
        assert!(
            (x.0 - y.x).abs() < 1e-9 && (x.1 - y.y).abs() < 1e-9,
            "{x:?} vs {y:?}"
        );
    }
}

#[test]
fn the_dots_are_interpolated_to_the_frames_time() {
    let d = Dir::new("interp");
    let mut s = session(&d, Curve::default());
    let dots = DotSet::new(pairs());
    for (params, t) in [
        (json!({"t": 0.0}), 0.0),
        (json!({"t": 1.0}), 1.0),
        (json!({"t": 0.25}), 0.25),
        (json!({"frame": 5}), 0.5),
        (json!({}), 0.0), // frame 0 by default
    ] {
        let mut p = params.as_object().cloned().unwrap_or_default();
        p.insert("out".into(), json!(d.s("f.png")));
        let out = run(&mut s, "frame.exportEdit", Value::Object(p)).unwrap();
        assert_eq!(out["dots"], 4);
        assert_eq!(out["dots_out"], d.s("f.inbetweendots"));
        assert!(d.path("f.png").exists());
        close(
            &positions(&d.path("f.inbetweendots")),
            &dots.interpolated(t),
        );
    }
}

#[test]
fn time_zero_is_image_ones_dots_and_one_is_image_twos() {
    let d = Dir::new("ends");
    let mut s = session(&d, Curve::default());
    let dots = DotSet::new(pairs());
    for (t, expected) in [
        (0.0, dots.first_positions()),
        (1.0, dots.second_positions()),
    ] {
        run(
            &mut s,
            "frame.exportEdit",
            json!({"out": d.s("e.png"), "t": t}),
        )
        .unwrap();
        close(&positions(&d.path("e.inbetweendots")), &expected);
    }
}

#[test]
fn the_shape_curve_decides_how_far_the_dots_have_moved() {
    let d = Dir::new("shape");
    let curve = Curve {
        points: vec![
            CurvePoint::new(0.0, 0.0),
            CurvePoint::new(0.5, 0.9),
            CurvePoint::new(1.0, 1.0),
        ],
        linear: true,
    };
    let mut s = session(&d, curve);
    run(
        &mut s,
        "frame.exportEdit",
        json!({"out": d.s("c.png"), "t": 0.5}),
    )
    .unwrap();
    close(
        &positions(&d.path("c.inbetweendots")),
        &DotSet::new(pairs()).interpolated(0.9),
    );
}

#[test]
fn a_custom_dot_path_is_used_and_the_files_reopen_through_dots_import() {
    let d = Dir::new("reopen");
    let mut s = session(&d, Curve::default());
    run(
        &mut s,
        "frame.exportEdit",
        json!({"out": d.s("mid.png"), "dots_out": d.s("mid.dots.inbetweendots"), "t": 0.5}),
    )
    .unwrap();
    assert!(d.path("mid.dots.inbetweendots").exists());
    assert!(!d.path("mid.inbetweendots").exists());

    // A fresh sequence starting from the exported frame takes the dots back.
    let mut t = Session::new();
    run(&mut t, "file.new", json!({"width": 64, "height": 48})).unwrap();
    run(
        &mut t,
        "sequence.image",
        json!({"image": 1, "path": d.s("mid.png")}),
    )
    .unwrap();
    run(
        &mut t,
        "dots.import",
        json!({"path": d.s("mid.dots.inbetweendots")}),
    )
    .unwrap();
    assert_eq!(t.project().sequences[0].dots.len(), 4);
    close(
        &t.project().sequences[0]
            .dots
            .first_positions()
            .iter()
            .map(|p| (p.x, p.y))
            .collect::<Vec<_>>(),
        &DotSet::new(pairs()).interpolated(0.5),
    );
}

#[test]
fn bad_requests_are_refused_and_write_nothing() {
    let d = Dir::new("bad");
    let mut s = session(&d, Curve::default());
    for params in [
        json!({"out": d.s("x.png"), "t": 0.5, "frame": 3}),
        json!({"out": d.s("x.png"), "t": 2.0}),
        json!({"out": d.s("x.png"), "sequence": 4}),
        json!({"out": d.s("x.bmp")}),
        json!({"out": d.s("no/such/dir/x.png")}),
        json!({"out": d.s("x.png"), "dots_out": d.s("no/such/dir/x.inbetweendots")}),
        json!({}),
    ] {
        assert!(
            run(&mut s, "frame.exportEdit", params.clone()).is_err(),
            "{params}"
        );
    }
    assert!(!d.path("x.png").exists());
    assert!(!d.path("x.inbetweendots").exists());
    // No sequences: disabled, not a crash.
    let mut empty = Session::new();
    assert!(matches!(
        run(&mut empty, "frame.exportEdit", json!({"out": d.s("x.png")})),
        Err(CommandError::Disabled { .. })
    ));
}
