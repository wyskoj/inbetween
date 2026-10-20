//! Command parameter lists as JSON Schema, and the id <-> tool name mapping.

use inbetween_engine::{ParamKind, ParamSpec, PathUse, MAX_STRING_BYTES};
use serde_json::{json, Map, Value};

/// The MCP tool name for a command id. Clients such as the Claude API only accept
/// `[a-zA-Z0-9_-]`, so the dots in an id become underscores (`export.frame` -> `export_frame`).
pub fn tool_name(id: &str) -> String {
    id.replace('.', "_")
}

fn element(kind: ParamKind) -> Value {
    match kind {
        ParamKind::IntegerList => json!({"type": "integer"}),
        ParamKind::NumberList => json!({"type": "number"}),
        _ => json!({"type": "string", "maxLength": MAX_STRING_BYTES}),
    }
}

fn describe(p: &ParamSpec) -> String {
    let mut text = p.description.to_string();
    if let Some(default) = p.default {
        text.push_str(&format!(" Default: {default}."));
    }
    let usage = match p.path_use {
        Some(PathUse::OpenFile) => Some("an existing file"),
        Some(PathUse::SaveFile) => Some("a file to write"),
        Some(PathUse::Folder) => Some("a folder"),
        None => None,
    };
    if let Some(usage) = usage {
        text.push_str(&format!(" Path: {usage}"));
        if !p.extensions.is_empty() {
            text.push_str(&format!(" ({})", p.extensions.join(", ")));
        }
        text.push('.');
    }
    text
}

/// The JSON Schema for one parameter.
pub fn param_schema(p: &ParamSpec) -> Value {
    let mut schema = match p.kind {
        ParamKind::Path | ParamKind::String => {
            let mut s = json!({"type": "string", "maxLength": MAX_STRING_BYTES});
            if !p.choices.is_empty() {
                s["enum"] = json!(p.choices);
            }
            s
        }
        ParamKind::Integer => json!({"type": "integer"}),
        ParamKind::Number => json!({"type": "number"}),
        ParamKind::Bool => json!({"type": "boolean"}),
        ParamKind::PathList | ParamKind::IntegerList | ParamKind::NumberList => {
            json!({"type": "array", "items": element(p.kind)})
        }
    };
    let list = matches!(
        p.kind,
        ParamKind::PathList | ParamKind::IntegerList | ParamKind::NumberList
    );
    let (lo, hi) = if list {
        ("minItems", "maxItems")
    } else {
        ("minimum", "maximum")
    };
    if let Some(min) = p.min {
        schema[lo] = json!(min);
    }
    if let Some(max) = p.max {
        schema[hi] = json!(max);
    }
    schema["description"] = Value::String(describe(p));
    schema
}

/// The `inputSchema` of a tool: an object with exactly the command's parameters.
pub fn input_schema(params: &[ParamSpec]) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for p in params {
        properties.insert(p.name.to_string(), param_schema(p));
        if p.required {
            required.push(Value::String(p.name.to_string()));
        }
    }
    let mut schema = json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false,
    });
    if !required.is_empty() {
        schema["required"] = Value::Array(required);
    }
    schema
}
