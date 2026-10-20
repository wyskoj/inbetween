//! Face detection through the `ib` binary: `face.detect`, then `face.locate` with no
//! parameters, then a rendered middle frame.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/detect/tests/data")
        .join(name)
        .display()
        .to_string()
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("inbetween-cli-detect-{tag}-{}", std::process::id()));
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

#[test]
fn locate_detects_with_no_parameters_and_the_middle_frame_renders() {
    let tmp = Scratch::new("locate");
    let start = tmp.s("start.inbetween");
    let params = format!(
        r#"{{"image1": {:?}, "image2": {:?}}}"#,
        fixture("portrait_a.png"),
        fixture("portrait_b.png")
    );
    let made = ib(&["run", "project.new", "--params", &params, "--save", &start]);
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );

    // face.detect reports one face per image.
    let report = ib(&["run", "face.detect", "--project", &start, "--params", "{}"]);
    assert!(
        report.status.success(),
        "{}",
        String::from_utf8_lossy(&report.stderr)
    );
    let text = String::from_utf8_lossy(&report.stdout);
    assert!(text.contains("\"landmarks\""), "{text}");

    // face.locate with `{}` detects and leaves 38 dot pairs behind.
    let out = tmp.s("out.inbetween");
    let placed = ib(&[
        "run",
        "face.locate",
        "--params",
        "{}",
        "--project",
        &start,
        "--save",
        &out,
    ]);
    assert!(
        placed.status.success(),
        "{}",
        String::from_utf8_lossy(&placed.stderr)
    );
    let project = inbetween_format::read_project(Path::new(&out)).unwrap();
    assert_eq!(project.sequences[0].dots.len(), 38);

    let mid = tmp.s("mid.png");
    let frame = ib(&["frame", &out, "--t", "0.5", "--out", &mid]);
    assert!(
        frame.status.success(),
        "{}",
        String::from_utf8_lossy(&frame.stderr)
    );
    assert!(std::fs::metadata(&mid).unwrap().len() > 1000);
}

#[test]
fn a_missing_face_is_an_actionable_error() {
    let tmp = Scratch::new("noface");
    let start = tmp.s("start.inbetween");
    let sun = tmp.s("flat.png");
    let flat = inbetween_raster::Image::filled(120, 90, [0.2, 0.5, 0.8, 1.0]).unwrap();
    std::fs::write(
        &sun,
        inbetween_codecs::encode_png(120, 90, &flat.to_rgba8()).unwrap(),
    )
    .unwrap();
    let params = format!(r#"{{"image1": {sun:?}, "image2": {sun:?}}}"#);
    assert!(
        ib(&["run", "project.new", "--params", &params, "--save", &start])
            .status
            .success()
    );
    let out = ib(&["run", "face.locate", "--params", "{}", "--project", &start]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("no face found in image 1 of sequence 0"),
        "{err}"
    );
}
