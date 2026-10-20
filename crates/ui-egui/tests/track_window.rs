//! The Track Curve window.
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

#[allow(dead_code)]
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

fn track(h: &H) -> inbetween_doc::Track {
    h.state().session().project().sequences[0].track.clone()
}

#[test]
fn dragging_a_point_bends_the_path_and_the_whole_window_is_one_undo_step() {
    let mut h = demo_harness();
    h.state_mut().invoke("curve.track");
    h.run();
    h.run();
    let canvas = h
        .state()
        .track_canvas()
        .expect("the window drew its canvas");
    run(
        &mut h,
        "curve.track",
        json!({"op": "preset", "name": "arc"}),
    );
    h.run();
    h.run();
    let before = track(&h);
    assert_eq!(before.points.len(), 3);
    // The arc's middle point sits at (0.5, 0.25): drag it up and out.
    let at = |u: f64, v: f64| {
        let x = (u + 0.5) / 2.0;
        let y = (v + 1.0) / 2.0;
        Pos2::new(
            canvas.min.x + canvas.width() * x as f32,
            canvas.min.y + canvas.height() * y as f32,
        )
    };
    let from = at(0.5, 0.25);
    drag(&mut h, from, at(0.9, -0.5));
    let after = track(&h);
    assert_eq!(after.points.len(), 3);
    assert!(
        (after.points[1].u - 0.9).abs() < 0.05,
        "{:?}",
        after.points[1]
    );
    assert!(
        (after.points[1].v + 0.5).abs() < 0.05,
        "{:?}",
        after.points[1]
    );
    assert_eq!(
        (after.points[0].u, after.points[0].v),
        (0.0, 0.0),
        "the ends are pinned"
    );
    assert_eq!((after.points[2].u, after.points[2].v), (1.0, 0.0));
    shot(&mut h, "track_window");
    // Click on empty canvas adds a point; right-click on it removes it.
    let spot = at(0.25, 0.6);
    frame(&mut h, vec![Event::PointerMoved(spot)]);
    frame(&mut h, vec![button(spot, PointerButton::Primary, true)]);
    frame(&mut h, vec![button(spot, PointerButton::Primary, false)]);
    h.run();
    assert_eq!(track(&h).points.len(), 4);
    frame(&mut h, vec![button(spot, PointerButton::Secondary, true)]);
    frame(&mut h, vec![button(spot, PointerButton::Secondary, false)]);
    h.run();
    assert_eq!(track(&h).points.len(), 3);
    // Everything the window did is one undo step.
    h.state_mut().close_track();
    h.run();
    run(&mut h, "edit.undo", json!({}));
    assert!(
        !track(&h).enabled && track(&h).points.len() == 2,
        "{:?}",
        track(&h)
    );
}
