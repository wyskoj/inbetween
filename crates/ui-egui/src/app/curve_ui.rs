//! Curve editor window and the commands its drags run.

use super::*;

impl InbetweenApp {
    pub(super) fn curve_window(&mut self, ctx: &Context) {
        let Some(mut kind) = self.state.curve_window else {
            return;
        };
        let Ok(seq) = self.session.sequence(self.state.sequence) else {
            return;
        };
        let curve = match kind {
            CurveKind::Feature => seq.feature_curve.clone(),
            CurveKind::Shape => seq.shape_curve.clone(),
        };
        let last = f64::from(seq.frames.saturating_sub(1).max(1));
        let ticks: Vec<f64> = (0..seq.frames.min(100_000))
            .map(|i| f64::from(i) / last)
            .collect();
        let now = seq.frame_time(self.state.frame);
        let mut open = true;
        let mut drag = self.curve_drag;
        let mut actions: Vec<CurveAction> = Vec::new();
        let mut linear = curve.linear;
        egui::Window::new("Curves")
            .open(&mut open)
            .resizable(false)
            .default_pos([260.0, 80.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for k in [CurveKind::Feature, CurveKind::Shape] {
                        ui.selectable_value(&mut kind, k, k.label());
                    }
                });
                ui.weak(kind.help());
                ui.add_space(4.0);
                let view = CurveView {
                    curve: &curve,
                    ticks: &ticks,
                    now,
                };
                actions.extend(curves::curve_editor(ui, &view, &mut drag));
                ui.weak("Click to add a point, drag to move, right-click to remove.");
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut linear, "Linear").changed() {
                        actions.push(CurveAction::SetLinear(linear));
                    }
                    if ui.button("Reset").clicked() {
                        actions.push(CurveAction::Reset);
                    }
                    if ui
                        .button("Apply to all sequences")
                        .on_hover_text("Copy this curve to every sequence")
                        .clicked()
                    {
                        actions.push(CurveAction::ApplyAll);
                    }
                });
            });
        self.curve_drag = drag;
        let command = kind.command();
        for action in actions {
            self.apply_curve(command, action);
        }
        if self.curve_group_open && (self.curve_drag.is_none() || !open) {
            let _ = self.run_command("edit.endGroup", &json!({}));
            self.curve_group_open = false;
            self.curve_drag = None;
        }
        self.state.curve_window = open.then_some(kind);
    }

    /// Run one curve edit as the matching `curve.*` command.
    pub fn apply_curve(&mut self, command: &str, action: CurveAction) {
        let _ = match action {
            CurveAction::Add { x, y } => {
                self.run_command(command, &json!({ "op": "add", "x": x, "y": y }))
            }
            CurveAction::BeginDrag => {
                let r = self.run_command("edit.beginGroup", &json!({}));
                self.curve_group_open = r.is_ok();
                r
            }
            CurveAction::Move { index, x, y } => self.run_command(
                command,
                &json!({ "op": "move", "index": index, "x": x, "y": y }),
            ),
            CurveAction::EndDrag => Ok(Value::Null),
            CurveAction::Remove(i) => {
                self.run_command(command, &json!({ "op": "remove", "index": i }))
            }
            CurveAction::SetLinear(b) => {
                self.run_command(command, &json!({ "op": "linear", "linear": b }))
            }
            CurveAction::Reset => self.run_command(command, &json!({ "op": "reset" })),
            CurveAction::ApplyAll => self.run_command(command, &json!({ "op": "apply_all" })),
        };
    }
}
