//! `cargo xtask wasm`: L0 to L5 must build for wasm32 (AGENTS.md rule 6).

use crate::layers::{self, LAYERS};
use std::process::Command;

const TARGET: &str = "wasm32-unknown-unknown";
const MAX_LAYER: u8 = 5;

pub fn run() -> Result<(), String> {
    let members = layers::workspace_members()?;
    let packages: Vec<&str> = members
        .iter()
        .map(|m| m.name.as_str())
        .filter(|n| {
            LAYERS
                .iter()
                .any(|(name, layer)| name == n && *layer <= MAX_LAYER)
        })
        .collect();
    if packages.is_empty() {
        println!("wasm: no L0 to L5 crates yet");
        return Ok(());
    }
    let mut cmd = Command::new("cargo");
    cmd.args(["check", "--target", TARGET]);
    for p in &packages {
        cmd.args(["-p", p]);
    }
    println!(
        "==> wasm: cargo check --target {TARGET} for {}",
        packages.join(", ")
    );
    let status = cmd
        .status()
        .map_err(|e| format!("failed to start cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "wasm build failed (is the target installed? `rustup target add {TARGET}`)"
        ))
    }
}
