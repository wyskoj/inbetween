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

/// The one selected dot, or None when nothing or several are selected.
fn selected(h: &H) -> Option<usize> {
    let sel = app(h).session().selection();
    if sel.len() == 1 {
        sel.iter().next().copied()
    } else {
        None
    }
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
    // The partner is placed where the existing dots carry that point (not necessarily the same).
    assert!(x2.is_finite() && y2.is_finite());
    assert_eq!(selected(&h), Some(before));
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
    assert_eq!(selected(&h), Some(8));
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
    assert_eq!(selected(&h), Some(3));
    h.press_key(Key::Escape);
    h.run();
    assert_eq!(selected(&h), None);
    click(&mut h, at, PointerButton::Primary);
    h.press_key(Key::Delete);
    h.run();
    assert_eq!(dots(&h).len(), before.len() - 1);
    assert_eq!(selected(&h), None);
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
    assert_eq!(selected(&h), Some(25));
    h.press_key_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(selected(&h), None);
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

#[test]
fn file_new_and_the_wizard_replace_the_project_through_the_menu_path() {
    let mut h = demo_harness();
    h.state_mut().invoke("file.new");
    h.run();
    assert!(h.state().form().is_none(), "New needs no input");
    assert_eq!(h.state().session().project().sequences.len(), 1);
    assert!(h.state().session().path().is_none());

    let demo = format!("{}/../../demo", env!("CARGO_MANIFEST_DIR"));
    h.state_mut().invoke("file.wizard");
    h.run();
    let form = h.state().form().expect("the wizard needs images");
    assert_eq!(form.id, "file.wizard");
    h.state_mut().submit_form();
    assert!(h
        .state()
        .form()
        .unwrap()
        .error
        .as_deref()
        .unwrap()
        .contains("required"));
    h.state_mut().form_mut().unwrap().set_list(
        "images",
        vec![format!("{demo}/sun.png"), format!("{demo}/moon.png")],
    );
    shot(&mut h, "wizard_form");
    h.state_mut().submit_form();
    h.run();
    assert!(h.state().form().is_none());
    let seqs = &h.state().session().project().sequences;
    assert_eq!(seqs.len(), 1);
    assert!(seqs[0].image1.0.ends_with("sun.png"));
    assert_eq!(h.state().status().map(|s| s.is_error), Some(false));
}

// ------------------------------------------------------------ unsaved changes, recent, visual

use inbetween_ui_egui::PromptChoice;

/// A copy of the demo project in a scratch folder, so tests may save over it.
fn demo_copy(tag: &str) -> (std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("inbetween-ui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let demo = format!("{}/../../demo", env!("CARGO_MANIFEST_DIR"));
    for f in ["demo.inbetween", "sun.png", "moon.png"] {
        std::fs::copy(format!("{demo}/{f}"), dir.join(f)).unwrap();
    }
    let path = dir.join("demo.inbetween").display().to_string();
    (dir, path)
}

fn dirty_demo(path: &str) -> H {
    let mut app = empty_app();
    app.run_command("project.open", &json!({ "path": path }))
        .unwrap();
    let mut h = harness_with(app);
    let at = screen(&h, Panel::Source1, 100.0, 100.0);
    click(&mut h, at, PointerButton::Primary);
    assert!(h.state().session().is_dirty());
    h
}

/// A project made from the demo images, never saved, with one added dot (so it is dirty).
fn dirty_unsaved() -> H {
    let demo = format!("{}/../../demo", env!("CARGO_MANIFEST_DIR"));
    let mut h = harness_with(empty_app());
    h.state_mut()
        .run_command(
            "project.new",
            &json!({"image1": format!("{demo}/sun.png"), "image2": format!("{demo}/moon.png")}),
        )
        .unwrap();
    h.run();
    let at = screen(&h, Panel::Source1, 100.0, 100.0);
    click(&mut h, at, PointerButton::Primary);
    assert!(h.state().session().is_dirty());
    h
}

#[test]
fn replacing_a_dirty_project_asks_first_and_cancel_keeps_everything() {
    let (dir, path) = demo_copy("prompt");
    let mut h = dirty_demo(&path);
    let before = h.state().session().project().clone();

    h.state_mut().invoke("file.new");
    h.run();
    assert!(h.state().prompt_open());
    assert_eq!(h.state().session().project(), &before);
    shot(&mut h, "unsaved_prompt");

    h.state_mut().answer_prompt(PromptChoice::Cancel);
    h.run();
    assert!(!h.state().prompt_open());
    assert_eq!(h.state().session().project(), &before);
    assert!(h.state().session().is_dirty());

    // The same guard covers opening a project and the wizard form.
    h.state_mut().open_project(&path);
    assert!(h.state().prompt_open());
    h.state_mut().answer_prompt(PromptChoice::Cancel);
    h.state_mut().invoke("file.wizard");
    let images = vec![
        dir.join("sun.png").display().to_string(),
        dir.join("moon.png").display().to_string(),
    ];
    h.state_mut().form_mut().unwrap().set_list("images", images);
    h.state_mut().submit_form();
    assert!(h.state().prompt_open());
    assert!(
        h.state().form().is_some(),
        "the form stays until the prompt is answered"
    );
    h.state_mut().answer_prompt(PromptChoice::Cancel);
    assert_eq!(h.state().session().project(), &before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dont_save_replaces_the_project_without_writing() {
    let (dir, path) = demo_copy("dontsave");
    let on_disk = std::fs::read_to_string(&path).unwrap();
    let mut h = dirty_demo(&path);
    h.state_mut().invoke("file.new");
    h.state_mut().answer_prompt(PromptChoice::DontSave);
    h.run();
    assert!(!h.state().prompt_open());
    assert!(h.state().session().path().is_none());
    assert_eq!(h.state().session().project().sequences.len(), 1);
    assert_eq!(h.state().session().project().sequences[0].dots.len(), 0);
    assert!(!h.state().session().is_dirty());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), on_disk);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_writes_the_file_then_replaces_the_project() {
    let (dir, path) = demo_copy("savechoice");
    let mut h = dirty_demo(&path);
    let dots = h.state().session().project().sequences[0].dots.len();
    h.state_mut().invoke("file.new");
    h.state_mut().answer_prompt(PromptChoice::Save);
    h.run();
    assert!(!h.state().prompt_open());
    assert!(
        h.state().session().path().is_none(),
        "the new project replaced it"
    );
    let saved = inbetween_format::read_project(std::path::Path::new(&path)).unwrap();
    assert_eq!(saved.sequences[0].dots.len(), dots);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_without_a_file_goes_through_save_as_and_then_carries_on() {
    let dir = std::env::temp_dir().join(format!("inbetween-ui-saveas-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = dirty_unsaved();
    h.state_mut().invoke("file.new");
    h.state_mut().answer_prompt(PromptChoice::Save);
    h.run();
    let form = h.state().form().expect("no file yet: Save As opens");
    assert_eq!(form.id, "project.saveAs");
    let target = dir.join("kept.inbetween");
    h.state_mut()
        .form_mut()
        .unwrap()
        .set("path", target.display().to_string());
    h.state_mut().submit_form();
    h.run();
    assert!(target.exists());
    assert!(h.state().form().is_none());
    // The new project has replaced it, and the saved file is on the recent list.
    assert!(h.state().session().path().is_none());
    assert_eq!(
        h.state().state.recent.first().map(String::as_str),
        Some(target.display().to_string().as_str())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_save_as_drops_the_pending_action() {
    let dir = std::env::temp_dir().join(format!("inbetween-ui-cancel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = dirty_unsaved();
    h.state_mut().invoke("file.new");
    h.state_mut().answer_prompt(PromptChoice::Save);
    h.run();
    assert!(h.state().form().is_some());
    frame(
        &mut h,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    h.run();
    assert!(h.state().form().is_none());
    assert!(h.state().session().is_dirty(), "nothing was replaced");
    // A later Save As must not run the stale action.
    h.state_mut().invoke("project.saveAs");
    let target = dir.join("later.inbetween");
    h.state_mut()
        .form_mut()
        .unwrap()
        .set("path", target.display().to_string());
    h.state_mut().submit_form();
    h.run();
    assert_eq!(
        h.state().session().path().map(|p| p.display().to_string()),
        Some(target.display().to_string()),
        "the project was saved, not replaced"
    );
    assert!(!h.state().prompt_open());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn closing_the_window_with_unsaved_changes_asks_first() {
    let (dir, path) = demo_copy("quit");
    let mut h = dirty_demo(&path);
    assert!(!h.state_mut().request_quit());
    assert!(h.state().prompt_open());
    h.state_mut().answer_prompt(PromptChoice::Cancel);
    assert!(!h.state_mut().request_quit(), "still dirty, asks again");
    h.state_mut().answer_prompt(PromptChoice::DontSave);
    assert!(
        h.state_mut().request_quit(),
        "the answer lets the window close"
    );
    // A clean project closes at once.
    let mut clean = demo_harness();
    assert!(clean.state_mut().request_quit());
    assert!(!clean.state().prompt_open());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn opening_and_saving_feed_the_recent_list() {
    let (dir, path) = demo_copy("recent");
    let mut app = empty_app();
    app.run_command("project.open", &json!({ "path": path }))
        .unwrap();
    assert_eq!(app.state.recent, vec![path.clone()]);
    let copy = dir.join("copy.inbetween").display().to_string();
    app.run_command("project.saveAs", &json!({ "path": copy }))
        .unwrap();
    assert_eq!(app.state.recent.len(), 2);
    assert!(app.state.recent[0].ends_with("copy.inbetween"));
    // Opening an older entry again moves it up instead of duplicating it.
    app.run_command("project.open", &json!({ "path": path }))
        .unwrap();
    assert_eq!(app.state.recent[0], path);
    assert_eq!(app.state.recent.len(), 2);
    // A failed open leaves the list alone.
    let before = app.state.recent.clone();
    let missing = dir.join("nope.inbetween").display().to_string();
    assert!(app
        .run_command("project.open", &json!({ "path": missing }))
        .is_err());
    assert_eq!(app.state.recent, before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_open_visual_grid_lists_a_folder_from_thumbnails_and_opens_a_project() {
    let (dir, path) = demo_copy("visual");
    let mut app = empty_app();
    app.run_command("project.open", &json!({ "path": path.clone() }))
        .unwrap();
    // Saving writes the thumbnail the grid shows.
    for name in ["one.inbetween", "two.inbetween"] {
        let target = dir.join(name).display().to_string();
        app.run_command("project.saveAs", &json!({ "path": target }))
            .unwrap();
    }
    std::fs::write(dir.join("broken.inbetween"), b"{nope").unwrap();
    let mut h = harness_with(app);
    h.state_mut().invoke("file.openVisual");
    h.run();
    let v = h.state().open_visual().expect("the window opens");
    assert_eq!(v.tab, inbetween_ui_egui::open_visual::Tab::Recent);
    assert!(v.entries.len() >= 2, "recent projects are listed");

    h.state_mut()
        .browse_visual_folder(&dir.display().to_string());
    h.run();
    let v = h.state().open_visual().unwrap();
    let names: Vec<&str> = v.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["broken", "demo", "one", "two"]);
    assert!(v.entries.iter().filter(|e| e.png.is_some()).count() >= 2);
    assert!(v.entries[0].error.is_some());
    shot(&mut h, "open_visual");

    let one = dir.join("one.inbetween").display().to_string();
    h.state_mut().open_project(&one);
    h.run();
    assert_eq!(
        h.state().session().path().map(|p| p.display().to_string()),
        Some(one)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ----------------------------------------------------------------------------- drag and drop

fn dropped(path: &str) -> egui::DroppedFile {
    egui::DroppedFile {
        path: Some(std::path::PathBuf::from(path)),
        name: std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        ..Default::default()
    }
}

/// Move the pointer to `at`, then deliver `files` as a drop on the next frame.
fn drop_files(h: &mut H, at: Pos2, files: Vec<egui::DroppedFile>) {
    frame(h, vec![mv(at)]);
    h.input_mut().dropped_files.extend(files);
    h.step();
    h.run();
}

#[test]
fn dropping_an_image_on_a_panel_sets_that_image() {
    let (dir, path) = demo_copy("drop-image");
    let other = dir.join("other.png");
    std::fs::copy(dir.join("sun.png"), &other).unwrap();
    let mut h = demo_harness_at(&path);
    let before = h.state().session().project().sequences[0].image1.0.clone();

    let on2 = screen(&h, Panel::Source2, 200.0, 150.0);
    drop_files(&mut h, on2, vec![dropped(&other.display().to_string())]);
    let seq = &h.state().session().project().sequences[0];
    assert!(seq.image2.0.ends_with("other.png"), "{}", seq.image2.0);
    assert_eq!(seq.image1.0, before, "Image 1 is untouched");
    assert!(h.state().session().is_dirty());
    assert_eq!(h.state().status().map(|s| s.is_error), Some(false));

    // On Image 1's panel it goes to Image 1 instead.
    let on1 = screen(&h, Panel::Source1, 200.0, 150.0);
    drop_files(&mut h, on1, vec![dropped(&other.display().to_string())]);
    let seq = &h.state().session().project().sequences[0];
    assert!(seq.image1.0.ends_with("other.png"), "{}", seq.image1.0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dropping_an_image_on_an_empty_app_starts_a_project_with_it() {
    let demo = format!("{}/../../demo", env!("CARGO_MANIFEST_DIR"));
    let mut h = harness_with(empty_app());
    drop_files(
        &mut h,
        Pos2::new(700.0, 450.0),
        vec![dropped(&format!("{demo}/sun.png"))],
    );
    let seqs = &h.state().session().project().sequences;
    assert_eq!(seqs.len(), 1);
    assert!(seqs[0].image1.0.ends_with("sun.png"));
    assert!(seqs[0].image2.0.is_empty());
}

#[test]
fn dropping_a_project_opens_it_after_the_unsaved_prompt() {
    let (dir, path) = demo_copy("drop-project");
    let other = dir.join("other.inbetween");
    std::fs::copy(&path, &other).unwrap();
    let mut h = dirty_demo(&path);
    drop_files(
        &mut h,
        Pos2::new(700.0, 450.0),
        vec![dropped(&other.display().to_string())],
    );
    assert!(h.state().prompt_open(), "unsaved changes: ask first");
    h.state_mut().answer_prompt(PromptChoice::DontSave);
    h.run();
    assert_eq!(
        h.state().session().path().map(|p| p.display().to_string()),
        Some(other.display().to_string())
    );
    assert!(!h.state().session().is_dirty());

    // A clean project opens straight away.
    drop_files(&mut h, Pos2::new(700.0, 450.0), vec![dropped(&path)]);
    assert!(!h.state().prompt_open());
    assert_eq!(
        h.state().session().path().map(|p| p.display().to_string()),
        Some(path)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dropping_a_dot_file_imports_it_into_the_current_sequence() {
    let (dir, path) = demo_copy("drop-dots");
    let dots_file = dir.join("exported.inbetweendots");
    let mut app = empty_app();
    app.run_command("project.open", &json!({ "path": path }))
        .unwrap();
    let count = app.session().project().sequences[0].dots.len();
    app.run_command(
        "dots.export",
        &json!({"path": dots_file.display().to_string()}),
    )
    .unwrap();
    app.run_command("file.new", &json!({})).unwrap();
    assert_eq!(app.session().project().sequences[0].dots.len(), 0);
    let mut h = harness_with(app);
    drop_files(
        &mut h,
        Pos2::new(700.0, 450.0),
        vec![dropped(&dots_file.display().to_string())],
    );
    assert_eq!(h.state().session().project().sequences[0].dots.len(), count);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dropping_several_images_opens_the_wizard_filled_in_drop_order() {
    let (dir, _) = demo_copy("drop-wizard");
    let moon = dir.join("moon.png").display().to_string();
    let sun = dir.join("sun.png").display().to_string();
    let mut h = harness_with(empty_app());
    drop_files(
        &mut h,
        Pos2::new(700.0, 450.0),
        vec![dropped(&moon), dropped(&sun), dropped(&moon)],
    );
    let form = h.state().form().expect("the wizard form opens");
    assert_eq!(form.id, "file.wizard");
    let params = form.to_params().unwrap();
    assert_eq!(params["images"], json!([moon, sun, moon]));
    // Running it builds the chain.
    h.state_mut().submit_form();
    h.run();
    assert_eq!(h.state().session().project().sequences.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unusable_drops_say_so_and_change_nothing() {
    let (dir, path) = demo_copy("drop-bad");
    let mut h = demo_harness_at(&path);
    let before = h.state().session().project().clone();
    let txt = dir.join("notes.txt");
    std::fs::write(&txt, b"hello").unwrap();
    drop_files(
        &mut h,
        Pos2::new(700.0, 450.0),
        vec![dropped(&txt.display().to_string())],
    );
    let status = h.state().status().unwrap();
    assert!(
        status.is_error && status.text.contains("notes.txt"),
        "{}",
        status.text
    );
    assert_eq!(h.state().session().project(), &before);

    // Bytes with no path (a browser drop) are refused politely too.
    drop_files(
        &mut h,
        Pos2::new(700.0, 450.0),
        vec![egui::DroppedFile {
            name: "photo.png".into(),
            bytes: Some(vec![1, 2, 3].into()),
            ..Default::default()
        }],
    );
    assert!(h.state().status().unwrap().is_error);
    assert_eq!(h.state().session().project(), &before);

    // An image file that cannot be decoded is an error from the engine, not a crash.
    let junk = dir.join("junk.png");
    std::fs::write(&junk, b"not an image").unwrap();
    let on2 = screen(&h, Panel::Source2, 200.0, 150.0);
    drop_files(&mut h, on2, vec![dropped(&junk.display().to_string())]);
    assert!(h.state().status().unwrap().is_error);
    assert_eq!(h.state().session().project(), &before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hovering_files_shows_where_they_would_land() {
    let (dir, path) = demo_copy("drop-hover");
    let mut h = demo_harness_at(&path);
    let on2 = screen(&h, Panel::Source2, 200.0, 150.0);
    frame(&mut h, vec![mv(on2)]);
    h.input_mut().hovered_files.push(egui::HoveredFile {
        path: Some(dir.join("sun.png")),
        mime: String::new(),
    });
    h.step();
    h.step();
    shot(&mut h, "drop_hover");
    // Nothing happens until the drop itself.
    assert!(!h.state().session().is_dirty());
    let _ = std::fs::remove_dir_all(&dir);
}

fn demo_harness_at(path: &str) -> H {
    let mut app = empty_app();
    app.run_command("project.open", &json!({ "path": path }))
        .unwrap();
    harness_with(app)
}

#[test]
fn the_preview_follows_a_dot_while_it_is_still_being_dragged() {
    let mut h = demo_harness();
    let original = dots(&h);
    let revision = app(&h).session().revision();
    let (_, _, x2, y2) = original[8];
    let from = screen(&h, Panel::Source2, x2 as f32, y2 as f32);
    frame(&mut h, vec![mv(from)]);
    frame(&mut h, vec![button(from, PointerButton::Primary, true)]);
    let mut seen = vec![revision];
    for i in 1..=4 {
        frame(&mut h, vec![mv(from + Vec2::new(8.0 * i as f32, 5.0 * i as f32))]);
        // Button still down: the project already holds the dot's new place, and the revision
        // the preview is keyed on has moved on.
        let now = app(&h).session().revision();
        assert!(now > *seen.last().unwrap(), "move {i} did not refresh the preview");
        seen.push(now);
    }
    assert_ne!(dots(&h)[8], original[8]);
    frame(&mut h, vec![button(from, PointerButton::Primary, false)]);
    h.run();
}
