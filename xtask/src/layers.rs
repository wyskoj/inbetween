//! `cargo xtask layers`: enforce the crate layering from `docs/architecture.md`.
//!
//! A crate may depend only on workspace crates in a strictly lower layer. Nothing below
//! L6 may use a UI or windowing crate. Register every new crate in [`LAYERS`].

use serde::Deserialize;
use std::process::Command;

/// Workspace crates and their layer. Crates appear as their milestone needs them.
pub const LAYERS: &[(&str, u8)] = &[
    ("inbetween-geom", 0),
    ("inbetween-raster", 0),
    ("inbetween-codecs", 0),
    ("inbetween-doc", 1),
    ("inbetween-morph", 2),
    ("inbetween-render-cpu", 3),
    ("inbetween-render-gpu", 3),
    ("inbetween-format", 3),
    ("inbetween-export", 4),
    ("inbetween-engine", 5),
    ("inbetween-ui-egui", 6),
    ("inbetween-mcp", 6),
    // Applications sit on top of everything.
    ("inbetween", 7),
    ("inbetween-cli", 7),
];

/// Workspace crates outside the layer stack (tooling and test helpers).
pub const EXEMPT: &[&str] = &["xtask", "inbetween-testkit"];

/// External crates that only L6 and above may use.
pub const UI_CRATES: &[&str] = &["egui", "eframe", "winit", "rfd"];

const UI_LAYER: u8 = 6;

#[derive(Debug, Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Package {
    id: String,
    name: String,
    dependencies: Vec<Dependency>,
}

#[derive(Debug, Deserialize)]
struct Dependency {
    name: String,
}

/// A workspace package reduced to what the layering check needs.
pub struct Member {
    pub name: String,
    pub deps: Vec<String>,
}

pub fn run() -> Result<(), String> {
    let out = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|e| format!("failed to run cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let members = parse_members(&out.stdout)?;
    let problems = check(&members);
    if problems.is_empty() {
        println!("layers: ok ({} crates checked)", members.len());
        Ok(())
    } else {
        Err(format!("layering violations:\n  {}", problems.join("\n  ")))
    }
}

fn parse_members(json: &[u8]) -> Result<Vec<Member>, String> {
    let meta: Metadata =
        serde_json::from_slice(json).map_err(|e| format!("bad cargo metadata: {e}"))?;
    Ok(meta
        .packages
        .into_iter()
        .filter(|p| meta.workspace_members.contains(&p.id))
        .map(|p| Member {
            name: p.name,
            deps: p.dependencies.into_iter().map(|d| d.name).collect(),
        })
        .collect())
}

fn layer_of(name: &str) -> Option<u8> {
    LAYERS.iter().find(|(n, _)| *n == name).map(|(_, l)| *l)
}

/// Returns one message per violation; empty means the workspace is clean.
pub fn check(members: &[Member]) -> Vec<String> {
    let mut problems = Vec::new();
    for m in members {
        if EXEMPT.contains(&m.name.as_str()) {
            continue;
        }
        let Some(layer) = layer_of(&m.name) else {
            problems.push(format!(
                "`{}` is not registered in xtask/src/layers.rs",
                m.name
            ));
            continue;
        };
        for dep in &m.deps {
            if let Some(dep_layer) = layer_of(dep) {
                if dep_layer >= layer {
                    problems.push(format!(
                        "`{}` (L{layer}) depends on `{dep}` (L{dep_layer}); only lower layers are allowed",
                        m.name
                    ));
                }
            } else if layer < UI_LAYER && UI_CRATES.contains(&dep.as_str()) {
                problems.push(format!(
                    "`{}` (L{layer}) must not depend on UI crate `{dep}`",
                    m.name
                ));
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(name: &str, deps: &[&str]) -> Member {
        Member {
            name: name.to_string(),
            deps: deps.iter().map(|d| d.to_string()).collect(),
        }
    }

    #[test]
    fn empty_workspace_is_clean() {
        assert!(check(&[member("xtask", &["serde"])]).is_empty());
    }

    #[test]
    fn lower_layer_dependency_is_allowed() {
        let ms = [
            member("inbetween-geom", &[]),
            member("inbetween-morph", &["inbetween-geom", "rayon"]),
        ];
        assert!(check(&ms).is_empty());
    }

    #[test]
    fn upward_and_same_layer_dependencies_are_rejected() {
        assert_eq!(
            check(&[member("inbetween-geom", &["inbetween-morph"])]).len(),
            1
        );
        assert_eq!(
            check(&[member("inbetween-geom", &["inbetween-raster"])]).len(),
            1
        );
    }

    #[test]
    fn ui_crates_are_banned_below_l6() {
        assert_eq!(check(&[member("inbetween-engine", &["egui"])]).len(), 1);
        assert!(check(&[member("inbetween-ui-egui", &["egui", "eframe"])]).is_empty());
    }

    #[test]
    fn unregistered_crate_is_reported() {
        let problems = check(&[member("inbetween-mystery", &[])]);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("not registered"));
    }

    #[test]
    fn registry_has_no_duplicates() {
        for (i, (a, _)) in LAYERS.iter().enumerate() {
            assert!(
                LAYERS.iter().skip(i + 1).all(|(b, _)| a != b),
                "duplicate {a}"
            );
        }
    }

    #[test]
    fn parses_cargo_metadata_json() {
        let json = br#"{"packages":[{"id":"a 0.1","name":"inbetween-geom","dependencies":[{"name":"serde"}]},{"id":"b 0.1","name":"outsider","dependencies":[]}],"workspace_members":["a 0.1"]}"#;
        let ms = parse_members(json).unwrap();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].name, "inbetween-geom");
        assert_eq!(ms[0].deps, vec!["serde".to_string()]);
    }
}
