//! Timing of the movie pass at 1080p. Run by hand:
//! `cargo test -p inbetween-render-cpu --release --test movie_pass_perf -- --ignored --nocapture`

use inbetween_doc::{Filter, Project};
use inbetween_raster::Image;
use inbetween_render_cpu::movie_pass::MoviePass;
use inbetween_render_cpu::EffectImages;
use std::time::Instant;

#[test]
#[ignore = "timing, run by hand in release mode"]
fn movie_pass_at_1080p() {
    let frame = Image::filled(1920, 1080, [0.4, 0.5, 0.6, 1.0]).unwrap();
    let mut back = Project::default();
    back.effects.back_color.enabled = true;
    back.effects.radial.enabled = true;
    let adjust = Project {
        adjust: inbetween_doc::AdjustSettings {
            brightness: 20.0,
            hue: 30.0,
            ..Default::default()
        },
        ..Project::default()
    };
    let blur = Project {
        filters: vec![Filter::Gaussian { sigma: 3.0 }],
        ..Project::default()
    };
    for (name, project) in [
        ("effects", back),
        ("adjust", adjust),
        ("gaussian filter", blur),
    ] {
        let pass = MoviePass::new(&project, EffectImages::default());
        let start = Instant::now();
        let runs = 5;
        for _ in 0..runs {
            pass.apply(frame.clone(), 0.5).unwrap();
        }
        println!("movie pass, {name}: {:?} per frame", start.elapsed() / runs);
    }
}
