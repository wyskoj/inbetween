//! The crop tool's drawing and pointer input on a source panel, and the caption drag shared by
//! the source panels and the preview.

use super::{
    draw_background, draw_message, draw_shown, view_input, Action, CropView, Grabs, SourcePanel,
    GRAB_RADIUS,
};
use crate::crop::{self, Handle};
use crate::view::View;
use egui::{Color32, CursorIcon, PointerButton, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2};

const CROP_HANDLE: f32 = 6.0;
/// How far above the top edge the rotate handle sits, in screen points.
const ROTATE_REACH: f32 = 28.0;

/// Follow a caption drag: report how far the pointer has moved since the press, in frame
/// pixels, and let go when the button comes up.
pub(super) fn caption_drag(
    ui: &Ui,
    resp: &egui::Response,
    rect: Rect,
    view: &View,
    grabs: &mut Grabs,
    actions: &mut Vec<Action>,
) {
    if resp.dragged_by(PointerButton::Primary) {
        let origin = ui.input(|i| i.pointer.press_origin());
        if let (Some(from), Some(now)) = (origin, resp.interact_pointer_pos()) {
            let (a, b) = (view.to_frame(rect, from), view.to_frame(rect, now));
            actions.push(Action::CaptionMove {
                dx: f64::from(b.x - a.x),
                dy: f64::from(b.y - a.y),
            });
        }
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    }
    if resp.drag_stopped_by(PointerButton::Primary) || !ui.input(|i| i.pointer.primary_down()) {
        grabs.caption = false;
        actions.push(Action::CaptionEnd);
    }
}

/// An outline around an editable caption.
pub(super) fn draw_caption_frame(
    painter: &egui::Painter,
    view: &View,
    rect: Rect,
    frame_rect: Rect,
) {
    let a = view.to_screen(rect, frame_rect.min);
    let b = view.to_screen(rect, frame_rect.max);
    let outline = Rect::from_two_pos(a, b).expand(3.0);
    painter.rect_stroke(
        outline,
        2.0,
        Stroke::new(1.5_f32, Color32::from_rgb(255, 205, 0)),
        StrokeKind::Outside,
    );
}

/// A source panel while the crop tool is on it: the picture without its crop, the box, its
/// handles. Nothing else (dots, mesh) is shown or edited.
pub(super) fn crop_mode(
    ui: &mut Ui,
    panel: &SourcePanel<'_>,
    crop: &CropView,
    view: &mut View,
    grabs: &mut Grabs,
    rect: Rect,
    resp: &egui::Response,
) -> super::PanelOutput {
    let mut actions = Vec::new();
    view_input(
        ui,
        resp,
        rect,
        view,
        &[PointerButton::Middle, PointerButton::Secondary],
    );
    // Work from a copy of the view so a pan this frame applies to the next one consistently.
    let v = *view;
    let mut pan = Vec2::ZERO;
    // Turned-picture pixels <-> screen points.
    let inverse = crop.fit.inverse().ok();
    let to_screen = |p: (f64, f64)| -> Option<Pos2> {
        let f = inverse?.apply(inbetween_geom::Point::new(p.0, p.1));
        Some(v.to_screen(rect, Pos2::new(f.x as f32, f.y as f32)))
    };
    let to_turned = |s: Pos2| -> (f64, f64) {
        let f = v.to_frame(rect, s);
        let p = crop
            .fit
            .apply(inbetween_geom::Point::new(f64::from(f.x), f64::from(f.y)));
        (p.x, p.y)
    };
    // Screen points per turned-picture pixel, to turn a pick distance and handle offset into
    // picture pixels.
    let scale = f64::from(v.zoom()) / crop.fit.a.abs().max(1e-9);
    let reach = f64::from(GRAB_RADIUS) / scale;
    let rotate_offset = f64::from(ROTATE_REACH) / scale;

    if resp.drag_started_by(PointerButton::Primary) {
        // egui reports a drag only after the pointer has moved a little; the handle was
        // grabbed where the button went down.
        let pressed = ui.input(|i| i.pointer.press_origin());
        if let Some(p) = pressed.or_else(|| resp.interact_pointer_pos()) {
            let at = to_turned(p);
            if let Some(handle) = crop::pick(&crop.tool, at, reach, rotate_offset) {
                grabs.crop = Some(handle);
                actions.push(Action::CropBegin { handle, at });
            }
        }
    }
    if resp.dragged_by(PointerButton::Primary) {
        match (grabs.crop, resp.interact_pointer_pos()) {
            (Some(_), Some(p)) => actions.push(Action::CropMove { to: to_turned(p) }),
            (None, _) => pan += resp.drag_delta(),
            _ => {}
        }
    }
    if resp.drag_stopped_by(PointerButton::Primary) && grabs.crop.take().is_some() {
        actions.push(Action::CropEnd);
    }
    if resp.hovered() {
        let icon = resp
            .hover_pos()
            .or(resp.interact_pointer_pos())
            .and_then(|p| crop::pick(&crop.tool, to_turned(p), reach, rotate_offset))
            .map_or(CursorIcon::Default, |h| match h {
                Handle::Move => CursorIcon::Move,
                Handle::Rotate => CursorIcon::Grab,
                Handle::Edge(0, _) => CursorIcon::ResizeVertical,
                Handle::Edge(_, 0) => CursorIcon::ResizeHorizontal,
                Handle::Edge(sx, sy) if sx == sy => CursorIcon::ResizeNwSe,
                Handle::Edge(..) => CursorIcon::ResizeNeSw,
            });
        ui.ctx().set_cursor_icon(icon);
    }

    // --- drawing ---
    let (painter, frame_rect) = draw_background(ui, rect, &v, panel.frame);
    match &panel.shown {
        Some(shown) => draw_shown(&painter, &v, rect, shown),
        None => draw_message(
            &painter,
            rect,
            panel.message.as_deref().unwrap_or("No image"),
        ),
    }
    painter.rect_stroke(
        frame_rect,
        0.0,
        Stroke::new(1.0_f32, Color32::from_gray(110)),
        StrokeKind::Outside,
    );
    let corners: Vec<Pos2> = crop
        .tool
        .corners()
        .iter()
        .filter_map(|c| to_screen(*c))
        .collect();
    if corners.len() == 4 {
        let dark = Stroke::new(3.0_f32, Color32::from_black_alpha(160));
        let light = Stroke::new(1.5_f32, Color32::WHITE);
        let thirds = Stroke::new(1.0_f32, Color32::from_white_alpha(70));
        for i in 1..3 {
            let t = i as f32 / 3.0;
            let lerp = |a: Pos2, b: Pos2| a + (b - a) * t;
            painter.line_segment(
                [lerp(corners[0], corners[1]), lerp(corners[3], corners[2])],
                thirds,
            );
            painter.line_segment(
                [lerp(corners[0], corners[3]), lerp(corners[1], corners[2])],
                thirds,
            );
        }
        for stroke in [dark, light] {
            for i in 0..4 {
                painter.line_segment([corners[i], corners[(i + 1) % 4]], stroke);
            }
        }
        for (sx, sy) in crop::EDGE_HANDLES {
            if let Some(p) = to_screen(crop.tool.edge_point(f64::from(sx), f64::from(sy))) {
                let r = Rect::from_center_size(p, Vec2::splat(CROP_HANDLE * 2.0));
                painter.rect_filled(r, 1.0, Color32::WHITE);
                painter.rect_stroke(
                    r,
                    1.0,
                    Stroke::new(1.0_f32, Color32::BLACK),
                    StrokeKind::Outside,
                );
            }
        }
        if let (Some(top), Some(knob)) = (
            to_screen(crop.tool.edge_point(0.0, -1.0)),
            to_screen(crop.tool.rotate_point(rotate_offset)),
        ) {
            painter.line_segment([top, knob], Stroke::new(1.0_f32, Color32::WHITE));
            painter.circle(
                knob,
                CROP_HANDLE,
                Color32::from_rgb(255, 205, 0),
                Stroke::new(1.0_f32, Color32::BLACK),
            );
        }
    }
    view.pan(pan);
    super::PanelOutput { rect, actions }
}
