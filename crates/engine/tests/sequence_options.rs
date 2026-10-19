//! Hold frames and morph types through the command registry.

use inbetween_doc::{ImageRef, MorphType, OutputSettings, Project, Sequence, MAX_HOLD};
use inbetween_engine::{CommandError, Registry, Session};
use serde_json::{json, Value};

fn session() -> Session {
    let mut s = Session::new();
    s.set_project(
        Project {
            output: OutputSettings {
                width: 16,
                height: 12,
            },
            sequences: vec![Sequence {
                image1: ImageRef::new("a.png"),
                image2: ImageRef::new("b.png"),
                frames: 10,
                ..Sequence::default()
            }],
        },
        None,
    );
    s
}

fn run(s: &mut Session, id: &str, params: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, id, &params)
}

#[test]
fn length_sets_morph_and_hold_frames_separately() {
    let mut s = session();
    let v = run(&mut s, "sequence.length", json!({"hold_start": 3})).unwrap();
    assert_eq!(
        (v["frames"].as_u64(), v["hold_start"].as_u64()),
        (Some(10), Some(3))
    );
    let v = run(
        &mut s,
        "sequence.length",
        json!({"hold_end": 5, "frames": 20}),
    )
    .unwrap();
    assert_eq!(v["total_frames"], 28);
    let seq = s.sequence(0).unwrap();
    assert_eq!((seq.frames, seq.hold_start, seq.hold_end), (20, 3, 5));
    // With no parameters it reports.
    let v = run(&mut s, "sequence.length", json!({})).unwrap();
    assert_eq!(v["total_frames"], 28);
}

#[test]
fn bad_lengths_are_refused_and_change_nothing() {
    let mut s = session();
    for params in [
        json!({"hold_start": MAX_HOLD + 1}),
        json!({"hold_end": -1}),
        json!({"frames": 1}),
        json!({"frames": 1000}),
        json!({"hold_start": "lots"}),
    ] {
        let e = run(&mut s, "sequence.length", params.clone()).unwrap_err();
        assert!(matches!(e, CommandError::BadParams(_)), "{params}: {e}");
    }
    let seq = s.sequence(0).unwrap();
    assert_eq!((seq.frames, seq.hold_start, seq.hold_end), (10, 0, 0));
}

#[test]
fn length_with_holds_is_one_undo_step() {
    let mut s = session();
    run(
        &mut s,
        "sequence.length",
        json!({"hold_start": 4, "hold_end": 2}),
    )
    .unwrap();
    run(&mut s, "edit.undo", json!({})).unwrap();
    let seq = s.sequence(0).unwrap();
    assert_eq!((seq.hold_start, seq.hold_end), (0, 0));
    run(&mut s, "edit.redo", json!({})).unwrap();
    assert_eq!(s.sequence(0).unwrap().hold_start, 4);
}

#[test]
fn morph_type_sets_reports_and_rejects() {
    let mut s = session();
    for t in MorphType::ALL {
        let v = run(&mut s, "sequence.morphType", json!({"type": t.id()})).unwrap();
        assert_eq!(v["type"], t.id());
        assert_eq!(v["needs_image2"], t.uses_image2());
        assert_eq!(v["uses_dots"], t.warps());
        assert_eq!(s.sequence(0).unwrap().morph, t);
    }
    let v = run(&mut s, "sequence.morphType", json!({})).unwrap();
    assert_eq!(v["type"], MorphType::ALL[6].id());
    let e = run(&mut s, "sequence.morphType", json!({"type": "melt"})).unwrap_err();
    assert!(e.to_string().contains("one of"), "{e}");
    assert!(matches!(
        run(&mut s, "sequence.morphType", json!({"type": 4})).unwrap_err(),
        CommandError::BadParams(_)
    ));
}

#[test]
fn fit_and_mesh_commands_set_their_options() {
    let mut s = session();
    let v = run(&mut s, "sequence.fit", json!({"fit": "cover"})).unwrap();
    assert_eq!(v["fit"], "cover");
    let v = run(&mut s, "sequence.mesh", json!({"mesh": "per_frame"})).unwrap();
    assert_eq!(v["mesh"], "per_frame");
    assert!(run(&mut s, "sequence.fit", json!({"fit": "zoom"})).is_err());
    assert!(run(&mut s, "sequence.mesh", json!({"mesh": "never"})).is_err());
    let seq = s.sequence(0).unwrap();
    assert_eq!(seq.fit.id(), "cover");
    assert_eq!(seq.mesh.id(), "per_frame");
}

#[test]
fn info_reports_the_holds() {
    let mut s = session();
    run(
        &mut s,
        "sequence.length",
        json!({"hold_start": 1, "hold_end": 2}),
    )
    .unwrap();
    let v = run(&mut s, "project.info", json!({})).unwrap();
    let seq = &v["sequences"][0];
    assert_eq!(
        (&seq["hold_start"], &seq["hold_end"], &seq["total_frames"]),
        (&json!(1), &json!(2), &json!(13))
    );
}
