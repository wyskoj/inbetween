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

    // Movie-level colour, a filter at both scopes, a movie effect and a sound.
    let plain = ok(&mut s, "frame_preview", json!({"t": 0.5, "max_size": 48}));
    ok(&mut s, "movie_adjust", json!({"negative": true}));
    ok(&mut s, "movie_filter_blur", json!({"radius": 2}));
    ok(
        &mut s,
        "movie_filter_blur",
        json!({"scope": "sequence", "radius": 1}),
    );
    ok(
        &mut s,
        "effect_backColor",
        json!({"scope": "movie", "color": "#336699"}),
    );
    let changed = ok(&mut s, "frame_preview", json!({"t": 0.5, "max_size": 48}));
    let data = |r: &Value| {
        r["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["type"] == "image")
            .unwrap()["data"]
            .clone()
    };
    assert_ne!(data(&plain), data(&changed));
    let tone = path("tone.wav");
    let mut wav = Vec::new();
    wav.extend_from_slice(&tiny_wav_header());
    wav.extend_from_slice(&[0, 0, 0x10, 0x20, 0, 0, 0xf0, 0xdf]);
    std::fs::write(&tone, wav).unwrap();
    let sound = ok(&mut s, "effect_sound", json!({"path": tone, "loop": true}));
    assert_eq!(sound["structuredContent"]["loop"], true);
    assert_eq!(sound["structuredContent"]["rate"], 8000);
    let bad = call(&mut s, "movie_filter_blur", json!({"radius": 0}));
    assert_eq!(bad["isError"], true);
    for _ in 0..5 {
        ok(&mut s, "edit_undo", json!({}));
    }

    let out = path("frame.png");
    ok(&mut s, "export_frame", json!({"out": out, "t": 0.0}));
    assert!(std::fs::metadata(&out).is_ok_and(|m| m.len() > 0));

    let saved = path("p.inbetween");
    ok(&mut s, "project_saveAs", json!({"path": saved}));
    let mut fresh = Server::new();
    ok(&mut fresh, "project_open", json!({"path": saved}));
}

/// The 44-byte header of a mono 8 kHz 16-bit WAV holding four samples.
fn tiny_wav_header() -> Vec<u8> {
    let mut h = Vec::new();
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&44u32.to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&8000u32.to_le_bytes());
    h.extend_from_slice(&16000u32.to_le_bytes());
    h.extend_from_slice(&2u16.to_le_bytes());
    h.extend_from_slice(&16u16.to_le_bytes());
    h.extend_from_slice(b"data");
    h.extend_from_slice(&8u32.to_le_bytes());
    h
}
