//! The Dots menu, driven headlessly: transforms grey out without a selection, and an on/off
//! option flips when clicked.
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

/// The menu bar's Dots comes first.
fn open_dots_menu(h: &mut H) {
    h.get_all_by_label("Dots").next().unwrap().click();
    h.run();
}

#[test]
fn the_dots_menu_greys_transforms_and_flips_options() {
    let mut h = harness();
    open_dots_menu(&mut h);
    assert!(h.get_by_label("Scale\u{2026}").is_disabled());
    assert!(h.get_by_label("Flip Horizontally").is_disabled());
    assert!(!h.get_by_label("Clear All Dots").is_disabled());
    shot(&mut h, "dots_menu_empty");

    let before = h.state().session().prefs().auto_jump;
    h.get_by_label("Jump to Partner").click();
    h.run();
    assert_ne!(h.state().session().prefs().auto_jump, before);

    h.state_mut()
        .run_command("dots.selectAll", &json!({}))
        .unwrap();
    h.run();
    open_dots_menu(&mut h);
    assert!(!h.get_by_label("Scale\u{2026}").is_disabled());
    assert!(!h.get_by_label("Flip Vertically").is_disabled());
    shot(&mut h, "dots_menu_selected");
}
