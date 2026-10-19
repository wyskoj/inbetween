//! Edited sources: caching, identity fast path and the morph invariants with edits.

use inbetween_doc::{
    AdjustSettings, Crop, DotPair, DotSet, Filter, ImageEdit, ImageRef, OutputSettings, Project,
    Sequence,
};
use inbetween_engine::Session;
use inbetween_geom::Point;
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("inbetween-src-{tag}-{}", std::process::id()));
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

/// A 64 x 48 picture with a distinct value at every pixel.
fn write_png(path: &Path, seed: u32) {
    let mut rgba = Vec::new();
    for y in 0..48u32 {
        for x in 0..64u32 {
            rgba.extend_from_slice(&[
                ((x * 4 + seed) % 256) as u8,
                ((y * 5 + seed * 3) % 256) as u8,
                ((x * y + seed) % 256) as u8,
                255,
            ]);
        }
    }
    std::fs::write(path, inbetween_codecs::encode_png(64, 48, &rgba).unwrap()).unwrap();
}

fn session(dir: &Dir) -> Session {
    write_png(&dir.0.join("a.png"), 10);
    write_png(&dir.0.join("b.png"), 90);
    let mut s = Session::new();
    s.set_project(
        Project {
            output: OutputSettings {
                width: 64,
                height: 48,
            },
            sequences: vec![Sequence {
                image1: ImageRef::new(dir.s("a.png")),
                image2: ImageRef::new(dir.s("b.png")),
                dots: DotSet::new(vec![
                    DotPair::fixed(Point::new(0.0, 0.0)),
                    DotPair::fixed(Point::new(63.0, 0.0)),
                    DotPair::fixed(Point::new(0.0, 47.0)),
                    DotPair::fixed(Point::new(63.0, 47.0)),
                ]),
                frames: 5,
                ..Sequence::default()
            }],
            ..Project::default()
        },
        None,
    );
    s
}

fn edit() -> ImageEdit {
    ImageEdit {
        flip_h: true,
        adjust: AdjustSettings {
            brightness: 20.0,
            ..AdjustSettings::default()
        },
        filters: vec![Filter::Gaussian { sigma: 1.0 }],
        ..ImageEdit::default()
    }
}

#[test]
fn an_identity_edit_keeps_the_raw_arc() {
    let d = Dir::new("ident");
    let mut s = session(&d);
    let raw = s.load_image(&d.s("a.png"), 1, 0).unwrap();
    let got = s.source(0, 1).unwrap();
    assert!(Arc::ptr_eq(&raw, &got));
    assert_eq!(s.cached_sources(), 0);
}

#[test]
fn an_edited_source_is_cached_and_follows_the_edit_and_the_file() {
    let d = Dir::new("cache");
    let mut s = session(&d);
    s.sequence_mut(0).unwrap().edit1 = edit();
    let a = s.source(0, 1).unwrap();
    let again = s.source(0, 1).unwrap();
    assert!(Arc::ptr_eq(&a, &again));
    assert_eq!(s.cached_sources(), 1);
    let raw = s.load_image(&d.s("a.png"), 1, 0).unwrap();
    assert_ne!(*a, *raw);

    // A different edit makes a different entry; going back reuses the first.
    s.sequence_mut(0).unwrap().edit1.filters.clear();
    let b = s.source(0, 1).unwrap();
    assert!(!Arc::ptr_eq(&a, &b));
    s.sequence_mut(0).unwrap().edit1 = edit();
    assert!(Arc::ptr_eq(&a, &s.source(0, 1).unwrap()));

    // Changing the file reprocesses it (the stamp includes the length, so change the size).
    let mut rgba = [9u8, 9, 9, 255].repeat(70 * 48);
    rgba[0] = 1;
    std::fs::write(
        d.0.join("a.png"),
        inbetween_codecs::encode_png(70, 48, &rgba).unwrap(),
    )
    .unwrap();
    let c = s.source(0, 1).unwrap();
    assert!(!Arc::ptr_eq(&a, &c));
    assert_eq!(c.size(), (70, 48));
}

#[test]
fn the_cache_is_bounded() {
    let d = Dir::new("bound");
    let mut s = session(&d);
    for i in 0..40 {
        let mut e = edit();
        e.adjust.contrast = f64::from(i);
        s.sequence_mut(0).unwrap().edit1 = e;
        s.source(0, 1).unwrap();
    }
    assert!(s.cached_sources() <= 32);
}

#[test]
fn the_morph_invariants_hold_with_edits() {
    let d = Dir::new("morph");
    let mut s = session(&d);
    s.sequence_mut(0).unwrap().edit1 = edit();
    s.sequence_mut(0).unwrap().edit2 = ImageEdit {
        crop: Some(Crop {
            x: 0.0,
            y: 0.0,
            w: 64.0,
            h: 48.0,
        }),
        flip_v: true,
        ..ImageEdit::default()
    };
    let one = s.source(0, 1).unwrap();
    let two = s.source(0, 2).unwrap();
    let prepared = s.prepare(0).unwrap();
    let renderer = prepared.renderer().unwrap();
    let first = renderer.render_t(0.0).unwrap();
    let last = renderer.render_t(1.0).unwrap();
    let max_diff = |a: &inbetween_raster::Image, b: &inbetween_raster::Image| {
        a.as_slice()
            .iter()
            .zip(b.as_slice())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0f32, f32::max)
    };
    assert!(
        max_diff(&first, &one) < 1e-4,
        "t=0 is the processed Image 1"
    );
    assert!(max_diff(&last, &two) < 1e-4, "t=1 is the processed Image 2");
}

#[test]
fn a_bad_edit_is_an_actionable_error() {
    let d = Dir::new("bad");
    let mut s = session(&d);
    s.sequence_mut(0).unwrap().edit1.angle = f64::NAN;
    let err = s.source(0, 1).unwrap_err().to_string();
    assert!(err.contains("Image 1") && err.contains("angle"), "{err}");
}
