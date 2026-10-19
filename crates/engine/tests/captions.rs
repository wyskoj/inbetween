//! Captions at the three scopes: image, sequence and movie.

use inbetween_doc::{
    Curve, CurvePoint, DotPair, DotSet, ImageRef, OutputSettings, Project, Sequence,
};
use inbetween_engine::{CommandError, Registry, Session};
use inbetween_geom::Point;
use inbetween_raster::Image;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const W: u32 = 160;
const H: u32 = 100;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("inbetween-cap-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn s(&self, name: &str) -> String {
        self.path(name).display().to_string()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_flat(path: &Path, grey: u8) {
    let rgba = [grey, grey, grey, 255].repeat((W * H) as usize);
    std::fs::write(path, inbetween_codecs::encode_png(W, H, &rgba).unwrap()).unwrap();
}

fn corners(shift: f64) -> DotSet {
    let pts = [
        (0.0, 0.0),
        (159.0, 0.0),
        (0.0, 99.0),
        (159.0, 99.0),
        (80.0, 50.0),
    ];
    DotSet::new(
        pts.iter()
            .map(|&(x, y)| DotPair::new(Point::new(x, y), Point::new(x + shift, y)))
            .collect(),
    )
}

fn session(dir: &Dir, shift: f64) -> Session {
    write_flat(&dir.path("a.png"), 30);
    write_flat(&dir.path("b.png"), 30);
    let mut s = Session::new();
    s.set_project(
        Project {
            output: OutputSettings {
                width: W,
                height: H,
            },
            sequences: vec![Sequence {
                image1: ImageRef::new(dir.s("a.png")),
                image2: ImageRef::new(dir.s("b.png")),
                dots: corners(shift),
                frames: 4,
                ..Sequence::default()
            }],
            ..Project::default()
        },
        None,
    );
    s
}

fn run(s: &mut Session, id: &str, params: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, id, &params)
}

/// Brightness above the flat grey background: the ink of a white caption.
fn ink(img: &Image) -> f32 {
    img.as_slice()
        .chunks_exact(4)
        .map(|p| (p[0] - 30.0 / 255.0).max(0.0))
        .sum()
}

fn centroid_x(img: &Image) -> f32 {
    let (mut sum, mut total) = (0.0, 0.0);
    for (i, p) in img.as_slice().chunks_exact(4).enumerate() {
        let v = (p[0] - 30.0 / 255.0).max(0.0);
        sum += v * (i % W as usize) as f32;
        total += v;
    }
    sum / total.max(1e-6)
}

fn frame(s: &mut Session, n: u32) -> Image {
    s.prepare(0)
        .unwrap()
        .renderer()
        .unwrap()
        .render_frame(n)
        .unwrap()
}

fn read_png(path: &Path) -> Image {
    let rgba = inbetween_codecs::read_file(path).unwrap();
    Image::from_rgba8(rgba.width, rgba.height, &rgba.data).unwrap()
}

#[test]
fn the_movie_caption_shows_on_every_exported_frame() {
    let d = Dir::new("movie");
    let mut s = session(&d, 0.0);
    run(&mut s, "export.movie", json!({"out_dir": d.s("plain")})).unwrap();
    for n in 1..=4 {
        assert_eq!(
            ink(&read_png(&d.path(&format!("plain/frame_000{n}.png")))),
            0.0
        );
    }
    run(
        &mut s,
        "caption.movie",
        json!({"text": "MOVIE", "size": 30, "y": 0.5}),
    )
    .unwrap();
    run(&mut s, "export.movie", json!({"out_dir": d.s("out")})).unwrap();
    let inks: Vec<f32> = (1..=4)
        .map(|n| ink(&read_png(&d.path(&format!("out/frame_000{n}.png")))))
        .collect();
    assert!(inks.iter().all(|v| *v > 20.0), "{inks:?}");
    assert!(
        inks.windows(2).all(|w| (w[0] - w[1]).abs() < 0.5),
        "{inks:?}"
    );
    // Single exported frames get it too.
    run(
        &mut s,
        "export.frame",
        json!({"out": d.s("one.png"), "frame": 2}),
    )
    .unwrap();
    assert!(ink(&read_png(&d.path("one.png"))) > 20.0);
    // It is in the project, not in any sequence.
    assert!(s.sequence(0).unwrap().caption.is_none());
    assert_eq!(s.project().caption.as_ref().unwrap().text, "MOVIE");
    // Cleared, it is gone.
    run(&mut s, "caption.movie", json!({"clear": true})).unwrap();
    run(
        &mut s,
        "export.frame",
        json!({"out": d.s("two.png"), "frame": 2}),
    )
    .unwrap();
    assert_eq!(ink(&read_png(&d.path("two.png"))), 0.0);
}

#[test]
fn the_sequence_caption_is_on_every_frame_and_fades_with_the_master_curve() {
    let d = Dir::new("seq");
    let mut s = session(&d, 0.0);
    run(
        &mut s,
        "caption.sequence",
        json!({"text": "Sequence", "size": 24, "y": 0.5}),
    )
    .unwrap();
    let inks: Vec<f32> = (0..4).map(|n| ink(&frame(&mut s, n))).collect();
    assert!(inks.iter().all(|v| *v > 20.0), "{inks:?}");
    s.sequence_mut(0).unwrap().master_curve = Curve {
        points: vec![CurvePoint::new(0.0, 1.0), CurvePoint::new(1.0, 0.0)],
        linear: true,
    };
    let last = frame(&mut s, 3);
    assert!(
        last.as_slice().iter().all(|v| *v == 0.0),
        "the fade covers the caption"
    );
    assert!(ink(&frame(&mut s, 0)) > 20.0);
    assert!(
        s.sequence(0).unwrap().needs_post_pass(),
        "the GPU path is skipped"
    );
}

#[test]
fn the_image_caption_warps_with_its_image() {
    let d = Dir::new("image");
    let mut s = session(&d, 20.0);
    run(
        &mut s,
        "image.caption",
        json!({"text": "Warp", "size": 24, "y": 0.5, "x": 0.4}),
    )
    .unwrap();
    let at0 = frame(&mut s, 0);
    let mid = frame(&mut s, 2); // t = 2/3 of the way: shape moved ~13 px
    let (c0, c2) = (centroid_x(&at0), centroid_x(&mid));
    assert!(ink(&at0) > 20.0 && ink(&mid) > 5.0);
    assert!(
        c2 - c0 > 6.0 && c2 - c0 < 20.0,
        "centroid moved {c0} -> {c2}"
    );
    // The caption is part of Image 1 only: it is gone at the end.
    assert!(ink(&frame(&mut s, 3)) < 0.5);
    // It also changes the processed source (and so the cache entry).
    let raw_edit_free = s.sequence(0).unwrap().edit2.is_identity();
    assert!(raw_edit_free);
    let a = s.source(0, 1).unwrap();
    run(&mut s, "image.caption", json!({"text": "Other"})).unwrap();
    let b = s.source(0, 1).unwrap();
    assert_ne!(*a, *b);
}

#[test]
fn fields_merge_clear_and_report() {
    let d = Dir::new("fields");
    let mut s = session(&d, 0.0);
    let r = run(&mut s, "caption.sequence", json!({})).unwrap();
    assert!(r["caption"].is_null(), "nothing yet");
    run(
        &mut s,
        "caption.sequence",
        json!({"text": "A\nB", "bold": true}),
    )
    .unwrap();
    run(
        &mut s,
        "caption.sequence",
        json!({
            "italic": true, "align": "right", "color": "#ff8000", "border_size": 2,
            "border_color": "#00000080", "x": 0.75, "y": 0.25, "size": 33,
            "shadow_size": 3, "shadow_distance": 5, "shadow_angle": 30, "shadow_color": "#112233"
        }),
    )
    .unwrap();
    let c = s.sequence(0).unwrap().caption.clone().unwrap();
    assert_eq!(c.text, "A\nB");
    assert!(c.bold && c.italic);
    assert_eq!(c.color, [255, 128, 0, 255]);
    assert_eq!(c.border_color, [0, 0, 0, 128]);
    assert_eq!((c.x, c.y, c.size, c.border_size), (0.75, 0.25, 33.0, 2.0));
    assert_eq!(
        (c.shadow.size, c.shadow.distance, c.shadow.angle),
        (3.0, 5.0, 30.0)
    );
    assert_eq!(c.shadow.color, [0x11, 0x22, 0x33, 255]);
    let r = run(&mut s, "caption.sequence", json!({"enabled": false})).unwrap();
    assert_eq!(r["caption"]["enabled"], false);
    assert_eq!(r["caption"]["text"], "A\nB", "settings survive hiding");
    assert_eq!(ink(&frame(&mut s, 0)), 0.0);
    run(&mut s, "caption.sequence", json!({"clear": true})).unwrap();
    assert!(s.sequence(0).unwrap().caption.is_none());
    // Image 2 has its own caption.
    run(&mut s, "image.caption", json!({"image": 2, "text": "two"})).unwrap();
    assert!(s.sequence(0).unwrap().edit1.is_identity());
    assert_eq!(
        s.sequence(0).unwrap().edit2.caption.as_ref().unwrap().text,
        "two"
    );
}

#[test]
fn bad_requests_change_nothing() {
    let d = Dir::new("bad");
    let mut s = session(&d, 0.0);
    run(&mut s, "caption.movie", json!({"text": "keep"})).unwrap();
    let before = s.project().clone();
    std::fs::write(d.path("junk.ttf"), b"definitely not a font").unwrap();
    for (id, params) in [
        ("caption.movie", json!({"color": "red"})),
        ("caption.movie", json!({"align": "middle"})),
        ("caption.movie", json!({"size": 0})),
        ("caption.movie", json!({"size": 1e9})),
        ("caption.movie", json!({"text": "x".repeat(5000)})),
        ("caption.movie", json!({"font": d.s("missing.ttf")})),
        ("caption.movie", json!({"font": d.s("junk.ttf")})),
        ("caption.sequence", json!({"sequence": 4, "text": "x"})),
        (
            "image.caption",
            json!({"font": d.s("junk.ttf"), "text": "x"}),
        ),
    ] {
        assert!(run(&mut s, id, params.clone()).is_err(), "{id} {params}");
        assert_eq!(*s.project(), before, "{id} {params}");
    }
    let err = run(&mut s, "caption.movie", json!({"font": d.s("junk.ttf")}))
        .unwrap_err()
        .to_string();
    assert!(err.contains("junk.ttf") && err.contains("font"), "{err}");
}

#[test]
fn custom_fonts_load_and_survive_save_reopen_and_append() {
    let d = Dir::new("fonts");
    let mut s = session(&d, 0.0);
    std::fs::write(
        d.path("my.ttf"),
        inbetween_render_cpu::caption::DEFAULT_FONT,
    )
    .unwrap();
    run(
        &mut s,
        "caption.sequence",
        json!({"text": "Font", "font": d.s("my.ttf"), "y": 0.5}),
    )
    .unwrap();
    run(
        &mut s,
        "image.caption",
        json!({"text": "Font", "font": d.s("my.ttf")}),
    )
    .unwrap();
    run(
        &mut s,
        "caption.movie",
        json!({"text": "Font", "font": d.s("my.ttf")}),
    )
    .unwrap();
    let want = frame(&mut s, 1);
    assert!(ink(&want) > 20.0);

    // Saved into a sub-folder, the font paths are relative to the new file.
    std::fs::create_dir_all(d.path("saved")).unwrap();
    run(
        &mut s,
        "project.saveAs",
        json!({"path": d.s("saved/p.inbetween")}),
    )
    .unwrap();
    let text = std::fs::read_to_string(d.path("saved/p.inbetween")).unwrap();
    assert!(
        !text.contains(&d.s("my.ttf")),
        "no absolute font path in the file"
    );
    assert!(text.contains("../my.ttf"), "{text}");

    // Reopening in a fresh session draws the same frame.
    let mut fresh = Session::new();
    run(
        &mut fresh,
        "project.open",
        json!({"path": d.s("saved/p.inbetween")}),
    )
    .unwrap();
    assert_eq!(frame(&mut fresh, 1), want);

    // Appending into another project keeps the fonts working.
    std::fs::create_dir_all(d.path("other")).unwrap();
    let mut target = session(&d, 0.0);
    run(
        &mut target,
        "project.saveAs",
        json!({"path": d.s("other/q.inbetween")}),
    )
    .unwrap();
    run(
        &mut target,
        "file.appendProject",
        json!({"path": d.s("saved/p.inbetween")}),
    )
    .unwrap();
    let appended = target
        .prepare(1)
        .unwrap()
        .renderer()
        .unwrap()
        .render_frame(1)
        .unwrap();
    assert_eq!(appended, want_without_movie(&mut fresh));

    // Replacing the font file reloads it: a broken file is now an error at render time.
    std::fs::write(d.path("my.ttf"), b"broken now!").unwrap();
    assert!(fresh.prepare(0).is_err() || fresh.source(0, 1).is_err());
}

/// Frame 1 of sequence 0 without the movie-wide caption (appending drops the project's).
fn want_without_movie(s: &mut Session) -> Image {
    frame(s, 1)
}

#[test]
fn off_screen_and_tiny_frames_are_fine() {
    let d = Dir::new("edge");
    let mut s = session(&d, 0.0);
    run(
        &mut s,
        "caption.sequence",
        json!({"text": "away", "x": 9, "y": -9, "size": 24}),
    )
    .unwrap();
    assert_eq!(ink(&frame(&mut s, 0)), 0.0);
    run(
        &mut s,
        "caption.sequence",
        json!({"x": 0.5, "y": 0.5, "size": 4000, "border_size": 4000}),
    )
    .unwrap();
    let f = frame(&mut s, 0);
    assert!(f.as_slice().iter().all(|v| v.is_finite()));
}
