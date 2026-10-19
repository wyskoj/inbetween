//! Source edits and captions through the `ib` binary: run a command on the demo project, save,
//! and render frame 0 (which shows Image 1 as edited).

use inbetween_codecs::read_file;
use inbetween_raster::{filter, transform, Image};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn demo_project() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../demo/demo.inbetween")
        .display()
        .to_string()
}

fn sun() -> Image {
    let rgba =
        read_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo/sun.png")).unwrap();
    Image::from_rgba8(rgba.width, rgba.height, &rgba.data).unwrap()
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("inbetween-cli-edit-{tag}-{}", std::process::id()));
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

/// Run `id` on `project`, save to `out`, and return the frame at t = 0 as 8-bit RGBA.
fn edit_and_render(s: &Scratch, project: &str, id: &str, params: &str, out: &str) -> Vec<u8> {
    let saved = s.s(out);
    let o = ib(&[
        "run",
        id,
        "--params",
        params,
        "--project",
        project,
        "--save",
        &saved,
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let png = s.s(&format!("{out}.png"));
    let o = ib(&["frame", &saved, "--t", "0", "--out", &png]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    read_file(Path::new(&png)).unwrap().data
}

fn close(a: &[u8], b: &[u8]) {
    assert_eq!(a.len(), b.len());
    let worst = a
        .iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0);
    assert!(worst <= 1, "differs by {worst} levels");
}

#[test]
fn rotate_run_save_and_frame_equal_the_raster_rotation() {
    let s = Scratch::new("rotate");
    let got = edit_and_render(
        &s,
        &demo_project(),
        "image.rotate",
        r#"{"turn":"180"}"#,
        "r.inbetween",
    );
    let want = transform::rotate_quarter(&sun(), 2).unwrap().to_rgba8();
    close(&got, &want);
    // A quarter turn is saved in the project file.
    let o = ib(&[
        "run",
        "image.rotate",
        "--params",
        r#"{"turn":"cw"}"#,
        "--project",
        &demo_project(),
        "--save",
        &s.s("q.inbetween"),
    ]);
    assert!(o.status.success());
    let text = std::fs::read_to_string(s.s("q.inbetween")).unwrap();
    assert!(text.contains("quarter_turns"), "{text}");
}

#[test]
fn a_gaussian_filter_through_the_cli_equals_the_raster_op() {
    let s = Scratch::new("gauss");
    let got = edit_and_render(
        &s,
        &demo_project(),
        "image.filter.gaussian",
        r#"{"sigma":3}"#,
        "g.inbetween",
    );
    let want = filter::gaussian(&sun(), 3.0).unwrap().to_rgba8();
    close(&got, &want);
}

#[test]
fn a_movie_caption_shows_in_the_rendered_frames() {
    let s = Scratch::new("caption");
    let plain = edit_and_render(
        &s,
        &demo_project(),
        "image.adjust",
        r#"{"brightness":0}"#,
        "p.inbetween",
    );
    let got = edit_and_render(
        &s,
        &demo_project(),
        "caption.movie",
        r##"{"text":"CLI caption","size":40,"color":"#ff00ff","y":0.5}"##,
        "c.inbetween",
    );
    assert_ne!(got, plain);
    let magenta = |d: &[u8]| {
        d.chunks_exact(4)
            .filter(|p| p[0] > 240 && p[1] < 20 && p[2] > 240)
            .count()
    };
    assert!(magenta(&got) > 100, "{} magenta pixels", magenta(&got));
    assert_eq!(magenta(&plain), 0);
}
