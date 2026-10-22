//! The source-editing tools in the shell: the Image menu, F3, the crop tool, the Adjust window,
//! the filter list and the Caption window with its drag.
//!
//! Set `INBETWEEN_UI_SHOTS=some/folder` to also save screenshots of each tool there.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use inbetween_ui_egui::app::QUICK_IMAGE;
use inbetween_ui_egui::state::{CaptionScope, Panel};
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

fn seq(h: &H) -> &inbetween_doc::Sequence {
    &h.state().session().project().sequences[0]
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

fn run(h: &mut H, id: &str, params: serde_json::Value) {
    h.state_mut()
        .run_command(id, &params)
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run();
}

#[test]
fn the_image_menu_lists_the_generated_commands_and_the_quick_items() {
    let mut h = demo_harness();
    h.get_by_label("Image").click();
    h.run();
    for label in [
        "Crop\u{2026}",
        "Adjust Color\u{2026}",
        "Swap Images",
        "Caption\u{2026}",
        "Rotate\u{2026}",
    ] {
        assert!(
            h.query_by_label(label).is_some(),
            "Image menu lacks {label}"
        );
    }
    for (label, ..) in QUICK_IMAGE {
        assert!(
            h.query_by_label(label).is_some(),
            "Image menu lacks {label}"
        );
    }
}

#[test]
fn quick_items_act_on_the_panel_last_used_and_dispatch_the_right_parameters() {
    let mut h = demo_harness();
    // Clicking the menu item flips Image 1 (the default focus).
    h.get_by_label("Image").click();
    h.run();
    h.get_by_label("Flip Horizontal").click();
    h.run();
    assert!(seq(&h).edit1.flip_h && !seq(&h).edit2.flip_h);
    // Pressing in the second panel moves the focus there.
    let p = screen(&h, Panel::Source2, 5.0, 5.0);
    frame(&mut h, vec![Event::PointerMoved(p)]);
    frame(&mut h, vec![button(p, PointerButton::Secondary, true)]);
    frame(&mut h, vec![button(p, PointerButton::Secondary, false)]);
    assert_eq!(h.state().state.focus_image, 2);
    h.get_by_label("Image").click();
    h.run();
    h.get_by_label("Rotate 90\u{b0} CW").click();
    h.run();
    assert_eq!(seq(&h).edit2.quarter_turns, 1);
    assert_eq!(seq(&h).edit1.quarter_turns, 0);
    // Every quick item names a real command and a valid parameter.
    let registry = inbetween_engine::Registry::standard();
    for (_, id, name, _) in QUICK_IMAGE {
        let spec = registry.get(id).unwrap();
        assert!(spec.params.iter().any(|p| p.name == *name), "{id} {name}");
    }
}

#[test]
fn f3_swaps_the_images() {
    let mut h = demo_harness();
    let (a, b) = (seq(&h).image1.clone(), seq(&h).image2.clone());
    key(&mut h, Key::F3);
    assert_eq!(seq(&h).image1, b);
    assert_eq!(seq(&h).image2, a);
    key(&mut h, Key::F3);
    assert_eq!(seq(&h).image1, a);
}

#[test]
fn dragging_the_crop_box_then_apply_gives_the_expected_rectangle() {
    let mut h = demo_harness();
    h.state_mut().invoke("image.crop");
    h.run();
    let tool = h.state().crop_tool().expect("the tool is on").clone();
    assert_eq!((tool.x, tool.y), (0.0, 0.0));
    let (w, hh) = (tool.w, tool.h);
    let zoom = f64::from(h.state().state.view(Panel::Source1).zoom());
    // Drag the bottom-right corner up and left (the demo images are shown 1:1 in the frame).
    let corner = screen(&h, Panel::Source1, w as f32, hh as f32);
    let to = corner + Vec2::new(-60.0, -40.0);
    drag(&mut h, corner, to);
    let moved = h.state().crop_tool().unwrap().clone();
    assert!(
        (moved.w - (w - 60.0 / zoom)).abs() < 2.0,
        "w {} vs {}",
        moved.w,
        w
    );
    assert!((moved.h - (hh - 40.0 / zoom)).abs() < 2.0);
    assert_eq!((moved.x, moved.y), (0.0, 0.0), "the far corner stays put");
    // Nothing is in the project until Apply.
    assert!(seq(&h).edit1.is_identity());
    key(&mut h, Key::Enter);
    assert!(h.state().crop_tool().is_none());
    let crop = seq(&h).edit1.crop.expect("applied");
    assert!((crop.w - moved.w).abs() < 1e-6 && (crop.h - moved.h).abs() < 1e-6);
    // Applying is one undo step that restores the edit and the dots.
    run(&mut h, "edit.undo", json!({}));
    assert!(seq(&h).edit1.is_identity());
}

#[test]
fn cancelling_the_crop_leaves_the_edit_alone_and_the_box_may_leave_the_picture() {
    let mut h = demo_harness();
    h.state_mut().invoke("image.crop");
    h.run();
    let tool = h.state().crop_tool().unwrap().clone();
    // Push the box well past the picture with the numeric fields' route.
    let mut wide = tool.clone();
    wide.x = -50.0;
    wide.w = tool.w + 120.0;
    h.state_mut().set_crop(wide);
    h.run();
    key(&mut h, Key::Escape);
    assert!(h.state().crop_tool().is_none());
    assert!(seq(&h).edit1.is_identity());
    // Applying a box that extends past the image works and keeps dots on their features.
    h.state_mut().invoke("image.crop");
    h.run();
    let mut past = h.state().crop_tool().unwrap().clone();
    past.x = -50.0;
    past.w += 100.0;
    past.angle = 10.0;
    h.state_mut().set_crop(past);
    h.state_mut().apply_crop();
    h.run();
    let crop = seq(&h).edit1.crop.unwrap();
    assert!(crop.x < 0.0 && crop.x + crop.w > tool.w);
    assert_eq!(seq(&h).edit1.angle, 10.0);
}

#[test]
fn the_crop_window_numbers_and_aspect_edit_the_box() {
    let mut h = demo_harness();
    h.state_mut().invoke("image.crop");
    h.run();
    let mut t = h.state().crop_tool().unwrap().clone();
    t.aspect = "1:1".into();
    t.snap_aspect(1.0);
    h.state_mut().set_crop(t.clone());
    h.run();
    let now = h.state().crop_tool().unwrap();
    assert!((now.w - now.h).abs() < 1e-9);
    // A non-finite box is refused, not stored.
    let mut bad = t;
    bad.w = f64::NAN;
    h.state_mut().set_crop(bad);
    assert!(h.state().crop_tool().unwrap().is_sane());
}

#[test]
fn an_adjust_session_is_one_undo_step_and_cancel_restores_the_edit() {
    let mut h = demo_harness();
    run(&mut h, "image.flip", json!({"axis": "vertical"})); // an earlier step that must survive
    h.state_mut().invoke("image.adjust");
    h.run();
    assert_eq!(h.state().state.adjust_window, Some(1));
    for v in [10, 20, 30, 40] {
        run(&mut h, "image.adjust", json!({"brightness": v, "image": 1}));
    }
    run(&mut h, "image.adjust", json!({"contrast": -10, "image": 1}));
    h.state_mut().close_adjust(true);
    h.run();
    assert_eq!(seq(&h).edit1.adjust.brightness, 40.0);
    run(&mut h, "edit.undo", json!({}));
    assert_eq!(
        seq(&h).edit1.adjust.brightness,
        0.0,
        "the whole session undoes at once"
    );
    assert!(seq(&h).edit1.flip_v, "and nothing more");

    // Cancel undoes what the window did and nothing else.
    h.state_mut().invoke("image.adjust");
    h.run();
    run(
        &mut h,
        "image.adjust",
        json!({"saturation": -50, "image": 1}),
    );
    run(&mut h, "image.adjust", json!({"hue": 20, "image": 1}));
    h.state_mut().close_adjust(false);
    h.run();
    assert!(seq(&h).edit1.adjust.is_identity());
    assert!(seq(&h).edit1.flip_v, "the earlier edit is still there");
    // Cancelling with nothing changed undoes nothing.
    h.state_mut().invoke("image.adjust");
    h.run();
    h.state_mut().close_adjust(false);
    assert!(seq(&h).edit1.flip_v);
}

#[test]
fn the_adjust_sliders_drive_the_command() {
    let mut h = demo_harness();
    h.state_mut().invoke("image.adjust");
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::Slider, "Brightness")
        .focus();
    h.run();
    for _ in 0..3 {
        key(&mut h, Key::ArrowRight);
    }
    assert!(
        seq(&h).edit1.adjust.brightness > 0.0,
        "arrow keys on the slider adjust the image"
    );
    h.get_by_label("Negative").click();
    h.run();
    assert!(seq(&h).edit1.adjust.negative);
    h.get_by_label("Cancel").click();
    h.run();
    assert!(seq(&h).edit1.adjust.is_identity() && h.state().state.adjust_window.is_none());
}

#[test]
fn the_filter_list_edits_through_forms_and_buttons() {
    let mut h = demo_harness();
    run(&mut h, "image.filter.blur", json!({"radius": 4}));
    run(&mut h, "image.filter.mosaic", json!({"cell": 9}));
    h.state_mut().invoke("image.adjust");
    h.run();
    assert!(h.query_all_by_label("Remove").count() == 2);
    // Edit opens the filter's own form, filled in, aimed at its position.
    h.state_mut().edit_filter(1, 1);
    h.run();
    {
        let form = h.state().form().expect("the form opens");
        assert_eq!(form.id, "image.filter.mosaic");
        let p = form.to_params().unwrap();
        assert_eq!(
            (p["cell"].as_i64(), p["index"].as_i64()),
            (Some(9), Some(1))
        );
    }
    h.state_mut().form_mut().unwrap().set("cell", "20");
    h.state_mut().submit_form();
    h.run();
    assert_eq!(
        seq(&h).edit1.filters[1],
        inbetween_doc::Filter::Mosaic { cell: 20 }
    );
    assert_eq!(seq(&h).edit1.filters.len(), 2, "replaced, not appended");
    // The Remove button on the first row.
    h.query_all_by_label("Remove").next().unwrap().click();
    h.run();
    assert_eq!(seq(&h).edit1.filters.len(), 1);
    h.get_by_label("Cancel").click();
    h.run();
    assert_eq!(
        seq(&h).edit1.filters.len(),
        2,
        "cancel brings the filters back"
    );
}

#[test]
fn filter_and_rotate_menu_items_open_prefilled_forms_instead_of_running() {
    let mut h = demo_harness();
    h.state_mut().state.focus_image = 2;
    h.state_mut().invoke("image.filter.gaussian");
    h.run();
    let form = h.state().form().expect("a filter asks first");
    assert_eq!(form.id, "image.filter.gaussian");
    assert_eq!(form.to_params().unwrap()["image"], 2);
    assert!(seq(&h).edit2.filters.is_empty());
    h.state_mut().submit_form();
    h.run();
    assert_eq!(seq(&h).edit2.filters.len(), 1);
    h.state_mut().invoke("image.rotate");
    h.run();
    assert_eq!(h.state().form().unwrap().id, "image.rotate");
}

#[test]
fn the_caption_window_edits_live_in_one_undo_step_and_the_preview_shows_it() {
    let mut h = demo_harness();
    h.state_mut().invoke("caption.movie");
    h.run();
    assert_eq!(h.state().state.caption_window, Some(CaptionScope::Movie));
    assert!(h.state_mut().set_caption(
        CaptionScope::Movie,
        json!({"text": "Hello", "size": 40, "y": 0.5})
    ));
    assert!(h
        .state_mut()
        .set_caption(CaptionScope::Movie, json!({"bold": true})));
    h.run();
    let c = h.state().caption_of(CaptionScope::Movie).unwrap();
    assert_eq!((c.text.as_str(), c.bold), ("Hello", true));
    h.state_mut().close_caption();
    h.run();
    run(&mut h, "edit.undo", json!({}));
    assert!(
        h.state().caption_of(CaptionScope::Movie).is_none(),
        "one step undoes the session"
    );
}

#[test]
fn dragging_a_caption_on_the_preview_moves_its_anchor_in_one_step() {
    let mut h = demo_harness();
    h.state_mut().invoke("caption.sequence");
    h.run();
    assert!(h.state_mut().set_caption(
        CaptionScope::Sequence,
        json!({"text": "Drag me", "size": 60, "x": 0.4, "y": 0.5, "align": "center"})
    ));
    h.run();
    h.run();
    let out = h.state().session().project().output;
    let c = h.state().caption_of(CaptionScope::Sequence).unwrap();
    let font = inbetween_render_cpu::caption::Font::bundled().unwrap();
    let b =
        inbetween_render_cpu::caption::caption_bounds(&c, &font, (out.width, out.height)).unwrap();
    let centre = screen(
        &h,
        Panel::Preview,
        ((b[0] + b[2]) / 2.0) as f32,
        ((b[1] + b[3]) / 2.0) as f32,
    );
    let zoom = f64::from(h.state().state.view(Panel::Preview).zoom());
    drag(&mut h, centre, centre + Vec2::new(60.0, 30.0));
    let moved = h.state().caption_of(CaptionScope::Sequence).unwrap();
    let want_dx = 60.0 / zoom / f64::from(out.width);
    let want_dy = 30.0 / zoom / f64::from(out.height);
    assert!(
        (moved.x - (0.4 + want_dx)).abs() < 0.02,
        "x {} vs {}",
        moved.x,
        0.4 + want_dx
    );
    assert!(
        (moved.y - (0.5 + want_dy)).abs() < 0.02,
        "y {} vs {}",
        moved.y,
        0.5 + want_dy
    );
    // The preview did not pan: the drag moved the caption.
    h.state_mut().close_caption();
    h.run();
    run(&mut h, "edit.undo", json!({}));
    assert!(h.state().caption_of(CaptionScope::Sequence).is_none());
}

#[test]
fn dragging_an_image_caption_on_its_panel_moves_it_against_the_picture() {
    let mut h = demo_harness();
    h.state_mut().open_caption(CaptionScope::Image1);
    h.run();
    assert!(h.state_mut().set_caption(
        CaptionScope::Image1,
        json!({"text": "On the sun", "size": 36, "x": 0.5, "y": 0.5})
    ));
    h.run();
    h.run();
    // The caption sits at the middle of the picture; grab it there and pull right.
    let (cx, cy) = {
        let p = h.state().session().project().output;
        (p.width as f32 / 2.0, p.height as f32 / 2.0)
    };
    let from = screen(&h, Panel::Source1, cx, cy);
    let before = h.state().caption_of(CaptionScope::Image1).unwrap().x;
    drag(&mut h, from, from + Vec2::new(50.0, 0.0));
    let after = h.state().caption_of(CaptionScope::Image1).unwrap().x;
    assert!(after > before + 0.01, "{before} -> {after}");
    h.state_mut().close_caption();
}

#[test]
fn the_library_folder_is_remembered_and_prefilled() {
    let mut h = demo_harness();
    let dir = std::env::temp_dir().join(format!("inbetween-ui-lib-{}", std::process::id()));
    run(
        &mut h,
        "image.saveToLibrary",
        json!({"dir": dir.display().to_string(), "name": "face"}),
    );
    assert_eq!(h.state().state.library_dir, dir.display().to_string());
    h.state_mut().invoke("image.saveToLibrary");
    h.run();
    let form = h.state().form().unwrap();
    assert_eq!(form.id, "image.saveToLibrary");
    assert_eq!(form.to_params().unwrap_err(), "`name` is required");
    let mut f = form.clone();
    f.set("name", "again");
    assert_eq!(f.to_params().unwrap()["dir"], dir.display().to_string());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_new_view_state_survives_json_and_hostile_files() {
    let mut s = ViewState {
        focus_image: 2,
        library_dir: "C:/faces".into(),
        adjust_window: Some(2),
        caption_window: Some(CaptionScope::Image2),
        ..ViewState::default()
    };
    let text = serde_json::to_string(&s).unwrap();
    assert_eq!(serde_json::from_str::<ViewState>(&text).unwrap(), s);
    s.focus_image = 9;
    s.adjust_window = Some(7);
    s.library_dir = "x".repeat(5000);
    s.sanitize();
    assert_eq!(
        (s.focus_image, s.adjust_window, s.library_dir.len()),
        (1, None, 0)
    );
    // Files from before these fields still load.
    let old: ViewState = serde_json::from_str(r#"{"frame": 3}"#).unwrap();
    assert_eq!((old.focus_image, old.crop.is_none()), (1, true));
}

#[test]
fn screenshots_of_each_tool() {
    if std::env::var_os("INBETWEEN_UI_SHOTS").is_none() {
        return;
    }
    let mut app = InbetweenApp::new(&egui::Context::default(), None, ViewState::default());
    app.run_command("project.open", &json!({ "path": demo_path() }))
        .unwrap();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 900.0))
        .with_step_dt(1.0 / 24.0)
        .wgpu()
        .build_state(|ctx, app: &mut InbetweenApp| app.show(ctx), app);
    h.run();
    h.state_mut().state.frame = 8;
    run(
        &mut h,
        "image.filter.gaussian",
        json!({"sigma": 1.5, "image": 2}),
    );
    run(
        &mut h,
        "image.adjust",
        json!({"saturation": 30, "image": 1}),
    );

    h.state_mut().invoke("image.crop");
    h.run();
    let mut t = h.state().crop_tool().unwrap().clone();
    t.x = 30.0;
    t.y = 20.0;
    t.w = 380.0;
    t.h = 270.0;
    t.angle = -8.0;
    h.state_mut().set_crop(t);
    h.run();
    shot(&mut h, "crop_tool");
    h.state_mut().cancel_crop();
    h.run();

    h.state_mut().invoke("image.adjust");
    h.run();
    run(
        &mut h,
        "image.adjust",
        json!({"brightness": 15, "hue": 25, "image": 1}),
    );
    shot(&mut h, "adjust_window");
    h.state_mut().close_adjust(true);
    h.run();

    h.state_mut().invoke("caption.movie");
    h.run();
    h.state_mut().set_caption(
        CaptionScope::Movie,
        json!({"text": "Sun to moon\nin between", "size": 44, "bold": true, "border_size": 2, "shadow_size": 5, "shadow_distance": 5, "y": 0.82}),
    );
    h.run();
    h.run();
    shot(&mut h, "caption_window");
    h.state_mut().close_caption();
    h.run();
    shot(&mut h, "captioned_preview");
}
