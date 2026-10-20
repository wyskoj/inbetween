//! The Face menu in the shell: Locate Face detects, and a missing face shows its message in
//! the form (where a face box can be given instead).

use egui::Vec2;
use egui_kittest::Harness;
use inbetween_ui_egui::{json, InbetweenApp, ViewState};

fn portraits_harness() -> Harness<'static, InbetweenApp> {
    let data = format!("{}/../detect/tests/data", env!("CARGO_MANIFEST_DIR"));
    let mut app = InbetweenApp::new(&egui::Context::default(), None, ViewState::default());
    app.run_command(
        "project.new",
        &json!({"image1": format!("{data}/portrait_a.png"), "image2": format!("{data}/portrait_b.png")}),
    )
    .unwrap();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 900.0))
        .build_state(|ctx, app: &mut InbetweenApp| app.show(ctx), app);
    h.run();
    h
}

#[test]
fn the_face_menu_lists_the_detection_commands() {
    let ids: Vec<_> = inbetween_engine::Registry::standard()
        .specs()
        .iter()
        .filter(|s| s.menu.starts_with("Face/"))
        .map(|s| s.id)
        .collect();
    for id in ["face.locate", "face.locateAll", "face.detect"] {
        assert!(ids.contains(&id), "{id} is not in the Face menu");
    }
}

#[test]
fn locate_face_detects_and_no_face_opens_the_form_with_the_message() {
    let mut h = portraits_harness();
    h.state_mut().invoke("face.locate");
    h.run();
    assert!(h.state().form().is_none());
    assert_eq!(h.state().session().project().sequences[0].dots.len(), 38);

    // A project whose images show no face: the message appears in the form.
    let demo = format!("{}/../../demo/demo.inbetween", env!("CARGO_MANIFEST_DIR"));
    h.state_mut()
        .run_command("project.open", &json!({ "path": demo }))
        .unwrap();
    h.state_mut().invoke("face.locate");
    h.run();
    shot(&mut h, "face_locate_no_face");
    let form = h.state().form().expect("the form opens with the error");
    assert_eq!(form.id, "face.locate");
    let message = form.error.clone().unwrap_or_default();
    assert!(message.contains("no face found in image"), "{message}");
    assert!(h.state().session().project().sequences[0].dots.len() != 38);
}

/// With `INBETWEEN_UI_SHOTS=folder`, save what the no-face form looks like.
fn shot(h: &mut Harness<'static, InbetweenApp>, name: &str) {
    let Some(dir) = std::env::var_os("INBETWEEN_UI_SHOTS") else {
        return;
    };
    if let Ok(image) = h.render() {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        image.save(dir.join(format!("{name}.png"))).unwrap();
    }
}
