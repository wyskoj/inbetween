//! The design tokens: every colour, text size, gap, radius and shadow the interface uses, defined
//! once. [`install`] turns them into egui's global style and keeps them in the context, so
//! windows read them with [`get`] instead of writing literal values. Change a value here and it
//! reaches every window. `docs/ui-design.md` explains the system and how to extend it.

use egui::{
    Color32, Context, CornerRadius, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle, Vec2,
};

/// The interface's medium weight, Geist Medium (titles, field labels, buttons). Installed by
/// [`crate::fonts::install`].
pub const MEDIUM: &str = "medium";

/// Field labels: 13 px medium. Use with `RichText::text_style`.
pub fn label_style() -> TextStyle {
    TextStyle::Name("label".into())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub color: Colors,
    pub text: TextSizes,
    pub space: Space,
    pub radius: Radii,
    pub control: Controls,
    pub shadow: Shadows,
}

/// Cool greys (hue about 220) for surfaces and text, one olive accent, and a danger hue.
/// Text colours pass WCAG AA on `field` and everything darker.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colors {
    /// Behind a modal window.
    pub backdrop: Color32,
    /// Behind the panels: the app's ground.
    pub app: Color32,
    /// Inside an image panel, around the picture (a step below `app`).
    pub well: Color32,
    /// Window footers: one step darker than the window.
    pub footer: Color32,
    /// Windows and panels.
    pub surface: Color32,
    /// Inputs, image slots, quiet buttons.
    pub field: Color32,
    pub field_hover: Color32,
    pub field_active: Color32,
    /// The edge of a window against the backdrop.
    pub edge: Color32,
    /// Frame ticks on the scrubber and other marks that must show on `field`.
    pub tick: Color32,
    /// Hairlines and input edges.
    pub line: Color32,
    /// The dashed edge of an empty drop target.
    pub dash: Color32,
    /// A disabled button's fill.
    pub disabled: Color32,
    pub text: Color32,
    pub text_secondary: Color32,
    pub text_tertiary: Color32,
    pub text_disabled: Color32,
    /// Primary button fill, selected rows and text selection (dark olive); white text on it is
    /// 7.7:1.
    pub accent: Color32,
    /// The primary button under the pointer: one step lighter.
    pub accent_hover: Color32,
    /// Links and accent text on dark surfaces (light sage, 9.7:1 on surface).
    pub accent_text: Color32,
    /// Focus rings, drop targets, the current item's outline and active edges (sage).
    pub focus: Color32,
    pub on_accent: Color32,
    /// Secondary text on an accent fill (5.5:1 on `accent`).
    pub on_accent_secondary: Color32,
    /// Danger: a tinted fill with light same-hue text, never grey text on red.
    pub danger_fill: Color32,
    pub danger_soft: Color32,
    pub danger_line: Color32,
    pub danger_text: Color32,
    pub danger_text_strong: Color32,
    pub warn_text: Color32,
}

/// Text sizes in points. Geist, two weights only: regular and [`MEDIUM`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextSizes {
    /// Window titles (medium).
    pub title: f32,
    /// Body and control text.
    pub body: f32,
    /// Field labels (medium).
    pub label: f32,
    /// Metadata and hints.
    pub small: f32,
    pub mono: f32,
}

/// The spacing scale. Every gap and padding is one of these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Space {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xxl: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radii {
    pub small: u8,
    pub control: u8,
    pub card: u8,
    pub window: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Controls {
    /// Buttons and inputs in dialogs.
    pub height: f32,
    /// Buttons inside cards and dense tool windows.
    pub compact_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadows {
    pub window: Shadow,
    pub popup: Shadow,
    /// Under a primary button.
    pub control: Shadow,
}

const fn grey(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

impl Theme {
    /// The dark theme the app ships with.
    pub const fn dark() -> Self {
        Self {
            color: Colors {
                backdrop: Color32::from_black_alpha(140),
                app: grey(0x15, 0x17, 0x1c),
                well: grey(0x0f, 0x11, 0x15),
                footer: grey(0x1a, 0x1d, 0x23),
                surface: grey(0x1e, 0x21, 0x28),
                field: grey(0x26, 0x2a, 0x33),
                field_hover: grey(0x2f, 0x34, 0x40),
                field_active: grey(0x36, 0x3c, 0x4a),
                edge: grey(0x2c, 0x31, 0x3b),
                tick: grey(0x4a, 0x50, 0x60),
                line: grey(0x33, 0x38, 0x44),
                dash: grey(0x6a, 0x72, 0x82),
                disabled: grey(0x2a, 0x2e, 0x37),
                text: grey(0xe8, 0xea, 0xef),
                text_secondary: grey(0xa3, 0xaa, 0xb8),
                text_tertiary: grey(0x8a, 0x92, 0xa2),
                text_disabled: grey(0x6a, 0x72, 0x82),
                accent: grey(0x4d, 0x56, 0x40),
                accent_hover: grey(0x5a, 0x64, 0x49),
                accent_text: grey(0xb5, 0xd3, 0x90),
                focus: grey(0xa5, 0xc5, 0x7f),
                on_accent: Color32::WHITE,
                on_accent_secondary: grey(0xd4, 0xdc, 0xc6),
                danger_fill: grey(0x2e, 0x21, 0x27),
                danger_soft: grey(0x47, 0x26, 0x2c),
                danger_line: grey(0xb4, 0x54, 0x5a),
                danger_text: grey(0xe0, 0xa3, 0x9c),
                danger_text_strong: grey(0xf6, 0xd4, 0xcf),
                warn_text: grey(0xe8, 0xb8, 0x6a),
            },
            text: TextSizes {
                title: 18.0,
                body: 14.0,
                label: 13.0,
                small: 12.0,
                mono: 13.0,
            },
            space: Space {
                xs: 4.0,
                sm: 8.0,
                md: 12.0,
                lg: 16.0,
                xl: 24.0,
                xxl: 32.0,
            },
            radius: Radii {
                small: 4,
                control: 6,
                card: 8,
                window: 12,
            },
            control: Controls {
                height: 36.0,
                compact_height: 28.0,
            },
            shadow: Shadows {
                window: Shadow {
                    offset: [0, 12],
                    blur: 32,
                    spread: 0,
                    color: Color32::from_black_alpha(128),
                },
                popup: Shadow {
                    offset: [0, 4],
                    blur: 12,
                    spread: 0,
                    color: Color32::from_black_alpha(96),
                },
                control: Shadow {
                    offset: [0, 1],
                    blur: 2,
                    spread: 0,
                    color: Color32::from_black_alpha(102),
                },
            },
        }
    }

    pub fn medium(&self, size: f32) -> FontId {
        FontId::new(size, FontFamily::Name(MEDIUM.into()))
    }

    pub fn regular(&self, size: f32) -> FontId {
        FontId::proportional(size)
    }

    /// egui's global style built from the tokens: what every window gets without asking.
    pub fn style(&self, mut style: egui::Style) -> egui::Style {
        let c = &self.color;
        let line = Stroke::new(1.0_f32, c.line);
        let control = CornerRadius::same(self.radius.control);

        style.text_styles = [
            (TextStyle::Heading, self.medium(self.text.title)),
            (TextStyle::Body, self.regular(self.text.body)),
            (TextStyle::Button, self.regular(self.text.body)),
            (TextStyle::Small, self.regular(self.text.small)),
            (TextStyle::Monospace, FontId::monospace(self.text.mono)),
            (label_style(), self.medium(self.text.label)),
        ]
        .into();

        let s = &mut style.spacing;
        s.item_spacing = Vec2::new(self.space.sm, self.space.xs);
        s.button_padding = Vec2::new(self.space.sm, self.space.xs);
        s.window_margin = Margin::same(self.space.md as i8);
        s.menu_margin = Margin::same(self.space.xs as i8);
        s.interact_size.y = 24.0;

        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = c.surface;
        v.window_fill = c.surface;
        v.window_stroke = Stroke::new(1.0_f32, c.edge);
        v.window_corner_radius = CornerRadius::same(self.radius.window);
        v.window_shadow = self.shadow.window;
        v.popup_shadow = self.shadow.popup;
        v.menu_corner_radius = CornerRadius::same(self.radius.card);
        v.faint_bg_color = c.footer;
        v.extreme_bg_color = c.field;
        v.code_bg_color = c.field;
        v.hyperlink_color = c.accent_text;
        v.error_fg_color = c.danger_text;
        v.warn_fg_color = c.warn_text;
        v.selection.bg_fill = c.accent;
        v.selection.stroke = Stroke::new(1.0_f32, c.text);

        let w = &mut v.widgets;
        w.noninteractive.bg_fill = c.surface;
        w.noninteractive.weak_bg_fill = c.surface;
        w.noninteractive.bg_stroke = line;
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, c.text_secondary);
        w.noninteractive.corner_radius = control;
        for (state, fill) in [
            (&mut w.inactive, c.field),
            (&mut w.hovered, c.field_hover),
            (&mut w.active, c.field_active),
            (&mut w.open, c.field_hover),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.corner_radius = control;
            state.fg_stroke = Stroke::new(1.0_f32, c.text);
            state.bg_stroke = line;
            state.expansion = 0.0;
        }
        w.hovered.bg_stroke = Stroke::new(1.0_f32, c.dash);
        w.active.bg_stroke = Stroke::new(1.0_f32, c.focus);
        style
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

fn key() -> egui::Id {
    egui::Id::new("inbetween_theme")
}

/// Make `theme` the interface's look: egui's global style, and what [`get`] returns.
pub fn install(ctx: &Context, theme: Theme) {
    ctx.data_mut(|d| d.insert_temp(key(), theme));
    ctx.style_mut(|style| *style = theme.style(style.clone()));
}

/// The installed theme (the default one before [`install`] has run).
pub fn get(ctx: &Context) -> Theme {
    ctx.data(|d| d.get_temp(key())).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_sets_the_global_style_and_get_returns_it() {
        let ctx = Context::default();
        let mut theme = Theme::dark();
        theme.color.surface = Color32::from_rgb(1, 2, 3);
        theme.text.body = 15.0;
        install(&ctx, theme);
        assert_eq!(get(&ctx), theme);
        let style = ctx.style();
        assert_eq!(style.visuals.window_fill, Color32::from_rgb(1, 2, 3));
        assert_eq!(style.visuals.panel_fill, Color32::from_rgb(1, 2, 3));
        assert_eq!(
            style.text_styles.get(&TextStyle::Body).map(|f| f.size),
            Some(15.0)
        );
    }

    #[test]
    fn every_spacing_and_size_token_is_on_the_four_point_grid() {
        let t = Theme::dark();
        let s = t.space;
        for v in [s.xs, s.sm, s.md, s.lg, s.xl, s.xxl] {
            assert_eq!(v % 4.0, 0.0, "{v}");
        }
        for v in [t.control.height, t.control.compact_height] {
            assert_eq!(v % 4.0, 0.0, "{v}");
        }
    }

    #[test]
    fn text_colours_keep_aa_contrast_on_fields() {
        // WCAG relative luminance of an sRGB colour.
        fn lum(c: Color32) -> f64 {
            let ch = |v: u8| {
                let v = f64::from(v) / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
        }
        let ratio = |a: Color32, b: Color32| {
            let (x, y) = (lum(a), lum(b));
            (x.max(y) + 0.05) / (x.min(y) + 0.05)
        };
        let c = Theme::dark().color;
        for (name, fg, bg) in [
            ("text on field", c.text, c.field),
            ("secondary on field", c.text_secondary, c.field),
            ("tertiary on field", c.text_tertiary, c.field),
            ("tertiary on footer", c.text_tertiary, c.footer),
            ("accent text on surface", c.accent_text, c.surface),
            ("white on accent", c.on_accent, c.accent),
            ("white on accent hover", c.on_accent, c.accent_hover),
            ("text on a selected row", c.text, c.accent),
            (
                "metadata on a selected row",
                c.on_accent_secondary,
                c.accent,
            ),
            ("focus ring on field", c.focus, c.field),
            ("danger text on danger fill", c.danger_text, c.danger_fill),
        ] {
            assert!(ratio(fg, bg) >= 4.5, "{name}: {:.2}", ratio(fg, bg));
        }
    }
}
