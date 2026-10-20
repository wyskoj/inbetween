//! `cargo xtask ci`: the same gates GitHub Actions runs.
//!
//! `cargo xtask ci --quick` runs the gates only for the crates with uncommitted or untracked
//! changes (plus the layer check, and the wasm and deny checks when they could be affected),
//! and shortens the mutation-fuzz loops. It is for the edit-test loop; run the full form
//! before you push, because quick mode does not test crates that merely depend on yours.

use std::collections::BTreeSet;
use std::process::Command;

pub fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        None => full(),
        Some("--quick") => quick(),
        Some(other) => Err(format!("unknown ci option `{other}` (use --quick or none)")),
    }
}

fn full() -> Result<(), String> {
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
    // The slim build: no bundled face detector, so `face.locate` falls back to a box.
    step(
        "slim clippy",
        &[
            "clippy",
            "-p",
            "inbetween-detect",
            "-p",
            "inbetween-engine",
            "--no-default-features",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    step(
        "slim test",
        &[
            "test",
            "-p",
            "inbetween-detect",
            "-p",
            "inbetween-engine",
            "--no-default-features",
        ],
    )?;
    step("layers", &["xtask", "layers"])?;
    step("wasm", &["xtask", "wasm"])?;
    deny()
}

/// What changed, as paths relative to the repository root (tracked changes and new files).
fn changed_paths() -> Result<Vec<String>, String> {
    let out = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .output()
        .map_err(|e| format!("failed to start git: {e}"))?;
    if !out.status.success() {
        return Err("`git status` failed".to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.get(3..))
        // A rename shows as `old -> new`; the new path is what exists now.
        .map(|p| {
            p.rsplit(" -> ")
                .next()
                .unwrap_or(p)
                .trim_matches('"')
                .to_string()
        })
        .collect())
}

/// The package a changed path belongs to, `None` for files that belong to no crate.
fn package_of(path: &str) -> Option<String> {
    let mut parts = path.split('/');
    match (parts.next()?, parts.next()?) {
        ("crates", dir) => Some(format!("inbetween-{dir}")),
        ("apps", dir) => Some(dir.to_string()),
        _ => None,
    }
}

/// Crates that never reach wasm (the shell and the binaries).
fn is_native_only(package: &str) -> bool {
    matches!(package, "inbetween-ui-egui" | "inbetween" | "inbetween-cli")
}

fn quick() -> Result<(), String> {
    let paths = changed_paths()?;
    let touches_build = paths
        .iter()
        .any(|p| p.starts_with("xtask/") || p == "Cargo.toml" || p.starts_with("rust-toolchain"));
    if touches_build {
        println!("==> the workspace build files changed: running the full gates");
        return full();
    }
    let packages: BTreeSet<String> = paths.iter().filter_map(|p| package_of(p)).collect();
    let docs_only = packages.is_empty() && !paths.iter().any(|p| p.ends_with(".rs"));
    if packages.is_empty() {
        println!(
            "==> no crate changed{}",
            if docs_only { " (docs only)" } else { "" }
        );
        return step("fmt", &["fmt", "--all", "--check"]);
    }
    println!(
        "==> quick mode, crates: {}",
        packages.iter().cloned().collect::<Vec<_>>().join(", ")
    );
    step("fmt", &["fmt", "--all", "--check"])?;
    let mut clippy: Vec<&str> = vec!["clippy"];
    let mut test: Vec<&str> = vec!["test"];
    for p in &packages {
        clippy.extend(["-p", p]);
        test.extend(["-p", p]);
    }
    clippy.extend(["--all-targets", "--", "-D", "warnings"]);
    step("clippy", &clippy)?;
    // Fuzz loops at a tenth of their length; CI and the full gate run them in full.
    step_env("test", &test, &[("INBETWEEN_FUZZ_SCALE", "10")])?;
    step("layers", &["xtask", "layers"])?;
    if packages.iter().any(|p| !is_native_only(p)) {
        step("wasm", &["xtask", "wasm"])?;
    }
    if paths
        .iter()
        .any(|p| p.ends_with("Cargo.toml") || p == "Cargo.lock" || p == "deny.toml")
    {
        deny()?;
    }
    Ok(())
}

fn step(name: &str, args: &[&str]) -> Result<(), String> {
    step_env(name, args, &[])
}

fn step_env(name: &str, args: &[&str], env: &[(&str, &str)]) -> Result<(), String> {
    println!("==> {name}: cargo {}", args.join(" "));
    let status = Command::new("cargo")
        .args(args)
        .envs(env.iter().copied())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_map_to_packages() {
        assert_eq!(
            package_of("crates/raster/src/lib.rs").as_deref(),
            Some("inbetween-raster")
        );
        assert_eq!(
            package_of("apps/inbetween-cli/tests/x.rs").as_deref(),
            Some("inbetween-cli")
        );
        assert_eq!(
            package_of("apps/inbetween/src/main.rs").as_deref(),
            Some("inbetween")
        );
        assert_eq!(package_of("docs/roadmap.md"), None);
        assert_eq!(package_of("AGENTS.md"), None);
    }

    #[test]
    fn only_the_shell_and_binaries_skip_wasm() {
        assert!(is_native_only("inbetween-ui-egui") && is_native_only("inbetween"));
        assert!(!is_native_only("inbetween-engine") && !is_native_only("inbetween-raster"));
    }
}
