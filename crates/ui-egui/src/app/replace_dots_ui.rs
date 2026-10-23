//! The Replace-the-dots question for the Face menu's marking items (desktop app only: the CLI
//! and MCP never ask).

use super::*;
use crate::theme;
use egui::{Frame, Margin};

/// The Face menu items that replace a sequence's dots with a face.
const MARKS: &[&str] = &["face.locate", "face.locateAll", "face.locateSequences"];

/// A marking command held back until the person agrees to replace the existing dots.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ReplaceDots {
    id: String,
    message: String,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl InbetweenApp {
    /// The question to ask before running `id`, if it would replace existing dots.
    pub(super) fn replace_dots_question(&self, id: &str) -> Option<ReplaceDots> {
        if !MARKS.contains(&id) {
            return None;
        }
        let faces = if id == "face.locateAll" {
            "faces"
        } else {
            "the face"
        };
        let message = if id == "face.locateSequences" {
            let all = self.session.project().sequences.len();
            let with = self
                .session
                .project()
                .sequences
                .iter()
                .filter(|s| !s.dots.pairs.is_empty())
                .count();
            if with == 0 {
                return None;
            }
            format!("{with} of {all} sequences already have dots. Marking {faces} replaces them.")
        } else {
            let seq = self.session.sequence(self.state.sequence).ok()?;
            let dots = seq.dots.pairs.len();
            if dots == 0 {
                return None;
            }
            let lines = seq.dots.lines.len();
            let have = if lines == 0 {
                plural(dots, "dot", "dots")
            } else {
                format!(
                    "{} and {}",
                    plural(dots, "dot", "dots"),
                    plural(lines, "line", "lines")
                )
            };
            format!("This sequence has {have}. Marking {faces} replaces them.")
        };
        Some(ReplaceDots {
            id: id.to_string(),
            message,
        })
    }

    /// Whether the Replace-the-dots question is showing.
    pub fn replace_dots_open(&self) -> bool {
        self.replace_dots.is_some()
    }

    /// Answer the question: `true` replaces the dots, `false` leaves them.
    pub fn answer_replace_dots(&mut self, replace: bool) {
        let Some(ask) = self.replace_dots.take() else {
            return;
        };
        if replace {
            if let Err(e) = self.run_command(&ask.id, &json!({})) {
                // No face found: the form lets a face box be given instead.
                if let (CommandError::BadParams(_), Some(spec)) =
                    (&e, self.registry.get(&ask.id).copied())
                {
                    let mut form = Form::for_spec(&spec);
                    form.error = Some(e.to_string());
                    self.form = Some(form);
                }
            }
        }
    }

    pub(super) fn replace_dots_window(&mut self, ctx: &Context) {
        let Some(ask) = self.replace_dots.clone() else {
            return;
        };
        let mut choice = None;
        let t = theme::get(ctx);
        egui::Modal::new(egui::Id::new("replace_dots")).show(ctx, |ui| {
            Frame::NONE
                .inner_margin(Margin::same(t.space.xl as i8))
                .show(ui, |ui| {
                    ui.heading("Replace the dots?");
                    ui.add_space(t.space.sm);
                    ui.label(&ask.message);
                    ui.add_space(t.space.xl);
                    ui.horizontal(|ui| {
                        if ui.button("Replace").clicked() {
                            choice = Some(true);
                        }
                        if ui.button("Cancel").clicked() {
                            choice = Some(false);
                        }
                    });
                });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            choice = Some(false);
        }
        if let Some(c) = choice {
            self.answer_replace_dots(c);
        }
    }
}
