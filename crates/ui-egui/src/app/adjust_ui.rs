//! The Adjust window: live colour sliders and the image's filter list.
//!
//! Every change runs `image.adjust` (or a filter command) at once, inside one undo group that
//! opens with the window: OK keeps it as one step, Cancel undoes it.

use super::*;
use inbetween_doc::Filter;

/// The filters the "Add filter" menu offers, with the commands that add them.
const FILTERS: &[(&str, &str)] = &[
    ("Smooth...", "image.filter.smooth"),
    ("Blur...", "image.filter.blur"),
    ("Gaussian Blur...", "image.filter.gaussian"),
    ("Zoom Blur...", "image.filter.zoomBlur"),
    ("Radial Blur...", "image.filter.radialBlur"),
    ("Sharpen...", "image.filter.sharpen"),
    ("Emboss...", "image.filter.emboss"),
    ("Mosaic...", "image.filter.mosaic"),
    ("Oil Paint...", "image.filter.oilPaint"),
    ("Splash...", "image.filter.splash"),
    ("Cylinder...", "image.filter.cylinder"),
    ("Whirl...", "image.filter.whirl"),
    ("Pinch...", "image.filter.pinch"),
];

/// The command that edits `filter`, and the form values that reproduce its settings.
pub(super) fn filter_fields(filter: &Filter) -> (&'static str, Vec<(&'static str, String)>) {
    let n = |v: f64| v.to_string();
    match *filter {
        Filter::Smooth { strength } => ("image.filter.smooth", vec![("strength", n(strength))]),
        Filter::Blur { radius } => ("image.filter.blur", vec![("radius", radius.to_string())]),
        Filter::Gaussian { sigma } => ("image.filter.gaussian", vec![("sigma", n(sigma))]),
        Filter::ZoomBlur { cx, cy, amount } => (
            "image.filter.zoomBlur",
            vec![("cx", n(cx)), ("cy", n(cy)), ("amount", n(amount))],
        ),
        Filter::RadialBlur { cx, cy, angle } => (
            "image.filter.radialBlur",
            vec![("cx", n(cx)), ("cy", n(cy)), ("angle", n(angle))],
        ),
        Filter::Sharpen { amount, radius } => (
            "image.filter.sharpen",
            vec![("amount", n(amount)), ("radius", n(radius))],
        ),
        Filter::Emboss { angle, depth, base } => (
            "image.filter.emboss",
            vec![("angle", n(angle)), ("depth", n(depth)), ("base", n(base))],
        ),
        Filter::Mosaic { cell } => ("image.filter.mosaic", vec![("cell", cell.to_string())]),
        Filter::OilPaint { radius, levels } => (
            "image.filter.oilPaint",
            vec![
                ("radius", radius.to_string()),
                ("levels", levels.to_string()),
            ],
        ),
        Filter::Splash { radius, seed } => (
            "image.filter.splash",
            vec![("radius", radius.to_string()), ("seed", seed.to_string())],
        ),
        Filter::Cylinder { vertical, amount } => {
            let mut fields = vec![("amount", n(amount))];
            if vertical {
                fields.push(("vertical", "true".to_string()));
            }
            ("image.filter.cylinder", fields)
        }
        Filter::Whirl {
            cx,
            cy,
            radius,
            angle,
        } => (
            "image.filter.whirl",
            vec![
                ("cx", n(cx)),
                ("cy", n(cy)),
                ("radius", n(radius)),
                ("angle", n(angle)),
            ],
        ),
        Filter::Pinch {
            cx,
            cy,
            radius,
            amount,
        } => (
            "image.filter.pinch",
            vec![
                ("cx", n(cx)),
                ("cy", n(cy)),
                ("radius", n(radius)),
                ("amount", n(amount)),
            ],
        ),
    }
}

/// One line describing a filter in the list.
pub(super) fn filter_label(filter: &Filter) -> String {
    let (id, fields) = filter_fields(filter);
    let name = FILTERS
        .iter()
        .find(|(_, i)| *i == id)
        .map_or(id, |(label, _)| label.trim_end_matches("..."));
    let settings: Vec<String> = fields.iter().map(|(k, v)| format!("{k} {v}")).collect();
    format!("{name}  ({})", settings.join(", "))
}

impl InbetweenApp {
    /// Open the Adjust window on `image` and start the undo group it lives in.
    pub fn open_adjust(&mut self, image: u8) {
        if self.state.adjust_window.is_some() {
            self.close_adjust(true);
        }
        self.state.adjust_window = Some(image);
        self.state.focus_image = image;
        self.adjust_changed = false;
        self.adjust_group_open = self.run_command("edit.beginGroup", &json!({})).is_ok();
    }

    /// Close the window: keep its changes as one undo step, or (`keep` false) undo them.
    pub fn close_adjust(&mut self, keep: bool) {
        if self.adjust_group_open {
            let _ = self.run_command("edit.endGroup", &json!({}));
            self.adjust_group_open = false;
        }
        if !keep && self.adjust_changed {
            let _ = self.run_command("edit.undo", &json!({}));
        }
        self.adjust_changed = false;
        self.state.adjust_window = None;
    }

    /// Open the form that edits filter `index` of `image`, filled with its settings.
    pub fn edit_filter(&mut self, image: u8, index: usize) {
        let Ok(seq) = self.session.sequence(self.state.sequence) else {
            return;
        };
        let Some(filter) = seq.edit(image).filters.get(index).copied() else {
            return;
        };
        let (id, fields) = filter_fields(&filter);
        let Some(spec) = self.registry.get(id).copied() else {
            return;
        };
        let mut form = Form::for_spec(&spec);
        for (name, value) in fields {
            if name == "vertical" {
                if let Some(f) = form.fields.iter_mut().find(|f| f.spec.name == name) {
                    f.flag = true;
                }
            } else {
                form.set(name, value);
            }
        }
        form.set("index", index.to_string());
        form.set("image", image.to_string());
        self.form = Some(form);
    }

    pub(super) fn adjust_window(&mut self, ctx: &Context) {
        let Some(image) = self.state.adjust_window else {
            return;
        };
        let Ok(seq) = self.session.sequence(self.state.sequence) else {
            self.close_adjust(true);
            return;
        };
        let edit = seq.edit(image).clone();
        let mut a = edit.adjust;
        let mut changes: Vec<(&'static str, Value)> = Vec::new();
        let (mut ok, mut cancel, mut reset) = (false, false, false);
        let mut list_action: Option<(usize, &'static str)> = None;
        let mut add: Option<&'static str> = None;
        let mut open = true;
        egui::Window::new("Adjust")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_TOP, [-12.0, 64.0])
            .show(ctx, |ui| {
                ui.label(format!("Image {image}"));
                let mut slider =
                    |ui: &mut egui::Ui,
                     value: &mut f64,
                     key: &'static str,
                     label: &str,
                     range: std::ops::RangeInclusive<f64>| {
                        if ui
                            .add(egui::Slider::new(value, range).text(label))
                            .changed()
                        {
                            changes.push((key, json!(*value)));
                        }
                    };
                slider(
                    ui,
                    &mut a.brightness,
                    "brightness",
                    "Brightness",
                    -100.0..=100.0,
                );
                slider(ui, &mut a.contrast, "contrast", "Contrast", -100.0..=100.0);
                slider(ui, &mut a.red, "red", "Red", -100.0..=100.0);
                slider(ui, &mut a.green, "green", "Green", -100.0..=100.0);
                slider(ui, &mut a.blue, "blue", "Blue", -100.0..=100.0);
                slider(ui, &mut a.hue, "hue", "Hue", -180.0..=180.0);
                slider(
                    ui,
                    &mut a.saturation,
                    "saturation",
                    "Saturation",
                    -100.0..=100.0,
                );
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut a.negative, "Negative").changed() {
                        changes.push(("negative", json!(a.negative)));
                    }
                    if ui.checkbox(&mut a.grayscale, "Grayscale").changed() {
                        changes.push(("grayscale", json!(a.grayscale)));
                    }
                    if ui.button("Reset").clicked() {
                        reset = true;
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.strong("Filters");
                    ui.menu_button("Add filter", |ui| {
                        for (label, id) in FILTERS {
                            if ui.button(*label).clicked() {
                                add = Some(id);
                                ui.close_menu();
                            }
                        }
                    });
                });
                if edit.filters.is_empty() {
                    ui.weak("None. Filters apply in this order, after the colour settings.");
                }
                for (i, f) in edit.filters.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}. {}", i + 1, filter_label(f)));
                        if ui.small_button("Edit").clicked() {
                            list_action = Some((i, "edit"));
                        }
                        if ui.small_button("\u{2191}").clicked() {
                            list_action = Some((i, "up"));
                        }
                        if ui.small_button("\u{2193}").clicked() {
                            list_action = Some((i, "down"));
                        }
                        if ui.small_button("Remove").clicked() {
                            list_action = Some((i, "remove"));
                        }
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ok = ui.button("OK").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        for (key, value) in changes {
            let mut params = json!({ "image": image });
            params[key] = value;
            if self.run_command("image.adjust", &params).is_ok() {
                self.adjust_changed = true;
            }
        }
        if reset
            && self
                .run_command("image.adjust", &json!({ "image": image, "reset": true }))
                .is_ok()
        {
            self.adjust_changed = true;
        }
        if let Some((index, what)) = list_action {
            let params = |extra: Value| {
                let mut p = json!({ "image": image, "index": index });
                if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
                    o.extend(e.clone());
                }
                p
            };
            let result = match what {
                "edit" => {
                    self.edit_filter(image, index);
                    Ok(Value::Null)
                }
                "up" if index > 0 => {
                    self.run_command("image.filter.move", &params(json!({ "to": index - 1 })))
                }
                "down" if index + 1 < edit.filters.len() => {
                    self.run_command("image.filter.move", &params(json!({ "to": index + 1 })))
                }
                "remove" => self.run_command("image.filter.remove", &params(json!({}))),
                _ => Ok(Value::Null),
            };
            if result.is_ok() && what != "edit" {
                self.adjust_changed = true;
            }
        }
        if let Some(id) = add {
            self.invoke(id);
        }
        if cancel {
            self.close_adjust(false);
        } else if ok || !open {
            self.close_adjust(true);
        }
    }
}
