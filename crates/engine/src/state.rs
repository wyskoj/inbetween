//! Session state that is not part of the project and not undone: the dot selection, the
//! dot clipboard and the display and tool preferences.

use inbetween_doc::{DotPair, Line};
use serde::{Deserialize, Serialize};

/// Dots (and the lines between them) held by copy and cut. Line indices refer to positions
/// inside `pairs`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DotClip {
    pub pairs: Vec<DotPair>,
    pub lines: Vec<Line>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DotStyle {
    #[default]
    Disc,
    Ring,
    Cross,
    Square,
    Diamond,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DotColorMode {
    /// One colour for every dot.
    Single,
    /// Cycle through eight colours by dot number.
    #[default]
    Cycle8,
}

/// Display and tool preferences. Shells persist them and push them into the session at
/// startup; the `view.toggle.*`, `prefs.*` and `dots.option.*` commands edit them.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayPrefs {
    pub show_dots: bool,
    pub show_lines: bool,
    pub show_triangles: bool,
    pub show_track: bool,
    pub show_grid: bool,
    pub dot_style: DotStyle,
    pub dot_color_mode: DotColorMode,
    /// Colour used by `DotColorMode::Single`, as 0xRRGGBB.
    pub dot_color: u32,
    /// Dot opacity, 64 to 255.
    pub dot_opacity: u8,
    pub magnifier: bool,
    /// Magnifier power, 1 to 4.
    pub magnifier_power: u8,
    /// A new dot also creates its partner (at the mapped position) instead of waiting for one.
    pub auto_partner: bool,
    /// After placing a dot, jump the cursor to the other image.
    pub auto_jump: bool,
}

impl Default for DisplayPrefs {
    fn default() -> Self {
        Self {
            show_dots: true,
            show_lines: true,
            show_triangles: true,
            show_track: false,
            show_grid: false,
            dot_style: DotStyle::Disc,
            dot_color_mode: DotColorMode::Cycle8,
            dot_color: 0x00E6_5C2E,
            dot_opacity: 255,
            magnifier: false,
            magnifier_power: 2,
            auto_partner: true,
            auto_jump: false,
        }
    }
}

impl DisplayPrefs {
    /// Pull every field into its legal range, so hostile saved settings do no harm.
    pub fn sanitized(mut self) -> Self {
        self.dot_opacity = self.dot_opacity.max(64);
        self.magnifier_power = self.magnifier_power.clamp(1, 4);
        self.dot_color &= 0x00FF_FFFF;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_round_trip_and_sanitize() {
        let p = DisplayPrefs::default();
        let back: DisplayPrefs = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(p, back);
        let odd: DisplayPrefs =
            serde_json::from_str(r#"{"dot_opacity":3,"magnifier_power":99,"unknown":1}"#).unwrap();
        let odd = odd.sanitized();
        assert_eq!((odd.dot_opacity, odd.magnifier_power), (64, 4));
        assert!(odd.show_dots);
    }
}
