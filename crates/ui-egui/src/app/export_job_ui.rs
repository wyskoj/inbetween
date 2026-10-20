//! Long exports run on a thread of their own so the window stays alive and can show progress
//! and a Cancel button.
//!
//! The session moves to the thread for the length of the export (the interface cannot touch it
//! meanwhile, so it shows only the progress window) and comes back when the thread ends. The
//! command is the same one the CLI runs; Cancel sets the session's cancel flag.

use super::*;
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
    label: String,
    cancelling: bool,
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
        let id = id.to_string();
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
            label: spec.label.to_string(),
            cancelling: false,
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
            let handle = job.handle.take();
            self.export_job = None;
            match handle.map(JoinHandle::join) {
                Some(Ok((session, result))) => {
                    self.session = session;
                    self.status = Some(match result {
                        Ok(_) => Status {
                            text: format!("{label}: done"),
                            is_error: false,
                        },
                        Err(e) => Status {
                            text: format!("{label}: {e}"),
                            is_error: true,
                        },
                    });
                }
                _ => {
                    self.status = Some(Status {
                        text: format!("{label} stopped unexpectedly; the project was lost"),
                        is_error: true,
                    });
                }
            }
            ctx.request_repaint();
            return false;
        }
        let done = job.progress.load(Ordering::Relaxed);
        let total = job.total.load(Ordering::Relaxed);
        let label = job.label.clone();
        let cancelling = job.cancelling;
        let mut cancel = false;
        egui::CentralPanel::default().show(ctx, |_| {});
        egui::Window::new(&label)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if total > 0 {
                    ui.add(
                        egui::ProgressBar::new((done as f32 / total as f32).clamp(0.0, 1.0))
                            .text(format!("{done} of {total} frames"))
                            .desired_width(280.0),
                    );
                } else {
                    ui.add(egui::Spinner::new());
                    ui.label(format!("{done} frames so far"));
                }
                if ui
                    .add_enabled(!cancelling, egui::Button::new("Cancel"))
                    .clicked()
                {
                    cancel = true;
                }
                if cancelling {
                    ui.weak("Stopping after the current frame...");
                }
            });
        if cancel {
            self.cancel_export();
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(80));
        true
    }
}
