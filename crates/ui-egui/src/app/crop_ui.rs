//! The crop tool: starting it, the uncropped picture it shows, dragging, applying, cancelling.
//!
//! While the tool is on, the source panel shows the picture without its crop (but with its
//! turns, flips and colour edits) so the box can be dragged past the edge. Nothing reaches the
//! project until Apply runs `image.crop`.

use super::*;
use crate::crop;
use inbetween_doc::ImageEdit;

/// A drag in progress on the crop box.
pub(super) struct CropDrag {
    pub(super) start: CropTool,
    pub(super) handle: Handle,
    pub(super) from: (f64, f64),
}

/// The uncropped picture's texture, kept while the image is unchanged.
pub(super) struct CropTexture {
    key: usize,
    handle: TextureHandle,
    size: (u32, u32),
}

/// The edit with its crop, angle and caption taken off: what the tool lays its box over.
fn uncropped(edit: &ImageEdit) -> ImageEdit {
    ImageEdit {
        crop: None,
        angle: 0.0,
        expand: false,
        caption: None,
        ..edit.clone()
    }
}

/// The aspect choices the crop window offers.
const ASPECTS: &[&str] = &[
    "free", "original", "output", "1:1", "4:3", "3:2", "16:9", "9:16",
];

impl InbetweenApp {
    /// Start cropping `image` of the current sequence, from its current crop (or the whole
    /// picture).
    pub fn start_crop(&mut self, image: u8) {
        let seq = self.state.sequence;
        let Ok(sequence) = self.session.sequence(seq) else {
            return;
        };
        let edit = sequence.edit(image).clone();
        match self.session.source_with(seq, image, &uncropped(&edit)) {
            Ok(img) => {
                let turned = (f64::from(img.width()), f64::from(img.height()));
                self.state.crop = Some(CropTool::from_edit(image, &edit, turned));
                self.state.focus_image = image;
                self.crop_drag = None;
                self.grabs.crop = None;
            }
            Err(e) => {
                self.announce(e.to_string(), ToastKind::Error, false);
            }
        }
    }

    /// The crop being drawn, if the tool is on.
    pub fn crop_tool(&self) -> Option<&CropTool> {
        self.state.crop.as_ref()
    }

    /// Replace the crop being drawn (the numeric fields and tests use this).
    pub fn set_crop(&mut self, tool: CropTool) {
        if self.state.crop.is_some() && tool.is_sane() {
            self.state.crop = Some(tool);
        }
    }

    /// Run `image.crop` with the box and leave the tool.
    pub fn apply_crop(&mut self) {
        let Some(tool) = self.state.crop.clone() else {
            return;
        };
        let mut params = tool.params();
        params["image"] = json!(tool.image);
        if self.run_command("image.crop", &params).is_ok() {
            self.state.crop = None;
            self.crop_drag = None;
            self.grabs.crop = None;
            self.state.refit_all();
        }
    }

    /// Leave the tool; the edit stays as it was.
    pub fn cancel_crop(&mut self) {
        self.state.crop = None;
        self.crop_drag = None;
        self.grabs.crop = None;
    }

    /// Size of the picture the box is drawn over, in pixels.
    fn crop_turned(&self) -> Option<(f64, f64)> {
        self.crop_texture
            .as_ref()
            .map(|t| (f64::from(t.size.0), f64::from(t.size.1)))
    }

    fn crop_ratio(&self, tool: &CropTool) -> Option<f64> {
        let turned = self.crop_turned()?;
        let out = self.session.project().output;
        crop::ratio_of(
            &tool.aspect,
            turned,
            (f64::from(out.width), f64::from(out.height)),
        )
    }

    pub(super) fn crop_begin(&mut self, handle: Handle, at: (f64, f64)) {
        if let Some(start) = self.state.crop.clone() {
            self.crop_drag = Some(CropDrag {
                start,
                handle,
                from: at,
            });
        }
    }

    pub(super) fn crop_move(&mut self, to: (f64, f64)) {
        let Some(d) = &self.crop_drag else {
            return;
        };
        let ratio = self.crop_ratio(&d.start);
        let next = crop::drag(&d.start, d.handle, d.from, to, ratio);
        if next.is_sane() {
            self.state.crop = Some(next);
        }
    }

    /// Show the uncropped picture, the box and its handles on the panel that has the tool.
    pub(super) fn crop_snapshot(&mut self, ctx: &Context, i: usize, snap: &mut Snapshot) {
        let Some(tool) = self
            .state
            .crop
            .clone()
            .filter(|t| usize::from(t.image) == i + 1)
        else {
            return;
        };
        let seq = self.state.sequence;
        let Ok(sequence) = self.session.sequence(seq) else {
            return;
        };
        let (edit, fit_mode) = (sequence.edit(tool.image).clone(), sequence.fit);
        let Ok(image) = self.session.source_with(seq, tool.image, &uncropped(&edit)) else {
            return;
        };
        let key = Arc::as_ptr(&image) as usize;
        if self.crop_texture.as_ref().is_none_or(|t| t.key != key) {
            let color = to_color_image(&image, ctx.input(|i| i.max_texture_side));
            let handle = ctx.load_texture("crop source", color, TextureOptions::LINEAR);
            self.crop_texture = Some(CropTexture {
                key,
                handle,
                size: image.size(),
            });
        }
        let Some(texture) = self.crop_texture.as_ref() else {
            return;
        };
        let out = self.session.project().output;
        let Ok(fit) = fit_transform(
            image.width(),
            image.height(),
            out.width,
            out.height,
            fit_mode,
        ) else {
            return;
        };
        let size = Vec2::new(image.width() as f32, image.height() as f32);
        let (frame_rect, uv) = content_mapping(&fit, size, snap.frame);
        snap.shown[i] = Some(Shown {
            id: texture.handle.id(),
            frame_rect,
            uv,
        });
        snap.dots[i].clear();
        snap.mesh[i].clear();
        snap.titles[i] = crate::panels::Title {
            name: "Cropping".into(),
            meta: format!(
                "{} \u{d7} {} \u{b7} drag the box, Enter applies, Esc cancels",
                image.width(),
                image.height()
            ),
            ..crate::panels::Title::new(if tool.image == 1 { "Start" } else { "End" })
        };
        snap.crop[i] = Some(CropView { tool, fit });
    }

    /// The Crop window: aspect, numbers, Apply and Cancel.
    pub(super) fn crop_window(&mut self, ctx: &Context) {
        let Some(mut tool) = self.state.crop.clone() else {
            return;
        };
        let before = tool.clone();
        let (mut apply, mut cancel, mut whole) = (false, false, false);
        let turned = self.crop_turned();
        egui::Window::new("Crop")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_TOP, [-12.0, 64.0])
            .show(ctx, |ui| {
                ui.label(format!("Image {}", tool.image));
                egui::Grid::new("crop_numbers")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Shape");
                        egui::ComboBox::from_id_salt("crop_aspect")
                            .selected_text(tool.aspect.clone())
                            .show_ui(ui, |ui| {
                                for a in ASPECTS {
                                    ui.selectable_value(&mut tool.aspect, (*a).to_string(), *a);
                                }
                            });
                        ui.end_row();
                        for (label, value) in [
                            ("x", &mut tool.x),
                            ("y", &mut tool.y),
                            ("w", &mut tool.w),
                            ("h", &mut tool.h),
                        ] {
                            ui.label(label);
                            ui.add(egui::DragValue::new(value).speed(1.0));
                            ui.end_row();
                        }
                        ui.label("angle");
                        ui.add(
                            egui::DragValue::new(&mut tool.angle)
                                .speed(0.5)
                                .range(-360.0..=360.0)
                                .suffix("\u{b0}"),
                        );
                        ui.end_row();
                    });
                ui.weak("Drag the box and its handles on the image. The round handle tilts it.");
                ui.horizontal(|ui| {
                    apply = ui.button("Apply").clicked();
                    cancel = ui.button("Cancel").clicked();
                    whole = ui.button("Whole picture").clicked();
                });
            });
        if whole {
            if let Some((w, h)) = turned {
                tool.x = 0.0;
                tool.y = 0.0;
                tool.w = w;
                tool.h = h;
                tool.angle = 0.0;
            }
        }
        if tool != before {
            // A changed shape setting snaps the height; a changed width follows the lock.
            let out = self.session.project().output;
            let ratio = turned.and_then(|t| {
                crop::ratio_of(
                    &tool.aspect,
                    t,
                    (f64::from(out.width), f64::from(out.height)),
                )
            });
            if let Some(r) = ratio {
                if tool.aspect != before.aspect || tool.w != before.w || tool.h == before.h {
                    tool.snap_aspect(r);
                } else if tool.h != before.h {
                    tool.w = (tool.h * r).max(1.0);
                }
            }
            self.set_crop(tool);
        }
        if apply {
            self.apply_crop();
        } else if cancel {
            self.cancel_crop();
        }
    }
}
