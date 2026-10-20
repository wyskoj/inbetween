//! What is drawn on top of the images: dots in their styles and colours, lines, the grid,
//! dot tracks, the ghost partner and the magnifier loupe.

use super::Shown;
use crate::view::View;
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};
use inbetween_engine::{DisplayPrefs, DotColorMode, DotStyle};

/// Base radius of a dot, in screen points.
pub const DOT_RADIUS: f32 = 5.0;

const CYCLE: [(u8, u8, u8); 8] = [
    (40, 200, 255),
    (120, 220, 90),
    (255, 170, 40),
    (170, 120, 255),
    (255, 110, 150),
    (60, 230, 200),
    (230, 220, 70),
    (110, 160, 255),
];

const UNPAIRED: (u8, u8, u8) = (255, 64, 200);
const SELECTED: (u8, u8, u8) = (255, 205, 0);

/// How one dot is drawn this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct DotLook {
    pub paired: bool,
    pub selected: bool,
    /// The pointer is over it.
    pub hovered: bool,
    /// Its partner on the other panel is hovered.
    pub partner_hovered: bool,
    /// It belongs to a hovered line's group.
    pub group_hovered: bool,
}

/// The colour of dot number `index`.
pub fn dot_color(prefs: &DisplayPrefs, index: usize, look: DotLook) -> Color32 {
    let (r, g, b) = if look.selected {
        SELECTED
    } else if look.hovered || look.partner_hovered || look.group_hovered {
        (255, 255, 255)
    } else if !look.paired {
        UNPAIRED
    } else {
        match prefs.dot_color_mode {
            DotColorMode::Single => (
                ((prefs.dot_color >> 16) & 0xFF) as u8,
                ((prefs.dot_color >> 8) & 0xFF) as u8,
                (prefs.dot_color & 0xFF) as u8,
            ),
            DotColorMode::Cycle8 => CYCLE[index % CYCLE.len()],
        }
    };
    Color32::from_rgba_unmultiplied(r, g, b, prefs.dot_opacity)
}

/// Draw one dot at screen position `p`.
pub fn draw_dot(painter: &Painter, p: Pos2, style: DotStyle, color: Color32, big: bool) {
    let r = if big { DOT_RADIUS + 1.5 } else { DOT_RADIUS };
    let shadow = Color32::from_black_alpha(190);
    match style {
        DotStyle::Disc => {
            painter.circle(p, r + 1.0, shadow, Stroke::NONE);
            painter.circle(p, r, color, Stroke::NONE);
        }
        DotStyle::Ring => {
            painter.circle_stroke(p, r + 0.5, Stroke::new(3.5_f32, shadow));
            painter.circle_stroke(p, r + 0.5, Stroke::new(2.0_f32, color));
        }
        DotStyle::Cross => {
            let d = Vec2::new(r + 1.0, 0.0);
            let e = Vec2::new(0.0, r + 1.0);
            for (a, b) in [(p - d, p + d), (p - e, p + e)] {
                painter.line_segment([a, b], Stroke::new(4.0_f32, shadow));
            }
            for (a, b) in [(p - d, p + d), (p - e, p + e)] {
                painter.line_segment([a, b], Stroke::new(2.0_f32, color));
            }
        }
        DotStyle::Square => {
            let rect = Rect::from_center_size(p, Vec2::splat(2.0 * r));
            painter.rect_filled(rect.expand(1.0), 0.0, shadow);
            painter.rect_filled(rect, 0.0, color);
        }
        DotStyle::Diamond => {
            let pts = |k: f32| {
                vec![
                    p + Vec2::new(0.0, -k),
                    p + Vec2::new(k, 0.0),
                    p + Vec2::new(0.0, k),
                    p + Vec2::new(-k, 0.0),
                ]
            };
            painter.add(egui::Shape::convex_polygon(
                pts(r * 1.4 + 1.0),
                shadow,
                Stroke::NONE,
            ));
            painter.add(egui::Shape::convex_polygon(
                pts(r * 1.4),
                color,
                Stroke::NONE,
            ));
        }
    }
}

/// The dot's number next to it.
pub fn draw_number(painter: &Painter, p: Pos2, text: String) {
    let at = p + Vec2::new(DOT_RADIUS + 4.0, -DOT_RADIUS - 4.0);
    let font = FontId::proportional(12.0);
    painter.text(
        at + Vec2::splat(1.0),
        Align2::LEFT_BOTTOM,
        text.clone(),
        font.clone(),
        Color32::BLACK,
    );
    painter.text(at, Align2::LEFT_BOTTOM, text, font, Color32::WHITE);
}

/// A line between two dots (screen positions); `hot` draws it highlighted.
pub fn draw_line(painter: &Painter, a: Pos2, b: Pos2, hot: bool) {
    if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
        return;
    }
    painter.line_segment([a, b], Stroke::new(4.0_f32, Color32::from_black_alpha(150)));
    let color = if hot {
        Color32::from_rgb(255, 255, 255)
    } else {
        Color32::from_rgba_unmultiplied(120, 210, 255, 220)
    };
    painter.line_segment(
        [a, b],
        Stroke::new(if hot { 2.5_f32 } else { 1.5_f32 }, color),
    );
}

/// A grid spacing, in frame pixels, that is at least `min_screen` points wide on screen.
pub fn grid_step(zoom: f32, min_screen: f32) -> f32 {
    let zoom = if zoom.is_finite() && zoom > 1e-6 {
        zoom
    } else {
        1.0
    };
    let mut step = 1.0_f32;
    'grow: loop {
        for m in [1.0_f32, 2.0, 5.0] {
            if step * m * zoom >= min_screen {
                step *= m;
                break 'grow;
            }
        }
        step *= 10.0;
        if step > 1.0e9 {
            break;
        }
    }
    step
}

/// Grid lines over the frame, every `grid_step` frame pixels.
pub fn draw_grid(painter: &Painter, view: &View, rect: Rect, frame: Vec2) {
    let step = grid_step(view.zoom, 14.0);
    let stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 40));
    let clip = painter.clip_rect();
    let mut x = 0.0_f32;
    let mut guard = 0;
    while x <= frame.x && guard < 4000 {
        let a = view.to_screen(rect, Pos2::new(x, 0.0));
        let b = view.to_screen(rect, Pos2::new(x, frame.y));
        if a.x >= clip.min.x - 1.0 && a.x <= clip.max.x + 1.0 {
            painter.line_segment([a, b], stroke);
        }
        x += step;
        guard += 1;
    }
    let mut y = 0.0_f32;
    guard = 0;
    while y <= frame.y && guard < 4000 {
        let a = view.to_screen(rect, Pos2::new(0.0, y));
        let b = view.to_screen(rect, Pos2::new(frame.x, y));
        if a.y >= clip.min.y - 1.0 && a.y <= clip.max.y + 1.0 {
            painter.line_segment([a, b], stroke);
        }
        y += step;
        guard += 1;
    }
}

/// Each dot's path between the images, as polylines in frame coordinates.
pub fn draw_tracks(painter: &Painter, view: &View, rect: Rect, tracks: &[Vec<Pos2>]) {
    for path in tracks {
        let pts: Vec<Pos2> = path
            .iter()
            .map(|p| view.to_screen(rect, *p))
            .filter(|p| p.x.is_finite() && p.y.is_finite())
            .collect();
        if pts.len() >= 2 {
            painter.add(egui::Shape::line(
                pts.clone(),
                Stroke::new(1.5_f32, Color32::from_rgba_unmultiplied(255, 200, 60, 200)),
            ));
            if let Some(end) = pts.last() {
                painter.circle_filled(*end, 2.0, Color32::from_rgb(255, 200, 60));
            }
        }
    }
}

/// A blinking marker where an unpaired dot's partner would go.
pub fn draw_ghost(painter: &Painter, p: Pos2, time: f64) {
    if (time * 3.0).fract() < 0.5 {
        painter.circle_stroke(p, DOT_RADIUS + 3.0, Stroke::new(2.0_f32, Color32::WHITE));
        painter.circle_stroke(
            p,
            DOT_RADIUS + 3.0,
            Stroke::new(
                1.0_f32,
                Color32::from_rgb(UNPAIRED.0, UNPAIRED.1, UNPAIRED.2),
            ),
        );
    }
}

/// The marquee rectangle while it is dragged (screen positions).
pub fn draw_marquee(painter: &Painter, a: Pos2, b: Pos2) {
    let rect = Rect::from_two_pos(a, b);
    painter.rect_filled(
        rect,
        0.0,
        Color32::from_rgba_unmultiplied(255, 255, 255, 25),
    );
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0_f32, Color32::from_rgb(255, 255, 255)),
        StrokeKind::Inside,
    );
}

/// Side of the loupe, in screen points.
pub const LOUPE_SIZE: f32 = 128.0;

/// A square loupe near the pointer: the panel's texture magnified `power` times around the
/// pointer with the dots drawn on top. `dots` are screen positions of the panel's dots.
#[allow(clippy::too_many_arguments)]
pub fn draw_loupe(
    painter: &Painter,
    view: &View,
    rect: Rect,
    shown: Option<&Shown>,
    pointer: Pos2,
    power: u8,
    dots: &[(Pos2, Color32)],
    style: DotStyle,
) {
    let power = f32::from(power.clamp(1, 4));
    let size = Vec2::splat(LOUPE_SIZE);
    // Sit down-right of the pointer, or flip to stay inside the panel.
    let mut min = pointer + Vec2::new(24.0, 24.0);
    if min.x + size.x > rect.max.x {
        min.x = pointer.x - 24.0 - size.x;
    }
    if min.y + size.y > rect.max.y {
        min.y = pointer.y - 24.0 - size.y;
    }
    let loupe = Rect::from_min_size(min, size);
    let inner = painter.with_clip_rect(loupe);
    inner.rect_filled(loupe, 0.0, Color32::from_gray(20));
    // Screen offset -> magnified: a point at pointer + d appears at loupe centre + d * power.
    let centre = loupe.center();
    let map = |p: Pos2| centre + (p - pointer) * power;
    if let Some(shown) = shown {
        let a = map(view.to_screen(rect, shown.frame_rect.min));
        let b = map(view.to_screen(rect, shown.frame_rect.max));
        let on_screen = Rect::from_min_max(a, b);
        if on_screen.width().is_finite() && on_screen.height().is_finite() {
            inner.image(shown.id, on_screen, shown.uv, Color32::WHITE);
        }
    }
    for (p, color) in dots {
        let q = map(*p);
        if loupe.expand(8.0).contains(q) {
            draw_dot(&inner, q, style, *color, false);
        }
    }
    painter.rect_stroke(
        loupe,
        0.0,
        Stroke::new(2.0_f32, Color32::WHITE),
        StrokeKind::Outside,
    );
    let c = loupe.center();
    painter.line_segment(
        [c - Vec2::new(5.0, 0.0), c + Vec2::new(5.0, 0.0)],
        Stroke::new(1.0_f32, Color32::from_white_alpha(160)),
    );
    painter.line_segment(
        [c - Vec2::new(0.0, 5.0), c + Vec2::new(0.0, 5.0)],
        Stroke::new(1.0_f32, Color32::from_white_alpha(160)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_step_grows_when_zoomed_out_and_survives_nonsense() {
        assert_eq!(grid_step(1.0, 14.0), 20.0);
        assert!(grid_step(0.1, 14.0) >= 140.0);
        assert!(grid_step(10.0, 14.0) <= 2.0);
        for z in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(grid_step(z, 14.0).is_finite());
        }
    }

    #[test]
    fn colours_distinguish_paired_unpaired_and_selected() {
        let prefs = DisplayPrefs::default();
        let paired = DotLook {
            paired: true,
            ..DotLook::default()
        };
        let a = dot_color(&prefs, 0, paired);
        let b = dot_color(&prefs, 0, DotLook::default());
        let c = dot_color(
            &prefs,
            0,
            DotLook {
                selected: true,
                ..paired
            },
        );
        assert!(a != b && a != c && b != c);
        assert!(dot_color(&prefs, 1, paired) != a, "cycle8 steps by number");
        let single = DisplayPrefs {
            dot_color_mode: DotColorMode::Single,
            ..prefs
        };
        assert_eq!(dot_color(&single, 0, paired), dot_color(&single, 5, paired));
        let faint = DisplayPrefs {
            dot_opacity: 64,
            ..prefs
        };
        assert_eq!(dot_color(&faint, 0, paired).a(), 64);
    }
}
