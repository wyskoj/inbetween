//! The Export dialogs, driven headlessly: each export command opens its own dialog from the
//! menu id, the dialog builds the command's parameters, and a finished export offers
//! "Show in folder".
//!
//! Set `INBETWEEN_UI_SHOTS=some/folder` to also save screenshots of each dialog there.

use egui::Vec2;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use inbetween_ui_egui::export_dialog::{Content, Kind};
use inbetween_ui_egui::{json, InbetweenApp, ViewState};

type H = Harness<'static, InbetweenApp>;

fn demo_path() -> String {
    format!("{}/../../demo/demo.inbetween", env!("CARGO_MANIFEST_DIR"))
}

fn harness() -> H {
    let mut app = InbetweenApp::new(&egui::Context::default(), None, ViewState::default());
    app.run_command("project.open", &json!({ "path": demo_path() }))
        .unwrap();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 1000.0))
        .with_step_dt(1.0 / 24.0)
        .build_state(|ctx, app: &mut InbetweenApp| app.show(ctx), app);
    h.run();
    h
}

/// What the dialog's primary button says right now.
fn primary(h: &H) -> String {
    let facts = h.state().export_facts();
    h.state()
        .export_dialog()
        .map(|d| d.action_label(&facts))
        .unwrap_or_default()
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

fn out_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("inbetween-ui-exd-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
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

#[test]
fn every_export_command_opens_its_own_dialog_not_the_generic_form() {
    for kind in Kind::ALL {
        let mut h = harness();
        h.state_mut().invoke(kind.id());
        h.run_steps(40);
        let d = h.state().export_dialog();
        assert_eq!(d.map(|d| d.kind), Some(kind), "{}", kind.id());
        assert!(h.state().form().is_none(), "{} opened the form", kind.id());
        h.get_by_label(&primary(&h));
        h.get_by_label("Cancel");
        shot(&mut h, &format!("export_{}", kind.id().replace('.', "_")));
    }
}

#[test]
fn cancel_and_escape_close_the_dialog_and_export_waits_for_a_path() {
    let mut h = harness();
    h.state_mut().invoke("export.frame");
    h.run();
    h.state_mut().export_dialog_mut().unwrap().path.clear();
    h.run();
    h.get_by_label(&primary(&h)).click();
    h.run();
    assert!(h.state().export_dialog().is_some(), "no path, nothing runs");
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().export_dialog().is_none());

    h.state_mut().invoke("export.pdf");
    h.run();
    h.input_mut().events.push(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    assert!(h.state().export_dialog().is_none());
}

#[test]
fn an_image_export_writes_the_file_and_offers_the_folder() {
    let dir = out_dir("image");
    let mut h = harness();
    h.state_mut().invoke("export.frame");
    h.run();
    let file = dir.join("mid.png");
    h.state_mut()
        .export_dialog_mut()
        .unwrap()
        .set_path(file.display().to_string());
    h.run();
    shot(&mut h, "export_image_ready");
    h.get_by_label(&primary(&h)).click();
    h.run();
    assert!(h.state().export_dialog().is_none());
    assert!(file.exists(), "the picture was written");
    assert_eq!(h.state().toast_folder(), Some(dir.as_path()));
    assert_eq!(h.state().toast_file(), Some(file.as_path()));
    let remembered = h.state().state.export_memory.get("export.frame");
    assert_eq!(
        remembered.map(|m| m.dir.clone()),
        Some(dir.display().to_string()),
        "the next image export starts in the same folder"
    );
    shot(&mut h, "export_image_done");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_folder_is_said_beside_the_name_and_can_be_created() {
    let dir = out_dir("missing");
    let mut h = harness();
    h.state_mut().invoke("export.frame");
    h.run();
    let missing = dir.join("blocker");
    h.state_mut()
        .export_dialog_mut()
        .unwrap()
        .set_path(missing.join("x.png").display().to_string());
    h.run();
    h.get_by_label("The folder “blocker” doesn’t exist.");
    shot(&mut h, "export_image_error");
    assert!(!h.state().export_dialog().unwrap().ready());
    h.get_by_label("Create folder").click();
    h.run();
    assert!(missing.is_dir(), "the folder was made");
    assert!(h.state().export_dialog().unwrap().ready());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exporting_over_a_file_asks_for_replace_by_name() {
    let dir = out_dir("replace");
    let mut h = harness();
    h.state_mut().invoke("export.frame");
    h.run();
    let file = dir.join("mid.png");
    std::fs::write(&file, b"old").unwrap();
    h.state_mut()
        .export_dialog_mut()
        .unwrap()
        .set_path(file.display().to_string());
    h.run();
    h.get_by_label("mid.png already exists. Export replaces it.");
    h.get_by_label("Replace").click();
    h.run();
    assert!(h.state().export_dialog().is_none());
    assert!(
        std::fs::metadata(&file).unwrap().len() > 3,
        "it was written over"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_movie_export_runs_from_the_dialog_with_a_progress_window() {
    let dir = out_dir("movie");
    let mut h = harness();
    h.state_mut().invoke("export.movie");
    h.run();
    {
        let d = h.state_mut().export_dialog_mut().unwrap();
        d.set_path(dir.join("frames").display().to_string());
        d.prefix = "shot".into();
        d.all_frames = false;
        d.first = "2".into();
        d.last = "5".into();
    }
    h.run();
    shot(&mut h, "export_sequence_range");
    h.get_by_label(&primary(&h)).click();
    h.step();
    assert!(h.state().export_dialog().is_none());
    assert!(h.state().export_running());
    wait_for_export(&mut h);
    let written = std::fs::read_dir(dir.join("frames")).unwrap().count();
    assert_eq!(written, 4, "frames 2 to 5");
    assert!(dir.join("frames").join("shot_0002.png").exists());
    assert_eq!(h.state().toast_folder(), Some(dir.join("frames").as_path()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_progress_window_shows_a_frame_and_a_stopping_state() {
    let dir = out_dir("progress");
    let mut h = harness();
    h.state_mut()
        .run_command("movie.size", &json!({"width": 512, "height": 512}))
        .unwrap();
    h.state_mut()
        .run_command("sequence.length", &json!({"frames": 999}))
        .unwrap();
    assert!(h.state_mut().start_export(
        "export.movie",
        &json!({"out_dir": dir.display().to_string()})
    ));
    for _ in 0..2000 {
        h.step();
        if std::fs::read_dir(&dir).is_ok_and(|r| r.count() >= 4) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    h.step();
    h.get_by_label("Stop");
    shot(&mut h, "export_progress");
    h.state_mut().cancel_export();
    h.step();
    h.get_by_label("Stopping…");
    shot(&mut h, "export_stopping");
    wait_for_export(&mut h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn video_without_ffmpeg_says_so_and_cannot_export() {
    let mut h = harness();
    h.state_mut().invoke("export.video");
    h.run();
    h.state_mut().export_dialog_mut().unwrap().ffmpeg_found = false;
    h.run();
    h.get_by_label("Video needs ffmpeg");
    h.get_by_label("Choose ffmpeg…");
    shot(&mut h, "export_video_no_ffmpeg");
    let d = h.state().export_dialog().unwrap();
    assert!(!d.ready());
    h.get_by_label("Export frames instead").click();
    h.run();
    assert_eq!(
        h.state().export_dialog().map(|d| d.kind),
        Some(Kind::MovieImages)
    );
}

#[test]
fn the_contact_sheet_and_animation_dialogs_follow_their_choices() {
    let mut h = harness();
    h.state_mut().invoke("export.pdf");
    h.run();
    {
        let d = h.state_mut().export_dialog_mut().unwrap();
        d.content = Content::Frames;
        d.all_frames = false;
        d.more = true;
    }
    h.run();
    shot(&mut h, "export_sheet_frames");
    h.state_mut().export_dialog_mut().unwrap().columns = "99".into();
    h.run();
    h.get_by_label("Pictures across must be between 1 and 12.");
    h.get_by_label("Cancel").click();
    h.run();

    h.state_mut().invoke("export.animation");
    h.run();
    {
        let d = h.state_mut().export_dialog_mut().unwrap();
        d.transparent = false;
        d.more = true;
    }
    h.run();
    h.get_by_label("Background");
    shot(&mut h, "export_animation_more");
}
