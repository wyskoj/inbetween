//! The toast: the transient card that says what a command just did (see `kit::toast`). Every
//! message also lands in the status bar, which keeps the latest one.

use super::*;
use crate::kit::{self, ToastKind};

pub(super) struct Toast {
    text: String,
    kind: ToastKind,
    undoable: bool,
    /// Frame time the card first showed; moved forward while the pointer rests on it.
    started: Option<f64>,
}

impl InbetweenApp {
    /// Tell the person what happened: a toast now, and the same words in the status bar.
    pub(super) fn announce(&mut self, text: String, kind: ToastKind, undoable: bool) {
        self.status = Some(Status {
            text: text.clone(),
            is_error: kind == ToastKind::Error,
        });
        self.toast = Some(Toast {
            text,
            kind,
            undoable,
            started: None,
        });
    }

    /// The text and kind of the toast on screen, for tests.
    pub fn toast_text(&self) -> Option<(&str, ToastKind)> {
        self.toast.as_ref().map(|t| (t.text.as_str(), t.kind))
    }

    pub(super) fn toast_window(&mut self, ctx: &Context) {
        let now = ctx.input(|i| i.time);
        let Some(toast) = self.toast.as_mut() else {
            return;
        };
        let started = *toast.started.get_or_insert(now);
        let age = now - started;
        if age > toast.kind.lifetime() + kit::FADE {
            self.toast = None;
            return;
        }
        let out = kit::toast(ctx, &toast.text, toast.kind, toast.undoable, age);
        if out.hovered {
            toast.started = Some(now);
        }
        // Wake for the start of the fade, then animate it.
        let until_fade = (toast.kind.lifetime() - age).max(0.0);
        let wait = if until_fade > 0.0 {
            until_fade
        } else {
            1.0 / 30.0
        };
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
        if out.dismissed {
            self.toast = None;
        }
        if out.undo {
            self.toast = None;
            let _ = self.run_command("edit.undo", &json!({}));
        }
    }
}
