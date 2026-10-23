//! A running export leaves a small copy of its latest frame for a progress window.

use inbetween_engine::{Registry, Session, FRAME_PREVIEW_SIDE};
use serde_json::json;

fn demo() -> String {
    format!("{}/../../demo/demo.inbetween", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn an_export_leaves_a_small_frame_and_taking_it_empties_the_slot() {
    let registry = Registry::standard();
    let mut session = Session::default();
    registry
        .run(&mut session, "project.open", &json!({ "path": demo() }))
        .unwrap();
    assert!(
        session.take_frame_preview().is_none(),
        "nothing exported yet"
    );
    let dir = std::env::temp_dir().join(format!("inbetween-fp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    registry
        .run(
            &mut session,
            "export.movie",
            &json!({ "out_dir": dir.display().to_string() }),
        )
        .unwrap();
    let frame = session.take_frame_preview().expect("a frame was kept");
    assert!(frame.width >= 1 && frame.width <= FRAME_PREVIEW_SIDE);
    assert!(frame.height >= 1 && frame.height <= FRAME_PREVIEW_SIDE);
    assert_eq!(frame.rgba.len(), (frame.width * frame.height * 4) as usize);
    assert!(session.take_frame_preview().is_none(), "taking empties it");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_frame_slot_shared_with_a_thread_sees_the_same_frame() {
    let registry = Registry::standard();
    let mut session = Session::default();
    registry
        .run(&mut session, "project.open", &json!({ "path": demo() }))
        .unwrap();
    let slot = session.frame_preview_handle();
    let dir = std::env::temp_dir().join(format!("inbetween-fp2-{}", std::process::id()));
    registry
        .run(
            &mut session,
            "export.sequence",
            &json!({ "out_dir": dir.display().to_string() }),
        )
        .unwrap();
    assert!(slot.lock().unwrap().is_some());
    let _ = std::fs::remove_dir_all(&dir);
}
