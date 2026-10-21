//! The parameter form window and its file pickers.

use super::*;

/// The editor for a list of paths: one row per path with move and remove buttons, and an
/// Add button that opens a multi-select file dialog.
pub(super) fn path_list_editor(ui: &mut egui::Ui, field: &mut crate::form::Field) {
    ui.vertical(|ui| {
        let mut remove = None;
        let mut swap = None;
        let count = field.list.len();
        for (i, path) in field.list.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{}.", i + 1));
                let name = std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                ui.label(name).on_hover_text(path);
                if ui.add_enabled(i > 0, egui::Button::new("Up")).clicked() {
                    swap = Some((i - 1, i));
                }
                if ui
                    .add_enabled(i + 1 < count, egui::Button::new("Down"))
                    .clicked()
                {
                    swap = Some((i, i + 1));
                }
                if ui.button("Remove").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some((a, b)) = swap {
            field.list.swap(a, b);
        }
        if let Some(i) = remove {
            field.list.remove(i);
        }
        if ui.button("Add...").clicked() {
            field.list.extend(browse_many(&field.spec));
        }
    });
}

pub(super) fn browse_many(spec: &inbetween_engine::ParamSpec) -> Vec<String> {
    let mut dialog = rfd::FileDialog::new();
    if !spec.extensions.is_empty() {
        dialog = dialog.add_filter("Files", spec.extensions);
    }
    dialog
        .pick_files()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.display().to_string())
        .collect()
}

/// A native file dialog for a path parameter.
pub(super) fn browse(spec: &inbetween_engine::ParamSpec) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();
    if !spec.extensions.is_empty() {
        dialog = dialog.add_filter("Files", spec.extensions);
    }
    let picked = match spec.path_use {
        Some(PathUse::Folder) => dialog.pick_folder(),
        Some(PathUse::SaveFile) => dialog.save_file(),
        Some(PathUse::OpenFile) | None => dialog.pick_file(),
    };
    picked.map(|p| p.display().to_string())
}

impl InbetweenApp {
    pub(super) fn form_window(&mut self, ctx: &Context) {
        let Some(mut form) = self.form.clone() else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        egui::Window::new(&form.title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(520.0);
                ui.label(&form.description);
                ui.add_space(6.0);
                egui::Grid::new("form")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        for field in &mut form.fields {
                            let required = if field.spec.required { " *" } else { "" };
                            ui.label(format!("{}{required}", field.spec.name))
                                .on_hover_text(field.spec.description);
                            ui.horizontal(|ui| {
                                if field.spec.kind == ParamKind::PathList {
                                    path_list_editor(ui, field);
                                } else if field.spec.kind == ParamKind::Bool {
                                    ui.checkbox(&mut field.flag, "");
                                } else {
                                    let hint = field.spec.default.unwrap_or(field.spec.description);
                                    ui.add(
                                        egui::TextEdit::singleline(&mut field.text)
                                            .desired_width(300.0)
                                            .hint_text(hint),
                                    );
                                }
                                if field.spec.kind == ParamKind::Path
                                    && ui.button("Browse...").clicked()
                                {
                                    if let Some(p) = browse(&field.spec) {
                                        field.text = p;
                                    }
                                }
                            });
                            ui.end_row();
                        }
                    });
                if let Some(e) = &form.error {
                    ui.add_space(4.0);
                    ui.colored_label(crate::theme::get(ui.ctx()).color.danger_text, e);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Run").clicked() {
                        submit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            submit = true;
        }
        self.form = Some(form);
        if cancel {
            self.form = None;
        } else if submit {
            self.submit_form();
        }
    }
}
