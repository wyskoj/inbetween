//! Command feedback: the toast that says what a command did, driven headlessly.
//!
//! Set `INBETWEEN_UI_SHOTS=some/folder` to also save screenshots of each state there.

use egui::Vec2;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use inbetween_ui_egui::kit::ToastKind;
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
    settle(&mut h);
    h
}

/// Let the toast from opening the demo go.
fn settle(h: &mut H) {
    for _ in 0..24 * 12 {
        h.step();
    }
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

fn toast(h: &H) -> Option<(String, ToastKind)> {
    h.state().toast_text().map(|(t, k)| (t.to_string(), k))
}

#[test]
fn a_command_that_changes_the_project_says_what_it_did_and_offers_undo() {
    let mut h = harness();
    h.state_mut()
        .run_command("dots.selectAll", &json!({}))
        .unwrap();
    h.state_mut().run_command("edit.copy", &json!({})).unwrap();
    h.state_mut().run_command("edit.paste", &json!({})).unwrap();
    h.run();
    let (text, kind) = toast(&h).expect("a toast");
    assert!(text.starts_with("Pasted "), "{text}");
    assert_eq!(kind, ToastKind::Success);
    h.get_by_label("Undo");
    shot(&mut h, "toast_pasted");

    h.get_by_label("Undo").click();
    h.run();
    assert_eq!(
        toast(&h).map(|t| t.0).as_deref(),
        Some("Undid the last change")
    );
}

#[test]
fn a_failed_command_toasts_the_error() {
    let mut h = harness();
    let _ = h
        .state_mut()
        .run_command("project.open", &json!({ "path": "no/such/file.inbetween" }));
    h.run();
    let (_, kind) = toast(&h).expect("a toast");
    assert_eq!(kind, ToastKind::Error);
    shot(&mut h, "toast_error");
}

#[test]
fn a_toast_fades_away_by_itself() {
    let mut h = harness();
    h.state_mut()
        .run_command("dots.clearAll", &json!({}))
        .unwrap();
    h.run();
    assert!(toast(&h).is_some());
    settle(&mut h);
    assert!(toast(&h).is_none());
}

#[test]
fn quiet_commands_do_not_toast() {
    let mut h = harness();
    h.state_mut()
        .run_command("dots.selectAll", &json!({}))
        .unwrap();
    h.run();
    assert!(toast(&h).is_none());
}
