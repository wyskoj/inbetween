//! The Edit menu, driven headlessly: its three groups, and rows that grey out when they cannot
//! run instead of disappearing.
//!
//! Set `INBETWEEN_UI_SHOTS=some/folder` to also save screenshots of each state there.

use egui::Vec2;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use inbetween_ui_egui::{json, InbetweenApp, ViewState};

type H = Harness<'static, InbetweenApp>;

fn harness() -> H {
    let mut app = InbetweenApp::new(&egui::Context::default(), None, ViewState::default());
    let path = format!("{}/../../demo/demo.inbetween", env!("CARGO_MANIFEST_DIR"));
    app.run_command("project.open", &json!({ "path": path }))
        .unwrap();
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

/// The menu bar's Edit comes first; the Edit dot tool shares the name.
fn open_edit_menu(h: &mut H) {
    h.get_all_by_label("Edit").next().unwrap().click();
    h.run();
}

/// "Delete" is left out: the dot tool of that name is on screen too.
const ROWS: [&str; 7] = [
    "Undo",
    "Redo",
    "Cut",
    "Copy",
    "Paste",
    "Select All Dots",
    "Deselect All",
];

#[test]
fn the_edit_menu_greys_rows_that_cannot_run() {
    let mut h = harness();
    open_edit_menu(&mut h);
    for label in ROWS {
        h.get_by_label(label);
    }
    assert!(h.get_by_label("Copy").is_disabled());
    assert!(h.get_by_label("Deselect All").is_disabled());
    assert!(!h.get_by_label("Select All Dots").is_disabled());
    shot(&mut h, "edit_menu_empty");

    h.get_by_label("Select All Dots").click();
    h.run();
    open_edit_menu(&mut h);
    assert!(!h.get_by_label("Copy").is_disabled());
    assert!(!h.get_by_label("Deselect All").is_disabled());
    shot(&mut h, "edit_menu_selected");
}
