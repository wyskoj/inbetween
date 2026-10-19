//! Plain-text rendering of engine results for people at a terminal.

use inbetween_engine::CommandInfo;
use serde_json::Value;
use std::fmt::Write;

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn n(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

pub fn render_summary(r: &Value) -> String {
    format!(
        "Wrote {} frame{} ({} x {}) to {}",
        n(r, "frames"),
        if n(r, "frames") == 1 { "" } else { "s" },
        n(r, "width"),
        n(r, "height"),
        s(r, "dir")
    )
}

pub fn frame_summary(r: &Value) -> String {
    format!(
        "Wrote {} ({} x {})",
        s(r, "out"),
        n(r, "width"),
        n(r, "height")
    )
}

/// Writing to a `String` cannot fail, so the `write!` results are ignored.
pub fn info(v: &Value) -> String {
    let mut out = String::new();
    match v.get("path").and_then(Value::as_str) {
        Some(p) => {
            let _ = writeln!(out, "Project: {p}");
        }
        None => out.push_str("Project: (unsaved)\n"),
    }
    let output = v.get("output").unwrap_or(&Value::Null);
    let _ = writeln!(
        out,
        "Output: {} x {}",
        n(output, "width"),
        n(output, "height")
    );
    let sequences = v
        .get("sequences")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let _ = writeln!(out, "Sequences: {}", sequences.len());
    for seq in sequences {
        let name = if s(seq, "name").is_empty() {
            "(unnamed)"
        } else {
            s(seq, "name")
        };
        let holds = match (n(seq, "hold_start"), n(seq, "hold_end")) {
            (0, 0) => String::new(),
            (a, b) => format!(" (+{a} hold at the start, +{b} at the end)"),
        };
        let _ = writeln!(
            out,
            "  {}. {name}: {} frames{holds}, {} dots, {}",
            n(seq, "index"),
            n(seq, "frames"),
            n(seq, "dots"),
            s(seq, "morph")
        );
        let _ = writeln!(out, "     Image 1: {}", s(seq, "image1"));
        let _ = writeln!(out, "     Image 2: {}", s(seq, "image2"));
    }
    out.trim_end().to_string()
}

pub fn commands(list: &[CommandInfo]) -> String {
    let mut out = String::new();
    for c in list {
        let shortcut = c.shortcut.map(|s| format!("  [{s}]")).unwrap_or_default();
        let _ = writeln!(out, "{}{shortcut}", c.id);
        let _ = writeln!(out, "    {} ({})", c.description, c.menu);
        for p in &c.params {
            let req = if p.required { "required" } else { "optional" };
            let mut line = format!("    --{} <{:?}> {req}: {}", p.name, p.kind, p.description);
            if let (Some(lo), Some(hi)) = (p.min, p.max) {
                let _ = write!(line, " [{lo}..{hi}]");
            }
            if let Some(d) = p.default {
                let _ = write!(line, " (default: {d})");
            }
            let _ = writeln!(out, "{line}");
        }
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_engine::Registry;
    use serde_json::json;

    #[test]
    fn summaries() {
        assert_eq!(
            render_summary(&json!({"frames": 1, "width": 4, "height": 3, "dir": "d"})),
            "Wrote 1 frame (4 x 3) to d"
        );
        assert!(render_summary(&json!({"frames": 12})).contains("12 frames"));
        assert_eq!(
            frame_summary(&json!({"out": "a.png", "width": 2, "height": 1})),
            "Wrote a.png (2 x 1)"
        );
    }

    #[test]
    fn info_lists_sequences() {
        let v = json!({
            "path": null,
            "output": {"width": 640, "height": 480},
            "sequences": [{"index": 0, "name": "", "frames": 12, "dots": 5, "morph": "morph", "image1": "a.png", "image2": "b.png"}]
        });
        let t = info(&v);
        assert!(
            t.contains("(unsaved)") && t.contains("640 x 480") && t.contains("12 frames, 5 dots")
        );
        assert!(t.contains("Image 1: a.png"));
        assert!(info(&json!({})).contains("Sequences: 0"));
    }

    #[test]
    fn every_command_is_listed_with_its_params() {
        let reg = Registry::standard();
        let t = commands(&reg.describe());
        for spec in reg.specs() {
            assert!(t.contains(spec.id), "{}", spec.id);
            for p in spec.params {
                assert!(
                    t.contains(&format!("--{}", p.name)),
                    "{} {}",
                    spec.id,
                    p.name
                );
            }
        }
    }
}
