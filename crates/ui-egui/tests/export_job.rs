#![allow(dead_code)]
//! Long exports run on a thread with a progress window and a Cancel button.
//!
//! Set `INBETWEEN_UI_SHOTS=some/folder` to also save a screenshot.

#[allow(unused_imports)]
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

fn wait_for_export(h: &mut H) {
    for _ in 0..4000 {
        h.step();
        if !h.state().export_running() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the export never finished");
}

fn out_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("inbetween-ui-export-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn an_export_runs_on_its_thread_and_the_session_comes_back() {
    let mut h = demo_harness();
    let dir = out_dir("ok");
    let project = h.state().session().project().clone();
    assert!(h.state_mut().start_export(
        "export.movie",
        &json!({"out_dir": dir.display().to_string()})
    ));
    assert!(h.state().export_running());
    // Only one at a time.
    assert!(!h.state_mut().start_export(
        "export.movie",
        &json!({"out_dir": dir.display().to_string()})
    ));
    wait_for_export(&mut h);
    assert_eq!(
        *h.state().session().project(),
        project,
        "the session is back, unchanged"
    );
    let status = h.state().status().unwrap().clone();
    assert!(
        !status.is_error && status.text.starts_with("Exported 12 frames"),
        "{status:?}"
    );
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 12);
    // The window is alive again: commands run as before.
    run(&mut h, "movie.fps", json!({"fps": 12}));
    assert_eq!(h.state().session().project().fps, 12.0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancel_stops_the_export_and_reports_it() {
    let mut h = demo_harness();
    run(&mut h, "movie.size", json!({"width": 512, "height": 512}));
    run(&mut h, "sequence.length", json!({"frames": 999}));
    let dir = out_dir("cancel");
    assert!(h.state_mut().start_export(
        "export.movie",
        &json!({"out_dir": dir.display().to_string()})
    ));
    // Let a few frames through, then press Cancel.
    for _ in 0..2000 {
        h.step();
        if std::fs::read_dir(&dir).is_ok_and(|r| r.count() >= 3) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    h.step();
    shot(&mut h, "export_progress");
    h.state_mut().cancel_export();
    wait_for_export(&mut h);
    let status = h.state().status().unwrap().clone();
    assert!(
        status.is_error && status.text.contains("cancelled"),
        "{status:?}"
    );
    assert!(std::fs::read_dir(&dir).unwrap().count() < 999);
    assert_eq!(h.state().session().project().sequences[0].frames, 999);
    let _ = std::fs::remove_dir_all(&dir);
}
