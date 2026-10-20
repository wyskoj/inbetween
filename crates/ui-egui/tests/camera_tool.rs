//! The camera tool in the shell: the Camera window, handles on the preview, and the
//! commands a drag runs.
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

fn camera(h: &H) -> inbetween_doc::Camera {
    h.state().session().project().sequences[0].camera.clone()
}

#[test]
fn dragging_the_picture_on_the_preview_adds_a_keyframe_and_undoes_in_one_step() {
    let mut h = demo_harness();
    h.state_mut().invoke("camera.keyframes");
    h.run();
    h.run();
    let out = h.state().session().project().output;
    let (w, ht) = (out.width as f32, out.height as f32);
    let centre = screen(&h, Panel::Preview, w / 2.0, ht / 2.0);
    let zoom = h.state().state.view(Panel::Preview).zoom();
    drag(&mut h, centre, centre + Vec2::new(40.0, 20.0));
    let cam = camera(&h);
    assert!(cam.enabled);
    assert_eq!(cam.keys.len(), 1, "{cam:?}");
    let k = cam.keys[0];
    assert!((f64::from(40.0 / zoom / w) - k.x).abs() < 0.02, "x {}", k.x);
    assert!(
        (f64::from(20.0 / zoom / ht) - k.y).abs() < 0.02,
        "y {}",
        k.y
    );
    shot(&mut h, "camera_tool");
    h.state_mut().close_camera();
    h.run();
    run(&mut h, "edit.undo", json!({}));
    assert!(camera(&h).keys.is_empty(), "one undo step");
}

#[test]
fn the_corner_square_scales_and_the_top_circle_rotates() {
    let mut h = demo_harness();
    h.state_mut().invoke("camera.keyframes");
    h.run();
    h.run();
    let out = h.state().session().project().output;
    let (w, ht) = (out.width as f32, out.height as f32);
    // Bottom-right corner: pull it in to halve the size.
    let corner = screen(&h, Panel::Preview, w, ht);
    let far = screen(&h, Panel::Preview, w * 0.75, ht * 0.75);
    drag(&mut h, corner, far);
    let k = camera(&h).keys[0];
    assert!(
        (k.scale_x - 0.5).abs() < 0.05 && (k.scale_y - 0.5).abs() < 0.05,
        "{k:?}"
    );
    // Rotate: drag the circle (above the top edge of the scaled picture) to the right.
    let pose = camera(&h).pose(0.0);
    let frame = (f64::from(w), f64::from(ht));
    let reach = f64::from(inbetween_ui_egui::panels::GRAB_RADIUS)
        / f64::from(h.state().state.view(Panel::Preview).zoom());
    let knob = inbetween_ui_egui::camera_tool::rotate_position(
        &pose,
        frame,
        inbetween_ui_egui::camera_tool::rotate_handle_gap(reach),
    );
    let from = screen(&h, Panel::Preview, knob.0 as f32, knob.1 as f32);
    let centre = screen(&h, Panel::Preview, w / 2.0, ht / 2.0);
    drag(&mut h, from, centre + Vec2::new(200.0, 0.0));
    let k = camera(&h).keys[0];
    assert!((k.rotation - 90.0).abs() < 1.0, "{}", k.rotation);
}

#[test]
fn the_window_edits_numbers_and_the_effect_camera_has_its_own_handles() {
    let mut h = demo_harness();
    h.state_mut().invoke("effect.camera");
    h.run();
    h.run();
    let out = h.state().session().project().output;
    let (w, ht) = (out.width as f32, out.height as f32);
    let centre = screen(&h, Panel::Preview, w / 2.0, ht / 2.0);
    drag(&mut h, centre, centre + Vec2::new(30.0, 0.0));
    // The mask camera moved, the sequence camera did not.
    let s = &h.state().session().project().sequences[0];
    assert_eq!(s.effects.cameras.mask.keys.len(), 1);
    assert!(s.camera.keys.is_empty());
    h.state_mut().close_camera();
    h.run();
}

#[test]
fn the_curve_window_also_edits_the_master_and_camera_curves() {
    use inbetween_ui_egui::curves::{CurveAction, CurveKind};
    let mut h = demo_harness();
    for (id, kind) in [
        ("curve.master", CurveKind::Master),
        ("camera.curve", CurveKind::Camera),
    ] {
        h.state_mut().invoke(id);
        h.run();
        assert_eq!(h.state().state.curve_window, Some(kind), "{id}");
    }
    h.state_mut().invoke("curve.master");
    h.run();
    h.state_mut().apply_curve(
        "curve.master",
        CurveAction::Move {
            index: 0,
            x: 0.0,
            y: 0.25,
        },
    );
    h.state_mut()
        .apply_curve("camera.curve", CurveAction::Add { x: 0.5, y: 0.2 });
    let s = &h.state().session().project().sequences[0];
    assert!((s.master_curve.eval(0.0) - 0.25).abs() < 1e-9);
    assert_eq!(s.camera.easing.points.len(), 3);
    shot(&mut h, "curve_window_master");
}
