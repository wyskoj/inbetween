//! Menu items that end in an ellipsis must open something. Commands whose parameters are all
//! optional used to run with no values, only report, and look like a dead click.

use egui::Vec2;
use egui_kittest::Harness;
use inbetween_ui_egui::{json, InbetweenApp, ViewState};

type H = Harness<'static, InbetweenApp>;

fn harness() -> H {
    let mut app = InbetweenApp::new(&egui::Context::default(), None, ViewState::default());
    let path = format!("{}/../../demo/demo.inbetween", env!("CARGO_MANIFEST_DIR"));
    app.run_command("project.open", &json!({ "path": path }))
        .unwrap();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1000.0, 700.0))
        .build_state(|ctx, app: &mut InbetweenApp| app.show(ctx), app);
    h.run();
    h
}

#[test]
fn set_or_report_menu_items_open_a_form() {
    for id in [
        "sequence.length",
        "sequence.reorder",
        "movie.filter.blur",
        "movie.filter.whirl",
        "sequence.morphType",
        "sequence.fit",
        "project.type",
        "movie.size",
        "movie.fps",
    ] {
        let mut h = harness();
        h.state_mut().invoke(id);
        h.run();
        let form = h.state().form();
        assert_eq!(form.map(|f| f.id), Some(id), "{id} opened no form");
    }
}

#[test]
fn movie_adjust_color_opens_the_adjust_window() {
    let mut h = harness();
    h.state_mut().invoke("movie.adjust");
    h.run();
    assert!(h.state().state.adjust_window.is_some());
}

#[test]
fn detecting_faces_and_benchmarking_say_what_they_found() {
    for id in ["face.detect", "tools.benchmark"] {
        let mut h = harness();
        h.state_mut().invoke(id);
        h.run();
        assert!(
            h.state().toast_text().is_some(),
            "{id} left no message behind"
        );
    }
}
