//! Loads the bundled models and detects the face in one image, printing a few landmarks.
//! Run natively and as a `wasm32-wasip1` binary (see `docs/perf.md` or `models/README.md`) to
//! check that tract loads and runs the converted models the same way on both.
//!
//! Usage: `wasm_check IMAGE.png`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use inbetween_detect::{mesh_map, Detector};
use inbetween_raster::Image;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: wasm_check IMAGE.png");
    let bytes = std::fs::read(path).expect("read the image");
    let d = inbetween_codecs::decode(&bytes).expect("decode");
    let img = Image::from_rgba8(d.width, d.height, &d.data).expect("image");
    let detector = Detector::new().expect("the bundled models load");
    println!("models loaded");
    let faces = detector.faces(&img).expect("detect");
    println!("faces: {}", faces.len());
    for f in &faces {
        println!(
            "score {:.3} box {:.1},{:.1} to {:.1},{:.1}",
            f.score, f.bounds.x0, f.bounds.y0, f.bounds.x1, f.bounds.y1
        );
        for name in ["pupil_right", "pupil_left", "nose_tip", "chin"] {
            let p = mesh_map::point(name, &f.mesh).expect("mapped");
            println!("{name} {:.2},{:.2}", p.x, p.y);
        }
    }
    if faces.len() != 1 {
        std::process::exit(1);
    }
}
