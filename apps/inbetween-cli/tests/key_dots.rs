//! Key-dot commands through the `ib` binary. Each `ib run` is its own session, so the
//! stateful ones (selection, clipboard) are driven by `indices` or checked for their error.

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
            std::env::temp_dir().join(format!("inbetween-cli-kd-{tag}-{}", std::process::id()));
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

fn ib(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ib"))
        .args(args)
        .output()
        .unwrap()
}

fn run(project: &str, id: &str, params: &str, save: &str) -> Output {
    ib(&[
        "run",
        id,
        "--params",
        params,
        "--project",
        project,
        "--save",
        save,
    ])
}

fn load(path: &str) -> inbetween_doc::Project {
    inbetween_format::read_project(Path::new(path)).unwrap()
}

#[test]
fn transform_by_indices_moves_one_image_only_and_saves() {
    let s = Scratch::new("transform");
    let before = load(&demo_project()).sequences[0].dots.clone();
    let out = s.s("t.inbetween");
    let o = run(
        &demo_project(),
        "dots.transform",
        r#"{"op":"move","dx":3,"dy":-2,"indices":[0,1],"image":2}"#,
        &out,
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let after = load(&out).sequences[0].dots.clone();
    assert_eq!(after.pairs[0].first, before.pairs[0].first);
    assert_eq!(after.pairs[0].second.x, before.pairs[0].second.x + 3.0);
    assert_eq!(after.pairs[1].second.y, before.pairs[1].second.y - 2.0);
    assert_eq!(after.pairs[2], before.pairs[2]);
    // A scale to nothing would stack the dots: refused, with a message.
    let o = run(
        &demo_project(),
        "dots.scale",
        r#"{"sx":0,"sy":0,"indices":[0,1,2]}"#,
        &s.s("bad.inbetween"),
    );
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("same spot"));
}

#[test]
fn selection_and_clipboard_commands_are_listed_and_explain_themselves() {
    let o = ib(&["commands"]);
    let text = String::from_utf8_lossy(&o.stdout).to_string();
    for id in [
        "dots.select",
        "dots.selectAll",
        "dots.transform",
        "dots.nudge",
        "dots.autoMatch",
        "dots.addSamePos",
        "dots.option.autoPartner",
        "edit.copy",
        "edit.paste",
        "lines.addPolyline",
        "view.toggle.grid",
        "prefs.dotStyle",
    ] {
        assert!(text.contains(id), "missing {id}");
    }
    let s = Scratch::new("paste");
    let o = run(&demo_project(), "edit.paste", "{}", &s.s("p.inbetween"));
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("clipboard"));
}

#[test]
fn a_polyline_and_unpaired_dots_survive_save_and_reload() {
    let s = Scratch::new("poly");
    let out = s.s("l.inbetween");
    let o = run(
        &demo_project(),
        "lines.addPolyline",
        r#"{"points":[5,5,60,5,60,40],"close":true}"#,
        &out,
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let dots = load(&out).sequences[0].dots.clone();
    assert_eq!(dots.lines.len(), 3);
    assert!(dots.validate_lines().is_ok());
    // A frame still renders with the extra dots and lines.
    let png = s.s("f.png");
    let o = ib(&["frame", &out, "--t", "0.5", "--out", &png]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}
