//! Never crash: hostile JSON-RPC lines, every tool with junk arguments, and oversized input.

use inbetween_engine::Registry;
use inbetween_mcp::schema::tool_name;
use inbetween_mcp::Server;
use serde_json::{json, Value};
use std::io::Cursor;

/// Loop counts shrink with `INBETWEEN_FUZZ_SCALE=10`, as in the other fuzz loops.
fn scaled(n: usize) -> usize {
    let scale = std::env::var("INBETWEEN_FUZZ_SCALE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    (n / scale).max(1)
}

/// A response line must be one JSON-RPC object carrying `result` or `error`.
fn check_reply(reply: &str) {
    assert!(!reply.contains('\n'), "reply spans lines");
    let v: Value = serde_json::from_str(reply).unwrap();
    assert_eq!(v["jsonrpc"], "2.0");
    assert!(
        v.get("result").is_some() ^ v.get("error").is_some(),
        "{reply}"
    );
}

#[test]
fn mutated_and_truncated_messages_never_panic() {
    let seeds = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
               "params": {"name": "export_frame", "arguments": {"out": "x.png", "t": 0.5}}}),
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
               "params": {"name": "dots_add", "arguments": {"x": 1e308, "y": -1e308}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    ]
    .map(|v| v.to_string().into_bytes());

    let mut server = Server::new();
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for round in 0..scaled(3000) {
        let mut bytes = seeds[round % seeds.len()].clone();
        for _ in 0..(next() % 4) {
            let at = (next() as usize) % bytes.len().max(1);
            match next() % 4 {
                0 => {
                    let i = at.min(bytes.len() - 1);
                    bytes[i] = next() as u8;
                }
                1 => bytes.truncate(at),
                2 => bytes.insert(at.min(bytes.len()), next() as u8),
                _ => {
                    bytes.remove(at.min(bytes.len() - 1));
                }
            }
            if bytes.is_empty() {
                bytes.push(b'{');
            }
        }
        // Mutations may break UTF-8; `serve` handles that, `handle` takes text.
        let mut out = Vec::new();
        let mut input = bytes.clone();
        input.push(b'\n');
        server.serve(Cursor::new(input), &mut out).unwrap();
        for line in String::from_utf8(out).unwrap().lines() {
            check_reply(line);
        }
    }
}

#[test]
fn deep_nesting_and_odd_numbers() {
    let mut server = Server::new();
    let deep = format!("{}1{}", "[".repeat(5000), "]".repeat(5000));
    check_reply(&server.handle(&deep).unwrap());
    let nested_args = format!(
        "{{\"id\":1,\"method\":\"tools/call\",\"params\":{{\"name\":\"project_info\",\"arguments\":{{\"a\":{deep}}}}}}}"
    );
    check_reply(&server.handle(&nested_args).unwrap());
    for id in ["1e999", "-0", "18446744073709551616", "\"\"", "0.5"] {
        let line = format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"ping\"}}");
        if let Some(reply) = server.handle(&line) {
            check_reply(&reply);
        }
    }
}

#[test]
fn every_tool_survives_junk_arguments() {
    let mut server = Server::new();
    let junk = [
        json!({}),
        json!(null),
        json!({"x": 1}),
        json!({"path": "", "out": "", "sequence": -1, "t": "x", "x": 1e308, "y": f64::MAX}),
        json!({"indices": [-1, 99999999999i64], "points": [1e308, null], "images": []}),
        json!({"sequence": 9_007_199_254_740_993i64, "frame": u32::MAX, "frames": 0, "on": "yes"}),
    ];
    for spec in Registry::standard().specs() {
        // Commands that write files do so relative to the working directory; point the
        // junk at nothing useful instead of skipping them.
        let name = tool_name(spec.id);
        for args in &junk {
            let line = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                              "params": {"name": name, "arguments": args}})
            .to_string();
            let reply = server.handle(&line).expect("a reply");
            check_reply(&reply);
            let v: Value = serde_json::from_str(&reply).unwrap();
            // A panic inside a command would surface as `internal`.
            assert_ne!(
                v["result"]["structuredContent"]["code"], "internal",
                "{name} {args}: {reply}"
            );
        }
    }
}

#[test]
fn oversized_lines_are_answered_and_skipped() {
    let mut server = Server::new();
    let mut input = vec![b'x'; inbetween_mcp::MAX_LINE_BYTES + 10];
    input.push(b'\n');
    input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n\n   \n");
    let mut out = Vec::new();
    server.serve(Cursor::new(input), &mut out).unwrap();
    let lines: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["error"]["code"], -32600);
    assert_eq!(lines[1]["id"], 9);
}
