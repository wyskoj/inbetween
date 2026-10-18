//! `cargo xtask ci`: the same gates GitHub Actions runs.

use std::process::Command;

pub fn run() -> Result<(), String> {
    step("fmt", &["fmt", "--all", "--check"])?;
    step(
        "clippy",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    step("test", &["test", "--workspace"])?;
    step("layers", &["xtask", "layers"])?;
    step("wasm", &["xtask", "wasm"])?;
    deny()
}

fn step(name: &str, args: &[&str]) -> Result<(), String> {
    println!("==> {name}: cargo {}", args.join(" "));
    let status = Command::new("cargo")
        .args(args)
        .status()
        .map_err(|e| format!("failed to start cargo for `{name}`: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("step `{name}` failed"))
    }
}

/// `cargo deny` is an optional local install; CI always has it.
fn deny() -> Result<(), String> {
    let installed = Command::new("cargo")
        .args(["deny", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if installed {
        step("deny", &["deny", "check"])
    } else if std::env::var_os("CI").is_some() {
        Err("cargo-deny is not installed".to_string())
    } else {
        println!("==> deny: skipped (install with `cargo install cargo-deny`)");
        Ok(())
    }
}
