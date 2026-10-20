//! The transport: playhead position, scrubbing and the bar under the canvases.
//!
//! The playhead has two scopes (`ViewState::preview_scope`). In `Sequence` scope it is a frame
//! of the selected sequence. In `Movie` scope it is a frame of the whole movie; in a sequence
//! project the shell selects whichever sequence the playhead is in, so every panel, curve and
//! overlay keeps working on `state.frame` as a frame of the selected sequence. In a layer
//! project every layer plays at once, so `state.frame` is the movie frame as it stands.

use super::*;
use crate::state::PreviewScope;
use inbetween_doc::ProjectType;

/// `m:ss.ff` for a time in seconds.
pub(crate) fn timecode(seconds: f64) -> String {
    let seconds = if seconds.is_finite() {
        seconds.max(0.0)
    } else {
        0.0
    };
    let minutes = (seconds / 60.0).floor();
    format!("{}:{:05.2}", minutes as u64, seconds - minutes * 60.0)
}

/// The frame a 0 to 100 scrub position stands for, in a run of `frames` frames.
pub(crate) fn frame_for_percent(percent: u32, frames: u32) -> u32 {
    let last = frames.saturating_sub(1);
    let at = (f64::from(percent.min(100)) / 100.0 * f64::from(last)).round();
    (at as u32).min(last)
}

/// The scrub position (0 to 100) of a frame in a run of `frames` frames.
pub(crate) fn percent_for_frame(frame: u32, frames: u32) -> u32 {
    let last = frames.saturating_sub(1);
    if last == 0 {
        return 0;
    }
    ((f64::from(frame.min(last)) / f64::from(last)) * 100.0).round() as u32
}

impl InbetweenApp {
    /// The preview plays the whole movie.
    pub(super) fn movie_scope(&self) -> bool {
        self.state.preview_scope == PreviewScope::Movie && self.session.has_sequences()
    }

    /// A layer project in movie scope: every layer shows its frame at once.
    pub(super) fn layer_movie(&self) -> bool {
        self.movie_scope() && self.session.project().kind == ProjectType::Layer
    }

    /// Frames in the run the transport moves through.
    pub(super) fn transport_frames(&self) -> u32 {
        if self.movie_scope() {
            u32::try_from(self.session.project().total_frames())
                .unwrap_or(u32::MAX)
                .max(1)
        } else {
            self.total_frames()
        }
    }

    /// Movie frame (counting from 0) at which the selected sequence starts in a sequence
    /// project; hidden sequences have no place in the movie and start at 0.
    fn sequence_start(&self) -> u32 {
        let project = self.session.project();
        let Some(selected) = project.sequences.get(self.state.sequence) else {
            return 0;
        };
        if !selected.visible {
            return 0;
        }
        let before: u64 = project.sequences[..self.state.sequence]
            .iter()
            .filter(|s| s.visible)
            .map(|s| u64::from(s.total_frames()))
            .sum();
        u32::try_from(before).unwrap_or(u32::MAX)
    }

    /// Where the playhead is in the run the transport moves through.
    pub(super) fn transport_pos(&self) -> u32 {
        if self.movie_scope() && !self.layer_movie() {
            self.sequence_start().saturating_add(self.state.frame)
        } else {
            self.state.frame
        }
        .min(self.transport_frames().saturating_sub(1))
    }

    /// Move the playhead. In movie scope this may select another sequence; playing carries on.
    pub(super) fn set_transport_pos(&mut self, pos: u32) {
        let pos = pos.min(self.transport_frames().saturating_sub(1));
        if self.movie_scope() && !self.layer_movie() {
            if let Some((index, local)) = self.session.project().locate(u64::from(pos)) {
                if index != self.state.sequence {
                    let playing = self.state.playing;
                    self.select_sequence(index);
                    self.state.playing = playing;
                }
                self.state.frame = local;
                return;
            }
        }
        self.state.frame = pos;
    }

    /// Step the playhead by `delta` frames, stopping at both ends.
    pub(super) fn step_transport(&mut self, delta: i64) {
        let at = i64::from(self.transport_pos()) + delta;
        let last = i64::from(self.transport_frames().saturating_sub(1));
        self.set_transport_pos(at.clamp(0, last) as u32);
    }

    /// The seconds the playhead is at, at the movie frame rate.
    fn playhead_seconds(&self) -> f64 {
        f64::from(self.transport_pos()) / self.session.project().fps.max(1.0)
    }

    pub(super) fn transport_bar(&mut self, ctx: &Context) {
        let frames = self.transport_frames();
        let enabled = frames > 1;
        let pos = self.transport_pos();
        // Just the timing, so the closure below does not borrow the session.
        let t = self
            .prepared()
            .map(|p| p.sequence.frame_time(self.state.frame));
        let movie_name = if self.movie_scope() {
            self.session
                .sequence(self.state.sequence)
                .ok()
                .map(|s| format!("{}  ", s.name))
        } else {
            None
        };
        let seconds = self.playhead_seconds();
        let mut new_pos = pos;
        let mut fps = self.session.project().fps;
        let mut fps_commit = false;
        let mut scope_toggle = false;
        egui::TopBottomPanel::bottom("transport").show(ctx, |ui| {
            let over_bar = ui.rect_contains_pointer(ui.max_rect());
            ui.horizontal(|ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    if ui.button("|<").on_hover_text("First frame").clicked() {
                        new_pos = 0;
                        self.play_forward = true;
                    }
                    if ui.button("<").on_hover_text("Previous frame").clicked() {
                        new_pos = pos.saturating_sub(1);
                    }
                    let label = if self.state.playing { "Pause" } else { "Play" };
                    if ui.button(label).clicked() {
                        self.state.playing = !self.state.playing;
                        self.play_forward = true;
                    }
                    if ui.button(">").on_hover_text("Next frame").clicked() {
                        new_pos = (pos + 1).min(frames.saturating_sub(1));
                    }
                    if ui.button(">|").on_hover_text("Last frame").clicked() {
                        new_pos = frames.saturating_sub(1);
                    }
                    ui.spacing_mut().slider_width = (ui.available_width() - 640.0).max(80.0);
                    if self.state.scrub_percent {
                        let mut percent = percent_for_frame(pos, frames);
                        let before = percent;
                        ui.add(egui::Slider::new(&mut percent, 0..=100).suffix("%"));
                        if percent != before {
                            new_pos = frame_for_percent(percent, frames);
                        }
                    } else {
                        let mut frame = pos;
                        ui.add(
                            egui::Slider::new(&mut frame, 0..=frames.saturating_sub(1))
                                .text("frame"),
                        );
                        new_pos = if frame != pos { frame } else { new_pos };
                    }
                    // The wheel over the bar steps one frame.
                    if over_bar {
                        let wheel = ui.input(|i| i.raw_scroll_delta.y);
                        if wheel > 0.0 {
                            new_pos = pos.saturating_sub(1);
                        } else if wheel < 0.0 {
                            new_pos = (pos + 1).min(frames.saturating_sub(1));
                        }
                    }
                });
                ui.label(format!(
                    "{}{} / {frames}  {}%  {}{}",
                    movie_name.clone().unwrap_or_default(),
                    pos + 1,
                    percent_for_frame(pos, frames),
                    timecode(seconds),
                    t.map_or(String::new(), |t| format!("  t = {t:.2}")),
                ));
                ui.checkbox(&mut self.state.scrub_percent, "%")
                    .on_hover_text("Scrub in 1% steps instead of frames");
                ui.checkbox(&mut self.state.looping, "Loop");
                ui.checkbox(&mut self.state.ping_pong, "Ping-pong");
                let response = ui.add(
                    egui::DragValue::new(&mut fps)
                        .range(1.0..=60.0)
                        .speed(0.1)
                        .max_decimals(2)
                        .suffix(" fps"),
                );
                if response.changed() && !response.dragged() {
                    fps_commit = true;
                }
                if response.drag_stopped() || response.lost_focus() {
                    fps_commit = true;
                }
                let scope = if self.movie_scope() {
                    "Movie"
                } else {
                    "Sequence"
                };
                if ui
                    .button(scope)
                    .on_hover_text(
                        "What the preview plays: the selected sequence or the whole movie",
                    )
                    .clicked()
                {
                    scope_toggle = true;
                }
            });
        });
        if fps_commit && (fps - self.session.project().fps).abs() > f64::EPSILON {
            let _ = self.run_command("movie.fps", &json!({ "fps": fps }));
        }
        if scope_toggle {
            self.run_action("view.previewScope");
        }
        if new_pos != pos {
            self.set_transport_pos(new_pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_and_frame_map_both_ways() {
        assert_eq!(frame_for_percent(0, 11), 0);
        assert_eq!(frame_for_percent(50, 11), 5);
        assert_eq!(frame_for_percent(100, 11), 10);
        assert_eq!(frame_for_percent(250, 11), 10);
        assert_eq!(frame_for_percent(40, 1), 0);
        assert_eq!(frame_for_percent(40, 0), 0);
        assert_eq!(percent_for_frame(5, 11), 50);
        assert_eq!(percent_for_frame(99, 11), 100);
        assert_eq!(percent_for_frame(3, 1), 0);
        for frames in [2u32, 3, 7, 24, 101, 999] {
            assert_eq!(frame_for_percent(0, frames), 0);
            assert_eq!(frame_for_percent(100, frames), frames - 1);
        }
    }

    #[test]
    fn timecode_formats_and_survives_nonsense() {
        assert_eq!(timecode(0.0), "0:00.00");
        assert_eq!(timecode(61.5), "1:01.50");
        assert_eq!(timecode(f64::NAN), "0:00.00");
        assert_eq!(timecode(-3.0), "0:00.00");
    }
}
