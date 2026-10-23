//! The Export dialogs: one window shape for every export command. The state and the checks live
//! in [`crate::export_dialog`]; this draws them and runs the command (a long one on its own
//! thread, with the progress window in `export_job_ui.rs`).

use super::*;
use crate::export_dialog::{Content, ExportDialog, Facts, FrameBy, Kind};
use crate::kit::{self, ButtonKind, Icon, Segment};
use crate::theme::Theme;
use egui::{Color32, CornerRadius, Frame, Margin, Pos2, Rect, Sense, Stroke, Vec2};
use std::path::{Path, PathBuf};

const WIDTH: f32 = 520.0;
const NUMBER_WIDTH: f32 = 72.0;
/// The picture of the chosen frame, and the page preview, beside their controls.
const THUMB: Vec2 = Vec2::new(112.0, 84.0);
/// The longest side of the picture rendered for the frame picker.
const THUMB_SIDE: u32 = 168;

/// What a click in the dialog asked for.
#[derive(Default)]
struct Clicks {
    change_folder: bool,
    change_dots: bool,
    create_folder: bool,
    browse_ffmpeg: bool,
    recheck_ffmpeg: bool,
    frames_instead: bool,
    export: bool,
    cancel: bool,
}

/// How a message under a field reads.
#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Danger,
    Warn,
}

impl InbetweenApp {
    pub fn export_dialog(&self) -> Option<&ExportDialog> {
        self.export_dialog.as_ref()
    }

    pub fn export_dialog_mut(&mut self) -> Option<&mut ExportDialog> {
        self.export_dialog.as_mut()
    }

    /// What the dialog needs to know about the open project.
    pub fn export_facts(&self) -> Facts {
        let project = self.session.project();
        let path = self.session.path();
        Facts {
            sequences: project
                .sequences
                .iter()
                .map(|s| (s.name.clone(), s.total_frames()))
                .collect(),
            total_frames: project.total_frames(),
            size: (project.output.width, project.output.height),
            fps: project.fps,
            has_sound: project.sound.is_some(),
            current: self.state.sequence,
            frame: self.state.frame,
            stem: path
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            dir: path
                .and_then(|p| p.parent())
                .map(|d| d.display().to_string())
                .unwrap_or_default(),
        }
    }

    pub(super) fn open_export(&mut self, kind: Kind) {
        let facts = self.export_facts();
        let mut d = ExportDialog::new(kind, &facts);
        if let Some(memory) = self.state.export_memory.get(kind.id()) {
            d.apply_memory(memory);
        }
        if kind == Kind::Video {
            d.ffmpeg_found = ffmpeg_works("ffmpeg");
        }
        self.export_thumb = None;
        self.export_dialog = Some(d);
    }

    /// Run the export the dialog describes (Export does the same). Does nothing until it is
    /// ready.
    pub fn submit_export(&mut self) {
        let Some(d) = self.export_dialog.as_ref() else {
            return;
        };
        let (id, kind, memory) = (d.kind.id(), d.kind, d.remember());
        let params = match d.to_params() {
            Ok(p) => p,
            Err(e) => {
                if let Some(d) = self.export_dialog.as_mut() {
                    d.error = Some(e);
                }
                return;
            }
        };
        if LONG_EXPORTS.contains(&id) {
            // Runs on a thread with a progress window; its result arrives as a toast.
            if self.start_export(id, &params) {
                self.state.export_memory.insert(id.to_string(), memory);
                self.export_dialog = None;
            }
            return;
        }
        match self.run_command(id, &params) {
            Ok(report) => {
                self.state.export_memory.insert(id.to_string(), memory);
                self.export_dialog = None;
                self.offer_output(
                    export_folder(kind.id(), &report),
                    export_file(kind.id(), &report),
                );
            }
            Err(e) => {
                if let Some(d) = self.export_dialog.as_mut() {
                    d.error = Some(friendly_error(&e.to_string()));
                }
            }
        }
    }

    /// The small picture of the frame the picker is on, rendered again only when it changes.
    fn export_thumb_for(
        &mut self,
        ctx: &Context,
        d: &ExportDialog,
        facts: &Facts,
    ) -> Option<(egui::TextureId, [usize; 2])> {
        if !matches!(d.kind, Kind::Frame | Kind::FrameDots) {
            return None;
        }
        let n = d.movie_frame_shown(facts)?;
        if self
            .export_thumb
            .as_ref()
            .is_none_or(|(have, _, _)| *have != n)
        {
            let image = self.session.movie_thumbnail(n, THUMB_SIDE).ok()?;
            let size = [image.width() as usize, image.height() as usize];
            let rgba = image.to_rgba8();
            if rgba.len() != size[0].saturating_mul(size[1]).saturating_mul(4) {
                return None;
            }
            let colour = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
            let texture = ctx.load_texture("export_frame_pick", colour, TextureOptions::LINEAR);
            self.export_thumb = Some((n, texture, size));
        }
        self.export_thumb
            .as_ref()
            .map(|(_, t, size)| (t.id(), *size))
    }

    pub(super) fn export_dialog_window(&mut self, ctx: &Context) {
        let Some(mut d) = self.export_dialog.take() else {
            return;
        };
        let facts = self.export_facts();
        let thumb = self.export_thumb_for(ctx, &d, &facts);
        let ready = d.ready();
        let note = d.note(&facts);
        let waiting_on_ffmpeg = d.kind == Kind::Video && !d.ffmpeg_found;
        let action = d.action_label(&facts);
        let subtitle = d.subtitle(&facts);
        let max_body = (ctx.screen_rect().height() - 300.0).max(240.0);
        let (mut clicks, (export, cancel), escape) = kit::dialog(
            ctx,
            "export_dialog",
            d.kind.title(),
            &subtitle,
            WIDTH,
            |ui, t| {
                let mut clicks = Clicks::default();
                // Scroll only when last frame's content did not fit: a scroll area always
                // starts with a bar that animates away, which makes the fields jump.
                let height_id = egui::Id::new("export_body_height");
                let last = ui.ctx().data(|d| d.get_temp::<f32>(height_id));
                let scrolls = last.is_some_and(|h| h > max_body);
                let mut draw = |ui: &mut egui::Ui| {
                    let top = ui.cursor().top();
                    body(ui, t, &mut d, &facts, thumb, &mut clicks);
                    let height = ui.cursor().top() - top;
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(height_id, height));
                    if (height > max_body) != scrolls {
                        // The content changed size: draw again with or without the scroll area.
                        ui.ctx().request_repaint();
                    }
                };
                if scrolls {
                    egui::ScrollArea::vertical()
                        .max_height(max_body)
                        .auto_shrink([false, true])
                        .show(ui, draw);
                } else {
                    draw(ui);
                }
                clicks
            },
            |ui, t| {
                if waiting_on_ffmpeg {
                    ui.label(
                        egui::RichText::new("Needs ffmpeg")
                            .size(t.text.label)
                            .color(t.color.warn_text),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(&note)
                            .size(t.text.label)
                            .color(t.color.text_tertiary),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let export =
                        kit::button(ui, ButtonKind::Primary, &action, ready, t.control.height)
                            .clicked();
                    let cancel =
                        kit::button(ui, ButtonKind::Quiet, "Cancel", true, t.control.height)
                            .clicked();
                    (export, cancel)
                })
                .inner
            },
        );
        clicks.export = export;
        clicks.cancel = cancel;

        if clicks.change_folder {
            let mut dialog = rfd::FileDialog::new();
            if Path::new(d.dir.trim()).is_dir() {
                dialog = dialog.set_directory(d.dir.trim());
            }
            if let Some(dir) = dialog.pick_folder() {
                d.set_dir(dir.display().to_string());
            }
        }
        if clicks.create_folder {
            if let Some(dir) = d.missing_folder() {
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    d.error = Some(friendly_error(&format!("{}: {e}", dir.display())));
                }
            }
        }
        if clicks.change_dots {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Dots", &["inbetweendots"])
                .save_file()
            {
                d.dots_path = p.display().to_string();
            }
        }
        if clicks.browse_ffmpeg {
            if let Some(p) = rfd::FileDialog::new().pick_file() {
                d.ffmpeg = p.display().to_string();
                d.ffmpeg_found = ffmpeg_works(&d.ffmpeg);
            }
        }
        if clicks.recheck_ffmpeg {
            let program = if d.ffmpeg.trim().is_empty() {
                "ffmpeg"
            } else {
                d.ffmpeg.trim()
            };
            d.ffmpeg_found = ffmpeg_works(program);
        }
        let enter = ready && ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let instead = clicks
            .frames_instead
            .then(|| (d.dir.clone(), d.stem.clone()));
        self.export_dialog = Some(d);
        if let Some((dir, stem)) = instead {
            // The same place and name, as numbered pictures.
            self.open_export(Kind::MovieImages);
            if let Some(d) = self.export_dialog.as_mut() {
                if !dir.is_empty() {
                    d.set_dir(&dir);
                }
                if !stem.is_empty() {
                    d.set_stem(format!("{stem} frames"));
                }
            }
        } else if escape || clicks.cancel {
            self.export_dialog = None;
        } else if clicks.export || enter {
            self.submit_export();
        }
    }
}

/// The folder a finished export is in, for "Show in folder".
pub fn export_folder(id: &str, report: &Value) -> Option<PathBuf> {
    if !id.starts_with("export.") && id != "frame.exportEdit" {
        return None;
    }
    if let Some(dir) = report["dir"].as_str().filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let out = PathBuf::from(report["out"].as_str().filter(|o| !o.is_empty())?);
    out.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(PathBuf::from)
}

/// The file a finished export wrote, which "Show in folder" selects. Numbered frames have none.
pub fn export_file(id: &str, report: &Value) -> Option<PathBuf> {
    if !id.starts_with("export.") && id != "frame.exportEdit" {
        return None;
    }
    if report["dir"].as_str().is_some_and(|d| !d.is_empty()) {
        return None;
    }
    report["out"]
        .as_str()
        .filter(|o| !o.is_empty())
        .map(PathBuf::from)
}

/// A command's error in plain words, with what to do about it.
pub fn friendly_error(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("os error 3") || lower.contains("cannot find the path") {
        "The folder doesn’t exist. Choose another, or create it.".to_string()
    } else if lower.contains("os error 5") || lower.contains("denied") {
        "Inbetween isn’t allowed to write there. Choose another folder.".to_string()
    } else if lower.contains("os error 112") || lower.contains("no space") {
        "The disk is full. Free some space or choose another drive.".to_string()
    } else if lower.contains("os error 32") || lower.contains("being used by another") {
        "Another program is using that file. Close it, or choose another name.".to_string()
    } else {
        raw.to_string()
    }
}

/// Whether `program -version` starts, i.e. the video encoder can be used.
fn ffmpeg_works(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("-version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

// ---------------------------------------------------------------------------- the body

fn body(
    ui: &mut egui::Ui,
    t: &Theme,
    d: &mut ExportDialog,
    facts: &Facts,
    thumb: Option<(egui::TextureId, [usize; 2])>,
    clicks: &mut Clicks,
) {
    ui.spacing_mut().item_spacing.y = t.space.sm;
    match d.kind {
        Kind::Frame => {
            kit::group_title(ui, "Output");
            name_block(ui, t, d, clicks);
            picture_format(ui, t, d);
            gap(ui, t);
            kit::group_title(ui, "Frame");
            frame_picker(ui, t, d, facts, thumb);
            sequence_section(ui, t, d, facts);
        }
        Kind::FrameDots => {
            kit::group_title(ui, "Output");
            name_block(ui, t, d, clicks);
            dots_line(ui, t, d, clicks);
            gap(ui, t);
            kit::group_title(ui, "Frame");
            frame_picker(ui, t, d, facts, thumb);
            sequence_section(ui, t, d, facts);
        }
        Kind::MovieImages | Kind::SequenceImages => {
            kit::group_title(ui, "Output");
            name_block(ui, t, d, clicks);
            prefix_section(ui, t, d);
            if d.kind == Kind::SequenceImages {
                sequence_section(ui, t, d, facts);
            }
            gap(ui, t);
            kit::group_title(ui, "Frames");
            range_section(ui, t, d, facts);
        }
        Kind::Animation => {
            kit::group_title(ui, "Output");
            name_block(ui, t, d, clicks);
            format_section(
                ui,
                t,
                d,
                &[("gif", "GIF"), ("apng", "APNG"), ("webp", "WebP")],
            );
            gap(ui, t);
            kit::group_title(ui, "Playback");
            speed_section(ui, t, d);
            card(ui, t, |ui| {
                kit::switch_row(ui, "Repeat forever", "Off plays it once", d.repeat)
                    .clicked()
                    .then(|| d.repeat = !d.repeat);
                divider(ui, t);
                kit::switch_row(
                    ui,
                    "Keep transparency",
                    if d.format == "gif" {
                        "GIF keeps it on or off, with no soft edges"
                    } else {
                        ""
                    },
                    d.transparent,
                )
                .clicked()
                .then(|| d.transparent = !d.transparent);
            });
            if d.uses_matte() {
                matte_section(ui, t, d, "Fills transparent areas");
            }
            gap(ui, t);
            kit::group_title(ui, "Frames");
            range_section(ui, t, d, facts);
        }
        Kind::Video => {
            kit::group_title(ui, "Output");
            name_block(ui, t, d, clicks);
            format_section(
                ui,
                t,
                d,
                &[("mp4", "MP4"), ("webm", "WebM"), ("prores", "ProRes")],
            );
            if !d.ffmpeg_found {
                gap(ui, t);
                ffmpeg_missing(ui, t, clicks);
            }
            gap(ui, t);
            kit::group_title(ui, "Playback");
            speed_section(ui, t, d);
            if facts.has_sound {
                card(ui, t, |ui| {
                    kit::switch_row(ui, "Include sound", "The movie has a sound track", d.audio)
                        .clicked()
                        .then(|| d.audio = !d.audio);
                });
            }
            gap(ui, t);
            kit::group_title(ui, "Frames");
            range_section(ui, t, d, facts);
        }
        Kind::Sheet => {
            sheet_section(ui, t, d, facts, clicks);
        }
    }

    if more_wanted(d) {
        gap(ui, t);
        if disclosure(ui, t, "More options", d.more).clicked() {
            d.more = !d.more;
        }
        if d.more {
            ui.add_space(t.space.sm);
            more_section(ui, t, d, facts, clicks);
        }
    }

    let message = d.error.clone().or_else(|| d.other_problem());
    if let Some(m) = message {
        ui.add_space(t.space.md);
        message_line(ui, t, Tone::Danger, &m);
    }
}

fn more_wanted(d: &ExportDialog) -> bool {
    d.kind != Kind::FrameDots
}

fn gap(ui: &mut egui::Ui, t: &Theme) {
    ui.add_space(t.space.xl - t.space.sm);
}

fn hint(ui: &mut egui::Ui, t: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(t.text.small)
            .color(t.color.text_tertiary),
    );
}

/// A line under a field: a mark, then what is wrong or worth knowing, in the field's colour.
fn message_line(ui: &mut egui::Ui, t: &Theme, tone: Tone, text: &str) {
    let colour = match tone {
        Tone::Danger => t.color.danger_text,
        Tone::Warn => t.color.warn_text,
    };
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.sm;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
        kit::icon(
            ui,
            Rect::from_center_size(
                Pos2::new(rect.center().x, rect.top() + 8.0),
                Vec2::splat(14.0),
            ),
            Icon::Alert,
            colour,
            1.6,
        );
        ui.add(egui::Label::new(egui::RichText::new(text).size(t.text.small).color(colour)).wrap());
    });
}

/// A small text button in the accent colour.
fn link(ui: &mut egui::Ui, t: &Theme, label: &str) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        t.medium(t.text.small),
        t.color.accent_text,
    );
    let size = Vec2::new(
        galley.size().x + t.space.sm * 2.0,
        t.control.compact_height - t.space.xs,
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(t.radius.control),
            t.color.field_hover,
        );
    }
    ui.painter().galley(
        Pos2::new(
            rect.left() + t.space.sm,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        t.color.accent_text,
    );
    response
}

/// A bordered group on the app background, for a few switches. Each row pads itself.
fn card<R>(ui: &mut egui::Ui, t: &Theme, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Frame::NONE
        .fill(t.color.app)
        .stroke(Stroke::new(1.0_f32, t.color.line))
        .corner_radius(CornerRadius::same(t.radius.card))
        .inner_margin(Margin::same(1))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui)
        })
        .inner
}

fn divider(ui: &mut egui::Ui, t: &Theme) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0_f32, t.color.line),
    );
}

/// A field `width` wide.
fn narrow<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        ui.set_max_width(width);
        ui.set_width(width);
        add(ui)
    })
    .inner
}

/// A short text field.
fn number_box(ui: &mut egui::Ui, text: &mut String, label: &str) -> egui::Response {
    let r = narrow(ui, NUMBER_WIDTH, |ui| kit::text_field(ui, text, ""));
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, label));
    r
}

/// A short text field with a unit or limit after it.
fn number(ui: &mut egui::Ui, t: &Theme, text: &mut String, label: &str, unit: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.md;
        number_box(ui, text, label);
        if !unit.is_empty() {
            hint(ui, t, unit);
        }
    });
}

/// A text field with a fixed ending inside its box (the file's extension), in the colour of its
/// state.
fn suffix_field(
    ui: &mut egui::Ui,
    t: &Theme,
    text: &mut String,
    suffix: &str,
    label: &str,
    tone: Option<Tone>,
) -> egui::Response {
    let line = match tone {
        Some(Tone::Danger) => t.color.danger_line,
        Some(Tone::Warn) => t.color.warn_line,
        None => t.color.line,
    };
    let outer = Frame::NONE
        .fill(t.color.field)
        .stroke(Stroke::new(1.0_f32, line))
        .corner_radius(CornerRadius::same(t.radius.control))
        .inner_margin(Margin::symmetric(t.space.md as i8, 0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(t.control.height - 2.0);
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = t.space.xs;
                let suffix_w = ui
                    .painter()
                    .layout_no_wrap(
                        suffix.to_string(),
                        t.regular(t.text.body),
                        Color32::PLACEHOLDER,
                    )
                    .size()
                    .x;
                let edit = ui.add(
                    egui::TextEdit::singleline(text)
                        .frame(false)
                        .font(t.regular(t.text.body))
                        .text_color(t.color.text)
                        .desired_width((ui.available_width() - suffix_w - t.space.xs).max(40.0)),
                );
                edit.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, label)
                });
                ui.label(
                    egui::RichText::new(suffix)
                        .size(t.text.body)
                        .color(t.color.text_tertiary),
                );
                edit
            })
            .inner
        });
    if outer.inner.has_focus() {
        ui.painter().rect_stroke(
            outer.response.rect,
            CornerRadius::same(t.radius.control),
            Stroke::new(2.0_f32, t.color.focus),
            egui::StrokeKind::Outside,
        );
    }
    outer.inner
}

fn pick(ui: &mut egui::Ui, t: &Theme, labels: &[&str], selected: usize) -> Option<usize> {
    let segments: Vec<Segment<'_>> = labels.iter().map(|l| Segment::text(l)).collect();
    if labels.len() <= 3 {
        kit::segmented_fill(ui, &segments, selected, true, t.control.height)
    } else {
        kit::segmented(ui, &segments, selected, true, t.control.height)
    }
}

fn disclosure(ui: &mut egui::Ui, t: &Theme, label: &str, open: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        t.medium(t.text.label),
        t.color.text_secondary,
    );
    let size = egui::vec2(
        16.0 + t.space.sm + galley.size().x,
        t.control.compact_height,
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, open, label));
    let fg = if response.hovered() || response.has_focus() {
        t.color.text
    } else {
        t.color.text_secondary
    };
    let glyph = Rect::from_center_size(
        Pos2::new(rect.left() + 8.0, rect.center().y),
        egui::vec2(14.0, 14.0),
    );
    kit::icon(
        ui,
        glyph,
        if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        },
        fg,
        2.0,
    );
    ui.painter().galley(
        Pos2::new(
            rect.left() + 16.0 + t.space.sm,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        fg,
    );
    response
}

/// `path` cut from the front so its end stays: "…\Renders\demo".
fn clip_front(path: &str, keep: usize) -> String {
    let count = path.chars().count();
    if count <= keep + 1 {
        return path.to_string();
    }
    let tail: String = path.chars().skip(count - keep).collect();
    format!("…{tail}")
}

// -------------------------------------------------------------------------- the sections

/// The file's name with its ending after it, what is wrong with it, and the folder it goes in.
fn name_block(ui: &mut egui::Ui, t: &Theme, d: &mut ExportDialog, clicks: &mut Clicks) {
    let label = if d.kind.is_folder() {
        "Folder name"
    } else {
        "File name"
    };
    kit::field_label(ui, label, None);
    let missing = d.missing_folder();
    let problem = d.problem();
    let replaces = d.replaces();
    let tone = if problem.is_some() || missing.is_some() {
        Some(Tone::Danger)
    } else if replaces.is_some() {
        Some(Tone::Warn)
    } else {
        None
    };
    let mut stem = d.stem.clone();
    let suffix = d.ext_suffix();
    suffix_field(ui, t, &mut stem, &suffix, label, tone);
    if stem != d.stem {
        d.set_stem(stem);
    }
    if let Some(p) = &problem {
        message_line(ui, t, Tone::Danger, p);
    } else if let Some(r) = &replaces {
        message_line(ui, t, Tone::Warn, r);
    }
    if let Some(dir) = missing {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.display().to_string());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = t.space.xs;
            ui.vertical(|ui| {
                message_line(
                    ui,
                    t,
                    Tone::Danger,
                    &format!("The folder “{name}” doesn’t exist."),
                );
            });
            if link(ui, t, "Create folder").clicked() {
                clicks.create_folder = true;
            }
        });
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.xs;
        if d.dir.trim().is_empty() {
            hint(ui, t, "No folder chosen yet.");
        } else {
            hint(ui, t, "in");
            ui.label(
                egui::RichText::new(clip_front(&d.dir, 40))
                    .font(egui::FontId::monospace(t.text.mono))
                    .color(t.color.text_secondary),
            );
        }
        if link(ui, t, "Change…").clicked() {
            clicks.change_folder = true;
        }
    });
}

/// PNG or JPEG, for a single picture.
fn picture_format(ui: &mut egui::Ui, t: &Theme, d: &mut ExportDialog) {
    let selected = usize::from(d.is_jpeg());
    if let Some(i) = pick(ui, t, &["PNG", "JPEG"], selected) {
        d.set_picture_format(i == 1);
    }
}

/// Where the dots file goes, as a sentence with a way to change it.
fn dots_line(ui: &mut egui::Ui, t: &Theme, d: &ExportDialog, clicks: &mut Clicks) {
    let beside = if d.dots_path.trim().is_empty() {
        format!("{}.inbetweendots", d.stem.trim())
    } else {
        Path::new(d.dots_path.trim())
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.xs;
        hint(
            ui,
            t,
            &if d.dots_path.trim().is_empty() {
                format!("Dots saved beside it as {beside}")
            } else {
                format!("Dots saved as {beside}")
            },
        );
        if link(ui, t, "Change dots file…").clicked() {
            clicks.change_dots = true;
        }
    });
}

/// The frame to export: a picture of it, a slider that snaps to frames, and its number.
fn frame_picker(
    ui: &mut egui::Ui,
    t: &Theme,
    d: &mut ExportDialog,
    facts: &Facts,
    thumb: Option<(egui::TextureId, [usize; 2])>,
) {
    let count = u32::try_from(d.scope_frames(facts))
        .unwrap_or(u32::MAX)
        .max(1);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.lg;
        let (rect, _) = ui.allocate_exact_size(THUMB, Sense::hover());
        let card = CornerRadius::same(t.radius.control);
        ui.painter().rect_filled(rect, card, t.color.app);
        if let Some((id, [w, h])) = thumb {
            let scale = (THUMB.x / w.max(1) as f32).min(THUMB.y / h.max(1) as f32);
            let shown = Rect::from_center_size(
                rect.center(),
                Vec2::new(w as f32 * scale, h as f32 * scale),
            );
            ui.painter().image(
                id,
                shown,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            kit::icon(
                ui,
                Rect::from_center_size(rect.center(), Vec2::splat(24.0)),
                Icon::Image,
                t.color.text_disabled,
                1.6,
            );
        }
        ui.painter().rect_stroke(
            rect,
            card,
            Stroke::new(1.0_f32, t.color.line),
            egui::StrokeKind::Inside,
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = t.space.sm;
            let mut n = d.picked_frame().unwrap_or(1).clamp(1, count);
            ui.spacing_mut().slider_width = ui.available_width();
            let before = n;
            ui.add(egui::Slider::new(&mut n, 1..=count).show_value(false))
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, "Frame"));
            if n != before {
                d.set_frame(n, count);
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = t.space.sm;
                hint(ui, t, "Frame");
                let mut text = d.frame.clone();
                number_box(ui, &mut text, "Frame number");
                if text != d.frame {
                    d.frame = text;
                    if let Some(k) = d.picked_frame().filter(|k| (1..=count).contains(k)) {
                        d.set_frame(k, count);
                    }
                }
                hint(ui, t, &format!("of {count}"));
            });
            let text = match d.frame_by {
                FrameBy::Frame => format!("Position {:.2} between the two images.", d.position),
                FrameBy::Position => {
                    format!(
                        "An exact position, {:.3}. Move the slider to use a frame.",
                        d.position
                    )
                }
                FrameBy::Movie => "A frame of the whole movie, from More options.".to_string(),
            };
            hint(ui, t, &text);
        });
    });
}

fn sequence_name(facts: &Facts, i: usize) -> String {
    match facts.sequences.get(i) {
        Some((name, _)) if !name.trim().is_empty() => format!("Sequence {}: {name}", i + 1),
        _ => format!("Sequence {}", i + 1),
    }
}

/// Which sequence a command acts on. Left out when the project has just one.
fn sequence_section(ui: &mut egui::Ui, t: &Theme, d: &mut ExportDialog, facts: &Facts) {
    if facts.sequences.len() < 2 {
        return;
    }
    kit::field_label(ui, "Sequence", None);
    sequence_combo(ui, t, d, facts);
}

fn sequence_combo(ui: &mut egui::Ui, t: &Theme, d: &mut ExportDialog, facts: &Facts) {
    let text = match d.sequence {
        Some(i) => sequence_name(facts, i),
        None => format!("Whole movie ({} sequences)", facts.sequences.len()),
    };
    ui.spacing_mut().interact_size.y = t.control.height;
    egui::ComboBox::from_id_salt("export_sequence")
        .width(ui.available_width())
        .selected_text(egui::RichText::new(text).size(t.text.body))
        .show_ui(ui, |ui| {
            if !d.kind.needs_sequence() {
                ui.selectable_value(
                    &mut d.sequence,
                    None,
                    format!("Whole movie ({} sequences)", facts.sequences.len()),
                );
            }
            for i in 0..facts.sequences.len() {
                ui.selectable_value(&mut d.sequence, Some(i), sequence_name(facts, i));
            }
        });
}

fn prefix_section(ui: &mut egui::Ui, t: &Theme, d: &mut ExportDialog) {
    kit::field_label(ui, "File names", None);
    suffix_field(
        ui,
        t,
        &mut d.prefix,
        "_0001.png",
        "File names start with",
        None,
    );
    hint(ui, t, "Each frame’s number is added to the start.");
}

fn range_section(ui: &mut egui::Ui, t: &Theme, d: &mut ExportDialog, facts: &Facts) {
    if !d.shows_range() {
        return;
    }
    let n = d.scope_frames(facts);
    let all = format!("All {n} frames");
    match pick(
        ui,
        t,
        &[all.as_str(), "Some frames"],
        usize::from(!d.all_frames),
    ) {
        Some(0) => d.all_frames = true,
        Some(1) => {
            d.all_frames = false;
            if d.first.trim().is_empty() {
                d.first = "1".into();
            }
            if d.last.trim().is_empty() {
                d.last = n.to_string();
            }
        }
        _ => {}
    }
    if !d.all_frames {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = t.space.sm;
            hint(ui, t, "Frames");
            number_box(ui, &mut d.first, "First frame");
            hint(ui, t, "to");
            number_box(ui, &mut d.last, "Last frame");
            hint(ui, t, &format!("of {n}"));
        });
        if let Some(p) = d.range_problem() {
            message_line(ui, t, Tone::Danger, &p);
        }
    }
}

fn format_section(ui: &mut egui::Ui, t: &Theme, d: &mut ExportDialog, formats: &[(&str, &str)]) {
    let labels: Vec<&str> = formats.iter().map(|f| f.1).collect();
    let selected = formats.iter().position(|f| f.0 == d.format).unwrap_or(0);
    if let Some(i) = pick(ui, t, &labels, selected) {
        if let Some((id, _)) = formats.get(i) {
            d.set_format(id);
        }
    }
    let note = match d.format.as_str() {
        "gif" => "Plays everywhere. 256 colours; transparency is on or off.",
        "apng" => "Full colour and soft transparency. Most browsers play it.",
        "webp" => "Full colour, soft transparency and the smallest files.",
        "webm" => "VP9 with transparency.",
        "prores" => "ProRes 4444 with transparency, for editing.",
        _ => "H.264. Plays everywhere, with no transparency.",
    };
    hint(ui, t, note);
}

fn speed_section(ui: &mut egui::Ui, t: &Theme, d: &mut ExportDialog) {
    kit::field_label(ui, "Speed", None);
    let default = d.default_fps;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.md;
        number_box(ui, &mut d.fps, "Speed");
        hint(ui, t, "frames a second");
        let same = d.fps.trim().parse::<f64>().ok() == Some(default);
        if !same && default > 0.0 {
            let text = format!("Reset to {default}");
            if link(ui, t, &text).clicked() {
                d.fps = format!("{default}");
            }
        }
    });
}

/// The colour transparent areas are filled with, with a swatch of it.
fn matte_section(ui: &mut egui::Ui, t: &Theme, d: &mut ExportDialog, note: &str) {
    ui.add_space(t.space.xs);
    kit::field_label(ui, "Background", None);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.md;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(t.control.height - 8.0), Sense::hover());
        let colour = parse_hex(d.matte.trim());
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(t.radius.control),
            colour.unwrap_or(t.color.field),
        );
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(t.radius.control),
            Stroke::new(1.0_f32, t.color.line),
            egui::StrokeKind::Inside,
        );
        narrow(ui, NUMBER_WIDTH + 40.0, |ui| {
            kit::text_field(ui, &mut d.matte, "#ffffff")
        })
        .widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Background colour")
        });
        hint(ui, t, note);
    });
}

fn parse_hex(text: &str) -> Option<Color32> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// The missing-encoder notice: a warning, not a fault, with a way out.
fn ffmpeg_missing(ui: &mut egui::Ui, t: &Theme, clicks: &mut Clicks) {
    Frame::NONE
        .fill(t.color.warn_fill)
        .stroke(Stroke::new(1.0_f32, t.color.warn_line))
        .corner_radius(CornerRadius::same(t.radius.card))
        .inner_margin(Margin::same(t.space.lg as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = t.space.xs;
            ui.label(
                egui::RichText::new("Video needs ffmpeg")
                    .font(t.medium(t.text.body))
                    .color(t.color.warn_text),
            );
            ui.label(
                egui::RichText::new(
                    "It isn’t installed, or Inbetween can’t find it. Install it, or point Inbetween at the program.",
                )
                .size(t.text.label)
                .color(t.color.text_secondary),
            );
            ui.add_space(t.space.sm);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = t.space.sm;
                if kit::button(
                    ui,
                    ButtonKind::Secondary,
                    "Choose ffmpeg…",
                    true,
                    t.control.compact_height,
                )
                .clicked()
                {
                    clicks.browse_ffmpeg = true;
                }
                if kit::button(
                    ui,
                    ButtonKind::Quiet,
                    "Check again",
                    true,
                    t.control.compact_height,
                )
                .clicked()
                {
                    clicks.recheck_ffmpeg = true;
                }
                if kit::button(
                    ui,
                    ButtonKind::Quiet,
                    "Export frames instead",
                    true,
                    t.control.compact_height,
                )
                .clicked()
                {
                    clicks.frames_instead = true;
                }
            });
        });
}

/// A page with the sheet's pictures laid out on it, so the settings can be judged by eye.
fn page_preview(ui: &mut egui::Ui, t: &Theme, d: &ExportDialog, facts: &Facts) {
    let landscape = d.landscape;
    let size = if landscape {
        Vec2::new(THUMB.x + 24.0, (THUMB.x + 24.0) / 1.414)
    } else {
        Vec2::new(THUMB.x - 8.0, (THUMB.x - 8.0) * 1.414)
    };
    let pictures = d.picture_total(facts).max(1);
    let columns = d.columns.trim().parse::<usize>().unwrap_or(3).clamp(1, 12);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = t.space.xs;
        let (page, _) = ui.allocate_exact_size(size, Sense::hover());
        let paper = t.color.text;
        ui.painter()
            .rect_filled(page, CornerRadius::same(t.radius.control / 2), paper);
        let inset = 7.0;
        let inner = page.shrink(inset);
        let mut y = inner.top();
        if !d.title.trim().is_empty() {
            ui.painter().rect_filled(
                Rect::from_min_size(
                    Pos2::new(inner.left(), y),
                    Vec2::new(inner.width() * 0.55, 4.0),
                ),
                CornerRadius::same(2),
                t.color.text_tertiary,
            );
            y += 9.0;
        }
        let gap = 4.0;
        let cell_w = (inner.width() - gap * (columns as f32 - 1.0)) / columns as f32;
        let cell_h = cell_w * 0.75 + if d.captions { 4.0 } else { 0.0 };
        let on_page = (pictures as usize).min(12);
        for i in 0..on_page {
            let (col, row) = (i % columns, i / columns);
            let x = inner.left() + col as f32 * (cell_w + gap);
            let top = y + row as f32 * (cell_h + gap);
            if top + cell_h > inner.bottom() + 0.5 {
                break;
            }
            ui.painter().rect_filled(
                Rect::from_min_size(Pos2::new(x, top), Vec2::new(cell_w, cell_w * 0.75)),
                CornerRadius::same(1),
                t.color.text_tertiary,
            );
            if d.captions {
                ui.painter().rect_filled(
                    Rect::from_min_size(
                        Pos2::new(x, top + cell_w * 0.75 + 1.0),
                        Vec2::new(cell_w * 0.7, 2.0),
                    ),
                    CornerRadius::same(1),
                    t.color.dash,
                );
            }
        }
        let pages = (pictures as usize).div_ceil(12).max(1);
        ui.allocate_ui(Vec2::new(size.x, t.text.small), |ui| {
            ui.set_width(size.x);
            ui.centered_and_justified(|ui| hint(ui, t, &format!("Page 1 of {pages}")));
        });
    });
}

fn sheet_section(
    ui: &mut egui::Ui,
    t: &Theme,
    d: &mut ExportDialog,
    facts: &Facts,
    clicks: &mut Clicks,
) {
    kit::group_title(ui, "Output");
    name_block(ui, t, d, clicks);
    gap(ui, t);
    kit::group_title(ui, "Pictures");
    let selected = match d.content {
        Content::Sources => 0,
        Content::Preview => 1,
        Content::Frames => 2,
    };
    match pick(
        ui,
        t,
        &["Start and end", "A few per sequence", "Every frame"],
        selected,
    ) {
        Some(0) => d.content = Content::Sources,
        Some(1) => d.content = Content::Preview,
        Some(2) => d.content = Content::Frames,
        _ => {}
    }
    match d.content {
        Content::Preview => {
            number(
                ui,
                t,
                &mut d.per_sequence,
                "Pictures per sequence",
                "pictures per sequence, evenly spaced",
            );
        }
        Content::Frames => range_section(ui, t, d, facts),
        Content::Sources => {}
    }
    gap(ui, t);
    kit::group_title(ui, "Page");
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.lg;
        page_preview(ui, t, d, facts);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = t.space.md;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = t.space.sm;
                let text = if d.letter { "Letter" } else { "A4" };
                ui.spacing_mut().interact_size.y = t.control.height;
                egui::ComboBox::from_id_salt("export_paper")
                    .width(72.0)
                    .selected_text(egui::RichText::new(text).size(t.text.body))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut d.letter, false, "A4");
                        ui.selectable_value(&mut d.letter, true, "Letter");
                    });
                match pick(ui, t, &["Portrait", "Landscape"], usize::from(d.landscape)) {
                    Some(0) => d.landscape = false,
                    Some(1) => d.landscape = true,
                    _ => {}
                }
            });
            number(ui, t, &mut d.columns, "Pictures across", "pictures across");
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = t.space.sm;
                kit::field_label(ui, "Heading", None);
                kit::text_field(ui, &mut d.title, "").widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::TextEdit,
                        true,
                        "Heading on every page",
                    )
                });
            });
        });
    });
    card(ui, t, |ui| {
        kit::switch_row(
            ui,
            "Captions",
            "What each picture is and which frame",
            d.captions,
        )
        .clicked()
        .then(|| d.captions = !d.captions);
    });
}

fn more_section(
    ui: &mut egui::Ui,
    t: &Theme,
    d: &mut ExportDialog,
    facts: &Facts,
    clicks: &mut Clicks,
) {
    let mut first = true;
    let mut space = |ui: &mut egui::Ui| {
        if !first {
            gap(ui, t);
        }
        first = false;
    };
    // Which sequence, where it is not already a main choice.
    if !d.kind.needs_sequence() && facts.sequences.len() > 1 {
        space(ui);
        kit::field_label(ui, "Sequence", None);
        sequence_combo(ui, t, d, facts);
    }
    if d.kind == Kind::Frame {
        space(ui);
        kit::field_label(ui, "Exact position", None);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = t.space.md;
            let mut p = d.position;
            narrow(ui, NUMBER_WIDTH, |ui| {
                ui.add(
                    egui::DragValue::new(&mut p)
                        .range(0.0..=1.0)
                        .speed(0.005)
                        .fixed_decimals(3),
                )
            })
            .widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, "Exact position")
            });
            if (p - d.position).abs() > f32::EPSILON {
                d.position = p;
                d.frame_by = FrameBy::Position;
            }
            hint(ui, t, "0 is the start image, 1 the end image");
        });
        if facts.sequences.len() > 1 {
            ui.add_space(t.space.sm);
            kit::field_label(ui, "Frame of the whole movie", None);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = t.space.md;
                let before = d.movie_frame.clone();
                number_box(ui, &mut d.movie_frame, "Movie frame");
                if d.movie_frame != before {
                    d.frame_by = FrameBy::Movie;
                }
                hint(ui, t, &format!("of {}", facts.total_frames));
            });
        }
    }
    if d.kind.has_direction_options() {
        space(ui);
        card(ui, t, |ui| {
            kit::switch_row(
                ui,
                "Play backwards",
                "Numbering follows the movie",
                d.reverse,
            )
            .clicked()
            .then(|| d.reverse = !d.reverse);
            divider(ui, t);
            kit::switch_row(
                ui,
                "Skip repeated joins",
                "Drops a frame that repeats where two sequences meet",
                d.drop_duplicates,
            )
            .clicked()
            .then(|| d.drop_duplicates = !d.drop_duplicates);
        });
    }
    if d.kind == Kind::Video && d.uses_matte() {
        space(ui);
        matte_section(ui, t, d, "MP4 has no transparency");
    }
    if d.is_jpeg() {
        space(ui);
        kit::field_label(ui, "JPEG quality", Some(&d.jpeg_quality.to_string()));
        ui.spacing_mut().slider_width = ui.available_width();
        let mut q = u32::from(d.jpeg_quality);
        ui.add(egui::Slider::new(&mut q, 1..=100).show_value(false))
            .widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, "JPEG quality")
            });
        d.jpeg_quality = q.clamp(1, 100) as u8;
    }
    if d.kind.has_dpi() {
        space(ui);
        kit::field_label(ui, "Resolution", None);
        number(
            ui,
            t,
            &mut d.dpi,
            "Resolution",
            "dots per inch, recorded in the file",
        );
    }
    if d.kind == Kind::Sheet {
        space(ui);
        kit::field_label(ui, "Picture size", None);
        number(ui, t, &mut d.thumb, "Picture size", "px on the long side");
        ui.add_space(t.space.sm);
        kit::field_label(ui, "JPEG quality", None);
        number(
            ui,
            t,
            &mut d.sheet_quality,
            "JPEG quality",
            "for the pictures in the PDF",
        );
    }
    if d.kind == Kind::Video {
        space(ui);
        kit::field_label(ui, "ffmpeg program", None);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = t.space.sm;
            kit::text_field(ui, &mut d.ffmpeg, "Found automatically");
        });
        if link(ui, t, "Choose ffmpeg…").clicked() {
            clicks.browse_ffmpeg = true;
        }
    }
    if d.kind.has_after_options() {
        space(ui);
        card(ui, t, |ui| {
            kit::switch_row(
                ui,
                "Open when done",
                "In your default program",
                d.open_after,
            )
            .clicked()
            .then(|| d.open_after = !d.open_after);
            divider(ui, t);
            kit::switch_row(
                ui,
                "Save the project first",
                "Only when it has unsaved changes",
                d.save_first,
            )
            .clicked()
            .then(|| d.save_first = !d.save_first);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_folder_is_where_the_output_went() {
        assert_eq!(
            export_folder("export.movie", &json!({ "dir": "C:/out/frames" })),
            Some(PathBuf::from("C:/out/frames"))
        );
        assert_eq!(
            export_folder("export.video", &json!({ "out": "C:/out/a.mp4" })),
            Some(PathBuf::from("C:/out"))
        );
        assert_eq!(
            export_folder("export.video", &json!({ "out": "a.mp4" })),
            None
        );
        assert_eq!(export_folder("export.video", &json!({})), None);
        assert_eq!(
            export_file("export.video", &json!({ "out": "C:/out/a.mp4" })),
            Some(PathBuf::from("C:/out/a.mp4"))
        );
        assert_eq!(
            export_file("export.movie", &json!({ "dir": "C:/out/frames" })),
            None
        );
        assert_eq!(
            export_folder("project.save", &json!({ "out": "C:/x/y" })),
            None
        );
    }

    #[test]
    fn raw_os_errors_become_sentences_with_a_next_step() {
        assert!(friendly_error(
            "file error: x.png: The system cannot find the path specified. (os error 3)"
        )
        .contains("doesn’t exist"));
        assert!(friendly_error("x: Access is denied. (os error 5)").contains("allowed"));
        assert!(friendly_error("x (os error 112)").contains("disk is full"));
        assert_eq!(friendly_error("something else"), "something else");
    }

    #[test]
    fn long_folders_keep_their_end() {
        assert_eq!(clip_front("C:/a", 40), "C:/a");
        let long = format!("C:/{}/demo", "folder/".repeat(20));
        let short = clip_front(&long, 40);
        assert!(short.starts_with('…') && short.ends_with("demo"));
        assert_eq!(short.chars().count(), 41);
    }

    #[test]
    fn a_background_reads_only_a_full_hex_colour() {
        assert_eq!(parse_hex("#ff0000"), Some(Color32::from_rgb(255, 0, 0)));
        assert_eq!(parse_hex("ff0000"), None);
        assert_eq!(parse_hex("#fff"), None);
    }
}
