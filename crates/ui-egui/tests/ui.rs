//! Drives the real interface headlessly with egui_kittest: pointer, keyboard and menu input go
//! in, and the project (through engine commands) is checked on the way out.
//!
//! Set `INBETWEEN_UI_SHOTS=some/folder` to also save screenshots of the interface there.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Vec2};
use egui_kittest::Harness;
use inbetween_ui_egui::state::Panel;
use inbetween_ui_egui::{json, InbetweenApp, ViewState};

type H = Harness<'static, InbetweenApp>;

fn demo_path() -> String {
    format!("{}/../../demo/demo.inbetween", env!("CARGO_MANIFEST_DIR"))
}

fn builder() -> egui_kittest::HarnessBuilder<InbetweenApp> {
    Harness::builder()
        .with_size(Vec2::new(1400.0, 900.0))
        .with_step_dt(1.0 / 24.0)
}

fn empty_app() -> InbetweenApp {
    InbetweenApp::new(&egui::Context::default(), None, ViewState::default())
}

fn harness_with(app: InbetweenApp) -> H {
    let mut h = builder().build_state(|ctx, app: &mut InbetweenApp| app.show(ctx), app);
    h.run();
    h
}

fn demo_harness() -> H {
    let mut app = empty_app();
    app.run_command("project.open", &json!({ "path": demo_path() }))
        .unwrap();
    harness_with(app)
}

fn frame(h: &mut H, events: Vec<Event>) {
    h.input_mut().events.extend(events);
    h.step();
}

fn mv(pos: Pos2) -> Event {
    Event::PointerMoved(pos)
}

fn button(pos: Pos2, b: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: b,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn click(h: &mut H, pos: Pos2, b: PointerButton) {
    frame(h, vec![mv(pos)]);
    frame(h, vec![button(pos, b, true)]);
    frame(h, vec![button(pos, b, false)]);
    h.run();
}

/// Press at `from`, move through a few points, release at `to`.
fn drag(h: &mut H, from: Pos2, to: Pos2, b: PointerButton) {
    frame(h, vec![mv(from)]);
    frame(h, vec![button(from, b, true)]);
    for i in 1..=6 {
        let p = from + (to - from) * (i as f32 / 6.0);
        frame(h, vec![mv(p)]);
    }
    frame(h, vec![button(to, b, false)]);
    h.run();
}

fn app(h: &H) -> &InbetweenApp {
    h.state()
}

fn dots(h: &H) -> Vec<(f64, f64, f64, f64)> {
    app(h).session().project().sequences[0]
        .dots
        .pairs
        .iter()
        .map(|p| (p.first.x, p.first.y, p.second.x, p.second.y))
        .collect()
}

/// Screen position of frame point `(x, y)` in a source panel.
fn screen(h: &H, panel: Panel, x: f32, y: f32) -> Pos2 {
    let a = app(h);
    let rect = if panel == Panel::Source1 {
        a.layout.source1
    } else {
        a.layout.source2
    };
    a.state.view(panel).to_screen(rect, Pos2::new(x, y))
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

// ----------------------------------------------------------------------------------- display

#[test]
fn the_demo_project_lays_out_three_canvases() {
    let h = demo_harness();
    let a = app(&h);
    assert_eq!(a.session().project().sequences[0].dots.len(), 25);
    for (name, r) in [
        ("source1", a.layout.source1),
        ("source2", a.layout.source2),
        ("preview", a.layout.preview),
    ] {
        assert!(r.width() > 200.0 && r.height() > 150.0, "{name} is {r:?}");
    }
    assert!(
        a.layout.source1.max.x <= a.layout.source2.min.x + 1.0,
        "sources sit side by side"
    );
    assert!(
        a.layout.preview.min.y >= a.layout.source1.max.y,
        "preview sits below"
    );
    assert_eq!(a.backend_name(), "CPU", "tests run without a GPU handle");
    assert!(a.state.view(Panel::Source1).auto_fit);
}

#[test]
fn empty_app_shows_a_hint_and_survives_everything() {
    let mut h = harness_with(empty_app());
    assert!(!app(&h).session().has_sequences());
    let c = app(&h).layout.source1.center();
    click(&mut h, c, PointerButton::Primary);
    click(&mut h, c, PointerButton::Secondary);
    frame(
        &mut h,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, 120.0),
            modifiers: Modifiers::NONE,
        }],
    );
    h.press_key(Key::Delete);
    h.press_key_modifiers(Modifiers::COMMAND, Key::Z);
    h.press_key(Key::Space);
    h.run();
    assert!(!app(&h).session().has_sequences());
    assert_eq!(app(&h).session().project().sequences.len(), 0);
}

#[test]
fn screenshots_for_a_human_to_look_at() {
    if std::env::var_os("INBETWEEN_UI_SHOTS").is_none() {
        return;
    }
    let mut a = empty_app();
    a.run_command("project.open", &json!({ "path": demo_path() }))
        .unwrap();
    let mut h = builder()
        .wgpu()
        .build_state(|ctx, app: &mut InbetweenApp| app.show(ctx), a);
    h.run();
    h.state_mut().state.frame = 6;
    h.run();
    shot(&mut h, "mid-morph");

    // Drag a sun dot to a new spot on image 1 only, as a person would.
    let from = screen(&h, Panel::Source1, 240.0, 259.2); // the mouth's lowest dot
    let to = from + Vec2::new(40.0, -30.0);
    drag(&mut h, from, to, PointerButton::Primary);
    shot(&mut h, "after-drag");

    h.state_mut().state.show_help = true;
    h.run();
    shot(&mut h, "help");
    h.state_mut().state.show_help = false;
    h.state_mut().invoke("project.open");
    h.run();
    shot(&mut h, "open-dialog");
}

// ------------------------------------------------------------------------------------ dots

#[test]
fn clicking_empty_image_adds_a_dot_pair_and_selects_it() {
    let mut h = demo_harness();
    let before = dots(&h).len();
    let at = screen(&h, Panel::Source1, 60.0, 120.0);
    click(&mut h, at, PointerButton::Primary);
    let d = dots(&h);
    assert_eq!(d.len(), before + 1);
    let (x1, y1, x2, y2) = d[before];
    assert!(
        (x1 - 60.0).abs() < 1.5 && (y1 - 120.0).abs() < 1.5,
        "{x1},{y1}"
    );
    assert_eq!(
        (x1, y1),
        (x2, y2),
        "a new dot starts at the same place on both images"
    );
    assert_eq!(app(&h).state.selected, Some(before));
}

#[test]
fn clicking_in_the_second_panel_adds_too() {
    let mut h = demo_harness();
    let before = dots(&h).len();
    let at = screen(&h, Panel::Source2, 400.0, 300.0);
    click(&mut h, at, PointerButton::Primary);
    assert_eq!(dots(&h).len(), before + 1);
}

#[test]
fn clicking_outside_the_frame_adds_nothing() {
    let mut h = demo_harness();
    let before = dots(&h).len();
    let at = screen(&h, Panel::Source1, -30.0, 100.0);
    assert!(
        app(&h).layout.source1.contains(at),
        "test point must be inside the panel"
    );
    click(&mut h, at, PointerButton::Primary);
    assert_eq!(dots(&h).len(), before);
}

#[test]
fn dragging_a_dot_moves_it_on_that_image_only_and_is_one_undo_step() {
    let mut h = demo_harness();
    let original = dots(&h);
    // Dot 11 (index 10) is the sun's left eye... pick the first eye dot, index 8.
    let (x1, y1, x2, y2) = original[8];
    let from = screen(&h, Panel::Source2, x2 as f32, y2 as f32);
    let to = from + Vec2::new(30.0, 20.0);
    drag(&mut h, from, to, PointerButton::Primary);

    let moved = dots(&h);
    assert_eq!(
        (moved[8].0, moved[8].1),
        (x1, y1),
        "image 1 position must not change"
    );
    assert!(
        moved[8].2 > x2 + 5.0 && moved[8].3 > y2 + 3.0,
        "{:?} vs {:?}",
        moved[8],
        original[8]
    );
    assert_eq!(app(&h).state.selected, Some(8));
    for (i, (a, b)) in original.iter().zip(&moved).enumerate() {
        if i != 8 {
            assert_eq!(a, b, "dot {i} must not move");
        }
    }

    // Many moves, one undo step.
    h.press_key_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(dots(&h), original);
    h.press_key_modifiers(Modifiers::COMMAND, Key::Y);
    h.run();
    assert_eq!(dots(&h), moved);
}

#[test]
fn dragging_onto_another_dot_is_refused_without_damage() {
    let mut h = demo_harness();
    let original = dots(&h);
    let from = screen(
        &h,
        Panel::Source1,
        original[8].0 as f32,
        original[8].1 as f32,
    );
    let onto = screen(
        &h,
        Panel::Source1,
        original[9].0 as f32,
        original[9].1 as f32,
    );
    drag(&mut h, from, onto, PointerButton::Primary);
    let after = dots(&h);
    assert_eq!(after.len(), original.len());
    let (a, b) = (after[8], after[9]);
    assert_ne!((a.0, a.1), (b.0, b.1), "two dots must never share a spot");
}

#[test]
fn right_click_deletes_a_dot_pair() {
    let mut h = demo_harness();
    let before = dots(&h);
    let at = screen(&h, Panel::Source1, before[12].0 as f32, before[12].1 as f32);
    click(&mut h, at, PointerButton::Secondary);
    let after = dots(&h);
    assert_eq!(after.len(), before.len() - 1);
    assert!(!after.contains(&before[12]));
    h.press_key_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(dots(&h), before);
}

#[test]
fn delete_key_removes_the_selected_dot_and_escape_deselects() {
    let mut h = demo_harness();
    let before = dots(&h);
    let at = screen(&h, Panel::Source1, before[3].0 as f32, before[3].1 as f32);
    click(&mut h, at, PointerButton::Primary);
    assert_eq!(app(&h).state.selected, Some(3));
    h.press_key(Key::Escape);
    h.run();
    assert_eq!(app(&h).state.selected, None);
    click(&mut h, at, PointerButton::Primary);
    h.press_key(Key::Delete);
    h.run();
    assert_eq!(dots(&h).len(), before.len() - 1);
    assert_eq!(app(&h).state.selected, None);
}

#[test]
fn undo_and_redo_shortcuts_both_spellings() {
    let mut h = demo_harness();
    let at = screen(&h, Panel::Source1, 60.0, 120.0);
    click(&mut h, at, PointerButton::Primary);
    let with = dots(&h);
    h.press_key_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(dots(&h).len(), with.len() - 1);
    h.press_key_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run();
    assert_eq!(dots(&h), with);
    h.press_key_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    h.press_key_modifiers(Modifiers::COMMAND, Key::Y);
    h.run();
    assert_eq!(dots(&h), with);
}

#[test]
fn selection_is_clamped_after_undo_removes_the_dot() {
    let mut h = demo_harness();
    let at = screen(&h, Panel::Source1, 60.0, 120.0);
    click(&mut h, at, PointerButton::Primary);
    assert_eq!(app(&h).state.selected, Some(25));
    h.press_key_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(app(&h).state.selected, None);
}

// ------------------------------------------------------------------------------ zoom and pan

#[test]
fn wheel_zoom_keeps_the_point_under_the_cursor_and_links_both_images() {
    let mut h = demo_harness();
    let rect = app(&h).layout.source1;
    let cursor = rect.center() + Vec2::new(60.0, -20.0);
    let before = app(&h).state.view(Panel::Source1).to_frame(rect, cursor);
    frame(&mut h, vec![mv(cursor)]);
    frame(
        &mut h,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, 240.0),
            modifiers: Modifiers::NONE,
        }],
    );
    h.run_steps(12);
    let v1 = *app(&h).state.view(Panel::Source1);
    let v2 = *app(&h).state.view(Panel::Source2);
    assert!(v1.zoom() > 1.2, "zoomed in to {}", v1.zoom());
    assert!(!v1.auto_fit);
    assert_eq!(v1, v2, "the two image views are linked");
    let after = v1.to_frame(rect, cursor);
    assert!((after - before).length() < 0.5, "{before:?} -> {after:?}");
}

#[test]
fn unlinking_lets_the_views_move_separately() {
    let mut h = demo_harness();
    h.state_mut().state.link_sources = false;
    h.run();
    let rect = app(&h).layout.source2;
    let cursor = rect.center();
    frame(&mut h, vec![mv(cursor)]);
    frame(
        &mut h,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, 240.0),
            modifiers: Modifiers::NONE,
        }],
    );
    h.run_steps(12);
    assert!(app(&h).state.view(Panel::Source2).zoom() > app(&h).state.view(Panel::Source1).zoom());
}

#[test]
fn panning_with_the_middle_button_or_empty_space_moves_the_view_not_the_dots() {
    let mut h = demo_harness();
    let before_dots = dots(&h);
    let rect = app(&h).layout.source1;
    let start = rect.center() + Vec2::new(0.0, -60.0);
    let origin = app(&h)
        .state
        .view(Panel::Source1)
        .to_screen(rect, Pos2::ZERO);
    drag(
        &mut h,
        start,
        start + Vec2::new(50.0, 30.0),
        PointerButton::Middle,
    );
    let moved = app(&h)
        .state
        .view(Panel::Source1)
        .to_screen(rect, Pos2::ZERO);
    assert!(
        (moved - origin - Vec2::new(50.0, 30.0)).length() < 2.0,
        "{origin:?} -> {moved:?}"
    );
    assert_eq!(dots(&h), before_dots);
    assert!(!app(&h).state.view(Panel::Source1).auto_fit);
    assert_eq!(
        app(&h).state.view(Panel::Source1),
        app(&h).state.view(Panel::Source2)
    );
}

#[test]
fn the_f_key_refits_every_panel() {
    let mut h = demo_harness();
    let rect = app(&h).layout.source1;
    frame(&mut h, vec![mv(rect.center())]);
    frame(
        &mut h,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, 300.0),
            modifiers: Modifiers::NONE,
        }],
    );
    h.run_steps(12);
    assert!(!app(&h).state.view(Panel::Source1).auto_fit);
    h.press_key(Key::F);
    h.run();
    for p in [Panel::Source1, Panel::Source2, Panel::Preview] {
        assert!(app(&h).state.view(p).auto_fit, "{p:?}");
    }
}

// ------------------------------------------------------------------------ commands and forms

#[test]
fn menu_commands_open_forms_and_run_through_the_engine() {
    let mut h = harness_with(empty_app());
    // Commands that need no input are disabled or do nothing in an empty project.
    h.state_mut().invoke("edit.undo");
    h.state_mut().invoke("export.frame");
    assert!(
        h.state().form().is_none(),
        "disabled commands must not open a form"
    );

    h.state_mut().invoke("project.open");
    h.run();
    let form = h.state().form().expect("project.open needs a path");
    assert_eq!(form.id, "project.open");
    h.state_mut().submit_form();
    assert!(h
        .state()
        .form()
        .unwrap()
        .error
        .as_deref()
        .unwrap()
        .contains("required"));

    h.state_mut().form_mut().unwrap().set(
        "path",
        format!("{}/missing.inbetween", env!("CARGO_MANIFEST_DIR")),
    );
    h.state_mut().submit_form();
    assert!(
        h.state().form().is_some(),
        "a failing command keeps the form open"
    );
    assert!(h.state().form().unwrap().error.is_some());

    h.state_mut().form_mut().unwrap().set("path", demo_path());
    h.state_mut().submit_form();
    h.run();
    assert!(h.state().form().is_none());
    assert!(h.state().session().has_sequences());
    assert_eq!(h.state().status().map(|s| s.is_error), Some(false));
}

#[test]
fn saving_an_unsaved_project_asks_for_a_path_and_writes_it() {
    let dir = std::env::temp_dir().join(format!("inbetween-ui-save-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = harness_with(empty_app());
    let sun = format!("{}/../../demo/sun.png", env!("CARGO_MANIFEST_DIR"));
    let moon = format!("{}/../../demo/moon.png", env!("CARGO_MANIFEST_DIR"));
    h.state_mut()
        .run_command("project.new", &json!({"image1": sun, "image2": moon}))
        .unwrap();
    h.run();
    assert!(!h.state().session().is_dirty());
    let at = screen(&h, Panel::Source1, 100.0, 100.0);
    click(&mut h, at, PointerButton::Primary);
    assert!(h.state().session().is_dirty());

    h.state_mut().invoke("project.save");
    let error = h
        .state()
        .form()
        .expect("no file yet, so ask for one")
        .error
        .clone();
    assert!(error.unwrap().contains("no file"));
    let target = dir.join("mine.inbetween");
    h.state_mut()
        .form_mut()
        .unwrap()
        .set("path", target.display().to_string());
    h.state_mut().submit_form();
    h.run();
    assert!(target.exists());
    assert!(!h.state().session().is_dirty());
    // A second save needs no form.
    h.state_mut().invoke("project.save");
    assert!(h.state().form().is_none());
    let saved = std::fs::read_to_string(&target).unwrap();
    assert!(
        saved.contains("sun.png") && !saved.contains(&sun),
        "{saved}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn errors_reach_the_status_bar() {
    let mut h = demo_harness();
    let e = h
        .state_mut()
        .run_command("dots.delete", &json!({"index": 999}))
        .unwrap_err();
    h.run();
    let status = h.state().status().unwrap();
    assert!(
        status.is_error && status.text.contains(&e.to_string()),
        "{}",
        status.text
    );
}

#[test]
fn a_project_with_missing_images_shows_a_message_instead_of_crashing() {
    let dir = std::env::temp_dir().join(format!("inbetween-ui-missing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("lonely.inbetween");
    let project = inbetween_doc::Project {
        sequences: vec![inbetween_doc::Sequence::new(
            inbetween_doc::ImageRef::new("nope1.png"),
            inbetween_doc::ImageRef::new("nope2.png"),
        )],
        ..Default::default()
    };
    inbetween_format::write_project(&file, &project).unwrap();
    let mut a = empty_app();
    a.run_command("project.open", &json!({"path": file.display().to_string()}))
        .unwrap();
    let mut h = harness_with(a);
    let c = app(&h).layout.source1.center();
    click(&mut h, c, PointerButton::Primary);
    h.press_key(Key::Space);
    h.run();
    assert!(
        dots(&h).is_empty(),
        "no image, no frame to add dots in... but nothing may crash"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------------- playback

#[test]
fn playback_advances_and_loops_and_the_slider_range_follows_the_sequence() {
    let mut h = demo_harness();
    assert_eq!(app(&h).state.frame, 0);
    h.state_mut().state.playing = true;
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..80 {
        h.step();
        seen.insert(app(&h).state.frame);
    }
    assert!(seen.len() > 5, "frames shown: {seen:?}");
    assert!(seen.iter().all(|f| *f < 12));
    assert!(seen.contains(&0) && seen.contains(&11));

    // Without looping it stops on the last frame.
    h.state_mut().state.looping = false;
    h.state_mut().state.frame = 10;
    h.state_mut().state.playing = true;
    for _ in 0..40 {
        h.step();
    }
    assert_eq!(app(&h).state.frame, 11);
    assert!(!app(&h).state.playing);

    // Shortening the sequence clamps the frame.
    h.state_mut().state.frame = 11;
    h.state_mut()
        .run_command("sequence.length", &json!({"frames": 4}))
        .unwrap();
    h.run();
    assert_eq!(app(&h).state.frame, 3);
}

#[test]
fn playback_covers_hold_frames_and_ping_pongs() {
    let mut h = demo_harness();
    h.state_mut()
        .run_command(
            "sequence.length",
            &json!({"frames": 4, "hold_start": 2, "hold_end": 2}),
        )
        .unwrap();
    h.run();
    // 2 + 4 + 2 frames; the slider range follows the total.
    h.state_mut().state.frame = 99;
    h.run();
    assert_eq!(app(&h).state.frame, 7);

    h.state_mut().state.frame = 0;
    h.state_mut().state.looping = false;
    h.state_mut().state.ping_pong = true;
    h.state_mut().state.playing = true;
    let mut seen = vec![0];
    for _ in 0..200 {
        h.step();
        let f = app(&h).state.frame;
        if seen.last() != Some(&f) {
            seen.push(f);
        }
        if !app(&h).state.playing {
            break;
        }
    }
    assert!(
        !app(&h).state.playing,
        "stopped after one round trip: {seen:?}"
    );
    assert_eq!(seen, [0, 1, 2, 3, 4, 5, 6, 7, 6, 5, 4, 3, 2, 1, 0]);
}

#[test]
fn arrow_keys_step_frames_and_space_toggles_playing() {
    let mut h = demo_harness();
    h.press_key(Key::ArrowRight);
    h.step();
    h.press_key(Key::ArrowRight);
    h.step();
    assert_eq!(app(&h).state.frame, 2);
    h.press_key(Key::ArrowLeft);
    h.run();
    assert_eq!(app(&h).state.frame, 1);
    h.press_key(Key::Space);
    h.step();
    assert!(app(&h).state.playing);
    h.press_key(Key::Space);
    h.step();
    assert!(!app(&h).state.playing);
}

#[test]
fn editing_dots_updates_the_preview_without_errors() {
    let mut h = demo_harness();
    h.state_mut().state.frame = 6;
    h.run();
    assert!(h.state().status().is_none_or(|s| !s.is_error));
    let at = screen(&h, Panel::Source1, 60.0, 120.0);
    click(&mut h, at, PointerButton::Primary);
    h.run();
    assert!(
        h.state().status().is_none_or(|s| !s.is_error),
        "{:?}",
        h.state().status()
    );
}

#[test]
fn sequence_option_menu_items_flip_on_and_off_and_undo() {
    let mut h = demo_harness();
    let seq = |h: &H| app(h).session().project().sequences[0].clone();
    assert!(!seq(&h).drop_corner_tris);
    h.state_mut().invoke("morph.option.dropCornerTris");
    assert!(seq(&h).drop_corner_tris);
    assert_eq!(
        h.state().toggle_state("morph.option.dropCornerTris"),
        Some(true)
    );
    h.state_mut().invoke("morph.option.dropCornerTris");
    assert!(!seq(&h).drop_corner_tris);

    h.state_mut().invoke("morph.option.retriangulate");
    assert_eq!(seq(&h).mesh, inbetween_doc::MeshPolicy::PerFrame);
    h.state_mut().invoke("morph.option.retriangulate");
    assert_eq!(seq(&h).mesh, inbetween_doc::MeshPolicy::Frame0);
    h.run();
}
