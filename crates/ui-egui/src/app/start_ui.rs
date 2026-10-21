//! The start screen: what the canvas area shows before any sequence exists. It offers the two
//! ways in (New From Two Images, Open Project) and the recent projects.

use super::*;
use crate::kit::{self, ButtonKind, Icon};

/// Recent projects listed on the start screen.
const RECENT_SHOWN: usize = 5;
const COLUMN: f32 = 480.0;
const RECENT_ROW: f32 = 56.0;

/// What a click on the start screen asked for.
enum StartAction {
    Invoke(&'static str),
    Open(String),
    Forget(String),
}

impl InbetweenApp {
    /// Draw the start screen in the central area.
    pub(super) fn start_screen(&mut self, ctx: &Context) {
        let t = crate::theme::get(ctx);
        let c = t.color;
        let recent: Vec<(String, bool)> = self
            .state
            .recent
            .iter()
            .take(RECENT_SHOWN)
            .map(|p| (p.clone(), std::path::Path::new(p).exists()))
            .collect();
        let mut action = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(c.app))
            .show(ctx, |ui| {
                let avail = ui.available_rect_before_wrap();
                let width = COLUMN.min(avail.width() - t.space.xl * 2.0).max(200.0);
                let recent_h = if recent.is_empty() {
                    0.0
                } else {
                    t.space.xxl + 24.0 + RECENT_ROW * recent.len() as f32
                };
                let height = 236.0 + recent_h;
                let top = (avail.center().y - height / 2.0).max(avail.top() + t.space.xl);
                let column = Rect::from_min_size(
                    Pos2::new(avail.center().x - width / 2.0, top),
                    Vec2::new(width, height),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
                    ui.vertical_centered(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let (badge, _) =
                            ui.allocate_exact_size(Vec2::splat(56.0), egui::Sense::hover());
                        ui.painter().circle_filled(badge.center(), 28.0, c.field);
                        kit::icon(
                            ui,
                            Rect::from_center_size(badge.center(), Vec2::splat(28.0)),
                            Icon::Morph,
                            c.focus,
                            1.6,
                        );
                        ui.add_space(t.space.lg);
                        ui.label(
                            egui::RichText::new("Start a morph")
                                .font(t.medium(t.text.title))
                                .color(c.text),
                        );
                        ui.add_space(t.space.sm);
                        ui.label(
                            egui::RichText::new(
                                "Pick two images and place dots on matching features. \
                                 Inbetween renders the frames between them.",
                            )
                            .size(t.text.body)
                            .line_height(Some(t.text.body + 6.0))
                            .color(c.text_secondary),
                        );
                        ui.add_space(t.space.xl);
                        // Two buttons, centred as a pair.
                        let pair = 200.0 + 128.0 + t.space.md;
                        ui.allocate_ui_with_layout(
                            Vec2::new(pair, t.control.height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = t.space.md;
                                if kit::button(
                                    ui,
                                    ButtonKind::Primary,
                                    "New from two images\u{2026}",
                                    true,
                                    t.control.height,
                                )
                                .clicked()
                                {
                                    action = Some(StartAction::Invoke("project.new"));
                                }
                                if kit::button(
                                    ui,
                                    ButtonKind::Secondary,
                                    "Open project\u{2026}",
                                    true,
                                    t.control.height,
                                )
                                .clicked()
                                {
                                    action = Some(StartAction::Invoke("project.open"));
                                }
                            },
                        );
                        ui.add_space(t.space.md);
                        ui.label(
                            egui::RichText::new("or drop images or a project into this window")
                                .size(t.text.small)
                                .color(c.text_tertiary),
                        );
                    });
                    if !recent.is_empty() {
                        ui.add_space(t.space.xxl);
                        egui::Frame::NONE
                            .inner_margin(egui::Margin::symmetric(t.space.md as i8, 0))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("Recent")
                                        .text_style(crate::theme::label_style())
                                        .color(c.text_secondary),
                                );
                            });
                        ui.add_space(t.space.sm);
                        egui::Frame::NONE
                            .fill(c.surface)
                            .corner_radius(egui::CornerRadius::same(t.radius.card))
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                for (i, (path, exists)) in recent.iter().enumerate() {
                                    if i > 0 {
                                        let y = ui.cursor().top();
                                        ui.painter().hline(
                                            ui.max_rect().x_range(),
                                            y,
                                            egui::Stroke::new(1.0_f32, c.field),
                                        );
                                    }
                                    if recent_row(ui, path, *exists).clicked() {
                                        action = Some(if *exists {
                                            StartAction::Open(path.clone())
                                        } else {
                                            StartAction::Forget(path.clone())
                                        });
                                    }
                                }
                            });
                    }
                });
            });
        match action {
            Some(StartAction::Invoke(id)) => self.invoke(id),
            Some(StartAction::Open(path)) => self.open_project(&path),
            Some(StartAction::Forget(path)) => self.state.remove_recent(&path),
            None => {}
        }
    }
}

/// One recent project: its file name, and its folder (or that it is gone).
fn recent_row(ui: &mut egui::Ui, path: &str, exists: bool) -> egui::Response {
    let t = crate::theme::get(ui.ctx());
    let c = t.color;
    let p = std::path::Path::new(path);
    let name = p
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned());
    let folder = p
        .parent()
        .map(|f| f.display().to_string())
        .unwrap_or_default();
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, RECENT_ROW), egui::Sense::click());
    let label = if exists {
        format!("Open {name}")
    } else {
        format!("Remove {name} from the list")
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(t.radius.card), c.field);
    }
    let pad = t.space.md;
    let icon_rect = Rect::from_min_size(
        Pos2::new(rect.left() + pad, rect.center().y - 12.0),
        Vec2::splat(24.0),
    );
    kit::icon(
        ui,
        icon_rect.shrink(2.0),
        Icon::Morph,
        if exists {
            c.text_secondary
        } else {
            c.text_disabled
        },
        1.5,
    );
    let text_left = icon_rect.right() + pad;
    let text_w = (rect.right() - pad - text_left).max(10.0);
    let line = |text: &str, size: f32, color: egui::Color32| {
        let mut job =
            egui::text::LayoutJob::simple_singleline(text.to_string(), t.regular(size), color);
        job.wrap = egui::text::TextWrapping::truncate_at_width(text_w);
        ui.painter().layout_job(job)
    };
    let title = line(
        &name,
        t.text.body,
        if exists { c.text } else { c.text_tertiary },
    );
    let detail = line(
        if exists {
            &folder
        } else {
            "Not found \u{b7} click to remove it from the list"
        },
        t.text.small,
        c.text_tertiary,
    );
    let total = title.size().y + 2.0 + detail.size().y;
    let y = rect.center().y - total / 2.0;
    let title_h = title.size().y;
    ui.painter().galley(Pos2::new(text_left, y), title, c.text);
    ui.painter().galley(
        Pos2::new(text_left, y + title_h + 2.0),
        detail,
        c.text_tertiary,
    );
    response.on_hover_text(path)
}
