//! The Effects window: one row per effect with an on/off box, a thumbnail slot (click to
//! browse, or drop a file on it), Clear, and the effect's own controls.
//!
//! A Sequence/Movie switch picks whose effects the rows edit. Every control runs the matching
//! `effect.*` command at once, inside one undo group that opens with the window, so a whole
//! session of slider moves is one undo step.

use super::*;
use crate::state::EffectScope;
use inbetween_doc::Effects;

/// A thumbnail slot as drawn last frame, for dropping files onto.
#[derive(Clone, Copy, Debug)]
pub(super) struct FxSlot {
    pub effect: &'static str,
    pub scope: EffectScope,
    pub rect: Rect,
}

/// Size of a thumbnail slot in points.
const SLOT: f32 = 64.0;

fn hex(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
}

/// One edit the window decided on this frame: the command and its settings.
type Change = (&'static str, Value);

impl InbetweenApp {
    /// Open the Effects window on `scope` and start the undo group it lives in.
    pub fn open_effects(&mut self, scope: EffectScope) {
        if self.state.effects_window.is_some() {
            self.close_effects();
        }
        self.state.effects_window = Some(scope);
        self.effects_group_open = self.run_command("edit.beginGroup", &json!({})).is_ok();
    }

    /// Close the window; what was changed stays as one undo step.
    pub fn close_effects(&mut self) {
        if self.effects_group_open {
            let _ = self.run_command("edit.endGroup", &json!({}));
            self.effects_group_open = false;
        }
        self.state.effects_window = None;
        self.fx_slots.clear();
    }

    /// The thumbnail of an effect image, loaded through the session's cache.
    fn effect_thumbnail(&mut self, ctx: &Context, path: &str) -> Option<TextureHandle> {
        if path.is_empty() {
            return None;
        }
        let image = self.session.load_effect_path(path).ok()?;
        let key = format!("fx:{path}");
        if let Some((old, handle)) = self.thumbs.get(&key) {
            if Arc::ptr_eq(old, &image) {
                return Some(handle.clone());
            }
        }
        let handle = ctx.load_texture(
            format!("effect {path}"),
            to_color_image(&image, 64),
            TextureOptions::LINEAR,
        );
        self.thumbs.insert(key, (image, handle.clone()));
        Some(handle)
    }

    /// Draw a slot. Returns true when it was clicked. The slot is remembered as a drop target.
    fn effect_slot(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        effect: &'static str,
        scope: EffectScope,
        shown: &str,
        weak_hint: &str,
    ) -> bool {
        let texture = self.effect_thumbnail(ctx, shown);
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(SLOT), egui::Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
        match texture {
            Some(t) => {
                painter.image(
                    t.id(),
                    rect.shrink(2.0),
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            None => {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    weak_hint,
                    egui::FontId::proportional(11.0),
                    ui.visuals().weak_text_color(),
                );
            }
        }
        painter.rect_stroke(
            rect,
            4.0,
            ui.visuals().widgets.inactive.bg_stroke,
            egui::StrokeKind::Inside,
        );
        self.fx_slots_next.push(FxSlot {
            effect,
            scope,
            rect,
        });
        response
            .on_hover_text("Click to choose a file, or drop one here")
            .clicked()
    }

    pub(super) fn effects_window(&mut self, ctx: &Context) {
        let Some(mut scope) = self.state.effects_window else {
            self.fx_slots.clear();
            return;
        };
        let sequence = self.state.sequence;
        let fx: Effects = match scope {
            EffectScope::Movie => self.session.project().effects.clone(),
            EffectScope::Sequence => match self.session.sequence(sequence) {
                Ok(s) => s.effects.clone(),
                Err(_) => {
                    self.close_effects();
                    return;
                }
            },
        };
        let sound = self.session.project().sound.clone();
        let mut changes: Vec<Change> = Vec::new();
        let mut browse: Option<(&'static str, &'static str)> = None;
        let mut open = true;
        let mut done = false;
        self.fx_slots_next.clear();
        egui::Window::new("Effects")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_TOP, [-12.0, 64.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut scope, EffectScope::Sequence, "Sequence");
                    ui.selectable_value(&mut scope, EffectScope::Movie, "Movie");
                });
                ui.weak(match scope {
                    EffectScope::Sequence => {
                        "Effects of the selected sequence; the camera moves them."
                    }
                    EffectScope::Movie => "Effects over every finished frame of the movie.",
                });
                ui.separator();
                let slider = |ui: &mut egui::Ui,
                              cmd: &'static str,
                              key: &'static str,
                              value: &mut f64,
                              label: &str,
                              range: std::ops::RangeInclusive<f64>,
                              changes: &mut Vec<Change>| {
                    if ui
                        .add(egui::Slider::new(value, range).text(label))
                        .changed()
                    {
                        changes.push((cmd, json!({ key: *value })));
                    }
                };

                // Back colour.
                ui.horizontal(|ui| {
                    let mut on = fx.back_color.enabled;
                    if ui.checkbox(&mut on, "Back colour").changed() {
                        changes.push(("effect.backColor", json!({ "enabled": on })));
                    }
                    let mut rgba = fx.back_color.rgba;
                    if ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed() {
                        changes.push(("effect.backColor", json!({ "color": hex(rgba) })));
                    }
                    if ui.small_button("Clear").clicked() {
                        changes.push(("effect.backColor", json!({ "clear": true })));
                    }
                });

                // Effects that hold an image.
                let rows: [(&'static str, &str, bool, &str, bool); 5] = [
                    (
                        "effect.background",
                        "Background",
                        fx.background.enabled,
                        &fx.background.image.0,
                        true,
                    ),
                    (
                        "effect.mask",
                        "Mask",
                        fx.mask.enabled,
                        &fx.mask.image.0,
                        true,
                    ),
                    (
                        "effect.foreground",
                        "Foreground",
                        fx.foreground.enabled,
                        &fx.foreground.image.0,
                        true,
                    ),
                    (
                        "effect.transition",
                        "Transition",
                        fx.transition.enabled,
                        &fx.transition.image.0,
                        scope == EffectScope::Sequence,
                    ),
                    (
                        "effect.light",
                        "Light",
                        fx.light.enabled,
                        &fx.light.image.0,
                        true,
                    ),
                ];
                for (cmd, label, enabled, image, shown) in rows {
                    if !shown {
                        continue;
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        let mut on = enabled;
                        let checkbox =
                            ui.add_enabled(!image.is_empty(), egui::Checkbox::new(&mut on, label));
                        if checkbox.changed() {
                            changes.push((cmd, json!({ "enabled": on })));
                        }
                        if ui.small_button("Clear").clicked() {
                            changes.push((cmd, json!({ "clear": true })));
                        }
                    });
                    ui.horizontal(|ui| {
                        if self.effect_slot(ui, ctx, cmd, scope, image, "drop an\nimage") {
                            browse = Some((cmd, "image"));
                        }
                        ui.vertical(|ui| match cmd {
                            "effect.background" => {
                                let mut v = fx.background.opacity;
                                slider(
                                    ui,
                                    cmd,
                                    "opacity",
                                    &mut v,
                                    "Opacity",
                                    0.0..=100.0,
                                    &mut changes,
                                );
                            }
                            "effect.foreground" => {
                                let mut v = fx.foreground.opacity;
                                slider(
                                    ui,
                                    cmd,
                                    "opacity",
                                    &mut v,
                                    "Opacity",
                                    0.0..=100.0,
                                    &mut changes,
                                );
                            }
                            "effect.mask" => {
                                let mut v = fx.mask.intensity;
                                slider(
                                    ui,
                                    cmd,
                                    "intensity",
                                    &mut v,
                                    "Intensity",
                                    -100.0..=100.0,
                                    &mut changes,
                                );
                                let mut inv = fx.mask.invert;
                                if ui.checkbox(&mut inv, "Invert").changed() {
                                    changes.push((cmd, json!({ "invert": inv })));
                                }
                            }
                            "effect.transition" => {
                                let mut v = fx.transition.softness;
                                slider(
                                    ui,
                                    cmd,
                                    "softness",
                                    &mut v,
                                    "Softness",
                                    0.0..=100.0,
                                    &mut changes,
                                );
                                let mut inv = fx.transition.invert;
                                if ui.checkbox(&mut inv, "Light pixels first").changed() {
                                    changes.push((cmd, json!({ "invert": inv })));
                                }
                            }
                            _ => {
                                let mut b = fx.light.bright;
                                slider(
                                    ui,
                                    cmd,
                                    "bright",
                                    &mut b,
                                    "Bright",
                                    0.0..=100.0,
                                    &mut changes,
                                );
                                let mut d = fx.light.dark;
                                slider(ui, cmd, "dark", &mut d, "Dark", 0.0..=100.0, &mut changes);
                            }
                        });
                    });
                }

                // Trail and rays: no image.
                for (cmd, label, enabled, overlay, rgba, intensity, over_text) in [
                    (
                        "effect.trail",
                        "Trail",
                        fx.trail.enabled,
                        fx.trail.overlay,
                        fx.trail.rgba,
                        fx.trail.intensity,
                        "Blur (off: tinted trail)",
                    ),
                    (
                        "effect.radial",
                        "Radial rays",
                        fx.radial.enabled,
                        fx.radial.overlay,
                        fx.radial.rgba,
                        fx.radial.intensity,
                        "Over the morph",
                    ),
                ] {
                    ui.separator();
                    ui.horizontal(|ui| {
                        let mut on = enabled;
                        if ui.checkbox(&mut on, label).changed() {
                            changes.push((cmd, json!({ "enabled": on })));
                        }
                        let mut c = rgba;
                        if ui.color_edit_button_srgba_unmultiplied(&mut c).changed() {
                            changes.push((cmd, json!({ "color": hex(c) })));
                        }
                        if ui.small_button("Clear").clicked() {
                            changes.push((cmd, json!({ "clear": true })));
                        }
                    });
                    let mut o = overlay;
                    if ui.checkbox(&mut o, over_text).changed() {
                        changes.push((cmd, json!({ "overlay": o })));
                    }
                    let mut v = intensity;
                    slider(
                        ui,
                        cmd,
                        "intensity",
                        &mut v,
                        "Intensity",
                        0.0..=100.0,
                        &mut changes,
                    );
                }

                if scope == EffectScope::Movie {
                    ui.separator();
                    let shown = sound.as_ref().map_or("", |s| s.path.0.as_str());
                    ui.horizontal(|ui| {
                        let mut on = sound.as_ref().is_some_and(|s| s.enabled);
                        let has = sound.is_some();
                        if ui
                            .add_enabled(has, egui::Checkbox::new(&mut on, "Sound"))
                            .changed()
                        {
                            changes.push(("effect.sound", json!({ "enabled": on })));
                        }
                        if ui.small_button("Clear").clicked() {
                            changes.push(("effect.sound", json!({ "clear": true })));
                        }
                    });
                    ui.horizontal(|ui| {
                        let name = std::path::Path::new(shown)
                            .file_name()
                            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                        let hint = if name.is_empty() {
                            "drop a\nWAV or MP3".to_string()
                        } else {
                            name
                        };
                        if self.effect_slot(ui, ctx, "effect.sound", scope, "", &hint) {
                            browse = Some(("effect.sound", "path"));
                        }
                        if let Some(s) = &sound {
                            ui.vertical(|ui| {
                                let mut looped = s.looped;
                                if ui.checkbox(&mut looped, "Loop").changed() {
                                    changes.push(("effect.sound", json!({ "loop": looped })));
                                }
                                let mut cut = s.cut_at_end;
                                if ui
                                    .checkbox(&mut cut, "Cut at the end of the movie")
                                    .changed()
                                {
                                    changes.push(("effect.sound", json!({ "cut": cut })));
                                }
                                let mut fi = s.fade_in;
                                slider(
                                    ui,
                                    "effect.sound",
                                    "fade_in",
                                    &mut fi,
                                    "Fade in (s)",
                                    0.0..=60.0,
                                    &mut changes,
                                );
                                let mut fo = s.fade_out;
                                slider(
                                    ui,
                                    "effect.sound",
                                    "fade_out",
                                    &mut fo,
                                    "Fade out (s)",
                                    0.0..=60.0,
                                    &mut changes,
                                );
                            });
                        }
                    });
                }
                ui.separator();
                if ui.button("Close").clicked() {
                    done = true;
                }
            });
        self.fx_slots = std::mem::take(&mut self.fx_slots_next);
        self.state.effects_window = Some(scope);
        for (cmd, mut params) in changes {
            if cmd != "effect.sound" {
                params["scope"] = json!(scope.id());
                params["sequence"] = json!(sequence);
            }
            let _ = self.run_command(cmd, &params);
        }
        if let Some((cmd, key)) = browse {
            let mut dialog = rfd::FileDialog::new();
            dialog = if cmd == "effect.sound" {
                dialog.add_filter("Sound", &["wav", "mp3"])
            } else {
                dialog.add_filter("Image", inbetween_codecs::IMAGE_EXTENSIONS)
            };
            if let Some(path) = dialog.pick_file() {
                let mut params = json!({ key: path.display().to_string() });
                if cmd != "effect.sound" {
                    params["scope"] = json!(scope.id());
                    params["sequence"] = json!(sequence);
                }
                let _ = self.run_command(cmd, &params);
            }
        }
        if done || !open {
            self.close_effects();
        }
    }
}
