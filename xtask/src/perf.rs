//! `cargo xtask perf [--quick] [--threads N]`: time a 1920x1080 frame with 1,000 dot pairs
//! in a release build, first on the CPU renderer (4 worker threads unless told otherwise),
//! then the face detector on a 1080p frame, then on the GPU renderer when an adapter exists.

use std::process::Command;

fn example(package: &str, example: &str, args: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "--quiet",
        "--release",
        "-p",
        package,
        "--example",
        example,
    ]);
    if !args.is_empty() {
        cmd.arg("--").args(args);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("failed to start cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "`{package}` missed its performance budget (or the harness failed); see above"
        ))
    }
}

pub fn run(args: &[String]) -> Result<(), String> {
    let cpu_args: Vec<&str> = args.iter().map(String::as_str).collect();
    let gpu_args: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| *a == "--quick")
        .collect();
    println!("== CPU renderer ==");
    let cpu = example("inbetween-render-cpu", "perf_cpu", &cpu_args);
    println!("\n== Face detector ==");
    let detect = example("inbetween-detect", "perf_detect", &gpu_args);
    println!("\n== GPU renderer ==");
    let gpu = example("inbetween-render-gpu", "perf_gpu", &gpu_args);
    cpu.and(detect).and(gpu)
}
