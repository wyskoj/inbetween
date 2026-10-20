//! The Mix Faces window: per-face feature and shape percentages, the average and random modes,
//! a candidate gallery, and saving or loading a recipe. Every mix is a `face.mix` command that
//! writes PNG files; the window shows them as thumbnails.

use super::*;
use egui::TextureHandle;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MixMode {
    Custom,
    Average,
    Random,
}

impl MixMode {
    fn id(self) -> &'static str {
        match self {
            Self::Custom => "custom",
            Self::Average => "average",
            Self::Random => "random",
        }
    }
}

#[derive(Clone)]
pub(super) struct MixRow {
    pub sequence: usize,
    pub on: bool,
    pub feature: f64,
    pub shape: f64,
}

pub(super) struct Candidate {
    pub path: PathBuf,
    pub texture: Option<TextureHandle>,
}

pub(super) struct MixUi {
    pub rows: Vec<MixRow>,
    pub mode: MixMode,
    pub lock: bool,
    pub seed: u64,
    pub count: u32,
    pub image: u8,
    pub candidates: Vec<Candidate>,
    pub chosen: Option<usize>,
    pub message: String,
    /// Seed of the first candidate of the last mix, so a recipe can be rebuilt for one.
    pub last_seed: u64,
}

impl Default for MixUi {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            mode: MixMode::Average,
            lock: true,
            seed: 1,
            count: 6,
            image: 1,
            candidates: Vec::new(),
            chosen: None,
            message: String::new(),
            last_seed: 1,
        }
    }
}

/// `values` as "a,b,c" for the command's percentage lists.
pub(crate) fn percent_list(values: &[f64]) -> String {
    values
        .iter()
        .map(|v| format!("{}", (v * 100.0).round() / 100.0))
        .collect::<Vec<_>>()
        .join(",")
}

/// Where the window writes its candidates.
fn mix_dir() -> PathBuf {
    std::env::temp_dir().join(format!("inbetween-mix-{}", std::process::id()))
}

impl InbetweenApp {
    /// Choose how the Mix Faces window mixes: `average`, `custom` or `random`, with the seed,
    /// the number of random candidates and the image to take faces from.
    pub fn configure_mix(&mut self, mode: &str, seed: u64, count: u32, image: u8) {
        let ui = &mut self.face.mix;
        ui.mode = match mode {
            "custom" => MixMode::Custom,
            "random" => MixMode::Random,
            _ => MixMode::Average,
        };
        ui.seed = seed;
        ui.count = count;
        ui.image = image;
    }

    /// The candidate files the last mix wrote.
    pub fn mix_candidates(&self) -> Vec<PathBuf> {
        self.face
            .mix
            .candidates
            .iter()
            .map(|c| c.path.clone())
            .collect()
    }

    pub fn open_face_mix(&mut self) {
        self.state.face_mix_window = true;
    }

    pub fn close_face_mix(&mut self) {
        self.state.face_mix_window = false;
    }

    /// Keep one row per sequence, remembering what was typed for the ones that remain.
    fn sync_mix_rows(&mut self) {
        let count = self.session.project().sequences.len();
        let rows = &mut self.face.mix.rows;
        if rows.len() == count && rows.iter().enumerate().all(|(i, r)| r.sequence == i) {
            return;
        }
        let old = std::mem::take(rows);
        *rows = (0..count)
            .map(|i| {
                old.iter()
                    .find(|r| r.sequence == i)
                    .cloned()
                    .unwrap_or(MixRow {
                        sequence: i,
                        on: true,
                        feature: 50.0,
                        shape: 50.0,
                    })
            })
            .collect();
    }

    /// The `face.mix` parameters the window's settings describe, writing to `out`.
    pub(crate) fn mix_params(&self, out: &std::path::Path, count: u32) -> Value {
        let ui = &self.face.mix;
        let used: Vec<&MixRow> = ui.rows.iter().filter(|r| r.on).collect();
        let sequences = used
            .iter()
            .map(|r| r.sequence.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let mut p = json!({
            "out": out.display().to_string(),
            "sequences": sequences,
            "image": ui.image,
            "mode": ui.mode.id(),
            "lock": ui.lock,
            "seed": ui.seed,
            "count": count,
        });
        if ui.mode == MixMode::Custom {
            let f: Vec<f64> = used.iter().map(|r| r.feature).collect();
            p["features"] = json!(percent_list(&f));
            if !ui.lock {
                let s: Vec<f64> = used.iter().map(|r| r.shape).collect();
                p["shapes"] = json!(percent_list(&s));
            }
        }
        p
    }

    fn load_candidates(&mut self, ctx: &Context, files: Vec<PathBuf>) {
        self.face.mix.candidates = files
            .into_iter()
            .map(|path| {
                let texture = inbetween_codecs::read_file(&path).ok().and_then(|img| {
                    let raster =
                        inbetween_raster::Image::from_rgba8(img.width, img.height, &img.data)
                            .ok()?;
                    let small = raster.downscale_to_fit(200).ok()?;
                    Some(ctx.load_texture(
                        format!("mix {}", path.display()),
                        to_color_image(&small, 512),
                        egui::TextureOptions::LINEAR,
                    ))
                });
                Candidate { path, texture }
            })
            .collect();
        self.face.mix.chosen = (!self.face.mix.candidates.is_empty()).then_some(0);
    }

    /// Run `face.mix` for the window's settings and show what it wrote.
    pub fn run_mix(&mut self, ctx: &Context) {
        let dir = mix_dir();
        let _ = std::fs::create_dir_all(&dir);
        // Old candidates would otherwise sit beside the new ones.
        if let Ok(read) = std::fs::read_dir(&dir) {
            for e in read.flatten() {
                let _ = std::fs::remove_file(e.path());
            }
        }
        let count = if self.face.mix.mode == MixMode::Random {
            self.face.mix.count.clamp(1, 12)
        } else {
            1
        };
        let params = self.mix_params(&dir.join("mix.png"), count);
        self.face.mix.last_seed = self.face.mix.seed;
        match self.run_command("face.mix", &params) {
            Ok(r) => {
                let files: Vec<PathBuf> = r["files"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(PathBuf::from))
                            .collect()
                    })
                    .unwrap_or_default();
                self.face.mix.message = format!("{} mixed face(s)", files.len());
                self.load_candidates(ctx, files);
            }
            Err(e) => {
                self.face.mix.message = e.to_string();
                self.face.mix.candidates.clear();
                self.face.mix.chosen = None;
            }
        }
    }

    pub(super) fn face_mix_window(&mut self, ctx: &Context) {
        if !self.state.face_mix_window {
            return;
        }
        self.sync_mix_rows();
        let names: Vec<String> = self
            .session
            .project()
            .sequences
            .iter()
            .map(|s| s.name.clone())
            .collect();
        let mut open = true;
        let (mut mix, mut save_pick, mut save_recipe, mut load_recipe) =
            (false, false, false, false);
        let ui_state = &mut self.face.mix;
        egui::Window::new("Mix Faces")
            .open(&mut open)
            .collapsible(false)
            .default_pos([280.0, 80.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Faces from");
                    ui.selectable_value(&mut ui_state.image, 1, "Image 1");
                    ui.selectable_value(&mut ui_state.image, 2, "Image 2");
                    ui.separator();
                    for (m, label) in [
                        (MixMode::Custom, "Custom"),
                        (MixMode::Average, "Average"),
                        (MixMode::Random, "Random"),
                    ] {
                        ui.selectable_value(&mut ui_state.mode, m, label);
                    }
                });
                if ui_state.mode != MixMode::Average {
                    ui.checkbox(&mut ui_state.lock, "Shape follows feature");
                }
                if ui_state.mode == MixMode::Random {
                    ui.horizontal(|ui| {
                        ui.label("Seed");
                        ui.add(
                            egui::DragValue::new(&mut ui_state.seed)
                                .range(0..=9_000_000_000_000_000u64),
                        );
                        ui.label("Candidates");
                        ui.add(egui::DragValue::new(&mut ui_state.count).range(1..=12));
                    });
                }
                egui::Grid::new("mix_rows")
                    .num_columns(4)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("");
                        ui.label("Face");
                        ui.label("Feature %");
                        ui.label("Shape %");
                        ui.end_row();
                        let custom = ui_state.mode == MixMode::Custom;
                        let lock = ui_state.lock;
                        for row in &mut ui_state.rows {
                            ui.checkbox(&mut row.on, "");
                            ui.label(names.get(row.sequence).map_or("", String::as_str));
                            ui.add_enabled(
                                custom && row.on,
                                egui::Slider::new(&mut row.feature, 0.0..=100.0).integer(),
                            );
                            if custom && lock {
                                row.shape = row.feature;
                            }
                            ui.add_enabled(
                                custom && row.on && !lock,
                                egui::Slider::new(&mut row.shape, 0.0..=100.0).integer(),
                            );
                            ui.end_row();
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button("Mix").clicked() {
                        mix = true;
                    }
                    if ui.button("Load recipe...").clicked() {
                        load_recipe = true;
                    }
                    if ui
                        .add_enabled(
                            ui_state.chosen.is_some(),
                            egui::Button::new("Save recipe..."),
                        )
                        .clicked()
                    {
                        save_recipe = true;
                    }
                    if ui
                        .add_enabled(
                            ui_state.chosen.is_some(),
                            egui::Button::new("Save picture..."),
                        )
                        .clicked()
                    {
                        save_pick = true;
                    }
                });
                if !ui_state.message.is_empty() {
                    ui.weak(&ui_state.message);
                }
                ui.horizontal_wrapped(|ui| {
                    for (i, c) in ui_state.candidates.iter().enumerate() {
                        let chosen = ui_state.chosen == Some(i);
                        let Some(tex) = &c.texture else { continue };
                        let size = tex.size_vec2();
                        let scale = (110.0 / size.x.max(size.y).max(1.0)).min(1.0);
                        let image = egui::Image::new(tex).fit_to_exact_size(size * scale);
                        let resp = ui.add(egui::Button::image(image).selected(chosen));
                        if resp.clicked() {
                            ui_state.chosen = Some(i);
                        }
                    }
                });
            });

        if mix {
            self.run_mix(ctx);
        }
        let chosen = self
            .face
            .mix
            .chosen
            .and_then(|i| self.face.mix.candidates.get(i).map(|c| (i, c.path.clone())));
        if let (true, Some((_, path))) = (save_pick, &chosen) {
            if let Some(to) = rfd::FileDialog::new()
                .add_filter("PNG", &["png"])
                .set_file_name("mix.png")
                .save_file()
            {
                if let Err(e) = std::fs::copy(path, &to) {
                    self.face.mix.message = format!("could not save: {e}");
                }
            }
        }
        if let (true, Some((k, _))) = (save_recipe, &chosen) {
            if let Some(to) = rfd::FileDialog::new()
                .add_filter("Mix recipe", &["inbetweenmix"])
                .save_file()
            {
                // Rebuild the chosen candidate on its own so the recipe describes exactly it.
                let dir = mix_dir();
                let mut params = self.mix_params(&dir.join("recipe.png"), 1);
                params["seed"] = json!(self.face.mix.last_seed.wrapping_add(*k as u64));
                params["recipe_save"] = json!(to.display().to_string());
                self.face.mix.message = match self.run_command("face.mix", &params) {
                    Ok(_) => "recipe saved".to_string(),
                    Err(e) => e.to_string(),
                };
            }
        }
        if load_recipe {
            if let Some(from) = rfd::FileDialog::new()
                .add_filter("Mix recipe", &["inbetweenmix"])
                .pick_file()
            {
                let dir = mix_dir();
                let _ = std::fs::create_dir_all(&dir);
                let mut params = self.mix_params(&dir.join("mix.png"), 1);
                params["recipe_load"] = json!(from.display().to_string());
                match self.run_command("face.mix", &params) {
                    Ok(r) => {
                        let files = r["files"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(PathBuf::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        self.load_candidates(ctx, files);
                        self.face.mix.message = "recipe applied".to_string();
                    }
                    Err(e) => self.face.mix.message = e.to_string(),
                }
            }
        }
        if !open {
            self.close_face_mix();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::percent_list;

    #[test]
    fn percentages_are_written_as_plain_lists() {
        assert_eq!(percent_list(&[50.0, 30.0, 20.0]), "50,30,20");
        assert_eq!(percent_list(&[33.333_333, 0.0]), "33.33,0");
        assert_eq!(percent_list(&[]), "");
    }
}
