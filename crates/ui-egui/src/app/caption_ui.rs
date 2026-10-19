//! The Caption window and dragging a caption on the canvas.
//!
//! The window edits one caption at a time (Image 1, Image 2, the sequence or the movie). Every
//! change runs the scope's command at once, inside one undo group that opens with the window.
//! While it is open the caption can also be dragged on the panel or preview it lives on; the
//! drag moves its anchor `(x, y)`.

use super::*;
use inbetween_doc::Caption;
use inbetween_render_cpu::caption::{caption_bounds, Font};

fn hex(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
}

impl InbetweenApp {
    /// Open the Caption window on `scope` and start the undo group it lives in.
    pub fn open_caption(&mut self, scope: CaptionScope) {
        if self.state.caption_window.is_some() {
            self.close_caption();
        }
        self.state.caption_window = Some(scope);
        if let Some(image) = scope.image() {
            self.state.focus_image = image;
        }
        self.caption_changed = false;
        self.caption_group_open = self.run_command("edit.beginGroup", &json!({})).is_ok();
    }

    /// Close the window; what was typed stays as one undo step.
    pub fn close_caption(&mut self) {
        if self.caption_group_open {
            let _ = self.run_command("edit.endGroup", &json!({}));
            self.caption_group_open = false;
        }
        self.caption_changed = false;
        self.caption_origin = None;
        self.state.caption_window = None;
    }

    /// The caption at `scope`, if there is one.
    pub fn caption_of(&self, scope: CaptionScope) -> Option<Caption> {
        match scope {
            CaptionScope::Movie => self.session.project().caption.clone(),
            CaptionScope::Sequence => self
                .session
                .sequence(self.state.sequence)
                .ok()?
                .caption
                .clone(),
            CaptionScope::Image1 | CaptionScope::Image2 => self
                .session
                .sequence(self.state.sequence)
                .ok()?
                .edit(scope.image()?)
                .caption
                .clone(),
        }
    }

    /// Run the scope's command with `params` (the image number is added for image scopes).
    pub fn set_caption(&mut self, scope: CaptionScope, mut params: Value) -> bool {
        if let (Some(image), Some(o)) = (scope.image(), params.as_object_mut()) {
            o.insert("image".to_string(), json!(image));
        }
        let ok = self.run_command(scope.command(), &params).is_ok();
        if ok {
            self.caption_changed = true;
        }
        ok
    }

    /// The font a caption draws with: its own file, or the bundled one.
    fn caption_font_for(&mut self, caption: &Caption) -> Option<Font> {
        match self.session.caption_font(caption) {
            Ok(Some((font, _))) => Some(font),
            _ => Font::bundled().ok(),
        }
    }

    /// Where the caption being edited sits on each canvas, so a drag can start on it.
    pub(super) fn caption_snapshot(&mut self, snap: &mut Snapshot) {
        let Some(scope) = self.state.caption_window else {
            return;
        };
        let Some(caption) = self.caption_of(scope).filter(Caption::is_visible) else {
            return;
        };
        let Some(font) = self.caption_font_for(&caption) else {
            return;
        };
        let out = self.session.project().output;
        match scope {
            CaptionScope::Sequence | CaptionScope::Movie => {
                if let Some(b) = caption_bounds(&caption, &font, (out.width, out.height)) {
                    snap.caption[2] = Some(Rect::from_min_max(
                        Pos2::new(b[0] as f32, b[1] as f32),
                        Pos2::new(b[2] as f32, b[3] as f32),
                    ));
                }
            }
            CaptionScope::Image1 | CaptionScope::Image2 => {
                let which = usize::from(scope.image().unwrap_or(1)) - 1;
                let Some(p) = self.prepared() else {
                    return;
                };
                let image = if which == 0 {
                    Some(Arc::clone(&p.image1))
                } else {
                    p.image2.clone()
                };
                let fit = image.as_ref().and_then(|i| {
                    fit_transform(i.width(), i.height(), out.width, out.height, p.sequence.fit).ok()
                });
                let (Some(image), Some(fit)) = (image, fit) else {
                    return;
                };
                let Some(b) = caption_bounds(&caption, &font, image.size()) else {
                    return;
                };
                // Picture pixels back to the frame.
                let Ok(back) = fit.inverse() else {
                    return;
                };
                let (a, c) = (
                    back.apply(inbetween_geom::Point::new(b[0], b[1])),
                    back.apply(inbetween_geom::Point::new(b[2], b[3])),
                );
                snap.caption[which] = Some(Rect::from_two_pos(
                    Pos2::new(a.x as f32, a.y as f32),
                    Pos2::new(c.x as f32, c.y as f32),
                ));
            }
        }
    }

    pub(super) fn caption_begin(&mut self) {
        let Some(scope) = self.state.caption_window else {
            return;
        };
        self.caption_origin = self.caption_of(scope).map(|c| (c.x, c.y));
    }

    /// Move the caption by `(dx, dy)` frame pixels from where the drag began.
    pub(super) fn caption_move(&mut self, dx: f64, dy: f64) {
        let (Some(scope), Some((x0, y0))) = (self.state.caption_window, self.caption_origin) else {
            return;
        };
        let out = self.session.project().output;
        let (mut ndx, mut ndy) = (
            dx / f64::from(out.width.max(1)),
            dy / f64::from(out.height.max(1)),
        );
        if let Some(which) = scope.image() {
            // A picture caption is placed against the picture: convert frame pixels to it.
            let Some(p) = self.prepared() else {
                return;
            };
            let image = if which == 1 {
                Some(Arc::clone(&p.image1))
            } else {
                p.image2.clone()
            };
            let Some(image) = image else {
                return;
            };
            let Ok(fit) = fit_transform(
                image.width(),
                image.height(),
                out.width,
                out.height,
                p.sequence.fit,
            ) else {
                return;
            };
            ndx = fit.a * dx / f64::from(image.width().max(1));
            ndy = fit.d * dy / f64::from(image.height().max(1));
        }
        let _ = self.set_caption(scope, json!({ "x": x0 + ndx, "y": y0 + ndy }));
    }

    pub(super) fn caption_window(&mut self, ctx: &Context) {
        let Some(mut scope) = self.state.caption_window else {
            return;
        };
        let mut shown = self.caption_of(scope).unwrap_or_default();
        let before = shown.clone();
        let mut changes = serde_json::Map::new();
        let (mut clear, mut close) = (false, false);
        let mut pick_font = false;
        let mut default_font = false;
        let mut open = true;
        let font_name = shown.font.as_ref().map_or_else(
            || "Default (Roboto)".to_string(),
            |f| {
                std::path::Path::new(&f.0)
                    .file_name()
                    .map_or_else(|| f.0.clone(), |n| n.to_string_lossy().into_owned())
            },
        );
        egui::Window::new("Caption")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_TOP, [-12.0, 64.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for s in CaptionScope::ALL {
                        ui.selectable_value(&mut scope, s, s.label());
                    }
                });
                ui.add(
                    egui::TextEdit::multiline(&mut shown.text)
                        .desired_rows(3)
                        .desired_width(300.0)
                        .hint_text("Caption text"),
                );
                ui.horizontal(|ui| {
                    ui.label("Font");
                    ui.label(&font_name);
                    if ui.small_button("Browse...").clicked() {
                        pick_font = true;
                    }
                    if ui.small_button("Default").clicked() {
                        default_font = true;
                    }
                });
                egui::Grid::new("caption_grid")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Size");
                        ui.add(
                            egui::DragValue::new(&mut shown.size)
                                .range(1.0..=4000.0)
                                .speed(0.5),
                        );
                        ui.end_row();
                        ui.label("Style");
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut shown.bold, "Bold");
                            ui.checkbox(&mut shown.italic, "Italic");
                        });
                        ui.end_row();
                        ui.label("Colour");
                        ui.color_edit_button_srgba_unmultiplied(&mut shown.color);
                        ui.end_row();
                        ui.label("Border");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut shown.border_size)
                                    .range(0.0..=4000.0)
                                    .speed(0.2),
                            );
                            ui.color_edit_button_srgba_unmultiplied(&mut shown.border_color);
                        });
                        ui.end_row();
                        ui.label("Align");
                        ui.horizontal(|ui| {
                            for a in inbetween_doc::Align::ALL {
                                ui.selectable_value(&mut shown.align, a, a.id());
                            }
                        });
                        ui.end_row();
                        ui.label("Position");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut shown.x).speed(0.005).prefix("x "));
                            ui.add(egui::DragValue::new(&mut shown.y).speed(0.005).prefix("y "));
                        });
                        ui.end_row();
                        ui.label("Shadow");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut shown.shadow.size)
                                    .range(0.0..=4000.0)
                                    .speed(0.2)
                                    .prefix("size "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut shown.shadow.distance)
                                    .speed(0.2)
                                    .prefix("dist "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut shown.shadow.angle)
                                    .speed(1.0)
                                    .suffix("\u{b0}"),
                            );
                            ui.color_edit_button_srgba_unmultiplied(&mut shown.shadow.color);
                        });
                        ui.end_row();
                    });
                ui.checkbox(&mut shown.enabled, "Shown");
                ui.weak(
                    "Drag the text on the image or the preview to move it. Size 0 shadow = none.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Clear").clicked() {
                        clear = true;
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });

        // Turn what changed into command parameters.
        let mut put = |key: &str, value: Value| {
            changes.insert(key.to_string(), value);
        };
        if shown.text != before.text {
            put("text", json!(shown.text));
        }
        if shown.size != before.size {
            put("size", json!(shown.size));
        }
        if shown.bold != before.bold {
            put("bold", json!(shown.bold));
        }
        if shown.italic != before.italic {
            put("italic", json!(shown.italic));
        }
        if shown.color != before.color {
            put("color", json!(hex(shown.color)));
        }
        if shown.border_size != before.border_size {
            put("border_size", json!(shown.border_size));
        }
        if shown.border_color != before.border_color {
            put("border_color", json!(hex(shown.border_color)));
        }
        if shown.align != before.align {
            put("align", json!(shown.align.id()));
        }
        if shown.x != before.x {
            put("x", json!(shown.x));
        }
        if shown.y != before.y {
            put("y", json!(shown.y));
        }
        if shown.shadow.size != before.shadow.size {
            put("shadow_size", json!(shown.shadow.size));
        }
        if shown.shadow.distance != before.shadow.distance {
            put("shadow_distance", json!(shown.shadow.distance));
        }
        if shown.shadow.angle != before.shadow.angle {
            put("shadow_angle", json!(shown.shadow.angle));
        }
        if shown.shadow.color != before.shadow.color {
            put("shadow_color", json!(hex(shown.shadow.color)));
        }
        if shown.enabled != before.enabled {
            put("enabled", json!(shown.enabled));
        }
        if default_font {
            put("font", json!(""));
        }
        if pick_font {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Fonts", &["ttf", "otf"])
                .pick_file()
            {
                put("font", json!(path.display().to_string()));
            }
        }
        if scope != self.state.caption_window.unwrap_or(scope) {
            self.state.caption_window = Some(scope);
            if let Some(image) = scope.image() {
                self.state.focus_image = image;
            }
        }
        if !changes.is_empty() {
            let target = self.state.caption_window.unwrap_or(scope);
            let _ = self.set_caption(target, Value::Object(changes));
        }
        if clear {
            let target = self.state.caption_window.unwrap_or(scope);
            let _ = self.set_caption(target, json!({ "clear": true }));
        }
        if close || !open {
            self.close_caption();
        }
    }
}
