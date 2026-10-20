//! `cargo xtask perf` harness for the face detector: a 1920x1080 frame holding a portrait,
//! mean of several runs after a warm-up. Fails when the mean passes the 30 ms budget.
//!
//! Usage: `cargo run --release -p inbetween-detect --example perf_detect [-- --quick]`
#![allow(clippy::unwrap_used, clippy::expect_used)]

use inbetween_detect::Detector;
use inbetween_raster::Image;
use std::time::Instant;

const BUDGET_MS: f64 = 30.0;

fn main() {
    let quick = std::env::args().any(|a| a == "--quick");
    let path = format!("{}/tests/data/portrait_a.png", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(path).expect("portrait fixture");
    let d = inbetween_codecs::decode(&bytes).expect("decodes");
    let p = Image::from_rgba8(d.width, d.height, &d.data).expect("image");
    let mut frame = Image::new(1920, 1080).expect("image");
    for y in 0..1080u32 {
        for x in 0..1920u32 {
            let sx = (x as f32 - 700.0) * p.width() as f32 / 864.0;
            let sy = y as f32 * p.height() as f32 / 1080.0;
            if sx >= 0.0 && (sx as u32) < p.width() {
                frame.set(x, y, p.sample_bilinear(sx, sy));
            }
        }
    }

    let t = Instant::now();
    let detector = Detector::new().expect("models load");
    println!(
        "load both models: {:.1} ms",
        t.elapsed().as_secs_f64() * 1e3
    );
    let _ = detector.faces(&frame).expect("warm-up");
    let runs: u32 = if quick { 5 } else { 20 };
    let t = Instant::now();
    let mut found = 0;
    for _ in 0..runs {
        found = detector.faces(&frame).expect("detects").len();
    }
    let mean = t.elapsed().as_secs_f64() * 1e3 / f64::from(runs);
    println!("1080p detect: {mean:.1} ms per image over {runs} runs ({found} face), budget {BUDGET_MS} ms");
    if found != 1 || mean > BUDGET_MS {
        eprintln!("detect perf failed: expected one face and a mean under {BUDGET_MS} ms");
        std::process::exit(1);
    }
}
