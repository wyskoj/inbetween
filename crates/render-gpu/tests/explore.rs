use inbetween_doc::{DotPair, DotSet, FitMode, MeshPolicy, MorphType, Sequence};
use inbetween_geom::Point;
use inbetween_raster::Image;
use inbetween_render_cpu::Renderer;
use inbetween_render_gpu::{GpuContext, GpuRenderer};
use inbetween_testkit::{alpha_gradient, checkerboard, demo, gradient, labelled_grid};

fn stats(a: &Image, b: &Image) -> (f32, usize, f32) {
    let mut max = 0f32;
    let mut bad = 0;
    let mut bad_max = 0f32;
    for (x, y) in a
        .as_slice()
        .chunks_exact(4)
        .zip(b.as_slice().chunks_exact(4))
    {
        let d = x
            .iter()
            .zip(y)
            .map(|(p, q)| (p - q).abs())
            .fold(0.0, f32::max);
        max = max.max(d);
        if d > 2e-3 {
            bad += 1;
            bad_max = bad_max.max(d);
        }
    }
    (max, bad, bad_max)
}

#[test]
fn explore() {
    let Ok(ctx) = GpuContext::new() else {
        println!("no gpu");
        return;
    };
    println!("adapter: {} ({})", ctx.adapter_name(), ctx.backend());
    let (w, h) = (480u32, 360u32);
    let (a, b) = demo::pair(w, h).unwrap();
    let dots: Vec<DotPair> = demo::dot_pairs(w, h)
        .into_iter()
        .map(|((x1, y1), (x2, y2))| DotPair::new(Point::new(x1, y1), Point::new(x2, y2)))
        .collect();
    let seq = Sequence {
        dots: DotSet::new(dots),
        frames: 12,
        ..Sequence::default()
    };
    let cpu = Renderer::new(&seq, (w, h), &a, Some(&b)).unwrap();
    let mut gpu = GpuRenderer::new(&ctx, &seq, (w, h), &a, Some(&b)).unwrap();
    for i in [0, 3, 6, 9, 11] {
        let c = cpu.render_frame(i).unwrap();
        let g = gpu.render_frame(i).unwrap();
        let (m, bad, bm) = stats(&c, &g);
        println!("demo frame {i}: max {m:.5}, pixels over 2e-3: {bad} (max there {bm:.4})");
    }
    let _ = (
        gradient,
        labelled_grid,
        alpha_gradient,
        checkerboard,
        FitMode::Stretch,
        MeshPolicy::Frame0,
        MorphType::Morph,
    );
}
