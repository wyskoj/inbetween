//! Shared pieces for redesigned windows: the modal dialog frame, buttons, field labels and text
//! fields, drawn from the [`crate::theme`] tokens. A window built from these follows the design
//! system without restating any colour or size.

use crate::theme::{self, Theme};
use egui::{
    Align2, Color32, Context, CornerRadius, Frame, Margin, Pos2, Rect, Response, RichText, Sense,
    Shape, Stroke, StrokeKind, Ui, Vec2,
};

/// A centred modal window: a title and one-line subtitle, a body, and a footer band. The
/// backdrop dims the app and blocks it. Returns the body's and footer's results and whether
/// the person pressed Escape.
pub fn dialog<B, F>(
    ctx: &Context,
    id: &str,
    title: &str,
    subtitle: &str,
    width: f32,
    body: impl FnOnce(&mut Ui, &Theme) -> B,
    footer: impl FnOnce(&mut Ui, &Theme) -> F,
) -> (B, F, bool) {
    let t = theme::get(ctx);
    let frame = Frame {
        inner_margin: Margin::ZERO,
        fill: t.color.surface,
        stroke: Stroke::new(1.0_f32, t.color.edge),
        corner_radius: CornerRadius::same(t.radius.window),
        shadow: t.shadow.window,
        ..Frame::default()
    };
    let modal = egui::Modal::new(egui::Id::new(id))
        .frame(frame)
        .backdrop_color(t.color.backdrop)
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let pad = t.space.xl as i8;
            Frame::NONE
                .inner_margin(Margin {
                    left: pad,
                    right: pad,
                    top: pad,
                    bottom: 0,
                })
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(title)
                            .font(t.medium(t.text.title))
                            .color(t.color.text),
                    );
                    ui.add_space(t.space.xs);
                    ui.label(
                        RichText::new(subtitle)
                            .size(t.text.body)
                            .line_height(Some(t.text.body + t.space.sm - 2.0))
                            .color(t.color.text_secondary),
                    );
                });
            let b = Frame::NONE
                .inner_margin(Margin::same(pad))
                .show(ui, |ui| body(ui, &t))
                .inner;
            let r = t.radius.window;
            let f = Frame::NONE
                .fill(t.color.footer)
                .corner_radius(CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: r,
                    se: r,
                })
                .inner_margin(Margin {
                    left: pad,
                    right: pad,
                    top: t.space.lg as i8,
                    bottom: t.space.lg as i8,
                })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = t.space.md;
                        footer(ui, &t)
                    })
                    .inner
                })
                .inner;
            (b, f)
        });
    // Escape closes; a click on the backdrop does not, so a stray click keeps what was picked.
    let escape = modal.is_top_modal
        && !modal.any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let (b, f) = modal.inner;
    (b, f, escape)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// The one action the window exists for: accent fill.
    Primary,
    /// Everything else in a footer (Cancel): text only, a fill on hover.
    Quiet,
    /// A small action inside a card: field fill.
    Secondary,
    /// A small action inside an error: the danger tint.
    Danger,
}

/// A button from the tokens. `height` is a control height token.
pub fn button(ui: &mut Ui, kind: ButtonKind, text: &str, enabled: bool, height: f32) -> Response {
    let t = theme::get(ui.ctx());
    let size = if height < t.control.height {
        t.text.label
    } else {
        t.text.body
    };
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_string(), t.medium(size), Color32::PLACEHOLDER);
    let pad = if height < t.control.height {
        t.space.md
    } else {
        t.space.lg
    };
    let want = Vec2::new(galley.size().x + pad * 2.0, height);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(want, sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, text));
    if ui.is_rect_visible(rect) {
        let c = &t.color;
        let hot = response.hovered() || response.has_focus();
        let down = response.is_pointer_button_down_on();
        let (fill, fg) = match (kind, enabled) {
            (_, false) if kind == ButtonKind::Quiet => (Color32::TRANSPARENT, c.text_disabled),
            (_, false) => (c.disabled, c.text_disabled),
            (ButtonKind::Primary, true) if down => (c.accent, c.on_accent),
            (ButtonKind::Primary, true) if hot => (c.accent_hover, c.on_accent),
            (ButtonKind::Primary, true) => (c.accent, c.on_accent),
            (ButtonKind::Quiet, true) if down => (c.field_active, c.text),
            (ButtonKind::Quiet, true) if hot => (c.field, c.text),
            (ButtonKind::Quiet, true) => (Color32::TRANSPARENT, c.text_secondary),
            (ButtonKind::Secondary, true) if down => (c.field_active, c.text),
            (ButtonKind::Secondary, true) if hot => (c.field_hover, c.text),
            (ButtonKind::Secondary, true) => (c.field, c.text),
            (ButtonKind::Danger, true) if hot || down => (c.danger_line, c.danger_text_strong),
            (ButtonKind::Danger, true) => (c.danger_soft, c.danger_text_strong),
        };
        let radius = CornerRadius::same(t.radius.control);
        if kind == ButtonKind::Primary && enabled {
            // Raised: a small shadow below, from the theme.
            ui.painter().add(t.shadow.control.as_shape(rect, radius));
        }
        ui.painter().rect_filled(rect, radius, fill);
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect.expand(2.0),
                CornerRadius::same(t.radius.control.saturating_add(2)),
                Stroke::new(1.5_f32, c.focus),
                StrokeKind::Outside,
            );
        }
        let pos = rect.center() - galley.size() / 2.0;
        ui.painter().galley(pos, galley, fg);
    }
    response
}

/// A field's label row: the name in medium secondary text, and an aside (such as "Optional")
/// at the right in tertiary text.
pub fn field_label(ui: &mut Ui, label: &str, aside: Option<&str>) {
    let t = theme::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .text_style(theme::label_style())
                .color(t.color.text_secondary),
        );
        if let Some(aside) = aside {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(aside)
                        .size(t.text.label)
                        .color(t.color.text_tertiary),
                );
            });
        }
    });
}

/// A one-line text field at the control height, filling the width.
pub fn text_field(ui: &mut Ui, text: &mut String, hint: &str) -> Response {
    let t = theme::get(ui.ctx());
    let pad_y = ((t.control.height - t.text.body) / 2.0 - 2.0).max(0.0);
    ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(RichText::new(hint).color(t.color.text_tertiary))
            .font(t.regular(t.text.body))
            .text_color(t.color.text)
            .margin(Margin::symmetric(t.space.md as i8, pad_y as i8))
            .min_size(Vec2::new(0.0, t.control.height))
            .desired_width(f32::INFINITY),
    )
}

/// Text in one of the theme's sizes and colours, cut off with "…" when too long.
pub fn text_line(ui: &mut Ui, text: &str, size: f32, color: Color32) -> Response {
    ui.add(
        egui::Label::new(RichText::new(text).size(size).color(color))
            .truncate()
            .selectable(false),
    )
}

/// The outline of a rounded rectangle as a closed list of points.
pub fn rounded_outline(rect: Rect, radius: f32) -> Vec<Pos2> {
    let r = radius
        .min(rect.width() / 2.0)
        .min(rect.height() / 2.0)
        .max(0.0);
    let corners = [
        (Pos2::new(rect.right() - r, rect.top() + r), -90.0_f32),
        (Pos2::new(rect.right() - r, rect.bottom() - r), 0.0),
        (Pos2::new(rect.left() + r, rect.bottom() - r), 90.0),
        (Pos2::new(rect.left() + r, rect.top() + r), 180.0),
    ];
    let mut points = Vec::with_capacity(4 * 7 + 1);
    for (centre, start) in corners {
        for step in 0..=6 {
            let a = (start + step as f32 * 15.0).to_radians();
            points.push(centre + Vec2::new(a.cos(), a.sin()) * r);
        }
    }
    if let Some(first) = points.first().copied() {
        points.push(first);
    }
    points
}

/// A dashed rounded outline: the edge of an empty drop target.
pub fn dashed_rect(ui: &Ui, rect: Rect, radius: f32, stroke: Stroke) {
    let outline = rounded_outline(rect, radius);
    ui.painter()
        .extend(Shape::dashed_line(&outline, stroke, 6.0, 4.0));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// A framed picture.
    Image,
    /// Two opposed arrows.
    Swap,
    Close,
    /// An exclamation in a circle.
    Alert,
}

/// Draw an icon as strokes inside `rect` (drawn on a 24-unit grid).
pub fn icon(ui: &Ui, rect: Rect, which: Icon, color: Color32, width: f32) {
    let p = ui.painter();
    let s = rect.width().min(rect.height()) / 24.0;
    let o = rect.center() - Vec2::splat(12.0 * s);
    let at = |x: f32, y: f32| o + Vec2::new(x, y) * s;
    let stroke = Stroke::new(width, color);
    let line =
        |pts: &[(f32, f32)]| Shape::line(pts.iter().map(|&(x, y)| at(x, y)).collect(), stroke);
    match which {
        Icon::Image => {
            p.rect_stroke(
                Rect::from_min_max(at(3.0, 4.0), at(21.0, 20.0)),
                CornerRadius::same((2.0 * s) as u8),
                stroke,
                StrokeKind::Middle,
            );
            p.circle_stroke(at(9.0, 10.0), 2.0 * s, stroke);
            p.add(line(&[(21.0, 16.0), (16.0, 11.0), (7.0, 20.0)]));
        }
        Icon::Swap => {
            p.add(line(&[(4.0, 9.0), (18.0, 9.0)]));
            p.add(line(&[(14.0, 5.0), (18.0, 9.0), (14.0, 13.0)]));
            p.add(line(&[(20.0, 15.0), (6.0, 15.0)]));
            p.add(line(&[(10.0, 11.0), (6.0, 15.0), (10.0, 19.0)]));
        }
        Icon::Close => {
            p.add(line(&[(6.0, 6.0), (18.0, 18.0)]));
            p.add(line(&[(18.0, 6.0), (6.0, 18.0)]));
        }
        Icon::Alert => {
            p.circle_stroke(at(12.0, 12.0), 9.0 * s, stroke);
            p.add(line(&[(12.0, 7.5), (12.0, 13.0)]));
            p.circle_filled(at(12.0, 16.5), width * 0.7, color);
        }
    }
}

/// A circle with an icon in it, for empty and error states.
pub fn icon_badge(
    ui: &Ui,
    centre: Pos2,
    diameter: f32,
    fill: Color32,
    which: Icon,
    color: Color32,
) {
    ui.painter().circle_filled(centre, diameter / 2.0, fill);
    let icon_rect = Rect::from_center_size(centre, Vec2::splat(diameter / 2.0));
    icon(ui, icon_rect, which, color, 1.6);
}

/// Text centred on a point, with a font and colour; returns its rectangle.
pub fn centred_text(ui: &Ui, at: Pos2, text: &str, font: egui::FontId, color: Color32) -> Rect {
    ui.painter()
        .text(at, Align2::CENTER_CENTER, text, font, color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_outline_is_closed_and_stays_inside_the_rect() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(100.0, 60.0));
        let pts = rounded_outline(rect, 8.0);
        assert_eq!(pts.first(), pts.last());
        for p in &pts {
            assert!(rect.expand(0.01).contains(*p), "{p:?}");
            assert!(p.x.is_finite() && p.y.is_finite());
        }
    }

    #[test]
    fn rounded_outline_survives_degenerate_rects() {
        for rect in [
            Rect::from_min_size(Pos2::ZERO, Vec2::ZERO),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(4.0, 0.0)),
            Rect::NOTHING,
        ] {
            let pts = rounded_outline(rect, 8.0);
            assert!(pts.iter().all(|p| p.x.is_finite() || rect == Rect::NOTHING));
        }
    }
}
