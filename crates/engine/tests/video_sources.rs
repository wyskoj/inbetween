//! Videos as picture sources: the morph runs from a clip's last frame to another's first, and
//! exports play the clips around it. Needs ffmpeg and ffprobe; skipped without them.

use inbetween_engine::{CommandError, Registry, Session};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let d = std::env::temp_dir().join(format!("inbetween-vs-{tag}-{}", std::process::id()));
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

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("-version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn available() -> bool {
    let ok = have("ffmpeg") && have("ffprobe");
    if !ok {
        eprintln!("skipped: no ffmpeg/ffprobe on PATH");
    }
    ok
}

/// A solid-colour clip of `seconds` at 10 frames per second.
fn clip(path: &str, colour: &str, size: &str, seconds: u32) {
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("color=c={colour}:s={size}:r=10:d={seconds}"))
        .args(["-pix_fmt", "yuv420p", path])
        .status()
        .unwrap();
    assert!(status.success());
}

fn run(s: &mut Session, id: &str, p: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, id, &p)
}

fn project(d: &Dir) -> Session {
    clip(&d.s("a.mp4"), "red", "64x48", 1);
    clip(&d.s("b.mp4"), "blue", "64x48", 1);
    let mut s = Session::new();
    run(
        &mut s,
        "tools.sequencePacker",
        json!({"images": [d.s("a.mp4"), d.s("b.mp4")], "frames": 4}),
    )
    .unwrap();
    s
}

fn pixel(path: &str, x: u32, y: u32) -> [u8; 4] {
    let img = inbetween_codecs::read_file(std::path::Path::new(path)).unwrap();
    let i = ((y * img.width + x) * 4) as usize;
    [
        img.data[i],
        img.data[i + 1],
        img.data[i + 2],
        img.data[i + 3],
    ]
}

fn is_red(p: [u8; 4]) -> bool {
    p[0] > 200 && p[1] < 60 && p[2] < 60
}

fn is_blue(p: [u8; 4]) -> bool {
    p[2] > 200 && p[0] < 60 && p[1] < 60
}

#[test]
fn the_morph_runs_from_the_last_frame_of_one_clip_to_the_first_of_the_next() {
    if !available() {
        return;
    }
    let d = Dir::new("morph");
    let mut s = project(&d);
    assert_eq!(
        (s.project().output.width, s.project().output.height),
        (64, 48)
    );
    let p = s.prepare(0).unwrap();
    let r = p.renderer().unwrap();
    let (first, last) = (r.render_t(0.0).unwrap(), r.render_t(1.0).unwrap());
    let at = |img: &inbetween_raster::Image| img.get(32, 24).unwrap();
    assert!(
        at(&first)[0] > 0.75 && at(&first)[2] < 0.25,
        "{:?}",
        at(&first)
    );
    assert!(
        at(&last)[2] > 0.75 && at(&last)[0] < 0.25,
        "{:?}",
        at(&last)
    );
    // A single exported frame needs no clips.
    run(
        &mut s,
        "export.frame",
        json!({"out": d.s("f.png"), "t": 0.0}),
    )
    .unwrap();
    assert!(is_red(pixel(&d.s("f.png"), 32, 24)));
    // The ends of the clips are cached on their own.
    assert!(s.cached_images() >= 2);
}

#[test]
fn exports_play_the_clips_around_the_morph() {
    if !available() {
        return;
    }
    let d = Dir::new("export");
    let mut s = project(&d);
    let r = run(&mut s, "export.movie", json!({"out_dir": d.s("frames")})).unwrap();
    let clip_frames = r["clip_frames"].as_u64().unwrap();
    // One second of each clip at the movie rate, plus the four morph frames.
    let per_clip = s.project().fps.round() as u64;
    assert!(
        clip_frames.abs_diff(2 * per_clip) <= 2,
        "{clip_frames} vs {}",
        2 * per_clip
    );
    let written = r["frames"].as_u64().unwrap();
    assert!(written.abs_diff(4 + clip_frames) <= 2, "{written}");
    let files: Vec<String> = {
        let mut v: Vec<String> = std::fs::read_dir(d.s("frames"))
            .unwrap()
            .map(|e| e.unwrap().path().display().to_string())
            .collect();
        v.sort();
        v
    };
    assert_eq!(files.len() as u64, written);
    // Red clip first, blue clip last, a blend in the middle.
    assert!(is_red(pixel(&files[0], 32, 24)));
    assert!(is_blue(pixel(files.last().unwrap(), 32, 24)));
    // Without clips only the morph is written.
    let r = run(
        &mut s,
        "export.movie",
        json!({"out_dir": d.s("plain"), "video_clips": false}),
    )
    .unwrap();
    assert_eq!(r["frames"], 4);
    assert_eq!(r["clip_frames"], 0);
    // A range, a single sequence or reversing cannot be numbered with clips in.
    for bad in [
        json!({"first": 2}),
        json!({"last": 3}),
        json!({"reverse": true}),
        json!({"sequence": 0}),
    ] {
        let mut p = bad.clone();
        p["out_dir"] = json!(d.s("bad"));
        let e = run(&mut s, "export.movie", p).unwrap_err();
        assert!(e.to_string().contains("whole movie"), "{bad}: {e}");
    }
    // Animated images and video files include the clips too.
    let r = run(
        &mut s,
        "export.animation",
        json!({"out": d.s("a.gif"), "fps": 10}),
    )
    .unwrap();
    assert!(r["clip_frames"].as_u64().unwrap().abs_diff(20) <= 2);
    assert!(r["frames"].as_u64().unwrap().abs_diff(24) <= 2, "{r}");
    let r = run(
        &mut s,
        "export.video",
        json!({"out": d.s("all.mp4"), "fps": 10, "audio": false}),
    )
    .unwrap();
    assert!(r["frames"].as_u64().unwrap().abs_diff(24) <= 2, "{r}");
    assert!(std::fs::metadata(d.s("all.mp4")).unwrap().len() > 500);
}

#[test]
fn clips_are_fitted_to_the_frame_and_mix_with_still_images() {
    if !available() {
        return;
    }
    let d = Dir::new("fit");
    // A portrait clip in a landscape movie with Contain: bars on the sides stay transparent.
    clip(&d.s("tall.mp4"), "green", "24x48", 1);
    let rgba: Vec<u8> = (0..64 * 48).flat_map(|_| [200u8, 200, 0, 255]).collect();
    std::fs::write(
        d.s("still.png"),
        inbetween_codecs::encode_png(64, 48, &rgba).unwrap(),
    )
    .unwrap();
    let mut s = Session::new();
    run(
        &mut s,
        "tools.sequencePacker",
        json!({"images": [d.s("still.png"), d.s("tall.mp4")], "frames": 3}),
    )
    .unwrap();
    run(&mut s, "sequence.fit", json!({"fit": "contain"})).unwrap();
    let r = run(&mut s, "export.movie", json!({"out_dir": d.s("frames")})).unwrap();
    // Only Image 2 is a video: its clip comes after the morph.
    let n = r["frames"].as_u64().unwrap();
    assert!(n.abs_diff(3 + 24) <= 2, "{r}");
    let last = d.0.join("frames").join(format!("frame_{n:04}.png"));
    let last = last.display().to_string();
    assert_eq!(pixel(&last, 2, 24)[3], 0, "the bar is transparent");
    let centre = pixel(&last, 32, 24);
    assert!(
        centre[1] > 100 && centre[0] < 60 && centre[3] == 255,
        "{centre:?}"
    );
    // Stretch fills the frame instead.
    run(&mut s, "sequence.fit", json!({"fit": "stretch"})).unwrap();
    run(&mut s, "export.movie", json!({"out_dir": d.s("stretched")})).unwrap();
    let last =
        d.0.join("stretched")
            .join(format!("frame_{n:04}.png"))
            .display()
            .to_string();
    assert_eq!(pixel(&last, 2, 24)[3], 255);
}

#[test]
fn unreadable_videos_are_errors_that_leave_the_project_alone() {
    if !available() {
        return;
    }
    let d = Dir::new("bad");
    std::fs::write(d.s("junk.mp4"), b"this is not a video").unwrap();
    clip(&d.s("ok.mp4"), "red", "32x32", 1);
    let mut s = Session::new();
    let before = s.project().clone();
    assert!(run(
        &mut s,
        "tools.sequencePacker",
        json!({"images": [d.s("junk.mp4"), d.s("ok.mp4")]})
    )
    .is_err());
    assert_eq!(*s.project(), before);
    assert!(run(
        &mut s,
        "file.wizard",
        json!({"images": [d.s("ok.mp4"), d.s("junk.mp4")]})
    )
    .is_err());
}
