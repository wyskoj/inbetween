//! The unsaved-changes prompt and the window close request.

use super::*;

impl InbetweenApp {
    pub(super) fn handle_close(&mut self, ctx: &Context) {
        if self.close_now {
            self.close_now = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.screenshot.is_some() || !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        if !self.request_quit() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }

    pub(super) fn prompt_window(&mut self, ctx: &Context) {
        let Some(pending) = self.pending.clone() else {
            return;
        };
        let name = self.session.path().and_then(|p| p.file_name()).map_or_else(
            || "this project".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let what = match pending.id.as_str() {
            QUIT => "before quitting",
            "file.close" => "before closing it",
            _ => "before replacing it",
        };
        let mut choice = None;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("Save the changes to {name} {what}?"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        choice = Some(PromptChoice::Save);
                    }
                    if ui.button("Don't Save").clicked() {
                        choice = Some(PromptChoice::DontSave);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(PromptChoice::Cancel);
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            choice = Some(PromptChoice::Cancel);
        }
        if let Some(c) = choice {
            self.answer_prompt(c);
        }
    }
}
