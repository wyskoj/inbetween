//! The New From Image List dialog, driven headlessly: it opens from the menu command, fills and
//! reorders its list, runs `file.wizard` through the engine, and respects the unsaved-changes
//! prompt.
//!
//! Set `INBETWEEN_UI_SHOTS=some/folder` to also save screenshots of each state there.

use egui::Vec2;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use inbetween_doc::{MorphType, ProjectType};
use inbetween_ui_egui::new_list::PlayAs;
use inbetween_ui_egui::{json, InbetweenApp, PromptChoice, ViewState};

type H = Harness<'static, InbetweenApp>;

fn demo(name: &str) -> String {
    format!("{}/../../demo/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn harness() -> H {
    let app = InbetweenApp::new(&egui::Context::default(), None, ViewState::default());
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 960.0))
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

fn names(h: &H) -> Vec<String> {
    h.state()
        .new_list()
        .unwrap()
        .images
        .iter()
        .map(|s| s.file_name())
        .collect()
}

#[test]
fn the_dialog_chains_the_listed_images() {
    let mut h = harness();
    h.state_mut().invoke("file.wizard");
    h.run();
    assert!(h.state().new_list().is_some(), "the menu command opens it");
    assert!(h.state().form().is_none(), "not the generic form");
    shot(&mut h, "new_list_empty");

    h.get_by_label("Create project").click();
    h.run();
    assert!(h.state().new_list().is_some(), "nothing to create yet");

    let d = h.state_mut().new_list_mut().unwrap();
    d.add(vec![demo("sun.png"), demo("moon.png"), demo("sun.png")]);
    d.looped = true;
    h.run();
    h.get_by_label("Loop back to the first image");
    shot(&mut h, "new_list_filled");

    // ("Layers" alone also names the main window's layout control.)
    h.state_mut().new_list_mut().unwrap().play_as = PlayAs::Layers;
    h.run();
    h.get_by_label("One after another").click();
    h.run();
    assert_eq!(h.state().new_list().unwrap().play_as, PlayAs::Sequence);

    h.get_by_label("Create project").click();
    h.run();
    assert!(h.state().new_list().is_none(), "success closes the dialog");
    let project = h.state().session().project();
    assert_eq!(project.kind, ProjectType::Sequence);
    let seqs = &project.sequences;
    assert_eq!(seqs.len(), 3, "two links and the loop back");
    assert!(seqs[2].image1.0.ends_with("sun.png") && seqs[2].image2.0.ends_with("sun.png"));
    assert_eq!(seqs[0].name, "sun to moon");
}

#[test]
fn rows_move_and_remove_from_their_buttons() {
    let mut h = harness();
    h.state_mut().invoke("file.wizard");
    h.run();
    h.state_mut()
        .new_list_mut()
        .unwrap()
        .add(vec![demo("sun.png"), demo("moon.png")]);
    h.run();
    // The row's buttons show on hover.
    h.get_by_label("moon.png").hover();
    h.run_steps(2);
    h.get_by_label("Move moon.png up").click();
    h.run_steps(2);
    assert_eq!(names(&h), ["moon.png", "sun.png"]);
    h.get_by_label("sun.png").hover();
    h.run_steps(2);
    h.get_by_label("Remove sun.png").click();
    h.run_steps(2);
    assert_eq!(names(&h), ["moon.png"]);
    assert!(!h.state().new_list().unwrap().ready());
}

#[test]
fn settings_reach_the_command() {
    let mut h = harness();
    h.state_mut().invoke("file.wizard");
    h.run();
    let d = h.state_mut().new_list_mut().unwrap();
    d.add(vec![demo("sun.png"), demo("moon.png")]);
    d.play_as = PlayAs::Layers;
    d.morph = MorphType::Warp1;
    d.frames = "12".into();
    d.width = "64".into();
    d.height = "48".into();
    h.run();
    h.state_mut().submit_new_list();
    h.run();
    let project = h.state().session().project();
    assert_eq!(project.kind, ProjectType::Layer);
    assert_eq!((project.output.width, project.output.height), (64, 48));
    assert_eq!(project.sequences[0].morph, MorphType::Warp1);
    assert_eq!(project.sequences[0].frames, 12);
}

#[test]
fn an_unreadable_file_or_bad_number_blocks_create() {
    let dir = std::env::temp_dir().join(format!("inbetween-ui-newlist-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let junk = dir.join("scan-003.psd");
    std::fs::write(&junk, b"8BPS but not really").unwrap();

    let mut h = harness();
    h.state_mut().invoke("file.wizard");
    h.run();
    let d = h.state_mut().new_list_mut().unwrap();
    d.add(vec![
        demo("sun.png"),
        junk.display().to_string(),
        demo("moon.png"),
    ]);
    d.play_as = PlayAs::Layers;
    d.frames = "48".into();
    h.run();
    shot(&mut h, "new_list_error");
    h.get_by_label("Replace");
    h.state_mut().submit_new_list();
    h.run();
    assert!(h.state().new_list().is_some());
    assert!(!h.state().session().has_sequences());

    let d = h.state_mut().new_list_mut().unwrap();
    d.remove(1);
    d.frames = "1".into();
    h.run();
    assert!(!h.state().new_list().unwrap().ready());
    h.state_mut().new_list_mut().unwrap().frames.clear();
    h.state_mut().submit_new_list();
    h.run();
    assert_eq!(h.state().session().project().sequences.len(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn escape_and_cancel_close_without_changing_anything() {
    let mut h = harness();
    h.state_mut().invoke("file.wizard");
    h.run();
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().new_list().is_none());

    h.state_mut().invoke("file.wizard");
    h.run();
    h.press_key(egui::Key::Escape);
    h.run();
    assert!(h.state().new_list().is_none());
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
    h.run();

    h.state_mut().invoke("file.wizard");
    h.run();
    h.state_mut().new_list_mut().unwrap().add(vec![
        demo("sun.png"),
        demo("moon.png"),
        demo("sun.png"),
    ]);
    h.state_mut().submit_new_list();
    h.run();
    assert!(h.state().prompt_open());
    assert!(
        h.state().new_list().is_some(),
        "the dialog waits for the answer"
    );

    h.state_mut().answer_prompt(PromptChoice::DontSave);
    h.run();
    assert!(h.state().new_list().is_none());
    assert_eq!(h.state().session().project().sequences.len(), 2);
}
