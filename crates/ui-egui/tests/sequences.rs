//! The sequence list and the curve editor, driven through the real interface.
//!
//! Set `INBETWEEN_UI_SHOTS=some/folder` to also save screenshots there.

use egui::{Event, Modifiers, PointerButton, Pos2, Vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use inbetween_ui_egui::curves::CurveAction;
use inbetween_ui_egui::seqlist::ListAction;
use inbetween_ui_egui::state::Panel;
use inbetween_ui_egui::{json, InbetweenApp, ViewState};

type H = Harness<'static, InbetweenApp>;

fn demo_dir() -> String {
    format!("{}/../../demo", env!("CARGO_MANIFEST_DIR"))
}

fn demo_harness() -> H {
    let mut app = InbetweenApp::new(&egui::Context::default(), None, ViewState::default());
    app.run_command(
        "project.open",
        &json!({ "path": format!("{}/demo.inbetween", demo_dir()) }),
    )
    .unwrap();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 900.0))
        .with_step_dt(1.0 / 24.0)
        .build_state(|ctx, app: &mut InbetweenApp| app.show(ctx), app);
    h.run();
    h
}

fn app(h: &H) -> &InbetweenApp {
    h.state()
}

fn app_mut(h: &mut H) -> &mut InbetweenApp {
    h.state_mut()
}

fn names(h: &H) -> Vec<String> {
    app(h)
        .session()
        .project()
        .sequences
        .iter()
        .map(|s| s.name.clone())
        .collect()
}

fn add_continued(h: &mut H, name: &str) {
    let image2 = format!("{}/sun.png", demo_dir());
    app_mut(h)
        .run_command("sequence.add", &json!({ "image2": image2, "name": name }))
        .unwrap();
    h.run();
}

fn click(h: &mut H, pos: Pos2) {
    let button = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    for event in [Event::PointerMoved(pos), button(true), button(false)] {
        h.input_mut().events.push(event);
        h.step();
    }
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

#[test]
fn adding_a_continued_sequence_selects_it_and_edits_go_to_it() {
    let mut h = demo_harness();
    let first_dots = app(&h).session().project().sequences[0].dots.len();
    add_continued(&mut h, "back");
    assert_eq!(app(&h).state.sequence, 1);
    assert_eq!(names(&h), ["sun to moon", "back"]);

    // A click on the image adds a dot to the sequence being edited, not to sequence 0.
    let a = app(&h);
    let at = a
        .state
        .view(Panel::Source1)
        .to_screen(a.layout.source1, Pos2::new(60.0, 120.0));
    click(&mut h, at);
    let project = app(&h).session().project();
    assert_eq!(project.sequences[1].dots.len(), 1);
    assert_eq!(project.sequences[0].dots.len(), first_dots);

    // Going back to the first sequence edits that one again.
    app_mut(&mut h).apply_list(ListAction::Select(0));
    h.run();
    assert_eq!(app(&h).state.sequence, 0);
    app_mut(&mut h)
        .run_command("dots.add", &json!({"x": 5.0, "y": 6.0}))
        .unwrap();
    assert_eq!(
        app(&h).session().project().sequences[0].dots.len(),
        first_dots + 1
    );
}

#[test]
fn list_actions_reorder_hide_and_delete_while_the_selection_follows() {
    let mut h = demo_harness();
    add_continued(&mut h, "b");
    add_continued(&mut h, "c");
    app_mut(&mut h)
        .run_command("sequence.rename", &json!({"sequence": 0, "name": "a"}))
        .unwrap();
    assert_eq!(names(&h), ["a", "b", "c"]);
    assert_eq!(app(&h).state.sequence, 2);

    // Moving a row selects it at its new position.
    app_mut(&mut h).apply_list(ListAction::Move(2, "top"));
    h.run();
    assert_eq!(names(&h), ["c", "a", "b"]);
    assert_eq!(app(&h).state.sequence, 0);

    // Hiding drops it from the movie; solo hides the rest.
    let total = |h: &H| app(h).session().project().total_frames();
    let all = total(&h);
    app_mut(&mut h).apply_list(ListAction::Visibility {
        index: 1,
        solo: false,
    });
    assert!(total(&h) < all);
    app_mut(&mut h).apply_list(ListAction::Visibility {
        index: 2,
        solo: true,
    });
    let only = u64::from(app(&h).session().project().sequences[2].total_frames());
    assert_eq!(total(&h), only);
    assert!(app(&h).session().project().sequences[2].visible);

    // Deleting a row above the current one keeps the same sequence selected.
    app_mut(&mut h).apply_list(ListAction::Select(2));
    app_mut(&mut h).apply_list(ListAction::Delete(0));
    h.run();
    assert_eq!(names(&h), ["a", "b"]);
    assert_eq!(app(&h).state.sequence, 1);
    // Deleting the current last row falls back to the new last row.
    app_mut(&mut h).apply_list(ListAction::Delete(1));
    h.run();
    assert_eq!(app(&h).state.sequence, 0);
    // Undo brings rows back and the selection stays valid.
    app_mut(&mut h)
        .run_command("edit.undo", &json!({}))
        .unwrap();
    h.run();
    assert_eq!(names(&h).len(), 2);
    assert!(app(&h).state.sequence < 2);
}

#[test]
fn menu_commands_work_on_the_current_sequence() {
    let mut h = demo_harness();
    add_continued(&mut h, "second");
    // No form is needed when only `sequence` is required.
    app_mut(&mut h).invoke("sequence.duplicate");
    h.run();
    assert_eq!(names(&h), ["sun to moon", "second", "second copy"]);
    assert_eq!(app(&h).state.sequence, 2);
    app_mut(&mut h).invoke("sequence.delete");
    h.run();
    assert_eq!(names(&h), ["sun to moon", "second"]);
    assert!(app(&h).form().is_none());
    // Setting a length from a command lands on the selected sequence.
    let before = app(&h).session().project().sequences[0].frames;
    app_mut(&mut h)
        .run_command("sequence.length", &json!({"frames": 9}))
        .unwrap();
    assert_eq!(app(&h).session().project().sequences[1].frames, 9);
    assert_eq!(app(&h).session().project().sequences[0].frames, before);
}

#[test]
fn f2_toggles_the_sequence_list() {
    let mut h = demo_harness();
    assert!(app(&h).state.show_sequence_list);
    let width = app(&h).layout.source1.width();
    h.press_key(egui::Key::F2);
    h.run();
    assert!(!app(&h).state.show_sequence_list);
    assert!(
        app(&h).layout.source1.width() > width,
        "panels use the space"
    );
}

#[test]
fn curve_window_edits_go_through_commands_and_a_drag_is_one_undo_step() {
    let mut h = demo_harness();
    app_mut(&mut h).invoke("curve.feature");
    h.run();
    assert!(app(&h).state.curve_window.is_some());

    let cmd = "curve.feature";
    let points = |h: &H| {
        app(h).session().project().sequences[0]
            .feature_curve
            .points
            .len()
    };
    app_mut(&mut h).apply_curve(cmd, CurveAction::Add { x: 0.5, y: 0.2 });
    assert_eq!(points(&h), 3);
    app_mut(&mut h).apply_curve(cmd, CurveAction::BeginDrag);
    for i in 1..=5 {
        let x = 0.5 + f64::from(i) * 0.05;
        app_mut(&mut h).apply_curve(
            cmd,
            CurveAction::Move {
                index: 1,
                x,
                y: 0.4,
            },
        );
    }
    app_mut(&mut h).apply_curve(cmd, CurveAction::EndDrag);
    // The window ends the group once the pointer is up (no drag in progress).
    h.run();
    let moved = app(&h).session().project().sequences[0]
        .feature_curve
        .points[1];
    assert!((moved.x - 0.75).abs() < 1e-9 && (moved.y - 0.4).abs() < 1e-9);
    assert!(app(&h).session().project().sequences[0]
        .shape_curve
        .is_identity());

    // The drag undoes in a single step; the earlier add is still there.
    app_mut(&mut h)
        .run_command("edit.undo", &json!({}))
        .unwrap();
    let p = app(&h).session().project().sequences[0]
        .feature_curve
        .points[1];
    assert!((p.x - 0.5).abs() < 1e-9 && (p.y - 0.2).abs() < 1e-9);

    app_mut(&mut h).apply_curve(cmd, CurveAction::Remove(1));
    assert_eq!(points(&h), 2);
    app_mut(&mut h).apply_curve(cmd, CurveAction::SetLinear(true));
    assert!(
        app(&h).session().project().sequences[0]
            .feature_curve
            .linear
    );
    app_mut(&mut h).apply_curve(cmd, CurveAction::Reset);
    assert!(
        !app(&h).session().project().sequences[0]
            .feature_curve
            .linear
    );
    // Edits the engine refuses leave the project alone and do not panic.
    app_mut(&mut h).apply_curve(cmd, CurveAction::Remove(0));
    app_mut(&mut h).apply_curve(
        cmd,
        CurveAction::Move {
            index: 7,
            x: 0.1,
            y: 0.1,
        },
    );
    h.run();
    assert_eq!(points(&h), 2);
    assert!(app(&h).status().is_some_and(|s| s.is_error));
}

#[test]
fn a_curve_edit_redraws_the_preview() {
    let mut h = demo_harness();
    app_mut(&mut h).state.frame = 5;
    h.run();
    let before = app(&h).session().revision();
    app_mut(&mut h)
        .run_command("curve.shape", &json!({"op": "add", "x": 0.5, "y": 0.9}))
        .unwrap();
    h.run();
    assert!(app(&h).session().revision() > before);
}

#[test]
fn screenshots_for_a_human_to_look_at() {
    if std::env::var_os("INBETWEEN_UI_SHOTS").is_none() {
        return;
    }
    let mut h = demo_harness();
    add_continued(&mut h, "Moon to sun");
    app_mut(&mut h)
        .run_command(
            "sequence.rename",
            &json!({"sequence": 0, "name": "Sun to moon"}),
        )
        .unwrap();
    app_mut(&mut h).apply_list(ListAction::Select(0));
    app_mut(&mut h)
        .run_command("curve.shape", &json!({"op": "add", "x": 0.3, "y": 0.7}))
        .unwrap();
    app_mut(&mut h).state.frame = 6;
    app_mut(&mut h).invoke("curve.shape");
    h.run();
    shot(&mut h, "sequences-and-curve");
}

fn drag(h: &mut H, from: Pos2, to: Pos2) {
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let frame = |h: &mut H, event| {
        h.input_mut().events.push(event);
        h.step();
    };
    frame(h, Event::PointerMoved(from));
    frame(h, button(from, true));
    for i in 1..=6 {
        frame(
            h,
            Event::PointerMoved(from + (to - from) * (i as f32 / 6.0)),
        );
    }
    frame(h, button(to, false));
    h.run();
}

/// Name three sequences a, b, c and return the on-screen top-left of each row's title.
fn three_rows(h: &mut H) -> Vec<Pos2> {
    add_continued(h, "b");
    add_continued(h, "c");
    app_mut(h)
        .run_command("sequence.rename", &json!({"sequence": 0, "name": "a"}))
        .unwrap();
    h.run();
    ["a", "b", "c"]
        .iter()
        .map(|n| {
            let r = h.get_by_label(n).raw_bounds().unwrap();
            Pos2::new(r.x0 as f32, r.y0 as f32)
        })
        .collect()
}

#[test]
fn list_actions_move_to_copy_frame_and_switch_the_project_type() {
    let mut h = demo_harness();
    three_rows(&mut h);
    app_mut(&mut h).apply_list(ListAction::MoveTo { from: 0, to: 2 });
    h.run();
    assert_eq!(names(&h), ["b", "c", "a"]);

    let first = app(&h).session().project().sequences[2].image1.clone();
    app_mut(&mut h).apply_list(ListAction::CopyFrame {
        from: 2,
        from_image: 1,
        to: 0,
        to_image: 2,
    });
    h.run();
    assert_eq!(app(&h).session().project().sequences[0].image2, first);

    app_mut(&mut h).apply_list(ListAction::ProjectType("layer"));
    h.run();
    assert_eq!(
        app(&h).session().project().kind,
        inbetween_doc::ProjectType::Layer
    );
    let _ = h.get_by_label("b  (layer 1, top)");
    let _ = h.get_by_label("a  (layer 3, below)");
    shot(&mut h, "sequence_list_layers");
    app_mut(&mut h).apply_list(ListAction::ProjectType("sequence"));
    h.run();
    assert_eq!(
        app(&h).session().project().kind,
        inbetween_doc::ProjectType::Sequence
    );
}

#[test]
fn dragging_a_row_onto_another_row_moves_it() {
    let mut h = demo_harness();
    let rows = three_rows(&mut h);
    // Grab the title text of row "a" and drop it on the title of row "c".
    drag(
        &mut h,
        rows[0] + Vec2::new(4.0, 6.0),
        rows[2] + Vec2::new(4.0, 6.0),
    );
    assert_eq!(names(&h), ["b", "c", "a"]);
    // One undo step.
    app_mut(&mut h)
        .run_command("edit.undo", &json!({}))
        .unwrap();
    assert_eq!(names(&h), ["a", "b", "c"]);
}

#[test]
fn dragging_a_thumbnail_onto_another_thumbnail_copies_the_image_and_its_dots() {
    let mut h = demo_harness();
    let rows = three_rows(&mut h);
    let thumb = |row: Pos2, image: u8| {
        // The thumbnails sit left of the title: image 2 is 8 px away, image 1 a thumbnail
        // further.
        let x = row.x - 8.0 - 22.0 - if image == 1 { 52.0 } else { 0.0 };
        Pos2::new(x, row.y + 14.0)
    };
    let source = app(&h).session().project().sequences[0].image1.clone();
    drag(&mut h, thumb(rows[0], 1), thumb(rows[2], 2));
    let seqs = &app(&h).session().project().sequences;
    assert_eq!(seqs[2].image2, source);
    assert_eq!(seqs[0].image1, source, "the source keeps its image");
    // Dropping a thumbnail on a bare title does nothing.
    let before = app(&h).session().project().clone();
    drag(&mut h, thumb(rows[0], 1), rows[1] + Vec2::new(4.0, 6.0));
    assert_eq!(app(&h).session().project().sequences, before.sequences);
}
