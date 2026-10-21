//! The New From Two Images dialog: two image slots, an optional name, and Create project.
//! The state lives in [`crate::new_two`]; this draws it and runs `project.new`.

use super::*;
use crate::kit::{self, ButtonKind, Icon};
use crate::new_two::{NewTwo, Slot};
use crate::theme::Theme;
use egui::epaint::RectShape;
use egui::text::LayoutJob;
use egui::{Color32, CornerRadius, Sense, Stroke, StrokeKind, UiBuilder};

const WIDTH: f32 = 560.0;
const SLOT_HEIGHT: f32 = 184.0;
const THUMB_HEIGHT: f32 = 120.0;
const SWAP_SIZE: f32 = 32.0;

/// What a click in the dialog asked for.
#[derive(Default)]
struct Clicks {
    browse: Option<usize>,
    clear: Option<usize>,
    swap: bool,
    create: bool,
    cancel: bool,
}

/// The slot thumbnails as textures, keyed by path so a swap keeps them.
pub(super) struct NewTwoUi {
    textures: [Option<(String, TextureHandle)>; 2],
    /// Where the slots were drawn last frame: the drop targets.
    pub(super) slot_rects: [Rect; 2],
}

impl Default for NewTwoUi {
    fn default() -> Self {
        Self {
            textures: [None, None],
            slot_rects: [Rect::NOTHING; 2],
        }
    }
}

impl InbetweenApp {
    pub fn new_two(&self) -> Option<&NewTwo> {
        self.new_two.as_ref()
    }

    pub fn new_two_mut(&mut self) -> Option<&mut NewTwo> {
        self.new_two.as_mut()
    }

    pub(super) fn open_new_two(&mut self) {
        self.new_two = Some(NewTwo::default());
        self.new_two_ui = NewTwoUi::default();
    }

    /// Run `project.new` with the picked images (Create project does the same). Does nothing
    /// until both images are readable.
    pub fn submit_new_two(&mut self) {
        let Some(params) = self.new_two.as_ref().and_then(NewTwo::to_params) else {
            return;
        };
        if self.guard("project.new", &params) {
            return; // the dialog stays open until the prompt is answered
        }
        match self.run_command("project.new", &params) {
            Ok(_) => self.new_two = None,
            Err(e) => {
                if let Some(d) = self.new_two.as_mut() {
                    d.error = Some(e.to_string());
                }
            }
        }
    }

    /// Files dropped while the dialog is open go into its slots.
    pub(super) fn drop_into_new_two(&mut self, paths: Vec<String>, pos: Option<Pos2>) {
        let over = pos.and_then(|p| {
            self.new_two_ui
                .slot_rects
                .iter()
                .position(|r| r.contains(p))
        });
        if let Some(d) = self.new_two.as_mut() {
            d.drop_files(paths, over);
        }
    }

    pub(super) fn new_two_window(&mut self, ctx: &Context) {
        let Some(dialog) = self.new_two.take() else {
            return;
        };
        self.sync_new_two_textures(ctx, &dialog);
        let mut d = dialog;
        let hover_files = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let pointer = ctx.input(|i| i.pointer.latest_pos());
        let textures = [
            self.new_two_ui.textures[0].as_ref().map(|(_, t)| t.id()),
            self.new_two_ui.textures[1].as_ref().map(|(_, t)| t.id()),
        ];
        let mut rects = self.new_two_ui.slot_rects;
        let ready = d.ready();
        let footer_note = match d.output_size() {
            Some((w, h)) => format!("{w} × {h}"),
            None => String::new(),
        };
        let (mut clicks, (create, cancel), escape) = kit::dialog(
            ctx,
            "new_two",
            "New from two images",
            "Pick an image to start from and one to end on. Inbetween renders the frames between them.",
            WIDTH,
            |ui, t| {
                let mut clicks = Clicks::default();
                let slot_w = (ui.available_width() - SWAP_SIZE - t.space.md * 2.0) / 2.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = t.space.md;
                    for i in 0..2 {
                        if i == 1 {
                            ui.vertical(|ui| {
                                ui.add_space(t.text.label + t.space.sm + (SLOT_HEIGHT - SWAP_SIZE) / 2.0);
                                if swap_button(ui, t, d.slots.iter().any(Option::is_some)).clicked() {
                                    clicks.swap = true;
                                }
                            });
                        }
                        ui.vertical(|ui| {
                            ui.set_width(slot_w);
                            ui.spacing_mut().item_spacing.y = t.space.sm;
                            kit::field_label(ui, if i == 0 { "Start image" } else { "End image" }, None);
                            let target = hover_files && pointer.is_some_and(|p| rects[i].contains(p));
                            let (rect, action) = slot(ui, t, i, d.slots[i].as_ref(), textures[i], target);
                            rects[i] = rect;
                            match action {
                                SlotAction::None => {}
                                SlotAction::Browse => clicks.browse = Some(i),
                                SlotAction::Clear => clicks.clear = Some(i),
                            }
                        });
                    }
                });
                ui.add_space(t.space.xl);
                ui.spacing_mut().item_spacing.y = t.space.sm;
                kit::field_label(ui, "Sequence name", Some("Optional"));
                let hint = d
                    .default_name()
                    .unwrap_or_else(|| "Named after both images".to_string());
                kit::text_field(ui, &mut d.name, &hint);
                if let Some(e) = &d.error {
                    ui.add_space(t.space.sm);
                    ui.label(egui::RichText::new(e).size(t.text.label).color(t.color.danger_text));
                }
                clicks
            },
            |ui, t| {
                let note = if footer_note.is_empty() {
                    egui::RichText::new("The project takes the start image’s size.")
                        .size(t.text.label)
                        .color(t.color.text_tertiary)
                } else {
                    egui::RichText::new(format!("Output {footer_note}, from the start image"))
                        .size(t.text.label)
                        .color(t.color.text_tertiary)
                };
                ui.label(note);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let create =
                        kit::button(ui, ButtonKind::Primary, "Create project", ready, t.control.height)
                            .clicked();
                    let cancel =
                        kit::button(ui, ButtonKind::Quiet, "Cancel", true, t.control.height).clicked();
                    (create, cancel)
                })
                .inner
            },
        );
        self.new_two_ui.slot_rects = rects;
        clicks.create = create;
        clicks.cancel = cancel;

        if let Some(i) = clicks.browse {
            if let Some(path) = browse_image() {
                d.set(i, path);
            }
        }
        if let Some(i) = clicks.clear {
            d.clear(i);
        }
        if clicks.swap {
            d.swap();
        }
        let enter = ready && ctx.input(|i| i.key_pressed(egui::Key::Enter));
        self.new_two = Some(d);
        if escape || clicks.cancel {
            self.new_two = None;
        } else if clicks.create || enter {
            self.submit_new_two();
        }
    }

    /// Make a texture for each slot's thumbnail when its file changes.
    fn sync_new_two_textures(&mut self, ctx: &Context, d: &NewTwo) {
        for i in 0..2 {
            let picked = d.slots[i]
                .as_ref()
                .and_then(|s| s.read.as_ref().ok().map(|p| (s.path.as_str(), p)));
            let current = self.new_two_ui.textures[i]
                .as_ref()
                .map(|(p, _)| p.as_str());
            match picked {
                None => self.new_two_ui.textures[i] = None,
                Some((path, _)) if current == Some(path) => {}
                Some((path, p)) => {
                    // A swap: reuse the other slot's texture when it shows this file.
                    let other = self.new_two_ui.textures[1 - i]
                        .as_ref()
                        .filter(|(q, _)| q == path)
                        .map(|(_, t)| t.clone());
                    let texture = other.unwrap_or_else(|| {
                        ctx.load_texture(
                            format!("new_two_{i}"),
                            p.thumb.clone(),
                            TextureOptions::LINEAR,
                        )
                    });
                    self.new_two_ui.textures[i] = Some((path.to_string(), texture));
                }
            }
        }
    }
}

fn browse_image() -> Option<String> {
    rfd::FileDialog::new()
        .add_filter("Images", inbetween_codecs::IMAGE_EXTENSIONS)
        .pick_file()
        .map(|p| p.display().to_string())
}

enum SlotAction {
    None,
    Browse,
    Clear,
}

fn swap_button(ui: &mut egui::Ui, t: &Theme, enabled: bool) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(SWAP_SIZE), sense);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            enabled,
            "Swap start and end images",
        )
    });
    let (fill, fg) = if !enabled {
        (Color32::TRANSPARENT, t.color.line)
    } else if response.hovered() {
        (t.color.field_hover, t.color.text)
    } else {
        (t.color.field, t.color.text_secondary)
    };
    ui.painter()
        .circle_filled(rect.center(), SWAP_SIZE / 2.0, fill);
    kit::icon(ui, rect.shrink(7.0), Icon::Swap, fg, 1.6);
    response.on_hover_text("Swap start and end")
}

/// One image slot: empty (a drop target), filled (thumbnail and details) or unreadable.
fn slot(
    ui: &mut egui::Ui,
    t: &Theme,
    index: usize,
    slot: Option<&Slot>,
    texture: Option<egui::TextureId>,
    drop_target: bool,
) -> (Rect, SlotAction) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, SLOT_HEIGHT), Sense::click());
    let which = if index == 0 { "start" } else { "end" };
    let card = CornerRadius::same(t.radius.card);
    let c = &t.color;
    let mut action = SlotAction::None;
    match slot {
        None => {
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    format!("Choose the {which} image"),
                )
            });
            let hot = response.hovered() || drop_target;
            let fill = if drop_target {
                c.field_active
            } else if hot {
                c.field_hover
            } else {
                c.field
            };
            ui.painter().rect_filled(rect, card, fill);
            let edge = if drop_target {
                c.focus
            } else if hot {
                c.text_secondary
            } else {
                c.dash
            };
            kit::dashed_rect(
                ui,
                rect.shrink(0.75),
                f32::from(t.radius.card),
                Stroke::new(1.5_f32, edge),
            );
            let centre = rect.center();
            kit::icon_badge(
                ui,
                centre - Vec2::new(0.0, 26.0),
                40.0,
                c.field_hover,
                Icon::Image,
                c.text_secondary,
            );
            let mut job = LayoutJob::default();
            let fmt = |color| egui::TextFormat::simple(t.regular(t.text.body), color);
            job.append("Drop an image or ", 0.0, fmt(c.text));
            job.append("browse", 0.0, fmt(c.accent_text));
            let galley = ui.painter().layout_job(job);
            let at = centre + Vec2::new(-galley.size().x / 2.0, 6.0);
            ui.painter().galley(at, galley, c.text);
            kit::centred_text(
                ui,
                centre + Vec2::new(0.0, 38.0),
                "PNG, JPEG, TIFF, PSD and more",
                t.regular(t.text.small),
                c.text_tertiary,
            );
            if response.clicked() {
                action = SlotAction::Browse;
            }
        }
        Some(s) => match &s.read {
            Ok(p) => {
                ui.painter().rect_filled(rect, card, c.field);
                let thumb = Rect::from_min_size(rect.min, Vec2::new(rect.width(), THUMB_HEIGHT));
                let top = CornerRadius {
                    nw: t.radius.card,
                    ne: t.radius.card,
                    sw: 0,
                    se: 0,
                };
                ui.painter().rect_filled(thumb, top, c.app);
                if let Some(id) = texture {
                    let uv = cover_uv(Vec2::new(p.width as f32, p.height as f32), thumb.size());
                    ui.painter()
                        .add(RectShape::filled(thumb, top, Color32::WHITE).with_texture(id, uv));
                }
                ui.painter().rect_stroke(
                    rect,
                    card,
                    Stroke::new(1.0_f32, c.line),
                    StrokeKind::Inside,
                );
                let thumb_hit =
                    ui.interact(thumb, ui.id().with(("replace", index)), Sense::click());
                thumb_hit.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!("Replace the {which} image"),
                    )
                });
                if drop_target {
                    ui.painter().rect_stroke(
                        rect,
                        card,
                        Stroke::new(2.0_f32, c.focus),
                        StrokeKind::Inside,
                    );
                }
                let close = Rect::from_min_size(
                    Pos2::new(thumb.right() - t.space.sm - 28.0, thumb.top() + t.space.sm),
                    Vec2::splat(28.0),
                );
                let close_hit = ui.interact(close, ui.id().with(("clear", index)), Sense::click());
                close_hit.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!("Remove the {which} image"),
                    )
                });
                let shade = if close_hit.hovered() { 230 } else { 184 };
                ui.painter().circle_filled(
                    close.center(),
                    14.0,
                    Color32::from_rgba_unmultiplied(15, 17, 21, shade),
                );
                kit::icon(ui, close.shrink(7.0), Icon::Close, c.text, 2.0);
                let meta = Rect::from_min_max(
                    Pos2::new(rect.left() + t.space.md, thumb.bottom() + t.space.md),
                    Pos2::new(rect.right() - t.space.md, rect.bottom() - t.space.md),
                );
                ui.scope_builder(UiBuilder::new().max_rect(meta), |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    kit::text_line(ui, &s.file_name(), t.text.body, c.text).on_hover_text(&s.path);
                    let details = if p.format.is_empty() {
                        format!("{} × {}", p.width, p.height)
                    } else {
                        format!("{} × {} · {}", p.width, p.height, p.format)
                    };
                    kit::text_line(ui, &details, t.text.small, c.text_tertiary);
                });
                if close_hit.clicked() {
                    action = SlotAction::Clear;
                } else if thumb_hit.clicked() {
                    action = SlotAction::Browse;
                }
            }
            Err(bad) => {
                ui.painter().rect_filled(rect, card, c.danger_fill);
                let edge = if drop_target { c.focus } else { c.danger_line };
                ui.painter().rect_stroke(
                    rect,
                    card,
                    Stroke::new(1.5_f32, edge),
                    StrokeKind::Inside,
                );
                kit::icon_badge(
                    ui,
                    rect.center_top() + Vec2::new(0.0, t.space.lg + 20.0),
                    40.0,
                    c.danger_soft,
                    Icon::Alert,
                    c.danger_text,
                );
                let body = Rect::from_min_max(
                    Pos2::new(
                        rect.left() + t.space.lg,
                        rect.top() + t.space.lg + 40.0 + t.space.md,
                    ),
                    Pos2::new(rect.right() - t.space.lg, rect.bottom() - t.space.md),
                );
                ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
                    ui.vertical_centered(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        kit::text_line(
                            ui,
                            &format!("Couldn’t read {}", s.file_name()),
                            t.text.body,
                            c.danger_text_strong,
                        )
                        .on_hover_text(&bad.detail);
                        ui.label(
                            egui::RichText::new(&bad.reason)
                                .size(t.text.small)
                                .color(c.danger_text),
                        );
                        ui.add_space(t.space.sm);
                        if kit::button(
                            ui,
                            ButtonKind::Danger,
                            "Choose another",
                            true,
                            t.control.compact_height,
                        )
                        .clicked()
                        {
                            action = SlotAction::Browse;
                        }
                    });
                });
            }
        },
    }
    (rect, action)
}

/// The part of a `source`-sized image that covers a `frame`-sized box without distortion, as
/// texture coordinates.
pub(super) fn cover_uv(source: Vec2, frame: Vec2) -> Rect {
    let full = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    if source.x <= 0.0 || source.y <= 0.0 || frame.x <= 0.0 || frame.y <= 0.0 {
        return full;
    }
    let (sa, fa) = (source.x / source.y, frame.x / frame.y);
    if sa > fa {
        let w = fa / sa;
        Rect::from_min_max(
            Pos2::new((1.0 - w) / 2.0, 0.0),
            Pos2::new((1.0 + w) / 2.0, 1.0),
        )
    } else {
        let h = sa / fa;
        Rect::from_min_max(
            Pos2::new(0.0, (1.0 - h) / 2.0),
            Pos2::new(1.0, (1.0 + h) / 2.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_crops_the_long_side_and_survives_zero_sizes() {
        let uv = cover_uv(Vec2::new(200.0, 100.0), Vec2::new(100.0, 100.0));
        assert!((uv.width() - 0.5).abs() < 1e-6 && (uv.height() - 1.0).abs() < 1e-6);
        assert!((uv.center().x - 0.5).abs() < 1e-6);
        let uv = cover_uv(Vec2::new(100.0, 400.0), Vec2::new(200.0, 100.0));
        assert!((uv.width() - 1.0).abs() < 1e-6 && uv.height() < 1.0);
        let full = cover_uv(Vec2::ZERO, Vec2::new(10.0, 10.0));
        assert_eq!(full.max, Pos2::new(1.0, 1.0));
    }
}
