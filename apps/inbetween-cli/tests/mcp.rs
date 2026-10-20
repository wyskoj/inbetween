//! `ib mcp`: the real binary speaking MCP over its pipes.

use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn demo_project() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../demo/demo.inbetween")
        .display()
        .to_string()
}

fn serve(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ib"))
        .arg("mcp")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Dropping stdin after the write closes it, which ends the server.
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn a_session_over_stdio_prints_only_protocol_lines() {
    let lines = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
               "params": {"protocolVersion": "2025-06-18"}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
               "params": {"name": "frame_preview", "arguments": {"t": 0.5, "max_size": 64}}}),
    ];
    let input: String = lines.iter().map(|l| format!("{l}\n")).collect();
    let out = serve(&["--project", &demo_project()], &input);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8(out.stdout).unwrap();
    let replies: Vec<Value> = stdout
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    // Three requests with ids, one notification without.
    assert_eq!(replies.len(), 3, "{stdout}");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "inbetween");
    assert!(replies[1]["result"]["tools"].as_array().unwrap().len() > 100);
    // The project was opened by --project, so the preview works with no other setup.
    let call = &replies[2]["result"];
    assert_eq!(call["isError"], false, "{call}");
    assert!(call["content"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["type"] == "image"));
}

#[test]
fn a_bad_project_stops_before_serving() {
    let out = serve(&["--project", "no/such/file.inbetween"], "");
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("could not open"), "{stderr}");
}

#[test]
fn an_empty_input_exits_cleanly() {
    let out = serve(&[], "");
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}
