//! Times every pixel operation on a 1920x1080 image. Run with:
//! `RAYON_NUM_THREADS=4 cargo run --release -p inbetween-raster --example filters`

use inbetween_raster::{filter, transform, Adjust, Image};
use std::time::Instant;

fn time(name: &str, f: impl Fn() -> Image) {
    let _ = f();
    let runs = 5;
    let start = Instant::now();
    for _ in 0..runs {
        let _ = f();
    }
    println!(
        "{name:<14} {:>8.1} ms",
        start.elapsed().as_secs_f64() * 1000.0 / f64::from(runs)
    );
}

fn main() {
    let Ok(mut img) = Image::new(1920, 1080) else {
        return;
    };
    for y in 0..1080u32 {
        for x in 0..1920u32 {
            let v = ((x ^ y) & 255) as f32 / 255.0;
            img.set(x, y, [v, 1.0 - v, v * 0.5, 1.0]);
        }
    }
    let ok = |r: Result<Image, inbetween_raster::RasterError>| r.unwrap_or_else(|_| img.clone());
    time("rotate90", || ok(transform::rotate_quarter(&img, 1)));
    time("flip", || ok(transform::flip(&img, true, false)));
    time("resample", || {
        ok(transform::resample(
            &img,
            1920,
            1080,
            [0.98, 0.17, 10.0, -0.17, 0.98, 30.0],
        ))
    });
    time("adjust", || {
        ok(inbetween_raster::adjust(
            &img,
            &Adjust {
                brightness: 20.0,
                contrast: 10.0,
                hue: 30.0,
                saturation: 10.0,
                ..Adjust::default()
            },
        ))
    });
    time("smooth", || ok(filter::smooth(&img, 1.0)));
    time("box r=10", || ok(filter::box_blur(&img, 10)));
    time("gaussian s=10", || ok(filter::gaussian(&img, 10.0)));
    time("zoom blur", || ok(filter::zoom_blur(&img, 0.5, 0.5, 0.3)));
    time("radial blur", || {
        ok(filter::radial_blur(&img, 0.5, 0.5, 20.0))
    });
    time("sharpen", || ok(filter::sharpen(&img, 1.0, 2.0)));
    time("emboss", || ok(filter::emboss(&img, 45.0, 1.0, 0.5)));
    time("mosaic 16", || ok(filter::mosaic(&img, 16)));
    time("oil r=4", || ok(filter::oil_paint(&img, 4, 64)));
    time("splash 8", || ok(filter::splash(&img, 8, 1)));
    time("cylinder", || ok(filter::cylinder(&img, false, 0.8)));
    time("whirl", || ok(filter::whirl(&img, 0.5, 0.5, 500.0, 90.0)));
    time("pinch", || ok(filter::pinch(&img, 0.5, 0.5, 500.0, 0.5)));
}
