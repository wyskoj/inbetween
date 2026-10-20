//! Movie settings through the `ib` binary: size, frame rate, lengths in seconds, and
//! rendering a movie frame.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn demo_project() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../demo/demo.inbetween")
        .display()
        .to_string()
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("inbetween-cli-mv-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn s(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(project: &str, id: &str, params: &str, save: Option<&str>) -> Output {
    let mut args = vec!["run", id, "--params", params, "--project", project];
    if let Some(save) = save {
        args.extend(["--save", save]);
    }
    Command::new(env!("CARGO_BIN_EXE_ib"))
        .args(args)
        .output()
        .unwrap()
}

fn load(path: &str) -> inbetween_doc::Project {
    inbetween_format::read_project(Path::new(path)).unwrap()
}

fn ok(o: &Output) {
    assert!(
        o.status.success(),
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
}

#[test]
fn size_fps_and_length_can_be_set_and_saved() {
    let s = Scratch::new("settings");
    let first = s.s("a.inbetween");
    ok(&run(
        &demo_project(),
        "movie.size",
        r#"{"preset":"720p"}"#,
        Some(&first),
    ));
    let p = load(&first);
    assert_eq!((p.output.width, p.output.height), (1280, 720));

    let second = s.s("b.inbetween");
    ok(&run(&first, "movie.fps", r#"{"fps":25}"#, Some(&second)));
    let third = s.s("c.inbetween");
    ok(&run(
        &second,
        "sequence.length",
        r#"{"seconds":2,"hold_end_seconds":0.4}"#,
        Some(&third),
    ));
    let p = load(&third);
    assert_eq!(p.fps, 25.0);
    assert_eq!(
        (p.sequences[0].frames, p.sequences[0].hold_end),
        (50, 10),
        "seconds are converted at the saved frame rate"
    );
}

#[test]
fn bad_settings_fail_with_a_message_and_write_nothing() {
    let s = Scratch::new("bad");
    for (id, params) in [
        ("movie.size", r#"{"preset":"nope"}"#),
        ("movie.size", r#"{"width":0}"#),
        ("movie.fps", r#"{"fps":0}"#),
        ("movie.fps", r#"{"fps":120}"#),
        ("sequence.length", r#"{"frames":10,"seconds":1}"#),
    ] {
        let out = s.s("never.inbetween");
        let o = run(&demo_project(), id, params, Some(&out));
        assert!(!o.status.success(), "{id} {params}");
        assert!(!String::from_utf8_lossy(&o.stderr).is_empty(), "{id}");
        assert!(!Path::new(&out).exists(), "{id} {params}");
    }
}

#[test]
fn a_movie_frame_renders_from_the_command_line() {
    let s = Scratch::new("frame");
    let out = s.s("m.png");
    ok(&run(
        &demo_project(),
        "export.frame",
        &format!(r#"{{"out":{:?},"movie_frame":5}}"#, out),
        None,
    ));
    let image = inbetween_codecs::read_file(Path::new(&out)).unwrap();
    assert!(image.width > 0 && image.height > 0);
    let o = run(
        &demo_project(),
        "export.frame",
        &format!(r#"{{"out":{:?},"movie_frame":99999}}"#, s.s("x.png")),
        None,
    );
    assert!(!o.status.success());
}
