//! Per-sequence effects: a solid back colour, a background image, a mask that controls the
//! morph's opacity, and a foreground overlay.
//!
//! Layer order, bottom to top: back colour, background image, the (masked) morph, foreground.

use crate::{DocError, ImageRef};
use serde::{Deserialize, Serialize};

/// A solid colour behind everything, as straight (non-premultiplied) 8-bit RGBA.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackColor {
    pub enabled: bool,
    pub rgba: [u8; 4],
}

impl Default for BackColor {
    fn default() -> Self {
        Self {
            enabled: false,
            rgba: [0, 0, 0, 255],
        }
    }
}

/// An image layer with an opacity: used for the background and the foreground.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageLayer {
    pub enabled: bool,
    pub image: ImageRef,
    /// 0 (invisible) to 100 (fully opaque).
    pub opacity: f64,
}

impl Default for ImageLayer {
    fn default() -> Self {
        Self {
            enabled: false,
            image: ImageRef::default(),
            opacity: 100.0,
        }
    }
}

/// A grayscale image that scales the morph's opacity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mask {
    pub enabled: bool,
    pub image: ImageRef,
    /// -100 to 100. Positive: dark areas of the mask hide the morph (100 hides them fully).
    /// Negative: light areas hide it. 0 has no effect.
    pub intensity: f64,
    /// Swap light and dark before the intensity applies.
    pub invert: bool,
}

impl Default for Mask {
    fn default() -> Self {
        Self {
            enabled: false,
            image: ImageRef::default(),
            intensity: 100.0,
            invert: false,
        }
    }
}

/// All effects of one sequence.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Effects {
    pub back_color: BackColor,
    pub background: ImageLayer,
    pub mask: Mask,
    pub foreground: ImageLayer,
}

impl Effects {
    /// Whether any effect changes the rendered frame (so the plain GPU morph is not enough).
    pub fn any_active(&self) -> bool {
        self.back_color.enabled
            || self.background.enabled
            || self.mask.enabled
            || self.foreground.enabled
    }

    /// The image references of effects that are switched on.
    pub fn active_images(&self) -> impl Iterator<Item = &ImageRef> {
        [
            (self.background.enabled, &self.background.image),
            (self.mask.enabled, &self.mask.image),
            (self.foreground.enabled, &self.foreground.image),
        ]
        .into_iter()
        .filter_map(|(on, image)| on.then_some(image))
    }

    pub fn validate(&self) -> Result<(), DocError> {
        let ok = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !ok(self.background.opacity, 0.0, 100.0) || !ok(self.foreground.opacity, 0.0, 100.0) {
            return Err(DocError::BadEffect("opacity must be between 0 and 100"));
        }
        if !ok(self.mask.intensity, -100.0, 100.0) {
            return Err(DocError::BadEffect(
                "mask intensity must be between -100 and 100",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_effects_are_off_and_valid() {
        let e = Effects::default();
        assert!(!e.any_active());
        assert_eq!(e.active_images().count(), 0);
        assert!(e.validate().is_ok());
    }

    #[test]
    fn active_images_lists_only_enabled_layers() {
        let mut e = Effects::default();
        e.mask.image = ImageRef::new("m.png");
        e.foreground.image = ImageRef::new("f.png");
        e.foreground.enabled = true;
        let got: Vec<_> = e.active_images().map(|r| r.0.as_str()).collect();
        assert_eq!(got, ["f.png"]);
        assert!(e.any_active());
    }

    #[test]
    fn hostile_numbers_are_rejected() {
        for bad in [f64::NAN, f64::INFINITY, -1.0, 100.5] {
            let mut e = Effects::default();
            e.background.opacity = bad;
            assert!(e.validate().is_err(), "{bad}");
        }
        for bad in [f64::NAN, -100.5, 101.0, f64::NEG_INFINITY] {
            let mut e = Effects::default();
            e.mask.intensity = bad;
            assert!(e.validate().is_err(), "{bad}");
        }
    }

    #[test]
    fn old_files_without_effects_load_as_off() {
        let e: Effects = serde_json::from_str("{}").unwrap_or_default();
        assert_eq!(e, Effects::default());
    }
}
