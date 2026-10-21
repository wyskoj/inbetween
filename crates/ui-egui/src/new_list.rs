//! The New From Image List dialog's state: an ordered list of images, whether the chain loops
//! back to the first, and the settings every sequence shares. It builds the parameters for
//! `file.wizard`, so the CLI and MCP keep using the same command. Each image is read when it is
//! added, for its thumbnail, its size and an early, specific error.

use crate::new_two::Slot;
use inbetween_doc::MorphType;
use serde_json::{json, Map, Value};

/// Longest side of a row thumbnail, in pixels.
pub const ROW_THUMB_SIDE: usize = 96;

/// Frames per morph when none is typed (the command's default).
pub const DEFAULT_FRAMES: u32 = 30;

/// The most images the command takes.
pub const MAX_IMAGES: usize = 1000;

/// The morph types offered, with their names in the dialog.
pub const MORPHS: [(MorphType, &str); 4] = [
    (MorphType::Morph, "Morph"),
    (MorphType::MorphHold, "Morph and hold"),
    (MorphType::Warp1, "Warp start"),
    (MorphType::Warp2, "Warp end"),
];

/// How the sequences play: one after another, or stacked as layers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayAs {
    #[default]
    Sequence,
    Layers,
}

impl PlayAs {
    fn id(self) -> &'static str {
        match self {
            Self::Sequence => "sequence",
            Self::Layers => "layer",
        }
    }
}

/// Why Create project is not available yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Blocker {
    /// Fewer than two images.
    TooFew,
    /// More than the command takes.
    TooMany,
    /// This image (by file name) could not be read.
    Unreadable(String),
    /// A number field holds something that is not a usable number.
    BadNumber(String),
}

impl Blocker {
    /// What to do about it, in a sentence.
    pub fn message(&self) -> String {
        match self {
            Self::TooFew => "Add at least two images. This replaces the open project.".into(),
            Self::TooMany => format!("Remove images to bring the list to {MAX_IMAGES} or fewer."),
            Self::Unreadable(name) => format!("Replace or remove {name} to continue."),
            Self::BadNumber(m) => m.clone(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct NewList {
    pub images: Vec<Slot>,
    pub looped: bool,
    pub play_as: PlayAs,
    pub morph: MorphType,
    /// Typed text; empty means the default.
    pub frames: String,
    pub width: String,
    pub height: String,
    /// The last problem from running the command.
    pub error: Option<String>,
}

/// An optional whole number in `1..=max`. Empty is `Ok(None)`.
fn parse_count(text: &str, what: &str, min: u32, max: u32) -> Result<Option<u32>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    match text.parse::<u32>() {
        Ok(n) if (min..=max).contains(&n) => Ok(Some(n)),
        _ => Err(format!(
            "{what} must be a whole number from {min} to {max}."
        )),
    }
}

impl NewList {
    /// A dialog that starts with these images (a drop of several files).
    pub fn with_images(paths: Vec<String>) -> Self {
        let mut d = Self::default();
        d.add(paths);
        d
    }

    /// Add images at the end, reading each now.
    pub fn add(&mut self, paths: Vec<String>) {
        self.images.extend(
            paths
                .into_iter()
                .map(|p| Slot::load_sized(p, ROW_THUMB_SIDE)),
        );
        self.error = None;
    }

    /// Put a different file in place of image `index`.
    pub fn replace(&mut self, index: usize, path: impl Into<String>) {
        if let Some(s) = self.images.get_mut(index) {
            *s = Slot::load_sized(path.into(), ROW_THUMB_SIDE);
            self.error = None;
        }
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.images.len() {
            self.images.remove(index);
            self.error = None;
        }
    }

    /// Move image `from` so it ends up at position `to` (both clamped to the list).
    pub fn move_to(&mut self, from: usize, to: usize) {
        if from >= self.images.len() {
            return;
        }
        let to = to.min(self.images.len() - 1);
        if from != to {
            let item = self.images.remove(from);
            self.images.insert(to, item);
            self.error = None;
        }
    }

    pub fn move_up(&mut self, index: usize) {
        if index > 0 {
            self.move_to(index, index - 1);
        }
    }

    pub fn move_down(&mut self, index: usize) {
        self.move_to(index, index.saturating_add(1));
    }

    /// The number of sequences the command will build.
    pub fn sequence_count(&self) -> usize {
        match self.images.len() {
            0 | 1 => 0,
            n => n - 1 + usize::from(self.looped),
        }
    }

    /// The project's size: typed, or else the first image's.
    pub fn output_size(&self) -> Option<(u32, u32)> {
        let first = self.images.first().and_then(|s| s.read.as_ref().ok());
        let typed = |text: &str| {
            parse_count(text, "", 1, inbetween_doc::MAX_SIDE)
                .ok()
                .flatten()
        };
        let w = typed(&self.width).or(first.map(|p| p.width))?;
        let h = typed(&self.height).or(first.map(|p| p.height))?;
        Some((w, h))
    }

    /// The first image's size, the default the size fields show.
    pub fn first_size(&self) -> Option<(u32, u32)> {
        self.images
            .first()
            .and_then(|s| s.read.as_ref().ok())
            .map(|p| (p.width, p.height))
    }

    pub fn frames_per_morph(&self) -> u32 {
        parse_count(&self.frames, "", 2, 999)
            .ok()
            .flatten()
            .unwrap_or(DEFAULT_FRAMES)
    }

    /// Frames in the finished movie: sequences in a row add up; layers play together.
    pub fn total_frames(&self) -> u64 {
        let per = u64::from(self.frames_per_morph());
        match (self.play_as, self.sequence_count()) {
            (_, 0) => 0,
            (PlayAs::Layers, _) => per,
            (PlayAs::Sequence, n) => per.saturating_mul(n as u64),
        }
    }

    /// The first thing that stops Create project, if any.
    pub fn blocker(&self) -> Option<Blocker> {
        if let Some(bad) = self.images.iter().find(|s| !s.is_ready()) {
            return Some(Blocker::Unreadable(bad.file_name()));
        }
        if self.images.len() < 2 {
            return Some(Blocker::TooFew);
        }
        if self.images.len() > MAX_IMAGES {
            return Some(Blocker::TooMany);
        }
        let side = inbetween_doc::MAX_SIDE;
        let checks = [
            parse_count(&self.frames, "Frames per morph", 2, 999),
            parse_count(&self.width, "Width", 1, side),
            parse_count(&self.height, "Height", 1, side),
        ];
        checks
            .into_iter()
            .find_map(Result::err)
            .map(Blocker::BadNumber)
    }

    pub fn ready(&self) -> bool {
        self.blocker().is_none()
    }

    /// The parameters for `file.wizard`, once everything is usable. Defaults are left out so
    /// the command chooses them.
    pub fn to_params(&self) -> Option<Value> {
        if !self.ready() {
            return None;
        }
        let mut o = Map::new();
        let paths: Vec<&str> = self.images.iter().map(|s| s.path.as_str()).collect();
        o.insert("images".into(), json!(paths));
        if self.play_as != PlayAs::Sequence {
            o.insert("kind".into(), json!(self.play_as.id()));
        }
        if self.looped {
            o.insert("loop".into(), json!(true));
        }
        if self.morph != MorphType::Morph {
            o.insert("morph".into(), json!(self.morph.id()));
        }
        let side = inbetween_doc::MAX_SIDE;
        for (key, text, min, max) in [
            ("frames", &self.frames, 2, 999),
            ("width", &self.width, 1, side),
            ("height", &self.height, 1, side),
        ] {
            if let Ok(Some(n)) = parse_count(text, key, min, max) {
                o.insert(key.into(), json!(n));
            }
        }
        Some(Value::Object(o))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo(name: &str) -> String {
        format!("{}/../../demo/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    fn three() -> NewList {
        NewList::with_images(vec![demo("sun.png"), demo("moon.png"), demo("sun.png")])
    }

    #[test]
    fn two_or_more_readable_images_make_the_command_parameters() {
        let mut d = NewList::default();
        assert_eq!(d.blocker(), Some(Blocker::TooFew));
        d.add(vec![demo("sun.png")]);
        assert!(!d.ready(), "one image is not enough");
        d.add(vec![demo("moon.png")]);
        assert!(d.ready());
        let p = d.to_params().unwrap();
        assert_eq!(p["images"], json!([demo("sun.png"), demo("moon.png")]));
        for key in ["kind", "loop", "morph", "frames", "width", "height"] {
            assert!(p.get(key).is_none(), "{key}: defaults are the command's");
        }
    }

    #[test]
    fn chosen_settings_reach_the_parameters_and_pass_the_schema() {
        let mut d = three();
        d.looped = true;
        d.play_as = PlayAs::Layers;
        d.morph = MorphType::Warp2;
        d.frames = " 48 ".into();
        d.width = "640".into();
        let p = d.to_params().unwrap();
        assert_eq!(p["kind"], "layer");
        assert_eq!(p["loop"], true);
        assert_eq!(p["morph"], "warp2");
        assert_eq!(p["frames"], 48);
        assert_eq!(p["width"], 640);
        assert!(p.get("height").is_none());
        let registry = inbetween_engine::Registry::standard();
        let spec = registry.get("file.wizard").unwrap();
        assert!(inbetween_engine::Params::parse(spec.params, &p).is_ok());
        for (m, _) in MORPHS {
            assert!(spec
                .params
                .iter()
                .any(|s| s.name == "morph" && s.choices.contains(&m.id())));
        }
    }

    #[test]
    fn counts_follow_the_list_and_the_loop() {
        let mut d = three();
        assert_eq!(d.sequence_count(), 2);
        assert_eq!(d.total_frames(), 60);
        d.looped = true;
        assert_eq!(d.sequence_count(), 3);
        d.frames = "10".into();
        assert_eq!(d.total_frames(), 30);
        d.play_as = PlayAs::Layers;
        assert_eq!(d.total_frames(), 10, "layers play together");
        d.remove(0);
        assert_eq!(d.sequence_count(), 2, "a looped pair goes there and back");
        d.remove(0);
        assert_eq!((d.sequence_count(), d.total_frames()), (0, 0));
    }

    #[test]
    fn reordering_moves_one_image_and_ignores_bad_indices() {
        let mut d = NewList::with_images(vec![demo("sun.png"), demo("moon.png")]);
        d.move_down(0);
        assert!(d.images[0].path.ends_with("moon.png"));
        d.move_up(1);
        assert!(d.images[0].path.ends_with("sun.png"));
        d.move_up(0);
        d.move_down(1);
        d.move_down(usize::MAX);
        d.move_to(9, 0);
        d.move_to(0, 99);
        assert!(d.images[1].path.ends_with("sun.png"), "clamped to the end");
        d.remove(5);
        d.replace(5, demo("sun.png"));
        assert_eq!(d.images.len(), 2);
        assert_eq!(NewList::default().sequence_count(), 0);
        NewList::default().move_to(0, 0);
    }

    #[test]
    fn an_unreadable_image_names_itself_and_blocks_create() {
        let dir = std::env::temp_dir().join(format!("inbetween-newlist-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let junk = dir.join("scan-003.psd");
        std::fs::write(&junk, b"not an image").unwrap();
        let mut d = three();
        d.replace(1, junk.display().to_string());
        assert_eq!(
            d.blocker(),
            Some(Blocker::Unreadable("scan-003.psd".into()))
        );
        assert!(d.blocker().unwrap().message().contains("scan-003.psd"));
        assert!(d.to_params().is_none());
        d.replace(1, demo("moon.png"));
        assert!(d.ready());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bad_numbers_block_create_with_a_sentence() {
        let mut d = three();
        for bad in ["1", "abc", "-4", "1000", "99999999999999"] {
            d.frames = bad.into();
            match d.blocker() {
                Some(Blocker::BadNumber(m)) => assert!(m.contains("Frames per morph"), "{m}"),
                other => panic!("{bad}: {other:?}"),
            }
        }
        d.frames.clear();
        d.height = "0".into();
        assert!(matches!(d.blocker(), Some(Blocker::BadNumber(m)) if m.contains("Height")));
        d.height = (inbetween_doc::MAX_SIDE + 1).to_string();
        assert!(!d.ready());
        d.height = "300".into();
        assert!(d.ready());
        let (w, _) = d.first_size().unwrap();
        assert_eq!(d.output_size(), Some((w, 300)));
    }

    #[test]
    fn too_many_images_are_refused_before_the_command_runs() {
        let mut d = NewList {
            images: vec![Slot::load_sized(demo("sun.png"), 4); MAX_IMAGES + 1],
            ..NewList::default()
        };
        assert_eq!(d.blocker(), Some(Blocker::TooMany));
        d.remove(0);
        assert!(d.ready());
    }
}
