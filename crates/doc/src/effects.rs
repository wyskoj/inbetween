//! Per-sequence effects: a solid back colour, a background image, a mask that controls the
//! morph's opacity, and a foreground overlay.
//!
//! Layer order, bottom to top: back colour, background image, the (masked) morph, foreground.

use crate::{Camera, DocError, ImageRef};
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

/// A grayscale image that decides *when* each pixel dissolves from Image 1 to Image 2.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transition {
    pub enabled: bool,
    pub image: ImageRef,
    /// 0 (hard edge between the two images) to 100 (the whole transition is one soft fade).
    pub softness: f64,
    /// Dissolve light areas first instead of dark ones.
    pub invert: bool,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            enabled: false,
            image: ImageRef::default(),
            softness: 20.0,
            invert: false,
        }
    }
}

/// A grayscale image that brightens or darkens the morph: light areas of the image brighten,
/// dark areas darken, mid gray leaves the pixel alone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Light {
    pub enabled: bool,
    pub image: ImageRef,
    /// 0 to 100: how far the lightest areas push the morph towards white.
    pub bright: f64,
    /// 0 to 100: how far the darkest areas push the morph towards black.
    pub dark: f64,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            enabled: false,
            image: ImageRef::default(),
            bright: 50.0,
            dark: 50.0,
        }
    }
}

/// Motion blur or a motion trail made from the frames just before the current one.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Trail {
    pub enabled: bool,
    /// On: blur (the earlier frames are averaged with this one). Off: a trail (tinted copies
    /// of the earlier frames fade out behind this one).
    pub overlay: bool,
    /// Colour of the trail copies, straight 8-bit RGBA. Unused by the blur.
    pub rgba: [u8; 4],
    /// 0 to 100: how many earlier frames are used (up to 8) and, for a trail, how strong.
    pub intensity: f64,
}

impl Default for Trail {
    fn default() -> Self {
        Self {
            enabled: false,
            overlay: true,
            rgba: [255, 255, 255, 255],
            intensity: 50.0,
        }
    }
}

/// Rays radiating from the frame centre, drawn over or under the morph.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Radial {
    pub enabled: bool,
    /// Draw over the morph (on) or between the background and the morph (off).
    pub overlay: bool,
    /// Ray colour, straight 8-bit RGBA.
    pub rgba: [u8; 4],
    /// 0 to 100.
    pub intensity: f64,
}

impl Default for Radial {
    fn default() -> Self {
        Self {
            enabled: false,
            overlay: true,
            rgba: [255, 255, 255, 255],
            intensity: 50.0,
        }
    }
}

/// The image effects that a camera can move, by name.
pub const CAMERA_EFFECTS: [&str; 5] = ["background", "mask", "foreground", "transition", "light"];

/// A camera for each still-image effect: it moves the effect's picture over time while the
/// morph stays put. Pose times are the morph time (the movie's progress for movie effects).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffectCameras {
    pub background: Camera,
    pub mask: Camera,
    pub foreground: Camera,
    pub transition: Camera,
    pub light: Camera,
}

impl EffectCameras {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The camera of effect `name` (one of [`CAMERA_EFFECTS`]).
    pub fn get(&self, name: &str) -> Option<&Camera> {
        Some(match name {
            "background" => &self.background,
            "mask" => &self.mask,
            "foreground" => &self.foreground,
            "transition" => &self.transition,
            "light" => &self.light,
            _ => return None,
        })
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Camera> {
        Some(match name {
            "background" => &mut self.background,
            "mask" => &mut self.mask,
            "foreground" => &mut self.foreground,
            "transition" => &mut self.transition,
            "light" => &mut self.light,
            _ => return None,
        })
    }

    pub fn validate(&self) -> Result<(), DocError> {
        for name in CAMERA_EFFECTS {
            if let Some(c) = self.get(name) {
                c.validate()?;
            }
        }
        Ok(())
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
    pub transition: Transition,
    pub light: Light,
    pub trail: Trail,
    pub radial: Radial,
    /// Cameras that move the still-image effects (`effect.camera`).
    #[serde(skip_serializing_if = "EffectCameras::is_default")]
    pub cameras: EffectCameras,
}

impl Effects {
    /// Whether any effect changes the rendered frame (so the plain GPU morph is not enough).
    pub fn any_active(&self) -> bool {
        self.back_color.enabled
            || self.background.enabled
            || self.mask.enabled
            || self.foreground.enabled
            || self.transition.enabled
            || self.light.enabled
            || self.trail.enabled
            || self.radial.enabled
    }

    /// Every image reference an effect holds, switched on or not.
    pub fn all_images(&self) -> [&ImageRef; 5] {
        [
            &self.background.image,
            &self.mask.image,
            &self.foreground.image,
            &self.transition.image,
            &self.light.image,
        ]
    }

    /// Mutable access to every image reference an effect holds.
    pub fn all_images_mut(&mut self) -> [&mut ImageRef; 5] {
        [
            &mut self.background.image,
            &mut self.mask.image,
            &mut self.foreground.image,
            &mut self.transition.image,
            &mut self.light.image,
        ]
    }

    /// The image references of effects that are switched on.
    pub fn active_images(&self) -> impl Iterator<Item = &ImageRef> {
        [
            (self.background.enabled, &self.background.image),
            (self.mask.enabled, &self.mask.image),
            (self.foreground.enabled, &self.foreground.image),
            (self.transition.enabled, &self.transition.image),
            (self.light.enabled, &self.light.image),
        ]
        .into_iter()
        .filter_map(|(on, image)| on.then_some(image))
    }

    /// Whether every effect is off and at its defaults (so nothing needs writing).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Checks for effects on the whole movie: everything a sequence has except the
    /// transition, which needs two images.
    pub fn validate_movie(&self) -> Result<(), DocError> {
        if self.transition.enabled {
            return Err(DocError::BadEffect(
                "a transition works on a sequence, not on the whole movie",
            ));
        }
        self.validate()
    }

    pub fn validate(&self) -> Result<(), DocError> {
        self.cameras.validate()?;
        let ok = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !ok(self.background.opacity, 0.0, 100.0) || !ok(self.foreground.opacity, 0.0, 100.0) {
            return Err(DocError::BadEffect("opacity must be between 0 and 100"));
        }
        if !ok(self.mask.intensity, -100.0, 100.0) {
            return Err(DocError::BadEffect(
                "mask intensity must be between -100 and 100",
            ));
        }
        if !ok(self.transition.softness, 0.0, 100.0) {
            return Err(DocError::BadEffect(
                "transition softness must be between 0 and 100",
            ));
        }
        if !ok(self.light.bright, 0.0, 100.0) || !ok(self.light.dark, 0.0, 100.0) {
            return Err(DocError::BadEffect(
                "light intensities must be between 0 and 100",
            ));
        }
        if !ok(self.trail.intensity, 0.0, 100.0) || !ok(self.radial.intensity, 0.0, 100.0) {
            return Err(DocError::BadEffect(
                "trail and radial intensity must be between 0 and 100",
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
    fn new_effects_count_as_active_and_are_checked() {
        let mut e = Effects::default();
        e.radial.enabled = true;
        assert!(e.any_active());
        for bad in [f64::NAN, -1.0, 101.0] {
            let mut e = Effects::default();
            e.transition.softness = bad;
            assert!(e.validate().is_err());
            let mut e = Effects::default();
            e.light.dark = bad;
            assert!(e.validate().is_err());
            let mut e = Effects::default();
            e.trail.intensity = bad;
            assert!(e.validate().is_err());
            let mut e = Effects::default();
            e.radial.intensity = bad;
            assert!(e.validate().is_err());
        }
        let mut e = Effects::default();
        e.light.image = ImageRef::new("l.png");
        e.light.enabled = true;
        assert_eq!(e.active_images().count(), 1);
        assert_eq!(e.all_images().len(), 5);
    }

    #[test]
    fn movie_effects_refuse_a_transition() {
        let mut e = Effects::default();
        assert!(e.validate_movie().is_ok());
        e.transition.enabled = true;
        assert!(e.validate_movie().is_err());
        assert!(e.validate().is_ok());
    }

    #[test]
    fn old_files_without_effects_load_as_off() {
        let e: Effects = serde_json::from_str("{}").unwrap_or_default();
        assert_eq!(e, Effects::default());
    }
}
