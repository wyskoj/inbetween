//! Frame Sequence mode: a scrolling grid with a thumbnail of every frame.
//!
//! Only the rows on screen are drawn, thumbnails are rendered a few at a time within a time
//! budget per UI frame, and finished ones are cached by project revision, scope and size.

use super::*;
use crate::state::PreviewScope;
use std::time::{Duration, Instant};

/// Most thumbnails kept; past it the cache starts over.
const MAX_CACHED: usize = 512;
/// Time spent rendering thumbnails in one UI frame.
const BUDGET: Duration = Duration::from_millis(12);
const GAP: f32 = 6.0;
const LABEL_H: f32 = 16.0;

#[derive(Default)]
pub(super) struct StripCache {
    key: Option<(u64, PreviewScope, usize, u32)>,
    thumbs: HashMap<u32, TextureHandle>,
}

impl StripCache {
    pub(super) fn clear(&mut self) {
        self.key = None;
        self.thumbs.clear();
    }

    pub(super) fn len(&self) -> usize {
        self.thumbs.len()
    }
}

/// How many thumbnails of `cell` points fit across `width`, and how many rows `frames` need.
pub(crate) fn grid_shape(width: f32, cell: f32, frames: u32) -> (usize, usize) {
    let across = if width.is_finite() && cell > 0.0 {
        ((width + GAP) / (cell + GAP)).floor().max(1.0) as usize
    } else {
        1
    };
    (across, (frames as usize).div_ceil(across))
}

impl InbetweenApp {
    /// Draw the grid into `ui`. A click moves the playhead, a double click goes back to the
    /// full layout at that frame.
    pub(super) fn frame_strip(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.session.has_sequences() {
            ui.label("Open a project to see its frames.");
            return;
        }
        ui.horizontal(|ui| {
            ui.label("Thumbnail size");
            ui.add(egui::Slider::new(&mut self.state.strip_size, 48.0..=256.0).integer());
            let scope = match self.state.preview_scope {
                PreviewScope::Sequence => "frames of the selected sequence",
                PreviewScope::Movie => "frames of the whole movie",
            };
            ui.label(format!("{} {scope}", self.transport_frames()));
        });
        let cell = self.state.strip_size.clamp(48.0, 256.0);
        let frames = self.transport_frames();
        let (across, rows) = grid_shape(ui.available_width(), cell, frames);
        let px = ((cell * ctx.pixels_per_point()).round() as u32).clamp(16, 1024);
        let key = (
            self.session.revision(),
            self.state.preview_scope,
            self.state.sequence,
            px,
        );
        if self.strip.key != Some(key) || self.strip.len() > MAX_CACHED {
            self.strip.clear();
            self.strip.key = Some(key);
        }
        let current = self.transport_pos();
        let row_h = cell * 0.75 + LABEL_H + GAP;
        let started = Instant::now();
        let mut missing = false;
        let mut clicked: Option<(u32, bool)> = None;
        let movie = self.movie_scope();
        let sequence = self.state.sequence;
        // Built on the first thumbnail that is missing.
        let mut renderer: Option<Prepared> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_h, rows, |ui, range| {
                for row in range {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = GAP;
                        for col in 0..across {
                            let frame = (row * across + col) as u32;
                            if frame >= frames {
                                break;
                            }
                            if !self.strip.thumbs.contains_key(&frame) {
                                if started.elapsed() < BUDGET {
                                    if renderer.is_none() && !movie {
                                        renderer = self.session.prepare(sequence).ok();
                                    }
                                    let image = if movie {
                                        self.session.movie_thumbnail(u64::from(frame) + 1, px).ok()
                                    } else {
                                        renderer.as_ref().and_then(|p| {
                                            let image =
                                                p.renderer().ok()?.render_frame(frame).ok()?;
                                            p.finish(image, frame)
                                                .ok()?
                                                .downscale_to_fit(px)
                                                .ok()
                                        })
                                    };
                                    if let Some(image) = image {
                                        let color = to_color_image(
                                            &image,
                                            ctx.input(|i| i.max_texture_side),
                                        );
                                        let tex = ctx.load_texture(
                                            format!("strip {frame}"),
                                            color,
                                            TextureOptions::LINEAR,
                                        );
                                        self.strip.thumbs.insert(frame, tex);
                                    }
                                }
                                if !self.strip.thumbs.contains_key(&frame) {
                                    missing = true;
                                }
                            }
                            ui.vertical(|ui| {
                                let (rect, response) = ui.allocate_exact_size(
                                    Vec2::new(cell, cell * 0.75),
                                    egui::Sense::click(),
                                );
                                ui.painter()
                                    .rect_filled(rect, 2.0, egui::Color32::from_gray(30));
                                if let Some(tex) = self.strip.thumbs.get(&frame) {
                                    let size = tex.size_vec2();
                                    let scale = (rect.width() / size.x).min(rect.height() / size.y);
                                    let shown = Rect::from_center_size(rect.center(), size * scale);
                                    ui.painter().image(
                                        tex.id(),
                                        shown,
                                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                        egui::Color32::WHITE,
                                    );
                                }
                                if frame == current {
                                    ui.painter().rect_stroke(
                                        rect,
                                        2.0_f32,
                                        egui::Stroke::new(
                                            2.0_f32,
                                            egui::Color32::from_rgb(90, 170, 255),
                                        ),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                                ui.label(
                                    egui::RichText::new(format!("{}", frame + 1)).small().weak(),
                                );
                                if response.double_clicked() {
                                    clicked = Some((frame, true));
                                } else if response.clicked() {
                                    clicked = Some((frame, false));
                                }
                            });
                        }
                    });
                }
            });
        if missing {
            ctx.request_repaint();
        }
        if let Some((frame, open)) = clicked {
            self.set_transport_pos(frame);
            if open {
                self.set_mode(crate::state::ViewMode::Full);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_fits_whole_cells_and_survives_nonsense() {
        assert_eq!(grid_shape(1000.0, 100.0, 25), (9, 3));
        assert_eq!(grid_shape(50.0, 100.0, 5), (1, 5));
        assert_eq!(grid_shape(f32::NAN, 100.0, 5), (1, 5));
        assert_eq!(grid_shape(500.0, 0.0, 5), (1, 5));
        assert_eq!(grid_shape(500.0, 100.0, 0), (4, 0));
    }
}
