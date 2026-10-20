//! The JSON-RPC and MCP handshake, tool list and error mapping.

use inbetween_engine::Registry;
use inbetween_mcp::schema::tool_name;
use inbetween_mcp::Server;
use serde_json::{json, Value};

fn ask(server: &mut Server, message: Value) -> Value {
    let reply = server.handle(&message.to_string()).unwrap();
    serde_json::from_str(&reply).unwrap()
}

fn call(server: &mut Server, name: &str, arguments: Value) -> Value {
    let reply = ask(
        server,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
               "params": {"name": name, "arguments": arguments}}),
    );
    reply["result"].clone()
}

#[test]
fn initialize_echoes_a_known_version_and_falls_back_to_the_newest() {
    let mut s = Server::new();
    for (asked, want) in [
        ("2024-11-05", "2024-11-05"),
        ("2025-03-26", "2025-03-26"),
        ("2025-06-18", "2025-06-18"),
        ("1999-01-01", "2025-06-18"),
    ] {
        let r = ask(
            &mut s,
            json!({"jsonrpc": "2.0", "id": 7, "method": "initialize",
                   "params": {"protocolVersion": asked, "capabilities": {}}}),
        );
        assert_eq!(r["id"], 7);
        assert_eq!(r["result"]["protocolVersion"], want);
        assert_eq!(r["result"]["serverInfo"]["name"], "inbetween");
        assert!(r["result"]["capabilities"]["tools"].is_object());
    }
    // No params at all still works.
    let r = ask(
        &mut s,
        json!({"jsonrpc": "2.0", "id": "a", "method": "initialize"}),
    );
    assert_eq!(r["id"], "a");
    assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
}

#[test]
fn ping_answers_and_notifications_do_not() {
    let mut s = Server::new();
    assert_eq!(
        ask(&mut s, json!({"jsonrpc": "2.0", "id": 2, "method": "ping"}))["result"],
        json!({})
    );
    for note in [
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 1}}),
        json!({"jsonrpc": "2.0", "method": "no/such/thing"}),
        json!({"jsonrpc": "2.0", "id": 3, "result": {}}),
    ] {
        assert!(s.handle(&note.to_string()).is_none(), "{note}");
    }
}

#[test]
fn protocol_faults_are_json_rpc_errors() {
    let mut s = Server::new();
    let mut code = |line: &str| -> i64 {
        let v: Value = serde_json::from_str(&s.handle(line).unwrap()).unwrap();
        assert!(v["error"]["message"].is_string(), "{line}");
        v["error"]["code"].as_i64().unwrap()
    };
    assert_eq!(code("not json"), -32700);
    assert_eq!(code(""), -32700);
    assert_eq!(code("[]"), -32600);
    assert_eq!(
        code("[{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}]"),
        -32600
    );
    assert_eq!(code("42"), -32600);
    assert_eq!(code("{\"id\": {}, \"method\": \"ping\"}"), -32600);
    assert_eq!(code("{\"id\": 1}"), -32600);
    assert_eq!(code("{\"id\": 1, \"method\": 5}"), -32600);
    assert_eq!(code("{\"id\": 1, \"method\": \"nope\"}"), -32601);
    assert_eq!(
        code("{\"id\": 1, \"method\": \"tools/call\", \"params\": {}}"),
        -32602
    );
    assert_eq!(
        code("{\"id\": 1, \"method\": \"tools/call\", \"params\": {\"name\": \"no_such\"}}"),
        -32602
    );
    assert_eq!(
        code("{\"id\": 1, \"method\": \"tools/call\", \"params\": {\"name\": \"project_info\", \"arguments\": [1]}}"),
        -32602
    );
}

#[test]
fn every_command_is_a_tool_with_a_sound_schema() {
    let mut s = Server::new();
    let r = ask(
        &mut s,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
    );
    let tools = r["result"]["tools"].as_array().unwrap();
    let registry = Registry::standard();
    assert_eq!(tools.len(), registry.specs().len());

    let mut names = std::collections::BTreeSet::new();
    for (tool, spec) in tools.iter().zip(registry.specs()) {
        let name = tool["name"].as_str().unwrap();
        assert_eq!(name, tool_name(spec.id));
        assert!(
            !name.is_empty()
                && name.len() <= 128
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "{name}"
        );
        assert!(names.insert(name.to_string()), "duplicate tool {name}");
        assert!(tool["description"].as_str().is_some_and(|d| !d.is_empty()));
        assert!(tool["title"].is_string());

        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "{name}");
        assert_eq!(schema["additionalProperties"], false, "{name}");
        let props = schema["properties"].as_object().unwrap();
        assert_eq!(props.len(), spec.params.len(), "{name}");
        for req in schema["required"].as_array().into_iter().flatten() {
            assert!(props.contains_key(req.as_str().unwrap()), "{name}: {req}");
        }
        for (pname, prop) in props {
            assert!(prop["type"].is_string(), "{name}.{pname}");
            if prop["type"] == "array" {
                assert!(prop["items"]["type"].is_string(), "{name}.{pname}");
            }
        }
    }
}

#[test]
fn schemas_carry_ranges_choices_and_requirements() {
    let mut s = Server::new();
    let r = ask(
        &mut s,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
    );
    let tools = r["result"]["tools"].as_array().unwrap();
    let find = |n: &str| tools.iter().find(|t| t["name"] == n).unwrap();

    let preview = &find("frame_preview")["inputSchema"]["properties"];
    assert_eq!(preview["max_size"]["type"], "integer");
    assert_eq!(preview["max_size"]["minimum"], 16.0);
    assert_eq!(preview["max_size"]["maximum"], 4096.0);
    assert!(preview["max_size"]["description"]
        .as_str()
        .unwrap()
        .contains("512"));

    let export = &find("export_frame")["inputSchema"];
    assert_eq!(export["required"], json!(["out"]));
    assert!(export["properties"]["out"]["description"]
        .as_str()
        .unwrap()
        .contains("png"));
}

#[test]
fn failing_commands_are_tool_errors_not_protocol_errors() {
    let mut s = Server::new();
    let disabled = call(&mut s, "sequence_length", json!({}));
    assert_eq!(disabled["isError"], true);
    assert_eq!(disabled["structuredContent"]["code"], "disabled");
    assert!(disabled["content"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("disabled: "));

    let bad = call(&mut s, "project_open", json!({"bogus": 1}));
    assert_eq!(bad["isError"], true);
    assert_eq!(bad["structuredContent"]["code"], "bad_params");

    let missing = call(
        &mut s,
        "project_open",
        json!({"path": "definitely/not/here.inbetween"}),
    );
    assert_eq!(missing["isError"], true);

    let info = call(&mut s, "project_info", Value::Null);
    assert_eq!(info["isError"], false);
    assert!(info["structuredContent"].is_object());
}
