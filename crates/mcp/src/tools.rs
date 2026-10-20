//! `tools/list` and `tools/call`.

use crate::schema::{input_schema, tool_name};
use inbetween_engine::{CommandError, CommandSpec, Registry, Session};
use serde_json::{json, Map, Value};

fn description(spec: &CommandSpec) -> String {
    let mut text = spec.description.to_string();
    if !spec.menu.is_empty() {
        text.push_str(&format!(" Menu: {}.", spec.menu));
    }
    if !spec.disabled_reason.is_empty() {
        text.push_str(&format!(" Unavailable when: {}.", spec.disabled_reason));
    }
    text
}

pub fn list(registry: &Registry) -> Value {
    let tools: Vec<Value> = registry
        .specs()
        .iter()
        .map(|spec| {
            json!({
                "name": tool_name(spec.id),
                "title": spec.label,
                "description": description(spec),
                "inputSchema": input_schema(spec.params),
            })
        })
        .collect();
    json!({ "tools": tools })
}

/// The command a tool name stands for.
pub fn find<'a>(registry: &'a Registry, name: &str) -> Option<&'a CommandSpec> {
    registry.specs().iter().find(|s| tool_name(s.id) == name)
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

/// Run a command and shape its outcome as an MCP tool result. A failing command is a
/// successful JSON-RPC response with `isError` set, so the agent can read why and retry.
pub fn call(
    registry: &Registry,
    session: &mut Session,
    spec: &CommandSpec,
    arguments: Map<String, Value>,
) -> Value {
    match registry.run_caught(session, spec.id, &Value::Object(arguments)) {
        Ok(value) => success(value),
        Err(e) => failure(&e),
    }
}

fn success(mut value: Value) -> Value {
    let mut content = Vec::new();
    // A rendered picture travels as an MCP image block, not as text.
    let image = value
        .as_object_mut()
        .and_then(|o| o.remove("png_base64"))
        .and_then(|v| v.as_str().map(str::to_string));
    content.push(json!({"type": "text", "text": pretty(&value)}));
    if let Some(data) = image {
        content.push(json!({"type": "image", "data": data, "mimeType": "image/png"}));
    }
    let mut out = json!({"content": content, "isError": false});
    if value.is_object() {
        out["structuredContent"] = value;
    }
    out
}

fn failure(e: &CommandError) -> Value {
    let code = e.code();
    let message = e.to_string();
    json!({
        "content": [{"type": "text", "text": format!("{code}: {message}")}],
        "structuredContent": {"code": code, "message": message},
        "isError": true,
    })
}
