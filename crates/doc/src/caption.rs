//! Caption text drawn over an image, a sequence or the whole movie.
//!
//! This is pure data. The renderer draws it (see `docs/decisions.md`). Sizes are in pixels of
//! the frame the caption is drawn on; the anchor `(x, y)` is a fraction of that frame.

use crate::{DocError, ImageRef};
use serde::{Deserialize, Serialize};

/// Longest caption text, in bytes.
pub const MAX_CAPTION_TEXT: usize = 4096;
/// Largest font size, border and shadow distance, in pixels.
pub const MAX_CAPTION_SIZE: f64 = 4000.0;
/// Largest anchor distance outside the frame, as a fraction of the frame.
pub const MAX_CAPTION_ANCHOR: f64 = 10.0;

/// How the lines of a caption line up with its anchor and each other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    /// The left edge of the text sits at the anchor.
    Left,
    /// The text is centred on the anchor.
    #[default]
    Center,
    /// The right edge of the text sits at the anchor.
    Right,
}

impl Align {
    pub const ALL: [Self; 3] = [Self::Left, Self::Center, Self::Right];

    pub fn id(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }
}

/// A soft drop shadow behind a caption. A `size` of 0 means no shadow.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shadow {
    /// Direction the shadow falls, in degrees counter-clockwise from the right.
    pub angle: f64,
    /// How far the shadow is offset, in pixels.
    pub distance: f64,
    /// Blur radius in pixels; 0 draws no shadow at all.
    pub size: f64,
    /// Straight sRGB RGBA.
    pub color: [u8; 4],
}

impl Default for Shadow {
    fn default() -> Self {
        Self {
            angle: -45.0,
            distance: 4.0,
            size: 0.0,
            color: [0, 0, 0, 160],
        }
    }
}

/// A caption. All fields have defaults, so a file may set only some of them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Caption {
    pub enabled: bool,
    /// The text; lines are separated by `\n`.
    pub text: String,
    /// A `.ttf` or `.otf` file. `None` uses the bundled font.
    pub font: Option<ImageRef>,
    /// Font size in pixels.
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    /// Straight sRGB RGBA.
    pub color: [u8; 4],
    /// Outline width in pixels; 0 draws none.
    pub border_size: f64,
    pub border_color: [u8; 4],
    pub align: Align,
    /// Anchor as a fraction of the frame width: where the text's left edge, centre or right
    /// edge (by `align`) sits.
    pub x: f64,
    /// Anchor as a fraction of the frame height: where the middle of the text block sits.
    pub y: f64,
    pub shadow: Shadow,
}

impl Default for Caption {
    fn default() -> Self {
        Self {
            enabled: true,
            text: String::new(),
            font: None,
            size: 48.0,
            bold: false,
            italic: false,
            color: [255, 255, 255, 255],
            border_size: 0.0,
            border_color: [0, 0, 0, 255],
            align: Align::Center,
            x: 0.5,
            y: 0.9,
            shadow: Shadow::default(),
        }
    }
}

fn check(v: f64, lo: f64, hi: f64, why: &'static str) -> Result<(), DocError> {
    if v.is_finite() && (lo..=hi).contains(&v) {
        Ok(())
    } else {
        Err(DocError::BadCaption(why))
    }
}

impl Caption {
    /// Whether drawing this caption changes anything.
    pub fn is_visible(&self) -> bool {
        self.enabled && !self.text.trim().is_empty() && self.color[3] > 0
    }

    pub fn validate(&self) -> Result<(), DocError> {
        if self.text.len() > MAX_CAPTION_TEXT {
            return Err(DocError::BadCaption("text is longer than 4096 bytes"));
        }
        check(
            self.size,
            1.0,
            MAX_CAPTION_SIZE,
            "size must be 1 to 4000 pixels",
        )?;
        check(
            self.border_size,
            0.0,
            MAX_CAPTION_SIZE,
            "border size must be 0 to 4000 pixels",
        )?;
        check(
            self.x,
            -MAX_CAPTION_ANCHOR,
            MAX_CAPTION_ANCHOR,
            "x must be between -10 and 10",
        )?;
        check(
            self.y,
            -MAX_CAPTION_ANCHOR,
            MAX_CAPTION_ANCHOR,
            "y must be between -10 and 10",
        )?;
        check(
            self.shadow.angle,
            -3600.0,
            3600.0,
            "shadow angle must be -3600 to 3600",
        )?;
        check(
            self.shadow.distance,
            -MAX_CAPTION_SIZE,
            MAX_CAPTION_SIZE,
            "shadow distance must be -4000 to 4000 pixels",
        )?;
        check(
            self.shadow.size,
            0.0,
            MAX_CAPTION_SIZE,
            "shadow size must be 0 to 4000 pixels",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_invisible_when_empty() {
        let c = Caption::default();
        assert!(c.validate().is_ok());
        assert!(!c.is_visible());
        let c = Caption {
            text: "hi".into(),
            ..Caption::default()
        };
        assert!(c.is_visible());
        assert!(!Caption {
            enabled: false,
            ..c.clone()
        }
        .is_visible());
    }

    #[test]
    fn bounds_are_refused() {
        let bad = |f: fn(&mut Caption)| {
            let mut c = Caption::default();
            f(&mut c);
            c.validate().is_err()
        };
        assert!(bad(|c| c.size = 0.0));
        assert!(bad(|c| c.size = f64::NAN));
        assert!(bad(|c| c.x = f64::INFINITY));
        assert!(bad(|c| c.border_size = -1.0));
        assert!(bad(|c| c.shadow.size = 1e9));
        assert!(bad(|c| c.text = "x".repeat(MAX_CAPTION_TEXT + 1)));
        let ok = Caption {
            text: "x".repeat(MAX_CAPTION_TEXT),
            ..Caption::default()
        };
        assert!(ok.validate().is_ok());
    }

    #[test]
    fn partial_json_fills_defaults() {
        let c: Caption = serde_json::from_str(r#"{"text":"a\nb","align":"left"}"#).unwrap();
        assert_eq!(c.text, "a\nb");
        assert_eq!(c.align, Align::Left);
        assert_eq!(c.size, 48.0);
    }
}
