//! A scripted agent conversation: state persists across tool calls, and a preview comes back
//! as an image.

use base64::Engine as _;
use inbetween_mcp::Server;
use serde_json::{json, Value};
use std::path::PathBuf;

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn call(server: &mut Server, name: &str, arguments: Value) -> Value {
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                         "params": {"name": name, "arguments": arguments}});
    let reply: Value = serde_json::from_str(&server.handle(&message.to_string()).unwrap()).unwrap();
    assert!(reply["error"].is_null(), "{reply}");
    reply["result"].clone()
}

fn ok(server: &mut Server, name: &str, arguments: Value) -> Value {
    let r = call(server, name, arguments);
    assert_eq!(r["isError"], false, "{name}: {r}");
    r
}

#[test]
fn open_edit_undo_preview_export() {
    let dir = Dir(std::env::temp_dir().join(format!("inbetween-mcp-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&dir.0);
    std::fs::create_dir_all(&dir.0).unwrap();
    let (a, b) = inbetween_testkit::demo::pair(96, 64).unwrap();
    let path = |n: &str| dir.0.join(n).display().to_string();
    for (name, img) in [("a.png", &a), ("b.png", &b)] {
        let png = inbetween_codecs::encode_png(96, 64, &img.to_rgba8()).unwrap();
        std::fs::write(dir.0.join(name), png).unwrap();
    }

    let mut s = Server::new();
    ok(
        &mut s,
        "file_wizard",
        json!({"images": [path("a.png"), path("b.png")]}),
    );
    let before = ok(&mut s, "project_info", json!({}));
    let dots = |info: &Value| info["structuredContent"]["sequences"][0]["dots"].clone();

    ok(&mut s, "dots_add", json!({"x": 30, "y": 20}));
    let added = ok(&mut s, "project_info", json!({}));
    assert_ne!(
        dots(&before),
        dots(&added),
        "the added dot is visible to the next call"
    );

    ok(&mut s, "edit_undo", json!({}));
    let undone = ok(&mut s, "project_info", json!({}));
    assert_eq!(dots(&before), dots(&undone));

    let preview = ok(&mut s, "frame_preview", json!({"t": 0.5, "max_size": 48}));
    let image = preview["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "image")
        .expect("an image block");
    assert_eq!(image["mimeType"], "image/png");
    let png = base64::engine::general_purpose::STANDARD
        .decode(image["data"].as_str().unwrap())
        .unwrap();
    let decoded = inbetween_codecs::decode(&png).unwrap();
    assert_eq!((decoded.width, decoded.height), (48, 32));
    let text = preview["content"][0]["text"].as_str().unwrap();
    assert!(!text.contains("png_base64") && text.len() < 400, "{text}");
    assert_eq!(preview["structuredContent"]["width"], 48);

    let out = path("frame.png");
    ok(&mut s, "export_frame", json!({"out": out, "t": 0.0}));
    assert!(std::fs::metadata(&out).is_ok_and(|m| m.len() > 0));

    let saved = path("p.inbetween");
    ok(&mut s, "project_saveAs", json!({"path": saved}));
    let mut fresh = Server::new();
    ok(&mut fresh, "project_open", json!({"path": saved}));
}
