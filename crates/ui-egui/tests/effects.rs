//! Effects in the shell: menu entries come from the registry, and the preview draws them.

use egui::Vec2;
use egui_kittest::Harness;
use inbetween_ui_egui::{json, InbetweenApp, ViewState};

fn demo_dir() -> String {
    format!("{}/../../demo", env!("CARGO_MANIFEST_DIR"))
}

fn harness() -> Harness<'static, InbetweenApp> {
    let mut app = InbetweenApp::new(&egui::Context::default(), None, ViewState::default());
    app.run_command(
        "project.open",
        &json!({ "path": format!("{}/demo.inbetween", demo_dir()) }),
    )
    .unwrap();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 900.0))
        .build_state(|ctx, app: &mut InbetweenApp| app.show(ctx), app);
    h.run();
    h
}

#[test]
fn effect_and_export_commands_are_in_the_menus() {
    let r = inbetween_engine::Registry::standard();
    for (id, menu) in [
        ("effect.mask", "Effects/"),
        ("effect.backColor", "Effects/"),
        ("effect.background", "Effects/"),
        ("effect.foreground", "Effects/"),
        ("export.animation", "File/Export/"),
        ("export.video", "File/Export/"),
    ] {
        let spec = r.specs().iter().find(|s| s.id == id).unwrap();
        assert!(spec.menu.starts_with(menu), "{id}: {}", spec.menu);
    }
}

#[test]
fn effects_edit_through_the_shell_and_undo() {
    let mut h = harness();
    let sun = format!("{}/sun.png", demo_dir());
    let on = |h: &Harness<'static, InbetweenApp>| {
        h.state().session().project().sequences[0]
            .effects
            .any_active()
    };
    h.state_mut()
        .run_command("effect.backColor", &json!({ "color": "#00ff00" }))
        .unwrap();
    h.state_mut()
        .run_command("effect.foreground", &json!({ "image": sun, "opacity": 50 }))
        .unwrap();
    h.run(); // the preview renders with effects without a GPU or a crash
    assert!(on(&h));
    // A bad file is refused and leaves the effects as they were.
    assert!(h
        .state_mut()
        .run_command("effect.mask", &json!({ "image": "no-such-file.png" }))
        .is_err());
    h.state_mut().run_command("edit.undo", &json!({})).unwrap();
    h.state_mut().run_command("edit.undo", &json!({})).unwrap();
    h.run();
    assert!(!on(&h));
}
