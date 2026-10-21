//! Dropdown menu rows: commands with a shortcut or hint at the right, two-line rows (Open
//! Recent), small headings, separators and submenus. Every menu body calls [`menu_setup`] first.

use super::{icon, Icon};
use crate::theme;
use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Color32, CornerRadius, FontId, Galley, InnerResponse, Pos2, Rect, Response, Sense, Stroke, Ui,
    Vec2, WidgetInfo, WidgetType,
};
use std::sync::Arc;

/// Spacing for a menu body: rows touch, and egui's own submenu buttons get the row's height
/// and padding.
pub fn menu_setup(ui: &mut Ui) {
    let t = theme::get(ui.ctx());
    let s = ui.spacing_mut();
    s.item_spacing.y = 0.0;
    s.button_padding = Vec2::new(t.space.md, 0.0);
    s.interact_size.y = t.control.compact_height;
}

/// One command row: the label at the left, a shortcut or hint (`hint`) at the right in a
/// quieter colour.
pub fn menu_row(ui: &mut Ui, label: &str, hint: Option<&str>, enabled: bool) -> Response {
    let t = theme::get(ui.ctx());
    let c = &t.color;
    let painter = ui.painter();
    let text = painter.layout_no_wrap(
        label.to_string(),
        t.regular(t.text.body),
        Color32::PLACEHOLDER,
    );
    let hint = hint.map(|h| {
        painter.layout_no_wrap(h.to_string(), t.regular(t.text.small), Color32::PLACEHOLDER)
    });
    let hint_w = hint.as_ref().map_or(0.0, |g| t.space.xl + g.size().x);
    let want = Vec2::new(
        text.size().x + hint_w + t.space.md * 2.0,
        t.control.compact_height,
    );
    let (rect, response) = allocate(ui, want, enabled, label);
    if ui.is_rect_visible(rect) {
        let hot = enabled && (response.hovered() || response.has_focus());
        paint_hover(ui, rect, hot);
        let inner = rect.shrink2(Vec2::new(t.space.md, 0.0));
        let (fg, quiet) = match (enabled, hot) {
            (false, _) => (c.text_disabled, c.text_disabled),
            (true, true) => (c.text, c.text_secondary),
            (true, false) => (c.text, c.text_tertiary),
        };
        let p = ui.painter();
        p.galley(
            Pos2::new(inner.left(), inner.center().y - text.size().y / 2.0),
            text,
            fg,
        );
        if let Some(g) = hint {
            p.galley(
                Pos2::new(
                    inner.right() - g.size().x,
                    inner.center().y - g.size().y / 2.0,
                ),
                g,
                quiet,
            );
        }
    }
    response
}

/// A taller row: a name over a quiet detail line (a project and its folder). `tag` is a short
/// warning at the right; `dim` greys the name (a missing file).
pub fn menu_row_detail(
    ui: &mut Ui,
    name: &str,
    detail: &str,
    tag: Option<&str>,
    dim: bool,
) -> Response {
    let t = theme::get(ui.ctx());
    let c = &t.color;
    let painter = ui.painter();
    let top = painter.layout_no_wrap(
        name.to_string(),
        t.regular(t.text.body),
        Color32::PLACEHOLDER,
    );
    let tag = tag.map(|s| {
        painter.layout_no_wrap(s.to_string(), t.regular(t.text.small), Color32::PLACEHOLDER)
    });
    let tag_w = tag.as_ref().map_or(0.0, |g| t.space.xl + g.size().x);
    // A long folder may widen the menu up to a point, then it is cut short.
    let detail_w = one_line(ui, detail, t.regular(t.text.small), f32::INFINITY)
        .size()
        .x
        .min(t.control.menu_width * 1.5);
    let want = Vec2::new(
        top.size().x.max(detail_w) + tag_w + t.space.md * 2.0,
        t.control.compact_height + t.space.lg,
    );
    let (rect, response) = allocate(ui, want, true, name);
    if ui.is_rect_visible(rect) {
        let hot = response.hovered() || response.has_focus();
        paint_hover(ui, rect, hot);
        let inner = rect.shrink2(Vec2::new(t.space.md, 0.0));
        let bottom = one_line(ui, detail, t.regular(t.text.small), inner.width() - tag_w);
        let lines = top.size().y + bottom.size().y;
        let y = inner.center().y - lines / 2.0;
        let p = ui.painter();
        let name_fg = if dim { c.text_disabled } else { c.text };
        let detail_fg = if hot {
            c.text_secondary
        } else {
            c.text_tertiary
        };
        let top_h = top.size().y;
        p.galley(Pos2::new(inner.left(), y), top, name_fg);
        p.galley(Pos2::new(inner.left(), y + top_h), bottom, detail_fg);
        if let Some(g) = tag {
            p.galley(
                Pos2::new(
                    inner.right() - g.size().x,
                    inner.center().y - g.size().y / 2.0,
                ),
                g,
                c.danger_text,
            );
        }
    }
    response
}

/// A small heading over a group of rows ("Whole movie").
pub fn menu_heading(ui: &mut Ui, text: &str) {
    let t = theme::get(ui.ctx());
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        t.medium(t.text.small),
        Color32::PLACEHOLDER,
    );
    let want = Vec2::new(
        galley.size().x + t.space.md * 2.0,
        galley.size().y + t.space.sm + t.space.xs,
    );
    let (rect, response) = ui.allocate_at_least(want, Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, text));
    let pos = Pos2::new(
        rect.left() + t.space.md,
        rect.bottom() - t.space.xs - galley.size().y,
    );
    ui.painter().galley(pos, galley, t.color.text_tertiary);
}

/// A hairline between groups, with `xs` above and below.
pub fn menu_separator(ui: &mut Ui) {
    let t = theme::get(ui.ctx());
    let (rect, _) = ui.allocate_at_least(Vec2::new(0.0, t.space.xs * 2.0 + 1.0), Sense::hover());
    let y = rect.center().y.round() + 0.5;
    ui.painter()
        .hline(rect.x_range(), y, Stroke::new(1.0_f32, t.color.line));
}

/// A row that opens a submenu on hover, with a chevron at the right.
pub fn submenu<R>(
    ui: &mut Ui,
    label: &str,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<Option<R>> {
    let t = theme::get(ui.ctx());
    let shown = ui.menu_button(label, |ui| {
        menu_setup(ui);
        add_contents(ui)
    });
    // egui draws its arrow as a font glyph the interface fonts lack; cover it with the row's
    // own fill and draw the chevron icon in its place.
    let rect = shown.response.rect;
    if ui.is_rect_visible(rect) {
        let w = &ui.style().visuals.widgets;
        let hot = shown.response.hovered();
        let fill = if hot {
            w.hovered.weak_bg_fill
        } else if shown.inner.is_some() {
            w.open.weak_bg_fill
        } else {
            ui.style().visuals.window_fill
        };
        let size = t.text.body;
        let slot = Rect::from_min_size(
            Pos2::new(
                rect.right() - t.space.md - size - t.space.xs,
                rect.top() + 1.0,
            ),
            Vec2::new(size + t.space.xs, rect.height() - 2.0),
        );
        ui.painter().rect_filled(slot, CornerRadius::ZERO, fill);
        let chevron = Rect::from_center_size(
            Pos2::new(rect.right() - t.space.md - size / 2.0, rect.center().y),
            Vec2::splat(size),
        );
        let color = if hot || shown.inner.is_some() {
            t.color.text_secondary
        } else {
            t.color.text_tertiary
        };
        icon(ui, chevron, Icon::Next, color, 1.5);
    }
    shown
}

/// `text` on one line, ending in an ellipsis when it is wider than `width`.
fn one_line(ui: &Ui, text: &str, font: FontId, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_string(), font, Color32::PLACEHOLDER);
    job.wrap = TextWrapping::truncate_at_width(width.max(0.0));
    ui.painter().layout_job(job)
}

fn allocate(ui: &mut Ui, want: Vec2, enabled: bool, label: &str) -> (Rect, Response) {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_at_least(want, sense);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, label));
    (rect, response)
}

fn paint_hover(ui: &Ui, rect: Rect, hot: bool) {
    if hot {
        let t = theme::get(ui.ctx());
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(t.radius.small),
            t.color.field_hover,
        );
    }
}
