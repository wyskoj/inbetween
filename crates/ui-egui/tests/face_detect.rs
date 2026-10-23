//! The Face menu in the shell: Mark Face detects, and a missing face shows its message in
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
fn the_face_menu_lists_the_marking_commands_and_not_detect() {
    let specs = inbetween_engine::Registry::standard().specs().to_vec();
    let labels: Vec<_> = specs
        .iter()
        .filter(|s| s.menu.starts_with("Face/"))
        .map(|s| (s.id, s.label))
        .collect();
    for (id, label) in [
        ("face.locate", "Mark Face"),
        ("face.locateAll", "Mark Several Faces"),
        ("face.locateSequences", "Mark Face in All Sequences"),
    ] {
        assert!(
            labels.contains(&(id, label)),
            "{id} is not in the Face menu as {label}"
        );
    }
    assert!(labels.iter().all(|(id, _)| *id != "face.detect"));
    let detect = specs.iter().find(|s| s.id == "face.detect").unwrap();
    assert_eq!(detect.menu, "");
}

#[test]
fn tooltips_show_the_first_sentence() {
    use inbetween_ui_egui::first_sentence;
    assert_eq!(
        first_sentence("Put dots on it. More detail."),
        "Put dots on it."
    );
    assert_eq!(first_sentence("No full stop"), "No full stop");
    assert_eq!(
        first_sentence("A .inbetween file. Next."),
        "A .inbetween file."
    );
    let all = inbetween_engine::Registry::standard();
    let spec = all.get("face.locate").unwrap();
    assert_eq!(
        first_sentence(spec.description),
        "Put dots on the face in both images."
    );
}

#[test]
fn mark_face_on_an_empty_sequence_runs_without_asking() {
    let mut h = portraits_harness();
    h.state_mut().invoke("face.locate");
    h.run();
    assert!(!h.state().replace_dots_open());
    assert!(h.state().form().is_none());
    assert_eq!(h.state().session().project().sequences[0].dots.len(), 38);
}

#[test]
fn mark_face_asks_before_replacing_dots() {
    let mut h = portraits_harness();
    h.state_mut().invoke("face.locate");
    h.run();
    h.state_mut().invoke("face.locate");
    h.run();
    assert!(h.state().replace_dots_open());
    shot(&mut h, "face_replace_dots");
    // Cancel keeps the dots and the dialog goes away.
    h.state_mut().answer_replace_dots(false);
    h.run();
    assert!(!h.state().replace_dots_open());
    assert_eq!(h.state().session().project().sequences[0].dots.len(), 38);
    // Replace marks the face again: still one face of dots.
    h.state_mut().invoke("face.locate");
    h.state_mut().answer_replace_dots(true);
    h.run();
    assert_eq!(h.state().session().project().sequences[0].dots.len(), 38);
}

#[test]
fn mark_face_in_all_sequences_asks_when_any_sequence_has_dots() {
    let mut h = portraits_harness();
    h.state_mut().invoke("face.locateSequences");
    h.run();
    assert!(!h.state().replace_dots_open());
    assert_eq!(h.state().session().project().sequences[0].dots.len(), 38);
    h.state_mut().invoke("face.locateSequences");
    h.run();
    assert!(h.state().replace_dots_open());
}

#[test]
fn a_missing_face_opens_the_form_with_the_message() {
    let mut h = portraits_harness();
    // A project whose images show no face: the message appears in the form.
    let demo = format!("{}/../../demo/demo.inbetween", env!("CARGO_MANIFEST_DIR"));
    h.state_mut()
        .run_command("project.open", &json!({ "path": demo }))
        .unwrap();
    h.state_mut().invoke("face.locate");
    if h.state().replace_dots_open() {
        h.state_mut().answer_replace_dots(true);
    }
    h.run();
    shot(&mut h, "face_locate_no_face");
    let form = h.state().form().expect("the form opens with the error");
    assert_eq!(form.id, "face.locate");
    let message = form.error.clone().unwrap_or_default();
    assert!(message.contains("no face found in image"), "{message}");
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
