//! The View, Sequence, Effects, Movie, Face and Help menus, driven headlessly: shell options
//! flip from the menu, Remove All Filters greys out with no movie filters, and Help opens.
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
        .with_size(Vec2::new(1000.0, 720.0))
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

/// Open (or, when it is open, close) a menu of the menu bar, which comes before any other
/// widget with the same label.
fn menu(h: &mut H, name: &str) {
    h.get_all_by_label(name).next().unwrap().click();
    h.run();
}

#[test]
fn view_options_flip_from_the_menu() {
    let mut h = harness();
    let linked = h.state().state.link_sources;
    menu(&mut h, "View");
    shot(&mut h, "view_menu");
    h.get_by_label("Link Image Views").click();
    h.run();
    assert_ne!(h.state().state.link_sources, linked);

    let numbers = h.state().state.show_numbers;
    menu(&mut h, "View");
    h.get_by_label("Dot Numbers").click();
    h.run();
    assert_ne!(h.state().state.show_numbers, numbers);
}

#[test]
fn movie_remove_all_filters_greys_until_there_is_a_filter() {
    let mut h = harness();
    menu(&mut h, "Movie");
    h.get_by_label("Filters").hover();
    h.run();
    assert!(h.get_by_label("Remove All Filters").is_disabled());
    shot(&mut h, "movie_menu_filters");
    menu(&mut h, "Movie");

    h.state_mut()
        .run_command("movie.filter.blur", &json!({}))
        .unwrap();
    h.run();
    menu(&mut h, "Movie");
    h.get_by_label("Filters").hover();
    h.run();
    assert!(!h.get_by_label("Remove All Filters").is_disabled());
}

#[test]
fn sequence_effects_face_and_help_menus_open() {
    let mut h = harness();
    menu(&mut h, "Sequence");
    h.get_by_label("Curves").hover();
    h.run();
    assert!(!h.get_by_label("Easing Curve").is_disabled());
    shot(&mut h, "sequence_menu_curves");
    menu(&mut h, "Sequence");

    menu(&mut h, "Effects");
    h.get_by_label("Effect Camera\u{2026}");
    shot(&mut h, "effects_menu");
    menu(&mut h, "Effects");

    menu(&mut h, "Face");
    h.get_by_label("Smart Drag");
    shot(&mut h, "face_menu");
    menu(&mut h, "Face");

    let help = h.state().state.show_help;
    menu(&mut h, "Help");
    h.get_by_label("Benchmark Rendering");
    shot(&mut h, "help_menu");
    h.get_by_label("Inbetween Help").click();
    h.run();
    assert_ne!(h.state().state.show_help, help);
}
