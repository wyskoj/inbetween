//! The Camera window and the camera handles on the preview.
//!
//! The window edits one camera at a time: the sequence's, or one still-image effect's. While it
//! is open the preview shows the moved picture's outline with handles; dragging them runs
//! `camera.keyframes` (or `effect.camera`) with `op=set` at the playhead's time, so a drag adds
//! or updates a keyframe there. Everything the window does is one undo group that opens with
//! it, and a drag is a series of commands inside that group.

use super::*;
use crate::camera_tool::{self, Grab};
use crate::state::CameraTarget;
use inbetween_doc::{Camera, CameraKey, CAMERA_EFFECTS};

/// A camera handle being dragged.
#[derive(Clone, Copy, Debug)]
pub(super) struct CameraDrag {
    grab: Grab,
    start: CameraKey,
    at: (f64, f64),
    t: f64,
    locked: bool,
}

const ASPECT_LABELS: [(&str, &str); 4] = [
    ("free", "Free"),
    ("locked", "Movie"),
    ("image1", "Image 1"),
    ("image2", "Image 2"),
];

impl InbetweenApp {
    /// Open the Camera window on `target` and start the undo group it lives in.
    pub fn open_camera(&mut self, target: CameraTarget) {
        if self.state.camera_window.is_some() {
            self.close_camera();
        }
        self.state.camera_window = Some(target);
        self.camera_group_open = self.run_command("edit.beginGroup", &json!({})).is_ok();
    }

    /// Close the window; what was done stays as one undo step.
    pub fn close_camera(&mut self) {
        if self.camera_group_open {
            let _ = self.run_command("edit.endGroup", &json!({}));
            self.camera_group_open = false;
        }
        self.camera_drag = None;
        self.state.camera_window = None;
    }

    fn camera_command(target: &CameraTarget) -> &'static str {
        if target.effect.is_some() {
            "effect.camera"
        } else {
            "camera.keyframes"
        }
    }

    /// The camera the window is editing.
    pub fn camera_of(&self, target: &CameraTarget) -> Option<Camera> {
        let project = self.session.project();
        match (&target.effect, target.movie) {
            (None, _) => project
                .sequences
                .get(self.state.sequence)
                .map(|s| s.camera.clone()),
            (Some(name), true) => project.effects.cameras.get(name).cloned(),
            (Some(name), false) => project
                .sequences
                .get(self.state.sequence)
                .and_then(|s| s.effects.cameras.get(name).cloned()),
        }
    }

    /// The camera's clock at the playhead: the sequence's morph time, or the movie's progress
    /// for a movie effect.
    fn camera_time(&self, target: &CameraTarget) -> f64 {
        if target.effect.is_some() && target.movie {
            let total = self.transport_frames();
            if total <= 1 {
                return 0.0;
            }
            return f64::from(self.transport_pos()) / f64::from(total - 1);
        }
        self.prepared()
            .map_or(0.0, |p| p.sequence.frame_time(self.state.frame))
    }

    /// Run the window's command with `extra` and the camera's address filled in.
    fn run_camera(&mut self, target: &CameraTarget, mut params: Value) -> bool {
        if let Some(o) = params.as_object_mut() {
            if let Some(name) = &target.effect {
                o.insert("effect".into(), json!(name));
                o.insert(
                    "scope".into(),
                    json!(if target.movie { "movie" } else { "sequence" }),
                );
            }
        }
        self.run_command(Self::camera_command(target), &params)
            .is_ok()
    }

    /// The outline and handles for the preview.
    pub(super) fn camera_snapshot(&mut self, snap: &mut Snapshot) {
        let Some(target) = self.state.camera_window.clone() else {
            return;
        };
        let Some(camera) = self.camera_of(&target) else {
            return;
        };
        let out = self.session.project().output;
        let t = self.camera_time(&target);
        let pose = if camera.enabled {
            camera.pose(t)
        } else {
            CameraKey::identity(t)
        };
        snap.camera = Some(panels::CameraOverlay {
            pose,
            frame: (f64::from(out.width), f64::from(out.height)),
        });
    }

    pub(super) fn camera_begin(&mut self, grab: Grab, at: (f64, f64)) {
        let Some(target) = self.state.camera_window.clone() else {
            return;
        };
        let Some(camera) = self.camera_of(&target) else {
            return;
        };
        let t = self.camera_time(&target);
        let start = if camera.enabled {
            camera.pose(t)
        } else {
            CameraKey::identity(t)
        };
        self.camera_drag = Some(CameraDrag {
            grab,
            start,
            at,
            t,
            locked: camera.aspect != inbetween_doc::CameraAspect::Free,
        });
    }

    pub(super) fn camera_move(&mut self, to: (f64, f64)) {
        let (Some(drag), Some(target)) = (self.camera_drag, self.state.camera_window.clone())
        else {
            return;
        };
        let out = self.session.project().output;
        let frame = (f64::from(out.width), f64::from(out.height));
        let pose = camera_tool::drag(drag.grab, &drag.start, frame, drag.at, to);
        let mut params = json!({ "op": "set", "t": drag.t });
        let Some(o) = params.as_object_mut() else {
            return;
        };
        match drag.grab {
            Grab::Move => {
                o.insert("x".into(), json!(pose.x));
                o.insert("y".into(), json!(pose.y));
            }
            Grab::Rotate => {
                o.insert("rotation".into(), json!(pose.rotation));
            }
            Grab::Scale(hx, hy) => {
                if hx != 0 {
                    o.insert("scale_x".into(), json!(pose.scale_x));
                }
                // With the proportions locked the engine derives the other scale.
                if hy != 0 && !(drag.locked && hx != 0) {
                    o.insert("scale_y".into(), json!(pose.scale_y));
                }
                if drag.locked && hx == 0 {
                    o.remove("scale_x");
                }
            }
        }
        let _ = self.run_camera(&target, params);
    }

    pub(super) fn camera_window(&mut self, ctx: &Context) {
        let Some(mut target) = self.state.camera_window.clone() else {
            return;
        };
        let Some(camera) = self.camera_of(&target) else {
            self.close_camera();
            return;
        };
        let t = self.camera_time(&target);
        let shown = if camera.enabled {
            camera.pose(t)
        } else {
            CameraKey::identity(t)
        };
        let mut pose = shown;
        let mut enabled = camera.enabled;
        let (mut remove, mut add_key) = (None, false);
        let mut op: Option<&'static str> = None;
        let mut aspect: Option<&'static str> = None;
        let mut close = false;
        let mut open = true;
        let mut file: Option<(&'static str, std::path::PathBuf)> = None;
        egui::Window::new("Camera")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_TOP, [-12.0, 64.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Moves");
                    let label = target
                        .effect
                        .as_deref()
                        .map_or_else(|| "The whole sequence".to_string(), |n| format!("{n} effect"));
                    egui::ComboBox::from_id_salt("camera_subject")
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut target.effect, None, "The whole sequence");
                            for name in CAMERA_EFFECTS {
                                ui.selectable_value(
                                    &mut target.effect,
                                    Some(name.to_string()),
                                    format!("{name} effect"),
                                );
                            }
                        });
                    if target.effect.is_some() {
                        ui.checkbox(&mut target.movie, "Whole movie");
                    }
                });
                ui.checkbox(&mut enabled, "Camera on");
                ui.horizontal(|ui| {
                    ui.label("Frame shape");
                    for (id, label) in ASPECT_LABELS {
                        let on = camera.aspect.id() == id;
                        if ui.selectable_label(on, label).clicked() && !on {
                            aspect = Some(id);
                        }
                    }
                });
                egui::Grid::new("camera_grid")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(format!("At t = {t:.2}"));
                        ui.label("");
                        ui.end_row();
                        ui.label("Offset");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut pose.x).speed(0.005).prefix("x "));
                            ui.add(egui::DragValue::new(&mut pose.y).speed(0.005).prefix("y "));
                        });
                        ui.end_row();
                        ui.label("Scale");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut pose.scale_x)
                                    .speed(0.01)
                                    .prefix("w "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut pose.scale_y)
                                    .speed(0.01)
                                    .prefix("h "),
                            );
                        });
                        ui.end_row();
                        ui.label("Rotation");
                        ui.add(
                            egui::DragValue::new(&mut pose.rotation)
                                .speed(0.5)
                                .suffix("\u{b0}"),
                        );
                        ui.end_row();
                    });
                ui.separator();
                ui.label("Keyframes");
                if camera.keys.is_empty() {
                    ui.weak("None yet: drag the outline on the preview, or add one here.");
                }
                for (i, k) in camera.keys.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "t {:.2}: ({:.2}, {:.2}) \u{d7} ({:.2}, {:.2}) {:.0}\u{b0}",
                            k.t, k.x, k.y, k.scale_x, k.scale_y, k.rotation
                        ));
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Add keyframe here").clicked() {
                        add_key = true;
                    }
                    for (label, id) in [
                        ("Reset", "reset"),
                        ("Copy", "copy"),
                        ("Paste", "paste"),
                        ("Apply to all", "apply_all"),
                    ] {
                        if ui.button(label).clicked() {
                            op = Some(id);
                        }
                    }
                    if ui.button("Save...").clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter("Camera", &["inbetweencamera"])
                            .save_file()
                        {
                            file = Some(("save", p));
                        }
                    }
                    if ui.button("Load...").clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter("Camera", &["inbetweencamera"])
                            .pick_file()
                        {
                            file = Some(("load", p));
                        }
                    }
                });
                ui.weak("Drag the outline on the preview to move it, the squares to scale (cross the far side to flip), the circle to turn.");
                if ui.button("Close").clicked() {
                    close = true;
                }
            });

        // The picker may have changed what the window edits: run the rest against the old
        // camera first, then switch.
        let current = self.state.camera_window.clone().unwrap_or_default();
        if let Some(id) = aspect {
            let _ = self.run_camera(&current, json!({ "aspect": id }));
        }
        let mut params = serde_json::Map::new();
        if pose.x != shown.x {
            params.insert("x".into(), json!(pose.x));
        }
        if pose.y != shown.y {
            params.insert("y".into(), json!(pose.y));
        }
        if pose.scale_x != shown.scale_x {
            params.insert("scale_x".into(), json!(pose.scale_x));
        }
        if pose.scale_y != shown.scale_y {
            params.insert("scale_y".into(), json!(pose.scale_y));
        }
        if pose.rotation != shown.rotation {
            params.insert("rotation".into(), json!(pose.rotation));
        }
        if !params.is_empty() || add_key {
            params.insert("op".into(), json!("set"));
            params.insert("t".into(), json!(t));
            let _ = self.run_camera(&current, Value::Object(params));
        }
        if let Some(i) = remove {
            let _ = self.run_camera(&current, json!({ "op": "remove", "index": i }));
        }
        if let Some(op) = op {
            let _ = self.run_camera(&current, json!({ "op": op }));
        }
        if let Some((op, path)) = file {
            let _ = self.run_camera(
                &current,
                json!({ "op": op, "path": path.display().to_string() }),
            );
        }
        if enabled != camera.enabled {
            let _ = self.run_camera(&current, json!({ "enabled": enabled }));
        }
        if target != current {
            self.camera_drag = None;
            self.state.camera_window = Some(target);
        }
        if close || !open {
            self.close_camera();
        }
    }
}
