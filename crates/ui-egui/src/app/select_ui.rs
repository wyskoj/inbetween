//! Dot editing in the shell: the tool strip, selection, marquee, line drawing, dragging a
//! selection or a line, nudging, and the key bindings for them. Everything goes through
//! engine commands; this file only decides which one.

use super::*;
use crate::state::Tool;

impl InbetweenApp {
    /// Turn what the person did on a source panel into engine commands.
    pub(super) fn apply_dots(&mut self, action: Action) {
        match action {
            Action::Add { x, y, image, same } => {
                if self.prepared().is_none() {
                    self.status = Some(Status {
                        text: "Load both images before placing dots.".to_string(),
                        is_error: true,
                    });
                    return;
                }
                let (id, params) = if same {
                    ("dots.addSamePos", json!({ "x": x, "y": y }))
                } else {
                    ("dots.add", json!({ "x": x, "y": y, "image": image }))
                };
                if let Ok(r) = self.run_command(id, &params) {
                    if let Some(i) = r["index"].as_u64() {
                        let _ = self.run_command("dots.select", &json!({ "indices": [i] }));
                        self.queue_jump(i as usize, image);
                    }
                }
            }
            Action::SelectDot { index, mode } => {
                let _ = self.run_command(
                    "dots.select",
                    &json!({ "mode": mode.id(), "indices": [index] }),
                );
            }
            Action::SelectLine { index, mode } => {
                let _ =
                    self.run_command("dots.select", &json!({ "mode": mode.id(), "line": index }));
            }
            Action::SelectNone => {
                let _ = self.run_command("dots.selectNone", &json!({}));
            }
            Action::Marquee { image, rect, mode } => {
                let _ = self.run_command(
                    "dots.select",
                    &json!({ "mode": mode.id(), "rect": rect, "image": image }),
                );
            }
            Action::BeginDrag { .. } => {
                if self.run_command("edit.beginGroup", &json!({})).is_ok() {
                    self.group_open = true;
                }
            }
            Action::Drag { index, image, x, y } => self.drag_selection(index, image, x, y),
            Action::EndDrag => self.end_group(),
            Action::Delete(i) => {
                self.chain = None;
                let _ = self.run_command("dots.delete", &json!({ "index": i }));
            }
            Action::LineClick {
                x,
                y,
                image,
                dot,
                finish,
            } => self.line_click(x, y, image, dot, finish),
            Action::LineEnd => self.chain = None,
            Action::InsertOnLine { line, x, y, image } => {
                if let Ok(r) = self.run_command(
                    "lines.insertDot",
                    &json!({ "line": line, "x": x, "y": y, "image": image }),
                ) {
                    if let Some(i) = r["index"].as_u64() {
                        let _ = self.run_command("dots.select", &json!({ "indices": [i] }));
                    }
                }
            }
            Action::LineDragBegin { .. } => {
                if self.run_command("edit.beginGroup", &json!({})).is_ok() {
                    self.group_open = true;
                }
            }
            Action::LineDrag {
                line,
                image,
                dx,
                dy,
            } => {
                let _ = self.run_command(
                    "lines.drag",
                    &json!({ "line": line, "image": image, "dx": dx, "dy": dy }),
                );
            }
            Action::LineDragEnd => self.end_group(),
            _ => {}
        }
    }

    /// Where dot `index` is on `image`, from the project.
    fn dot_end(&self, index: usize, image: u8) -> Option<inbetween_geom::Point> {
        let seq = self.session.project().sequences.get(self.state.sequence)?;
        let pair = seq.dots.pairs.get(index)?;
        pair.has_end(image).then(|| pair.end(image))
    }

    /// Move the whole selection so the grabbed dot follows the pointer to (x, y).
    fn drag_selection(&mut self, index: usize, image: u8, x: f64, y: f64) {
        let Some(now) = self.dot_end(index, image) else {
            return;
        };
        let (dx, dy) = (x - now.x, y - now.y);
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        let mut params = json!({ "dx": dx, "dy": dy, "image": image });
        if !self.session.selection().contains(&index) {
            params["indices"] = json!([index]);
        }
        let _ = self.run_command("dots.nudge", &params);
    }

    /// One click of the Line tool: a dot (new, or the one clicked) joined to the last.
    fn line_click(&mut self, x: f64, y: f64, image: u8, dot: Option<usize>, finish: bool) {
        if self.prepared().is_none() {
            return;
        }
        let began = self.run_command("edit.beginGroup", &json!({})).is_ok();
        let index = match dot {
            Some(d) => Some(d),
            None => self
                .run_command("dots.add", &json!({ "x": x, "y": y, "image": image }))
                .ok()
                .and_then(|r| r["index"].as_u64())
                .map(|i| i as usize),
        };
        if let Some(i) = index {
            if let Some(prev) = self.chain.filter(|p| *p != i) {
                let _ = self.run_command("lines.connect", &json!({ "a": prev, "b": i }));
            }
            let _ = self.run_command("dots.select", &json!({ "indices": [i] }));
        }
        if began {
            let _ = self.run_command("edit.endGroup", &json!({}));
        }
        self.chain = if finish { None } else { index };
    }

    /// With auto-jump on, move the pointer to where the partner of a new dot is.
    fn queue_jump(&mut self, index: usize, image: u8) {
        if !self.session.prefs().auto_jump {
            return;
        }
        let other = 3 - image;
        let Some(p) = self.dot_end(index, other) else {
            return;
        };
        let rect = if other == 1 {
            self.layout.source1
        } else {
            self.layout.source2
        };
        let view = self.state.views[usize::from(other) - 1];
        self.jump = Some(view.to_screen(rect, Pos2::new(p.x as f32, p.y as f32)));
    }

    /// The tool strip above the canvases.
    pub(super) fn tool_strip(&mut self, ctx: &Context) {
        let mut tool = self.state.tool;
        let prefs = *self.session.prefs();
        let mut auto_partner = prefs.auto_partner;
        let mut magnifier = prefs.magnifier;
        let mut power = i64::from(prefs.magnifier_power);
        egui::TopBottomPanel::top("tools").show(ctx, |ui| {
            ui.horizontal(|ui| {
                for t in Tool::ALL {
                    ui.selectable_value(&mut tool, t, t.label());
                }
                ui.separator();
                ui.checkbox(&mut auto_partner, "Auto partner")
                    .on_hover_text("Adding a dot also places its partner on the other image");
                ui.checkbox(&mut magnifier, "Magnifier");
                if magnifier {
                    ui.add(egui::DragValue::new(&mut power).range(1..=4).prefix("x"));
                }
                let n = self.session.selection().len();
                if n > 0 {
                    ui.separator();
                    ui.label(format!("{n} selected"));
                }
            });
        });
        if tool != self.state.tool {
            self.state.tool = tool;
            self.chain = None;
        }
        if auto_partner != prefs.auto_partner {
            let _ = self.run_command("dots.option.autoPartner", &json!({ "on": auto_partner }));
        }
        if magnifier != prefs.magnifier || power != i64::from(prefs.magnifier_power) {
            let _ = self.run_command(
                "view.magnifier",
                &json!({ "on": magnifier, "power": power }),
            );
        }
    }

    /// Keys for dots: tools, magnifier power, nudging, and ending a line. Returns true when a
    /// key was used and the rest of the key handling should be skipped.
    pub(super) fn dot_keys(&mut self, ctx: &Context) -> bool {
        use egui::Key;
        let (mods, pressed) = ctx.input(|i| {
            let k = |key| i.key_pressed(key);
            (
                i.modifiers,
                [
                    k(Key::E),
                    k(Key::A),
                    k(Key::D),
                    k(Key::L),
                    k(Key::N),
                    k(Key::M),
                    k(Key::ArrowLeft),
                    k(Key::ArrowRight),
                    k(Key::ArrowUp),
                    k(Key::ArrowDown),
                    k(Key::Enter),
                    k(Key::Escape),
                    k(Key::B),
                    k(Key::F),
                ],
            )
        });
        let [e, a, d, l, n, m, left, right, up, down, enter, escape, b, f] = pressed;
        let plain = !mods.command && !mods.alt && !mods.shift;
        if plain {
            for (key, tool) in [
                (e, Tool::Edit),
                (a, Tool::Add),
                (d, Tool::Delete),
                (l, Tool::Line),
            ] {
                if key {
                    self.state.tool = tool;
                    self.chain = None;
                    return true;
                }
            }
            if n || m {
                let power = i64::from(self.session.prefs().magnifier_power);
                let power = if m { power + 1 } else { power - 1 }.clamp(1, 4);
                let _ = self.run_command("view.magnifier", &json!({ "power": power }));
                return true;
            }
        }
        if mods.command && !mods.alt {
            if b && self.transport_pos() > 0 {
                self.step_transport(-1);
                return true;
            }
            if f && self.transport_pos() + 1 < self.transport_frames() {
                self.step_transport(1);
                return true;
            }
        }
        if enter && self.state.tool == Tool::Line {
            self.chain = None;
        }
        if escape {
            self.chain = None;
            let _ = self.run_command("dots.selectNone", &json!({}));
        }
        let (dx, dy) = (
            f64::from(i32::from(right) - i32::from(left)),
            f64::from(i32::from(down) - i32::from(up)),
        );
        if (dx != 0.0 || dy != 0.0) && !self.session.selection().is_empty() && !mods.alt {
            let step = if mods.command { 10.0 } else { 1.0 };
            let image = self.state.focus_image;
            let _ = self.run_command(
                "dots.nudge",
                &json!({ "dx": dx * step, "dy": dy * step, "image": image }),
            );
            return true;
        }
        false
    }
}
