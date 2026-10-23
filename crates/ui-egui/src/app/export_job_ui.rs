//! Long exports run on a thread of their own so the window stays alive and can show progress
//! and a Cancel button.
//!
//! The session moves to the thread for the length of the export (the interface cannot touch it
//! meanwhile, so it shows only the progress window) and comes back when the thread ends. The
//! command is the same one the CLI runs; Cancel sets the session's cancel flag.

use super::*;
use crate::kit::{self, ButtonKind, Icon};
use egui::{CornerRadius, Sense};
use inbetween_engine::FramePreview;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;

/// Commands that can take long enough to deserve a progress window.
pub const LONG_EXPORTS: &[&str] = &[
    "export.movie",
    "export.sequence",
    "export.animation",
    "export.video",
    "export.pdf",
];

pub(super) struct ExportJob {
    handle: Option<JoinHandle<(Session, Result<Value, CommandError>)>>,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    id: String,
    label: String,
    /// The file or folder being written, shown under the title.
    target: String,
    cancelling: bool,
    started: std::time::Instant,
    /// The slot the export thread leaves its latest small frame in.
    frames: Arc<std::sync::Mutex<Option<FramePreview>>>,
    /// The latest frame as a texture, with its size.
    thumb: Option<(TextureHandle, [usize; 2])>,
}

/// What the export is making, for the progress window's title.
fn noun(id: &str) -> &'static str {
    match id {
        "export.video" => "video",
        "export.animation" => "animated image",
        "export.pdf" => "contact sheet",
        _ => "image sequence",
    }
}

/// A long path cut from the front, so the end (the name) stays: "…\Renders\sun-moon.mp4".
fn shorten(path: &str) -> String {
    const KEEP: usize = 44;
    let count = path.chars().count();
    if count <= KEEP + 1 {
        return path.to_string();
    }
    let tail: String = path.chars().skip(count - KEEP).collect();
    format!("…{tail}")
}

/// "about 12 s left" from the time taken so far; `None` until a frame is done.
fn time_left(done: u64, total: u64, elapsed: f64) -> Option<String> {
    if done == 0 || total <= done || !elapsed.is_finite() {
        return None;
    }
    let secs = (elapsed / done as f64 * (total - done) as f64)
        .round()
        .max(1.0) as u64;
    Some(if secs < 90 {
        format!("about {secs} s left")
    } else {
        format!("about {} min left", secs.div_ceil(60))
    })
}

/// "14 s so far", or minutes and seconds past a minute.
fn time_taken(elapsed: f64) -> String {
    let secs = if elapsed.is_finite() {
        elapsed.max(0.0).round() as u64
    } else {
        0
    };
    if secs < 60 {
        format!("{secs} s so far")
    } else {
        format!("{} min {} s so far", secs / 60, secs % 60)
    }
}

/// The last part of a path: the name of the folder or file being written.
fn leaf(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl InbetweenApp {
    /// Whether an export is running on its thread.
    pub fn export_running(&self) -> bool {
        self.export_job.is_some()
    }

    /// Ask the running export to stop at its next frame.
    pub fn cancel_export(&mut self) {
        if let Some(job) = self.export_job.as_mut() {
            job.cancel.store(true, Ordering::Relaxed);
            job.cancelling = true;
        }
    }

    /// Start `id` on a thread of its own. Returns `false` when it cannot (another export is
    /// running, or the command does not exist).
    pub fn start_export(&mut self, id: &str, params: &Value) -> bool {
        if self.export_job.is_some() {
            return false;
        }
        let Some(spec) = self.registry.get(id).copied() else {
            return false;
        };
        let params = self.with_current_sequence(id, params);
        let session = std::mem::take(&mut self.session);
        let cancel = session.cancel_handle();
        let (progress, total) = session.progress_handles();
        let frames = session.frame_preview_handle();
        let target = ["out", "out_dir"]
            .iter()
            .find_map(|k| params[*k].as_str())
            .unwrap_or_default()
            .to_string();
        let id = id.to_string();
        let job_id = id.clone();
        let handle = std::thread::spawn(move || {
            let mut session = session;
            let registry = Registry::standard();
            let result = registry.run_caught(&mut session, &id, &params);
            (session, result)
        });
        self.export_job = Some(ExportJob {
            handle: Some(handle),
            cancel,
            progress,
            total,
            id: job_id,
            label: spec.label.to_string(),
            target,
            cancelling: false,
            started: std::time::Instant::now(),
            frames,
            thumb: None,
        });
        true
    }

    /// While an export runs: collect it when it ends, otherwise draw only its progress window.
    /// Returns whether the rest of the interface must be skipped this frame.
    pub(super) fn export_busy(&mut self, ctx: &Context) -> bool {
        let Some(job) = self.export_job.as_mut() else {
            return false;
        };
        if job.handle.as_ref().is_some_and(JoinHandle::is_finished) {
            let label = job.label.clone();
            let id = job.id.clone();
            let handle = job.handle.take();
            self.export_job = None;
            match handle.map(JoinHandle::join) {
                Some(Ok((session, result))) => {
                    self.session = session;
                    match result {
                        Ok(report) => {
                            let text = feedback::describe(&id, &report)
                                .map_or_else(|| format!("{label}: done"), |f| f.message);
                            self.announce(text, ToastKind::Success, false);
                            self.offer_output(
                                export_folder(&id, &report),
                                export_file(&id, &report),
                            );
                        }
                        Err(e) => self.announce(format!("{label}: {e}"), ToastKind::Error, false),
                    }
                }
                _ => {
                    self.announce(
                        format!("{label} stopped unexpectedly; the project was lost"),
                        ToastKind::Error,
                        false,
                    );
                }
            }
            ctx.request_repaint();
            return false;
        }
        let done = job.progress.load(Ordering::Relaxed);
        let total = job.total.load(Ordering::Relaxed);
        if let Some(frame) = job.frames.lock().ok().and_then(|mut f| f.take()) {
            let size = [frame.width as usize, frame.height as usize];
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &frame.rgba);
            job.thumb = Some((
                ctx.load_texture("export_frame", image, TextureOptions::LINEAR),
                size,
            ));
        }
        let title = if job.cancelling {
            "Stopping…".to_string()
        } else {
            format!("Exporting {}", noun(&job.id))
        };
        let subtitle = if job.cancelling && job.id.starts_with("export.") && is_frames(&job.id) {
            format!(
                "Finishing frame {}. The {done} frames already in “{}” are kept.",
                done + 1,
                leaf(&job.target)
            )
        } else if job.cancelling {
            "Finishing the current frame.".to_string()
        } else if job.target.is_empty() {
            "Writing the frames.".to_string()
        } else {
            shorten(&job.target)
        };
        let taken = job.started.elapsed().as_secs_f64();
        let left = time_left(done, total, taken);
        let so_far = time_taken(taken);
        let cancelling = job.cancelling;
        let thumb = job.thumb.as_ref().map(|(t, s)| (t.id(), *s));
        egui::CentralPanel::default().show(ctx, |_| {});
        let (_, cancel, _) = kit::dialog(
            ctx,
            "export_progress",
            &title,
            &subtitle,
            480.0,
            |ui, t| {
                progress_body(
                    ui,
                    t,
                    done,
                    total,
                    &so_far,
                    left.as_deref(),
                    thumb,
                    cancelling,
                )
            },
            |ui, t| {
                ui.label(
                    egui::RichText::new("Inbetween is busy until this finishes.")
                        .size(t.text.label)
                        .color(t.color.text_tertiary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    kit::button(
                        ui,
                        ButtonKind::Secondary,
                        "Stop",
                        !cancelling,
                        t.control.height,
                    )
                    .clicked()
                })
                .inner
            },
        );
        if cancel {
            self.cancel_export();
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(80));
        true
    }
}

const THUMB_BOX: Vec2 = Vec2::new(112.0, 84.0);

/// Whether the export writes a folder of numbered frames, which a stop leaves in place.
fn is_frames(id: &str) -> bool {
    matches!(id, "export.movie" | "export.sequence")
}

/// The frame just finished, the percentage, the progress bar and its figures.
#[allow(clippy::too_many_arguments)]
fn progress_body(
    ui: &mut egui::Ui,
    t: &crate::theme::Theme,
    done: u64,
    total: u64,
    so_far: &str,
    left: Option<&str>,
    thumb: Option<(egui::TextureId, [usize; 2])>,
    cancelling: bool,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.lg;
        let (rect, _) = ui.allocate_exact_size(THUMB_BOX, Sense::hover());
        let card = CornerRadius::same(t.radius.control);
        ui.painter().rect_filled(rect, card, t.color.app);
        match thumb {
            Some((id, [w, h])) => {
                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                // Fit the whole frame inside the box, keeping its shape.
                let scale = (THUMB_BOX.x / w.max(1) as f32).min(THUMB_BOX.y / h.max(1) as f32);
                let shown = Rect::from_center_size(
                    rect.center(),
                    Vec2::new(w as f32 * scale, h as f32 * scale),
                );
                ui.painter().image(id, shown, uv, egui::Color32::WHITE);
            }
            None => {
                let at = Rect::from_center_size(rect.center(), Vec2::splat(24.0));
                kit::icon(ui, at, Icon::Image, t.color.text_disabled, 1.6);
            }
        }
        ui.painter().rect_stroke(
            rect,
            card,
            egui::Stroke::new(1.0_f32, t.color.line),
            egui::StrokeKind::Inside,
        );
        // The figures sit centred against the picture.
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), THUMB_BOX.y),
            egui::Layout::top_down(egui::Align::Min).with_main_align(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.y = t.space.sm;
                let fraction = if total > 0 {
                    (done as f32 / total as f32).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                if total > 0 {
                    let colour = if cancelling {
                        t.color.text_secondary
                    } else {
                        t.color.text
                    };
                    ui.label(
                        egui::RichText::new(format!("{}%", (fraction * 100.0).floor() as u32))
                            .font(t.medium(t.text.title + 6.0))
                            .color(colour),
                    );
                }
                let width = ui.available_width();
                let (bar, _) = ui.allocate_exact_size(Vec2::new(width, 8.0), Sense::hover());
                ui.painter()
                    .rect_filled(bar, CornerRadius::same(4), t.color.field_active);
                let fill = if cancelling {
                    t.color.dash
                } else {
                    t.color.focus
                };
                ui.painter().rect_filled(
                    Rect::from_min_size(bar.min, Vec2::new(bar.width() * fraction, bar.height())),
                    CornerRadius::same(4),
                    fill,
                );
                let count = if total > 0 {
                    format!("{done} of {total} frames")
                } else {
                    format!("{done} frames so far")
                };
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(count)
                            .size(t.text.label)
                            .color(t.color.text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let note = match left.filter(|_| !cancelling) {
                            Some(left) => format!("{so_far} · {left}"),
                            None if cancelling => "stopping".to_string(),
                            None => so_far.to_string(),
                        };
                        ui.label(
                            egui::RichText::new(note)
                                .size(t.text.label)
                                .color(t.color.text_tertiary),
                        );
                    });
                });
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_paths_keep_their_end() {
        assert_eq!(shorten("C:/a/b.mp4"), "C:/a/b.mp4");
        let long = format!("C:/{}/sun-moon.mp4", "folder/".repeat(20));
        let short = shorten(&long);
        assert!(short.starts_with('…') && short.ends_with("sun-moon.mp4"));
        assert_eq!(short.chars().count(), 45);
    }

    #[test]
    fn the_time_so_far_switches_to_minutes() {
        assert_eq!(time_taken(14.2), "14 s so far");
        assert_eq!(time_taken(125.0), "2 min 5 s so far");
        assert_eq!(time_taken(f64::NAN), "0 s so far");
        assert_eq!(leaf("C:/out/demo frames"), "demo frames");
    }

    #[test]
    fn the_estimate_waits_for_a_frame_and_switches_to_minutes() {
        assert_eq!(time_left(0, 100, 5.0), None);
        assert_eq!(time_left(100, 100, 5.0), None);
        assert_eq!(time_left(50, 100, f64::NAN), None);
        assert_eq!(time_left(50, 100, 10.0).as_deref(), Some("about 10 s left"));
        assert_eq!(
            time_left(1, 100, 10.0).as_deref(),
            Some("about 17 min left")
        );
        assert_eq!(time_left(99, 100, 0.001).as_deref(), Some("about 1 s left"));
    }
}
