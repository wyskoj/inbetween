//! Shell-only view actions (see [`crate::actions`]): zoom, layouts, fullscreen, sync, scope.

use super::*;
use crate::actions::{ACTIONS, ALIASES};
use crate::state::{PreviewScope, ViewMode};

/// Where a view's content sits relative to the middle of its panel, so a view copied to a panel
/// of another size shows the same part of the frame in the middle.
pub(crate) fn carry_view(view: View, from: Rect, to: Rect) -> View {
    let shift = (to.size() - from.size()) / 2.0;
    View {
        zoom: view.zoom,
        offset: [view.offset[0] + shift.x, view.offset[1] + shift.y],
        auto_fit: view.auto_fit,
    }
}

impl InbetweenApp {
    fn panel_rect(&self, panel: Panel) -> Rect {
        match panel {
            Panel::Source1 => self.layout.source1,
            Panel::Source2 => self.layout.source2,
            Panel::Preview => self.layout.preview,
        }
    }

    /// The panels a zoom action changes: the one under the pointer and the ones linked to it,
    /// or all three when the pointer is not over a panel (the menu).
    fn zoom_targets(&self, pointer: Option<Pos2>) -> Vec<Panel> {
        let sources = |first: Panel| -> Vec<Panel> {
            let mut out = vec![first];
            if self.state.link_sources {
                out.push(if first == Panel::Source1 {
                    Panel::Source2
                } else {
                    Panel::Source1
                });
            }
            if self.state.sync_preview {
                out.push(Panel::Preview);
            }
            out
        };
        match self.panel_at(pointer) {
            Some(Panel::Source1) => sources(Panel::Source1),
            Some(Panel::Source2) => sources(Panel::Source2),
            Some(Panel::Preview) if self.state.sync_preview => {
                vec![Panel::Preview, Panel::Source1, Panel::Source2]
            }
            Some(Panel::Preview) => vec![Panel::Preview],
            None => vec![Panel::Source1, Panel::Source2, Panel::Preview],
        }
    }

    fn frame_size(&self) -> Option<Vec2> {
        self.prepared()
            .map(|p| Vec2::new(p.output.0 as f32, p.output.1 as f32))
    }

    /// Zoom action `id` on the panels under `pointer`.
    fn run_zoom(&mut self, id: &str, pointer: Option<Pos2>) {
        let Some(frame) = self.frame_size() else {
            return;
        };
        for panel in self.zoom_targets(pointer) {
            let rect = self.panel_rect(panel);
            let view = self.state.view_mut(panel);
            match id {
                "view.zoomIn" => view.step_preset(rect, true),
                "view.zoomOut" => view.step_preset(rect, false),
                "view.fit" => view.auto_fit = true,
                "view.actualSize" => *view = View::actual_size(frame, rect),
                "view.fitWidth" => *view = View::fit_width(frame, rect),
                "view.fitHeight" => *view = View::fit_height(frame, rect),
                _ => {}
            }
        }
    }

    pub(super) fn set_mode(&mut self, mode: ViewMode) {
        self.state.mode = mode;
        // The panels change size, so let every view settle on a fresh fit.
        self.state.refit_all();
    }

    /// Run the shell action `id`. Returns false for an id that is not one.
    pub fn run_action(&mut self, id: &str) -> bool {
        self.run_action_at(id, None)
    }

    pub(super) fn run_action_at(&mut self, id: &str, pointer: Option<Pos2>) -> bool {
        match id {
            "view.zoomIn" | "view.zoomOut" | "view.fit" | "view.actualSize" | "view.fitWidth"
            | "view.fitHeight" => self.run_zoom(id, pointer),
            "view.syncSources" => self.state.link_sources = !self.state.link_sources,
            "view.syncPreview" => self.state.sync_preview = !self.state.sync_preview,
            "view.mode.full" => self.set_mode(ViewMode::Full),
            "view.mode.edit" => self.set_mode(ViewMode::Edit),
            "view.mode.preview" => self.set_mode(ViewMode::Preview),
            "view.mode.frames" => self.set_mode(ViewMode::Frames),
            "view.mode.dual" => self.set_mode(ViewMode::Dual),
            "view.fullscreen" => self.fullscreen_request = Some(!self.fullscreen),
            "view.sequenceList" => self.state.show_sequence_list = !self.state.show_sequence_list,
            "view.previewScope" => {
                self.state.preview_scope = match self.state.preview_scope {
                    PreviewScope::Sequence => PreviewScope::Movie,
                    PreviewScope::Movie => PreviewScope::Sequence,
                };
                self.state.playing = false;
                self.preview.clear(self.gpu.as_ref());
                self.strip.clear();
            }
            "view.toggle.checkerboard" => self.state.checkerboard = !self.state.checkerboard,
            "view.toggle.numbers" => self.state.show_numbers = !self.state.show_numbers,
            "view.toggle.mute" => self.state.muted = !self.state.muted,
            "view.help" => self.state.show_help = !self.state.show_help,
            "app.quit" => {
                if self.request_quit() {
                    self.close_now = true;
                }
            }
            _ => return false,
        }
        true
    }

    /// Whether an on/off or pick-one action is on, for its mark in the menu. Plain actions
    /// (zoom, Help) give `None`.
    pub(super) fn action_checked(&self, id: &str) -> Option<bool> {
        Some(match id {
            "view.syncSources" => self.state.link_sources,
            "view.syncPreview" => self.state.sync_preview,
            "view.mode.full" => self.state.mode == ViewMode::Full,
            "view.mode.edit" => self.state.mode == ViewMode::Edit,
            "view.mode.preview" => self.state.mode == ViewMode::Preview,
            "view.mode.frames" => self.state.mode == ViewMode::Frames,
            "view.mode.dual" => self.state.mode == ViewMode::Dual,
            "view.fullscreen" => self.fullscreen,
            "view.sequenceList" => self.state.show_sequence_list,
            "view.previewScope" => self.state.preview_scope == PreviewScope::Movie,
            "view.toggle.checkerboard" => self.state.checkerboard,
            "view.toggle.numbers" => self.state.show_numbers,
            "view.toggle.mute" => self.state.muted,
            _ => return None,
        })
    }

    /// Consume the shortcuts of the action table. True when one ran.
    pub(super) fn action_keys(&mut self, ctx: &Context) -> bool {
        let pointer = ctx.input(|i| i.pointer.hover_pos());
        let bound = ACTIONS
            .iter()
            .filter_map(|a| a.shortcut.map(|s| (a.id, s)))
            .chain(ALIASES.iter().map(|(s, id)| (*id, *s)));
        // Keys with more modifiers first, so Alt+Ctrl+0 is not taken for Ctrl+0.
        let mut keys: Vec<_> = bound
            .filter_map(|(id, text)| Some((id, shortcuts::parse(text)?)))
            .collect();
        keys.sort_by_key(|(_, sc)| {
            let m = sc.modifiers;
            std::cmp::Reverse(usize::from(m.alt) + usize::from(m.command) + usize::from(m.shift))
        });
        let mut hit = None;
        for (id, sc) in keys {
            if ctx.input_mut(|i| i.consume_shortcut(&sc)) {
                hit = Some(id);
                break;
            }
        }
        match hit {
            Some(id) => self.run_action_at(id, pointer),
            None => false,
        }
    }

    /// Carry out a fullscreen request and keep the preview, sources and sync honest.
    pub(super) fn apply_view_requests(&mut self, ctx: &Context) {
        if let Some(on) = self.fullscreen_request.take() {
            self.fullscreen = on;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
        }
    }
}
