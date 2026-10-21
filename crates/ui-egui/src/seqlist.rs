//! The sequence list panel: one row per sequence with both thumbnails, its name and an eye
//! toggle. It only reports what was clicked or dropped ([`ListAction`]); the app runs the
//! matching `sequence.*` commands.
//!
//! Dragging a row onto another row moves it there (`sequence.reorder`). Dragging a thumbnail
//! onto another row's thumbnail copies that image with its dots (`sequence.copyFrame`).

use crate::kit::{self, ButtonKind, Icon, Segment};
use egui::epaint::RectShape;
use egui::{Color32, CornerRadius, Sense, Stroke, TextureId, Ui, Vec2};
use std::sync::Arc;

/// What the list shows for one sequence.
#[derive(Clone, Debug)]
pub struct Row {
    pub label: String,
    pub visible: bool,
    pub frames: u32,
    pub thumbs: [Option<TextureId>; 2],
    pub selected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListAction {
    Select(usize),
    /// The eye was clicked; `solo` when a modifier key was held.
    Visibility {
        index: usize,
        solo: bool,
    },
    /// `up`, `down`, `top` or `bottom`.
    Move(usize, &'static str),
    /// A row was dragged to the position of another row.
    MoveTo {
        from: usize,
        to: usize,
    },
    /// A thumbnail was dropped on another row's thumbnail. Images are numbered 1 and 2.
    CopyFrame {
        from: usize,
        from_image: u8,
        to: usize,
        to_image: u8,
    },
    /// `sequence` or `layer` (`project.type`).
    ProjectType(&'static str),
    Duplicate(usize),
    Delete(usize),
    Rename(usize),
    Link(usize),
    AddBlank,
    AddContinued,
}

/// What a drag in the list carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Payload {
    Row(usize),
    Frame { sequence: usize, image: u8 },
}

/// Where a payload was released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Row(usize),
    Thumb { sequence: usize, image: u8 },
}

/// The action a drop asks for, or `None` when it would change nothing (a row onto itself, an
/// image onto its own slot) or does not fit (a row onto a thumbnail is a row move, an image
/// onto a bare row does nothing).
pub fn drop_action(payload: Payload, target: Target) -> Option<ListAction> {
    match (payload, target) {
        (Payload::Row(from), Target::Row(to) | Target::Thumb { sequence: to, .. })
            if from != to =>
        {
            Some(ListAction::MoveTo { from, to })
        }
        (
            Payload::Frame { sequence, image },
            Target::Thumb {
                sequence: to,
                image: to_image,
            },
        ) if (sequence, image) != (to, to_image) => Some(ListAction::CopyFrame {
            from: sequence,
            from_image: image,
            to,
            to_image,
        }),
        _ => None,
    }
}

/// The label a row wears: its name, plus its stack position in a layer project.
pub fn row_title(label: &str, index: usize, layers: bool) -> String {
    if layers {
        let place = if index == 0 { "top" } else { "below" };
        format!("{label}  (layer {}, {place})", index + 1)
    } else {
        label.to_string()
    }
}

/// A row's second line: length, and where it sits in a layer project or that it is hidden.
pub fn row_meta(frames: u32, seconds: f64, visible: bool, index: usize, layers: bool) -> String {
    let length = format!("{frames} frames \u{b7} {seconds:.2} s");
    let place = if layers {
        let where_ = if index == 0 { "top" } else { "below" };
        format!("Layer {}, {where_} \u{b7} ", index + 1)
    } else {
        String::new()
    };
    let hidden = if visible { "" } else { "Hidden \u{b7} " };
    format!("{hidden}{place}{length}")
}

/// The two halves of the line under the list: the movie's length, and how many sequences
/// are in it.
pub fn summary(
    layers: bool,
    movie_frames: u64,
    seconds: f64,
    shown: usize,
    total: usize,
) -> (String, String) {
    let kind = if layers { "Stacked" } else { "Movie" };
    (
        format!("{kind} \u{b7} {movie_frames} frames \u{b7} {seconds:.2} s"),
        format!("{shown} of {total} shown"),
    )
}

const THUMB: Vec2 = Vec2::new(36.0, 28.0);
const EYE: f32 = 28.0;

/// One thumbnail, rounded on its outer side (`left` for Image 1).
fn thumb(ui: &mut Ui, id: Option<TextureId>, dim: bool, left: bool) -> egui::Response {
    let t = crate::theme::get(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(THUMB, Sense::drag());
    let r = t.radius.small;
    let corners = if left {
        CornerRadius {
            nw: r,
            sw: r,
            ne: 0,
            se: 0,
        }
    } else {
        CornerRadius {
            nw: 0,
            sw: 0,
            ne: r,
            se: r,
        }
    };
    ui.painter().rect_filled(rect, corners, t.color.field);
    if let Some(id) = id {
        let tint = if dim {
            Color32::from_white_alpha(100)
        } else {
            Color32::WHITE
        };
        ui.painter()
            .add(RectShape::filled(rect, corners, tint).with_texture(
                id,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
            ));
    }
    response
}

fn hovered_payload(response: &egui::Response) -> Option<Arc<Payload>> {
    response.dnd_hover_payload::<Payload>()
}

/// Draw the list; returns the actions the user asked for. `layers` is true for a layer
/// project, where rows are stacked layers rather than steps in time. `fps` turns frame counts
/// into seconds.
pub fn sequence_list(
    ui: &mut Ui,
    rows: &[Row],
    movie_frames: u64,
    layers: bool,
    fps: f64,
) -> Vec<ListAction> {
    let t = crate::theme::get(ui.ctx());
    let c = t.color;
    let seconds = |frames: f64| if fps > 0.0 { frames / fps } else { 0.0 };
    let mut actions = Vec::new();
    egui::Frame::NONE
        .inner_margin(egui::Margin {
            left: 16,
            right: 12,
            top: 10,
            bottom: 10,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Sequences")
                        .text_style(crate::theme::label_style())
                        .color(c.text_secondary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let kinds = [
                        Segment::text("In order").tip("Sequences play one after another"),
                        Segment::text("Layers").tip("Sequences play at once, the first on top"),
                    ];
                    let current = usize::from(layers);
                    if let Some(i) = kit::segmented(ui, &kinds, current, true, 24.0) {
                        if i != current {
                            actions.push(ListAction::ProjectType(if i == 1 {
                                "layer"
                            } else {
                                "sequence"
                            }));
                        }
                    }
                });
            });
        });
    let last = rows.len().saturating_sub(1);
    let footer = 92.0;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height((ui.available_height() - footer).max(60.0))
        .show(ui, |ui| {
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(8, 0))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = t.space.xs;
                    for (index, row) in rows.iter().enumerate() {
                        list_row(ui, row, index, last, layers, seconds(f64::from(row.frames)), &mut actions);
                    }
                    if rows.is_empty() {
                        ui.add_space(t.space.xl);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("No sequences yet")
                                    .size(t.text.body)
                                    .color(c.text_secondary),
                            );
                            ui.add_space(t.space.xs);
                            ui.label(
                                egui::RichText::new(
                                    "Each sequence morphs one image into another. A movie plays them in order.",
                                )
                                .size(t.text.small)
                                .color(c.text_tertiary),
                            );
                        });
                    }
                });
        });
    // The footer: a hairline, the add buttons and the movie's length.
    let line_y = ui.cursor().top();
    let full = ui.max_rect();
    ui.painter()
        .hline(full.x_range(), line_y, egui::Stroke::new(1.0_f32, c.field));
    egui::Frame::NONE
        .inner_margin(egui::Margin {
            left: 16,
            right: 16,
            top: 12,
            bottom: 12,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(t.space.sm, t.space.md);
            ui.horizontal(|ui| {
                if kit::icon_text_button(
                    ui,
                    ButtonKind::Secondary,
                    Some(Icon::Plus),
                    "Blank",
                    true,
                    t.control.compact_height,
                )
                .on_hover_text("Add an empty sequence")
                .clicked()
                {
                    actions.push(ListAction::AddBlank);
                }
                if kit::icon_text_button(
                    ui,
                    ButtonKind::Secondary,
                    Some(Icon::Plus),
                    "Continued\u{2026}",
                    true,
                    t.control.compact_height,
                )
                .on_hover_text("Add a sequence that starts where the last one ends")
                .clicked()
                {
                    actions.push(ListAction::AddContinued);
                }
            });
            let shown = rows.iter().filter(|r| r.visible).count();
            let (left, right) = summary(
                layers,
                movie_frames,
                seconds(movie_frames as f64),
                shown,
                rows.len(),
            );
            // Side by side when both fit, else the count goes under the length.
            let font = t.regular(t.text.small);
            let width = |s: &str| {
                ui.painter()
                    .layout_no_wrap(s.to_string(), font.clone(), c.text_tertiary)
                    .size()
                    .x
            };
            if width(&left) + width(&right) + t.space.lg <= ui.available_width() {
                ui.horizontal(|ui| {
                    ui.label(meta(ui, left));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(meta(ui, right));
                    });
                });
            } else {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(meta(ui, left));
                ui.label(meta(ui, right));
            }
        });
    actions
}

/// One sequence: both thumbnails, its name and length, and the eye.
fn list_row(
    ui: &mut Ui,
    row: &Row,
    index: usize,
    last: usize,
    layers: bool,
    seconds: f64,
    actions: &mut Vec<ListAction>,
) {
    let t = crate::theme::get(ui.ctx());
    let c = t.color;
    let background = ui.painter().add(egui::Shape::Noop);
    let mut thumb_drops = Vec::new();
    let inner = egui::Frame::NONE
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for slot in 0..2 {
                    let image = slot as u8 + 1;
                    let response = thumb(ui, row.thumbs[slot], !row.visible, slot == 0);
                    response.dnd_set_drag_payload(Payload::Frame {
                        sequence: index,
                        image,
                    });
                    if hovered_payload(&response).is_some_and(|p| matches!(*p, Payload::Frame { .. })) {
                        ui.painter().rect_stroke(
                            response.rect,
                            CornerRadius::same(t.radius.small),
                            Stroke::new(2.0_f32, c.focus),
                            egui::StrokeKind::Outside,
                        );
                        if let Some(p) = response.dnd_release_payload::<Payload>() {
                            thumb_drops.push((
                                *p,
                                Target::Thumb {
                                    sequence: index,
                                    image,
                                },
                            ));
                        }
                    }
                    response.on_hover_text(format!(
                        "Image {image}: drag onto another sequence's thumbnail to copy it with its dots"
                    ));
                }
                ui.add_space(t.space.md);
                let text_w = (ui.available_width() - EYE - t.space.sm).max(20.0);
                let (title_color, meta_color) = if row.selected {
                    (c.on_accent, c.on_accent_secondary)
                } else if row.visible {
                    (c.text, c.text_tertiary)
                } else {
                    (c.text_tertiary, c.text_tertiary)
                };
                let title = ui
                    .allocate_ui(Vec2::new(text_w, THUMB.y), |ui| {
                        ui.set_width(text_w);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&row.label)
                                        .size(t.text.body)
                                        .color(title_color),
                                )
                                .truncate()
                                .selectable(false),
                            )
                            .on_hover_text(row_title(&row.label, index, layers));
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(row_meta(
                                        row.frames,
                                        seconds,
                                        row.visible,
                                        index,
                                        layers,
                                    ))
                                    .size(t.text.small)
                                    .color(meta_color),
                                )
                                .truncate()
                                .selectable(false),
                            );
                        });
                    })
                    .response;
                // The title is the row's drag handle; the thumbnails drag images.
                let handle = ui.interact(
                    title.rect,
                    ui.id().with(("row-handle", index)),
                    Sense::drag(),
                );
                handle.dnd_set_drag_payload(Payload::Row(index));
                ui.add_space(t.space.sm);
                let (glyph, label) = if row.visible {
                    (Icon::Eye, "Hide sequence")
                } else {
                    (Icon::EyeOff, "Show sequence")
                };
                let eye = kit::icon_button(ui, glyph, label, false, true, EYE)
                    .on_hover_text("Show or hide (Shift-click: solo)");
                if eye.clicked() {
                    let solo = ui.input(|i| i.modifiers.shift || i.modifiers.command);
                    actions.push(ListAction::Visibility { index, solo });
                }
            });
        });
    let response = inner.response.interact(Sense::click());
    let fill = if row.selected {
        c.accent
    } else if response.hovered() {
        c.field
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().set(
        background,
        RectShape::filled(response.rect, CornerRadius::same(t.radius.card), fill),
    );
    if response.clicked() {
        actions.push(ListAction::Select(index));
    }
    // Rows are drop targets by pointer position: the title handle and thumbnails lie on top
    // of the row, which would hide the row from a hover test.
    let over_row = ui
        .input(|i| i.pointer.interact_pos())
        .is_some_and(|p| response.rect.contains(p));
    if over_row {
        let carried = egui::DragAndDrop::payload::<Payload>(ui.ctx());
        if let Some(Payload::Row(_)) = carried.as_deref() {
            ui.painter().rect_stroke(
                response.rect,
                CornerRadius::same(t.radius.card),
                Stroke::new(2.0_f32, c.focus),
                egui::StrokeKind::Inside,
            );
            if ui.input(|i| i.pointer.any_released()) {
                if let Some(p) = egui::DragAndDrop::take_payload::<Payload>(ui.ctx()) {
                    thumb_drops.push((*p, Target::Row(index)));
                }
            }
        }
    }
    for (payload, target) in thumb_drops {
        actions.extend(drop_action(payload, target));
    }
    response.context_menu(|ui| {
        let mut item = |ui: &mut Ui, text: &str, enabled: bool, a: ListAction| {
            if ui.add_enabled(enabled, egui::Button::new(text)).clicked() {
                actions.push(a);
                ui.close_menu();
            }
        };
        item(ui, "Rename...", true, ListAction::Rename(index));
        item(ui, "Duplicate", true, ListAction::Duplicate(index));
        item(ui, "Link to previous", index > 0, ListAction::Link(index));
        ui.separator();
        item(ui, "Move up", index > 0, ListAction::Move(index, "up"));
        item(
            ui,
            "Move down",
            index < last,
            ListAction::Move(index, "down"),
        );
        item(ui, "Move to top", index > 0, ListAction::Move(index, "top"));
        item(
            ui,
            "Move to bottom",
            index < last,
            ListAction::Move(index, "bottom"),
        );
        ui.separator();
        item(ui, "Delete", true, ListAction::Delete(index));
    });
}

/// Metadata text: the theme's small size in tertiary grey.
fn meta(ui: &Ui, text: String) -> egui::RichText {
    let t = crate::theme::get(ui.ctx());
    egui::RichText::new(text)
        .size(t.text.small)
        .color(t.color.text_tertiary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_dropped_on_another_row_or_its_thumbnail_moves() {
        let want = Some(ListAction::MoveTo { from: 3, to: 1 });
        assert_eq!(drop_action(Payload::Row(3), Target::Row(1)), want);
        assert_eq!(
            drop_action(
                Payload::Row(3),
                Target::Thumb {
                    sequence: 1,
                    image: 2
                }
            ),
            want
        );
        assert_eq!(drop_action(Payload::Row(2), Target::Row(2)), None);
    }

    #[test]
    fn an_image_dropped_on_another_thumbnail_copies() {
        let from = Payload::Frame {
            sequence: 0,
            image: 2,
        };
        assert_eq!(
            drop_action(
                from,
                Target::Thumb {
                    sequence: 3,
                    image: 1
                }
            ),
            Some(ListAction::CopyFrame {
                from: 0,
                from_image: 2,
                to: 3,
                to_image: 1
            })
        );
        // Onto itself, or onto a bare row, nothing happens.
        assert_eq!(
            drop_action(
                from,
                Target::Thumb {
                    sequence: 0,
                    image: 2
                }
            ),
            None
        );
        assert_eq!(drop_action(from, Target::Row(1)), None);
        // The other image of the same sequence is a valid copy.
        assert!(drop_action(
            from,
            Target::Thumb {
                sequence: 0,
                image: 1
            }
        )
        .is_some());
    }

    #[test]
    fn layer_rows_say_where_they_sit() {
        assert_eq!(row_title("a", 0, false), "a");
        assert_eq!(row_title("a", 0, true), "a  (layer 1, top)");
        assert_eq!(row_title("b", 2, true), "b  (layer 3, below)");
        let (left, right) = summary(true, 10, 0.5, 2, 3);
        assert!(left.starts_with("Stacked") && left.contains("10 frames"));
        assert_eq!(right, "2 of 3 shown");
        assert!(summary(false, 10, 0.5, 2, 3).0.starts_with("Movie"));
        assert_eq!(row_meta(12, 0.5, true, 0, false), "12 frames \u{b7} 0.50 s");
        assert!(row_meta(12, 0.5, false, 1, true).starts_with("Hidden \u{b7} Layer 2, below"));
    }
}
