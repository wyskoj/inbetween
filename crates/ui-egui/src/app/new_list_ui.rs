//! The New From Image List dialog: an ordered, reorderable list of images, a loop switch, the
//! shared sequence settings, and Create project. The state lives in [`crate::new_list`]; this
//! draws it and runs `file.wizard`.

use super::new_two_ui::cover_uv;
use super::*;
use crate::kit::{self, ButtonKind, Icon, Segment};
use crate::new_list::{Blocker, NewList, PlayAs, MORPHS};
use crate::new_two::Slot;
use crate::theme::Theme;
use egui::epaint::RectShape;
use egui::text::LayoutJob;
use egui::{Color32, CornerRadius, Sense, Stroke, StrokeKind, TextureId, UiBuilder};
use std::collections::HashMap;

const WIDTH: f32 = 600.0;
const LIST_HEIGHT: f32 = 312.0;
const ROW_HEIGHT: f32 = 52.0;
const ROW_GAP: f32 = 2.0;
const LOOP_ROW_HEIGHT: f32 = 32.0;
const THUMB: Vec2 = Vec2::new(48.0, 36.0);
const ROW_BUTTON: f32 = 28.0;

/// What a click in the dialog asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Add,
    Replace(usize),
    Remove(usize),
    Up(usize),
    Down(usize),
    MoveTo(usize, usize),
}

/// The row thumbnails as textures, keyed by path, and where things were drawn last frame.
#[derive(Default)]
pub(super) struct NewListUi {
    textures: HashMap<String, TextureHandle>,
    /// The list: the drop target.
    pub(super) list_rect: Option<Rect>,
    /// The row being dragged by its handle.
    drag: Option<usize>,
}

impl InbetweenApp {
    pub fn new_list(&self) -> Option<&NewList> {
        self.new_list.as_ref()
    }

    pub fn new_list_mut(&mut self) -> Option<&mut NewList> {
        self.new_list.as_mut()
    }

    /// Open the dialog, with `paths` already in the list (a drop of several images).
    pub(super) fn open_new_list(&mut self, paths: Vec<String>) {
        self.new_list = Some(NewList::with_images(paths));
        self.new_list_ui = NewListUi::default();
    }

    /// Run `file.wizard` with the listed images (Create project does the same). Does nothing
    /// until the list can succeed.
    pub fn submit_new_list(&mut self) {
        let Some(params) = self.new_list.as_ref().and_then(NewList::to_params) else {
            return;
        };
        if self.guard("file.wizard", &params) {
            return; // the dialog stays open until the prompt is answered
        }
        match self.run_command("file.wizard", &params) {
            Ok(_) => self.new_list = None,
            Err(e) => {
                if let Some(d) = self.new_list.as_mut() {
                    d.error = Some(e.to_string());
                }
            }
        }
    }

    /// Files dropped while the dialog is open join the end of the list.
    pub(super) fn drop_into_new_list(&mut self, paths: Vec<String>) {
        if let Some(d) = self.new_list.as_mut() {
            d.add(paths);
        }
    }

    pub(super) fn new_list_window(&mut self, ctx: &Context) {
        let Some(mut d) = self.new_list.take() else {
            return;
        };
        self.sync_new_list_textures(ctx, &d);
        let textures: Vec<Option<TextureId>> = d
            .images
            .iter()
            .map(|s| self.new_list_ui.textures.get(&s.path).map(|t| t.id()))
            .collect();
        let hover_files = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let pointer = ctx.input(|i| i.pointer.latest_pos());
        let drop_target = hover_files
            && pointer.is_some_and(|p| self.new_list_ui.list_rect.is_some_and(|r| r.contains(p)));
        let mut drag = self.new_list_ui.drag;
        let blocker = d.blocker();
        let ready = blocker.is_none();
        let total = d.total_frames();
        let typed_size = !d.width.trim().is_empty() || !d.height.trim().is_empty();
        let size = d.output_size();

        let ((action, list_rect), (create, cancel), escape) = kit::dialog(
            ctx,
            "new_list",
            "New from image list",
            "Chain several images: each one morphs into the next. Add dots afterwards.",
            WIDTH,
            |ui, t| {
                ui.spacing_mut().item_spacing.y = t.space.sm;
                let aside = images_aside(&d);
                kit::field_label(ui, "Images", aside.as_deref());
                let (list_rect, mut action) = if d.images.is_empty() {
                    let (rect, clicked) = empty_list(ui, t, drop_target);
                    (rect, clicked.then_some(Action::Add))
                } else {
                    image_list(ui, t, &d, &textures, drop_target, &mut drag)
                };
                ui.add_space(t.space.xs);
                ui.horizontal(|ui| {
                    if kit::switch(
                        ui,
                        "Loop back to the first image",
                        d.looped,
                        t.control.compact_height,
                    )
                    .clicked()
                    {
                        d.looped = !d.looped;
                    }
                    if !d.images.is_empty() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let add = kit::icon_text_button(
                                ui,
                                ButtonKind::Secondary,
                                Some(Icon::Plus),
                                "Add images",
                                true,
                                t.control.compact_height,
                            );
                            if add.clicked() {
                                action = Some(Action::Add);
                            }
                        });
                    }
                });
                ui.add_space(t.space.xl);
                settings(ui, t, &mut d);
                if let Some(e) = &d.error {
                    ui.add_space(t.space.md);
                    ui.label(
                        egui::RichText::new(e)
                            .size(t.text.label)
                            .color(t.color.danger_text),
                    );
                }
                (action, list_rect)
            },
            |ui, t| {
                footer_note(ui, t, blocker.as_ref(), total, size, typed_size);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let create = kit::button(
                        ui,
                        ButtonKind::Primary,
                        "Create project",
                        ready,
                        t.control.height,
                    )
                    .clicked();
                    let cancel =
                        kit::button(ui, ButtonKind::Quiet, "Cancel", true, t.control.height)
                            .clicked();
                    (create, cancel)
                })
                .inner
            },
        );
        self.new_list_ui.list_rect = Some(list_rect);
        self.new_list_ui.drag = drag;

        match action {
            Some(Action::Add) => {
                let paths = browse_images();
                if !paths.is_empty() {
                    d.add(paths);
                }
            }
            Some(Action::Replace(i)) => {
                if let Some(path) = browse_images().into_iter().next() {
                    d.replace(i, path);
                }
            }
            Some(Action::Remove(i)) => d.remove(i),
            Some(Action::Up(i)) => d.move_up(i),
            Some(Action::Down(i)) => d.move_down(i),
            Some(Action::MoveTo(from, to)) => d.move_to(from, to),
            None => {}
        }
        let enter = ready && ctx.input(|i| i.key_pressed(egui::Key::Enter));
        self.new_list = Some(d);
        if escape || cancel {
            self.new_list = None;
        } else if create || enter {
            self.submit_new_list();
        }
    }

    /// Make a texture for each readable image the first time it is listed; drop the rest.
    fn sync_new_list_textures(&mut self, ctx: &Context, d: &NewList) {
        let textures = &mut self.new_list_ui.textures;
        textures.retain(|path, _| d.images.iter().any(|s| &s.path == path));
        for s in &d.images {
            if let (Ok(p), false) = (&s.read, textures.contains_key(&s.path)) {
                let texture = ctx.load_texture(
                    format!("new_list:{}", s.path),
                    p.thumb.clone(),
                    TextureOptions::LINEAR,
                );
                textures.insert(s.path.clone(), texture);
            }
        }
    }
}

fn browse_images() -> Vec<String> {
    rfd::FileDialog::new()
        .add_filter("Images", inbetween_codecs::IMAGE_EXTENSIONS)
        .pick_files()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.display().to_string())
        .collect()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "5 images · 4 morphs" beside the Images label.
fn images_aside(d: &NewList) -> Option<String> {
    let n = d.images.len();
    if n == 0 {
        return None;
    }
    let images = plural(n, "image", "images");
    let seqs = d.sequence_count();
    if seqs == 0 {
        return Some(images);
    }
    let what = match d.play_as {
        PlayAs::Sequence => plural(seqs, "morph", "morphs"),
        PlayAs::Layers => plural(seqs, "layer", "layers"),
    };
    Some(format!("{images} · {what}"))
}

/// The empty list: one large drop target that also browses.
fn empty_list(ui: &mut egui::Ui, t: &Theme, drop_target: bool) -> (Rect, bool) {
    let c = &t.color;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), LIST_HEIGHT), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Choose images"));
    let hot = response.hovered() || drop_target;
    let fill = if drop_target {
        c.field_active
    } else if hot {
        c.field_hover
    } else {
        c.field
    };
    let card = CornerRadius::same(t.radius.card);
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
    job.append("Drop images or ", 0.0, fmt(c.text));
    job.append("browse", 0.0, fmt(c.accent_text));
    let galley = ui.painter().layout_job(job);
    let at = centre + Vec2::new(-galley.size().x / 2.0, 6.0);
    ui.painter().galley(at, galley, c.text);
    kit::centred_text(
        ui,
        centre + Vec2::new(0.0, 38.0),
        "Two or more, in the order they play. You can reorder them after.",
        t.regular(t.text.small),
        c.text_tertiary,
    );
    (rect, response.clicked())
}

/// The filled list: a recessed, scrolling box of rows, with drag-to-reorder by the handles.
fn image_list(
    ui: &mut egui::Ui,
    t: &Theme,
    d: &NewList,
    textures: &[Option<TextureId>],
    drop_target: bool,
    drag: &mut Option<usize>,
) -> (Rect, Option<Action>) {
    let c = &t.color;
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), LIST_HEIGHT), Sense::hover());
    let card = CornerRadius::same(t.radius.card);
    ui.painter().rect_filled(rect, card, c.field);
    let mut action = None;
    let mut row_rects = Vec::with_capacity(d.images.len());
    let inner = rect.shrink(t.space.xs);
    ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
        egui::ScrollArea::vertical()
            .id_salt("new_list_rows")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = ROW_GAP;
                let count = d.images.len();
                for (i, slot) in d.images.iter().enumerate() {
                    let texture = textures.get(i).copied().flatten();
                    let (row_rect, row_action) = row(ui, t, i, count, slot, texture, drag);
                    row_rects.push(row_rect);
                    action = action.or(row_action);
                }
                if d.looped && count >= 2 {
                    if let Some(first) = d.images.first() {
                        loop_row(ui, t, &first.file_name());
                    }
                }
            });
    });
    ui.painter()
        .rect_stroke(rect, card, Stroke::new(1.0_f32, c.line), StrokeKind::Inside);
    if drop_target {
        ui.painter().rect_stroke(
            rect,
            card,
            Stroke::new(2.0_f32, c.focus),
            StrokeKind::Inside,
        );
    }
    // While a handle is held, mark where the row would land; on release, move it there.
    if let Some(from) = *drag {
        let (down, pointer) = ui.input(|i| (i.pointer.primary_down(), i.pointer.latest_pos()));
        let slot = pointer
            .map(|p| row_rects.iter().filter(|r| r.center().y < p.y).count())
            .unwrap_or(from);
        if down {
            let y = match (
                slot.checked_sub(1).and_then(|k| row_rects.get(k)),
                row_rects.get(slot),
            ) {
                (Some(above), _) => above.bottom() + ROW_GAP / 2.0,
                (None, Some(below)) => below.top() - ROW_GAP / 2.0,
                (None, None) => inner.top(),
            };
            let y = y.clamp(inner.top(), inner.bottom());
            ui.painter().line_segment(
                [
                    Pos2::new(inner.left() + t.space.xs, y),
                    Pos2::new(inner.right() - t.space.xs, y),
                ],
                Stroke::new(2.0_f32, c.focus),
            );
        } else {
            let to = if slot > from { slot - 1 } else { slot };
            if to != from {
                action = Some(Action::MoveTo(from, to));
            }
            *drag = None;
        }
    }
    (rect, action)
}

/// One image: handle, number, thumbnail, name and details, and its actions on hover.
fn row(
    ui: &mut egui::Ui,
    t: &Theme,
    index: usize,
    count: usize,
    slot: &Slot,
    texture: Option<TextureId>,
    drag: &mut Option<usize>,
) -> (Rect, Option<Action>) {
    let c = &t.color;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::hover());
    let name = slot.file_name();
    let bad = slot.read.as_ref().err();
    let dragging = *drag == Some(index);
    let hovered = response.hovered() || dragging;
    let radius = CornerRadius::same(t.radius.control);
    if bad.is_some() {
        ui.painter().rect_filled(rect, radius, c.danger_fill);
        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(1.0_f32, c.danger_line),
            StrokeKind::Inside,
        );
    } else if hovered {
        ui.painter().rect_filled(rect, radius, c.field_hover);
    }

    // The handle: drag the row to a new place.
    let grip = Rect::from_min_size(rect.min, Vec2::new(24.0, rect.height()));
    let grip_hit = ui
        .interact(grip, ui.id().with(("grip", index)), Sense::drag())
        .on_hover_cursor(egui::CursorIcon::Grab);
    grip_hit.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Other,
            true,
            format!("Drag to reorder {name}"),
        )
    });
    if grip_hit.drag_started() {
        *drag = Some(index);
    }
    let grip_color = if hovered {
        c.text_secondary
    } else {
        c.text_disabled
    };
    kit::icon(
        ui,
        Rect::from_center_size(grip.center(), Vec2::splat(16.0)),
        Icon::Grip,
        grip_color,
        1.5,
    );

    let number_color = if bad.is_some() {
        c.danger_text
    } else {
        c.text_tertiary
    };
    let number_right = grip.right() + 16.0;
    ui.painter().text(
        Pos2::new(number_right, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        (index + 1).to_string(),
        t.regular(t.text.small),
        number_color,
    );

    let thumb = Rect::from_min_size(
        Pos2::new(number_right + t.space.md, rect.center().y - THUMB.y / 2.0),
        THUMB,
    );
    let small = CornerRadius::same(t.radius.small);
    match (&slot.read, texture) {
        (Ok(p), Some(id)) => {
            let uv = cover_uv(Vec2::new(p.width as f32, p.height as f32), THUMB);
            ui.painter().rect_filled(thumb, small, c.app);
            ui.painter()
                .add(RectShape::filled(thumb, small, Color32::WHITE).with_texture(id, uv));
            ui.painter().rect_stroke(
                thumb,
                small,
                Stroke::new(1.0_f32, c.line),
                StrokeKind::Outside,
            );
        }
        (Ok(_), None) => {
            ui.painter().rect_filled(thumb, small, c.app);
        }
        (Err(_), _) => {
            ui.painter().rect_filled(thumb, small, c.danger_soft);
            kit::icon(
                ui,
                Rect::from_center_size(thumb.center(), Vec2::splat(18.0)),
                Icon::Alert,
                c.danger_text,
                1.8,
            );
        }
    }

    // Actions at the right: always for a bad file, on hover otherwise.
    let mut action = None;
    let mut text_right = rect.right() - t.space.md;
    if bad.is_some() || hovered {
        let area = rect.shrink2(Vec2::new(t.space.xs, 0.0));
        let used = ui.scope_builder(
            UiBuilder::new()
                .max_rect(area)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let remove = kit::icon_button(
                    ui,
                    Icon::Close,
                    &format!("Remove {name}"),
                    false,
                    true,
                    ROW_BUTTON,
                );
                if remove.clicked() {
                    action = Some(Action::Remove(index));
                }
                if bad.is_some() {
                    ui.add_space(t.space.xs);
                    let replace = kit::button(
                        ui,
                        ButtonKind::Danger,
                        "Replace",
                        true,
                        t.control.compact_height,
                    );
                    if replace.clicked() {
                        action = Some(Action::Replace(index));
                    }
                } else {
                    let down = kit::icon_button(
                        ui,
                        Icon::ArrowDown,
                        &format!("Move {name} down"),
                        false,
                        index + 1 < count,
                        ROW_BUTTON,
                    );
                    if down.clicked() {
                        action = Some(Action::Down(index));
                    }
                    let up = kit::icon_button(
                        ui,
                        Icon::ArrowUp,
                        &format!("Move {name} up"),
                        false,
                        index > 0,
                        ROW_BUTTON,
                    );
                    if up.clicked() {
                        action = Some(Action::Up(index));
                    }
                }
            },
        );
        text_right = used.response.rect.left() - t.space.sm;
    }

    let text = Rect::from_min_max(
        Pos2::new(thumb.right() + t.space.md, rect.top()),
        Pos2::new(text_right.max(thumb.right() + t.space.md), rect.bottom()),
    );
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(text)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space((ROW_HEIGHT - 36.0) / 2.0);
            match bad {
                None => {
                    kit::text_line(ui, &name, t.text.body, c.text).on_hover_text(&slot.path);
                    let details = match &slot.read {
                        Ok(p) if p.format.is_empty() => format!("{} × {}", p.width, p.height),
                        Ok(p) => format!("{} × {} · {}", p.width, p.height, p.format),
                        Err(_) => String::new(),
                    };
                    kit::text_line(ui, &details, t.text.small, c.text_tertiary);
                }
                Some(e) => {
                    kit::text_line(ui, &name, t.text.body, c.danger_text_strong)
                        .on_hover_text(&e.detail);
                    let why = format!("Couldn’t read this file. {}", e.reason);
                    kit::text_line(ui, &why, t.text.small, c.danger_text).on_hover_text(&e.detail);
                }
            }
        },
    );
    (rect, action)
}

/// The quiet last row when the chain loops: where it goes after the last image.
fn loop_row(ui: &mut egui::Ui, t: &Theme, first: &str) {
    let c = &t.color;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), LOOP_ROW_HEIGHT),
        Sense::hover(),
    );
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 32.0, rect.center().y),
        Vec2::splat(14.0),
    );
    kit::icon(ui, icon_rect, Icon::Return, c.focus, 1.8);
    let mut job = LayoutJob::default();
    let font = t.regular(t.text.small);
    job.append(
        "Then back to ",
        0.0,
        egui::TextFormat::simple(font.clone(), c.text_tertiary),
    );
    job.append(
        &format!("1 · {first}"),
        0.0,
        egui::TextFormat::simple(font, c.text_secondary),
    );
    let galley = ui.painter().layout_job(job);
    let at = Pos2::new(
        icon_rect.right() + t.space.sm,
        rect.center().y - galley.size().y / 2.0,
    );
    ui.painter().galley(at, galley, c.text);
}

/// The settings every sequence shares, in two columns.
fn settings(ui: &mut egui::Ui, t: &Theme, d: &mut NewList) {
    let col = (ui.available_width() - t.space.lg) / 2.0;
    let column = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui)| {
        ui.vertical(|ui| {
            ui.set_width(col);
            ui.spacing_mut().item_spacing.y = t.space.sm;
            add(ui);
        });
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.lg;
        column(ui, &mut |ui| {
            kit::field_label(ui, "Frames per morph", None);
            kit::text_field(
                ui,
                &mut d.frames,
                &crate::new_list::DEFAULT_FRAMES.to_string(),
            );
        });
        column(ui, &mut |ui| {
            kit::field_label(ui, "Morph type", None);
            let current = MORPHS
                .iter()
                .find(|(m, _)| *m == d.morph)
                .map_or("Morph", |(_, name)| name);
            ui.spacing_mut().interact_size.y = t.control.height;
            egui::ComboBox::from_id_salt("new_list_morph")
                .width(col)
                .height(f32::INFINITY)
                .selected_text(egui::RichText::new(current).size(t.text.body))
                .show_ui(ui, |ui| {
                    for (m, name) in MORPHS {
                        ui.selectable_value(&mut d.morph, m, name);
                    }
                });
        });
    });
    ui.add_space(t.space.xl);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.lg;
        column(ui, &mut |ui| {
            kit::field_label(ui, "Play as", None);
            let selected = usize::from(d.play_as == PlayAs::Layers);
            let segments = [Segment::text("One after another"), Segment::text("Layers")];
            match kit::segmented(ui, &segments, selected, true, t.control.height) {
                Some(0) => d.play_as = PlayAs::Sequence,
                Some(1) => d.play_as = PlayAs::Layers,
                _ => {}
            }
        });
        column(ui, &mut |ui| {
            kit::field_label(ui, "Output size", Some("Optional"));
            let (w, h) = d
                .first_size()
                .map_or(("Width".to_string(), "Height".to_string()), |(w, h)| {
                    (w.to_string(), h.to_string())
                });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = t.space.sm;
                let times = ui.painter().layout_no_wrap(
                    "×".into(),
                    t.regular(t.text.body),
                    t.color.text_tertiary,
                );
                let field = ((col - times.size().x - t.space.sm * 2.0) / 2.0).max(0.0);
                let size = Vec2::new(field, t.control.height);
                ui.allocate_ui(size, |ui| kit::text_field(ui, &mut d.width, &w));
                ui.label(
                    egui::RichText::new("×")
                        .size(t.text.body)
                        .color(t.color.text_tertiary),
                );
                ui.allocate_ui(size, |ui| kit::text_field(ui, &mut d.height, &h));
            });
        });
    });
}

/// The footer's left side: what is missing, or what will be made.
fn footer_note(
    ui: &mut egui::Ui,
    t: &Theme,
    blocker: Option<&Blocker>,
    total: u64,
    size: Option<(u32, u32)>,
    typed_size: bool,
) {
    let c = &t.color;
    let font = t.regular(t.text.label);
    let mut job = LayoutJob::default();
    match blocker {
        Some(b @ Blocker::TooFew) => {
            job.append(
                &b.message(),
                0.0,
                egui::TextFormat::simple(font, c.text_tertiary),
            );
        }
        Some(b) => {
            job.append(
                &b.message(),
                0.0,
                egui::TextFormat::simple(font, c.danger_text),
            );
        }
        None => {
            let frames = format!("{total} {}", if total == 1 { "frame" } else { "frames" });
            job.append(
                &frames,
                0.0,
                egui::TextFormat::simple(font.clone(), c.text_secondary),
            );
            let rest = match size {
                Some((w, h)) if typed_size => format!(", output {w} × {h}"),
                _ => ", output size from the first image".to_string(),
            };
            job.append(&rest, 0.0, egui::TextFormat::simple(font, c.text_tertiary));
        }
    }
    ui.label(job);
}
