//! Key-dot editing through the real interface: tools, marquee, same-position add, lines,
//! nudging, clipboard, overlays and the loupe.
//!
//! Set `INBETWEEN_UI_SHOTS=some/folder` to also save screenshots there.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Vec2};
use egui_kittest::Harness;
use inbetween_ui_egui::state::{Panel, Tool};
use inbetween_ui_egui::{json, InbetweenApp, ViewState};

type H = Harness<'static, InbetweenApp>;

fn demo_path() -> String {
    format!("{}/../../demo/demo.inbetween", env!("CARGO_MANIFEST_DIR"))
}

fn harness() -> H {
    let mut app = InbetweenApp::new(&egui::Context::default(), None, ViewState::default());
    app.run_command("project.open", &json!({ "path": demo_path() }))
        .unwrap();
    app.run_command("dots.clearAll", &json!({})).unwrap();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 900.0))
        .with_step_dt(1.0 / 24.0)
        .build_state(|ctx, app: &mut InbetweenApp| app.show(ctx), app);
    h.run();
    h
}

fn frame(h: &mut H, events: Vec<Event>) {
    h.input_mut().events.extend(events);
    h.step();
}

/// Like `frame`, with the modifier keys held for that frame.
fn frame_m(h: &mut H, events: Vec<Event>, modifiers: Modifiers) {
    h.input_mut().modifiers = modifiers;
    frame(h, events);
}

fn btn(pos: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    }
}

fn click(h: &mut H, pos: Pos2, modifiers: Modifiers) {
    frame_m(h, vec![Event::PointerMoved(pos)], modifiers);
    frame_m(h, vec![btn(pos, true, modifiers)], modifiers);
    frame_m(h, vec![btn(pos, false, modifiers)], modifiers);
    h.input_mut().modifiers = Modifiers::NONE;
    h.run();
}

fn click_at(h: &mut H, panel: Panel, x: f32, y: f32, modifiers: Modifiers) {
    let at = screen(h, panel, x, y);
    click(h, at, modifiers);
}

fn drag(h: &mut H, from: Pos2, to: Pos2, modifiers: Modifiers) {
    frame_m(h, vec![Event::PointerMoved(from)], modifiers);
    frame_m(h, vec![btn(from, true, modifiers)], modifiers);
    for i in 1..=6 {
        let p = from + (to - from) * (i as f32 / 6.0);
        frame_m(h, vec![Event::PointerMoved(p)], modifiers);
    }
    frame_m(h, vec![btn(to, false, modifiers)], modifiers);
    h.input_mut().modifiers = Modifiers::NONE;
    h.run();
}

fn key(h: &mut H, key: Key, modifiers: Modifiers) {
    frame_m(
        h,
        vec![Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }],
        modifiers,
    );
    h.input_mut().modifiers = Modifiers::NONE;
    h.run();
}

fn screen(h: &H, panel: Panel, x: f32, y: f32) -> Pos2 {
    let a = h.state();
    let rect = if panel == Panel::Source1 {
        a.layout.source1
    } else {
        a.layout.source2
    };
    a.state.view(panel).to_screen(rect, Pos2::new(x, y))
}

fn cmd(h: &mut H, id: &str, p: serde_json::Value) {
    h.state_mut()
        .run_command(id, &p)
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run();
}

fn add3(h: &mut H) {
    // Partners differ from the first positions, so "same position" is observable.
    for (x, y, x2, y2) in [
        (100, 100, 130, 110),
        (200, 100, 230, 120),
        (400, 300, 420, 330),
    ] {
        cmd(h, "dots.add", json!({"x": x, "y": y, "x2": x2, "y2": y2}));
    }
}

fn dots(h: &H) -> Vec<inbetween_doc::DotPair> {
    h.state().session().project().sequences[0]
        .dots
        .pairs
        .clone()
}

fn selection(h: &H) -> Vec<usize> {
    h.state().session().selection().iter().copied().collect()
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

#[test]
fn marquee_selects_and_shift_adds_alt_subtracts() {
    let mut h = harness();
    add3(&mut h);
    h.state_mut().state.tool = Tool::Edit;
    h.run();
    let (a, b) = (
        screen(&h, Panel::Source1, 60.0, 60.0),
        screen(&h, Panel::Source1, 260.0, 160.0),
    );
    drag(&mut h, a, b, Modifiers::NONE);
    assert_eq!(selection(&h), [0, 1]);
    shot(&mut h, "key_dots_marquee");
    let (c, d) = (
        screen(&h, Panel::Source1, 350.0, 250.0),
        screen(&h, Panel::Source1, 450.0, 350.0),
    );
    drag(&mut h, c, d, Modifiers::SHIFT);
    assert_eq!(selection(&h), [0, 1, 2]);
    drag(&mut h, a, b, Modifiers::ALT);
    assert_eq!(selection(&h), [2]);
    // A click on empty image clears; clicking a dot selects just it.
    click_at(&mut h, Panel::Source1, 300.0, 50.0, Modifiers::NONE);
    assert!(selection(&h).is_empty());
    click_at(&mut h, Panel::Source1, 200.0, 100.0, Modifiers::NONE);
    assert_eq!(selection(&h), [1]);
}

#[test]
fn dragging_a_selected_dot_moves_the_whole_selection_as_one_undo_step() {
    let mut h = harness();
    add3(&mut h);
    h.state_mut().state.tool = Tool::Edit;
    cmd(&mut h, "dots.select", json!({"indices": [0, 1]}));
    let before = dots(&h);
    let from = screen(&h, Panel::Source1, 100.0, 100.0);
    let to = screen(&h, Panel::Source1, 120.0, 130.0);
    drag(&mut h, from, to, Modifiers::NONE);
    let after = dots(&h);
    for i in [0, 1] {
        assert!(
            (after[i].first.x - before[i].first.x - 20.0).abs() < 2.0,
            "dot {i}: {before:?} {after:?}"
        );
        assert_eq!(after[i].second, before[i].second);
    }
    assert_eq!(after[2], before[2]);
    cmd(&mut h, "edit.undo", json!({}));
    assert_eq!(dots(&h), before);
}

#[test]
fn alt_click_puts_the_partner_at_the_same_position() {
    let mut h = harness();
    add3(&mut h);
    let at = screen(&h, Panel::Source1, 300.0, 200.0);
    click(&mut h, at, Modifiers::ALT);
    let d = dots(&h);
    assert_eq!(d.len(), 4);
    assert_eq!(d[3].first, d[3].second);
    // A plain click maps the partner through the existing dots instead.
    click_at(&mut h, Panel::Source1, 250.0, 150.0, Modifiers::NONE);
    let d = dots(&h);
    assert_eq!(d.len(), 5);
    assert!(d[4].first != d[4].second);
}

#[test]
fn unpaired_dots_when_auto_partner_is_off_then_completed_on_the_other_image() {
    let mut h = harness();
    cmd(&mut h, "dots.option.autoPartner", json!({"on": false}));
    click_at(&mut h, Panel::Source1, 100.0, 100.0, Modifiers::NONE);
    assert!(!dots(&h)[0].is_paired());
    click_at(&mut h, Panel::Source2, 140.0, 110.0, Modifiers::NONE);
    let d = dots(&h);
    assert_eq!(d.len(), 1);
    assert!(d[0].is_paired());
    assert_eq!(
        (d[0].second.x.round(), d[0].second.y.round()),
        (140.0, 110.0)
    );
}

#[test]
fn line_tool_draws_a_chain_and_shift_click_ends_it() {
    let mut h = harness();
    h.state_mut().state.tool = Tool::Line;
    h.run();
    for (x, y) in [(100.0, 100.0), (200.0, 120.0)] {
        click_at(&mut h, Panel::Source1, x, y, Modifiers::NONE);
    }
    click_at(&mut h, Panel::Source1, 300.0, 200.0, Modifiers::SHIFT);
    let dots_now = h.state().session().project().sequences[0].dots.clone();
    assert_eq!((dots_now.len(), dots_now.lines.len()), (3, 2));
    // The chain ended, so the next click starts a new one with no line to the old dot.
    click_at(&mut h, Panel::Source1, 400.0, 300.0, Modifiers::NONE);
    assert_eq!(
        h.state().session().project().sequences[0].dots.lines.len(),
        2
    );
    shot(&mut h, "key_dots_lines");
    // Each click was one undo step.
    cmd(&mut h, "edit.undo", json!({}));
    assert_eq!(dots(&h).len(), 3);
    cmd(&mut h, "edit.undo", json!({}));
    assert_eq!(dots(&h).len(), 2);
}

#[test]
fn dragging_a_line_in_the_edit_tool_moves_its_group() {
    let mut h = harness();
    cmd(
        &mut h,
        "lines.addPolyline",
        json!({"points": [100, 100, 300, 100, 300, 200]}),
    );
    h.state_mut().state.tool = Tool::Edit;
    h.run();
    let before = dots(&h);
    let from = screen(&h, Panel::Source1, 200.0, 100.0);
    let to = screen(&h, Panel::Source1, 200.0, 140.0);
    drag(&mut h, from, to, Modifiers::NONE);
    let after = dots(&h);
    for i in 0..3 {
        assert!(
            (after[i].first.y - before[i].first.y - 40.0).abs() < 2.0,
            "dot {i}"
        );
    }
}

#[test]
fn arrow_keys_nudge_the_selection_and_otherwise_step_frames() {
    let mut h = harness();
    add3(&mut h);
    cmd(&mut h, "dots.selectAll", json!({}));
    let before = dots(&h);
    h.state_mut().state.focus_image = 1;
    key(&mut h, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(dots(&h)[0].first.x, before[0].first.x + 1.0);
    key(&mut h, Key::ArrowDown, Modifiers::COMMAND);
    assert_eq!(dots(&h)[0].first.y, before[0].first.y + 10.0);
    assert_eq!(dots(&h)[0].second, before[0].second);
    cmd(&mut h, "edit.undo", json!({}));
    assert_eq!(dots(&h)[0].first.y, before[0].first.y);
    cmd(&mut h, "edit.undo", json!({}));
    assert_eq!(dots(&h), before);
    // Nothing selected: the arrow steps the preview frame, as before.
    cmd(&mut h, "dots.selectNone", json!({}));
    let frame0 = h.state().state.frame;
    key(&mut h, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(h.state().state.frame, frame0 + 1);
    assert_eq!(dots(&h), before);
}

#[test]
fn copy_paste_cut_and_delete_by_keyboard() {
    let mut h = harness();
    add3(&mut h);
    cmd(&mut h, "dots.select", json!({"indices": [0, 1]}));
    key(&mut h, Key::C, Modifiers::COMMAND);
    key(&mut h, Key::V, Modifiers::COMMAND);
    assert_eq!(dots(&h).len(), 5);
    assert_eq!(selection(&h), [3, 4]);
    key(&mut h, Key::X, Modifiers::COMMAND);
    assert_eq!(dots(&h).len(), 3);
    key(&mut h, Key::V, Modifiers::COMMAND);
    assert_eq!(dots(&h).len(), 5);
    key(&mut h, Key::Delete, Modifiers::NONE);
    assert_eq!(dots(&h).len(), 3);
    key(&mut h, Key::A, Modifiers::COMMAND);
    assert_eq!(selection(&h), [0, 1, 2]);
    // Bare letters pick tools, but Ctrl+letter does not.
    key(&mut h, Key::L, Modifiers::NONE);
    assert_eq!(h.state().state.tool, Tool::Line);
    key(&mut h, Key::E, Modifiers::NONE);
    assert_eq!(h.state().state.tool, Tool::Edit);
    key(&mut h, Key::D, Modifiers::COMMAND);
    assert_eq!(h.state().state.tool, Tool::Edit);
    assert!(!h.state().session().prefs().show_dots);
}

#[test]
fn overlays_toggle_and_the_loupe_and_tracks_draw() {
    let mut h = harness();
    add3(&mut h);
    cmd(&mut h, "lines.connect", json!({"a": 0, "b": 1}));
    cmd(&mut h, "dots.option.autoPartner", json!({"on": false}));
    click_at(&mut h, Panel::Source1, 320.0, 80.0, Modifiers::NONE);
    for id in [
        "view.toggle.track",
        "view.toggle.grid",
        "view.toggle.triangles",
    ] {
        cmd(&mut h, id, json!({}));
    }
    cmd(&mut h, "prefs.dotStyle", json!({"style": "diamond"}));
    cmd(&mut h, "view.magnifier", json!({"on": true, "power": 3}));
    // Hover the unpaired dot so its ghost partner blinks on the other panel.
    let hover = screen(&h, Panel::Source1, 320.0, 80.0);
    frame(&mut h, vec![Event::PointerMoved(hover)]);
    h.run();
    shot(&mut h, "key_dots_overlays");
    for style in ["disc", "ring", "cross", "square"] {
        cmd(&mut h, "prefs.dotStyle", json!({"style": style}));
    }
    key(&mut h, Key::M, Modifiers::NONE);
    assert_eq!(h.state().session().prefs().magnifier_power, 4);
    key(&mut h, Key::N, Modifiers::NONE);
    assert_eq!(h.state().session().prefs().magnifier_power, 3);
    // The preferences are saved with the view state.
    assert!(h.state().session().prefs().show_grid);
}

#[test]
fn two_thousand_and_forty_eight_dots_draw() {
    let mut h = harness();
    cmd(&mut h, "dots.option.autoPartner", json!({"on": false}));
    let points: Vec<f64> = (0..2048u32)
        .flat_map(|i| [f64::from(i % 64) * 9.0 + 5.0, f64::from(i / 64) * 9.0 + 5.0])
        .collect();
    cmd(&mut h, "lines.addPolyline", json!({"points": points}));
    assert_eq!(dots(&h).len(), 2048);
    shot(&mut h, "key_dots_2048");
}
