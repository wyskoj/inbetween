//! The toast: a small card just above the transport bar that says what a command just did.
//! The status bar holds the same words but sits in a corner; a toast appears where the person
//! is looking, fades by itself, and offers Undo when the command edited the project.

use super::{button, icon, ButtonKind, Icon};
use crate::theme;
use egui::{Align2, Color32, Context, CornerRadius, Frame, Margin, Order, Sense, Stroke, Vec2};

/// How a toast reads. Errors and warnings stay up longer than a plain "done".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Info,
    Warning,
    Error,
}

impl ToastKind {
    /// Seconds on screen before the fade starts.
    pub fn lifetime(self) -> f64 {
        match self {
            Self::Success | Self::Info => 3.5,
            Self::Warning => 6.0,
            Self::Error => 8.0,
        }
    }
}

/// What the person did with a toast this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToastOutcome {
    pub undo: bool,
    pub dismissed: bool,
    /// The pointer is over the card, so the timer should wait.
    pub hovered: bool,
}

/// Seconds the fade at the end of a toast's life takes.
pub const FADE: f64 = 0.4;

/// Draw the toast `age` seconds into its life. `age` past `kind.lifetime()` fades it out;
/// the caller drops it once `age` passes `lifetime + FADE`.
pub fn toast(ctx: &Context, text: &str, kind: ToastKind, undoable: bool, age: f64) -> ToastOutcome {
    let t = theme::get(ctx);
    let fade = ((kind.lifetime() + FADE - age) / FADE).clamp(0.0, 1.0) as f32;
    let (accent, glyph) = match kind {
        ToastKind::Success => (t.color.accent_text, Icon::Check),
        ToastKind::Info => (t.color.text_secondary, Icon::Check),
        ToastKind::Warning => (t.color.warn_text, Icon::Alert),
        ToastKind::Error => (t.color.danger_text, Icon::Alert),
    };
    let mut out = ToastOutcome::default();
    egui::Area::new(egui::Id::new("inbetween_toast"))
        .order(Order::Foreground)
        .anchor(
            Align2::CENTER_BOTTOM,
            Vec2::new(0.0, -(t.space.xxl * 3.0 + t.space.lg)),
        )
        .interactable(true)
        .show(ctx, |ui| {
            ui.set_opacity(fade);
            let frame = Frame {
                inner_margin: Margin::symmetric(t.space.lg as i8, t.space.md as i8),
                fill: t.color.surface,
                stroke: Stroke::new(1.0_f32, edge(&t, kind)),
                corner_radius: CornerRadius::same(t.radius.card),
                shadow: t.shadow.popup,
                ..Frame::default()
            };
            let card = frame.show(ui, |ui| {
                ui.set_max_width(520.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = t.space.md;
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::splat(t.text.body + 2.0), Sense::hover());
                    icon(ui, rect, glyph, accent, 1.8);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(text)
                                .size(t.text.body)
                                .color(t.color.text),
                        )
                        .selectable(false),
                    );
                    if undoable
                        && button(
                            ui,
                            ButtonKind::Secondary,
                            "Undo",
                            true,
                            t.control.compact_height,
                        )
                        .clicked()
                    {
                        out.undo = true;
                    }
                });
            });
            let response = card.response.interact(Sense::click());
            out.hovered = response.hovered();
            if response.clicked() {
                out.dismissed = true;
            }
        });
    out
}

fn edge(t: &theme::Theme, kind: ToastKind) -> Color32 {
    match kind {
        ToastKind::Error => t.color.danger_line,
        ToastKind::Warning => t.color.warn_text,
        ToastKind::Success | ToastKind::Info => t.color.edge,
    }
}
