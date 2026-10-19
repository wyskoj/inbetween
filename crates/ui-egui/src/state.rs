//! All the state the shell keeps for itself, in one serde struct.
//!
//! It holds only view state (zoom, pan, selection, playback, toggles), never project data, so
//! the CLI and MCP can read or drive it and a restart restores the layout.

use crate::curves::CurveKind;
use crate::view::View;
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewState {
    /// Zoom and pan of the two source panels and the preview.
    pub views: [View; 3],
    /// Keep the two source panels' zoom and pan identical, so a dot sits at the same screen
    /// position on both.
    pub link_sources: bool,
    pub show_mesh: bool,
    pub show_numbers: bool,
    /// The selected dot pair, if any.
    pub selected: Option<usize>,
    /// The frame the preview shows.
    pub frame: u32,
    pub playing: bool,
    pub looping: bool,
    /// Play forward, then backward, instead of jumping back to the start.
    pub ping_pong: bool,
    pub fps: f32,
    /// Share of the height the two source panels get, the rest goes to the preview.
    pub top_fraction: f32,
    pub show_help: bool,
    /// The sequence the panels edit and the preview plays.
    pub sequence: usize,
    pub show_sequence_list: bool,
    /// The curve editor window, when open.
    pub curve_window: Option<CurveKind>,
    /// Projects opened or saved lately, newest first (at most [`MAX_RECENT`]).
    pub recent: Vec<String>,
}

/// How many recent projects are remembered.
pub const MAX_RECENT: usize = 10;

impl Default for ViewState {
    fn default() -> Self {
        Self {
            views: [View::default(); 3],
            link_sources: true,
            show_mesh: true,
            show_numbers: true,
            selected: None,
            frame: 0,
            playing: false,
            looping: true,
            ping_pong: false,
            fps: 24.0,
            top_fraction: 0.5,
            show_help: false,
            sequence: 0,
            show_sequence_list: true,
            curve_window: None,
            recent: Vec::new(),
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
        let mut seen: Vec<String> = Vec::new();
        for p in std::mem::take(&mut self.recent) {
            if !p.is_empty() && p.len() <= 4096 && !seen.contains(&p) && seen.len() < MAX_RECENT {
                seen.push(p);
            }
        }
        self.recent = seen;
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
            selected: Some(3),
            frame: 7,
            fps: 30.0,
            show_mesh: false,
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
