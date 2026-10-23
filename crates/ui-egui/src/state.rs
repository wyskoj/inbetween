//! All the state the shell keeps for itself, in one serde struct.
//!
//! It holds only view state (zoom, pan, selection, playback, toggles), never project data, so
//! the CLI and MCP can read or drive it and a restart restores the layout.

use crate::crop::CropTool;
use crate::curves::CurveKind;
use crate::view::View;
use inbetween_engine::DisplayPrefs;
use serde::{Deserialize, Serialize};

/// Which of the three canvases a view belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Source1,
    Source2,
    Preview,
}

impl Panel {
    pub const fn index(self) -> usize {
        match self {
            Self::Source1 => 0,
            Self::Source2 => 1,
            Self::Preview => 2,
        }
    }
}

/// What a click on a source panel does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    /// Select, marquee-select and drag dots and lines.
    Edit,
    /// Click to add a dot (Alt+click puts the partner at the same position).
    #[default]
    Add,
    /// Click a dot to delete it.
    Delete,
    /// Each click drops a dot and joins it to the last one.
    Line,
}

impl Tool {
    pub const ALL: [Self; 4] = [Self::Edit, Self::Add, Self::Delete, Self::Line];

    pub fn label(self) -> &'static str {
        match self {
            Self::Edit => "Edit",
            Self::Add => "Add",
            Self::Delete => "Delete",
            Self::Line => "Line",
        }
    }
}

/// How the canvases are arranged (F5 to F9).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    /// Both source images over the preview.
    #[default]
    Full,
    /// Both source images, full height.
    Edit,
    /// The preview alone.
    Preview,
    /// A grid of thumbnails of every frame.
    Frames,
    /// The editor here and the preview in a second window.
    Dual,
}

impl ViewMode {
    pub const ALL: [Self; 5] = [
        Self::Full,
        Self::Edit,
        Self::Preview,
        Self::Frames,
        Self::Dual,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Edit => "Edit",
            Self::Preview => "Preview",
            Self::Frames => "Frame Sequence",
            Self::Dual => "Dual Window",
        }
    }
}

/// What the preview and transport play.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewScope {
    /// The selected sequence only.
    #[default]
    Sequence,
    /// The whole movie: every visible sequence in turn (or stacked, in a layer project).
    Movie,
}

/// What the Effects window edits: one sequence's effects or the whole movie's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectScope {
    #[default]
    Sequence,
    Movie,
}

impl EffectScope {
    /// The `scope` parameter of the `effect.*` commands.
    pub fn id(self) -> &'static str {
        match self {
            Self::Sequence => "sequence",
            Self::Movie => "movie",
        }
    }
}

/// What the Camera window edits: the sequence's camera, or one effect's.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraTarget {
    /// `None` for the sequence camera, otherwise `background`, `mask`, ...
    pub effect: Option<String>,
    /// The effect belongs to the whole movie, not the sequence.
    pub movie: bool,
}

/// What the caption window edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionScope {
    Image1,
    Image2,
    Sequence,
    Movie,
}

impl CaptionScope {
    pub const ALL: [Self; 4] = [Self::Image1, Self::Image2, Self::Sequence, Self::Movie];

    pub fn label(self) -> &'static str {
        match self {
            Self::Image1 => "Image 1",
            Self::Image2 => "Image 2",
            Self::Sequence => "Sequence",
            Self::Movie => "Movie",
        }
    }

    /// The command that sets a caption at this scope.
    pub fn command(self) -> &'static str {
        match self {
            Self::Image1 | Self::Image2 => "image.caption",
            Self::Sequence => "caption.sequence",
            Self::Movie => "caption.movie",
        }
    }

    /// The source image (1 or 2) this scope belongs to, if it is a per-image one.
    pub fn image(self) -> Option<u8> {
        match self {
            Self::Image1 => Some(1),
            Self::Image2 => Some(2),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewState {
    /// Zoom and pan of the two source panels and the preview.
    pub views: [View; 3],
    /// Keep the two source panels' zoom and pan identical, so a dot sits at the same screen
    /// position on both.
    pub link_sources: bool,
    pub show_numbers: bool,
    /// The active tool on the source panels.
    pub tool: Tool,
    /// Display and dot-tool preferences; the session holds the live copy, this one is saved.
    pub prefs: DisplayPrefs,
    /// The frame the preview shows.
    pub frame: u32,
    pub playing: bool,
    pub looping: bool,
    /// Play forward, then backward, instead of jumping back to the start.
    pub ping_pong: bool,
    pub fps: f32,
    /// Share of the height the two source panels get, the rest goes to the preview.
    pub top_fraction: f32,
    /// Share of the width Image 1 gets of the two source panels.
    pub left_fraction: f32,
    pub mode: ViewMode,
    pub preview_scope: PreviewScope,
    /// Draw a checkerboard behind transparent areas.
    pub checkerboard: bool,
    /// Play the movie's sound with the preview (off: muted).
    pub muted: bool,
    /// Keep the preview zoomed and panned like the source panels.
    pub sync_preview: bool,
    /// The scrub slider shows percent (in 1% steps) instead of frames.
    pub scrub_percent: bool,
    /// Edge of a frame-strip thumbnail in points.
    pub strip_size: f32,
    pub show_help: bool,
    /// The sequence the panels edit and the preview plays.
    pub sequence: usize,
    pub show_sequence_list: bool,
    /// The curve editor window, when open.
    pub curve_window: Option<CurveKind>,
    /// Projects opened or saved lately, newest first (at most [`MAX_RECENT`]).
    pub recent: Vec<String>,
    /// The source panel (1 or 2) last used; the Image menu's quick items act on it.
    pub focus_image: u8,
    /// The face library folder last used, offered again by the Save To Library form.
    pub library_dir: String,
    /// The Adjust window, when open: which image it edits.
    pub adjust_window: Option<u8>,
    /// The Effects window, when open.
    pub effects_window: Option<EffectScope>,
    /// The Caption window, when open.
    pub caption_window: Option<CaptionScope>,
    /// The Camera window, when open.
    pub camera_window: Option<CameraTarget>,
    /// The Face Model, Mix Faces and Face Library windows.
    pub face_model_window: bool,
    pub face_mix_window: bool,
    pub face_library_window: bool,
    /// The Track Curve window.
    pub track_window: bool,
    /// The crop being drawn, when the crop tool is active.
    pub crop: Option<CropTool>,
    /// What each Export dialog was last set to, by command id, so the next one starts there.
    pub export_memory: std::collections::BTreeMap<String, crate::export_dialog::ExportMemory>,
}

/// How many recent projects are remembered.
pub const MAX_RECENT: usize = 10;

impl Default for ViewState {
    fn default() -> Self {
        Self {
            views: [View::default(); 3],
            link_sources: true,
            show_numbers: true,
            tool: Tool::Add,
            prefs: DisplayPrefs::default(),
            frame: 0,
            playing: false,
            looping: true,
            ping_pong: false,
            fps: 24.0,
            top_fraction: 0.5,
            left_fraction: 0.5,
            mode: ViewMode::Full,
            preview_scope: PreviewScope::Sequence,
            checkerboard: true,
            muted: false,
            sync_preview: false,
            scrub_percent: false,
            strip_size: 120.0,
            show_help: false,
            sequence: 0,
            show_sequence_list: true,
            curve_window: None,
            recent: Vec::new(),
            focus_image: 1,
            library_dir: String::new(),
            adjust_window: None,
            effects_window: None,
            caption_window: None,
            camera_window: None,
            face_model_window: false,
            face_mix_window: false,
            face_library_window: false,
            track_window: false,
            crop: None,
            export_memory: Default::default(),
        }
    }
}

impl ViewState {
    /// Repair anything a hand-edited or old file could have made unusable.
    pub fn sanitize(&mut self) {
        if !self.fps.is_finite() {
            self.fps = 24.0;
        }
        self.fps = self.fps.clamp(1.0, 60.0);
        if !self.top_fraction.is_finite() {
            self.top_fraction = 0.5;
        }
        self.top_fraction = self.top_fraction.clamp(0.2, 0.8);
        if !self.left_fraction.is_finite() {
            self.left_fraction = 0.5;
        }
        self.left_fraction = self.left_fraction.clamp(0.2, 0.8);
        if !self.strip_size.is_finite() {
            self.strip_size = 120.0;
        }
        self.strip_size = self.strip_size.clamp(48.0, 256.0);
        self.prefs = self.prefs.sanitized();
        let mut seen: Vec<String> = Vec::new();
        for p in std::mem::take(&mut self.recent) {
            if !p.is_empty() && p.len() <= 4096 && !seen.contains(&p) && seen.len() < MAX_RECENT {
                seen.push(p);
            }
        }
        self.recent = seen;
        if !(1..=2).contains(&self.focus_image) {
            self.focus_image = 1;
        }
        if self.library_dir.len() > 4096 {
            self.library_dir.clear();
        }
        if self.adjust_window.is_some_and(|i| !(1..=2).contains(&i)) {
            self.adjust_window = None;
        }
        if self
            .crop
            .as_ref()
            .is_some_and(|c| !c.is_sane() || !(1..=2).contains(&c.image))
        {
            self.crop = None;
        }
        for v in &mut self.views {
            if !v.zoom.is_finite() || !v.offset.iter().all(|o| o.is_finite()) {
                *v = View::default();
            }
        }
    }

    pub fn view(&self, panel: Panel) -> &View {
        &self.views[panel.index()]
    }

    pub fn view_mut(&mut self, panel: Panel) -> &mut View {
        &mut self.views[panel.index()]
    }

    /// Put `path` at the top of the recent list, once, dropping the oldest past the cap.
    pub fn push_recent(&mut self, path: &str) {
        if path.is_empty() {
            return;
        }
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(MAX_RECENT);
    }

    /// Forget one recent project (for example a file that no longer exists).
    pub fn remove_recent(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
    }

    /// Ask every panel to re-fit its frame (after opening a project, for example).
    pub fn refit_all(&mut self) {
        for v in &mut self.views {
            v.auto_fit = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trips_through_json() {
        let mut s = ViewState {
            tool: Tool::Line,
            frame: 7,
            fps: 30.0,
            show_numbers: false,
            ..ViewState::default()
        };
        s.views[1].zoom = 2.5;
        s.views[1].auto_fit = false;
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<ViewState>(&text).unwrap(), s);
    }

    #[test]
    fn old_and_newer_files_still_load() {
        assert_eq!(
            serde_json::from_str::<ViewState>("{}").unwrap(),
            ViewState::default()
        );
        let s: ViewState =
            serde_json::from_str(r#"{"frame": 4, "something_from_the_future": [1, 2]}"#).unwrap();
        assert_eq!(s.frame, 4);
        assert!(s.link_sources);
    }

    #[test]
    fn sanitize_repairs_nonsense() {
        let mut s = ViewState {
            fps: f32::NAN,
            top_fraction: 7.0,
            ..ViewState::default()
        };
        s.views[2] = View {
            zoom: f32::INFINITY,
            offset: [f32::NAN, 0.0],
            auto_fit: false,
        };
        s.sanitize();
        assert_eq!(s.fps, 24.0);
        assert_eq!(s.top_fraction, 0.8);
        assert_eq!(s.views[2], View::default());
        s.fps = 0.0;
        s.sanitize();
        assert_eq!(s.fps, 1.0);
    }

    #[test]
    fn recent_list_is_newest_first_deduplicated_and_capped() {
        let mut s = ViewState::default();
        for i in 0..15 {
            s.push_recent(&format!("p{i}.inbetween"));
        }
        assert_eq!(s.recent.len(), MAX_RECENT);
        assert_eq!(s.recent[0], "p14.inbetween");
        assert_eq!(s.recent[MAX_RECENT - 1], "p5.inbetween");
        // Opening an older one again moves it to the top without duplicating it.
        s.push_recent("p9.inbetween");
        assert_eq!(s.recent[0], "p9.inbetween");
        assert_eq!(s.recent.iter().filter(|p| *p == "p9.inbetween").count(), 1);
        assert_eq!(s.recent.len(), MAX_RECENT);
        s.push_recent("");
        assert_eq!(s.recent.len(), MAX_RECENT);
        s.remove_recent("p9.inbetween");
        assert!(!s.recent.contains(&"p9.inbetween".to_string()));
        // The list survives a round trip, and old files without it load empty.
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(
            serde_json::from_str::<ViewState>(&text).unwrap().recent,
            s.recent
        );
        assert!(serde_json::from_str::<ViewState>("{}")
            .unwrap()
            .recent
            .is_empty());
    }

    #[test]
    fn sanitize_cleans_the_recent_list() {
        let mut s = ViewState {
            recent: vec!["a".into(), "".into(), "a".into(), "x".repeat(5000)],
            ..ViewState::default()
        };
        s.recent.extend((0..20).map(|i| format!("f{i}")));
        s.sanitize();
        assert_eq!(s.recent.len(), MAX_RECENT);
        assert_eq!(s.recent[0], "a");
        assert!(s.recent.iter().all(|p| !p.is_empty() && p.len() <= 4096));
    }

    #[test]
    fn views_are_addressed_by_panel() {
        let mut s = ViewState::default();
        s.view_mut(Panel::Preview).zoom = 3.0;
        assert_eq!(s.view(Panel::Preview).zoom, 3.0);
        assert_eq!(s.view(Panel::Source1).zoom, 1.0);
        s.view_mut(Panel::Preview).auto_fit = false;
        s.refit_all();
        assert!(s.view(Panel::Preview).auto_fit);
    }
}
