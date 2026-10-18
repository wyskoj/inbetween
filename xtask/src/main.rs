//! Repository automation: `cargo xtask <layers|ci>`.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod ci;
mod layers;
mod perf;
mod wasm;

use std::process::ExitCode;

const USAGE: &str =
    "usage: cargo xtask <task>\n  layers  check crate layering\n  wasm    check L0 to L5 build for wasm32\n  perf    time a 1080p frame with 1,000 dot pairs (--quick, --threads N)
  ci      run every CI gate locally";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("layers") => layers::run(),
        Some("wasm") => wasm::run(),
        Some("perf") => perf::run(args.get(1..).unwrap_or_default()),
        Some("ci") => ci::run(),
        Some(other) => Err(format!("unknown task `{other}`\n{USAGE}")),
        None => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}
