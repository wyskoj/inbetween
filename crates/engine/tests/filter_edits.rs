//! `image.filter.*`: appending, replacing, removing, reordering and rendering filters.

use inbetween_doc::{DotPair, DotSet, Filter, ImageRef, OutputSettings, Project, Sequence};
use inbetween_engine::{CommandError, Registry, Session};
use inbetween_geom::Point;
use inbetween_raster::filter;
use serde_json::{json, Value};
use std::path::PathBuf;

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn session(tag: &str) -> (Dir, Session) {
    let dir = std::env::temp_dir().join(format!("inbetween-fe-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut rgba = Vec::new();
    for y in 0..40u32 {
        for x in 0..56u32 {
            rgba.extend_from_slice(&[(x * 4) as u8, (y * 6) as u8, ((x ^ y) * 9) as u8, 255]);
        }
    }
    let png = inbetween_codecs::encode_png(56, 40, &rgba).unwrap();
    std::fs::write(dir.join("a.png"), &png).unwrap();
    std::fs::write(dir.join("b.png"), &png).unwrap();
    let mut s = Session::new();
    s.set_project(
        Project {
            output: OutputSettings {
                width: 56,
                height: 40,
            },
            sequences: vec![Sequence {
                image1: ImageRef::new(dir.join("a.png").display().to_string()),
                image2: ImageRef::new(dir.join("b.png").display().to_string()),
                dots: DotSet::new(vec![DotPair::fixed(Point::new(5.0, 5.0))]),
                frames: 3,
                ..Sequence::default()
            }],
            ..Project::default()
        },
        None,
    );
    (Dir(dir), s)
}

fn run(s: &mut Session, id: &str, params: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, id, &params)
}

fn filters(s: &Session, which: u8) -> Vec<Filter> {
    s.sequence(0).unwrap().edit(which).filters.clone()
}

#[test]
fn every_filter_command_appends_its_filter_with_defaults_and_parameters() {
    let (_d, mut s) = session("append");
    let cases: Vec<(&str, Value, Filter)> = vec![
        ("smooth", json!({}), Filter::Smooth { strength: 1.0 }),
        ("blur", json!({"radius": 5}), Filter::Blur { radius: 5 }),
        ("gaussian", json!({}), Filter::Gaussian { sigma: 4.0 }),
        (
            "zoomBlur",
            json!({"amount": 0.5, "cx": 0.25}),
            Filter::ZoomBlur {
                cx: 0.25,
                cy: 0.5,
                amount: 0.5,
            },
        ),
        (
            "radialBlur",
            json!({"angle": 30}),
            Filter::RadialBlur {
                cx: 0.5,
                cy: 0.5,
                angle: 30.0,
            },
        ),
        (
            "sharpen",
            json!({"amount": 2}),
            Filter::Sharpen {
                amount: 2.0,
                radius: 1.5,
            },
        ),
        (
            "emboss",
            json!({"angle": 90, "depth": 2, "base": 0.4}),
            Filter::Emboss {
                angle: 90.0,
                depth: 2.0,
                base: 0.4,
            },
        ),
        ("mosaic", json!({"cell": 6}), Filter::Mosaic { cell: 6 }),
        (
            "oilPaint",
            json!({"radius": 2}),
            Filter::OilPaint {
                radius: 2,
                levels: 32,
            },
        ),
        (
            "splash",
            json!({"radius": 7, "seed": 42}),
            Filter::Splash {
                radius: 7,
                seed: 42,
            },
        ),
        (
            "cylinder",
            json!({"vertical": true, "amount": -0.4}),
            Filter::Cylinder {
                vertical: true,
                amount: -0.4,
            },
        ),
        (
            "whirl",
            json!({"radius": 20, "angle": -45}),
            Filter::Whirl {
                cx: 0.5,
                cy: 0.5,
                radius: 20.0,
                angle: -45.0,
            },
        ),
        (
            "pinch",
            json!({"radius": 25, "amount": -0.5}),
            Filter::Pinch {
                cx: 0.5,
                cy: 0.5,
                radius: 25.0,
                amount: -0.5,
            },
        ),
    ];
    let mut want = Vec::new();
    for (name, params, filter) in cases {
        run(&mut s, &format!("image.filter.{name}"), params)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        want.push(filter);
        assert_eq!(filters(&s, 1), want, "after {name}");
    }
    assert!(
        s.sequence(0).unwrap().edit2.is_identity(),
        "Image 2 untouched"
    );
    // They all run: render the source through the whole stack.
    let img = s.source(0, 1).unwrap();
    assert_eq!(img.size(), (56, 40));
    assert!(img.as_slice().iter().all(|v| v.is_finite()));
}

#[test]
fn index_replaces_remove_and_move_reorder_and_clear_empties() {
    let (_d, mut s) = session("manage");
    run(&mut s, "image.filter.blur", json!({"radius": 2})).unwrap();
    run(&mut s, "image.filter.mosaic", json!({"cell": 4})).unwrap();
    run(
        &mut s,
        "image.filter.gaussian",
        json!({"sigma": 1, "image": 2}),
    )
    .unwrap();
    run(
        &mut s,
        "image.filter.blur",
        json!({"radius": 9, "index": 0}),
    )
    .unwrap();
    assert_eq!(
        filters(&s, 1),
        vec![Filter::Blur { radius: 9 }, Filter::Mosaic { cell: 4 }]
    );
    run(&mut s, "image.filter.move", json!({"index": 0, "to": 1})).unwrap();
    assert_eq!(
        filters(&s, 1),
        vec![Filter::Mosaic { cell: 4 }, Filter::Blur { radius: 9 }]
    );
    run(&mut s, "image.filter.remove", json!({"index": 0})).unwrap();
    assert_eq!(filters(&s, 1), vec![Filter::Blur { radius: 9 }]);
    assert_eq!(filters(&s, 2).len(), 1);
    run(&mut s, "image.filter.clear", json!({})).unwrap();
    assert!(filters(&s, 1).is_empty());
    assert_eq!(filters(&s, 2).len(), 1, "clear is per image");
    run(
        &mut s,
        "image.reset",
        json!({"part": "filters", "image": 2}),
    )
    .unwrap();
    assert!(s.sequence(0).unwrap().edit2.is_identity());
}

#[test]
fn bad_indexes_bounds_and_the_cap_are_refused_without_changes() {
    let (_d, mut s) = session("bad");
    run(&mut s, "image.filter.blur", json!({})).unwrap();
    let before = s.sequence(0).unwrap().clone();
    for (id, params) in [
        ("image.filter.remove", json!({"index": 1})),
        ("image.filter.remove", json!({"index": -1})),
        ("image.filter.move", json!({"index": 0, "to": 5})),
        ("image.filter.blur", json!({"index": 3})),
        ("image.filter.blur", json!({"radius": 0})),
        ("image.filter.blur", json!({"radius": 101})),
        ("image.filter.gaussian", json!({"sigma": 0.1})),
        ("image.filter.mosaic", json!({"cell": 1})),
        ("image.filter.whirl", json!({"radius": 1e12})),
    ] {
        let err = run(&mut s, id, params.clone());
        assert!(err.is_err(), "{id} {params}");
        assert_eq!(*s.sequence(0).unwrap(), before, "{id} {params}");
    }
    for _ in 1..16 {
        run(&mut s, "image.filter.smooth", json!({})).unwrap();
    }
    assert_eq!(filters(&s, 1).len(), 16);
    let err = run(&mut s, "image.filter.smooth", json!({}))
        .unwrap_err()
        .to_string();
    assert!(err.contains("16"), "{err}");
    assert_eq!(filters(&s, 1).len(), 16);
}

#[test]
fn the_rendered_frame_is_the_raster_op_applied_to_the_source() {
    let (_d, mut s) = session("render");
    let raw = s.source(0, 1).unwrap();
    run(&mut s, "image.filter.gaussian", json!({"sigma": 2})).unwrap();
    run(&mut s, "image.filter.mosaic", json!({"cell": 5})).unwrap();
    let want = filter::mosaic(&filter::gaussian(&raw, 2.0).unwrap(), 5).unwrap();
    let got = s.source(0, 1).unwrap();
    assert_eq!(*got, want);
    let prepared = s.prepare(0).unwrap();
    let frame0 = prepared.renderer().unwrap().render_t(0.0).unwrap();
    let max = frame0
        .as_slice()
        .iter()
        .zip(want.as_slice())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(max < 1e-4, "frame 0 shows the filtered Image 1 ({max})");
}

#[test]
fn undo_removes_a_filter_and_the_cache_serves_the_redo() {
    let (_d, mut s) = session("undo");
    let raw = s.source(0, 1).unwrap();
    run(&mut s, "image.filter.blur", json!({"radius": 3})).unwrap();
    let blurred = s.source(0, 1).unwrap();
    run(&mut s, "edit.undo", json!({})).unwrap();
    assert!(std::sync::Arc::ptr_eq(&raw, &s.source(0, 1).unwrap()));
    run(&mut s, "edit.redo", json!({})).unwrap();
    assert!(std::sync::Arc::ptr_eq(&blurred, &s.source(0, 1).unwrap()));
}
