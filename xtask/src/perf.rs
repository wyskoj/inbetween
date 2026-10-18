//! `cargo xtask perf [--quick] [--threads N]`: time a 1920x1080 frame with 1,000 dot pairs
//! (release build, CPU renderer, 4 worker threads unless told otherwise).

use std::process::Command;

pub fn run(args: &[String]) -> Result<(), String> {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "--quiet",
        "--release",
        "-p",
        "inbetween-render-cpu",
        "--example",
        "perf",
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
        Err("performance budget missed (or the harness failed); see the numbers above".to_string())
    }
}
