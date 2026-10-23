//! M18: zoom keys, layouts, splitters, the movie-wide preview, scrubbing and the frame strip,
//! driven through the real interface.
//!
//! Set `INBETWEEN_UI_SHOTS=some/folder` to also save screenshots there.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use inbetween_ui_egui::state::{Panel, PreviewScope, ViewMode};
use inbetween_ui_egui::{json, InbetweenApp, ViewState};

type H = Harness<'static, InbetweenApp>;

fn demo_dir() -> String {
    format!("{}/../../demo", env!("CARGO_MANIFEST_DIR"))
}

fn harness_with(app: InbetweenApp) -> H {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 900.0))
        .with_step_dt(1.0 / 24.0)
        .build_state(|ctx, app: &mut InbetweenApp| app.show(ctx), app);
    h.run();
    h
}

fn demo_app() -> InbetweenApp {
    let mut app = InbetweenApp::new(&egui::Context::default(), None, ViewState::default());
    app.run_command(
        "project.open",
        &json!({ "path": format!("{}/demo.inbetween", demo_dir()) }),
    )
    .unwrap();
    app
}

fn demo_harness() -> H {
    harness_with(demo_app())
}

/// The demo with a second, continued sequence.
fn two_sequences() -> H {
    let mut app = demo_app();
    app.run_command(
        "sequence.add",
        &json!({ "image2": format!("{}/sun.png", demo_dir()), "name": "Second" }),
    )
    .unwrap();
    harness_with(app)
}

fn app(h: &H) -> &InbetweenApp {
    h.state()
}

fn frame(h: &mut H, events: Vec<Event>) {
    h.input_mut().events.extend(events);
    h.step();
}

fn move_to(h: &mut H, pos: Pos2) {
    frame(h, vec![Event::PointerMoved(pos)]);
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn click(h: &mut H, pos: Pos2) {
    move_to(h, pos);
    frame(h, vec![button(pos, true)]);
    frame(h, vec![button(pos, false)]);
    h.run();
}

fn drag(h: &mut H, from: Pos2, to: Pos2) {
    move_to(h, from);
    frame(h, vec![button(from, true)]);
    for i in 1..=6 {
        move_to(h, from + (to - from) * (i as f32 / 6.0));
    }
    frame(h, vec![button(to, false)]);
    h.run();
}

fn shot(h: &mut H, name: &str) {
    let Some(dir) = std::env::var_os("INBETWEEN_UI_SHOTS") else {
        return;
    };
    if let Ok(image) = h.render() {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        image.save(dir.join(format!("{name}.png"))).unwrap();
    }
}

const CTRL_ALT: Modifiers = Modifiers {
    alt: true,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};

#[test]
fn zoom_keys_step_the_ladder_fit_and_set_100_percent() {
    let mut h = demo_harness();
    let c = app(&h).layout.source1.center();
    move_to(&mut h, c);
    h.press_key_modifiers(Modifiers::COMMAND, Key::Equals);
    h.run();
    let z = app(&h).state.view(Panel::Source1).zoom;
    assert!([0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0].contains(&z), "{z}");
    assert!(!app(&h).state.view(Panel::Source1).auto_fit);
    // The linked second source follows.
    assert_eq!(app(&h).state.view(Panel::Source2).zoom, z);
    h.press_key_modifiers(Modifiers::COMMAND, Key::Minus);
    h.run();
    assert!(app(&h).state.view(Panel::Source1).zoom < z);

    h.press_key_modifiers(CTRL_ALT, Key::Num0);
    h.run();
    assert_eq!(app(&h).state.view(Panel::Source1).zoom, 1.0);

    h.press_key_modifiers(Modifiers::COMMAND, Key::Num0);
    h.run();
    assert!(app(&h).state.view(Panel::Source1).auto_fit);

    // Fit width and fit height fill one axis of the panel.
    h.press_key_modifiers(Modifiers::ALT, Key::ArrowRight);
    h.run();
    let a = app(&h);
    let (rect, view) = (a.layout.source1, *a.state.view(Panel::Source1));
    let frame_w = a.session().project().output.width as f32;
    assert!((frame_w * view.zoom() - (rect.width() - 20.0)).abs() < 1.0);
    h.press_key_modifiers(Modifiers::ALT, Key::ArrowDown);
    h.run();
    let a = app(&h);
    let (rect, view) = (a.layout.source1, *a.state.view(Panel::Source1));
    let frame_h = a.session().project().output.height as f32;
    assert!((frame_h * view.zoom() - (rect.height() - 20.0)).abs() < 1.0);
}

#[test]
fn the_wheel_reaches_1600_percent_but_the_keys_stop_at_800() {
    let mut h = demo_harness();
    let c = app(&h).layout.source1.center();
    move_to(&mut h, c);
    for _ in 0..12 {
        h.press_key_modifiers(Modifiers::COMMAND, Key::Equals);
        h.run();
    }
    assert_eq!(app(&h).state.view(Panel::Source1).zoom, 8.0);
    for _ in 0..30 {
        frame(
            &mut h,
            vec![Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, 400.0),
                modifiers: Modifiers::NONE,
            }],
        );
    }
    assert_eq!(app(&h).state.view(Panel::Source1).zoom, 16.0);
}

#[test]
fn the_preview_can_follow_the_sources() {
    let mut h = demo_harness();
    h.state_mut().run_action("view.syncPreview");
    h.run();
    let c = app(&h).layout.source1.center();
    move_to(&mut h, c);
    h.press_key_modifiers(Modifiers::COMMAND, Key::Equals);
    h.run();
    let a = app(&h);
    assert_eq!(
        a.state.view(Panel::Preview).zoom,
        a.state.view(Panel::Source1).zoom
    );
    // And the other way round: zooming the preview moves the sources.
    let p = a.layout.preview.center();
    move_to(&mut h, p);
    h.press_key_modifiers(Modifiers::COMMAND, Key::Equals);
    h.run();
    let a = app(&h);
    assert_eq!(
        a.state.view(Panel::Source1).zoom,
        a.state.view(Panel::Preview).zoom
    );
}

#[test]
fn function_keys_switch_layouts_and_f11_asks_for_fullscreen() {
    let mut h = demo_harness();
    assert_eq!(app(&h).state.mode, ViewMode::Full);
    assert!(app(&h).layout.preview.width() > 0.0);

    h.press_key(Key::F6);
    h.run();
    assert_eq!(app(&h).state.mode, ViewMode::Edit);
    assert!(app(&h).layout.preview.width() == 0.0);
    assert!(app(&h).layout.source1.height() > 500.0);

    h.press_key(Key::F7);
    h.run();
    assert_eq!(app(&h).state.mode, ViewMode::Preview);
    assert!(app(&h).layout.source1.width() == 0.0);
    assert!(app(&h).layout.preview.height() > 500.0);
    shot(&mut h, "mode_preview");

    h.press_key(Key::F8);
    h.step();
    assert_eq!(app(&h).state.mode, ViewMode::Frames);
    assert!(app(&h).layout.source1.width() == 0.0);
    shot(&mut h, "mode_frames");

    h.press_key(Key::F9);
    h.run();
    assert_eq!(app(&h).state.mode, ViewMode::Dual);
    assert!(app(&h).layout.source1.width() > 0.0);
    shot(&mut h, "mode_dual");

    h.press_key(Key::F5);
    h.run();
    assert_eq!(app(&h).state.mode, ViewMode::Full);
    assert!(app(&h).layout.preview.width() > 0.0);

    // Fullscreen is a viewport request; the shell must not choke on it either way.
    h.press_key(Key::F11);
    h.run();
    h.press_key_modifiers(Modifiers::ALT, Key::Enter);
    h.run();
    assert_eq!(app(&h).state.mode, ViewMode::Full);
}

#[test]
fn the_view_menu_lists_the_actions_with_their_keys() {
    let mut h = demo_harness();
    h.get_by_label("View").click();
    h.run();
    for label in [
        "Zoom In",
        "Best Fit",
        "Fit Width",
        "Frame Sequence",
        "Dual Window",
        "Full Screen",
    ] {
        assert!(
            h.query_by_label_contains(label).is_some(),
            "View menu lacks {label}"
        );
    }
    h.get_by_label_contains("Frame Sequence").click();
    h.step();
    h.step();
    assert_eq!(app(&h).state.mode, ViewMode::Frames);
}

#[test]
fn dragging_a_splitter_resizes_and_is_remembered() {
    let mut h = demo_harness();
    let a = app(&h);
    let top = a.state.top_fraction;
    let s1 = a.layout.source1;
    // The handle between the sources and the preview.
    let handle = Pos2::new(s1.center().x, s1.max.y + 3.0);
    drag(&mut h, handle, handle + Vec2::new(0.0, 120.0));
    let a = app(&h);
    assert!(
        a.state.top_fraction > top + 0.05,
        "{}",
        a.state.top_fraction
    );
    assert!(a.layout.source1.height() > s1.height());

    // The handle between Image 1 and Image 2.
    let s1 = a.layout.source1;
    let left = a.state.left_fraction;
    let handle = Pos2::new(s1.max.x + 3.0, s1.center().y);
    drag(&mut h, handle, handle - Vec2::new(150.0, 0.0));
    let a = app(&h);
    assert!(
        a.state.left_fraction < left - 0.05,
        "{}",
        a.state.left_fraction
    );
    assert!(a.layout.source1.width() < s1.width());

    // Dragging far never makes a panel vanish.
    let handle = Pos2::new(a.layout.source1.max.x + 3.0, a.layout.source1.center().y);
    drag(&mut h, handle, handle - Vec2::new(5000.0, 0.0));
    assert!(app(&h).state.left_fraction >= 0.2);
    assert!(app(&h).layout.source1.width() > 100.0);

    // The fractions survive a restart through the saved state.
    let saved = serde_json::to_string(&app(&h).state).unwrap();
    let back: ViewState = serde_json::from_str(&saved).unwrap();
    assert_eq!(back.top_fraction, app(&h).state.top_fraction);
    assert_eq!(back.left_fraction, app(&h).state.left_fraction);
}

#[test]
fn the_movie_scope_plays_across_the_join_and_follows_the_playhead() {
    let mut h = two_sequences();
    assert_eq!(
        app(&h).state.sequence,
        1,
        "the continued sequence is selected"
    );
    assert_eq!(app(&h).state.preview_scope, PreviewScope::Sequence);
    h.state_mut().run_action("view.previewScope");
    h.run();
    assert_eq!(app(&h).state.preview_scope, PreviewScope::Movie);

    // First frame of the movie is in sequence 0.
    h.get_by_label("First frame").click();
    h.run();
    assert_eq!((app(&h).state.sequence, app(&h).state.frame), (0, 0));
    // Last frame is the last of sequence 1.
    h.get_by_label("Last frame").click();
    h.run();
    let last = app(&h).session().project().sequences[1].total_frames() - 1;
    assert_eq!((app(&h).state.sequence, app(&h).state.frame), (1, last));

    // Playing from the start visits both sequences, in order, without stopping.
    h.get_by_label("First frame").click();
    h.run();
    h.state_mut().state.looping = false;
    h.state_mut().state.playing = true;
    let mut order = vec![(app(&h).state.sequence, app(&h).state.frame)];
    for _ in 0..80 {
        h.step();
        let at = (app(&h).state.sequence, app(&h).state.frame);
        if order.last() != Some(&at) {
            order.push(at);
        }
    }
    let total: usize = app(&h)
        .session()
        .project()
        .sequences
        .iter()
        .map(|s| s.total_frames() as usize)
        .sum();
    assert_eq!(
        order.len(),
        total,
        "every movie frame shown once: {order:?}"
    );
    assert!(order.windows(2).all(|w| w[0] < w[1]), "in order: {order:?}");
    assert_eq!(order.first().map(|o| o.0), Some(0));
    assert_eq!(order.last().map(|o| o.0), Some(1));
    assert!(!app(&h).state.playing, "stopped at the end");

    // Back to the sequence scope: the frame is the selected sequence's own.
    h.state_mut().run_action("view.previewScope");
    h.run();
    assert_eq!(app(&h).state.preview_scope, PreviewScope::Sequence);
    assert_eq!(app(&h).state.sequence, 1);
}

#[test]
fn a_layer_project_previews_the_stacked_movie() {
    let mut h = two_sequences();
    h.state_mut()
        .run_command("project.type", &json!({ "kind": "layer" }))
        .unwrap();
    h.state_mut().run_action("view.previewScope");
    h.run();
    h.get_by_label("Last frame").click();
    h.run();
    let longest = app(&h)
        .session()
        .project()
        .sequences
        .iter()
        .map(|s| s.total_frames())
        .max()
        .unwrap();
    assert_eq!(app(&h).state.frame, longest - 1);
    assert!(app(&h).status().is_none_or(|s| !s.is_error));
    shot(&mut h, "layer_movie_preview");
}

#[test]
fn scrubbing_by_percent_and_the_wheel_over_the_bar() {
    let mut h = demo_harness();
    h.state_mut().state.scrub_percent = true;
    h.run();
    shot(&mut h, "scrub_percent");
    // The wheel over the transport bar steps one frame.
    let bar = centre(h.get_by_label("Last frame")) + Vec2::new(120.0, 0.0);
    move_to(&mut h, bar);
    let before = app(&h).state.frame;
    frame(
        &mut h,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, -50.0),
            modifiers: Modifiers::NONE,
        }],
    );
    h.step();
    assert_eq!(app(&h).state.frame, before + 1);
    frame(
        &mut h,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, 50.0),
            modifiers: Modifiers::NONE,
        }],
    );
    h.step();
    assert_eq!(app(&h).state.frame, before);
}

#[test]
fn the_frame_rate_is_a_project_setting_the_transport_follows() {
    let mut h = demo_harness();
    assert_eq!(app(&h).session().project().fps, 24.0);
    h.state_mut()
        .run_command("movie.fps", &json!({ "fps": 30 }))
        .unwrap();
    h.run();
    assert_eq!(app(&h).state.fps, 30.0);
    // One undo step takes it back.
    h.state_mut().run_command("edit.undo", &json!({})).unwrap();
    h.run();
    assert_eq!(app(&h).state.fps, 24.0);
}

#[test]
fn the_frame_strip_shows_thumbnails_and_a_click_moves_the_playhead() {
    let mut h = demo_harness();
    h.press_key(Key::F8);
    for _ in 0..30 {
        h.step();
    }
    shot(&mut h, "frame_strip");
    // Click above the label "4": that is frame index 3's thumbnail.
    let label = centre(h.get_by_label("4"));
    click(&mut h, label - Vec2::new(0.0, 30.0));
    assert_eq!(app(&h).state.frame, 3);
    assert_eq!(app(&h).state.mode, ViewMode::Frames);
    // A double click goes back to the full layout at that frame.
    let label = centre(h.get_by_label("6"));
    let at = label - Vec2::new(0.0, 30.0);
    move_to(&mut h, at);
    frame(&mut h, vec![button(at, true)]);
    frame(&mut h, vec![button(at, false)]);
    frame(&mut h, vec![button(at, true)]);
    frame(&mut h, vec![button(at, false)]);
    h.run();
    assert_eq!(app(&h).state.frame, 5);
    assert_eq!(app(&h).state.mode, ViewMode::Full);
}

#[test]
fn the_checkerboard_can_be_switched_off_and_everything_still_draws() {
    let mut h = demo_harness();
    // Dropping the corner triangles leaves transparent areas for the board to show through.
    h.state_mut()
        .run_command("morph.option.dropCornerTris", &json!({"on": true}))
        .unwrap();
    h.run();
    assert!(app(&h).state.checkerboard);
    shot(&mut h, "checkerboard_on");
    h.state_mut().run_action("view.toggle.checkerboard");
    h.run();
    assert!(!app(&h).state.checkerboard);
    shot(&mut h, "checkerboard_off");
    // Saved state keeps it.
    let saved = serde_json::to_string(&app(&h).state).unwrap();
    let back: ViewState = serde_json::from_str(&saved).unwrap();
    assert!(!back.checkerboard);
}

#[test]
fn hostile_saved_state_is_repaired() {
    let mut state: ViewState = serde_json::from_str(
        r#"{"left_fraction": 99, "strip_size": -4, "mode": "frames", "preview_scope": "movie"}"#,
    )
    .unwrap();
    state.sanitize();
    assert!((0.2..=0.8).contains(&state.left_fraction));
    assert!((48.0..=256.0).contains(&state.strip_size));
    state.left_fraction = f32::NAN;
    state.strip_size = f32::INFINITY;
    state.sanitize();
    assert_eq!(state.left_fraction, 0.5);
    // An empty app with a saved Frames and Movie state starts without trouble.
    let mut app = InbetweenApp::new(&egui::Context::default(), None, state);
    let _ = app.run_action("view.zoomIn");
    let mut h = harness_with(app);
    h.run();
    assert!(!app_has_project(&h));
}

fn app_has_project(h: &H) -> bool {
    app(h).session().has_sequences()
}

/// Where a widget found by its accessibility label sits on screen.
fn centre(node: egui_kittest::kittest::Node<'_>) -> Pos2 {
    let r = node.bounding_box().unwrap();
    Pos2::new(((r.x0 + r.x1) / 2.0) as f32, ((r.y0 + r.y1) / 2.0) as f32)
}

#[test]
fn undo_inside_the_adjust_window_keeps_the_rest_of_the_session_one_step() {
    let mut h = demo_harness();
    let depth = |h: &H| h.state().session().undo_depth();
    let start = depth(&h);
    h.state_mut().open_adjust(1);
    let adjust = |h: &mut H, p: serde_json::Value| {
        let mut p = p;
        p["image"] = json!(1);
        h.state_mut().run_command("image.adjust", &p).unwrap();
    };
    adjust(&mut h, json!({"brightness": 10}));
    h.state_mut().run_command("edit.undo", &json!({})).unwrap();
    // Two more changes after the undo are still a single step.
    adjust(&mut h, json!({"brightness": 20}));
    adjust(&mut h, json!({"contrast": 5}));
    h.state_mut().close_adjust(true);
    assert_eq!(depth(&h), start + 1);
    h.state_mut().run_command("edit.undo", &json!({})).unwrap();
    assert!(h.state().session().project().sequences[0]
        .edit1
        .adjust
        .is_identity());
}

#[test]
fn the_scrubber_keeps_its_width_as_the_counters_change() {
    let mut h = demo_harness();
    let last = 40;
    let mut widths = Vec::new();
    for f in 0..last {
        h.state_mut().state.frame = f;
        h.run();
        let r = h
            .get_by_role_and_label(egui::accesskit::Role::Slider, "Frame")
            .bounding_box()
            .unwrap();
        widths.push((r.x1 - r.x0) as f32);
    }
    assert!(
        widths.windows(2).all(|w| (w[0] - w[1]).abs() < 0.01),
        "scrubber width changed: {widths:?}"
    );
}
