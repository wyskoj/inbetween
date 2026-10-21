//! Dropped files: classifying them, running them and the hover overlay.

use super::*;

impl InbetweenApp {
    /// The image panel under the pointer, if it is over one.
    pub(super) fn panel_at(&self, pos: Option<Pos2>) -> Option<Panel> {
        let pos = pos?;
        [
            (Panel::Source1, self.layout.source1),
            (Panel::Source2, self.layout.source2),
            (Panel::Preview, self.layout.preview),
        ]
        .into_iter()
        .find(|(_, rect)| rect.contains(pos))
        .map(|(panel, _)| panel)
    }

    /// What the pointer is over: an effect slot, an image panel, or neither.
    pub(super) fn zone_at(&self, pos: Option<Pos2>) -> DropZone {
        if let Some(p) = pos {
            if let Some(slot) = self.fx_slots.iter().find(|s| s.rect.contains(p)) {
                return DropZone::EffectSlot {
                    effect: slot.effect,
                    scope: slot.scope,
                };
            }
        }
        self.panel_at(pos).into()
    }

    pub(super) fn image1_is_empty(&self) -> bool {
        self.session
            .sequence(self.state.sequence)
            .map_or(true, |s| s.image1.0.is_empty())
    }

    pub(super) fn to_dropped(files: &[egui::DroppedFile]) -> Vec<Dropped> {
        files
            .iter()
            .map(|f| Dropped {
                path: f.path.as_ref().map(|p| p.display().to_string()),
                name: if f.name.is_empty() {
                    "dropped file".to_string()
                } else {
                    f.name.clone()
                },
            })
            .collect()
    }

    /// Carry out what dropped files mean (see [`dragdrop::route_drop`]).
    pub(super) fn handle_drops(&mut self, ctx: &Context) {
        let files = ctx.input(|i| i.raw.dropped_files.clone());
        if files.is_empty() {
            return;
        }
        if self.new_two.is_some() {
            let paths = files
                .iter()
                .filter_map(|f| f.path.as_ref().map(|p| p.display().to_string()))
                .collect();
            self.drop_into_new_two(paths, ctx.input(|i| i.pointer.latest_pos()));
            return;
        }
        let over = self.zone_at(ctx.input(|i| i.pointer.latest_pos()));
        let actions = dragdrop::route_drop(&Self::to_dropped(&files), over, self.image1_is_empty());
        self.apply_drops(actions);
    }

    /// Run drop actions through the engine; problems reach the status bar.
    pub fn apply_drops(&mut self, actions: Vec<DropAction>) {
        for action in actions {
            match action {
                DropAction::Unsupported(message) => {
                    self.status = Some(Status {
                        text: message,
                        is_error: true,
                    });
                }
                DropAction::OpenProject(path) => self.open_project(&path),
                DropAction::ImportDots(path) => {
                    let _ = self.run_command("dots.import", &json!({ "path": path }));
                }
                DropAction::SetImage { image, path } => {
                    if !self.session.has_sequences()
                        && self.run_command("file.new", &json!({})).is_err()
                    {
                        continue;
                    }
                    let _ = self
                        .run_command("sequence.image", &json!({ "image": image, "path": path }));
                }
                DropAction::SetEffectImage {
                    effect,
                    scope,
                    path,
                } => {
                    let params = json!({
                        "scope": scope.id(),
                        "sequence": self.state.sequence,
                        "image": path,
                    });
                    let _ = self.run_command(effect, &params);
                }
                DropAction::SetSound(path) => {
                    let _ = self.run_command("effect.sound", &json!({ "path": path }));
                }
                DropAction::Wizard(paths) => {
                    if let Some(spec) = self.registry.get("file.wizard").copied() {
                        let mut form = Form::for_spec(&spec);
                        form.set_list("images", paths);
                        self.form = Some(form);
                    }
                }
            }
        }
    }

    /// While files hover over the window, shade where they would land and say what happens.
    pub(super) fn drop_overlay(&self, ctx: &Context) {
        let hovered = ctx.input(|i| i.raw.hovered_files.clone());
        if hovered.is_empty() || self.new_two.is_some() {
            return; // the New From Two Images dialog shows its own drop targets
        }
        let files: Vec<Dropped> = hovered
            .iter()
            .map(|f| Dropped {
                path: f.path.as_ref().map(|p| p.display().to_string()),
                name: "dropped file".to_string(),
            })
            .collect();
        let zone = self.zone_at(ctx.input(|i| i.pointer.latest_pos()));
        let over = match zone {
            DropZone::Panel(p) => Some(p),
            _ => None,
        };
        let slot_rect = self
            .fx_slots
            .iter()
            .find(|s| {
                zone == DropZone::EffectSlot {
                    effect: s.effect,
                    scope: s.scope,
                }
            })
            .map(|s| s.rect);
        let hint = dragdrop::hover_hint(&files, zone, self.image1_is_empty());
        let routed = dragdrop::route_drop(&files, zone, self.image1_is_empty());
        let single_image = matches!(
            routed.first(),
            Some(DropAction::SetImage { .. } | DropAction::SetEffectImage { .. })
        );
        // A single image lands on one panel or slot; anything else acts on the whole window.
        let target = match (single_image, over) {
            (true, _) if slot_rect.is_some() => slot_rect.unwrap_or(self.layout.source1),
            (true, Some(Panel::Source1)) => self.layout.source1,
            (true, Some(Panel::Source2)) => self.layout.source2,
            (true, _) => {
                if self.image1_is_empty() {
                    self.layout.source1
                } else {
                    self.layout.source2
                }
            }
            _ => ctx.screen_rect().shrink(4.0),
        };
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("drop_overlay"),
        ));
        let accent = crate::theme::get(ctx).color.focus;
        painter.rect_filled(target, 6.0, accent.gamma_multiply(0.18));
        painter.rect_stroke(
            target,
            6.0,
            egui::Stroke::new(2.0_f32, accent),
            egui::StrokeKind::Inside,
        );
        let font = egui::FontId::proportional(20.0);
        let text_size = painter
            .layout_no_wrap(hint.clone(), font.clone(), egui::Color32::WHITE)
            .size();
        painter.rect_filled(
            Rect::from_center_size(target.center(), text_size + Vec2::new(24.0, 14.0)),
            6.0,
            egui::Color32::from_black_alpha(190),
        );
        painter.text(
            target.center(),
            egui::Align2::CENTER_CENTER,
            hint,
            font,
            egui::Color32::WHITE,
        );
    }
}
