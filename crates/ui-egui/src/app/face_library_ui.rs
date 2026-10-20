//! The Face Library window: a browser for a folder of images with their dots. Tiles can be
//! dragged onto Image 1 or Image 2 to reuse them in the project (`face.library use`), or used
//! with the buttons; the current image can be saved into the library.

use super::*;
use egui::{Color32, TextureHandle};
use std::collections::HashMap;

/// What a dragged library tile carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryDrag {
    pub folder: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LibEntry {
    pub folder: String,
    pub name: String,
    pub dots: usize,
}

#[derive(Default)]
pub(super) struct LibraryUi {
    pub entries: Vec<LibEntry>,
    pub thumbs: HashMap<(String, String), Option<TextureHandle>>,
    pub selected: Option<usize>,
    pub message: String,
    pub save_name: String,
    /// The folder the entries were listed from, to know when to list again.
    pub listed: String,
}

/// Most thumbnails decoded in one frame, so a big library does not freeze the window.
const THUMBS_PER_FRAME: usize = 6;

/// Which source panel a screen point is over (1 or 2).
pub(crate) fn panel_at(source1: Rect, source2: Rect, at: Pos2) -> Option<u8> {
    if source1.contains(at) {
        Some(1)
    } else if source2.contains(at) {
        Some(2)
    } else {
        None
    }
}

impl InbetweenApp {
    /// The `(folder, name)` of every entry the window lists.
    pub fn library_entries(&self) -> Vec<(String, String)> {
        self.face
            .library
            .entries
            .iter()
            .map(|e| (e.folder.clone(), e.name.clone()))
            .collect()
    }

    pub fn open_face_library(&mut self) {
        self.state.face_library_window = true;
        self.face.library.listed.clear();
    }

    pub fn close_face_library(&mut self) {
        self.state.face_library_window = false;
    }

    fn library_params(&self, extra: Value) -> Value {
        let mut p = json!({ "dir": self.state.library_dir });
        if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        p
    }

    /// List the library folder again.
    pub fn refresh_library(&mut self) {
        let lib = &mut self.face.library;
        lib.listed = self.state.library_dir.clone();
        lib.selected = None;
        lib.thumbs.clear();
        if self.state.library_dir.is_empty() {
            lib.entries.clear();
            lib.message = "Choose a library folder.".to_string();
            return;
        }
        let params = self.library_params(json!({ "op": "list" }));
        match self.run_command("face.library", &params) {
            Ok(r) => {
                let entries: Vec<LibEntry> = r["entries"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|e| LibEntry {
                                folder: e["folder"].as_str().unwrap_or_default().to_string(),
                                name: e["name"].as_str().unwrap_or_default().to_string(),
                                dots: e["dots"].as_u64().unwrap_or(0) as usize,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                self.face.library.message = format!("{} image(s)", entries.len());
                self.face.library.entries = entries;
            }
            Err(e) => {
                self.face.library.entries.clear();
                self.face.library.message = e.to_string();
            }
        }
    }

    /// Put a library entry's image and dots into the current sequence as `image`.
    pub fn use_library_entry(&mut self, drag: &LibraryDrag, image: u8) {
        let mut extra = json!({ "op": "use", "name": drag.name, "image": image });
        if !drag.folder.is_empty() {
            extra["folder"] = json!(drag.folder);
        }
        let params = self.library_params(extra);
        self.face.library.message = match self.run_command("face.library", &params) {
            Ok(_) => format!("{} is now Image {image}", drag.name),
            Err(e) => e.to_string(),
        };
    }

    fn library_thumbnail(&self, ctx: &Context, e: &LibEntry) -> Option<TextureHandle> {
        let mut path = std::path::PathBuf::from(&self.state.library_dir);
        if !e.folder.is_empty() {
            path.push(&e.folder);
        }
        path.push(format!("{}.png", e.name));
        let img = inbetween_codecs::read_file(&path).ok()?;
        let raster = inbetween_raster::Image::from_rgba8(img.width, img.height, &img.data).ok()?;
        let small = raster.downscale_to_fit(120).ok()?;
        Some(ctx.load_texture(
            format!("library {}", path.display()),
            to_color_image(&small, 512),
            egui::TextureOptions::LINEAR,
        ))
    }

    pub(super) fn face_library_window(&mut self, ctx: &Context) {
        if !self.state.face_library_window {
            return;
        }
        // A tile let go over a source panel is used as that image.
        if ctx.input(|i| i.pointer.any_released()) {
            if let Some(drag) = egui::DragAndDrop::take_payload::<LibraryDrag>(ctx) {
                let at = ctx.input(|i| i.pointer.interact_pos());
                if let Some(image) =
                    at.and_then(|p| panel_at(self.layout.source1, self.layout.source2, p))
                {
                    self.use_library_entry(&drag, image);
                }
            }
        }
        if self.face.library.listed != self.state.library_dir {
            self.refresh_library();
        }
        let mut open = true;
        let (mut browse, mut refresh) = (false, false);
        let mut use_as: Option<u8> = None;
        let (mut remove, mut save) = (false, false);
        let entries = self.face.library.entries.clone();
        let mut selected = self.face.library.selected;
        let mut save_name = self.face.library.save_name.clone();
        // Decode a few thumbnails each frame.
        let mut budget = THUMBS_PER_FRAME;
        for e in &entries {
            if budget == 0 {
                break;
            }
            let key = (e.folder.clone(), e.name.clone());
            if !self.face.library.thumbs.contains_key(&key) {
                let t = self.library_thumbnail(ctx, e);
                self.face.library.thumbs.insert(key, t);
                budget -= 1;
            }
        }
        egui::Window::new("Face Library")
            .open(&mut open)
            .collapsible(false)
            .default_pos([300.0, 90.0])
            .default_size([460.0, 380.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Folder");
                    ui.label(if self.state.library_dir.is_empty() {
                        "(none)"
                    } else {
                        &self.state.library_dir
                    });
                    if ui.button("Browse...").clicked() {
                        browse = true;
                    }
                    if ui.button("Refresh").clicked() {
                        refresh = true;
                    }
                });
                ui.weak(&self.face.library.message);
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .id_salt("face_library_tiles")
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for (i, e) in entries.iter().enumerate() {
                                let id = egui::Id::new(("library tile", &e.folder, &e.name));
                                let payload = LibraryDrag {
                                    folder: e.folder.clone(),
                                    name: e.name.clone(),
                                };
                                let tex = self
                                    .face
                                    .library
                                    .thumbs
                                    .get(&(e.folder.clone(), e.name.clone()))
                                    .and_then(Option::as_ref);
                                let tile = ui.dnd_drag_source(id, payload, |ui| {
                                    ui.vertical(|ui| {
                                        match tex {
                                            Some(t) => {
                                                let s = t.size_vec2();
                                                let k = (96.0 / s.x.max(s.y).max(1.0)).min(1.0);
                                                ui.add(
                                                    egui::Image::new(t).fit_to_exact_size(s * k),
                                                );
                                            }
                                            None => {
                                                ui.allocate_space(Vec2::splat(96.0));
                                            }
                                        }
                                        ui.label(&e.name);
                                        ui.weak(format!("{} dots", e.dots));
                                    });
                                });
                                let r = tile.response.interact(egui::Sense::click());
                                if r.clicked() {
                                    selected = Some(i);
                                }
                                if selected == Some(i) {
                                    ui.painter().rect_stroke(
                                        tile.response.rect.expand(2.0),
                                        3.0,
                                        egui::Stroke::new(1.5_f32, Color32::from_rgb(0, 200, 255)),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                            }
                        });
                    });
                ui.horizontal(|ui| {
                    let have = selected.is_some();
                    if ui
                        .add_enabled(have, egui::Button::new("Use as Image 1"))
                        .clicked()
                    {
                        use_as = Some(1);
                    }
                    if ui
                        .add_enabled(have, egui::Button::new("Use as Image 2"))
                        .clicked()
                    {
                        use_as = Some(2);
                    }
                    if ui.add_enabled(have, egui::Button::new("Remove")).clicked() {
                        remove = true;
                    }
                });
                ui.weak("Drag a picture onto Image 1 or Image 2 to use it there.");
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Save the current Image");
                    ui.add(egui::TextEdit::singleline(&mut save_name).desired_width(120.0));
                    if ui
                        .add_enabled(
                            !save_name.trim().is_empty() && !self.state.library_dir.is_empty(),
                            egui::Button::new("Save to library"),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });

        self.face.library.selected = selected;
        self.face.library.save_name = save_name.clone();
        if browse {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                self.state.library_dir = dir.display().to_string();
            }
        }
        if refresh {
            self.face.library.listed.clear();
        }
        let chosen = selected.and_then(|i| entries.get(i)).cloned();
        if let (Some(image), Some(e)) = (use_as, &chosen) {
            let drag = LibraryDrag {
                folder: e.folder.clone(),
                name: e.name.clone(),
            };
            self.use_library_entry(&drag, image);
        }
        if let (true, Some(e)) = (remove, &chosen) {
            let mut extra = json!({ "op": "remove", "name": e.name });
            if !e.folder.is_empty() {
                extra["folder"] = json!(e.folder);
            }
            let params = self.library_params(extra);
            if let Err(err) = self.run_command("face.library", &params) {
                self.face.library.message = err.to_string();
            }
            self.face.library.listed.clear();
        }
        if save {
            let params = self.library_params(json!({
                "name": save_name.trim(),
                "image": self.state.focus_image,
                "overwrite": false,
            }));
            self.face.library.message = match self.run_command("image.saveToLibrary", &params) {
                Ok(_) => format!("saved {}", save_name.trim()),
                Err(e) => e.to_string(),
            };
            self.face.library.listed.clear();
        }
        if !open {
            self.close_face_library();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_lands_on_the_panel_under_the_pointer() {
        let a = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(100.0, 100.0));
        let b = Rect::from_min_max(Pos2::new(110.0, 0.0), Pos2::new(210.0, 100.0));
        assert_eq!(panel_at(a, b, Pos2::new(10.0, 10.0)), Some(1));
        assert_eq!(panel_at(a, b, Pos2::new(150.0, 50.0)), Some(2));
        assert_eq!(panel_at(a, b, Pos2::new(105.0, 50.0)), None);
        assert_eq!(panel_at(Rect::ZERO, Rect::ZERO, Pos2::new(1.0, 1.0)), None);
    }
}
