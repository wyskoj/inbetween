//! The three canvases: two source panels where dots are placed and moved, and the preview.
//!
//! Panels only draw and read input. What the person did comes back as [`Action`]s, which the
//! app turns into engine commands, so nothing here edits the project.

use crate::hit;
use crate::view::View;
use egui::{
    Align2, Color32, CursorIcon, FontId, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind,
    TextureId, Ui, Vec2,
};

/// How close, in screen points, the pointer must be to grab a dot.
pub const GRAB_RADIUS: f32 = 9.0;
const DOT_RADIUS: f32 = 5.0;

/// What the person did on a canvas, in frame coordinates (output pixels).
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Click on empty image: add a dot pair here.
    Add { x: f64, y: f64 },
    /// Click on a dot, or `None` to clear the selection.
    Select(Option<usize>),
    /// A drag on a dot began.
    BeginDrag { index: usize, image: u8 },
    /// The dragged dot moved to this frame position.
    Drag {
        index: usize,
        image: u8,
        x: f64,
        y: f64,
    },
    /// The drag ended.
    EndDrag,
    /// Right-click on a dot: remove the pair.
    Delete(usize),
}

/// A dot being dragged between frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DotDrag {
    pub index: usize,
    pub image: u8,
}

/// A texture and where it sits in the frame.
#[derive(Clone, Copy, Debug)]
pub struct Shown {
    pub id: TextureId,
    /// The part of the frame the texture covers, in frame coordinates.
    pub frame_rect: Rect,
    /// The part of the texture shown there, 0 to 1.
    pub uv: Rect,
}

/// Everything a source panel draws.
pub struct SourcePanel<'a> {
    /// 1 or 2.
    pub image: u8,
    pub title: String,
    /// Output size in pixels.
    pub frame: Vec2,
    pub shown: Option<Shown>,
    /// Dot positions on this image, frame coordinates, in dot order.
    pub dots: &'a [Pos2],
    /// Mesh triangles on this image, frame coordinates.
    pub mesh: &'a [[Pos2; 3]],
    pub selected: Option<usize>,
    pub show_mesh: bool,
    pub show_numbers: bool,
    /// Shown in the middle when there is nothing to draw.
    pub message: Option<String>,
}

pub struct PanelOutput {
    /// The canvas rectangle on screen.
    pub rect: Rect,
    pub actions: Vec<Action>,
}

const BACKDROP: Color32 = Color32::from_gray(24);
const FRAME_BG: Color32 = Color32::from_gray(40);

fn dot_color(selected: bool, hovered: bool) -> Color32 {
    if selected {
        Color32::from_rgb(255, 205, 0)
    } else if hovered {
        Color32::WHITE
    } else {
        Color32::from_rgb(40, 200, 255)
    }
}

/// Allocate a canvas that fills the remaining space under a one-line header.
fn canvas(ui: &mut Ui, title: &str) -> (Rect, egui::Response) {
    ui.label(egui::RichText::new(title).strong());
    let size = ui.available_size().max(Vec2::splat(20.0));
    ui.allocate_exact_size(size, Sense::click_and_drag())
}

/// Wheel and pinch zoom about the cursor, and drag-to-pan with the given buttons.
fn view_input(
    ui: &Ui,
    resp: &egui::Response,
    rect: Rect,
    view: &mut View,
    pan_buttons: &[PointerButton],
) {
    if resp.hovered() {
        let (scroll, pinch, cursor) = ui.input(|i| {
            (
                i.smooth_scroll_delta.y,
                i.zoom_delta(),
                i.pointer.hover_pos(),
            )
        });
        let factor = pinch * (scroll * 0.0015).exp();
        if (factor - 1.0).abs() > 1e-6 {
            view.zoom_at(rect, cursor.unwrap_or_else(|| rect.center()), factor);
        }
    }
    if pan_buttons.iter().any(|b| resp.dragged_by(*b)) {
        view.pan(resp.drag_delta());
    }
}

fn draw_background(ui: &Ui, rect: Rect, view: &View, frame: Vec2) -> (egui::Painter, Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, BACKDROP);
    let frame_rect = view.frame_rect(rect, frame);
    painter.rect_filled(frame_rect, 0.0, FRAME_BG);
    (painter, frame_rect)
}

fn draw_shown(painter: &egui::Painter, view: &View, rect: Rect, shown: &Shown) {
    let on_screen = Rect::from_min_max(
        view.to_screen(rect, shown.frame_rect.min),
        view.to_screen(rect, shown.frame_rect.max),
    );
    painter.image(shown.id, on_screen, shown.uv, Color32::WHITE);
}

fn draw_mesh(painter: &egui::Painter, view: &View, rect: Rect, mesh: &[[Pos2; 3]]) {
    let stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 110));
    for tri in mesh {
        let p: Vec<Pos2> = tri.iter().map(|q| view.to_screen(rect, *q)).collect();
        if p.iter().all(|q| q.x.is_finite() && q.y.is_finite()) {
            painter.line_segment([p[0], p[1]], stroke);
            painter.line_segment([p[1], p[2]], stroke);
            painter.line_segment([p[2], p[0]], stroke);
        }
    }
}

fn draw_message(painter: &egui::Painter, rect: Rect, text: &str) {
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(15.0),
        Color32::from_gray(170),
    );
}

/// Draw a source panel and report what the person did in it.
pub fn source_panel(
    ui: &mut Ui,
    panel: &SourcePanel<'_>,
    view: &mut View,
    drag: &mut Option<DotDrag>,
) -> PanelOutput {
    let (rect, resp) = canvas(ui, &panel.title);
    if view.auto_fit {
        *view = View::fit(panel.frame, rect);
    }
    let mut actions = Vec::new();

    let screen_dots: Vec<Pos2> = panel
        .dots
        .iter()
        .map(|d| view.to_screen(rect, *d))
        .collect();
    let pointer = resp.interact_pointer_pos().or_else(|| resp.hover_pos());
    let over = pointer.and_then(|p| hit::pick(&screen_dots, p, GRAB_RADIUS));

    // --- input ---
    view_input(
        ui,
        &resp,
        rect,
        view,
        &[PointerButton::Middle, PointerButton::Secondary],
    );
    if resp.drag_started_by(PointerButton::Primary) {
        if let Some(index) = resp
            .interact_pointer_pos()
            .and_then(|p| hit::pick(&screen_dots, p, GRAB_RADIUS))
        {
            *drag = Some(DotDrag {
                index,
                image: panel.image,
            });
            actions.push(Action::Select(Some(index)));
            actions.push(Action::BeginDrag {
                index,
                image: panel.image,
            });
        }
    }
    if resp.dragged_by(PointerButton::Primary) {
        match (*drag, resp.interact_pointer_pos()) {
            (Some(d), Some(p)) if d.image == panel.image => {
                let f = view.to_frame(rect, p);
                actions.push(Action::Drag {
                    index: d.index,
                    image: d.image,
                    x: f64::from(f.x),
                    y: f64::from(f.y),
                });
            }
            (None, _) => view.pan(resp.drag_delta()),
            _ => {}
        }
    }
    if resp.drag_stopped_by(PointerButton::Primary) && drag.take().is_some() {
        actions.push(Action::EndDrag);
    }
    if resp.clicked_by(PointerButton::Primary) {
        if let Some(p) = resp.interact_pointer_pos() {
            match hit::pick(&screen_dots, p, GRAB_RADIUS) {
                Some(i) => actions.push(Action::Select(Some(i))),
                None => {
                    let f = view.to_frame(rect, p);
                    if Rect::from_min_size(Pos2::ZERO, panel.frame).contains(f) {
                        actions.push(Action::Add {
                            x: f64::from(f.x),
                            y: f64::from(f.y),
                        });
                    } else {
                        actions.push(Action::Select(None));
                    }
                }
            }
        }
    }
    if resp.secondary_clicked() {
        if let Some(i) = resp
            .interact_pointer_pos()
            .and_then(|p| hit::pick(&screen_dots, p, GRAB_RADIUS))
        {
            actions.push(Action::Delete(i));
        }
    }
    if resp.hovered() {
        let icon = if drag.is_some() {
            CursorIcon::Grabbing
        } else if over.is_some() {
            CursorIcon::Grab
        } else {
            CursorIcon::Crosshair
        };
        ui.ctx().set_cursor_icon(icon);
    }

    // --- drawing ---
    let (painter, frame_rect) = draw_background(ui, rect, view, panel.frame);
    match &panel.shown {
        Some(shown) => draw_shown(&painter, view, rect, shown),
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
    if panel.show_mesh {
        draw_mesh(&painter, view, rect, panel.mesh);
    }
    let hovered = over.filter(|_| drag.is_none());
    for (i, p) in screen_dots.iter().enumerate() {
        let selected = panel.selected == Some(i);
        let r = if selected || hovered == Some(i) {
            DOT_RADIUS + 1.5
        } else {
            DOT_RADIUS
        };
        painter.circle(*p, r + 1.0, Color32::from_black_alpha(190), Stroke::NONE);
        painter.circle(*p, r, dot_color(selected, hovered == Some(i)), Stroke::NONE);
        if panel.show_numbers {
            let at = *p + Vec2::new(r + 3.0, -r - 3.0);
            let font = FontId::proportional(12.0);
            painter.text(
                at + Vec2::splat(1.0),
                Align2::LEFT_BOTTOM,
                (i + 1).to_string(),
                font.clone(),
                Color32::BLACK,
            );
            painter.text(
                at,
                Align2::LEFT_BOTTOM,
                (i + 1).to_string(),
                font,
                Color32::WHITE,
            );
        }
    }
    PanelOutput { rect, actions }
}

/// Everything the preview draws.
pub struct PreviewPanel<'a> {
    pub title: String,
    pub frame: Vec2,
    pub shown: Option<Shown>,
    /// Mesh triangles at the current frame, frame coordinates.
    pub mesh: &'a [[Pos2; 3]],
    pub show_mesh: bool,
    pub message: Option<String>,
}

/// Draw the preview. It is view-only: any drag pans and the wheel zooms.
pub fn preview_panel(ui: &mut Ui, panel: &PreviewPanel<'_>, view: &mut View) -> Rect {
    let (rect, resp) = canvas(ui, &panel.title);
    if view.auto_fit {
        *view = View::fit(panel.frame, rect);
    }
    view_input(
        ui,
        &resp,
        rect,
        view,
        &[
            PointerButton::Primary,
            PointerButton::Middle,
            PointerButton::Secondary,
        ],
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::Default);
    }
    let (painter, frame_rect) = draw_background(ui, rect, view, panel.frame);
    match &panel.shown {
        Some(shown) => draw_shown(&painter, view, rect, shown),
        None => draw_message(
            &painter,
            rect,
            panel.message.as_deref().unwrap_or("No preview"),
        ),
    }
    painter.rect_stroke(
        frame_rect,
        0.0,
        Stroke::new(1.0_f32, Color32::from_gray(110)),
        StrokeKind::Outside,
    );
    if panel.show_mesh {
        draw_mesh(&painter, view, rect, panel.mesh);
    }
    if let Some(message) = panel.message.as_deref().filter(|_| panel.shown.is_some()) {
        // A problem with the current frame: keep the last good image and say so on top of it.
        let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 22.0));
        painter.rect_filled(bar, 0.0, Color32::from_rgba_unmultiplied(120, 20, 20, 220));
        painter.text(
            bar.left_center() + Vec2::new(8.0, 0.0),
            Align2::LEFT_CENTER,
            message,
            FontId::proportional(13.0),
            Color32::WHITE,
        );
    }
    rect
}
