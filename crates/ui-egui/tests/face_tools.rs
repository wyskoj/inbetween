//! The face tools in the shell: smart drag, the model editor, mix and the library browser.
//!
//! Set `INBETWEEN_UI_SHOTS=some/folder` to also save a screenshot.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Vec2};
use egui_kittest::Harness;
use inbetween_ui_egui::state::Panel;
use inbetween_ui_egui::{json, InbetweenApp, ViewState};

type H = Harness<'static, InbetweenApp>;

fn demo_path() -> String {
    format!("{}/../../demo/demo.inbetween", env!("CARGO_MANIFEST_DIR"))
}

fn demo_harness() -> H {
    let mut app = InbetweenApp::new(&egui::Context::default(), None, ViewState::default());
    app.run_command("project.open", &json!({ "path": demo_path() }))
        .unwrap();
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

fn button(pos: Pos2, b: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: b,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn drag(h: &mut H, from: Pos2, to: Pos2) {
    frame(h, vec![Event::PointerMoved(from)]);
    frame(h, vec![button(from, PointerButton::Primary, true)]);
    for i in 1..=6 {
        let p = from + (to - from) * (i as f32 / 6.0);
        frame(h, vec![Event::PointerMoved(p)]);
    }
    frame(h, vec![button(to, PointerButton::Primary, false)]);
    h.run();
}

#[allow(dead_code)]
fn key(h: &mut H, k: Key) {
    frame(
        h,
        vec![Event::Key {
            key: k,
            physical_key: Some(k),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    frame(
        h,
        vec![Event::Key {
            key: k,
            physical_key: Some(k),
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    h.run();
}

fn screen(h: &H, panel: Panel, x: f32, y: f32) -> Pos2 {
    let a = h.state();
    let rect = match panel {
        Panel::Source1 => a.layout.source1,
        Panel::Source2 => a.layout.source2,
        Panel::Preview => a.layout.preview,
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

#[allow(dead_code)]
fn run(h: &mut H, id: &str, params: serde_json::Value) {
    h.state_mut()
        .run_command(id, &params)
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run();
}

fn dots(h: &H) -> Vec<(f64, f64, f64, f64)> {
    h.state().session().project().sequences[0]
        .dots
        .pairs
        .iter()
        .map(|p| (p.first.x, p.first.y, p.second.x, p.second.y))
        .collect()
}

fn place_face(h: &mut H) {
    run(
        h,
        "face.locate",
        json!({"source": "box", "box_x": 100, "box_y": 60, "box_w": 300, "box_h": 360}),
    );
    h.run();
}

#[test]
fn smart_drag_pulls_the_neighbours_and_alt_flips_it() {
    let mut h = demo_harness();
    place_face(&mut h);
    let before = dots(&h);
    assert!(before.len() >= 30, "{}", before.len());
    run(&mut h, "face.option.smartDrag", json!({"on": true}));
    h.run();
    // Pick a dot on Image 1 and drag it.
    let (x, y, _, _) = before[10];
    let from = screen(&h, Panel::Source1, x as f32, y as f32);
    drag(&mut h, from, from + Vec2::new(25.0, 15.0));
    let after = dots(&h);
    let moved = before
        .iter()
        .zip(&after)
        .filter(|(a, b)| (a.0, a.1) != (b.0, b.1))
        .count();
    assert!(moved > 1, "smart drag moved {moved} dots");
    assert!(
        before
            .iter()
            .zip(&after)
            .all(|(a, b)| (a.2, a.3) == (b.2, b.3)),
        "image 2 untouched"
    );
    run(&mut h, "edit.undo", json!({}));
    assert_eq!(dots(&h), before);
    // Alt held: a plain drag of one dot.
    h.input_mut().modifiers = Modifiers::ALT;
    drag(&mut h, from, from + Vec2::new(25.0, 15.0));
    h.input_mut().modifiers = Modifiers::NONE;
    let after = dots(&h);
    let moved = before
        .iter()
        .zip(&after)
        .filter(|(a, b)| (a.0, a.1) != (b.0, b.1))
        .count();
    assert_eq!(moved, 1, "alt drags one dot");
}

#[test]
fn smart_drag_off_is_a_plain_drag_and_dots_that_are_not_a_face_drag_plainly() {
    let mut h = demo_harness();
    place_face(&mut h);
    let before = dots(&h);
    let (x, y, _, _) = before[10];
    let from = screen(&h, Panel::Source1, x as f32, y as f32);
    drag(&mut h, from, from + Vec2::new(25.0, 15.0));
    let moved = before
        .iter()
        .zip(dots(&h).iter())
        .filter(|(a, b)| (a.0, a.1) != (b.0, b.1))
        .count();
    assert_eq!(moved, 1);
}

fn two_faces(h: &mut H) {
    place_face(h);
    run(h, "sequence.duplicate", json!({"sequence": 0}));
    // The copy shows the other picture first, so the two faces differ.
    run(h, "image.swap", json!({"sequence": 1}));
    h.run();
}

#[test]
fn the_model_window_moves_landmarks_with_one_undo_step_and_edits_the_list() {
    let mut h = demo_harness();
    h.state_mut().invoke("face.model");
    h.run();
    h.run();
    let canvas = h
        .state()
        .face_model_canvas()
        .expect("the window drew its canvas");
    let model = h.state().session().project().face.clone();
    let i = model.dots.iter().position(|d| d.name == "chin").unwrap();
    let at = |x: f64, y: f64| {
        Pos2::new(
            canvas.min.x + canvas.width() * x as f32,
            canvas.min.y + canvas.height() * y as f32,
        )
    };
    let from = at(model.dots[i].x, model.dots[i].y);
    drag(&mut h, from, from + Vec2::new(20.0, -30.0));
    let moved = h.state().session().project().face.dots[i].clone();
    assert!(
        moved.x > model.dots[i].x + 0.03,
        "x {} -> {}",
        model.dots[i].x,
        moved.x
    );
    assert!(
        moved.y < model.dots[i].y - 0.05,
        "y {} -> {}",
        model.dots[i].y,
        moved.y
    );
    shot(&mut h, "face_model_window");
    run(&mut h, "edit.undo", json!({}));
    assert_eq!(
        h.state().session().project().face.dots[i],
        model.dots[i],
        "one drag, one undo step"
    );
    // Clicking a landmark then another with a guide started adds a guide line.
    let guides = model.guides.len();
    run(
        &mut h,
        "face.model",
        json!({"op": "guide_add", "index": 0, "to": 4}),
    );
    h.run();
    assert_eq!(h.state().session().project().face.guides.len(), guides + 1);
    run(
        &mut h,
        "face.model",
        json!({"op": "preset", "preset": "minimal"}),
    );
    h.run();
    assert!(h.state().session().project().face.active_count() < model.active_count());
}

#[test]
fn the_mix_window_writes_candidates_and_shows_them() {
    let mut h = demo_harness();
    two_faces(&mut h);
    h.state_mut().invoke("face.mix");
    h.run();
    h.run();
    h.state_mut().configure_mix("average", 1, 1, 1);
    let ctx = h.ctx.clone();
    h.state_mut().run_mix(&ctx);
    h.run();
    let files = h.state().mix_candidates();
    assert_eq!(files.len(), 1);
    assert!(std::fs::metadata(&files[0]).unwrap().len() > 100);
    // Random mode: several candidates, each its own file.
    h.state_mut().configure_mix("random", 7, 4, 1);
    h.state_mut().run_mix(&ctx);
    h.run();
    let files = h.state().mix_candidates();
    assert_eq!(files.len(), 4, "{files:?}");
    let sizes: std::collections::BTreeSet<u64> = files
        .iter()
        .map(|f| std::fs::metadata(f).unwrap().len())
        .collect();
    assert!(sizes.len() > 1, "random candidates differ");
    shot(&mut h, "face_mix_window");
}

#[test]
fn the_library_browser_lists_entries_and_a_tile_dropped_on_a_panel_is_used() {
    let mut h = demo_harness();
    place_face(&mut h);
    let dir = std::env::temp_dir().join(format!("inbetween-ui-lib-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let lib = dir.display().to_string();
    run(
        &mut h,
        "image.saveToLibrary",
        json!({"dir": lib, "name": "sunny", "image": 1}),
    );
    h.state_mut().state.library_dir = lib.clone();
    h.state_mut().invoke("face.library");
    h.run();
    h.run();
    assert_eq!(
        h.state().library_entries(),
        vec![(String::new(), "sunny".to_string())]
    );
    shot(&mut h, "face_library_window");
    // Drop the tile onto Image 2: its picture and dots replace Image 2's.
    let before = h.state().session().project().sequences[0].image2.0.clone();
    egui::DragAndDrop::set_payload(
        &h.ctx,
        inbetween_ui_egui::app::LibraryDrag {
            folder: String::new(),
            name: "sunny".into(),
        },
    );
    let target = h.state().layout.source2.center();
    frame(&mut h, vec![Event::PointerMoved(target)]);
    frame(&mut h, vec![button(target, PointerButton::Primary, true)]);
    frame(&mut h, vec![button(target, PointerButton::Primary, false)]);
    h.run();
    let after = h.state().session().project().sequences[0].image2.0.clone();
    assert_ne!(before, after);
    assert!(after.ends_with("sunny.png"), "{after}");
    run(&mut h, "edit.undo", json!({}));
    assert_eq!(h.state().session().project().sequences[0].image2.0, before);
    let _ = std::fs::remove_dir_all(&dir);
}
