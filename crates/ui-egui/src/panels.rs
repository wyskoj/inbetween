//! The three canvases: two source panels where dots are placed and moved, and the preview.
//!
//! Panels only draw and read input. What the person did comes back as [`Action`]s, which the
//! app turns into engine commands, so nothing here edits the project.

use crate::camera_tool::{self, Grab};
use crate::crop::{CropTool, Handle};
use crate::hit;
use crate::state::Tool;
use crate::view::View;
use egui::{
    Align2, Color32, CursorIcon, FontId, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind,
    TextureId, Ui, Vec2,
};
use inbetween_doc::CameraKey;
use inbetween_engine::DisplayPrefs;
use std::collections::BTreeSet;

mod crop_view;
pub mod overlay;
use crop_view::{caption_drag, crop_mode, draw_caption_frame};
use overlay::{dot_color, DotLook};

/// How close, in screen points, the pointer must be to grab a dot.
pub const GRAB_RADIUS: f32 = 9.0;
/// How close, in screen points, the pointer must be to grab a line.
pub const LINE_GRAB: f32 = 5.0;

/// How a selection gesture combines with the current selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelMode {
    Set,
    Add,
    Subtract,
    Toggle,
}

impl SelMode {
    pub fn id(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Add => "add",
            Self::Subtract => "subtract",
            Self::Toggle => "toggle",
        }
    }
}

/// What the person did on a canvas, in frame coordinates (output pixels).
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Click on empty image with the Add tool: add a dot here (`same`: partner at the same spot).
    Add {
        x: f64,
        y: f64,
        image: u8,
        same: bool,
    },
    /// Click on a dot.
    SelectDot {
        index: usize,
        mode: SelMode,
    },
    /// Click on a line: select its connected dots.
    SelectLine {
        index: usize,
        mode: SelMode,
    },
    /// Click on empty image with the Edit tool.
    SelectNone,
    /// A marquee finished: x0, y0, x1, y1 in frame pixels on `image`.
    Marquee {
        image: u8,
        rect: [f64; 4],
        mode: SelMode,
    },
    /// A drag on a dot began.
    BeginDrag {
        index: usize,
        image: u8,
    },
    /// The dragged dot moved to this frame position.
    Drag {
        index: usize,
        image: u8,
        x: f64,
        y: f64,
    },
    /// The drag ended.
    EndDrag,
    /// Delete tool click, or right-click on a dot: remove the dot.
    Delete(usize),
    /// Line tool click: drop a dot (or continue from `dot`) and join it to the last one.
    LineClick {
        x: f64,
        y: f64,
        image: u8,
        dot: Option<usize>,
        finish: bool,
    },
    /// Right-click, Enter or Esc with the Line tool: stop drawing the chain.
    LineEnd,
    /// Click on a line with the Add tool: put a dot on it.
    InsertOnLine {
        line: usize,
        x: f64,
        y: f64,
        image: u8,
    },
    /// A drag on a line began.
    LineDragBegin {
        line: usize,
    },
    /// The dragged line's group moves by this much on `image`.
    LineDrag {
        line: usize,
        image: u8,
        dx: f64,
        dy: f64,
    },
    LineDragEnd,
    /// The crop box was grabbed; `at` is the position in the turned picture's pixels.
    CropBegin {
        handle: Handle,
        at: (f64, f64),
    },
    /// The grabbed crop handle is now here (turned picture pixels).
    CropMove {
        to: (f64, f64),
    },
    CropEnd,
    /// A drag on the caption began.
    CaptionBegin,
    /// The caption has been dragged this far from where the drag began, in frame pixels.
    CaptionMove {
        dx: f64,
        dy: f64,
    },
    CaptionEnd,
    /// A drag on the camera frame began: what it grabbed, and where (frame pixels).
    CameraBegin {
        grab: Grab,
        at: (f64, f64),
    },
    /// The grabbed camera handle is now here (frame pixels).
    CameraMove {
        to: (f64, f64),
    },
    CameraEnd,
}

/// What the pointer has hold of besides a dot, kept between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Grabs {
    /// A crop handle (or the box itself) is being dragged.
    pub crop: Option<Handle>,
    /// The caption is being dragged.
    pub caption: bool,
    /// A camera handle (or the moved picture) is being dragged.
    pub camera: bool,
    /// A line is being dragged.
    pub line: Option<usize>,
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

/// A dot on one image: where it is, which dot it is, and whether it has a partner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelDot {
    pub pos: Pos2,
    pub index: usize,
    pub paired: bool,
}

/// A line between two dots that are both on this image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelLine {
    pub a: Pos2,
    pub b: Pos2,
    /// The line number.
    pub index: usize,
    /// The dot numbers at its two ends.
    pub a_dot: usize,
    pub b_dot: usize,
}

/// What the pointer is over in a panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hover {
    pub dot: Option<usize>,
    pub line: Option<usize>,
}

/// Everything a source panel draws.
pub struct SourcePanel<'a> {
    /// 1 or 2.
    pub image: u8,
    pub title: String,
    /// Output size in pixels.
    pub frame: Vec2,
    pub shown: Option<Shown>,
    /// The dots that are on this image, frame coordinates.
    pub dots: &'a [PanelDot],
    /// Lines between them.
    pub lines: &'a [PanelLine],
    /// Mesh triangles on this image, frame coordinates.
    pub mesh: &'a [[Pos2; 3]],
    pub selected: &'a BTreeSet<usize>,
    pub prefs: DisplayPrefs,
    pub tool: Tool,
    pub show_numbers: bool,
    /// The partner of the dot hovered on the other panel.
    pub partner_hover: Option<usize>,
    /// The dots of the line group being hovered.
    pub hot_group: &'a [usize],
    /// Where an unpaired dot hovered on the other panel would be matched, blinking.
    pub ghost: Option<Pos2>,
    /// The last dot of the Line tool's chain, to draw the rubber band from.
    pub chain_from: Option<Pos2>,
    /// Shown in the middle when there is nothing to draw.
    pub message: Option<String>,
    /// While the crop tool is on this image: the box to draw and edit.
    pub crop: Option<CropView>,
    /// While the caption window is open for this image: where the caption sits, in frame
    /// coordinates, so a drag that starts on it moves it.
    pub caption: Option<Rect>,
    /// Draw a checkerboard behind transparent areas.
    pub checker: bool,
}

/// The crop tool as a source panel draws it.
pub struct CropView {
    pub tool: CropTool,
    /// Frame coordinates to pixels of the turned picture (the picture is shown fitted in the
    /// frame, so this is the inverse of how it is placed).
    pub fit: inbetween_geom::Affine,
}

pub struct PanelOutput {
    /// The canvas rectangle on screen.
    pub rect: Rect,
    pub actions: Vec<Action>,
    pub hover: Hover,
}

const BACKDROP: Color32 = Color32::from_gray(24);
const FRAME_BG: Color32 = Color32::from_gray(40);

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

/// A 16 x 16 texture of four 8 x 8 squares, repeated to tile the checkerboard.
fn checker_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::new("inbetween_checker");
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return t;
    }
    let mut pixels = Vec::with_capacity(16 * 16);
    for y in 0..16 {
        for x in 0..16 {
            let dark = (x / 8 + y / 8) % 2 == 0;
            pixels.push(if dark {
                Color32::from_gray(58)
            } else {
                Color32::from_gray(84)
            });
        }
    }
    let image = egui::ColorImage {
        size: [16, 16],
        pixels,
    };
    let options = egui::TextureOptions {
        wrap_mode: egui::TextureWrapMode::Repeat,
        ..egui::TextureOptions::NEAREST
    };
    let handle = ctx.load_texture("checkerboard", image, options);
    ctx.data_mut(|d| d.insert_temp(id, handle.clone()));
    handle
}

fn draw_background(
    ui: &Ui,
    rect: Rect,
    view: &View,
    frame: Vec2,
    checker: bool,
) -> (egui::Painter, Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, BACKDROP);
    let frame_rect = view.frame_rect(rect, frame);
    painter.rect_filled(frame_rect, 0.0, FRAME_BG);
    let visible = frame_rect.intersect(rect);
    if checker && visible.width() > 0.0 && visible.height() > 0.0 {
        // One 16-point tile, anchored to the frame so the squares pan with the picture.
        let texture = checker_texture(ui.ctx());
        let min = (visible.min - frame_rect.min) / 16.0;
        let uv = Rect::from_min_size(min.to_pos2(), visible.size() / 16.0);
        painter.image(texture.id(), visible, uv, Color32::WHITE);
    }
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

fn mode_for_marquee(shift: bool, alt: bool) -> SelMode {
    if alt {
        SelMode::Subtract
    } else if shift {
        SelMode::Add
    } else {
        SelMode::Set
    }
}

/// Draw a source panel and report what the person did in it.
pub fn source_panel(
    ui: &mut Ui,
    panel: &SourcePanel<'_>,
    view: &mut View,
    drag: &mut Option<DotDrag>,
    grabs: &mut Grabs,
) -> PanelOutput {
    let (rect, resp) = canvas(ui, &panel.title);
    if view.auto_fit {
        *view = View::fit(panel.frame, rect);
    }
    if let Some(crop) = &panel.crop {
        return crop_mode(ui, panel, crop, view, grabs, rect, &resp);
    }
    let mut actions = Vec::new();
    let tool = panel.tool;
    let (shift, alt) = ui.input(|i| (i.modifiers.shift, i.modifiers.alt));

    let screen_dots: Vec<Pos2> = panel
        .dots
        .iter()
        .map(|d| view.to_screen(rect, d.pos))
        .collect();
    let screen_lines: Vec<(Pos2, Pos2)> = panel
        .lines
        .iter()
        .map(|l| (view.to_screen(rect, l.a), view.to_screen(rect, l.b)))
        .collect();
    let pointer = resp.interact_pointer_pos().or_else(|| resp.hover_pos());
    let over_slot = pointer.and_then(|p| hit::pick(&screen_dots, p, GRAB_RADIUS));
    let over = over_slot.and_then(|i| panel.dots.get(i)).map(|d| d.index);
    let over_line = if over.is_none() && panel.prefs.show_lines {
        pointer
            .and_then(|p| hit::pick_segment(&screen_lines, p, LINE_GRAB))
            .and_then(|i| panel.lines.get(i))
            .map(|l| l.index)
    } else {
        None
    };
    let dot_at = |p: Pos2| {
        hit::pick(&screen_dots, p, GRAB_RADIUS)
            .and_then(|i| panel.dots.get(i))
            .map(|d| d.index)
    };
    let line_at = |p: Pos2| {
        if !panel.prefs.show_lines {
            return None;
        }
        hit::pick_segment(&screen_lines, p, LINE_GRAB)
            .and_then(|i| panel.lines.get(i))
            .map(|l| l.index)
    };
    let in_frame = |f: Pos2| Rect::from_min_size(Pos2::ZERO, panel.frame).contains(f);

    // --- input ---
    view_input(
        ui,
        &resp,
        rect,
        view,
        &[PointerButton::Middle, PointerButton::Secondary],
    );
    let caption_here = |p: Pos2| {
        panel
            .caption
            .is_some_and(|r| r.contains(view.to_frame(rect, p)))
    };
    if resp.drag_started_by(PointerButton::Primary) {
        if let Some(p) = resp.interact_pointer_pos() {
            if caption_here(p) && dot_at(p).is_none() {
                grabs.caption = true;
                actions.push(Action::CaptionBegin);
            }
        }
    }
    let had_caption = grabs.caption;
    if grabs.caption {
        caption_drag(ui, &resp, rect, view, grabs, &mut actions);
    }
    if !had_caption && resp.drag_started_by(PointerButton::Primary) {
        // egui reports the drag a few pixels after the press; what was grabbed is what was
        // under the pointer when the button went down.
        let pressed_at = ui
            .input(|i| i.pointer.press_origin())
            .or_else(|| resp.interact_pointer_pos());
        if let Some(p) = pressed_at {
            if let (Some(index), true) = (dot_at(p), matches!(tool, Tool::Edit | Tool::Add)) {
                *drag = Some(DotDrag {
                    index,
                    image: panel.image,
                });
                if !panel.selected.contains(&index) {
                    actions.push(Action::SelectDot {
                        index,
                        mode: if shift { SelMode::Add } else { SelMode::Set },
                    });
                }
                actions.push(Action::BeginDrag {
                    index,
                    image: panel.image,
                });
            } else if let (Some(line), Tool::Edit) = (line_at(p), tool) {
                grabs.line = Some(line);
                actions.push(Action::SelectLine {
                    index: line,
                    mode: SelMode::Set,
                });
                actions.push(Action::LineDragBegin { line });
            }
        }
    }
    // The press position is gone by the frame the button is released, so keep it meanwhile.
    let origin_id = egui::Id::new(("marquee origin", panel.image));
    if let Some(o) = ui.input(|i| i.pointer.press_origin()) {
        ui.data_mut(|d| d.insert_temp(origin_id, o));
    }
    let marquee_from: Option<Pos2> = ui.data(|d| d.get_temp(origin_id));
    let marquee_live = tool == Tool::Edit
        && !had_caption
        && drag.is_none()
        && grabs.line.is_none()
        && resp.dragged_by(PointerButton::Primary);
    if !had_caption && resp.dragged_by(PointerButton::Primary) {
        match (*drag, grabs.line, resp.interact_pointer_pos()) {
            (Some(d), _, Some(p)) if d.image == panel.image => {
                let f = view.to_frame(rect, p);
                actions.push(Action::Drag {
                    index: d.index,
                    image: d.image,
                    x: f64::from(f.x),
                    y: f64::from(f.y),
                });
            }
            (None, Some(line), Some(p)) => {
                let delta = resp.drag_delta();
                let now = view.to_frame(rect, p);
                let before = view.to_frame(rect, p - delta);
                actions.push(Action::LineDrag {
                    line,
                    image: panel.image,
                    dx: f64::from(now.x - before.x),
                    dy: f64::from(now.y - before.y),
                });
            }
            (None, None, _) if tool != Tool::Edit => view.pan(resp.drag_delta()),
            _ => {}
        }
    }
    if resp.drag_stopped_by(PointerButton::Primary) && !had_caption {
        if drag.take().is_some() {
            actions.push(Action::EndDrag);
        } else if grabs.line.take().is_some() {
            actions.push(Action::LineDragEnd);
        } else if tool == Tool::Edit {
            if let (Some(a), Some(b)) = (marquee_from, resp.interact_pointer_pos()) {
                let (fa, fb) = (view.to_frame(rect, a), view.to_frame(rect, b));
                actions.push(Action::Marquee {
                    image: panel.image,
                    rect: [
                        f64::from(fa.x),
                        f64::from(fa.y),
                        f64::from(fb.x),
                        f64::from(fb.y),
                    ],
                    mode: mode_for_marquee(shift, alt),
                });
            }
        }
    }
    if resp.clicked_by(PointerButton::Primary) {
        if let Some(p) = resp.interact_pointer_pos() {
            let f = view.to_frame(rect, p);
            let (fx, fy) = (f64::from(f.x), f64::from(f.y));
            let dot = dot_at(p);
            let line = if dot.is_none() { line_at(p) } else { None };
            match tool {
                Tool::Edit => match (dot, line) {
                    (Some(index), _) => actions.push(Action::SelectDot {
                        index,
                        mode: if shift { SelMode::Toggle } else { SelMode::Set },
                    }),
                    (None, Some(index)) => actions.push(Action::SelectLine {
                        index,
                        mode: if shift { SelMode::Add } else { SelMode::Set },
                    }),
                    _ if !shift => actions.push(Action::SelectNone),
                    _ => {}
                },
                Tool::Add => match (dot, line) {
                    (Some(index), _) => actions.push(Action::SelectDot {
                        index,
                        mode: SelMode::Set,
                    }),
                    (None, Some(index)) if in_frame(f) => actions.push(Action::InsertOnLine {
                        line: index,
                        x: fx,
                        y: fy,
                        image: panel.image,
                    }),
                    _ if in_frame(f) => actions.push(Action::Add {
                        x: fx,
                        y: fy,
                        image: panel.image,
                        same: alt,
                    }),
                    _ => actions.push(Action::SelectNone),
                },
                Tool::Delete => {
                    if let Some(index) = dot {
                        actions.push(Action::Delete(index));
                    }
                }
                Tool::Line => {
                    if dot.is_some() || in_frame(f) {
                        actions.push(Action::LineClick {
                            x: fx,
                            y: fy,
                            image: panel.image,
                            dot,
                            finish: shift,
                        });
                    }
                }
            }
        }
    }
    if resp.secondary_clicked() {
        if tool == Tool::Line {
            actions.push(Action::LineEnd);
        } else if let Some(i) = resp.interact_pointer_pos().and_then(dot_at) {
            actions.push(Action::Delete(i));
        }
    }
    if resp.hovered() {
        let icon = if drag.is_some() || grabs.line.is_some() {
            CursorIcon::Grabbing
        } else if over.is_some() || over_line.is_some() {
            CursorIcon::Grab
        } else {
            match tool {
                Tool::Edit => CursorIcon::Default,
                _ => CursorIcon::Crosshair,
            }
        };
        ui.ctx().set_cursor_icon(icon);
    }

    // --- drawing ---
    let (painter, frame_rect) = draw_background(ui, rect, view, panel.frame, panel.checker);
    match &panel.shown {
        Some(shown) => draw_shown(&painter, view, rect, shown),
        None => draw_message(
            &painter,
            rect,
            panel.message.as_deref().unwrap_or("No image"),
        ),
    }
    if panel.prefs.show_grid {
        overlay::draw_grid(&painter, view, rect, panel.frame);
    }
    painter.rect_stroke(
        frame_rect,
        0.0,
        Stroke::new(1.0_f32, Color32::from_gray(110)),
        StrokeKind::Outside,
    );
    if panel.prefs.show_triangles {
        draw_mesh(&painter, view, rect, panel.mesh);
    }
    let hovering = drag.is_none() && grabs.line.is_none();
    let hot = |i: usize| panel.hot_group.contains(&i);
    if panel.prefs.show_lines {
        for (l, (a, b)) in panel.lines.iter().zip(&screen_lines) {
            let line_hot =
                (hovering && over_line == Some(l.index)) || (hot(l.a_dot) && hot(l.b_dot));
            overlay::draw_line(&painter, *a, *b, line_hot);
        }
    }
    if let (Some(from), Some(p)) = (panel.chain_from, pointer) {
        overlay::draw_line(&painter, view.to_screen(rect, from), p, true);
    }
    let mut loupe_dots = Vec::new();
    if panel.prefs.show_dots {
        for (d, p) in panel.dots.iter().zip(&screen_dots) {
            let hovered = hovering && over == Some(d.index);
            let look = DotLook {
                paired: d.paired,
                selected: panel.selected.contains(&d.index),
                hovered,
                partner_hovered: panel.partner_hover == Some(d.index),
                group_hovered: hot(d.index),
            };
            let color = dot_color(&panel.prefs, d.index, look);
            let big = look.selected || hovered || look.partner_hovered;
            overlay::draw_dot(&painter, *p, panel.prefs.dot_style, color, big);
            loupe_dots.push((*p, color));
            if panel.show_numbers {
                overlay::draw_number(&painter, *p, (d.index + 1).to_string());
            }
        }
    }
    if let Some(g) = panel.ghost {
        let t = ui.input(|i| i.time);
        overlay::draw_ghost(&painter, view.to_screen(rect, g), t);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(120));
    }
    if marquee_live {
        if let (Some(a), Some(b)) = (marquee_from, resp.interact_pointer_pos()) {
            overlay::draw_marquee(&painter, a, b);
        }
    }
    if panel.prefs.magnifier && resp.hovered() {
        if let Some(p) = resp.hover_pos() {
            overlay::draw_loupe(
                &painter,
                view,
                rect,
                panel.shown.as_ref(),
                p,
                panel.prefs.magnifier_power,
                &loupe_dots,
                panel.prefs.dot_style,
            );
        }
    }
    PanelOutput {
        rect,
        actions,
        hover: Hover {
            dot: if hovering { over } else { None },
            line: if hovering { over_line } else { None },
        },
    }
}

/// Everything the preview draws.
pub struct PreviewPanel<'a> {
    pub title: String,
    pub frame: Vec2,
    pub shown: Option<Shown>,
    /// Mesh triangles at the current frame, frame coordinates.
    pub mesh: &'a [[Pos2; 3]],
    pub prefs: DisplayPrefs,
    /// Each dot's path, frame coordinates (drawn when `prefs.show_track` is on).
    pub tracks: &'a [Vec<Pos2>],
    pub message: Option<String>,
    /// While the caption window is open for the sequence or the movie: where the caption sits,
    /// in frame coordinates. A drag that starts on it moves it instead of panning.
    pub caption: Option<Rect>,
    /// While the Camera window is open: the camera being edited, with handles on the preview.
    pub camera: Option<CameraOverlay>,
    pub checker: bool,
}

/// The pose the camera handles are drawn from, and the frame it moves, in output pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraOverlay {
    pub pose: CameraKey,
    pub frame: (f64, f64),
}

/// Draw the preview. It is view-only: any drag pans and the wheel zooms, except a drag that
/// starts on an editable caption.
pub fn preview_panel(
    ui: &mut Ui,
    panel: &PreviewPanel<'_>,
    view: &mut View,
    grabs: &mut Grabs,
) -> PanelOutput {
    let (rect, resp) = canvas(ui, &panel.title);
    if view.auto_fit {
        *view = View::fit(panel.frame, rect);
    }
    let mut actions = Vec::new();
    if resp.drag_started_by(PointerButton::Primary) {
        let pressed = ui
            .input(|i| i.pointer.press_origin())
            .or_else(|| resp.interact_pointer_pos());
        if let (Some(c), Some(p)) = (panel.camera, pressed) {
            let at = view.to_frame(rect, p);
            let reach = f64::from(GRAB_RADIUS / view.zoom().max(1e-6));
            let at = (f64::from(at.x), f64::from(at.y));
            if let Some(grab) = camera_tool::pick(&c.pose, c.frame, at, reach) {
                grabs.camera = true;
                actions.push(Action::CameraBegin { grab, at });
            }
        }
    }
    if grabs.camera {
        if resp.dragged_by(PointerButton::Primary) {
            if let Some(now) = resp.interact_pointer_pos() {
                let f = view.to_frame(rect, now);
                actions.push(Action::CameraMove {
                    to: (f64::from(f.x), f64::from(f.y)),
                });
            }
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        }
        if resp.drag_stopped_by(PointerButton::Primary) || !ui.input(|i| i.pointer.primary_down()) {
            grabs.camera = false;
            actions.push(Action::CameraEnd);
        }
    }
    if resp.drag_started_by(PointerButton::Primary) && !grabs.camera {
        if let (Some(r), Some(p)) = (panel.caption, resp.interact_pointer_pos()) {
            if r.contains(view.to_frame(rect, p)) {
                grabs.caption = true;
                actions.push(Action::CaptionBegin);
            }
        }
    }
    if grabs.caption {
        caption_drag(ui, &resp, rect, view, grabs, &mut actions);
    }
    let pan_buttons: &[PointerButton] = if grabs.caption || grabs.camera {
        &[PointerButton::Middle, PointerButton::Secondary]
    } else {
        &[
            PointerButton::Primary,
            PointerButton::Middle,
            PointerButton::Secondary,
        ]
    };
    view_input(ui, &resp, rect, view, pan_buttons);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::Default);
    }
    let (painter, frame_rect) = draw_background(ui, rect, view, panel.frame, panel.checker);
    match &panel.shown {
        Some(shown) => draw_shown(&painter, view, rect, shown),
        None => draw_message(
            &painter,
            rect,
            panel.message.as_deref().unwrap_or("No preview"),
        ),
    }
    if panel.prefs.show_grid {
        overlay::draw_grid(&painter, view, rect, panel.frame);
    }
    painter.rect_stroke(
        frame_rect,
        0.0,
        Stroke::new(1.0_f32, Color32::from_gray(110)),
        StrokeKind::Outside,
    );
    if panel.prefs.show_triangles {
        draw_mesh(&painter, view, rect, panel.mesh);
    }
    if panel.prefs.show_track {
        overlay::draw_tracks(&painter, view, rect, panel.tracks);
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
    if let Some(caption) = panel.caption {
        draw_caption_frame(&painter, view, rect, caption);
    }
    if let Some(c) = panel.camera {
        crop_view::draw_camera_frame(&painter, view, rect, &c);
    }
    PanelOutput {
        rect,
        actions,
        hover: Hover::default(),
    }
}
