//! The File menu, driven headlessly: its groups, Open Recent with a missing project, the Export
//! submenu's headings, and Close Project and Quit behind the unsaved-changes prompt.
//!
//! Set `INBETWEEN_UI_SHOTS=some/folder` to also save screenshots of each state there.

use egui::Vec2;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use inbetween_ui_egui::{json, InbetweenApp, PromptChoice, ViewState};

type H = Harness<'static, InbetweenApp>;

fn demo(name: &str) -> String {
    format!("{}/../../demo/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn missing() -> String {
    std::env::temp_dir()
        .join("inbetween-file-menu-gone")
        .join("old-test.inbetween")
        .display()
        .to_string()
}

fn harness() -> H {
    let state = ViewState {
        recent: vec![demo("demo.inbetween"), missing()],
        ..ViewState::default()
    };
    let app = InbetweenApp::new(&egui::Context::default(), None, state);
    let mut h = Harness::builder()
        .with_size(Vec2::new(900.0, 640.0))
        .with_step_dt(1.0 / 24.0)
        .build_state(|ctx, app: &mut InbetweenApp| app.show(ctx), app);
    h.run();
    h
}

fn shot(h: &mut H, name: &str) {
    let Some(dir) = std::env::var_os("INBETWEEN_UI_SHOTS") else {
        return;
    };
    match h.render() {
        Ok(image) => {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            image.save(dir.join(format!("{name}.png"))).unwrap();
        }
        Err(e) => println!("SKIP screenshot {name}: {e}"),
    }
}

fn open_file_menu(h: &mut H) {
    h.get_by_label("File").click();
    h.run();
}

#[test]
fn the_file_menu_shows_its_groups_and_open_recent() {
    let mut h = harness();
    open_file_menu(&mut h);
    for label in [
        "New Project",
        "New From Two Images\u{2026}",
        "New From Image List\u{2026}",
        "Open\u{2026}",
        "Save",
        "Save As\u{2026}",
        "Add Sequences From Project\u{2026}",
        "Close Project",
        "Quit",
    ] {
        h.get_by_label(label);
    }
    shot(&mut h, "file_menu");

    h.get_by_label("Open Recent").hover();
    h.run();
    h.get_by_label("demo.inbetween");
    h.get_by_label("old-test.inbetween");
    h.get_by_label("Browse Projects\u{2026}");
    shot(&mut h, "file_menu_recent");

    // A missing project is taken off the list when clicked.
    h.get_by_label("old-test.inbetween").click();
    h.run();
    assert_eq!(h.state().state.recent, vec![demo("demo.inbetween")]);
}

#[test]
fn the_export_submenu_groups_by_scope() {
    let mut h = harness();
    h.state_mut()
        .run_command("project.open", &json!({ "path": demo("demo.inbetween") }))
        .unwrap();
    h.run();
    open_file_menu(&mut h);
    h.get_by_label("Export").hover();
    h.run();
    for label in [
        "Whole movie",
        "Video\u{2026}",
        "Animated Image\u{2026}",
        "This sequence",
        "This frame",
        "Image\u{2026}",
        "Image With Dots\u{2026}",
        "Contact Sheet\u{2026}",
    ] {
        h.get_by_label(label);
    }
    assert_eq!(h.get_all_by_label("Image Sequence\u{2026}").count(), 2);
    shot(&mut h, "file_menu_export");
}

#[test]
fn close_project_asks_first_then_returns_to_the_start_screen() {
    let mut h = harness();
    h.state_mut()
        .run_command("project.open", &json!({ "path": demo("demo.inbetween") }))
        .unwrap();
    h.state_mut()
        .run_command("sequence.addBlank", &json!({}))
        .unwrap();
    h.run();
    assert!(h.state().session().is_dirty());

    open_file_menu(&mut h);
    h.get_by_label("Close Project").click();
    h.run();
    assert!(h.state().prompt_open(), "unsaved changes: ask first");
    assert!(h.state().session().has_sequences());
    shot(&mut h, "close_project_prompt");

    h.state_mut().answer_prompt(PromptChoice::DontSave);
    h.run();
    assert!(!h.state().session().has_sequences());
    assert!(h.state().session().path().is_none());
}

#[test]
fn quit_asks_about_unsaved_changes() {
    let mut h = harness();
    h.state_mut()
        .run_command("project.open", &json!({ "path": demo("demo.inbetween") }))
        .unwrap();
    h.state_mut()
        .run_command("sequence.addBlank", &json!({}))
        .unwrap();
    h.run();
    open_file_menu(&mut h);
    h.get_by_label("Quit").click();
    h.run();
    assert!(h.state().prompt_open());
    h.state_mut().answer_prompt(PromptChoice::Cancel);
    h.run();
    assert!(!h.state().prompt_open());
    assert!(
        h.state().session().has_sequences(),
        "Cancel keeps everything"
    );
}
