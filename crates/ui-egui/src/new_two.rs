//! The New From Two Images dialog's state: two image slots (start and end) and an optional
//! sequence name. It builds the parameters for `project.new`, so the CLI and MCP keep using the
//! same command. Each slot reads its file when it is picked, for the thumbnail, the size and an
//! early, specific error.

use egui::ColorImage;
use inbetween_codecs::{CodecError, Rgba8Image};
use inbetween_engine::CommandError;
use serde_json::{json, Map, Value};
use std::path::Path;

/// Longest side of a slot thumbnail, in pixels.
pub const THUMB_SIDE: usize = 320;

/// A picked file that could be read.
#[derive(Clone, Debug)]
pub struct Picked {
    pub width: u32,
    pub height: u32,
    /// The format shown under the name ("PNG"), from the extension.
    pub format: String,
    /// Small enough for a texture; turned into one by the window.
    pub thumb: ColorImage,
}

/// A picked file that could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unreadable {
    /// What to do about it, in a sentence.
    pub reason: String,
    /// The engine's own message, for the tooltip.
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct Slot {
    pub path: String,
    pub read: Result<Picked, Unreadable>,
}

impl Slot {
    /// Read the file at `path` (an image, or a video's first frame).
    pub fn load(path: String) -> Self {
        let read = match inbetween_engine::source_media::read_any(Path::new(&path)) {
            Ok(image) => Ok(Picked {
                width: image.width,
                height: image.height,
                format: format_name(&path),
                thumb: thumbnail(&image, THUMB_SIDE),
            }),
            Err(e) => Err(unreadable(&e)),
        };
        Self { path, read }
    }

    pub fn file_name(&self) -> String {
        Path::new(&self.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.clone())
    }

    pub fn is_ready(&self) -> bool {
        self.read.is_ok()
    }
}

/// Say what went wrong in words a person can act on; keep the precise message for the tooltip.
fn unreadable(e: &CommandError) -> Unreadable {
    let reason = match e {
        CommandError::Codec(CodecError::Io(_)) | CommandError::Io(_) => {
            "The file couldn’t be opened."
        }
        CommandError::Codec(CodecError::BadSize { .. }) => "The image is too large or empty.",
        CommandError::Codec(_) => "The file may be damaged or in an unsupported format.",
        _ => "The file can’t be used as an image.",
    };
    Unreadable {
        reason: reason.to_string(),
        detail: e.to_string(),
    }
}

/// "PNG" for `sun.png`; empty when there is no extension.
pub fn format_name(path: &str) -> String {
    let ext = Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_uppercase())
        .unwrap_or_default();
    match ext.as_str() {
        "JPG" => "JPEG".to_string(),
        "TIF" => "TIFF".to_string(),
        _ => ext,
    }
}

/// Sample `image` down by a whole factor so its longer side is at most `max_side`.
pub fn thumbnail(image: &Rgba8Image, max_side: usize) -> ColorImage {
    let (w, h) = (image.width as usize, image.height as usize);
    let step = w.max(h).div_ceil(max_side.max(1)).max(1);
    let (ow, oh) = (w.div_ceil(step).max(1), h.div_ceil(step).max(1));
    let mut rgba = Vec::with_capacity(ow.saturating_mul(oh).saturating_mul(4));
    for y in 0..oh {
        for x in 0..ow {
            let i = ((y * step) * w + x * step) * 4;
            match image.data.get(i..i + 4) {
                Some(p) => rgba.extend_from_slice(p),
                None => rgba.extend([0, 0, 0, 0]),
            }
        }
    }
    ColorImage::from_rgba_unmultiplied([ow, oh], &rgba)
}

fn stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The dialog. `slots[0]` is the start image (Image 1), `slots[1]` the end image (Image 2).
#[derive(Clone, Debug, Default)]
pub struct NewTwo {
    pub slots: [Option<Slot>; 2],
    pub name: String,
    /// The last problem from running the command.
    pub error: Option<String>,
}

impl NewTwo {
    /// Put a file in slot 0 (start) or 1 (end), reading it now.
    pub fn set(&mut self, slot: usize, path: impl Into<String>) {
        if let Some(s) = self.slots.get_mut(slot) {
            *s = Some(Slot::load(path.into()));
            self.error = None;
        }
    }

    pub fn clear(&mut self, slot: usize) {
        if let Some(s) = self.slots.get_mut(slot) {
            *s = None;
            self.error = None;
        }
    }

    pub fn swap(&mut self) {
        self.slots.swap(0, 1);
        self.error = None;
    }

    /// Put dropped files where they belong: on the slot they were dropped on, else into empty
    /// slots in order (two files fill both).
    pub fn drop_files(&mut self, paths: Vec<String>, over: Option<usize>) {
        let mut paths = paths.into_iter();
        if let Some(slot) = over.filter(|&s| s < 2) {
            if let Some(p) = paths.next() {
                self.set(slot, p);
            }
            if let Some(p) = paths.next() {
                self.set(1 - slot, p);
            }
            return;
        }
        let empty: Vec<usize> = (0..2).filter(|&i| self.slots[i].is_none()).collect();
        let targets = if empty.is_empty() { vec![0, 1] } else { empty };
        for (slot, path) in targets.into_iter().zip(paths) {
            self.set(slot, path);
        }
    }

    /// Both slots hold readable images.
    pub fn ready(&self) -> bool {
        self.slots
            .iter()
            .all(|s| s.as_ref().is_some_and(Slot::is_ready))
    }

    /// The name the command gives the sequence when none is typed, once both images are ready.
    pub fn default_name(&self) -> Option<String> {
        match &self.slots {
            [Some(a), Some(b)] if self.ready() => {
                Some(format!("{} to {}", stem(&a.path), stem(&b.path)))
            }
            _ => None,
        }
    }

    /// The project's size: the start image's.
    pub fn output_size(&self) -> Option<(u32, u32)> {
        match &self.slots[0] {
            Some(Slot { read: Ok(p), .. }) => Some((p.width, p.height)),
            _ => None,
        }
    }

    /// The parameters for `project.new`, once both images are ready.
    pub fn to_params(&self) -> Option<Value> {
        if !self.ready() {
            return None;
        }
        let [Some(a), Some(b)] = &self.slots else {
            return None;
        };
        let mut object = Map::new();
        object.insert("image1".into(), json!(a.path));
        object.insert("image2".into(), json!(b.path));
        let name = self.name.trim();
        if !name.is_empty() {
            object.insert("name".into(), json!(name));
        }
        Some(Value::Object(object))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo(name: &str) -> String {
        format!("{}/../../demo/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn picking_both_images_makes_the_command_parameters() {
        let mut d = NewTwo::default();
        assert!(!d.ready());
        assert!(d.to_params().is_none());
        d.set(0, demo("sun.png"));
        assert!(!d.ready(), "one image is not enough");
        let (w, h) = d.output_size().unwrap();
        assert!(w > 0 && h > 0);
        d.set(1, demo("moon.png"));
        assert!(d.ready());
        assert_eq!(d.default_name().as_deref(), Some("sun to moon"));
        let p = d.to_params().unwrap();
        assert_eq!(p["image1"], demo("sun.png"));
        assert_eq!(p["image2"], demo("moon.png"));
        assert!(
            p.get("name").is_none(),
            "an empty name lets the command choose"
        );
        d.name = "  Day and night ".into();
        assert_eq!(d.to_params().unwrap()["name"], "Day and night");
    }

    #[test]
    fn the_parameters_pass_the_engine_schema() {
        let mut d = NewTwo::default();
        d.set(0, demo("sun.png"));
        d.set(1, demo("moon.png"));
        d.name = "x".into();
        let registry = inbetween_engine::Registry::standard();
        let spec = registry.get("project.new").unwrap();
        assert!(inbetween_engine::Params::parse(spec.params, &d.to_params().unwrap()).is_ok());
    }

    #[test]
    fn swapping_exchanges_start_and_end() {
        let mut d = NewTwo::default();
        d.set(0, demo("sun.png"));
        d.set(1, demo("moon.png"));
        d.swap();
        assert_eq!(d.default_name().as_deref(), Some("moon to sun"));
        d.clear(0);
        assert!(!d.ready());
        assert!(d.output_size().is_none());
    }

    #[test]
    fn unreadable_files_say_why_and_block_create() {
        let dir = std::env::temp_dir().join(format!("inbetween-newtwo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let junk = dir.join("scan.psd");
        std::fs::write(&junk, b"not an image at all").unwrap();
        let mut d = NewTwo::default();
        d.set(0, demo("sun.png"));
        d.set(1, junk.display().to_string());
        let err = d.slots[1].as_ref().unwrap().read.as_ref().unwrap_err();
        assert!(err.reason.contains("damaged"), "{err:?}");
        assert!(!err.detail.is_empty());
        assert!(!d.ready());
        assert!(
            d.default_name().is_none(),
            "no name is promised for a file that can't be used"
        );
        assert!(d.to_params().is_none());

        d.set(1, dir.join("missing.png").display().to_string());
        let err = d.slots[1].as_ref().unwrap().read.as_ref().unwrap_err();
        assert!(err.reason.contains("opened"), "{err:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dropped_files_fill_the_slot_under_the_pointer_or_the_empty_ones() {
        let (sun, moon) = (demo("sun.png"), demo("moon.png"));
        let mut d = NewTwo::default();
        d.drop_files(vec![sun.clone(), moon.clone()], None);
        assert_eq!(d.default_name().as_deref(), Some("sun to moon"));

        let mut d = NewTwo::default();
        d.drop_files(vec![moon.clone()], Some(1));
        assert!(d.slots[0].is_none());
        d.drop_files(vec![sun.clone()], None);
        assert_eq!(d.default_name().as_deref(), Some("sun to moon"));

        // Both full and dropped on nothing: replace in order.
        d.drop_files(vec![moon.clone(), sun.clone()], None);
        assert_eq!(d.default_name().as_deref(), Some("moon to sun"));
        // Dropped on the end slot with two files: the second goes to the other slot.
        d.drop_files(vec![moon, sun], Some(1));
        assert_eq!(d.default_name().as_deref(), Some("sun to moon"));
        d.drop_files(Vec::new(), Some(7));
    }

    #[test]
    fn thumbnails_fit_the_limit_and_survive_tiny_images() {
        let big = Rgba8Image {
            width: 1000,
            height: 10,
            data: vec![255; 1000 * 10 * 4],
        };
        let t = thumbnail(&big, THUMB_SIDE);
        assert!(t.size[0] <= THUMB_SIDE && t.size[1] >= 1);
        let tiny = Rgba8Image {
            width: 1,
            height: 1,
            data: vec![9, 9, 9, 255],
        };
        assert_eq!(thumbnail(&tiny, THUMB_SIDE).size, [1, 1]);
        let short = Rgba8Image {
            width: 4,
            height: 4,
            data: vec![0; 3],
        };
        assert_eq!(thumbnail(&short, 2).size, [2, 2]);
    }

    #[test]
    fn format_names_read_well() {
        assert_eq!(format_name("a/b.jpg"), "JPEG");
        assert_eq!(format_name("b.Tif"), "TIFF");
        assert_eq!(format_name("b.png"), "PNG");
        assert_eq!(format_name("noext"), "");
    }
}
