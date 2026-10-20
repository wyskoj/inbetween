//! `effect.sound`: the model, refusals, saving, and muxing into video.

use inbetween_doc::{DotPair, DotSet, ImageRef, OutputSettings, Project, Sequence};
use inbetween_engine::{CommandError, Registry, Session};
use inbetween_geom::Point;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};

struct Dir(PathBuf);

impl Dir {
    fn s(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A mono 16-bit WAV of `seconds` of a tone.
fn wav(seconds: f64) -> Vec<u8> {
    let rate = 8000u32;
    let n = (seconds * f64::from(rate)) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + n * 2).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(n * 2).to_le_bytes());
    for i in 0..n {
        let v = ((f64::from(i) * 0.2).sin() * 10000.0) as i16;
        b.extend_from_slice(&v.to_le_bytes());
    }
    b
}

fn session(tag: &str) -> (Session, Dir) {
    let dir = std::env::temp_dir().join(format!("inbetween-snd-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let png = inbetween_codecs::encode_png(16, 12, &[90, 40, 200, 255].repeat(16 * 12)).unwrap();
    std::fs::write(dir.join("a.png"), &png).unwrap();
    std::fs::write(dir.join("b.png"), &png).unwrap();
    std::fs::write(dir.join("tone.wav"), wav(0.5)).unwrap();
    let corners = [(0.0, 0.0), (15.0, 0.0), (0.0, 11.0), (15.0, 11.0)];
    let dots = DotSet::new(
        corners
            .iter()
            .map(|&(x, y)| DotPair::new(Point::new(x, y), Point::new(x, y)))
            .collect(),
    );
    let mut s = Session::new();
    s.set_project(
        Project {
            output: OutputSettings {
                width: 16,
                height: 12,
            },
            fps: 10.0,
            sequences: vec![Sequence {
                image1: ImageRef::new(dir.join("a.png").display().to_string()),
                image2: ImageRef::new(dir.join("b.png").display().to_string()),
                dots,
                frames: 20, // two seconds at 10 fps
                ..Sequence::default()
            }],
            ..Project::default()
        },
        None,
    );
    (s, Dir(dir))
}

fn run(s: &mut Session, id: &str, params: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, id, &params)
}

#[test]
fn a_sound_is_probed_set_changed_and_undone() {
    let (mut s, d) = session("set");
    let r = run(
        &mut s,
        "effect.sound",
        json!({"path": d.s("tone.wav"), "loop": true, "fade_out": 0.25}),
    )
    .unwrap();
    assert_eq!(r["enabled"], true);
    assert_eq!(r["loop"], true);
    assert_eq!(r["rate"], 8000);
    assert!((r["duration"].as_f64().unwrap() - 0.5).abs() < 1e-6);
    assert!(s
        .project()
        .sound
        .as_ref()
        .is_some_and(|x| x.is_active() && x.looped));
    run(&mut s, "effect.sound", json!({"enabled": false})).unwrap();
    assert!(!s.project().sound.as_ref().unwrap().enabled);
    run(&mut s, "edit.undo", json!({})).unwrap();
    assert!(s.project().sound.as_ref().unwrap().enabled);
    run(&mut s, "effect.sound", json!({"clear": true})).unwrap();
    assert!(s.project().sound.is_none());
}

#[test]
fn bad_sounds_are_refused_and_change_nothing() {
    let (mut s, d) = session("bad");
    std::fs::write(d.0.join("junk.wav"), b"not a sound").unwrap();
    for params in [
        json!({"path": d.s("junk.wav")}),
        json!({"path": d.s("missing.wav")}),
        json!({"path": d.s("a.png")}),
        json!({"fade_in": 2}),
        json!({"enabled": true}),
        json!({"path": d.s("tone.wav"), "fade_in": 99}),
    ] {
        assert!(
            run(&mut s, "effect.sound", params.clone()).is_err(),
            "{params}"
        );
    }
    assert!(s.project().sound.is_none());
}

#[test]
fn the_sound_is_saved_and_rebased_with_the_project() {
    let (mut s, d) = session("save");
    run(
        &mut s,
        "effect.sound",
        json!({"path": d.s("tone.wav"), "fade_in": 0.1}),
    )
    .unwrap();
    let sub = d.0.join("moved");
    std::fs::create_dir_all(&sub).unwrap();
    let file = sub.join("p.inbetween").display().to_string();
    run(&mut s, "project.save", json!({"path": file})).unwrap();
    let mut again = Session::new();
    run(&mut again, "project.open", json!({"path": file})).unwrap();
    let r = run(&mut again, "effect.sound", json!({})).unwrap();
    assert!((r["duration"].as_f64().unwrap() - 0.5).abs() < 1e-6, "{r}");
    assert_eq!(r["fade_in"], 0.1);
}

fn have(program: &str) -> bool {
    Command::new(program)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[test]
fn a_video_export_carries_the_sound_and_audio_false_leaves_it_out() {
    if !(have("ffmpeg") && have("ffprobe")) {
        eprintln!("skipped: no ffmpeg/ffprobe on PATH");
        return;
    }
    let (mut s, d) = session("mux");
    run(
        &mut s,
        "effect.sound",
        json!({"path": d.s("tone.wav"), "loop": true}),
    )
    .unwrap();
    let r = run(&mut s, "export.video", json!({"out": d.s("with.mp4")})).unwrap();
    assert!(r["audio"].as_str().unwrap().ends_with("tone.wav"), "{r}");
    run(
        &mut s,
        "export.video",
        json!({"out": d.s("without.mp4"), "audio": false}),
    )
    .unwrap();
    let streams = |file: &str, kind: &str| -> String {
        let out = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                kind,
                "-show_entries",
                "stream=duration",
                "-of",
                "csv=p=0",
            ])
            .arg(d.s(file))
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    let audio = streams("with.mp4", "a");
    assert!(!audio.is_empty(), "no audio stream");
    let secs: f64 = audio.lines().next().unwrap().parse().unwrap();
    assert!((secs - 2.0).abs() < 0.2, "audio is {secs} s");
    assert!(streams("without.mp4", "a").is_empty());
    // Animated images have no sound and say so.
    let r = run(&mut s, "export.animation", json!({"out": d.s("a.gif")})).unwrap();
    assert_eq!(r["audio"], "skipped (format has no sound)");
}
