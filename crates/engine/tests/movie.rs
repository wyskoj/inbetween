//! `movie.size`, `movie.fps`, `sequence.length` in seconds, and rendering any movie frame.

use inbetween_doc::{DotPair, DotSet, ImageRef, OutputSettings, Project, Sequence};
use inbetween_engine::{CommandError, Registry, Session};
use inbetween_geom::Point;
use inbetween_morph::fit_transform;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("inbetween-mv-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
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

const W: u32 = 64;
const H: u32 = 48;
const MARK1: (u32, u32) = (20, 14);
const MARK2: (u32, u32) = (45, 33);

fn write_png(path: &Path, marker: (u32, u32), other: (u32, u32)) {
    let mut rgba = Vec::new();
    for y in 0..H {
        for x in 0..W {
            let near = |c: (u32, u32)| x.abs_diff(c.0) <= 3 && y.abs_diff(c.1) <= 3;
            let px = if near(marker) {
                [255, 0, 255, 255]
            } else if near(other) {
                [0, 255, 255, 255]
            } else {
                [(x * 3) as u8 / 2, (y * 4) as u8 / 2, 90, 255]
            };
            rgba.extend_from_slice(&px);
        }
    }
    std::fs::write(path, inbetween_codecs::encode_png(W, H, &rgba).unwrap()).unwrap();
}

fn seq(dir: &Dir, frames: u32) -> Sequence {
    let at = |m: (u32, u32)| Point::new(f64::from(m.0) + 0.5, f64::from(m.1) + 0.5);
    Sequence {
        image1: ImageRef::new(dir.s("a.png")),
        image2: ImageRef::new(dir.s("b.png")),
        dots: DotSet::new(vec![
            DotPair::new(at(MARK1), at(MARK1)),
            DotPair::new(at(MARK2), at(MARK2)),
        ]),
        frames,
        ..Sequence::default()
    }
}

fn session(dir: &Dir, sequences: usize) -> Session {
    write_png(&dir.0.join("a.png"), MARK1, MARK2);
    write_png(&dir.0.join("b.png"), MARK2, MARK1);
    let mut s = Session::new();
    s.set_project(
        Project {
            output: OutputSettings {
                width: W,
                height: H,
            },
            sequences: (0..sequences).map(|i| seq(dir, 5 + i as u32)).collect(),
            ..Project::default()
        },
        None,
    );
    s
}

fn run(s: &mut Session, id: &str, params: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, id, &params)
}

fn under(s: &mut Session, dot: usize) -> [f32; 4] {
    let image = s.source(0, 1).unwrap();
    let seq = s.sequence(0).unwrap();
    let out = s.project().output;
    let fit = fit_transform(
        image.width(),
        image.height(),
        out.width,
        out.height,
        seq.fit,
    )
    .unwrap();
    let p = fit.apply(seq.dots.pairs[dot].first);
    image.sample_bilinear(p.x as f32, p.y as f32)
}

fn is_magenta(p: [f32; 4]) -> bool {
    p[0] > 0.9 && p[1] < 0.1 && p[2] > 0.9
}

fn is_cyan(p: [f32; 4]) -> bool {
    p[0] < 0.1 && p[1] > 0.9 && p[2] > 0.9
}

fn on_features(s: &mut Session, step: &str) {
    let (a, b) = (under(s, 0), under(s, 1));
    assert!(is_magenta(a), "{step}: dot 0 is on {a:?}");
    assert!(is_cyan(b), "{step}: dot 1 is on {b:?}");
}

#[test]
fn dots_stay_on_their_features_in_every_fit_mode() {
    for fit in ["stretch", "contain", "cover"] {
        let d = Dir::new(fit);
        let mut s = session(&d, 1);
        run(&mut s, "sequence.fit", json!({"fit": fit})).unwrap();
        on_features(&mut s, "start");
        for (w, h) in [(128, 72), (40, 90), (64, 48), (200, 20)] {
            let r = run(&mut s, "movie.size", json!({"width": w, "height": h})).unwrap();
            assert_eq!(
                (r["width"].as_u64(), r["height"].as_u64()),
                (Some(w), Some(h))
            );
            assert_eq!(
                (s.project().output.width, s.project().output.height),
                (w as u32, h as u32)
            );
            on_features(&mut s, &format!("{fit} {w}x{h}"));
        }
    }
}

#[test]
fn undo_restores_size_and_dots() {
    let d = Dir::new("undo");
    let mut s = session(&d, 1);
    let before = s.project().clone();
    run(&mut s, "movie.size", json!({"preset": "1080p"})).unwrap();
    assert_eq!(s.project().output.width, 1920);
    assert_ne!(s.project().sequences[0].dots, before.sequences[0].dots);
    run(&mut s, "edit.undo", json!({})).unwrap();
    assert_eq!(*s.project(), before);
}

#[test]
fn keep_dots_presets_and_match_image() {
    let d = Dir::new("modes");
    let mut s = session(&d, 1);
    let dots = s.project().sequences[0].dots.clone();
    run(
        &mut s,
        "movie.size",
        json!({"preset": "square1080", "keep_dots": true}),
    )
    .unwrap();
    assert_eq!(s.project().output.width, 1080);
    assert_eq!(s.project().output.height, 1080);
    assert_eq!(s.project().sequences[0].dots, dots);
    // Match Image 1 after a 90 degree turn: the size is the edited one.
    run(
        &mut s,
        "image.rotate",
        json!({"turn": "cw", "keep_dots": true}),
    )
    .unwrap();
    run(
        &mut s,
        "movie.size",
        json!({"match_image": 1, "keep_dots": true}),
    )
    .unwrap();
    assert_eq!(
        (s.project().output.width, s.project().output.height),
        (H, W)
    );
    // No parameters reports and changes nothing.
    let r = run(&mut s, "movie.size", json!({})).unwrap();
    assert_eq!(r["width"], H);
}

#[test]
fn bad_sizes_and_mixed_modes_change_nothing() {
    let d = Dir::new("bad");
    let mut s = session(&d, 1);
    let before = s.project().clone();
    for params in [
        json!({"preset": "nope"}),
        json!({"preset": "720p", "width": 10}),
        json!({"match_image": 1, "preset": "720p"}),
        json!({"width": 0}),
        json!({"width": 99999999}),
        json!({"match_image": 1, "sequence": 5}),
    ] {
        assert!(
            run(&mut s, "movie.size", params.clone()).is_err(),
            "{params}"
        );
        assert_eq!(*s.project(), before, "{params}");
    }
}

#[test]
fn contain_with_a_missing_source_refuses_unless_keep_dots() {
    let d = Dir::new("missing");
    let mut s = session(&d, 1);
    run(&mut s, "sequence.fit", json!({"fit": "contain"})).unwrap();
    std::fs::remove_file(d.0.join("b.png")).unwrap();
    let before = s.project().clone();
    let e = run(&mut s, "movie.size", json!({"width": 100, "height": 100})).unwrap_err();
    assert!(e.to_string().contains("Image"), "{e}");
    assert_eq!(*s.project(), before);
    run(
        &mut s,
        "movie.size",
        json!({"width": 100, "height": 100, "keep_dots": true}),
    )
    .unwrap();
    assert_eq!(s.project().output.width, 100);
}

#[test]
fn captions_scale_with_the_height() {
    let d = Dir::new("caption");
    let mut s = session(&d, 1);
    run(
        &mut s,
        "caption.movie",
        json!({"text": "Hi", "size": 10, "border_size": 2}),
    )
    .unwrap();
    run(&mut s, "caption.sequence", json!({"text": "Yo", "size": 6})).unwrap();
    run(&mut s, "movie.size", json!({"width": 128, "height": 96})).unwrap();
    let c = s.project().caption.clone().unwrap();
    assert_eq!((c.size, c.border_size), (20.0, 4.0));
    assert_eq!(s.project().sequences[0].caption.clone().unwrap().size, 12.0);
}

#[test]
fn morph_ends_still_match_after_a_resize() {
    let d = Dir::new("ends");
    let mut s = session(&d, 1);
    run(&mut s, "movie.size", json!({"width": 96, "height": 54})).unwrap();
    let first = s.render_movie_frame(1).unwrap();
    let last = s.render_movie_frame(5).unwrap();
    let p = s.prepare(0).unwrap();
    let r = p.renderer().unwrap();
    assert_eq!(first, r.render_t(0.0).unwrap());
    assert_eq!(last, r.render_t(1.0).unwrap());
}

#[test]
fn fps_validates_and_survives_save_and_open() {
    let d = Dir::new("fps");
    let mut s = session(&d, 1);
    assert_eq!(run(&mut s, "movie.fps", json!({})).unwrap()["fps"], 24.0);
    let before = s.project().clone();
    for bad in [json!({"fps": 0}), json!({"fps": 61}), json!({"fps": -3})] {
        assert!(run(&mut s, "movie.fps", bad).is_err());
        assert_eq!(*s.project(), before);
    }
    run(&mut s, "movie.fps", json!({"fps": 29.97})).unwrap();
    let path = d.s("p.inbetween");
    run(&mut s, "project.saveAs", json!({"path": path})).unwrap();
    let mut t = Session::new();
    run(&mut t, "project.open", json!({"path": path})).unwrap();
    assert_eq!(t.project().fps, 29.97);
}

#[test]
fn sequence_length_in_seconds_and_apply_all() {
    let d = Dir::new("length");
    let mut s = session(&d, 3);
    run(&mut s, "movie.fps", json!({"fps": 25})).unwrap();
    let r = run(
        &mut s,
        "sequence.length",
        json!({"sequence": 1, "seconds": 2, "hold_start_seconds": 0.5}),
    )
    .unwrap();
    assert_eq!(r["frames"], 50);
    assert_eq!(r["hold_start"], 13); // 12.5 rounds away from zero
    assert_eq!(r["total_frames"], 63);
    assert_eq!(r["seconds"], 63.0 / 25.0);
    // Both spellings of one field is an error.
    assert!(run(
        &mut s,
        "sequence.length",
        json!({"frames": 10, "seconds": 1})
    )
    .is_err());
    // Apply to all.
    let r = run(
        &mut s,
        "sequence.length",
        json!({"frames": 7, "apply_all": true}),
    )
    .unwrap();
    assert_eq!(r["changed"], 3);
    assert!(s.project().sequences.iter().all(|q| q.frames == 7));
    // All or nothing: 1000 frames is refused and nothing changes.
    let before = s.project().clone();
    assert!(run(
        &mut s,
        "sequence.length",
        json!({"seconds": 999, "apply_all": true})
    )
    .is_err());
    assert_eq!(*s.project(), before);
}

#[test]
fn movie_frame_crosses_sequences_and_export_frame_takes_it() {
    let d = Dir::new("movieframe");
    let mut s = session(&d, 2); // 5 + 6 frames
    assert!(s.render_movie_frame(0).is_err());
    assert!(s.render_movie_frame(12).is_err());
    // Movie frame 6 is the first frame of sequence 1.
    let join = s.render_movie_frame(6).unwrap();
    let p = s.prepare(1).unwrap();
    assert_eq!(join, p.renderer().unwrap().render_frame(0).unwrap());
    let out = d.s("m.png");
    run(
        &mut s,
        "export.frame",
        json!({"out": out, "movie_frame": 11}),
    )
    .unwrap();
    let written = inbetween_codecs::read_file(Path::new(&out)).unwrap();
    assert_eq!(written.width, W);
    assert!(run(
        &mut s,
        "export.frame",
        json!({"out": d.s("x.png"), "movie_frame": 2, "t": 0.5})
    )
    .is_err());
    let t = s.movie_thumbnail(3, 16).unwrap();
    assert!(t.width().max(t.height()) <= 16);
}

#[test]
fn undo_keeps_more_than_two_hundred_steps_and_a_byte_budget_for_big_projects() {
    let d = Dir::new("undo-depth");
    let mut s = session(&d, 1);
    for i in 0..500 {
        run(&mut s, "movie.fps", json!({"fps": 20 + (i % 2)})).unwrap();
    }
    assert_eq!(s.undo_depth(), 500, "small projects keep every step");

    // A project with a great many dots costs more per step, so the oldest steps go first,
    // but never fewer than 200 remain.
    let at = |i: u32| Point::new(f64::from(i % 60) + 0.25, f64::from(i / 60) + 0.25);
    let pairs = (0..2048).map(|i| DotPair::new(at(i), at(i))).collect();
    s.set_project(
        Project {
            sequences: vec![Sequence {
                dots: DotSet::new(pairs),
                ..seq(&d, 5)
            }],
            ..Project::default()
        },
        None,
    );
    for i in 0..4000 {
        run(&mut s, "movie.fps", json!({"fps": 20 + (i % 2)})).unwrap();
    }
    let depth = s.undo_depth();
    assert!((200..4000).contains(&depth), "depth {depth}");
    // Undo still walks all the way back through what was kept.
    for _ in 0..depth {
        run(&mut s, "edit.undo", json!({})).unwrap();
    }
    assert!(run(&mut s, "edit.undo", json!({})).is_err());
}

/// No GPU texture can be this wide (limits are 8192 to 16384); exports must not depend on one.
#[test]
fn export_is_not_capped_by_the_gpu() {
    let d = Dir::new("wide");
    let mut s = session(&d, 1);
    run(&mut s, "movie.size", json!({"width": 20000, "height": 16})).unwrap();
    let out = d.s("wide.png");
    run(&mut s, "export.frame", json!({"out": out, "t": 0.5})).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    let img = inbetween_codecs::decode(&bytes).unwrap();
    assert_eq!((img.width, img.height), (20000, 16));
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn export_can_pick_one_sequence() {
    let d = Dir::new("range");
    let mut s = session(&d, 3); // 5, 6 and 7 frames
    let out = d.s("only");
    let r = run(
        &mut s,
        "export.movie",
        json!({"out_dir": out, "sequence": 1}),
    )
    .unwrap();
    assert_eq!(r["frames"], 6);
    assert_eq!(
        files_in(Path::new(&out)),
        (6..=11)
            .map(|n| format!("frame_{n:04}.png"))
            .collect::<Vec<_>>()
    );
    // first and last count inside the sequence.
    let out = d.s("inner");
    let r = run(
        &mut s,
        "export.movie",
        json!({"out_dir": out, "sequence": 2, "first": 2, "last": 3}),
    )
    .unwrap();
    assert_eq!(r["frames"], 2);
    assert_eq!(
        files_in(Path::new(&out)),
        ["frame_0013.png", "frame_0014.png"]
    );
    // Bad picks are errors.
    let bad = |s: &mut Session, p: Value| run(s, "export.movie", p).unwrap_err();
    assert!(matches!(
        bad(&mut s, json!({"out_dir": d.s("x"), "sequence": 9})),
        CommandError::NoSuchSequence { .. }
    ));
    assert!(matches!(
        bad(
            &mut s,
            json!({"out_dir": d.s("x"), "sequence": 0, "first": 3, "last": 2})
        ),
        CommandError::BadParams(_)
    ));
}

#[test]
fn jpeg_quality_and_dpi_are_honoured() {
    let d = Dir::new("opts");
    let mut s = session(&d, 1);
    let (lo, hi) = (d.s("lo.jpg"), d.s("hi.jpg"));
    run(
        &mut s,
        "export.frame",
        json!({"out": lo, "t": 0.5, "quality": 5}),
    )
    .unwrap();
    run(
        &mut s,
        "export.frame",
        json!({"out": hi, "t": 0.5, "quality": 98, "dpi": 300}),
    )
    .unwrap();
    let (lo_b, hi_b) = (std::fs::read(&lo).unwrap(), std::fs::read(&hi).unwrap());
    assert!(lo_b.len() < hi_b.len(), "{} vs {}", lo_b.len(), hi_b.len());
    assert_eq!(hi_b[14..16], 300u16.to_be_bytes());
    let png = d.s("a.png");
    run(
        &mut s,
        "export.frame",
        json!({"out": png, "t": 0.5, "dpi": 72}),
    )
    .unwrap();
    let bytes = std::fs::read(&png).unwrap();
    assert_eq!(&bytes[37..41], b"pHYs");
    assert!(inbetween_codecs::decode(&bytes).is_ok());
    let bad = run(&mut s, "export.frame", json!({"out": png, "dpi": 0}));
    assert!(bad.is_err());
}

#[test]
fn a_running_export_can_be_cancelled() {
    let d = Dir::new("cancel");
    let mut s = session(&d, 1);
    run(&mut s, "movie.size", json!({"width": 512, "height": 512})).unwrap();
    run(&mut s, "sequence.length", json!({"frames": 999})).unwrap();
    let out = d.s("cancelled");
    let flag = s.cancel_handle();
    let watch = PathBuf::from(&out);
    let stopper = std::thread::spawn(move || {
        for _ in 0..20_000 {
            if std::fs::read_dir(&watch).is_ok_and(|r| r.count() >= 3) {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });
    let err = run(&mut s, "export.movie", json!({"out_dir": out})).unwrap_err();
    stopper.join().unwrap();
    assert_eq!(err, CommandError::Cancelled);
    assert!(files_in(Path::new(&out)).len() < 999);
    // The next export starts clean.
    let r = run(
        &mut s,
        "export.movie",
        json!({"out_dir": d.s("again"), "last": 3}),
    )
    .unwrap();
    assert_eq!(r["frames"], 3);
}
