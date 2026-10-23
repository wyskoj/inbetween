//! The Export dialogs' state: one struct for every export command, with the fields each kind
//! shows. It builds the same parameters the CLI and MCP send, checks them as they are typed
//! (so the Export button stays disabled until the command can succeed), and works out the
//! footer note ("96 frames · 1080 × 720 px"). Drawing lives in `app/export_ui.rs`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::{Component, Path, PathBuf};

/// Which export a dialog is for. Each maps to one engine command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// One frame as a picture (`export.frame`).
    Frame,
    /// One frame and its dots (`frame.exportEdit`).
    FrameDots,
    /// The whole movie as numbered pictures (`export.movie`).
    MovieImages,
    /// One sequence as numbered pictures (`export.sequence`).
    SequenceImages,
    /// GIF, APNG or WebP (`export.animation`).
    Animation,
    /// MP4, WebM or ProRes (`export.video`).
    Video,
    /// A PDF of pictures (`export.pdf`).
    Sheet,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Frame,
        Kind::FrameDots,
        Kind::MovieImages,
        Kind::SequenceImages,
        Kind::Animation,
        Kind::Video,
        Kind::Sheet,
    ];

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }

    /// The command this dialog runs.
    pub fn id(self) -> &'static str {
        match self {
            Kind::Frame => "export.frame",
            Kind::FrameDots => "frame.exportEdit",
            Kind::MovieImages => "export.movie",
            Kind::SequenceImages => "export.sequence",
            Kind::Animation => "export.animation",
            Kind::Video => "export.video",
            Kind::Sheet => "export.pdf",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Kind::Frame => "Export Image",
            Kind::FrameDots => "Export Image With Dots",
            Kind::MovieImages => "Export Image Sequence",
            Kind::SequenceImages => "Export This Sequence",
            Kind::Animation => "Export Animated Image",
            Kind::Video => "Export Video",
            Kind::Sheet => "Export Contact Sheet",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Kind::Frame => "Save one frame of the morph as a picture.",
            Kind::FrameDots => "Save a frame and its dots, so the frame can start a new sequence.",
            Kind::MovieImages => "Save every frame of the movie as a numbered PNG.",
            Kind::SequenceImages => "Save every frame of one sequence as a numbered PNG.",
            Kind::Animation => "Save the movie as a GIF, APNG or WebP.",
            Kind::Video => "Save the movie as MP4, WebM or ProRes.",
            Kind::Sheet => "Save a PDF of pictures from the movie, with a caption under each.",
        }
    }

    /// Writes into a folder (numbered files) rather than to one file.
    pub fn is_folder(self) -> bool {
        matches!(self, Kind::MovieImages | Kind::SequenceImages)
    }

    /// Needs one sequence picked (the others default to the whole movie).
    pub fn needs_sequence(self) -> bool {
        matches!(self, Kind::Frame | Kind::FrameDots | Kind::SequenceImages)
    }

    /// Has a first and last frame to choose.
    pub fn has_range(self) -> bool {
        matches!(
            self,
            Kind::MovieImages | Kind::SequenceImages | Kind::Animation | Kind::Video
        )
    }

    pub fn has_direction_options(self) -> bool {
        matches!(self, Kind::MovieImages | Kind::Animation | Kind::Video)
    }

    pub fn has_dpi(self) -> bool {
        matches!(self, Kind::Frame | Kind::MovieImages | Kind::SequenceImages)
    }

    pub fn has_after_options(self) -> bool {
        self != Kind::FrameDots
    }

    pub fn is_movie_video(self) -> bool {
        matches!(self, Kind::Animation | Kind::Video)
    }
}

/// How a single frame is named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameBy {
    /// A point between the images, 0 to 1.
    Position,
    /// A frame of one sequence, counting from 1.
    Frame,
    /// A frame of the whole movie, counting from 1.
    Movie,
}

/// What a contact sheet shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Content {
    Sources,
    Preview,
    Frames,
}

impl Content {
    fn id(self) -> &'static str {
        match self {
            Content::Sources => "sources",
            Content::Preview => "preview",
            Content::Frames => "frames",
        }
    }
}

/// What the dialog needs to know about the open project.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    /// Each sequence's name and frame count.
    pub sequences: Vec<(String, u32)>,
    pub total_frames: u64,
    pub size: (u32, u32),
    pub fps: f64,
    pub has_sound: bool,
    /// The sequence the panels are showing.
    pub current: usize,
    /// The frame the preview is on, counting from 0 within that sequence.
    pub frame: u32,
    /// The project file's name without its extension; empty when unsaved.
    pub stem: String,
    /// The folder the project file is in; empty when unsaved.
    pub dir: String,
}

#[derive(Clone, Debug)]
pub struct ExportDialog {
    pub kind: Kind,
    /// The file to write, or the folder for numbered files. Built from `dir`, `stem` and `ext`;
    /// empty until both a folder and a name are known.
    pub path: String,
    /// The folder the file goes in (for numbered files, the folder that holds the new one).
    pub dir: String,
    /// The file's name without its ending (for numbered files, the new folder's name).
    pub stem: String,
    /// The file's ending without the dot; empty for numbered files.
    pub ext: String,
    /// The dots file ([`Kind::FrameDots`]); empty means beside the picture.
    pub dots_path: String,
    pub prefix: String,
    pub frame_by: FrameBy,
    /// 0 to 1.
    pub position: f32,
    pub frame: String,
    pub movie_frame: String,
    /// `None` is the whole movie.
    pub sequence: Option<usize>,
    pub all_frames: bool,
    pub first: String,
    pub last: String,
    /// `gif`, `apng`, `webp`, or `mp4`, `webm`, `prores`.
    pub format: String,
    pub fps: String,
    /// The movie's speed, which an empty or equal `fps` means.
    pub default_fps: f64,
    pub repeat: bool,
    pub transparent: bool,
    pub audio: bool,
    pub matte: String,
    pub reverse: bool,
    pub drop_duplicates: bool,
    pub jpeg_quality: u8,
    pub dpi: String,
    pub ffmpeg: String,
    /// Whether the video encoder could be started; set by the window.
    pub ffmpeg_found: bool,
    pub open_after: bool,
    pub save_first: bool,
    pub content: Content,
    pub per_sequence: String,
    pub columns: String,
    pub letter: bool,
    pub landscape: bool,
    pub captions: bool,
    pub title: String,
    /// The heading used when `title` is left alone: the project's name.
    pub default_title: String,
    pub thumb: String,
    pub sheet_quality: String,
    /// "More options" is open.
    pub more: bool,
    /// The last problem from running the command.
    pub error: Option<String>,
}

const PAGE_PICTURES: usize = 12;

const DEFAULT_PREFIX: &str = "frame";
const DEFAULT_MATTE: &str = "#ffffff";
const DEFAULT_DPI: u64 = 72;
const DEFAULT_PER_SEQUENCE: u64 = 5;
const DEFAULT_COLUMNS: u64 = 3;
const DEFAULT_THUMB: u64 = 360;
const DEFAULT_SHEET_QUALITY: u64 = 85;

impl ExportDialog {
    pub fn new(kind: Kind, facts: &Facts) -> Self {
        let mut d = Self {
            kind,
            path: String::new(),
            dir: String::new(),
            stem: String::new(),
            ext: String::new(),
            dots_path: String::new(),
            prefix: DEFAULT_PREFIX.into(),
            frame_by: if kind == Kind::Frame {
                FrameBy::Frame
            } else {
                FrameBy::Position
            },
            position: 0.5,
            frame: String::new(),
            movie_frame: String::new(),
            sequence: kind.needs_sequence().then_some(facts.current),
            all_frames: true,
            first: String::new(),
            last: String::new(),
            format: match kind {
                Kind::Video => "mp4".into(),
                _ => "gif".into(),
            },
            fps: number_text(facts.fps),
            default_fps: facts.fps,
            repeat: true,
            transparent: true,
            audio: true,
            matte: DEFAULT_MATTE.into(),
            reverse: false,
            drop_duplicates: false,
            jpeg_quality: 92,
            dpi: DEFAULT_DPI.to_string(),
            ffmpeg: String::new(),
            ffmpeg_found: true,
            open_after: false,
            save_first: false,
            content: Content::Preview,
            per_sequence: DEFAULT_PER_SEQUENCE.to_string(),
            columns: DEFAULT_COLUMNS.to_string(),
            letter: false,
            landscape: false,
            captions: true,
            title: facts.stem.clone(),
            default_title: facts.stem.clone(),
            thumb: DEFAULT_THUMB.to_string(),
            sheet_quality: DEFAULT_SHEET_QUALITY.to_string(),
            more: false,
            error: None,
        };
        d.ext = d.default_extension().to_string();
        if !facts.dir.is_empty() {
            d.dir = normalise(&facts.dir);
            let stem = if facts.stem.is_empty() {
                "movie"
            } else {
                &facts.stem
            };
            d.stem = if kind.is_folder() {
                format!("{stem} frames")
            } else {
                stem.to_string()
            };
            d.rebuild();
        }
        // Open on the frame the preview is showing.
        if matches!(kind, Kind::Frame | Kind::FrameDots) {
            let count = d.scope_frames(facts).min(u64::from(u32::MAX)) as u32;
            d.set_frame(facts.frame.saturating_add(1), count);
        }
        d
    }

    /// Pick frame `n` (counting from 1) of `count`: the position follows, so every kind that
    /// takes a point between the images sends the same place.
    pub fn set_frame(&mut self, n: u32, count: u32) {
        let n = n.clamp(1, count.max(1));
        self.frame = n.to_string();
        self.position = if count > 1 {
            (n - 1) as f32 / (count - 1) as f32
        } else {
            0.0
        };
        if self.kind == Kind::Frame {
            self.frame_by = FrameBy::Frame;
        }
        self.error = None;
    }

    /// The frame picked, counting from 1; `None` when the box holds something else.
    pub fn picked_frame(&self) -> Option<u32> {
        parse_whole(&self.frame).and_then(|n| u32::try_from(n).ok())
    }

    /// Put `path` together from the folder, the name and the ending.
    fn rebuild(&mut self) {
        self.path = if self.dir.trim().is_empty() || self.stem.trim().is_empty() {
            String::new()
        } else {
            let name = if self.ext.is_empty() {
                self.stem.trim().to_string()
            } else {
                format!("{}.{}", self.stem.trim(), self.ext)
            };
            Path::new(&self.dir).join(name).display().to_string()
        };
    }

    /// Change the file's name (without its ending).
    pub fn set_stem(&mut self, stem: impl Into<String>) {
        self.stem = stem.into();
        self.error = None;
        self.rebuild();
    }

    /// Change the folder.
    pub fn set_dir(&mut self, dir: impl AsRef<str>) {
        self.dir = normalise(dir.as_ref());
        self.error = None;
        self.rebuild();
    }

    /// Whether the first and last frame can be chosen: the movie-wide kinds, and a contact
    /// sheet of every frame.
    pub fn shows_range(&self) -> bool {
        self.kind.has_range() || (self.kind == Kind::Sheet && self.content == Content::Frames)
    }

    /// The extension a file kind writes by default.
    fn default_extension(&self) -> &'static str {
        match self.kind {
            Kind::Frame | Kind::FrameDots => "png",
            Kind::Animation => extension_of_format(&self.format),
            Kind::Video => extension_of_format(&self.format),
            Kind::Sheet => "pdf",
            Kind::MovieImages | Kind::SequenceImages => "",
        }
    }

    /// Set the whole path (typed or browsed). A video or animation follows the extension it is
    /// given.
    pub fn set_path(&mut self, path: impl Into<String>) {
        let path = normalise(&path.into());
        self.error = None;
        let p = Path::new(path.trim());
        self.dir = p
            .parent()
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        if self.kind.is_folder() {
            self.stem = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.ext.clear();
        } else {
            self.stem = p
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.ext = p
                .extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        self.rebuild();
        if self.path.is_empty() {
            self.path = path.trim().to_string();
        }
        if self.kind.is_movie_video() {
            if let Some(f) = format_of_extension(self.kind, extension(&self.path)) {
                self.format = f.to_string();
            }
        }
    }

    /// Pick a format; the file name's extension changes with it.
    pub fn set_format(&mut self, format: &str) {
        self.format = format.to_string();
        self.error = None;
        self.ext = extension_of_format(format).to_string();
        self.rebuild();
    }

    /// Pick PNG or JPEG for a picture.
    pub fn set_picture_format(&mut self, jpeg: bool) {
        self.ext = if jpeg { "jpg" } else { "png" }.to_string();
        self.error = None;
        self.rebuild();
    }

    /// The ending shown after the name, with its dot; empty for numbered files.
    pub fn ext_suffix(&self) -> String {
        if self.ext.is_empty() {
            String::new()
        } else {
            format!(".{}", self.ext)
        }
    }

    /// The folder being written to, when it does not exist.
    pub fn missing_folder(&self) -> Option<PathBuf> {
        let dir = self.dir.trim();
        if dir.is_empty() || Path::new(dir).is_dir() {
            None
        } else {
            Some(PathBuf::from(dir))
        }
    }

    /// What the export would write over, as a sentence; `None` when nothing is in the way.
    pub fn replaces(&self) -> Option<String> {
        if self.path.trim().is_empty() || self.missing_folder().is_some() {
            return None;
        }
        let target = Path::new(self.path.trim());
        if self.kind.is_folder() {
            let prefix = match self.prefix.trim() {
                "" => DEFAULT_PREFIX,
                p => p,
            };
            let n = std::fs::read_dir(target)
                .ok()?
                .flatten()
                .filter(|e| numbered_name(&e.file_name().to_string_lossy(), prefix))
                .count();
            return (n > 0).then(|| {
                format!(
                    "{} named {prefix}_####.png will be replaced.",
                    if n == 1 {
                        "1 file".to_string()
                    } else {
                        format!("{n} files")
                    }
                )
            });
        }
        target.is_file().then(|| {
            format!(
                "{} already exists. Export replaces it.",
                target
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            )
        })
    }

    /// What the Export button says: the result, or Replace when it would write over something.
    pub fn action_label(&self, facts: &Facts) -> String {
        let replacing = self.replaces().is_some();
        match self.kind {
            Kind::MovieImages | Kind::SequenceImages => {
                let verb = if replacing { "Replace" } else { "Export" };
                format!("{verb} {}", frames_text(self.frame_count(facts)))
            }
            _ if replacing => "Replace".to_string(),
            Kind::FrameDots => "Export 2 files".to_string(),
            _ => format!("Export {}", self.format_name()),
        }
    }

    /// The window's second line, naming the sequence when the project has just one.
    pub fn subtitle(&self, facts: &Facts) -> String {
        let name = match facts.sequences.as_slice() {
            [(name, _)] if !name.trim().is_empty() => Some(name.trim()),
            _ => None,
        };
        match (self.kind, name) {
            (Kind::Frame, Some(n)) => format!("Save one frame of “{n}” as a picture."),
            (Kind::SequenceImages, Some(n)) => {
                format!("Save every frame of “{n}” as a numbered PNG.")
            }
            _ => self.kind.subtitle().to_string(),
        }
    }

    /// `jpg` when the picture is a JPEG.
    pub fn is_jpeg(&self) -> bool {
        self.kind == Kind::Frame && matches!(extension(&self.path).as_str(), "jpg" | "jpeg")
    }

    /// The format the picture or movie will be in, for the footer.
    pub fn format_name(&self) -> &'static str {
        match self.kind {
            Kind::Frame => {
                if self.is_jpeg() {
                    "JPEG"
                } else {
                    "PNG"
                }
            }
            Kind::MovieImages | Kind::SequenceImages => "PNG",
            Kind::Animation | Kind::Video => match self.format.as_str() {
                "gif" => "GIF",
                "apng" => "APNG",
                "webp" => "WebP",
                "webm" => "WebM",
                "prores" => "ProRes",
                _ => "MP4",
            },
            Kind::Sheet => "PDF",
            Kind::FrameDots => "PNG",
        }
    }

    /// Whether transparent areas get filled with the matte colour.
    pub fn uses_matte(&self) -> bool {
        match self.kind {
            Kind::Animation => !self.transparent,
            Kind::Video => self.format == "mp4",
            _ => false,
        }
    }

    /// Frames in the chosen scope (the sequence, or the whole movie).
    pub fn scope_frames(&self, facts: &Facts) -> u64 {
        match self.sequence {
            Some(i) => facts.sequences.get(i).map_or(0, |s| u64::from(s.1)),
            None => facts.total_frames,
        }
    }

    /// Frames the export will write, after the range.
    pub fn frame_count(&self, facts: &Facts) -> u64 {
        let all = self.scope_frames(facts);
        if self.all_frames || !self.shows_range() {
            return all;
        }
        let first = parse_whole(&self.first).unwrap_or(1).max(1);
        let last = parse_whole(&self.last).unwrap_or(all).min(all);
        if last < first {
            0
        } else {
            last - first + 1
        }
    }

    /// How many pictures a contact sheet will hold.
    pub fn picture_total(&self, facts: &Facts) -> u64 {
        self.picture_count(facts)
    }

    /// The movie frame (counting from 1) a single-frame export is on, for its picture.
    pub fn movie_frame_shown(&self, facts: &Facts) -> Option<u64> {
        if self.frame_by == FrameBy::Movie && self.kind == Kind::Frame {
            return parse_whole(&self.movie_frame).filter(|n| (1..=facts.total_frames).contains(n));
        }
        let index = self.sequence.unwrap_or(facts.current);
        let count = u64::from(facts.sequences.get(index)?.1);
        if count == 0 {
            return None;
        }
        let before: u64 = facts.sequences[..index]
            .iter()
            .map(|s| u64::from(s.1))
            .sum();
        let k = match self.frame_by {
            FrameBy::Frame => u64::from(self.picked_frame()?).clamp(1, count) - 1,
            _ => {
                let p = f64::from(self.position.clamp(0.0, 1.0));
                if p.is_finite() {
                    (p * (count - 1) as f64).round() as u64
                } else {
                    0
                }
            }
        };
        Some(before + k + 1)
    }

    fn picture_count(&self, facts: &Facts) -> u64 {
        match self.content {
            Content::Sources => 2 * facts.sequences.len() as u64,
            Content::Preview => {
                let per = parse_whole(&self.per_sequence).unwrap_or(5);
                facts
                    .sequences
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| self.sequence.is_none_or(|s| s == *i))
                    .map(|(_, s)| per.min(u64::from(s.1)))
                    .sum()
            }
            Content::Frames => self.frame_count(facts),
        }
    }

    /// The footer's note: what the export will produce.
    pub fn note(&self, facts: &Facts) -> String {
        let (w, h) = facts.size;
        let size = format!("{w} × {h} px");
        match self.kind {
            Kind::Frame => format!("{size} · {}", self.format_name()),
            Kind::FrameDots => "2 files · picture and dots".to_string(),
            Kind::MovieImages | Kind::SequenceImages => {
                format!("{} · {size}", frames_text(self.frame_count(facts)))
            }
            Kind::Animation | Kind::Video => {
                let n = self.frame_count(facts);
                let fps = parse_number(&self.fps).unwrap_or(facts.fps);
                let secs = if fps > 0.0 { n as f64 / fps } else { 0.0 };
                format!("{} · {secs:.1} s · {size}", frames_text(n))
            }
            Kind::Sheet => {
                let n = self.picture_count(facts);
                let pages = (n as usize).div_ceil(PAGE_PICTURES).max(1);
                format!(
                    "{} · {}",
                    if n == 1 {
                        "1 picture".to_string()
                    } else {
                        format!("{n} pictures")
                    },
                    if pages == 1 {
                        "about 1 page".to_string()
                    } else {
                        format!("about {pages} pages")
                    }
                )
            }
        }
    }

    /// What is wrong with what has been typed, in a sentence; `None` when it can run.
    pub fn problem(&self) -> Option<String> {
        if self.path.trim().is_empty() {
            return None; // not yet an error: the button just waits
        }
        if self.stem.contains(['/', '\\', ':']) {
            return Some("The name can’t contain / \\ or :.".into());
        }
        match self.kind {
            Kind::Frame | Kind::FrameDots => {
                if !matches!(extension(&self.path).as_str(), "png" | "jpg" | "jpeg") {
                    return Some("The file name must end in .png or .jpg.".into());
                }
            }
            Kind::Sheet => {
                if extension(&self.path) != "pdf" {
                    return Some("The file name must end in .pdf.".into());
                }
            }
            Kind::Animation | Kind::Video => {
                if format_of_extension(self.kind, extension(&self.path)).is_none() {
                    return Some(if self.kind == Kind::Video {
                        "The file name must end in .mp4, .webm or .mov.".into()
                    } else {
                        "The file name must end in .gif, .png, .apng or .webp.".into()
                    });
                }
            }
            Kind::MovieImages | Kind::SequenceImages => {
                let prefix = self.prefix.trim();
                if prefix.contains(['/', '\\', ':']) {
                    return Some("The file name start can’t contain / \\ or :.".into());
                }
            }
        }
        None
    }

    /// What is wrong with the chosen range of frames, shown under it.
    pub fn range_problem(&self) -> Option<String> {
        let whole = |text: &str, what: &str| -> Option<String> {
            let t = text.trim();
            if t.is_empty() || parse_whole(t).is_some_and(|n| n >= 1) {
                None
            } else {
                Some(format!("{what} must be a whole number, 1 or more."))
            }
        };
        if self.shows_range() && !self.all_frames {
            if let Some(p) =
                whole(&self.first, "First frame").or_else(|| whole(&self.last, "Last frame"))
            {
                return Some(p);
            }
            if let (Some(a), Some(b)) = (parse_whole(&self.first), parse_whole(&self.last)) {
                if b < a {
                    return Some("The last frame comes before the first.".into());
                }
            }
        }
        None
    }

    /// The first other field that cannot be read, shown under the form.
    pub fn other_problem(&self) -> Option<String> {
        let whole = |text: &str, what: &str| -> Option<String> {
            let t = text.trim();
            if t.is_empty() || parse_whole(t).is_some_and(|n| n >= 1) {
                None
            } else {
                Some(format!("{what} must be a whole number, 1 or more."))
            }
        };
        if self.frame_by == FrameBy::Frame && self.kind == Kind::Frame {
            if let Some(p) = whole(&self.frame, "Frame number") {
                return Some(p);
            }
        }
        if self.frame_by == FrameBy::Movie && self.kind == Kind::Frame {
            if let Some(p) = whole(&self.movie_frame, "Movie frame") {
                return Some(p);
            }
        }
        if self.kind.is_movie_video() {
            let t = self.fps.trim();
            if !t.is_empty() && !parse_number(t).is_some_and(|n| (1.0..=60.0).contains(&n)) {
                return Some("Speed must be between 1 and 60 frames a second.".into());
            }
            if self.uses_matte()
                && !self.matte.trim().is_empty()
                && !is_hex_colour(self.matte.trim())
            {
                return Some("The background colour looks like #ffffff.".into());
            }
        }
        if self.kind.has_dpi() {
            let t = self.dpi.trim();
            if !t.is_empty() && parse_whole(t).is_none_or(|n| n == 0) {
                return Some("Resolution must be a whole number of dots per inch.".into());
            }
        }
        if self.kind == Kind::Sheet {
            for (text, what, max) in [
                (&self.per_sequence, "Pictures per sequence", 50),
                (&self.columns, "Pictures across", 12),
            ] {
                let t = text.trim();
                if !t.is_empty() && !parse_whole(t).is_some_and(|n| (1..=max).contains(&n)) {
                    return Some(format!("{what} must be between 1 and {max}."));
                }
            }
            for (text, what, lo, hi) in [
                (&self.thumb, "Picture size", 32, 2048),
                (&self.sheet_quality, "JPEG quality", 1, 100),
            ] {
                let t = text.trim();
                if !t.is_empty() && !parse_whole(t).is_some_and(|n| (lo..=hi).contains(&n)) {
                    return Some(format!("{what} must be between {lo} and {hi}."));
                }
            }
        }
        None
    }

    fn field_problem(&self) -> Option<String> {
        self.range_problem().or_else(|| self.other_problem())
    }

    /// The first thing keeping Export from running.
    pub fn blocker(&self) -> Option<String> {
        self.problem().or_else(|| self.field_problem())
    }

    /// Whether Export can run.
    pub fn ready(&self) -> bool {
        if self.path.trim().is_empty()
            || self.blocker().is_some()
            || self.missing_folder().is_some()
        {
            return false;
        }
        self.kind != Kind::Video || self.ffmpeg_found
    }

    /// The parameters of the command. Fields left empty are left out, so the command's own
    /// defaults apply.
    pub fn to_params(&self) -> Result<Value, String> {
        if let Some(p) = self.blocker() {
            return Err(p);
        }
        let mut o = Map::new();
        let path = self.path.trim();
        if path.is_empty() {
            return Err(if self.kind.is_folder() {
                "Choose a folder first.".into()
            } else {
                "Choose where to save first.".into()
            });
        }
        o.insert(
            if self.kind.is_folder() {
                "out_dir"
            } else {
                "out"
            }
            .into(),
            json!(path),
        );
        if let Some(s) = self.sequence {
            // A single frame given as a movie frame names its sequence itself.
            if !(self.kind == Kind::Frame && self.frame_by == FrameBy::Movie) {
                o.insert("sequence".into(), json!(s));
            }
        }
        match self.kind {
            Kind::Frame => {
                match self.frame_by {
                    FrameBy::Position => {
                        o.insert("t".into(), json!(round3(self.position)));
                    }
                    FrameBy::Frame => {
                        // The dialog counts from 1; the command counts from 0.
                        let n = parse_whole(&self.frame).unwrap_or(1).max(1) - 1;
                        o.insert("frame".into(), json!(n));
                    }
                    FrameBy::Movie => {
                        let n = parse_whole(&self.movie_frame).unwrap_or(1).max(1);
                        o.insert("movie_frame".into(), json!(n));
                    }
                }
                if self.is_jpeg() && self.jpeg_quality != 92 {
                    o.insert("quality".into(), json!(self.jpeg_quality));
                }
            }
            Kind::FrameDots => {
                o.insert("t".into(), json!(round3(self.position)));
                let dots = self.dots_path.trim();
                if !dots.is_empty() {
                    o.insert("dots_out".into(), json!(dots));
                }
            }
            Kind::MovieImages | Kind::SequenceImages => {
                let prefix = self.prefix.trim();
                if !prefix.is_empty() && prefix != DEFAULT_PREFIX {
                    o.insert("prefix".into(), json!(prefix));
                }
            }
            Kind::Animation | Kind::Video => {
                o.insert("format".into(), json!(self.format));
                if let Some(fps) = parse_number(&self.fps).filter(|f| *f != self.default_fps) {
                    o.insert("fps".into(), json!(fps));
                }
                if self.kind == Kind::Animation {
                    if !self.repeat {
                        o.insert("loop".into(), json!(false));
                    }
                    if !self.transparent {
                        o.insert("transparent".into(), json!(false));
                    }
                }
                if self.kind == Kind::Video && facts_audio_off(self) {
                    o.insert("audio".into(), json!(false));
                }
                if self.uses_matte()
                    && !self.matte.trim().is_empty()
                    && !self.matte.trim().eq_ignore_ascii_case(DEFAULT_MATTE)
                {
                    o.insert("matte".into(), json!(self.matte.trim()));
                }
                if self.kind == Kind::Video && !self.ffmpeg.trim().is_empty() {
                    o.insert("ffmpeg".into(), json!(self.ffmpeg.trim()));
                }
            }
            Kind::Sheet => {
                o.insert("content".into(), json!(self.content.id()));
                for (name, text, default) in [
                    ("per_sequence", &self.per_sequence, DEFAULT_PER_SEQUENCE),
                    ("columns", &self.columns, DEFAULT_COLUMNS),
                    ("thumb", &self.thumb, DEFAULT_THUMB),
                    ("quality", &self.sheet_quality, DEFAULT_SHEET_QUALITY),
                ] {
                    if let Some(n) = parse_whole(text).filter(|n| *n != default) {
                        o.insert(name.into(), json!(n));
                    }
                }
                if self.letter {
                    o.insert("page".into(), json!("letter"));
                }
                if self.landscape {
                    o.insert("landscape".into(), json!(true));
                }
                if !self.captions {
                    o.insert("captions".into(), json!(false));
                }
                let title = self.title.trim();
                if !title.is_empty() && title != self.default_title.trim() {
                    o.insert("title".into(), json!(title));
                }
            }
        }
        if self.shows_range() && !self.all_frames {
            if let Some(n) = parse_whole(&self.first) {
                o.insert("first".into(), json!(n));
            }
            if let Some(n) = parse_whole(&self.last) {
                o.insert("last".into(), json!(n));
            }
        }
        if self.kind.has_direction_options() {
            if self.reverse {
                o.insert("reverse".into(), json!(true));
            }
            if self.drop_duplicates {
                o.insert("drop_duplicates".into(), json!(true));
            }
        }
        if self.kind.has_dpi() {
            if let Some(n) = parse_whole(&self.dpi).filter(|n| *n != DEFAULT_DPI) {
                o.insert("dpi".into(), json!(n));
            }
        }
        if self.kind.has_after_options() {
            if self.open_after {
                o.insert("open_after".into(), json!(true));
            }
            if self.save_first {
                o.insert("save_first".into(), json!(true));
            }
        }
        Ok(Value::Object(o))
    }
}

/// Sound is sent as off only when the person turned it off.
fn facts_audio_off(d: &ExportDialog) -> bool {
    !d.audio
}

/// A number as the person would type it: `24`, not `24.0`.
fn number_text(n: f64) -> String {
    if n.is_finite() {
        format!("{n}")
    } else {
        String::new()
    }
}

/// Whether `name` is `prefix_` and digits and `.png`, the name an image sequence writes.
fn numbered_name(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .and_then(|r| r.strip_prefix('_'))
        .and_then(|r| r.strip_suffix(".png"))
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
}

/// `path` with `.` and `..` worked out and the system's separators, so what is shown is what
/// is written. A `..` that has nothing to remove is kept.
pub fn normalise(path: &str) -> String {
    let mut out = PathBuf::new();
    for part in Path::new(path.trim()).components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                let pops = out
                    .components()
                    .next_back()
                    .is_some_and(|c| matches!(c, Component::Normal(_)));
                if pops {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out.display().to_string()
}

/// What the dialog remembers of one kind of export, for the next time it opens.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportMemory {
    pub dir: String,
    pub format: String,
    pub jpeg: bool,
    pub fps: String,
    pub repeat: Option<bool>,
    pub transparent: Option<bool>,
    pub open_after: bool,
    pub letter: bool,
    pub landscape: bool,
    pub captions: Option<bool>,
}

impl ExportDialog {
    /// The choices worth carrying to the next export of this kind.
    pub fn remember(&self) -> ExportMemory {
        ExportMemory {
            dir: if Path::new(self.dir.trim()).is_dir() {
                self.dir.trim().to_string()
            } else {
                String::new()
            },
            format: if self.kind.is_movie_video() {
                self.format.clone()
            } else {
                String::new()
            },
            jpeg: self.kind == Kind::Frame && self.ext == "jpg",
            fps: self.fps.trim().to_string(),
            repeat: Some(self.repeat),
            transparent: Some(self.transparent),
            open_after: self.open_after,
            letter: self.letter,
            landscape: self.landscape,
            captions: Some(self.captions),
        }
    }

    /// Start from what was chosen last time.
    pub fn apply_memory(&mut self, m: &ExportMemory) {
        if !m.dir.is_empty() && Path::new(&m.dir).is_dir() {
            self.set_dir(&m.dir);
        }
        match self.kind {
            Kind::Animation | Kind::Video if !m.format.is_empty() => {
                let ok = format_of_extension(self.kind, extension_of_format(&m.format).into());
                if ok == Some(m.format.as_str()) {
                    self.set_format(&m.format);
                }
            }
            Kind::Frame if m.jpeg => self.set_picture_format(true),
            _ => {}
        }
        if self.kind.is_movie_video() && parse_number(&m.fps).is_some() {
            self.fps = m.fps.clone();
        }
        if self.kind == Kind::Animation {
            self.repeat = m.repeat.unwrap_or(self.repeat);
            self.transparent = m.transparent.unwrap_or(self.transparent);
        }
        if self.kind.has_after_options() {
            self.open_after = m.open_after;
        }
        if self.kind == Kind::Sheet {
            self.letter = m.letter;
            self.landscape = m.landscape;
            self.captions = m.captions.unwrap_or(self.captions);
        }
        self.error = None;
    }
}

fn frames_text(n: u64) -> String {
    if n == 1 {
        "1 frame".to_string()
    } else {
        format!("{n} frames")
    }
}

fn round3(v: f32) -> f64 {
    if v.is_finite() {
        (f64::from(v.clamp(0.0, 1.0)) * 1000.0).round() / 1000.0
    } else {
        0.0
    }
}

fn parse_whole(text: &str) -> Option<u64> {
    text.trim().parse().ok()
}

fn parse_number(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

fn is_hex_colour(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// The lower-case extension of `path`, without the dot.
pub fn extension(path: &str) -> String {
    Path::new(path.trim())
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn extension_of_format(format: &str) -> &'static str {
    match format {
        "gif" => "gif",
        "apng" => "png",
        "webp" => "webp",
        "webm" => "webm",
        "prores" => "mov",
        _ => "mp4",
    }
}

/// The format a file extension means for this kind; `.png` is APNG for an animation.
fn format_of_extension(kind: Kind, ext: String) -> Option<&'static str> {
    match (kind, ext.as_str()) {
        (Kind::Animation, "gif") => Some("gif"),
        (Kind::Animation, "png" | "apng") => Some("apng"),
        (Kind::Animation, "webp") => Some("webp"),
        (Kind::Video, "mp4") => Some("mp4"),
        (Kind::Video, "webm") => Some("webm"),
        (Kind::Video, "mov") => Some("prores"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            sequences: vec![("sun to moon".into(), 48), ("moon to star".into(), 48)],
            total_frames: 96,
            size: (1080, 720),
            fps: 24.0,
            has_sound: true,
            current: 1,
            frame: 4,
            stem: "sun".into(),
            dir: std::env::temp_dir().display().to_string(),
        }
    }

    fn dialog(kind: Kind) -> ExportDialog {
        ExportDialog::new(kind, &facts())
    }

    #[test]
    fn every_kind_maps_to_its_command_and_back() {
        for k in Kind::ALL {
            assert_eq!(Kind::from_id(k.id()), Some(k));
        }
        assert_eq!(Kind::from_id("export.nothing"), None);
    }

    #[test]
    fn paths_start_beside_the_project_or_empty() {
        assert!(dialog(Kind::Frame).path.ends_with("sun.png"));
        assert!(dialog(Kind::MovieImages).path.ends_with("sun frames"));
        let mut f = facts();
        f.dir.clear();
        assert_eq!(ExportDialog::new(Kind::Video, &f).path, "");
        assert!(!ExportDialog::new(Kind::Video, &f).ready());
    }

    #[test]
    fn a_frame_defaults_to_the_current_sequence_and_counts_from_one() {
        let mut d = dialog(Kind::Frame);
        d.frame_by = FrameBy::Frame;
        d.frame = "1".into();
        let p = d.to_params().unwrap();
        assert_eq!(p["frame"], 0);
        assert_eq!(p["sequence"], 1);
        assert!(p.get("t").is_none());
        d.frame_by = FrameBy::Movie;
        d.movie_frame = "60".into();
        let p = d.to_params().unwrap();
        assert_eq!(p["movie_frame"], 60);
        assert!(
            p.get("sequence").is_none(),
            "a movie frame names its sequence"
        );
    }

    #[test]
    fn jpeg_quality_is_sent_only_for_jpeg() {
        let mut d = dialog(Kind::Frame);
        d.jpeg_quality = 70;
        assert!(d.to_params().unwrap().get("quality").is_none());
        d.set_path("C:/work/a.JPG");
        assert!(d.is_jpeg());
        assert_eq!(d.to_params().unwrap()["quality"], 70);
        assert_eq!(d.format_name(), "JPEG");
    }

    #[test]
    fn a_wrong_extension_blocks_export_with_a_sentence() {
        let mut d = dialog(Kind::Frame);
        d.set_path("C:/work/a.bmp");
        assert!(!d.ready());
        assert!(d.blocker().unwrap().contains(".png"));
        assert!(d.to_params().is_err());
        d.set_path(format!("{}/a.png", facts().dir));
        assert!(d.ready());
    }

    #[test]
    fn a_format_changes_the_extension_and_the_extension_changes_the_format() {
        let mut d = dialog(Kind::Animation);
        d.set_format("webp");
        assert!(d.path.ends_with("sun.webp"), "{}", d.path);
        d.set_format("apng");
        assert!(d.path.ends_with("sun.png"));
        d.set_path("C:/work/x.gif");
        assert_eq!(d.format, "gif");
        let p = d.to_params().unwrap();
        assert_eq!(p["format"], "gif");
        let mut v = dialog(Kind::Video);
        v.set_path("C:/work/x.mov");
        assert_eq!(v.format, "prores");
    }

    #[test]
    fn options_that_match_the_default_are_left_out() {
        let p = dialog(Kind::Animation).to_params().unwrap();
        let keys: Vec<_> = p.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["format", "out"].map(String::from));
        let mut d = dialog(Kind::Animation);
        d.repeat = false;
        d.transparent = false;
        d.matte = "#ff0000".into();
        d.fps = "12".into();
        let p = d.to_params().unwrap();
        assert_eq!(p["loop"], false);
        assert_eq!(p["transparent"], false);
        assert_eq!(p["matte"], "#ff0000");
        assert_eq!(p["fps"], 12.0);
    }

    #[test]
    fn the_range_is_sent_only_when_chosen() {
        let mut d = dialog(Kind::MovieImages);
        assert!(d.to_params().unwrap().get("first").is_none());
        d.all_frames = false;
        d.first = "10".into();
        d.last = "20".into();
        let p = d.to_params().unwrap();
        assert_eq!(
            (p["first"].as_u64(), p["last"].as_u64()),
            (Some(10), Some(20))
        );
        assert_eq!(d.frame_count(&facts()), 11);
        d.last = "5".into();
        assert!(d.blocker().unwrap().contains("last frame comes before"));
        d.last = "x".into();
        assert!(!d.ready());
    }

    #[test]
    fn a_sequence_scope_counts_its_own_frames() {
        let mut d = dialog(Kind::Video);
        assert_eq!(d.frame_count(&facts()), 96);
        d.sequence = Some(0);
        assert_eq!(d.frame_count(&facts()), 48);
        assert_eq!(d.to_params().unwrap()["sequence"], 0);
        assert_eq!(d.note(&facts()), "48 frames · 2.0 s · 1080 × 720 px");
    }

    #[test]
    fn video_waits_for_ffmpeg_and_sends_audio_only_when_off() {
        let mut d = dialog(Kind::Video);
        d.ffmpeg_found = false;
        assert!(!d.ready());
        d.ffmpeg_found = true;
        assert!(d.ready());
        assert!(d.to_params().unwrap().get("audio").is_none());
        d.audio = false;
        assert_eq!(d.to_params().unwrap()["audio"], false);
        d.ffmpeg = "C:/tools/ffmpeg.exe".into();
        assert_eq!(d.to_params().unwrap()["ffmpeg"], "C:/tools/ffmpeg.exe");
    }

    #[test]
    fn matte_applies_to_mp4_and_to_flattened_animations_only() {
        let mut v = dialog(Kind::Video);
        assert!(v.uses_matte());
        v.set_format("webm");
        assert!(!v.uses_matte());
        let mut a = dialog(Kind::Animation);
        assert!(!a.uses_matte());
        a.transparent = false;
        assert!(a.uses_matte());
        a.matte = "red".into();
        assert!(a.blocker().unwrap().contains("#ffffff"));
    }

    #[test]
    fn a_contact_sheet_checks_its_numbers() {
        let mut d = dialog(Kind::Sheet);
        let p = d.to_params().unwrap();
        assert_eq!(p["content"], "preview");
        d.columns = "13".into();
        assert!(d.blocker().unwrap().contains("between 1 and 12"));
        d.columns = "4".into();
        d.letter = true;
        d.captions = false;
        d.title = " Holiday ".into();
        let p = d.to_params().unwrap();
        assert_eq!(p["columns"], 4);
        assert_eq!(p["page"], "letter");
        assert_eq!(p["captions"], false);
        assert_eq!(p["title"], "Holiday");
        d.set_path("C:/work/a.txt");
        assert!(d.blocker().unwrap().contains(".pdf"));
    }

    #[test]
    fn sheet_picture_counts_follow_the_content() {
        let f = facts();
        let mut d = dialog(Kind::Sheet);
        assert_eq!(d.note(&f), "10 pictures · about 1 page");
        d.content = Content::Sources;
        assert_eq!(d.note(&f), "4 pictures · about 1 page");
        d.content = Content::Frames;
        assert_eq!(d.note(&f), "96 pictures · about 8 pages");
        d.sequence = Some(0);
        d.content = Content::Preview;
        d.per_sequence = "3".into();
        assert_eq!(d.note(&f), "3 pictures · about 1 page");
    }

    #[test]
    fn dots_export_sends_the_dots_path_only_when_typed() {
        let mut d = dialog(Kind::FrameDots);
        assert!(d.to_params().unwrap().get("dots_out").is_none());
        d.dots_path = "C:/work/mid.inbetweendots".into();
        d.position = 0.25;
        let p = d.to_params().unwrap();
        assert_eq!(p["dots_out"], "C:/work/mid.inbetweendots");
        assert_eq!(p["t"], 0.25);
        assert!(p.get("open_after").is_none());
    }

    #[test]
    fn nonsense_numbers_never_reach_the_command() {
        let mut d = dialog(Kind::Frame);
        d.frame_by = FrameBy::Position;
        d.position = f32::NAN;
        assert_eq!(d.to_params().unwrap()["t"], 0.0);
        d.dpi = "-3".into();
        assert!(d.to_params().is_err());
        d.dpi = "300".into();
        assert_eq!(d.to_params().unwrap()["dpi"], 300);
        let mut s = dialog(Kind::MovieImages);
        s.prefix = "a/b".into();
        assert!(!s.ready());
        let empty = ExportDialog::new(Kind::Sheet, &Facts::default());
        assert_eq!(empty.note(&Facts::default()), "0 pictures · about 1 page");
        assert!(empty.to_params().is_err());
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("inbetween-exd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_picture_opens_on_the_frame_in_preview() {
        let d = dialog(Kind::Frame);
        // Frame 4 (counting from 0) of the second sequence's 48 is picked number 5.
        assert_eq!(d.frame, "5");
        assert_eq!(d.frame_by, FrameBy::Frame);
        assert!((d.position - 4.0 / 47.0).abs() < 1e-6);
        let p = d.to_params().unwrap();
        assert_eq!(
            (p["frame"].as_u64(), p["sequence"].as_u64()),
            (Some(4), Some(1))
        );
        let dots = dialog(Kind::FrameDots);
        assert_eq!(dots.to_params().unwrap()["t"], round3(4.0 / 47.0));
    }

    #[test]
    fn picking_a_frame_moves_the_position_and_stays_in_range() {
        let mut d = dialog(Kind::FrameDots);
        d.set_frame(1, 12);
        assert_eq!(d.position, 0.0);
        d.set_frame(99, 12);
        assert_eq!((d.frame.as_str(), d.position), ("12", 1.0));
        d.set_frame(3, 1);
        assert_eq!(d.position, 0.0, "one frame has nowhere to move");
        assert_eq!(d.picked_frame(), Some(1));
    }

    #[test]
    fn the_picture_shown_is_the_movie_frame_picked() {
        let f = facts();
        let mut d = dialog(Kind::Frame);
        assert_eq!(d.movie_frame_shown(&f), Some(48 + 5));
        d.set_frame(1, 48);
        assert_eq!(d.movie_frame_shown(&f), Some(49));
        d.sequence = Some(0);
        assert_eq!(d.movie_frame_shown(&f), Some(1));
        d.frame_by = FrameBy::Movie;
        d.movie_frame = "70".into();
        assert_eq!(d.movie_frame_shown(&f), Some(70));
        d.movie_frame = "970".into();
        assert_eq!(d.movie_frame_shown(&f), None);
        d.frame_by = FrameBy::Position;
        d.position = f32::NAN;
        assert_eq!(d.movie_frame_shown(&f), Some(1));
    }

    #[test]
    fn the_name_and_folder_are_edited_apart_and_make_the_path() {
        let mut d = dialog(Kind::Frame);
        assert_eq!((d.stem.as_str(), d.ext.as_str()), ("sun", "png"));
        d.set_stem("mid");
        assert!(d.path.ends_with("mid.png"), "{}", d.path);
        d.set_picture_format(true);
        assert!(d.path.ends_with("mid.jpg") && d.is_jpeg());
        d.set_dir("C:/out/./a/../b");
        let parent = Path::new(&d.path)
            .parent()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned());
        assert_eq!(parent.as_deref(), Some("b"));
        d.set_stem("  ");
        assert_eq!(d.path, "", "no name, nothing to export");
        assert!(!d.ready());
        d.set_stem("a/b");
        assert!(d.blocker().unwrap().contains("can’t contain"));
    }

    #[test]
    fn typed_paths_are_tidied_and_split() {
        assert_eq!(
            Path::new(&normalise("C:/a/../b/./c.png"))
                .components()
                .count(),
            Path::new("C:/b/c.png").components().count()
        );
        assert!(normalise("../x").starts_with(".."));
        let mut d = dialog(Kind::MovieImages);
        d.set_path("C:/out/shots");
        assert_eq!((d.stem.as_str(), d.ext.as_str()), ("shots", ""));
        assert_eq!(d.ext_suffix(), "");
        let mut a = dialog(Kind::Animation);
        a.set_path("C:/out/x.webp");
        assert_eq!((a.stem.as_str(), a.ext_suffix().as_str()), ("x", ".webp"));
        assert_eq!(a.format, "webp");
    }

    #[test]
    fn exporting_over_a_file_or_numbered_frames_is_called_out() {
        let dir = temp_dir("replace");
        let f = facts();
        let mut d = dialog(Kind::Frame);
        d.set_dir(dir.display().to_string());
        d.set_stem("mid");
        assert_eq!(d.replaces(), None);
        assert_eq!(d.action_label(&f), "Export PNG");
        std::fs::write(dir.join("mid.png"), b"x").unwrap();
        assert!(d.replaces().unwrap().contains("mid.png already exists"));
        assert_eq!(d.action_label(&f), "Replace");

        let frames = dir.join("shots");
        std::fs::create_dir_all(&frames).unwrap();
        let mut m = dialog(Kind::MovieImages);
        m.set_path(frames.display().to_string());
        assert_eq!(m.replaces(), None);
        assert_eq!(m.action_label(&f), "Export 96 frames");
        for name in [
            "frame_0001.png",
            "frame_0002.png",
            "other.png",
            "frame_x.png",
        ] {
            std::fs::write(frames.join(name), b"x").unwrap();
        }
        assert_eq!(
            m.replaces().as_deref(),
            Some("2 files named frame_####.png will be replaced.")
        );
        assert_eq!(m.action_label(&f), "Replace 96 frames");
        m.prefix = "other".into();
        assert_eq!(m.replaces(), None, "other.png is not numbered");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_folder_blocks_export_until_it_exists() {
        let dir = temp_dir("missing");
        let mut d = dialog(Kind::Video);
        d.set_dir(dir.join("blocker").display().to_string());
        assert_eq!(d.missing_folder(), Some(dir.join("blocker")));
        assert!(!d.ready());
        assert_eq!(d.replaces(), None);
        std::fs::create_dir_all(dir.join("blocker")).unwrap();
        assert_eq!(d.missing_folder(), None);
        assert!(d.ready());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_option_starts_on_its_real_default_and_params_still_leave_it_out() {
        for kind in Kind::ALL {
            let d = dialog(kind);
            let p = d.to_params().unwrap();
            for key in [
                "fps",
                "matte",
                "prefix",
                "dpi",
                "per_sequence",
                "columns",
                "thumb",
                "quality",
                "title",
            ] {
                assert!(p.get(key).is_none(), "{kind:?} sends {key}: {p}");
            }
        }
        let d = dialog(Kind::Video);
        assert_eq!((d.fps.as_str(), d.matte.as_str()), ("24", "#ffffff"));
        let mut s = dialog(Kind::Sheet);
        assert_eq!((s.columns.as_str(), s.per_sequence.as_str()), ("3", "5"));
        s.columns = "4".into();
        s.title = "Holiday".into();
        let p = s.to_params().unwrap();
        assert_eq!(
            (p["columns"].as_u64(), p["title"].as_str()),
            (Some(4), Some("Holiday"))
        );
        let mut v = dialog(Kind::Video);
        v.fps = "30".into();
        assert_eq!(v.to_params().unwrap()["fps"], 30.0);
    }

    #[test]
    fn the_last_choices_come_back_next_time() {
        let dir = temp_dir("memory");
        let mut a = dialog(Kind::Animation);
        a.set_dir(dir.display().to_string());
        a.set_format("webp");
        a.fps = "12".into();
        a.repeat = false;
        a.open_after = true;
        let memory = a.remember();
        let json = serde_json::to_string(&memory).unwrap();
        let back: ExportMemory = serde_json::from_str(&json).unwrap();
        let mut next = dialog(Kind::Animation);
        next.apply_memory(&back);
        assert_eq!(next.format, "webp");
        assert!(next.path.ends_with("sun.webp") && next.dir == a.dir);
        assert_eq!(next.fps, "12");
        assert!(!next.repeat && next.open_after);
        // A folder that has gone away is not restored, and unknown fields are tolerated.
        let _ = std::fs::remove_dir_all(&dir);
        let mut later = dialog(Kind::Animation);
        let before = later.dir.clone();
        later.apply_memory(&back);
        assert_eq!(later.dir, before);
        let odd: ExportMemory = serde_json::from_str(r#"{"format":"nope","extra":1}"#).unwrap();
        let mut v = dialog(Kind::Video);
        v.apply_memory(&odd);
        assert_eq!(v.format, "mp4");
    }

    #[test]
    fn a_single_sequence_is_named_in_the_subtitle() {
        let mut f = facts();
        f.sequences = vec![("sun to moon".into(), 12)];
        f.total_frames = 12;
        f.current = 0;
        let d = ExportDialog::new(Kind::Frame, &f);
        assert_eq!(
            d.subtitle(&f),
            "Save one frame of “sun to moon” as a picture."
        );
        assert_eq!(
            dialog(Kind::Frame).subtitle(&facts()),
            Kind::Frame.subtitle()
        );
    }
}
