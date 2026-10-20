//! The Face Model window: which landmarks the face model uses, which are important, its guide
//! lines, presets, and saving or loading a model file.
//!
//! The window draws the model on the unit face box. Click a landmark to select it, drag it to
//! move it (one undo step per drag), and use the list for the rest. Every change runs
//! `face.model`.

use super::*;
use egui::{Color32, Sense, Stroke};

/// What the face windows (model, mix, library) remember between frames.
#[derive(Default)]
pub(super) struct FaceUi {
    // Face model window.
    pub model_selected: Option<usize>,
    pub guide_from: Option<usize>,
    pub model_drag: Option<usize>,
    pub model_group_open: bool,
    pub canvas: Option<Rect>,
    // Mix window.
    pub mix: super::face_mix_ui::MixUi,
    // Library window.
    pub library: super::face_library_ui::LibraryUi,
}

/// The landmark nearest to `at` within `reach`, among `points` (screen positions).
pub(crate) fn nearest_landmark(points: &[(usize, Pos2)], at: Pos2, reach: f32) -> Option<usize> {
    points
        .iter()
        .map(|(i, p)| (*i, p.distance(at)))
        .filter(|(_, d)| *d <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

const CANVAS: Vec2 = Vec2::new(280.0, 340.0);
const REACH: f32 = 9.0;

impl InbetweenApp {
    /// Where the Face Model window drew its landmark canvas last frame (for tests).
    pub fn face_model_canvas(&self) -> Option<Rect> {
        self.face.canvas
    }

    pub fn open_face_model(&mut self) {
        self.state.face_model_window = true;
    }

    pub fn close_face_model(&mut self) {
        self.end_model_drag();
        self.state.face_model_window = false;
    }

    fn end_model_drag(&mut self) {
        self.face.model_drag = None;
        if self.face.model_group_open {
            let _ = self.run_command("edit.endGroup", &json!({}));
            self.face.model_group_open = false;
        }
    }

    /// Screen position of unit-box point `(x, y)` in the canvas.
    fn model_point(rect: Rect, x: f64, y: f64) -> Pos2 {
        Pos2::new(
            rect.min.x + rect.width() * x as f32,
            rect.min.y + rect.height() * y as f32,
        )
    }

    pub(super) fn face_model_window(&mut self, ctx: &Context) {
        if !self.state.face_model_window {
            return;
        }
        let model = self.session.project().face.clone();
        let mut open = true;
        let mut ops: Vec<Value> = Vec::new();
        let mut rename: Option<String> = None;
        let (mut save, mut load) = (false, false);
        let mut released = false;
        let mut selected = self.face.model_selected.filter(|i| *i < model.dots.len());
        let mut guide_from = self.face.guide_from.filter(|i| *i < model.dots.len());
        egui::Window::new("Face Model")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos([260.0, 70.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Preset");
                    for name in inbetween_doc::FACE_PRESETS {
                        if ui.button(*name).clicked() {
                            ops.push(json!({ "op": "preset", "preset": name }));
                            selected = None;
                            guide_from = None;
                        }
                    }
                    if ui.button("Save...").clicked() {
                        save = true;
                    }
                    if ui.button("Load...").clicked() {
                        load = true;
                    }
                });
                let mut name = model.name.clone();
                ui.horizontal(|ui| {
                    ui.label("Name");
                    if ui.text_edit_singleline(&mut name).lost_focus() && name != model.name {
                        rename = Some(name.clone());
                    }
                });
                ui.label(format!(
                    "{} landmarks, {} used",
                    model.dots.len(),
                    model.active_count()
                ));
                ui.horizontal_top(|ui| {
                    let (rect, resp) = ui.allocate_exact_size(CANVAS, Sense::click_and_drag());
                    self.face.canvas = Some(rect);
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, 2.0, Color32::from_gray(28));
                    painter.rect_stroke(
                        rect,
                        2.0,
                        Stroke::new(1.0_f32, Color32::from_gray(90)),
                        egui::StrokeKind::Inside,
                    );
                    let at = |i: usize| {
                        model
                            .dots
                            .get(i)
                            .map(|d| Self::model_point(rect, d.x, d.y))
                    };
                    for g in &model.guides {
                        if let (Some(a), Some(b)) = (at(g[0]), at(g[1])) {
                            painter.line_segment(
                                [a, b],
                                Stroke::new(1.0_f32, Color32::from_rgb(90, 140, 200)),
                            );
                        }
                    }
                    let points: Vec<(usize, Pos2)> = model
                        .dots
                        .iter()
                        .enumerate()
                        .filter_map(|(i, d)| Some((i, at(i).filter(|_| d.enabled)?)))
                        .collect();
                    for (i, d) in model.dots.iter().enumerate() {
                        let Some(p) = at(i) else { continue };
                        let (radius, colour) = match (d.enabled, d.important) {
                            (false, _) => (2.5, Color32::from_gray(80)),
                            (true, true) => (5.5, Color32::from_rgb(255, 170, 40)),
                            (true, false) => (3.5, Color32::from_gray(220)),
                        };
                        painter.circle_filled(p, radius, colour);
                        if Some(i) == selected {
                            painter.circle_stroke(
                                p,
                                radius + 3.0,
                                Stroke::new(1.5_f32, Color32::from_rgb(0, 200, 255)),
                            );
                        }
                        if Some(i) == guide_from {
                            painter.circle_stroke(
                                p,
                                radius + 5.0,
                                Stroke::new(1.5_f32, Color32::from_rgb(120, 220, 120)),
                            );
                        }
                    }
                    // Pick on press so the drag starts on what was under the pointer.
                    if resp.drag_started() || resp.clicked() {
                        let pressed = ui
                            .input(|i| i.pointer.press_origin())
                            .or_else(|| resp.interact_pointer_pos());
                        if let Some(hit) = pressed.and_then(|p| nearest_landmark(&points, p, REACH)) {
                            match guide_from.take() {
                                Some(a) if a != hit && resp.clicked() => {
                                    ops.push(json!({ "op": "guide_add", "index": a, "to": hit }));
                                    selected = Some(hit);
                                }
                                _ => {
                                    selected = Some(hit);
                                    if resp.drag_started() {
                                        self.face.model_drag = Some(hit);
                                        if self.run_command("edit.beginGroup", &json!({})).is_ok() {
                                            self.face.model_group_open = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let (Some(i), true, Some(p)) = (
                        self.face.model_drag,
                        resp.dragged(),
                        resp.interact_pointer_pos(),
                    ) {
                        let x = f64::from((p.x - rect.min.x) / rect.width().max(1.0));
                        let y = f64::from((p.y - rect.min.y) / rect.height().max(1.0));
                        ops.push(json!({
                            "op": "move",
                            "index": i,
                            "x": x.clamp(-0.5, 1.5),
                            "y": y.clamp(-0.5, 1.5),
                        }));
                    }
                    if resp.drag_stopped() {
                        released = true;
                    }

                    ui.vertical(|ui| {
                        ui.set_min_width(260.0);
                        egui::ScrollArea::vertical()
                            .max_height(CANVAS.y - 70.0)
                            .id_salt("face_model_list")
                            .show(ui, |ui| {
                                for (i, d) in model.dots.iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        let mut on = d.enabled;
                                        if ui.checkbox(&mut on, "").on_hover_text("Use").changed() {
                                            ops.push(json!({ "op": "enable", "index": i, "on": on }));
                                        }
                                        let mut star = d.important;
                                        if ui
                                            .toggle_value(&mut star, "\u{2605}")
                                            .on_hover_text("Important: highlighted, held stiffer by smart drag")
                                            .changed()
                                        {
                                            ops.push(json!({ "op": "important", "index": i, "on": star }));
                                        }
                                        if ui
                                            .selectable_label(selected == Some(i), &d.name)
                                            .clicked()
                                        {
                                            selected = Some(i);
                                        }
                                    });
                                }
                            });
                        ui.separator();
                        ui.horizontal(|ui| {
                            let have = selected.is_some();
                            if ui
                                .add_enabled(have, egui::Button::new("Guide from selected..."))
                                .on_hover_text("Then click the other landmark")
                                .clicked()
                            {
                                guide_from = selected;
                            }
                            if guide_from.is_some() {
                                ui.weak("click the second landmark");
                            }
                        });
                        ui.label("Guide lines");
                        egui::ScrollArea::vertical()
                            .max_height(70.0)
                            .id_salt("face_model_guides")
                            .show(ui, |ui| {
                                for (g, pair) in model.guides.iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        let label = |i: usize| {
                                            model.dots.get(i).map_or("?", |d| d.name.as_str())
                                        };
                                        ui.label(format!("{} - {}", label(pair[0]), label(pair[1])));
                                        if ui.small_button("Remove").clicked() {
                                            ops.push(json!({ "op": "guide_remove", "guide": g }));
                                        }
                                    });
                                }
                            });
                    });
                });
                ui.weak("Drag a landmark to move it. Orange = important.");
            });

        if released {
            self.end_model_drag();
        }
        self.face.model_selected = selected;
        self.face.guide_from = guide_from;
        for op in ops {
            let _ = self.run_command("face.model", &op);
        }
        if let Some(name) = rename {
            let _ = self.run_command("face.model", &json!({ "op": "rename", "name": name }));
        }
        if save {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Face model", &["inbetweenface"])
                .save_file()
            {
                let _ = self.run_command(
                    "face.model",
                    &json!({ "op": "save", "path": p.display().to_string() }),
                );
            }
        }
        if load {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Face model", &["inbetweenface"])
                .pick_file()
            {
                let _ = self.run_command(
                    "face.model",
                    &json!({ "op": "load", "path": p.display().to_string() }),
                );
            }
        }
        if !open {
            self.close_face_model();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_landmark_within_reach_wins() {
        let pts = [
            (0, Pos2::new(10.0, 10.0)),
            (1, Pos2::new(14.0, 10.0)),
            (2, Pos2::new(100.0, 100.0)),
        ];
        assert_eq!(nearest_landmark(&pts, Pos2::new(13.0, 10.0), 9.0), Some(1));
        assert_eq!(nearest_landmark(&pts, Pos2::new(50.0, 50.0), 9.0), None);
        assert_eq!(nearest_landmark(&[], Pos2::ZERO, 9.0), None);
    }
}
