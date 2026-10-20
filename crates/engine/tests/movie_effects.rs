//! Effects over the whole movie: back colour through a transparent morph, the movie trail,
//! refusals, undo, and saving.

use inbetween_doc::{
    DotPair, DotSet, ImageRef, OutputSettings, Project, ProjectType, Sequence,
};
use inbetween_engine::{CommandError, Registry, Session};
use inbetween_geom::Point;
use inbetween_raster::Image;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const W: u32 = 32;
const H: u32 = 24;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("inbetween-mfx-{tag}-{}", std::process::id()));
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

fn write_png(path: &Path, rgba: [u8; 4], block: Option<u32>) {
    let mut data = Vec::new();
    for y in 0..H {
        for x in 0..W {
            let lit = block.is_some_and(|b| x >= b && x < b + 8 && (6..14).contains(&y));
            data.extend_from_slice(&if lit { [255, 255, 255, 255] } else { rgba });
        }
    }
    std::fs::write(path, inbetween_codecs::encode_png(W, H, &data).unwrap()).unwrap();
}

fn corners() -> DotSet {
    let pts = [(0.0, 0.0), (31.0, 0.0), (0.0, 23.0), (31.0, 23.0)];
    DotSet::new(
        pts.iter()
            .map(|&(x, y)| DotPair::new(Point::new(x, y), Point::new(x, y)))
            .collect(),
    )
}

fn project(dir: &Dir, kind: ProjectType, a: ([u8; 4], Option<u32>), b: ([u8; 4], Option<u32>)) -> Session {
    write_png(&dir.path("a.png"), a.0, a.1);
    write_png(&dir.path("b.png"), b.0, b.1);
    let mut s = Session::new();
    s.set_project(
        Project {
            kind,
            output: OutputSettings {
                width: W,
                height: H,
            },
            sequences: vec![Sequence {
                image1: ImageRef::new(dir.s("a.png")),
                image2: ImageRef::new(dir.s("b.png")),
                dots: corners(),
                frames: 5,
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

fn read_png(path: &Path) -> Vec<u8> {
    inbetween_codecs::read_file(path).unwrap().data
}

fn px(img: &Image, x: u32, y: u32) -> [f32; 4] {
    img.get(x, y).unwrap()
}

#[test]
fn a_movie_back_colour_shows_through_a_transparent_morph_in_both_project_kinds() {
    for kind in [ProjectType::Sequence, ProjectType::Layer] {
        let d = Dir::new(&format!("back-{kind:?}"));
        let mut s = project(&d, kind, ([0, 0, 0, 0], None), ([0, 0, 0, 0], None));
        assert_eq!(px(&s.render_movie_frame(2).unwrap(), 3, 3)[3], 0.0);
        let report = run(
            &mut s,
            "effect.backColor",
            json!({"scope": "movie", "color": "#0000ff"}),
        )
        .unwrap();
        assert_eq!(report["scope"], "movie");
        assert_eq!(report["sequence"], Value::Null);
        for n in 1..=5 {
            assert_eq!(
                px(&s.render_movie_frame(n).unwrap(), 3, 3),
                [0.0, 0.0, 1.0, 1.0],
                "{kind:?} frame {n}"
            );
        }
        // The sequence's own effects are untouched.
        assert!(!s.project().sequences[0].effects.back_color.enabled);
    }
}

#[test]
fn the_movie_trail_is_the_same_in_an_export_and_in_a_single_frame() {
    let d = Dir::new("trail");
    // A white block that is on the left in Image 1 and on the right in Image 2 dissolves in
    // brightness, so a blur of earlier frames changes the picture.
    let mut s = project(
        &d,
        ProjectType::Sequence,
        ([20, 20, 20, 255], Some(2)),
        ([20, 20, 20, 255], Some(20)),
    );
    run(&mut s, "export.movie", json!({"out_dir": d.s("plain")})).unwrap();
    let plain3 = read_png(&d.path("plain/frame_0003.png"));
    run(
        &mut s,
        "effect.trail",
        json!({"scope": "movie", "overlay": true, "intensity": 100}),
    )
    .unwrap();
    run(&mut s, "export.movie", json!({"out_dir": d.s("trail")})).unwrap();
    let trail3 = read_png(&d.path("trail/frame_0003.png"));
    assert_ne!(plain3, trail3, "the trail changed nothing");
    // Frame 1 has nothing before it.
    assert_eq!(
        read_png(&d.path("plain/frame_0001.png")),
        read_png(&d.path("trail/frame_0001.png"))
    );
    // One frame on its own looks the same as inside the export.
    assert_eq!(s.render_movie_frame(3).unwrap().to_rgba8(), trail3);
    assert_eq!(s.render_movie_frame(5).unwrap().to_rgba8(), read_png(&d.path("trail/frame_0005.png")));
}

#[test]
fn a_transition_cannot_be_a_movie_effect() {
    let d = Dir::new("transition");
    let mut s = project(&d, ProjectType::Sequence, ([9, 9, 9, 255], None), ([9, 9, 9, 255], None));
    let err = run(
        &mut s,
        "effect.transition",
        json!({"scope": "movie", "image": d.s("a.png")}),
    );
    assert!(matches!(err, Err(CommandError::BadParams(_))), "{err:?}");
    assert!(!s.project().effects.transition.enabled);
}

#[test]
fn movie_effects_undo_and_redo() {
    let d = Dir::new("undo");
    let mut s = project(&d, ProjectType::Sequence, ([9, 9, 9, 255], None), ([9, 9, 9, 255], None));
    run(&mut s, "effect.radial", json!({"scope": "movie", "intensity": 60})).unwrap();
    assert!(s.project().effects.radial.enabled);
    run(&mut s, "edit.undo", json!({})).unwrap();
    assert!(!s.project().effects.radial.enabled);
    run(&mut s, "edit.redo", json!({})).unwrap();
    assert!(s.project().effects.radial.enabled);
}

#[test]
fn movie_effect_images_are_saved_and_rebased() {
    let d = Dir::new("save");
    let mut s = project(&d, ProjectType::Sequence, ([9, 9, 9, 255], None), ([9, 9, 9, 255], None));
    write_png(&d.path("fg.png"), [255, 0, 0, 128], None);
    run(
        &mut s,
        "effect.foreground",
        json!({"scope": "movie", "image": d.s("fg.png"), "opacity": 70}),
    )
    .unwrap();
    let before = s.render_movie_frame(2).unwrap();
    run(&mut s, "project.save", json!({"path": d.s("p.inbetween")})).unwrap();
    let mut again = Session::new();
    run(&mut again, "project.open", json!({"path": d.s("p.inbetween")})).unwrap();
    assert!(again.project().effects.foreground.enabled);
    assert_eq!(again.render_movie_frame(2).unwrap(), before);
    // Saved into another folder, the effect image is rebased and still found.
    let sub = d.path("moved");
    std::fs::create_dir_all(&sub).unwrap();
    let moved = sub.join("q.inbetween");
    run(&mut s, "project.save", json!({"path": moved.display().to_string()})).unwrap();
    let mut third = Session::new();
    run(&mut third, "project.open", json!({"path": moved.display().to_string()})).unwrap();
    assert_eq!(third.render_movie_frame(2).unwrap(), before);
}

#[test]
fn a_movie_effect_without_an_image_is_refused() {
    let d = Dir::new("noimg");
    let mut s = project(&d, ProjectType::Sequence, ([9, 9, 9, 255], None), ([9, 9, 9, 255], None));
    let err = run(&mut s, "effect.mask", json!({"scope": "movie", "enabled": true}));
    assert!(matches!(err, Err(CommandError::BadParams(_))), "{err:?}");
}
