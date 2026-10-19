//! The sequence list panel: one row per sequence with both thumbnails, its name and an eye
//! toggle. It only reports what was clicked ([`ListAction`]); the app runs the matching
//! `sequence.*` commands.

use egui::{Color32, Sense, TextureId, Ui, Vec2};

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
    Duplicate(usize),
    Delete(usize),
    Rename(usize),
    Link(usize),
    AddBlank,
    AddContinued,
}

const THUMB: Vec2 = Vec2::new(44.0, 30.0);

fn thumb(ui: &mut Ui, id: Option<TextureId>, dim: bool) {
    let (rect, _) = ui.allocate_exact_size(THUMB, Sense::hover());
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
}

/// Draw the list; returns the actions the user asked for.
pub fn sequence_list(ui: &mut Ui, rows: &[Row], movie_frames: u64) -> Vec<ListAction> {
    let mut actions = Vec::new();
    ui.heading("Sequences");
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
                        thumb(ui, row.thumbs[0], !row.visible);
                        thumb(ui, row.thumbs[1], !row.visible);
                        ui.vertical(|ui| {
                            let text = egui::RichText::new(&row.label);
                            ui.label(if row.visible {
                                text
                            } else {
                                text.weak().strikethrough()
                            });
                            ui.weak(format!("{} frames", row.frames));
                        });
                    });
                });
                let response = inner.response.interact(Sense::click());
                if response.clicked() {
                    actions.push(ListAction::Select(index));
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
    ui.weak(format!(
        "Movie: {movie_frames} frames from {shown} of {} sequences",
        rows.len()
    ));
    actions
}
