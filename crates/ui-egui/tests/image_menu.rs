//! The Image menu, driven headlessly: Reset Edits greys out until an image has an edit.
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

/// The menu bar's Image comes first.
fn open_image_menu(h: &mut H) {
    h.get_all_by_label("Image").next().unwrap().click();
    h.run();
}

#[test]
fn the_image_menu_greys_reset_until_an_image_is_edited() {
    let mut h = harness();
    h.state_mut()
        .run_command("image.reset", &json!({ "image": 1 }))
        .ok();
    h.state_mut()
        .run_command("image.reset", &json!({ "image": 2 }))
        .ok();
    h.run();
    open_image_menu(&mut h);
    assert!(h.get_by_label("Reset Edits\u{2026}").is_disabled());
    assert!(!h.get_by_label("Replace Image\u{2026}").is_disabled());
    shot(&mut h, "image_menu_unedited");
    // Clicking Image again closes the menu, so the next open starts fresh.
    open_image_menu(&mut h);

    h.state_mut()
        .run_command("image.flip", &json!({ "image": 2, "axis": "horizontal" }))
        .unwrap();
    h.run();
    open_image_menu(&mut h);
    assert!(!h.get_by_label("Reset Edits\u{2026}").is_disabled());
    h.get_by_label("Filters").hover();
    h.run();
    shot(&mut h, "image_menu_filters");
}
