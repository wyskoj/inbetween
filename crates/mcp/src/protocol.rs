//! JSON-RPC 2.0 envelopes and the MCP handshake.

use serde_json::{json, Value};

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

/// Protocol versions this server speaks, newest first.
pub const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

pub const INSTRUCTIONS: &str = "Inbetween is an image morphing app. This server holds one open project for the whole connection: \
every tool is an Inbetween command, and edits made by one call are seen by the next. Start with `project_open` \
(or `file_new` / `file_wizard`), then edit with the `dots_*`, `sequence_*`, `curve_*` and `effect_*` tools. \
`frame_preview` returns a picture of any frame so you can check your work; `export_*` write files. \
Undo with `edit_undo`. Save with `project_save` or `project_saveAs`.";

pub fn result(id: &Value, result: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

pub fn error(id: &Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

/// The version to answer an `initialize` with: the client's if we speak it, else our newest.
pub fn negotiate(requested: Option<&str>) -> &'static str {
    requested
        .and_then(|r| VERSIONS.iter().find(|v| **v == r))
        .copied()
        .unwrap_or(VERSIONS[0])
}

pub fn initialize_result(version: &str) -> Value {
    json!({
        "protocolVersion": version,
        "capabilities": {"tools": {"listChanged": false}},
        "serverInfo": {"name": "inbetween", "version": env!("CARGO_PKG_VERSION")},
        "instructions": INSTRUCTIONS,
    })
}
