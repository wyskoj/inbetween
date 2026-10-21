//! Shared pieces for redesigned windows: the modal dialog frame, buttons, field labels and text
//! fields, drawn from the [`crate::theme`] tokens. A window built from these follows the design
//! system without restating any colour or size.

mod menu;
pub use menu::*;

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
    icon_text_button(ui, kind, None, text, enabled, height)
}

/// A button from the tokens with an optional icon before its text.
pub fn icon_text_button(
    ui: &mut Ui,
    kind: ButtonKind,
    glyph: Option<Icon>,
    text: &str,
    enabled: bool,
    height: f32,
) -> Response {
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
    let icon_w = if glyph.is_some() { 12.0 + 6.0 } else { 0.0 };
    let want = Vec2::new(galley.size().x + icon_w + pad * 2.0, height);
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
        let left = rect.center().x - (galley.size().x + icon_w) / 2.0;
        if let Some(which) = glyph {
            let r = Rect::from_min_size(Pos2::new(left, rect.center().y - 6.0), Vec2::splat(12.0));
            icon(ui, r, which, fg, 2.0);
        }
        let pos = Pos2::new(left + icon_w, rect.center().y - galley.size().y / 2.0);
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
    /// Two opposed arrows (also ping-pong playback).
    Swap,
    Close,
    /// An exclamation in a circle.
    Alert,
    /// The Edit tool: a pointer.
    Cursor,
    /// The Add tool: a dot with ticks.
    AddDot,
    /// The Delete tool: a circle with a minus.
    RemoveDot,
    /// The Line tool: a joined path.
    Polyline,
    /// Show dots.
    Dots,
    /// Show lines.
    Lines,
    /// Show triangles.
    Triangles,
    LayoutFull,
    LayoutImages,
    LayoutPreview,
    LayoutFrames,
    First,
    Previous,
    Play,
    Pause,
    Next,
    Last,
    Loop,
    Eye,
    EyeOff,
    Plus,
    /// Six dots: a drag handle.
    Grip,
    ArrowUp,
    ArrowDown,
    /// A hooked arrow back to the left: a chain that loops to its start.
    Return,
    /// A circle becoming a square: the start screen.
    Morph,
}

/// Draw an icon inside `rect` (drawn on a 24-unit grid), with strokes `width` wide.
pub fn icon(ui: &Ui, rect: Rect, which: Icon, color: Color32, width: f32) {
    let p = ui.painter();
    let s = rect.width().min(rect.height()) / 24.0;
    let o = rect.center() - Vec2::splat(12.0 * s);
    let at = |x: f32, y: f32| o + Vec2::new(x, y) * s;
    let stroke = Stroke::new(width, color);
    let line =
        |pts: &[(f32, f32)]| Shape::line(pts.iter().map(|&(x, y)| at(x, y)).collect(), stroke);
    let closed = |pts: &[(f32, f32)]| {
        Shape::closed_line(pts.iter().map(|&(x, y)| at(x, y)).collect(), stroke)
    };
    let fill = |pts: &[(f32, f32)]| {
        Shape::convex_polygon(
            pts.iter().map(|&(x, y)| at(x, y)).collect(),
            color,
            Stroke::NONE,
        )
    };
    let frame = |x0: f32, y0: f32, x1: f32, y1: f32| {
        p.rect_stroke(
            Rect::from_min_max(at(x0, y0), at(x1, y1)),
            CornerRadius::same((2.0 * s) as u8),
            stroke,
            StrokeKind::Middle,
        );
    };
    let eye = || {
        let mut pts = Vec::with_capacity(34);
        for i in 0..=16 {
            let t = i as f32 / 16.0;
            pts.push((
                2.0 + 20.0 * t,
                12.0 - 7.0 * (t * std::f32::consts::PI).sin(),
            ));
        }
        for i in 1..16 {
            let t = 1.0 - i as f32 / 16.0;
            pts.push((
                2.0 + 20.0 * t,
                12.0 + 7.0 * (t * std::f32::consts::PI).sin(),
            ));
        }
        p.add(closed(&pts));
        p.circle_stroke(at(12.0, 12.0), 3.0 * s, stroke);
    };
    match which {
        Icon::Cursor => {
            p.add(closed(&[
                (5.0, 3.0),
                (19.0, 11.0),
                (13.0, 13.0),
                (10.0, 19.0),
            ]));
        }
        Icon::AddDot => {
            p.circle_stroke(at(12.0, 12.0), 3.0 * s, stroke);
            for seg in [
                [(12.0, 2.0), (12.0, 6.0)],
                [(12.0, 18.0), (12.0, 22.0)],
                [(2.0, 12.0), (6.0, 12.0)],
                [(18.0, 12.0), (22.0, 12.0)],
            ] {
                p.add(line(&seg));
            }
        }
        Icon::RemoveDot => {
            p.circle_stroke(at(12.0, 12.0), 8.0 * s, stroke);
            p.add(line(&[(8.0, 12.0), (16.0, 12.0)]));
        }
        Icon::Polyline => {
            p.add(line(&[
                (4.0, 18.0),
                (10.0, 10.0),
                (14.0, 15.0),
                (20.0, 6.0),
            ]));
            p.circle_filled(at(4.0, 18.0), 1.8 * s, color);
            p.circle_filled(at(20.0, 6.0), 1.8 * s, color);
        }
        Icon::Dots => {
            for (x, y) in [(6.0, 7.0), (17.0, 9.0), (10.0, 17.0)] {
                p.circle_filled(at(x, y), 2.4 * s, color);
            }
        }
        Icon::Lines => {
            p.add(line(&[(4.0, 17.0), (10.0, 8.0), (15.0, 14.0), (20.0, 6.0)]));
        }
        Icon::Triangles => {
            p.add(closed(&[(3.0, 20.0), (12.0, 4.0), (21.0, 20.0)]));
            p.add(line(&[(12.0, 4.0), (12.0, 20.0)]));
            p.add(line(&[(7.5, 12.0), (16.5, 12.0)]));
        }
        Icon::LayoutFull => {
            frame(3.0, 3.0, 21.0, 21.0);
            p.add(line(&[(3.0, 11.0), (21.0, 11.0)]));
            p.add(line(&[(12.0, 3.0), (12.0, 11.0)]));
        }
        Icon::LayoutImages => {
            frame(3.0, 3.0, 21.0, 21.0);
            p.add(line(&[(12.0, 3.0), (12.0, 21.0)]));
        }
        Icon::LayoutPreview => frame(3.0, 3.0, 21.0, 21.0),
        Icon::LayoutFrames => {
            for (x, y) in [(3.0, 3.0), (14.0, 3.0), (3.0, 14.0), (14.0, 14.0)] {
                frame(x, y, x + 7.0, y + 7.0);
            }
        }
        Icon::First => {
            p.rect_filled(Rect::from_min_max(at(5.0, 5.0), at(7.4, 19.0)), 1.0, color);
            p.add(fill(&[(19.0, 5.5), (19.0, 18.5), (9.0, 12.0)]));
        }
        Icon::Last => {
            p.rect_filled(
                Rect::from_min_max(at(16.6, 5.0), at(19.0, 19.0)),
                1.0,
                color,
            );
            p.add(fill(&[(5.0, 5.5), (15.0, 12.0), (5.0, 18.5)]));
        }
        Icon::Previous => {
            p.add(line(&[(15.0, 5.0), (8.0, 12.0), (15.0, 19.0)]));
        }
        Icon::Next => {
            p.add(line(&[(9.0, 5.0), (16.0, 12.0), (9.0, 19.0)]));
        }
        Icon::Play => {
            p.add(fill(&[(7.0, 4.5), (20.0, 12.0), (7.0, 19.5)]));
        }
        Icon::Pause => {
            p.rect_filled(Rect::from_min_max(at(6.0, 5.0), at(10.0, 19.0)), 1.0, color);
            p.rect_filled(
                Rect::from_min_max(at(14.0, 5.0), at(18.0, 19.0)),
                1.0,
                color,
            );
        }
        Icon::Loop => {
            p.add(line(&[(17.0, 2.0), (20.0, 5.0), (17.0, 8.0)]));
            p.add(line(&[(4.0, 11.0), (4.0, 9.0), (8.0, 5.0), (20.0, 5.0)]));
            p.add(line(&[(7.0, 22.0), (4.0, 19.0), (7.0, 16.0)]));
            p.add(line(&[
                (20.0, 13.0),
                (20.0, 15.0),
                (16.0, 19.0),
                (4.0, 19.0),
            ]));
        }
        Icon::Eye => eye(),
        Icon::EyeOff => {
            eye();
            p.add(line(&[(3.0, 3.0), (21.0, 21.0)]));
        }
        Icon::Plus => {
            p.add(line(&[(12.0, 5.0), (12.0, 19.0)]));
            p.add(line(&[(5.0, 12.0), (19.0, 12.0)]));
        }
        Icon::Grip => {
            for (x, y) in [
                (9.0, 6.0),
                (15.0, 6.0),
                (9.0, 12.0),
                (15.0, 12.0),
                (9.0, 18.0),
                (15.0, 18.0),
            ] {
                p.circle_filled(at(x, y), 1.5 * s, color);
            }
        }
        Icon::ArrowUp => {
            p.add(line(&[(12.0, 19.0), (12.0, 5.0)]));
            p.add(line(&[(6.0, 11.0), (12.0, 5.0), (18.0, 11.0)]));
        }
        Icon::ArrowDown => {
            p.add(line(&[(12.0, 5.0), (12.0, 19.0)]));
            p.add(line(&[(6.0, 13.0), (12.0, 19.0), (18.0, 13.0)]));
        }
        Icon::Return => {
            p.add(line(&[(9.0, 14.0), (4.0, 9.0), (9.0, 4.0)]));
            let arc: Vec<(f32, f32)> = (0..=8)
                .map(|k| {
                    let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * k as f32 / 8.0;
                    (15.0 + 5.0 * a.cos(), 14.0 + 5.0 * a.sin())
                })
                .collect();
            let mut pts = vec![(4.0, 9.0), (15.0, 9.0)];
            pts.extend(arc);
            pts.push((12.0, 19.0));
            p.add(line(&pts));
        }
        Icon::Morph => {
            p.circle_stroke(at(7.0, 12.0), 4.0 * s, stroke);
            p.rect_stroke(
                Rect::from_min_max(at(13.0, 8.0), at(21.0, 16.0)),
                CornerRadius::same((2.0 * s) as u8),
                stroke,
                StrokeKind::Middle,
            );
            p.circle_filled(at(11.5, 12.0), width * 0.5, color);
        }
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

/// One part of a segmented control or toggle group.
#[derive(Clone, Copy, Debug, Default)]
pub struct Segment<'a> {
    /// Shown text; empty for an icon-only segment.
    pub label: &'a str,
    pub icon: Option<Icon>,
    /// A key shown after the label in tertiary text ("A").
    pub key: Option<&'a str>,
    /// Hover text, and the accessible name of an icon-only segment.
    pub tip: &'a str,
}

impl<'a> Segment<'a> {
    pub fn text(label: &'a str) -> Self {
        Self {
            label,
            ..Self::default()
        }
    }

    pub fn icon(icon: Icon, label: &'a str) -> Self {
        Self {
            label,
            icon: Some(icon),
            ..Self::default()
        }
    }

    /// An icon with no visible text; `tip` names it.
    pub fn icon_only(icon: Icon, tip: &'a str) -> Self {
        Self {
            icon: Some(icon),
            tip,
            ..Self::default()
        }
    }

    pub fn key(mut self, key: &'a str) -> Self {
        self.key = Some(key);
        self
    }

    pub fn tip(mut self, tip: &'a str) -> Self {
        self.tip = tip;
        self
    }
}

/// A row of segments on a recessed track; `is_on` says which are lit. Returns the index clicked.
fn segment_row(
    ui: &mut Ui,
    segments: &[Segment<'_>],
    is_on: impl Fn(usize) -> bool,
    exclusive: bool,
    enabled: bool,
    height: f32,
) -> Option<usize> {
    let t = theme::get(ui.ctx());
    let c = t.color;
    let size = if height < t.control.compact_height {
        t.text.small
    } else {
        t.text.label
    };
    let mut clicked = None;
    Frame::NONE
        .fill(c.app)
        .corner_radius(CornerRadius::same(t.radius.card))
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                // Inside a right-to-left bar the row is laid out from the right, so walk the
                // segments backwards to keep them reading left to right.
                let rtl = ui.layout().main_dir() == egui::Direction::RightToLeft;
                let order: Vec<usize> = if rtl {
                    (0..segments.len()).rev().collect()
                } else {
                    (0..segments.len()).collect()
                };
                for (i, seg) in order
                    .into_iter()
                    .filter_map(|i| Some((i, segments.get(i)?)))
                {
                    let on = is_on(i);
                    let font = t.regular(size);
                    let text = (!seg.label.is_empty()).then(|| {
                        ui.painter().layout_no_wrap(
                            seg.label.to_string(),
                            font.clone(),
                            Color32::PLACEHOLDER,
                        )
                    });
                    let key = seg.key.map(|k| {
                        ui.painter().layout_no_wrap(
                            k.to_string(),
                            t.regular(t.text.small - 1.0),
                            c.text_tertiary,
                        )
                    });
                    let icon_w = if seg.icon.is_some() { 14.0 } else { 0.0 };
                    let text_w = text.as_ref().map_or(0.0, |g| g.size().x);
                    let key_w = key.as_ref().map_or(0.0, |g| g.size().x + 6.0);
                    let gap = if seg.icon.is_some() && text.is_some() {
                        6.0
                    } else {
                        0.0
                    };
                    let pad = if text.is_some() { 10.0 } else { 7.0 };
                    let width = (icon_w + gap + text_w + key_w + pad * 2.0).max(height);
                    let sense = if enabled {
                        Sense::click()
                    } else {
                        Sense::hover()
                    };
                    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), sense);
                    let name = if seg.label.is_empty() {
                        seg.tip
                    } else {
                        seg.label
                    };
                    response.widget_info(|| {
                        let kind = if exclusive {
                            egui::WidgetType::RadioButton
                        } else {
                            egui::WidgetType::Checkbox
                        };
                        egui::WidgetInfo::selected(kind, enabled, on, name)
                    });
                    let hot = enabled && response.hovered();
                    if on {
                        ui.painter().rect_filled(
                            rect,
                            CornerRadius::same(t.radius.control),
                            c.field_hover,
                        );
                    } else if hot {
                        ui.painter().rect_filled(
                            rect,
                            CornerRadius::same(t.radius.control),
                            c.field,
                        );
                    }
                    let fg = if !enabled {
                        c.text_disabled
                    } else if on || hot {
                        c.text
                    } else {
                        c.text_secondary
                    };
                    let mut x = rect.left() + pad;
                    if text.is_none() && key.is_none() {
                        x = rect.center().x - icon_w / 2.0;
                    }
                    if let Some(which) = seg.icon {
                        let r = Rect::from_min_size(
                            Pos2::new(x, rect.center().y - 7.0),
                            Vec2::splat(14.0),
                        );
                        icon(ui, r, which, fg, 1.5);
                        x += icon_w + gap;
                    }
                    if let Some(g) = text {
                        let y = rect.center().y - g.size().y / 2.0;
                        let w = g.size().x;
                        ui.painter().galley(Pos2::new(x, y), g, fg);
                        x += w + 6.0;
                    }
                    if let Some(g) = key {
                        let y = rect.center().y - g.size().y / 2.0;
                        ui.painter().galley(Pos2::new(x, y), g, c.text_tertiary);
                    }
                    let response = if seg.tip.is_empty() {
                        response
                    } else {
                        response.on_hover_text(seg.tip)
                    };
                    if response.clicked() {
                        clicked = Some(i);
                    }
                }
            });
        });
    clicked
}

/// A segmented control: one choice of several. Returns the segment clicked.
pub fn segmented(
    ui: &mut Ui,
    segments: &[Segment<'_>],
    selected: usize,
    enabled: bool,
    height: f32,
) -> Option<usize> {
    segment_row(ui, segments, |i| i == selected, true, enabled, height)
}

/// Independent on/off toggles drawn as one group. Returns the toggle clicked.
pub fn toggle_group(
    ui: &mut Ui,
    segments: &[Segment<'_>],
    on: &[bool],
    height: f32,
) -> Option<usize> {
    segment_row(
        ui,
        segments,
        |i| on.get(i).copied().unwrap_or(false),
        false,
        true,
        height,
    )
}

/// A square icon button. `on` marks a toggle that is active (its icon in the focus colour).
pub fn icon_button(
    ui: &mut Ui,
    which: Icon,
    label: &str,
    on: bool,
    enabled: bool,
    size: f32,
) -> Response {
    let t = theme::get(ui.ctx());
    let c = t.color;
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), sense);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, label));
    let hot = enabled && (response.hovered() || response.has_focus());
    let fill = if on || hot {
        c.field
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(t.radius.control), fill);
    let fg = if !enabled {
        c.text_disabled
    } else if on {
        c.focus
    } else if hot {
        c.text
    } else {
        c.text_secondary
    };
    let glyph = (size * 0.5).round();
    icon(
        ui,
        Rect::from_center_size(rect.center(), Vec2::splat(glyph)),
        which,
        fg,
        1.8,
    );
    response.on_hover_text(label)
}

/// A small text toggle the size of an icon button ("%").
pub fn text_toggle(ui: &mut Ui, text: &str, label: &str, on: bool, size: f32) -> Response {
    let t = theme::get(ui.ctx());
    let c = t.color;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, label));
    let hot = response.hovered();
    let fill = if on || hot {
        c.field
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(t.radius.control), fill);
    let fg = if on {
        c.focus
    } else if hot {
        c.text
    } else {
        c.text_secondary
    };
    centred_text(ui, rect.center(), text, t.medium(t.text.label), fg);
    response.on_hover_text(label)
}

/// A round filled button with an icon: Play.
pub fn round_button(
    ui: &mut Ui,
    which: Icon,
    label: &str,
    enabled: bool,
    diameter: f32,
) -> Response {
    let t = theme::get(ui.ctx());
    let c = t.color;
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(diameter), sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    let (fill, fg) = if !enabled {
        (c.disabled, c.text_disabled)
    } else if response.hovered() {
        (c.accent_hover, c.on_accent)
    } else {
        (c.accent, c.on_accent)
    };
    if enabled {
        ui.painter().add(
            t.shadow
                .control
                .as_shape(rect, CornerRadius::same((diameter / 2.0) as u8)),
        );
    }
    ui.painter()
        .circle_filled(rect.center(), diameter / 2.0, fill);
    icon(
        ui,
        Rect::from_center_size(
            rect.center() + Vec2::new(diameter * 0.03, 0.0),
            Vec2::splat(diameter * 0.4),
        ),
        which,
        fg,
        1.8,
    );
    response.on_hover_text(label)
}

/// An on/off switch with a label, as a pill that lights up when on. Returns the response; the
/// caller flips its value when it is clicked.
pub fn switch(ui: &mut Ui, label: &str, on: bool, height: f32) -> Response {
    let t = theme::get(ui.ctx());
    let c = t.color;
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        t.regular(t.text.label),
        Color32::PLACEHOLDER,
    );
    let track = Vec2::new(26.0, 16.0);
    let width = 6.0 + track.x + 8.0 + galley.size().x + 10.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, label));
    let hot = response.hovered();
    let fill = if on {
        c.field_hover
    } else if hot {
        c.field
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(t.radius.control), fill);
    let track_rect = Rect::from_min_size(
        Pos2::new(rect.left() + 6.0, rect.center().y - track.y / 2.0),
        track,
    );
    let (track_fill, knob, knob_x) = if on {
        (c.accent, c.accent_text, track_rect.right() - 8.0)
    } else {
        (c.line, c.text_tertiary, track_rect.left() + 8.0)
    };
    ui.painter()
        .rect_filled(track_rect, CornerRadius::same(8), track_fill);
    ui.painter()
        .circle_filled(Pos2::new(knob_x, track_rect.center().y), 6.0, knob);
    let fg = if on || hot { c.text } else { c.text_secondary };
    let pos = Pos2::new(
        track_rect.right() + 8.0,
        rect.center().y - galley.size().y / 2.0,
    );
    ui.painter().galley(pos, galley, fg);
    response
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
