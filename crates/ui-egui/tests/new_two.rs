//! The New From Two Images dialog, driven headlessly: it opens from the menu command, fills its
//! slots, runs `project.new` through the engine, and respects the unsaved-changes prompt.
//!
//! Set `INBETWEEN_UI_SHOTS=some/folder` to also save screenshots of each state there.

use egui::Vec2;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use inbetween_ui_egui::{json, InbetweenApp, PromptChoice, ViewState};

type H = Harness<'static, InbetweenApp>;

fn demo(name: &str) -> String {
    format!("{}/../../demo/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn harness() -> H {
    let app = InbetweenApp::new(&egui::Context::default(), None, ViewState::default());
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 900.0))
        .with_step_dt(1.0 / 24.0)
        .build_state(|ctx, app: &mut InbetweenApp| app.show(ctx), app);
    h.run();
    h
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
fn the_dialog_creates_a_project_from_two_images() {
    let mut h = harness();
    h.state_mut().invoke("project.new");
    h.run();
    assert!(
        h.state().new_two().is_some(),
        "the menu command opens the dialog"
    );
    assert!(h.state().form().is_none(), "not the generic form");
    shot(&mut h, "new_two_empty");

    // Create project does nothing until both images are picked.
    h.get_by_label("Create project").click();
    h.run();
    assert!(h.state().new_two().is_some());
    assert!(!h.state().session().has_sequences());

    let d = h.state_mut().new_two_mut().unwrap();
    d.set(0, demo("sun.png"));
    d.set(1, demo("moon.png"));
    h.run();
    shot(&mut h, "new_two_ready");

    h.get_by_label("Swap start and end images").click();
    h.run();
    assert_eq!(
        h.state().new_two().unwrap().default_name().as_deref(),
        Some("moon to sun")
    );
    h.get_by_label("Swap start and end images").click();
    h.run();

    h.get_by_label("Create project").click();
    h.run();
    assert!(h.state().new_two().is_none(), "success closes the dialog");
    let seqs = &h.state().session().project().sequences;
    assert_eq!(seqs.len(), 1);
    assert!(seqs[0].image1.0.ends_with("sun.png"));
    assert!(seqs[0].image2.0.ends_with("moon.png"));
    assert_eq!(seqs[0].name, "sun to moon");
}

#[test]
fn a_typed_name_is_used_and_slots_can_be_cleared() {
    let mut h = harness();
    h.state_mut().invoke("project.new");
    h.run();
    let d = h.state_mut().new_two_mut().unwrap();
    d.set(0, demo("sun.png"));
    d.set(1, demo("moon.png"));
    d.name = "Day into night".into();
    h.run();
    h.get_by_label("Remove the end image").click();
    h.run();
    assert!(h.state().new_two().unwrap().slots[1].is_none());
    h.state_mut()
        .new_two_mut()
        .unwrap()
        .drop_files(vec![demo("moon.png")], None);
    h.run();
    h.state_mut().submit_new_two();
    h.run();
    assert_eq!(
        h.state().session().project().sequences[0].name,
        "Day into night"
    );
}

#[test]
fn an_unreadable_file_shows_in_its_slot_and_blocks_create() {
    let dir = std::env::temp_dir().join(format!("inbetween-ui-newtwo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let junk = dir.join("scan-003.psd");
    std::fs::write(&junk, b"8BPS but not really").unwrap();

    let mut h = harness();
    h.state_mut().invoke("project.new");
    h.run();
    let d = h.state_mut().new_two_mut().unwrap();
    d.set(0, demo("sun.png"));
    d.set(1, junk.display().to_string());
    h.run();
    shot(&mut h, "new_two_error");
    h.get_by_label("Choose another");
    h.state_mut().submit_new_two();
    h.run();
    assert!(h.state().new_two().is_some());
    assert!(!h.state().session().has_sequences());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn escape_and_cancel_close_without_changing_anything() {
    let mut h = harness();
    h.state_mut().invoke("project.new");
    h.run();
    h.state_mut().new_two_mut().unwrap().set(0, demo("sun.png"));
    h.run();
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().new_two().is_none());

    h.state_mut().invoke("project.new");
    h.run();
    h.press_key(egui::Key::Escape);
    h.run();
    assert!(h.state().new_two().is_none());
    assert!(!h.state().session().has_sequences());
}

#[test]
fn replacing_unsaved_work_asks_first() {
    let mut h = harness();
    h.state_mut()
        .run_command(
            "project.new",
            &json!({"image1": demo("moon.png"), "image2": demo("sun.png")}),
        )
        .unwrap();
    h.state_mut()
        .run_command("sequence.addBlank", &json!({}))
        .unwrap();
    assert!(h.state().session().is_dirty());
    h.run();

    h.state_mut().invoke("project.new");
    h.run();
    let d = h.state_mut().new_two_mut().unwrap();
    d.set(0, demo("sun.png"));
    d.set(1, demo("moon.png"));
    h.state_mut().submit_new_two();
    h.run();
    assert!(h.state().prompt_open());
    assert!(
        h.state().new_two().is_some(),
        "the dialog waits for the answer"
    );
    assert_eq!(h.state().session().project().sequences.len(), 2);

    h.state_mut().answer_prompt(PromptChoice::DontSave);
    h.run();
    assert!(h.state().new_two().is_none());
    let seqs = &h.state().session().project().sequences;
    assert_eq!(seqs.len(), 1);
    assert!(seqs[0].image1.0.ends_with("sun.png"));
}
