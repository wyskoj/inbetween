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
                    self.announce(
                        "Load both images before placing dots.".to_string(),
                        ToastKind::Warning,
                        false,
                    );
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
            Action::Drag {
                index,
                image,
                x,
                y,
                alt,
            } => self.drag_selection(index, image, x, y, alt),
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

    /// A smart drag of face dot `index` to (x, y): the face it belongs to (the project's face
    /// model gives how many dots a face has) follows. `false` when the dot is not on a face the
    /// model could have placed, so the caller drags plainly.
    fn smart_drag(&mut self, index: usize, image: u8, x: f64, y: f64) -> bool {
        let count = self.session.project().face.active_count();
        let dots = self
            .session
            .project()
            .sequences
            .get(self.state.sequence)
            .map_or(0, |s| s.dots.pairs.len());
        if count == 0 || dots < count {
            return false;
        }
        // Faces sit one after another, each as many dots as the model has landmarks.
        let first = index / count * count;
        if first + count > dots {
            return false;
        }
        self.run_command(
            "face.locate.smartDrag",
            &json!({ "index": index, "image": image, "x": x, "y": y, "first": first }),
        )
        .is_ok()
    }

    /// Where dot `index` is on `image`, from the project.
    fn dot_end(&self, index: usize, image: u8) -> Option<inbetween_geom::Point> {
        let seq = self.session.project().sequences.get(self.state.sequence)?;
        let pair = seq.dots.pairs.get(index)?;
        pair.has_end(image).then(|| pair.end(image))
    }

    /// Move the whole selection so the grabbed dot follows the pointer to (x, y).
    fn drag_selection(&mut self, index: usize, image: u8, x: f64, y: f64, alt: bool) {
        if self.session.prefs().smart_drag != alt && self.smart_drag(index, image, x, y) {
            return;
        }
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
        use crate::kit::{self, Icon, Segment};
        let t = crate::theme::get(ctx);
        let mut tool = self.state.tool;
        let prefs = *self.session.prefs();
        let mut auto_partner = prefs.auto_partner;
        let mut auto_jump = prefs.auto_jump;
        let mut magnifier = prefs.magnifier;
        let mut power = i64::from(prefs.magnifier_power);
        let enabled = self.session.has_sequences();
        let selected = self.session.selection().len();
        let mut clear_selection = false;
        let mut overlay_click = None;
        let overlays = [prefs.show_dots, prefs.show_lines, prefs.show_triangles];
        egui::TopBottomPanel::top("tools")
            .frame(
                egui::Frame::NONE
                    .fill(t.color.surface)
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = t.space.sm;
                    let tools = [
                        Segment::icon(Icon::Cursor, "Edit")
                            .key("E")
                            .tip("Select, drag and marquee dots and lines"),
                        Segment::icon(Icon::AddDot, "Add")
                            .key("A")
                            .tip("Click to add a dot (Alt+click: partner at the same place)"),
                        Segment::icon(Icon::RemoveDot, "Delete")
                            .key("D")
                            .tip("Click a dot to delete it"),
                        Segment::icon(Icon::Polyline, "Line")
                            .key("L")
                            .tip("Each click drops a dot joined to the last one"),
                    ];
                    let current = Tool::ALL.iter().position(|x| *x == tool).unwrap_or(0);
                    if let Some(i) =
                        kit::segmented(ui, &tools, current, enabled, t.control.compact_height)
                    {
                        tool = Tool::ALL.get(i).copied().unwrap_or(tool);
                    }
                    ui.add_space(t.space.sm);
                    if kit::switch(ui, "Auto partner", auto_partner, t.control.compact_height)
                        .on_hover_text("Adding a dot also places its partner on the other image")
                        .clicked()
                    {
                        auto_partner = !auto_partner;
                    }
                    if kit::switch(ui, "Jump to partner", auto_jump, t.control.compact_height)
                        .on_hover_text("After adding a dot, the cursor moves to its partner")
                        .clicked()
                    {
                        auto_jump = !auto_jump;
                    }
                    if kit::switch(ui, "Magnifier", magnifier, t.control.compact_height).clicked() {
                        magnifier = !magnifier;
                    }
                    if magnifier {
                        ui.add(
                            egui::DragValue::new(&mut power)
                                .range(1..=4)
                                .prefix("\u{d7}"),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let show = [
                            Segment::icon_only(Icon::Dots, "Show dots"),
                            Segment::icon_only(Icon::Lines, "Show lines"),
                            Segment::icon_only(Icon::Triangles, "Show triangles"),
                        ];
                        overlay_click =
                            kit::toggle_group(ui, &show, &overlays, t.control.compact_height);
                        if selected > 0 {
                            ui.add_space(t.space.sm);
                            egui::Frame::NONE
                                .fill(t.color.field)
                                .corner_radius(egui::CornerRadius::same(t.radius.control))
                                .inner_margin(egui::Margin {
                                    left: 10,
                                    right: 2,
                                    top: 2,
                                    bottom: 2,
                                })
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = t.space.xs;
                                        let noun = if selected == 1 { "dot" } else { "dots" };
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{selected} {noun} selected"
                                            ))
                                            .size(t.text.label)
                                            .color(t.color.text),
                                        );
                                        if kit::icon_button(
                                            ui,
                                            Icon::Close,
                                            "Clear selection",
                                            false,
                                            true,
                                            t.control.compact_height - 4.0,
                                        )
                                        .clicked()
                                        {
                                            clear_selection = true;
                                        }
                                    });
                                });
                        }
                    });
                });
            });
        if clear_selection {
            let _ = self.run_command("dots.selectNone", &json!({}));
        }
        if let Some(i) = overlay_click {
            let ids = [
                "view.toggle.dots",
                "view.toggle.lines",
                "view.toggle.triangles",
            ];
            if let (Some(id), Some(on)) = (ids.get(i), overlays.get(i)) {
                let _ = self.run_command(id, &json!({ "on": !on }));
            }
        }
        if tool != self.state.tool {
            self.state.tool = tool;
            self.chain = None;
        }
        if auto_partner != prefs.auto_partner {
            let _ = self.run_command("dots.option.autoPartner", &json!({ "on": auto_partner }));
        }
        if auto_jump != prefs.auto_jump {
            let _ = self.run_command("dots.option.autoJump", &json!({ "on": auto_jump }));
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
