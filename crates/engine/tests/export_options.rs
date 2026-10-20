//! `save_first` and `open_after` on the export commands.

use inbetween_engine::{CommandError, Registry, Session};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let d = std::env::temp_dir().join(format!("inbetween-after-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Self(d)
    }

    fn s(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(s: &mut Session, id: &str, p: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, id, &p)
}

/// A saved two-image project, plus a record of what the opener was asked to open.
fn setup(d: &Dir) -> (Session, Arc<Mutex<Vec<PathBuf>>>) {
    for (name, shade) in [("a.png", 10u8), ("b.png", 240)] {
        let rgba: Vec<u8> = (0..16 * 12).flat_map(|_| [shade, 90, 160, 255]).collect();
        std::fs::write(
            d.0.join(name),
            inbetween_codecs::encode_png(16, 12, &rgba).unwrap(),
        )
        .unwrap();
    }
    let mut s = Session::new();
    run(
        &mut s,
        "tools.sequencePacker",
        json!({"images": [d.s("a.png"), d.s("b.png")], "frames": 3}),
    )
    .unwrap();
    run(
        &mut s,
        "project.saveAs",
        json!({"path": d.s("movie.inbetween")}),
    )
    .unwrap();
    let opened = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&opened);
    s.set_opener(Arc::new(move |p: &Path| {
        log.lock().unwrap().push(p.to_path_buf());
        Ok(())
    }));
    (s, opened)
}

#[test]
fn save_first_writes_unsaved_changes_and_open_after_opens_the_result() {
    let d = Dir::new("a");
    let (mut s, opened) = setup(&d);
    run(&mut s, "movie.fps", json!({"fps": 12})).unwrap();
    assert!(s.is_dirty());
    // Without the option the project is left unsaved and nothing is opened.
    let r = run(
        &mut s,
        "export.frame",
        json!({"out": d.s("plain.png"), "t": 0.5}),
    )
    .unwrap();
    assert!(s.is_dirty() && opened.lock().unwrap().is_empty());
    assert!(r.get("opened").is_none() && r.get("saved_project").is_none());

    let out = d.s("first.png");
    let r = run(
        &mut s,
        "export.frame",
        json!({"out": out, "t": 0.5, "save_first": true, "open_after": true}),
    )
    .unwrap();
    assert!(!s.is_dirty());
    assert_eq!(r["saved_project"], d.s("movie.inbetween"));
    assert_eq!(r["opened"], true);
    assert_eq!(*opened.lock().unwrap(), vec![PathBuf::from(&out)]);
    let reloaded = inbetween_format::read_project(Path::new(&d.s("movie.inbetween"))).unwrap();
    assert_eq!(reloaded.fps, 12.0);

    // Clean project: nothing to save, still opens.
    let r = run(
        &mut s,
        "export.frame",
        json!({"out": d.s("second.png"), "save_first": true}),
    )
    .unwrap();
    assert_eq!(r["saved_project"], Value::Null);
}

#[test]
fn folders_and_animations_open_too_and_a_failing_opener_is_a_note_not_an_error() {
    let d = Dir::new("b");
    let (mut s, opened) = setup(&d);
    let dir = d.s("frames");
    run(
        &mut s,
        "export.movie",
        json!({"out_dir": dir, "open_after": true}),
    )
    .unwrap();
    run(
        &mut s,
        "export.sequence",
        json!({"out_dir": d.s("seq"), "open_after": true}),
    )
    .unwrap();
    let gif = d.s("out.gif");
    run(
        &mut s,
        "export.animation",
        json!({"out": gif, "open_after": true, "save_first": true}),
    )
    .unwrap();
    assert_eq!(
        *opened.lock().unwrap(),
        vec![
            PathBuf::from(&dir),
            PathBuf::from(d.s("seq")),
            PathBuf::from(&gif)
        ]
    );
    s.set_opener(Arc::new(|_: &Path| Err("no viewer installed".to_string())));
    let r = run(
        &mut s,
        "export.frame",
        json!({"out": d.s("x.png"), "open_after": true}),
    )
    .unwrap();
    assert_eq!(r["opened"]["error"], "no viewer installed");
    assert!(Path::new(&d.s("x.png")).exists());
}

#[test]
fn save_first_without_a_file_is_skipped_and_a_failed_save_stops_the_export() {
    let d = Dir::new("c");
    let (mut s, _) = setup(&d);
    // A project that was never saved has nowhere to save to.
    let mut fresh = Session::new();
    run(
        &mut fresh,
        "tools.sequencePacker",
        json!({"images": [d.s("a.png"), d.s("b.png")]}),
    )
    .unwrap();
    let r = run(
        &mut fresh,
        "export.frame",
        json!({"out": d.s("n.png"), "save_first": true}),
    )
    .unwrap();
    assert_eq!(r["saved_project"], Value::Null);
    // The project file's folder vanishes: saving fails, and nothing is exported.
    run(&mut s, "movie.fps", json!({"fps": 10})).unwrap();
    let sub = d.0.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    run(
        &mut s,
        "project.saveAs",
        json!({"path": sub.join("m.inbetween").display().to_string()}),
    )
    .unwrap();
    run(&mut s, "movie.fps", json!({"fps": 11})).unwrap();
    std::fs::remove_dir_all(&sub).unwrap();
    let out = d.s("never.png");
    assert!(run(
        &mut s,
        "export.frame",
        json!({"out": out, "save_first": true})
    )
    .is_err());
    assert!(!Path::new(&out).exists());
}
