//! `movie.adjust` and `movie.filter.*` at movie and sequence scope.

use inbetween_doc::{DotPair, DotSet, ImageRef, OutputSettings, Project, Sequence};
use inbetween_engine::{CommandError, Registry, Session};
use inbetween_geom::Point;
use serde_json::{json, Value};
use std::path::PathBuf;

const W: u32 = 48;
const H: u32 = 36;

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn session(tag: &str) -> (Session, Dir) {
    let dir = std::env::temp_dir().join(format!("inbetween-mflt-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut rgba = Vec::new();
    for y in 0..H {
        for x in 0..W {
            let edge = (x / 6 + y / 6) % 2 == 0;
            let v = if edge { 230 } else { 25 };
            rgba.extend_from_slice(&[v, (x * 5) as u8, (y * 6) as u8, 255]);
        }
    }
    let png = inbetween_codecs::encode_png(W, H, &rgba).unwrap();
    for name in ["a.png", "b.png"] {
        std::fs::write(dir.join(name), &png).unwrap();
    }
    let corners = [(0.0, 0.0), (47.0, 0.0), (0.0, 35.0), (47.0, 35.0)];
    let dots = DotSet::new(
        corners
            .iter()
            .map(|&(x, y)| DotPair::new(Point::new(x, y), Point::new(x, y)))
            .collect(),
    );
    let mut s = Session::new();
    s.set_project(
        Project {
            output: OutputSettings {
                width: W,
                height: H,
            },
            sequences: vec![Sequence {
                image1: ImageRef::new(dir.join("a.png").display().to_string()),
                image2: ImageRef::new(dir.join("b.png").display().to_string()),
                dots,
                frames: 3,
                ..Sequence::default()
            }],
            ..Project::default()
        },
        None,
    );
    (s, Dir(dir))
}

fn run(s: &mut Session, id: &str, params: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, id, &params)
}

fn frame(s: &mut Session) -> Vec<u8> {
    s.render_movie_frame(2).unwrap().to_rgba8()
}

const FILTERS: [&str; 13] = [
    "smooth",
    "blur",
    "gaussian",
    "zoomBlur",
    "radialBlur",
    "sharpen",
    "emboss",
    "mosaic",
    "oilPaint",
    "splash",
    "cylinder",
    "whirl",
    "pinch",
];

#[test]
fn every_filter_changes_the_exported_frame_at_both_scopes() {
    for scope in ["movie", "sequence"] {
        for name in FILTERS {
            let (mut s, _d) = session(&format!("{scope}-{name}"));
            let base = frame(&mut s);
            let id = format!("movie.filter.{name}");
            let report = run(&mut s, &id, json!({"scope": scope, "sequence": 0})).unwrap();
            assert_eq!(report["scope"], scope, "{id}");
            assert_eq!(report["filters"].as_array().map(Vec::len), Some(1), "{id}");
            let filtered = frame(&mut s);
            assert_ne!(base, filtered, "{id} at {scope} scope changed nothing");
            let held = match scope {
                "movie" => s.project().filters.len(),
                _ => s.project().sequences[0].filters.len(),
            };
            assert_eq!(held, 1);
            run(&mut s, "movie.filter.clear", json!({"scope": scope})).unwrap();
            assert_eq!(frame(&mut s), base, "{id}: clear did not restore the frame");
        }
    }
}

#[test]
fn neutral_adjust_and_cleared_filters_leave_the_frame_alone() {
    let (mut s, _d) = session("neutral");
    let base = frame(&mut s);
    run(&mut s, "movie.adjust", json!({})).unwrap();
    assert_eq!(frame(&mut s), base);
    run(&mut s, "movie.adjust", json!({"brightness": 40, "grayscale": true})).unwrap();
    assert_ne!(frame(&mut s), base);
    run(&mut s, "movie.adjust", json!({"reset": true})).unwrap();
    assert_eq!(frame(&mut s), base);
    assert!(s.project().adjust.is_identity());
}

#[test]
fn the_order_of_the_list_is_respected() {
    let (mut s, _d) = session("order");
    run(&mut s, "movie.filter.mosaic", json!({"cell": 8})).unwrap();
    run(&mut s, "movie.filter.gaussian", json!({"sigma": 2})).unwrap();
    let mosaic_then_blur = frame(&mut s);
    run(&mut s, "movie.filter.move", json!({"index": 1, "to": 0})).unwrap();
    assert_eq!(s.project().filters[0].name(), "gaussian");
    let blur_then_mosaic = frame(&mut s);
    assert_ne!(mosaic_then_blur, blur_then_mosaic);
    run(&mut s, "movie.filter.remove", json!({"index": 0})).unwrap();
    assert_eq!(s.project().filters.len(), 1);
    // Replacing in place.
    run(&mut s, "movie.filter.blur", json!({"index": 0, "radius": 2})).unwrap();
    assert_eq!(s.project().filters[0].name(), "blur");
}

#[test]
fn filters_and_adjust_undo_and_redo() {
    let (mut s, _d) = session("undo");
    let base = frame(&mut s);
    run(&mut s, "movie.filter.blur", json!({"radius": 3})).unwrap();
    run(&mut s, "movie.adjust", json!({"negative": true})).unwrap();
    run(&mut s, "movie.filter.blur", json!({"scope": "sequence", "radius": 2})).unwrap();
    run(&mut s, "edit.undo", json!({})).unwrap();
    assert!(s.project().sequences[0].filters.is_empty());
    run(&mut s, "edit.undo", json!({})).unwrap();
    assert!(s.project().adjust.is_identity());
    run(&mut s, "edit.undo", json!({})).unwrap();
    assert_eq!(frame(&mut s), base);
    run(&mut s, "edit.redo", json!({})).unwrap();
    assert_eq!(s.project().filters.len(), 1);
}

#[test]
fn bad_parameters_are_errors_and_change_nothing() {
    let (mut s, _d) = session("bad");
    for (id, params) in [
        ("movie.filter.blur", json!({"radius": 0})),
        ("movie.filter.blur", json!({"radius": 1000})),
        ("movie.filter.gaussian", json!({"sigma": "x"})),
        ("movie.filter.remove", json!({"index": 0})),
        ("movie.filter.move", json!({"index": 0, "to": 1})),
        ("movie.filter.blur", json!({"index": 3})),
        ("movie.filter.blur", json!({"scope": "image"})),
        ("movie.filter.blur", json!({"scope": "sequence", "sequence": 9})),
        ("movie.adjust", json!({"hue": 500})),
    ] {
        assert!(run(&mut s, id, params.clone()).is_err(), "{id} {params}");
    }
    assert!(s.project().filters.is_empty() && s.project().adjust.is_identity());
    // The sixteen-filter limit.
    for _ in 0..16 {
        run(&mut s, "movie.filter.blur", json!({})).unwrap();
    }
    assert!(run(&mut s, "movie.filter.blur", json!({})).is_err());
    assert_eq!(s.project().filters.len(), 16);
}

#[test]
fn a_saved_project_keeps_its_movie_filters() {
    let (mut s, d) = session("save");
    run(&mut s, "movie.filter.mosaic", json!({"cell": 6})).unwrap();
    run(&mut s, "movie.adjust", json!({"contrast": 30})).unwrap();
    let before = frame(&mut s);
    let file = d.0.join("p.inbetween").display().to_string();
    run(&mut s, "project.save", json!({"path": file})).unwrap();
    let mut again = Session::new();
    run(&mut again, "project.open", json!({"path": file})).unwrap();
    assert_eq!(frame(&mut again), before);
}
