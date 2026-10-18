//! Timing harness behind `cargo xtask perf` for the GPU renderer: the same 1920x1080 scene
//! with 1,000 dot pairs that the CPU harness uses.
//!
//! Run in release mode: `cargo run --release -p inbetween-render-gpu --example perf_gpu`.
//! Flag: `--quick` (fewer iterations). `INBETWEEN_GPU_BUDGET_MS` overrides the budget.
//! With no GPU the harness says so and succeeds.

use inbetween_doc::{DotPair, DotSet, Sequence};
use inbetween_geom::Point;
use inbetween_render_cpu::Renderer;
use inbetween_render_gpu::{GpuContext, GpuRenderer};
use inbetween_testkit::{gradient, labelled_grid, perf};
use std::process::ExitCode;
use std::time::Instant;

const W: u32 = perf::WIDTH;
const H: u32 = perf::HEIGHT;
/// Real-time preview: one frame inside a 60 fps frame time.
const DEFAULT_BUDGET_MS: f64 = 16.7;

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

fn median(times: &mut [f64]) -> f64 {
    times.sort_by(f64::total_cmp);
    times.get(times.len() / 2).copied().unwrap_or(0.0)
}

fn main() -> ExitCode {
    let quick = std::env::args().any(|a| a == "--quick");
    let budget: f64 = std::env::var("INBETWEEN_GPU_BUDGET_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_BUDGET_MS);
    let iterations = if quick { 10 } else { 60 };

    let ctx = match GpuContext::new() {
        Ok(ctx) => ctx,
        Err(e) => {
            println!("gpu:        skipped ({e})");
            return ExitCode::SUCCESS;
        }
    };
    let (a, b) = match (gradient(W, H), labelled_grid(W, H, 60)) {
        (Ok(a), Ok(b)) => (a, b),
        _ => {
            eprintln!("could not build the test images");
            return ExitCode::FAILURE;
        }
    };
    let dots = DotSet::new(
        perf::dot_pairs()
            .into_iter()
            .map(|((x1, y1), (x2, y2))| DotPair::new(Point::new(x1, y1), Point::new(x2, y2)))
            .collect(),
    );
    let seq = Sequence {
        dots,
        frames: 30,
        ..Sequence::default()
    };

    let start = Instant::now();
    let mut gpu = match GpuRenderer::new(&ctx, &seq, (W, H), &a, Some(&b)) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("gpu renderer failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    gpu.wait();
    let setup = ms(start);

    // Warm up (shader compilation, caches), then time frames spread across the sequence.
    for i in 0..3 {
        if gpu.draw(f64::from(i) / 3.0).is_err() {
            eprintln!("gpu draw failed");
            return ExitCode::FAILURE;
        }
        gpu.wait();
    }
    let frame_t = |i: u32| f64::from(i % 30) / 29.0;

    // Draw only: what a live preview pays per frame (plan, upload, draw, wait).
    let mut draw_times = Vec::new();
    for i in 0..iterations {
        let start = Instant::now();
        if gpu.draw(frame_t(i)).is_err() {
            eprintln!("gpu draw failed");
            return ExitCode::FAILURE;
        }
        gpu.wait();
        draw_times.push(ms(start));
    }
    // Draw plus copying the frame back into an `Image`: what an export pays.
    let mut readback_times = Vec::new();
    for i in 0..iterations.min(21) {
        let start = Instant::now();
        if gpu.render_t(frame_t(i)).is_err() {
            eprintln!("gpu render failed");
            return ExitCode::FAILURE;
        }
        readback_times.push(ms(start));
    }
    let (draw_min, draw_max) = (
        draw_times.iter().copied().fold(f64::INFINITY, f64::min),
        draw_times.iter().copied().fold(0.0, f64::max),
    );
    let draw_median = median(&mut draw_times);
    let readback_median = median(&mut readback_times);

    // One frame against the CPU oracle, so the numbers come with a correctness check.
    let (worst, visible, mean) = match (
        Renderer::new(&seq, (W, H), &a, Some(&b)).and_then(|r| r.render_frame(15)),
        gpu.render_frame(15),
    ) {
        (Ok(cpu), Ok(g)) => {
            let (mut worst, mut visible, mut sum) = (0.0_f32, 0usize, 0.0_f64);
            for (x, y) in cpu
                .as_slice()
                .chunks_exact(4)
                .zip(g.as_slice().chunks_exact(4))
            {
                let d = x
                    .iter()
                    .zip(y)
                    .map(|(p, q)| (p - q).abs())
                    .fold(0.0_f32, f32::max);
                worst = worst.max(d);
                visible += usize::from(d > 0.01);
                sum += x
                    .iter()
                    .zip(y)
                    .map(|(p, q)| f64::from((p - q).abs()))
                    .sum::<f64>();
            }
            (worst, visible, sum / (f64::from(W) * f64::from(H) * 4.0))
        }
        _ => {
            eprintln!("could not compare against the CPU frame");
            return ExitCode::FAILURE;
        }
    };
    println!("gpu:        {} ({})", ctx.adapter_name(), ctx.backend());
    println!("scene:      {W}x{H}, {} dot pairs", seq.dots.len());
    println!("setup:      {setup:.1} ms (pipeline, two source uploads, targets)");
    println!(
        "draw:       median {draw_median:.2} ms, min {draw_min:.2}, max {draw_max:.2} over {iterations} frames"
    );
    println!("with readback: median {readback_median:.1} ms (copy to a CPU image)");
    println!(
        "vs CPU:     frame 15 max difference {worst:.5}, mean {mean:.7}, {visible} pixels over 0.01 ({:.4}%)",
        visible as f64 / (f64::from(W) * f64::from(H)) * 100.0
    );
    println!("budget:     {budget:.1} ms per frame (60 fps)");
    if draw_median <= budget {
        println!("result:     PASS");
        ExitCode::SUCCESS
    } else {
        println!(
            "result:     FAIL ({:.1}x over budget)",
            draw_median / budget
        );
        ExitCode::FAILURE
    }
}
