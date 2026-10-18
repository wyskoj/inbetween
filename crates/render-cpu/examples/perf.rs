//! Timing harness behind `cargo xtask perf`: one 1920x1080 frame, 1,000 dot pairs, CPU.
//!
//! Run in release mode: `cargo run --release -p inbetween-render-cpu --example perf`.
//! Flags: `--quick` (fewer iterations), `--threads N` (default 4, the budget's reference
//! machine). `INBETWEEN_PERF_BUDGET_MS` overrides the budget.

use inbetween_doc::{DotPair, DotSet, Sequence};
use inbetween_geom::Point;
use inbetween_morph::Planner;
use inbetween_render_cpu::Renderer;
use inbetween_testkit::{gradient, labelled_grid};
use std::process::ExitCode;
use std::time::{Duration, Instant};

const W: u32 = 1920;
const H: u32 = 1080;
const COLS: u32 = 40;
const ROWS: u32 = 25; // 40 x 25 = 1,000 dots
const DEFAULT_BUDGET_MS: f64 = 100.0;

/// Deterministic jitter so every run times exactly the same scene.
struct Rng(u64);

impl Rng {
    fn unit(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Jittered grid of dots on Image 1, each nudged smoothly on Image 2 (no folds).
fn dots() -> DotSet {
    let (cw, ch) = (
        f64::from(W) / f64::from(COLS),
        f64::from(H) / f64::from(ROWS),
    );
    let mut rng = Rng(0x5EED_1234_ABCD_9876);
    let mut pairs = Vec::new();
    for row in 0..ROWS {
        for col in 0..COLS {
            let x = (f64::from(col) + 0.5 + (rng.unit() - 0.5) * 0.5) * cw;
            let y = (f64::from(row) + 0.5 + (rng.unit() - 0.5) * 0.5) * ch;
            let dx = (y / 140.0).sin() * 0.3 * cw;
            let dy = (x / 170.0).cos() * 0.3 * ch;
            pairs.push(DotPair::new(Point::new(x, y), Point::new(x + dx, y + dy)));
        }
    }
    DotSet::new(pairs)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn main() -> ExitCode {
    let quick = std::env::args().any(|a| a == "--quick");
    let budget: f64 = std::env::var("INBETWEEN_PERF_BUDGET_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_BUDGET_MS);
    let iterations = if quick { 5 } else { 21 };
    let args: Vec<String> = std::env::args().collect();
    let threads: usize = args
        .iter()
        .position(|a| a == "--threads")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(4)
        .max(1);
    // The budget is stated for a 4-core machine, so measure with 4 workers by default.
    if rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .is_err()
    {
        eprintln!("could not set the thread count");
        return ExitCode::FAILURE;
    }
    let machine = std::thread::available_parallelism().map_or(1, usize::from);

    let (a, b) = match (gradient(W, H), labelled_grid(W, H, 60)) {
        (Ok(a), Ok(b)) => (a, b),
        _ => {
            eprintln!("could not build the test images");
            return ExitCode::FAILURE;
        }
    };
    let seq = Sequence {
        dots: dots(),
        frames: 30,
        ..Sequence::default()
    };

    let start = Instant::now();
    let planner = match Planner::new(&seq, W, H) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("planning failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let triangulate = start.elapsed();
    let start = Instant::now();
    let tris = planner.plan(0.5).map(|p| p.tris.len()).unwrap_or(0);
    let plan = start.elapsed();

    let renderer = match Renderer::new(&seq, (W, H), &a, Some(&b)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("renderer failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    // Warm up caches and the thread pool, then time frames spread across the sequence.
    // The checksum of one frame proves an optimisation did not change a single pixel.
    let checksum = match renderer.render_frame(15) {
        Ok(img) => img
            .to_rgba8()
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |h, &b| {
                (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
            }),
        Err(e) => {
            eprintln!("render failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut times: Vec<f64> = Vec::new();
    for i in 0..iterations {
        let frame = (i * 29 / (iterations - 1).max(1)) as u32;
        let start = Instant::now();
        let ok = renderer.render_frame(frame).is_ok();
        let took = ms(start.elapsed());
        if !ok {
            eprintln!("render failed on frame {frame}");
            return ExitCode::FAILURE;
        }
        times.push(took);
    }
    times.sort_by(f64::total_cmp);
    let median = times[times.len() / 2];

    println!(
        "scene:      {W}x{H}, {} dot pairs, {tris} triangles, {threads} threads (machine has {machine})",
        seq.dots.len()
    );
    println!(
        "setup:      {:.1} ms (two Delaunay triangulations)",
        ms(triangulate)
    );
    println!("plan:       {:.2} ms per frame", ms(plan));
    println!(
        "render:     median {median:.1} ms, min {:.1}, max {:.1} over {iterations} frames",
        times[0],
        times[times.len() - 1]
    );
    println!("checksum:   {checksum:016x} (frame 15)");
    println!("budget:     {budget:.0} ms per frame at 4 threads");
    if median <= budget {
        println!("result:     PASS");
        ExitCode::SUCCESS
    } else {
        println!("result:     FAIL ({:.1}x over budget)", median / budget);
        ExitCode::FAILURE
    }
}
