//! The sequence list panel: one row per sequence with both thumbnails, its name and an eye
//! toggle. It only reports what was clicked or dropped ([`ListAction`]); the app runs the
//! matching `sequence.*` commands.
//!
//! Dragging a row onto another row moves it there (`sequence.reorder`). Dragging a thumbnail
//! onto another row's thumbnail copies that image with its dots (`sequence.copyFrame`).

use egui::{Color32, Sense, Stroke, TextureId, Ui, Vec2};
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

/// The line under the list.
pub fn summary(layers: bool, movie_frames: u64, shown: usize, total: usize) -> String {
    let kind = if layers {
        "stacked"
    } else {
        "one after another"
    };
    format!("Movie: {movie_frames} frames from {shown} of {total} sequences, {kind}")
}

const THUMB: Vec2 = Vec2::new(44.0, 30.0);

fn thumb(ui: &mut Ui, id: Option<TextureId>, dim: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(THUMB, Sense::drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, Color32::from_gray(30));
    if let Some(id) = id {
        let tint = if dim {
            Color32::from_gray(90)
        } else {
            Color32::WHITE
        };
        painter.image(
            id,
            rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
            tint,
        );
    }
    response
}

fn hovered_payload(response: &egui::Response) -> Option<Arc<Payload>> {
    response.dnd_hover_payload::<Payload>()
}

/// Draw the list; returns the actions the user asked for. `layers` is true for a layer
/// project, where rows are stacked layers rather than steps in time.
pub fn sequence_list(
    ui: &mut Ui,
    rows: &[Row],
    movie_frames: u64,
    layers: bool,
) -> Vec<ListAction> {
    let mut actions = Vec::new();
    ui.horizontal(|ui| {
        ui.heading("Sequences");
        let mut kind = if layers { "layer" } else { "sequence" };
        let before = kind;
        egui::ComboBox::from_id_salt("project_type")
            .selected_text(if layers { "Layers" } else { "In order" })
            .width(80.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut kind, "sequence", "In order")
                    .on_hover_text("Sequences play one after another");
                ui.selectable_value(&mut kind, "layer", "Layers")
                    .on_hover_text("Sequences play at once, the first on top");
            });
        if kind != before {
            actions.push(ListAction::ProjectType(kind));
        }
    });
    let last = rows.len().saturating_sub(1);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height((ui.available_height() - 70.0).max(60.0))
        .show(ui, |ui| {
            for (index, row) in rows.iter().enumerate() {
                let frame = egui::Frame::new()
                    .inner_margin(4.0)
                    .corner_radius(3.0)
                    .fill(if row.selected {
                        Color32::from_rgb(40, 62, 92)
                    } else {
                        Color32::TRANSPARENT
                    });
                let mut thumb_drops = Vec::new();
                let inner = frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let eye = if row.visible { "on" } else { "off" };
                        let eye_response = ui
                            .small_button(eye)
                            .on_hover_text("Show or hide (Shift-click: solo)");
                        if eye_response.clicked() {
                            let solo = ui.input(|i| i.modifiers.shift || i.modifiers.command);
                            actions.push(ListAction::Visibility { index, solo });
                        }
                        for slot in 0..2 {
                            let image = slot as u8 + 1;
                            let response = thumb(ui, row.thumbs[slot], !row.visible);
                            response.dnd_set_drag_payload(Payload::Frame {
                                sequence: index,
                                image,
                            });
                            if let Some(p) = hovered_payload(&response) {
                                if matches!(*p, Payload::Frame { .. }) {
                                    ui.painter().rect_stroke(
                                        response.rect,
                                        2.0,
                                        Stroke::new(2.0_f32, Color32::from_rgb(120, 200, 255)),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                            }
                            if hovered_payload(&response)
                                .is_some_and(|p| matches!(*p, Payload::Frame { .. }))
                            {
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
                        let title = ui.vertical(|ui| {
                            let text = egui::RichText::new(row_title(&row.label, index, layers));
                            ui.label(if row.visible {
                                text
                            } else {
                                text.weak().strikethrough()
                            });
                            ui.weak(format!("{} frames", row.frames));
                        });
                        // The title is the row's drag handle; the thumbnails drag images.
                        let handle = ui.interact(
                            title.response.rect,
                            ui.id().with(("row-handle", index)),
                            Sense::drag(),
                        );
                        handle.dnd_set_drag_payload(Payload::Row(index));
                    });
                });
                let response = inner.response.interact(Sense::click());
                if response.clicked() {
                    actions.push(ListAction::Select(index));
                }
                // Rows are drop targets by pointer position: the title handle and thumbnails
                // lie on top of the row, which would hide the row from a hover test.
                let over_row = ui
                    .input(|i| i.pointer.interact_pos())
                    .is_some_and(|p| response.rect.contains(p));
                if over_row {
                    let carried = egui::DragAndDrop::payload::<Payload>(ui.ctx());
                    if let Some(Payload::Row(_)) = carried.as_deref() {
                        ui.painter().rect_stroke(
                            response.rect,
                            3.0,
                            Stroke::new(2.0_f32, Color32::from_rgb(255, 200, 90)),
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
            if rows.is_empty() {
                ui.weak("No sequences yet.");
            }
        });
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("+ Blank").clicked() {
            actions.push(ListAction::AddBlank);
        }
        if ui
            .button("+ Continued...")
            .on_hover_text("Add a sequence that starts where the last one ends")
            .clicked()
        {
            actions.push(ListAction::AddContinued);
        }
    });
    let shown = rows.iter().filter(|r| r.visible).count();
    ui.weak(summary(layers, movie_frames, shown, rows.len()));
    actions
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
        assert!(summary(true, 10, 2, 3).ends_with("stacked"));
        assert!(summary(false, 10, 2, 3).ends_with("one after another"));
    }
}
