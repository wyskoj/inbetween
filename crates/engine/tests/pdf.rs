//! `export.pdf`.

use inbetween_engine::{CommandError, Registry, Session};
use serde_json::{json, Value};
use std::path::PathBuf;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let d = std::env::temp_dir().join(format!("inbetween-pdf-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Self(d)
    }

    fn s(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(s: &mut Session, id: &str, p: Value) -> Result<Value, CommandError> {
    Registry::standard().run(s, id, &p)
}

fn project(d: &Dir) -> Session {
    for (name, shade) in [("a.png", 20u8), ("b.png", 120), ("c.png", 220)] {
        let rgba: Vec<u8> = (0..32 * 24)
            .flat_map(|i| [shade, (i % 32 * 8) as u8, 90, 255])
            .collect();
        std::fs::write(
            d.0.join(name),
            inbetween_codecs::encode_png(32, 24, &rgba).unwrap(),
        )
        .unwrap();
    }
    let mut s = Session::new();
    run(
        &mut s,
        "tools.sequencePacker",
        json!({"images": [d.s("a.png"), d.s("b.png"), d.s("c.png")], "frames": 4}),
    )
    .unwrap();
    s
}

fn count(h: &[u8], n: &str) -> usize {
    h.windows(n.len()).filter(|w| *w == n.as_bytes()).count()
}

#[test]
fn every_content_kind_makes_a_pdf_with_the_right_number_of_pictures() {
    let d = Dir::new("a");
    let mut s = project(&d);
    let out = d.s("sheet.pdf");
    // Sources: two sequences, two images each.
    let r = run(
        &mut s,
        "export.pdf",
        json!({"out": out, "content": "sources", "title": "Sources"}),
    )
    .unwrap();
    assert_eq!(r["pictures"], 4);
    let pdf = std::fs::read(&out).unwrap();
    assert!(pdf.starts_with(b"%PDF-1.4"));
    assert_eq!(count(&pdf, "/Subtype /Image"), 4);
    assert_eq!(count(&pdf, "(a to b: Image 1, a.png)"), 1);
    // Preview: 3 frames of each of 2 sequences.
    let r = run(&mut s, "export.pdf", json!({"out": out, "per_sequence": 3})).unwrap();
    assert_eq!(r["pictures"], 6);
    if let Some(dir) = std::env::var_os("INBETWEEN_UI_SHOTS") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(&out, dir.join("contact_sheet.pdf")).unwrap();
    }
    let pdf = std::fs::read(&out).unwrap();
    assert_eq!(count(&pdf, "frame 4 of 4)"), 2);
    // Frames: every movie frame, 8 in all, or a range.
    let r = run(
        &mut s,
        "export.pdf",
        json!({"out": out, "content": "frames", "columns": 4}),
    )
    .unwrap();
    assert_eq!(r["pictures"], 8);
    let r = run(
        &mut s,
        "export.pdf",
        json!({"out": out, "content": "frames", "first": 2, "last": 3}),
    )
    .unwrap();
    assert_eq!(r["pictures"], 2);
    // One sequence only.
    let r = run(
        &mut s,
        "export.pdf",
        json!({"out": out, "content": "frames", "sequence": 1}),
    )
    .unwrap();
    assert_eq!(r["pictures"], 4);
    let r = run(
        &mut s,
        "export.pdf",
        json!({"out": out, "content": "sources", "sequence": 0}),
    )
    .unwrap();
    assert_eq!(r["pictures"], 2);
}

#[test]
fn embedded_pictures_decode_and_options_change_the_page() {
    let d = Dir::new("b");
    let mut s = project(&d);
    let out = d.s("sheet.pdf");
    run(&mut s, "export.pdf", json!({"out": out, "content": "sources", "captions": false, "landscape": true, "page": "letter", "title": "T"})).unwrap();
    let pdf = std::fs::read(&out).unwrap();
    assert_eq!(count(&pdf, "/MediaBox [0 0 792 612]"), 1);
    assert_eq!(count(&pdf, "Image 1,"), 0, "no captions");
    assert_eq!(count(&pdf, "(T)"), 1);
    // The first embedded JPEG is a real picture of the right size.
    let find = |from: usize, needle: &str| {
        from + pdf[from..]
            .windows(needle.len())
            .position(|w| w == needle.as_bytes())
            .unwrap()
    };
    let key = "/Filter /DCTDecode /Length ";
    let at = find(0, key) + key.len();
    let digits: String = pdf[at..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .map(|b| *b as char)
        .collect();
    let len: usize = digits.parse().unwrap();
    let start = find(
        at, "stream
",
    ) + 7;
    let jpeg = &pdf[start..start + len];
    let img = inbetween_codecs::decode(jpeg).unwrap();
    assert_eq!((img.width, img.height), (32, 24));
}

#[test]
fn bad_requests_are_refused_and_save_open_options_work() {
    let d = Dir::new("c");
    let mut s = project(&d);
    let out = d.s("x.pdf");
    assert!(matches!(
        run(&mut s, "export.pdf", json!({"out": out, "sequence": 9})),
        Err(CommandError::NoSuchSequence { .. })
    ));
    assert!(run(
        &mut s,
        "export.pdf",
        json!({"out": out, "content": "frames", "first": 5, "last": 2})
    )
    .is_err());
    assert!(run(
        &mut s,
        "export.pdf",
        json!({"out": out, "content": "nonsense"})
    )
    .is_err());
    assert!(!std::path::Path::new(&out).exists());
    let opened = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = std::sync::Arc::clone(&opened);
    s.set_opener(std::sync::Arc::new(move |p: &std::path::Path| {
        log.lock().unwrap().push(p.to_path_buf());
        Ok(())
    }));
    let r = run(
        &mut s,
        "export.pdf",
        json!({"out": out, "open_after": true}),
    )
    .unwrap();
    assert_eq!(r["opened"], true);
    assert_eq!(*opened.lock().unwrap(), vec![PathBuf::from(&out)]);
    // An empty project has nothing to draw.
    let mut empty = Session::new();
    assert!(run(&mut empty, "export.pdf", json!({"out": d.s("e.pdf")})).is_err());
}
