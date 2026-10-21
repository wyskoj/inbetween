//! The Open Visually window: recent projects, or the projects in a folder, as a grid of
//! thumbnails. The list comes from the engine's `file.openVisual` command; this file only
//! parses its result and draws it.

use base64::Engine as _;
use egui::{Context, TextureHandle, TextureOptions, Vec2};
use serde_json::Value;
use std::collections::HashMap;

/// Longest side a thumbnail is drawn at, in points.
const CELL_IMAGE: f32 = 160.0;
const CELL_WIDTH: f32 = 176.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Recent,
    Folder,
}

/// One project in the grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub sequences: Option<u64>,
    pub kind: String,
    pub modified: Option<u64>,
    /// Why the project could not be read, when it could not.
    pub error: Option<String>,
    /// The embedded thumbnail, as PNG bytes.
    pub png: Option<Vec<u8>>,
}

/// What the person did in the window this frame.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    None,
    /// Open this project.
    Open(String),
    /// Choose another folder to browse.
    PickFolder,
    /// The tab changed; the list needs refreshing.
    Refresh,
    Close,
}

/// Turn a `file.openVisual` result into entries, and say whether the list was cut short.
pub fn parse_entries(result: &Value) -> (Vec<Entry>, bool) {
    let list = result["projects"].as_array().map_or(&[][..], Vec::as_slice);
    let entries = list
        .iter()
        .map(|e| Entry {
            path: e["path"].as_str().unwrap_or_default().to_string(),
            name: e["name"].as_str().unwrap_or_default().to_string(),
            sequences: e["sequences"].as_u64(),
            kind: e["kind"].as_str().unwrap_or_default().to_string(),
            modified: e["modified"].as_u64(),
            error: e["error"].as_str().map(str::to_string),
            png: e["thumbnail_png_base64"]
                .as_str()
                .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok()),
        })
        .collect();
    (entries, result["truncated"].as_bool().unwrap_or(false))
}

pub struct OpenVisual {
    pub tab: Tab,
    pub folder: Option<String>,
    pub entries: Vec<Entry>,
    pub selected: Option<usize>,
    /// A line under the grid: the list was cut short, or the folder could not be read.
    pub note: Option<String>,
    /// Decoded thumbnails, kept while the file (path and modified time) is unchanged.
    textures: HashMap<(String, Option<u64>), Option<TextureHandle>>,
}

impl Default for OpenVisual {
    fn default() -> Self {
        Self {
            tab: Tab::Recent,
            folder: None,
            entries: Vec::new(),
            selected: None,
            note: None,
            textures: HashMap::new(),
        }
    }
}

impl OpenVisual {
    pub fn set_entries(&mut self, result: &Value) {
        let (entries, truncated) = parse_entries(result);
        self.entries = entries;
        self.selected = None;
        self.note = truncated.then(|| "Only the first projects of the folder are shown.".into());
        let live: Vec<(String, Option<u64>)> = self
            .entries
            .iter()
            .map(|e| (e.path.clone(), e.modified))
            .collect();
        self.textures.retain(|k, _| live.contains(k));
    }

    fn texture(&mut self, ctx: &Context, entry: &Entry) -> Option<TextureHandle> {
        let key = (entry.path.clone(), entry.modified);
        if let Some(hit) = self.textures.get(&key) {
            return hit.clone();
        }
        let handle = entry
            .png
            .as_deref()
            .and_then(|png| inbetween_codecs::decode(png).ok())
            .map(|img| {
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [img.width as usize, img.height as usize],
                    &img.data,
                );
                ctx.load_texture(
                    format!("project {}", entry.path),
                    color,
                    TextureOptions::LINEAR,
                )
            });
        self.textures.insert(key, handle.clone());
        handle
    }

    pub fn show(&mut self, ctx: &Context) -> Outcome {
        let mut outcome = Outcome::None;
        let mut open = true;
        egui::Window::new("Open Visually")
            .open(&mut open)
            .collapsible(false)
            .default_width(700.0)
            .min_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (tab, label) in [(Tab::Recent, "Recent"), (Tab::Folder, "Folder")] {
                        if ui.selectable_label(self.tab == tab, label).clicked() && self.tab != tab
                        {
                            self.tab = tab;
                            outcome = Outcome::Refresh;
                        }
                    }
                    if self.tab == Tab::Folder {
                        ui.separator();
                        if ui.button("Choose folder...").clicked() {
                            outcome = Outcome::PickFolder;
                        }
                        if let Some(f) = &self.folder {
                            ui.add(egui::Label::new(f.as_str()).truncate());
                        }
                    }
                });
                ui.separator();
                let entries = self.entries.clone();
                egui::ScrollArea::vertical()
                    .max_height(380.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        if entries.is_empty() {
                            ui.label(match self.tab {
                                Tab::Recent => "No recent projects yet.",
                                Tab::Folder if self.folder.is_none() => {
                                    "Choose a folder to see its projects."
                                }
                                Tab::Folder => "No projects in this folder.",
                            });
                        }
                        ui.horizontal_wrapped(|ui| {
                            for (i, entry) in entries.iter().enumerate() {
                                self.cell(ui, ctx, i, entry, &mut outcome);
                            }
                        });
                    });
                if let Some(note) = &self.note {
                    ui.label(note.as_str());
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let chosen = self
                        .selected
                        .and_then(|i| self.entries.get(i))
                        .filter(|e| e.error.is_none());
                    if ui
                        .add_enabled(chosen.is_some(), egui::Button::new("Open"))
                        .clicked()
                    {
                        if let Some(e) = chosen {
                            outcome = Outcome::Open(e.path.clone());
                        }
                    }
                    if ui.button("Close").clicked() {
                        outcome = Outcome::Close;
                    }
                });
            });
        if !open {
            outcome = Outcome::Close;
        }
        outcome
    }

    fn cell(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        i: usize,
        entry: &Entry,
        out: &mut Outcome,
    ) {
        let texture = self.texture(ctx, entry);
        let selected = self.selected == Some(i);
        let frame = egui::Frame::group(ui.style()).fill(if selected {
            ui.visuals().selection.bg_fill
        } else {
            ui.visuals().faint_bg_color
        });
        let response = frame
            .show(ui, |ui| {
                ui.set_width(CELL_WIDTH - 16.0);
                ui.vertical_centered(|ui| {
                    match &texture {
                        Some(t) => {
                            let size = t.size_vec2();
                            let scale = (CELL_IMAGE / size.x.max(size.y).max(1.0)).min(1.0);
                            ui.image((t.id(), size * scale));
                        }
                        None => {
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::new(CELL_IMAGE, CELL_IMAGE * 0.6),
                                egui::Sense::hover(),
                            );
                            ui.painter()
                                .rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "no preview",
                                egui::FontId::proportional(12.0),
                                ui.visuals().weak_text_color(),
                            );
                        }
                    }
                    ui.label(egui::RichText::new(&entry.name).strong());
                    match (&entry.error, entry.sequences) {
                        (Some(e), _) => {
                            ui.colored_label(
                                crate::theme::get(ui.ctx()).color.danger_text,
                                "cannot open",
                            )
                            .on_hover_text(e);
                        }
                        (None, Some(n)) => {
                            let s = if n == 1 { "sequence" } else { "sequences" };
                            ui.weak(format!("{n} {s}"));
                        }
                        (None, None) => {}
                    }
                });
            })
            .response
            .interact(egui::Sense::click())
            .on_hover_text(&entry.path);
        if response.clicked() {
            self.selected = Some(i);
        }
        if response.double_clicked() && entry.error.is_none() {
            *out = Outcome::Open(entry.path.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn results_parse_into_entries() {
        let png = base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3]);
        let (entries, truncated) = parse_entries(&json!({
            "truncated": true,
            "projects": [
                {"path": "a.inbetween", "name": "a", "sequences": 2, "kind": "layer",
                 "modified": 5, "thumbnail_png_base64": png},
                {"path": "b.inbetween", "name": "b", "error": "bad"},
                {"path": "c.inbetween", "name": "c", "sequences": 0, "thumbnail_png_base64": null},
            ],
        }));
        assert!(truncated);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].png.as_deref(), Some(&[1u8, 2, 3][..]));
        assert_eq!(
            (entries[0].sequences, entries[0].kind.as_str()),
            (Some(2), "layer")
        );
        assert_eq!(entries[1].error.as_deref(), Some("bad"));
        assert!(entries[2].png.is_none());
        // Anything unexpected is an empty list, not a panic.
        for junk in [
            json!(null),
            json!([]),
            json!({"projects": 5}),
            json!({"projects": [1, "x"]}),
        ] {
            let _ = parse_entries(&junk);
        }
        assert!(parse_entries(&json!(null)).0.is_empty());
    }
}
