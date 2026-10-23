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
    pub(super) fn sequence_start(&self) -> u32 {
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
        use crate::kit::{self, Icon, Segment};
        let t = crate::theme::get(ctx);
        let c = t.color;
        let frames = self.transport_frames();
        let enabled = frames > 1;
        let pos = self.transport_pos();
        let movie_name = if self.movie_scope() {
            self.session
                .sequence(self.state.sequence)
                .ok()
                .map(|s| s.name.clone())
        } else {
            None
        };
        let seconds = self.playhead_seconds();
        let mut new_pos = pos;
        let mut fps = self.session.project().fps;
        let mut fps_commit = false;
        let mut scope_click = None;
        let movie = self.movie_scope();
        let has_sequences = self.session.has_sequences();
        egui::TopBottomPanel::bottom("transport")
            .frame(
                egui::Frame::NONE
                    .fill(c.surface)
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(ctx, |ui| {
                let over_bar = ui.rect_contains_pointer(ui.max_rect());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = t.space.xs;
                    let small = t.control.height - 4.0;
                    if kit::icon_button(ui, Icon::First, "First frame", false, enabled, small)
                        .clicked()
                    {
                        new_pos = 0;
                        self.play_forward = true;
                    }
                    if kit::icon_button(ui, Icon::Previous, "Previous frame", false, enabled, small)
                        .clicked()
                    {
                        new_pos = pos.saturating_sub(1);
                    }
                    let (glyph, label) = if self.state.playing {
                        (Icon::Pause, "Pause")
                    } else {
                        (Icon::Play, "Play")
                    };
                    if kit::round_button(ui, glyph, label, enabled, t.control.height + 4.0)
                        .clicked()
                    {
                        self.state.playing = !self.state.playing;
                        self.play_forward = true;
                    }
                    if kit::icon_button(ui, Icon::Next, "Next frame", false, enabled, small)
                        .clicked()
                    {
                        new_pos = (pos + 1).min(frames.saturating_sub(1));
                    }
                    if kit::icon_button(ui, Icon::Last, "Last frame", false, enabled, small)
                        .clicked()
                    {
                        new_pos = frames.saturating_sub(1);
                    }
                    ui.add_space(t.space.md);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = t.space.xs;
                        let scopes = [
                            Segment::text("Sequence").tip("Preview the selected sequence"),
                            Segment::text("Movie").tip("Preview the whole movie"),
                        ];
                        scope_click = kit::segmented(
                            ui,
                            &scopes,
                            usize::from(movie),
                            has_sequences,
                            t.control.compact_height,
                        );
                        ui.add_space(t.space.sm);
                        let response = ui
                            .add_sized(
                                [72.0, t.control.compact_height],
                                egui::DragValue::new(&mut fps)
                                    .range(1.0..=60.0)
                                    .speed(0.1)
                                    .max_decimals(2)
                                    .suffix(" fps"),
                            )
                            .on_hover_text("Frames per second: drag or type");
                        if response.changed() && !response.dragged() {
                            fps_commit = true;
                        }
                        if response.drag_stopped() || response.lost_focus() {
                            fps_commit = true;
                        }
                        ui.add_space(t.space.sm);
                        if kit::text_toggle(
                            ui,
                            "%",
                            "Scrub in 1% steps instead of frames",
                            self.state.scrub_percent,
                            small,
                        )
                        .clicked()
                        {
                            self.state.scrub_percent = !self.state.scrub_percent;
                        }
                        if kit::icon_button(
                            ui,
                            Icon::Swap,
                            "Ping-pong: play forward, then back",
                            self.state.ping_pong,
                            true,
                            small,
                        )
                        .clicked()
                        {
                            self.state.ping_pong = !self.state.ping_pong;
                        }
                        if kit::icon_button(ui, Icon::Loop, "Loop", self.state.looping, true, small)
                            .clicked()
                        {
                            self.state.looping = !self.state.looping;
                        }
                        ui.add_space(t.space.md);
                        // Right to left: the timecode, then the frame counter. Each reserves the
                        // width of its widest text, so the scrubber does not resize as they change.
                        let total_seconds = f64::from(frames) / fps.max(1.0);
                        fixed_width_label(
                            ui,
                            timecode(seconds),
                            &timecode(total_seconds),
                            t.text.small,
                            c.text_tertiary,
                        );
                        ui.add_space(t.space.xs);
                        let (counter, widest) = if self.state.scrub_percent {
                            (
                                format!("{}%", percent_for_frame(pos, frames)),
                                "100%".to_owned(),
                            )
                        } else {
                            (
                                format!("{} / {frames}", pos + 1),
                                format!("{frames} / {frames}"),
                            )
                        };
                        let counter = fixed_width_label(
                            ui,
                            counter,
                            &widest,
                            t.text.body,
                            if enabled { c.text } else { c.text_tertiary },
                        );
                        if let Some(name) = &movie_name {
                            counter.on_hover_text(format!("In \u{201c}{name}\u{201d}"));
                        }
                        ui.add_space(t.space.md);
                        if let Some(p) =
                            scrubber(ui, pos, frames, enabled, self.state.scrub_percent)
                        {
                            new_pos = p;
                        }
                    });
                    if over_bar && enabled {
                        let wheel = ui.input(|i| i.raw_scroll_delta.y);
                        if wheel > 0.0 {
                            new_pos = pos.saturating_sub(1);
                        } else if wheel < 0.0 {
                            new_pos = (pos + 1).min(frames.saturating_sub(1));
                        }
                    }
                });
            });
        if fps_commit && (fps - self.session.project().fps).abs() > f64::EPSILON {
            let _ = self.run_command("movie.fps", &json!({ "fps": fps }));
        }
        if scope_click.is_some_and(|i| i != usize::from(movie)) {
            self.run_action("view.previewScope");
        }
        if new_pos != pos {
            self.set_transport_pos(new_pos);
        }
    }
}

/// A label that always takes the width of `widest`, so text that changes width inside it does
/// not move the widgets laid out after it. Digits are measured as the widest digit, because the
/// font is proportional. The text is right-aligned inside the reserved width.
fn fixed_width_label(
    ui: &mut egui::Ui,
    text: String,
    widest: &str,
    size: f32,
    color: egui::Color32,
) -> egui::Response {
    let font = egui::FontId::proportional(size);
    let width = ui.fonts(|f| {
        let measure = |s: String| f.layout_no_wrap(s, font.clone(), color).size().x;
        let digit = ('0'..='9')
            .map(|d| (d, measure(d.to_string())))
            .fold(('0', 0.0_f32), |a, b| if b.1 > a.1 { b } else { a })
            .0;
        let widest: String = widest
            .chars()
            .map(|ch| if ch.is_ascii_digit() { digit } else { ch })
            .collect();
        measure(widest).max(measure(
            text.chars()
                .map(|ch| if ch.is_ascii_digit() { digit } else { ch })
                .collect(),
        ))
    });
    let height = ui.spacing().interact_size.y;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width.ceil() + 1.0, height), egui::Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text.clone()));
    ui.painter().text(
        egui::pos2(rect.right(), rect.center().y),
        egui::Align2::RIGHT_CENTER,
        text,
        font,
        color,
    );
    response
}

/// The frame a point `fraction` (0 to 1) along the scrubber stands for. In percent mode the
/// position snaps to whole percents first.
pub(crate) fn frame_at_fraction(fraction: f32, frames: u32, percent: bool) -> u32 {
    let f = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if percent {
        return frame_for_percent((f * 100.0).round() as u32, frames);
    }
    let last = frames.saturating_sub(1);
    ((f * last as f32).round() as u32).min(last)
}

/// The scrubber: a track with the played part in the focus colour, a tick per frame when they
/// fit, and a knob at the playhead. Returns a new position when it was clicked or dragged.
fn scrubber(ui: &mut egui::Ui, pos: u32, frames: u32, enabled: bool, percent: bool) -> Option<u32> {
    let t = crate::theme::get(ui.ctx());
    let c = t.color;
    let width = ui.available_width().max(80.0);
    let sense = if enabled {
        egui::Sense::click_and_drag()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 28.0), sense);
    let last = frames.saturating_sub(1).max(1);
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::Slider, enabled, "Frame");
        info.value = Some(f64::from(pos + 1));
        info
    });
    let inner = rect.shrink2(Vec2::new(7.0, 0.0));
    let track_y = rect.center().y - 3.0;
    let track = Rect::from_min_max(
        Pos2::new(inner.left(), track_y - 2.0),
        Pos2::new(inner.right(), track_y + 2.0),
    );
    ui.painter()
        .rect_filled(track, egui::CornerRadius::same(2), c.line);
    let fraction = if enabled {
        pos as f32 / last as f32
    } else {
        0.0
    };
    let knob_x = inner.left() + inner.width() * fraction;
    if enabled {
        let played = Rect::from_min_max(track.min, Pos2::new(knob_x, track.max.y));
        ui.painter()
            .rect_filled(played, egui::CornerRadius::same(2), c.focus);
    }
    if enabled && frames > 1 && inner.width() / frames as f32 >= 4.0 {
        for i in 0..frames {
            let x = inner.left() + inner.width() * i as f32 / last as f32;
            ui.painter().vline(
                x,
                (track.bottom() + 4.0)..=(track.bottom() + 8.0),
                egui::Stroke::new(1.0_f32, c.tick),
            );
        }
    }
    if enabled {
        let hot = response.hovered() || response.dragged();
        let knob = Pos2::new(knob_x, track.center().y);
        ui.painter().circle_filled(
            knob + Vec2::new(0.0, 1.0),
            7.0,
            egui::Color32::from_black_alpha(110),
        );
        ui.painter()
            .circle_filled(knob, if hot { 8.0 } else { 7.0 }, c.text);
    }
    let pointer = response.interact_pointer_pos()?;
    if !(response.clicked() || response.dragged()) || inner.width() <= 0.0 {
        return None;
    }
    let f = (pointer.x - inner.left()) / inner.width();
    Some(frame_at_fraction(f, frames, percent))
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
    fn the_scrubber_maps_its_width_onto_the_frames() {
        assert_eq!(frame_at_fraction(0.0, 12, false), 0);
        assert_eq!(frame_at_fraction(1.0, 12, false), 11);
        assert_eq!(frame_at_fraction(0.5, 11, false), 5);
        assert_eq!(frame_at_fraction(-3.0, 12, false), 0);
        assert_eq!(frame_at_fraction(9.0, 12, false), 11);
        assert_eq!(frame_at_fraction(f32::NAN, 12, false), 0);
        assert_eq!(frame_at_fraction(0.7, 1, false), 0);
        assert_eq!(frame_at_fraction(0.7, 0, false), 0);
        assert_eq!(frame_at_fraction(0.504, 101, true), 50);
    }

    #[test]
    fn timecode_formats_and_survives_nonsense() {
        assert_eq!(timecode(0.0), "0:00.00");
        assert_eq!(timecode(61.5), "1:01.50");
        assert_eq!(timecode(f64::NAN), "0:00.00");
        assert_eq!(timecode(-3.0), "0:00.00");
    }
}
