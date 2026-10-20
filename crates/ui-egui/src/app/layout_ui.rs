//! The canvases and how they are arranged: the six layouts (F5 to F9), the draggable splitters,
//! the second window of the dual layout, and the zoom link between the panels.

use super::*;
use crate::state::ViewMode;
use egui::Color32;

/// Space between panels, which is also the splitter handle.
const GAP: f32 = 6.0;
const MIN_PANEL: f32 = 60.0;

/// Where each piece goes inside the central area for one layout.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Rects {
    pub source1: Option<Rect>,
    pub source2: Option<Rect>,
    pub preview: Option<Rect>,
    /// The handle between the sources and the preview.
    pub split_rows: Option<Rect>,
    /// The handle between Image 1 and Image 2.
    pub split_cols: Option<Rect>,
}

/// Lay the panels of `mode` out in `avail`. `top` is the share of the height the sources get
/// when the preview is shown too, `left` the share of the width Image 1 gets.
pub(crate) fn compute_rects(mode: ViewMode, avail: Rect, top: f32, left: f32) -> Rects {
    let (top, left) = (
        if top.is_finite() {
            top.clamp(0.2, 0.8)
        } else {
            0.5
        },
        if left.is_finite() {
            left.clamp(0.2, 0.8)
        } else {
            0.5
        },
    );
    let width = avail.width().max(0.0);
    let height = avail.height().max(0.0);
    let left_w = ((width - GAP).max(0.0) * left).clamp(
        MIN_PANEL.min(width),
        (width - GAP - MIN_PANEL).max(MIN_PANEL),
    );
    let right_w = (width - GAP - left_w).max(0.0);
    let sources = |h: f32| {
        let s1 = Rect::from_min_size(avail.min, Vec2::new(left_w, h));
        let s2 = Rect::from_min_size(
            Pos2::new(avail.min.x + left_w + GAP, avail.min.y),
            Vec2::new(right_w, h),
        );
        let cols = Rect::from_min_size(
            Pos2::new(avail.min.x + left_w, avail.min.y),
            Vec2::new(GAP, h),
        );
        (s1, s2, cols)
    };
    match mode {
        ViewMode::Full => {
            let top_h = (height * top - GAP / 2.0)
                .max(MIN_PANEL)
                .min((height - GAP).max(0.0));
            let (s1, s2, cols) = sources(top_h);
            let bottom =
                Rect::from_min_max(Pos2::new(avail.min.x, avail.min.y + top_h + GAP), avail.max);
            Rects {
                source1: Some(s1),
                source2: Some(s2),
                preview: Some(bottom),
                split_rows: Some(Rect::from_min_size(
                    Pos2::new(avail.min.x, avail.min.y + top_h),
                    Vec2::new(width, GAP),
                )),
                split_cols: Some(cols),
            }
        }
        ViewMode::Edit | ViewMode::Dual => {
            let (s1, s2, cols) = sources(height);
            Rects {
                source1: Some(s1),
                source2: Some(s2),
                preview: None,
                split_rows: None,
                split_cols: Some(cols),
            }
        }
        ViewMode::Preview => Rects {
            preview: Some(avail),
            ..Rects::default()
        },
        ViewMode::Frames => Rects::default(),
    }
}

impl InbetweenApp {
    pub(super) fn canvases(&mut self, ctx: &Context) {
        // Everything the panels need is copied out first, so drawing never borrows the session.
        let mut snapshot = self.snapshot(ctx);
        let mut crops = std::mem::take(&mut snapshot.crop);
        let mut views = self.state.views;
        let mut drag = self.drag;
        let mut grabs = self.grabs;
        let mut actions: Vec<Action> = Vec::new();
        let show_numbers = self.state.show_numbers;
        let prefs = *self.session.prefs();
        let tool = self.state.tool;
        let selected = self.session.selection().clone();
        let mut hover = [Hover::default(); 2];
        let mut top_fraction = self.state.top_fraction;
        let mut left_fraction = self.state.left_fraction;
        let mode = self.state.mode;
        let checker = self.state.checkerboard;
        let mut layout = Layout::default();

        egui::CentralPanel::default().show(ctx, |ui| {
            let avail = ui.available_rect_before_wrap();
            if mode == ViewMode::Frames {
                self.frame_strip(ui);
                return;
            }
            let rects = compute_rects(mode, avail, top_fraction, left_fraction);
            let frame = snapshot.frame;

            for (index, slot) in [(0usize, rects.source1), (1, rects.source2)] {
                let Some(rect) = slot else { continue };
                let panel = SourcePanel {
                    image: index as u8 + 1,
                    title: snapshot.titles[index].clone(),
                    frame,
                    shown: snapshot.shown[index],
                    dots: &snapshot.dots[index],
                    lines: &snapshot.lines[index],
                    mesh: &snapshot.mesh[index],
                    selected: &selected,
                    prefs,
                    tool,
                    show_numbers,
                    partner_hover: snapshot.partner_hover[index],
                    hot_group: &snapshot.hot_group,
                    ghost: snapshot.ghost[index],
                    chain_from: snapshot.chain_from[index],
                    message: Some(snapshot.message.clone()),
                    crop: crops[index].take(),
                    caption: snapshot.caption[index],
                    checker,
                };
                let out = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                        panels::source_panel(ui, &panel, &mut views[index], &mut drag, &mut grabs)
                    })
                    .inner;
                actions.extend(out.actions);
                hover[index] = out.hover;
                if index == 0 {
                    layout.source1 = out.rect;
                } else {
                    layout.source2 = out.rect;
                }
            }
            if let (Some(rect), ViewMode::Full | ViewMode::Preview) = (rects.preview, mode) {
                let preview = PreviewPanel {
                    title: snapshot.titles[2].clone(),
                    frame,
                    shown: snapshot.shown[2],
                    mesh: &snapshot.mesh[2],
                    prefs,
                    tracks: &snapshot.tracks,
                    message: snapshot.preview_message.clone(),
                    caption: snapshot.caption[2],
                    checker,
                };
                let out = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                        panels::preview_panel(ui, &preview, &mut views[2], &mut grabs)
                    })
                    .inner;
                layout.preview = out.rect;
                actions.extend(out.actions);
            }

            // Splitters: drag to resize, double-click to reset.
            if let Some(handle) = rects.split_rows {
                let r = ui.interact(handle, egui::Id::new("split_rows"), egui::Sense::drag());
                if r.hovered() || r.dragged() {
                    ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                }
                if r.dragged() && avail.height() > 0.0 {
                    top_fraction += r.drag_delta().y / avail.height();
                }
                if r.double_clicked() {
                    top_fraction = 0.5;
                }
                ui.painter().rect_filled(
                    handle.shrink2(Vec2::new(0.0, 2.0)),
                    1.0,
                    Color32::from_gray(60),
                );
            }
            if let Some(handle) = rects.split_cols {
                let r = ui.interact(handle, egui::Id::new("split_cols"), egui::Sense::drag());
                if r.hovered() || r.dragged() {
                    ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
                if r.dragged() && avail.width() > 0.0 {
                    left_fraction += r.drag_delta().x / avail.width();
                }
                if r.double_clicked() {
                    left_fraction = 0.5;
                }
                ui.painter().rect_filled(
                    handle.shrink2(Vec2::new(2.0, 0.0)),
                    1.0,
                    Color32::from_gray(60),
                );
            }
        });

        // The dual layout shows the preview in a window of its own.
        let mut dual_closed = false;
        if mode == ViewMode::Dual {
            let preview = PreviewPanel {
                title: snapshot.titles[2].clone(),
                frame: snapshot.frame,
                shown: snapshot.shown[2],
                mesh: &snapshot.mesh[2],
                prefs,
                tracks: &snapshot.tracks,
                message: snapshot.preview_message.clone(),
                caption: snapshot.caption[2],
                checker,
            };
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("inbetween_preview"),
                egui::ViewportBuilder::default()
                    .with_title("Inbetween Preview")
                    .with_inner_size([960.0, 600.0]),
                |ctx, class| {
                    let mut body = |ui: &mut egui::Ui| {
                        let out = panels::preview_panel(ui, &preview, &mut views[2], &mut grabs);
                        actions.extend(out.actions);
                    };
                    if class == egui::ViewportClass::Embedded {
                        egui::Window::new("Preview")
                            .default_size([640.0, 420.0])
                            .show(ctx, |ui| body(ui));
                    } else {
                        egui::CentralPanel::default().show(ctx, |ui| body(ui));
                    }
                    if ctx.input(|i| i.viewport().close_requested()) {
                        dual_closed = true;
                    }
                },
            );
        }
        if dual_closed {
            self.set_mode(ViewMode::Full);
        }
        self.state.top_fraction = top_fraction.clamp(0.2, 0.8);
        self.state.left_fraction = left_fraction.clamp(0.2, 0.8);

        // Linked source views move together: whichever one changed drives the other.
        let old = self.state.views;
        if self.state.link_sources {
            if views[0] != old[0] {
                views[1] = views[0];
            } else if views[1] != old[1] {
                views[0] = views[1];
            }
        }
        // The preview can follow the sources (and drive them) once a view was changed by hand.
        if self.state.sync_preview && layout.preview.area() > 0.0 && layout.source1.area() > 0.0 {
            if (views[0] != old[0] || views[1] != old[1]) && !views[0].auto_fit {
                views[2] = actions_ui::carry_view(views[0], layout.source1, layout.preview);
            } else if views[2] != old[2] && !views[2].auto_fit {
                views[0] = actions_ui::carry_view(views[2], layout.preview, layout.source1);
                views[1] = actions_ui::carry_view(views[2], layout.preview, layout.source2);
            }
        }
        self.state.views = views;
        self.drag = drag;
        self.hover = hover;
        self.grabs = grabs;
        self.layout = layout;
        self.track_focus(ctx);
        for action in actions {
            self.apply(action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn avail() -> Rect {
        Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(1000.0, 600.0))
    }

    #[test]
    fn full_layout_has_all_three_and_splitters_between_them() {
        let r = compute_rects(ViewMode::Full, avail(), 0.5, 0.5);
        let (s1, s2, p) = (r.source1.unwrap(), r.source2.unwrap(), r.preview.unwrap());
        assert!(s1.max.x < s2.min.x && s1.max.y < p.min.y);
        assert!((s1.width() - s2.width()).abs() < 1e-3);
        assert_eq!(p.max, avail().max);
        assert_eq!(r.split_rows.unwrap().min.y, s1.max.y);
        assert_eq!(r.split_cols.unwrap().min.x, s1.max.x);
    }

    #[test]
    fn each_mode_shows_the_right_panels() {
        let a = avail();
        let edit = compute_rects(ViewMode::Edit, a, 0.5, 0.5);
        assert!(edit.source1.is_some() && edit.preview.is_none());
        assert_eq!(edit.source1.unwrap().height(), a.height());
        let dual = compute_rects(ViewMode::Dual, a, 0.5, 0.5);
        assert!(dual.preview.is_none() && dual.source2.is_some());
        let preview = compute_rects(ViewMode::Preview, a, 0.5, 0.5);
        assert_eq!(preview.preview, Some(a));
        assert!(preview.source1.is_none() && preview.split_cols.is_none());
        assert_eq!(
            compute_rects(ViewMode::Frames, a, 0.5, 0.5),
            Rects::default()
        );
    }

    #[test]
    fn fractions_move_the_split_and_nonsense_is_repaired() {
        let a = avail();
        let wide = compute_rects(ViewMode::Full, a, 0.5, 0.7);
        let narrow = compute_rects(ViewMode::Full, a, 0.5, 0.3);
        assert!(wide.source1.unwrap().width() > narrow.source1.unwrap().width());
        let tall = compute_rects(ViewMode::Full, a, 0.8, 0.5);
        assert!(
            tall.source1.unwrap().height()
                > compute_rects(ViewMode::Full, a, 0.3, 0.5)
                    .source1
                    .unwrap()
                    .height()
        );
        let bad = compute_rects(ViewMode::Full, a, f32::NAN, f32::INFINITY);
        assert!(bad.source1.unwrap().width().is_finite());
        // A tiny area never produces negative sizes.
        let tiny = Rect::from_min_size(Pos2::ZERO, Vec2::new(30.0, 10.0));
        for mode in ViewMode::ALL {
            let r = compute_rects(mode, tiny, 0.5, 0.5);
            for rect in [r.source1, r.source2, r.preview].into_iter().flatten() {
                assert!(rect.width() >= 0.0 && rect.height() >= 0.0, "{mode:?}");
            }
        }
    }
}
