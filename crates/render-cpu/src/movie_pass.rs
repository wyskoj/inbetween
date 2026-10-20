//! The movie pass: effects, colour adjustment and filters applied to a finished movie frame.
//!
//! Order: movie effects (the trail is the engine's job, it needs earlier frames), then the
//! colour adjustment, then the filter list. The movie caption goes on after this pass.

use crate::source::{adjust_settings, apply_filter};
use crate::{effects, EffectImages, RenderError};
use inbetween_doc::{AdjustSettings, Effects, Filter, Project};
use inbetween_raster::Image;

/// Everything the movie pass needs, borrowed from the project and the loaded effect images.
#[derive(Clone, Copy, Debug)]
pub struct MoviePass<'a> {
    pub effects: &'a Effects,
    pub images: EffectImages<'a>,
    pub adjust: &'a AdjustSettings,
    pub filters: &'a [Filter],
}

impl<'a> MoviePass<'a> {
    pub fn new(project: &'a Project, images: EffectImages<'a>) -> Self {
        Self {
            effects: &project.effects,
            images,
            adjust: &project.adjust,
            filters: &project.filters,
        }
    }

    /// Whether the pass changes anything at all.
    pub fn is_identity(&self) -> bool {
        !self.effects.any_active() && self.adjust.is_identity() && self.filters.is_empty()
    }

    /// Run the pass over `frame`. `t` is the movie's progress, 0 to 1, which drives the rays.
    pub fn apply(&self, mut frame: Image, t: f64) -> Result<Image, RenderError> {
        effects::apply(self.effects, &self.images, &mut frame, t)?;
        if !self.adjust.is_identity() {
            self.adjust.validate()?;
            frame = inbetween_raster::adjust(&frame, &adjust_settings(self.adjust))?;
        }
        for f in self.filters {
            frame = apply_filter(&frame, f)?;
        }
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Image {
        let mut img = Image::new(6, 4).unwrap();
        for y in 0..4 {
            for x in 0..6 {
                img.set(x, y, [x as f32 / 6.0, y as f32 / 4.0, 0.5, 1.0]);
            }
        }
        img
    }

    fn pass<'a>(p: &'a Project) -> MoviePass<'a> {
        MoviePass::new(p, EffectImages::default())
    }

    #[test]
    fn everything_off_is_the_identity() {
        let p = Project::default();
        assert!(pass(&p).is_identity());
        let out = pass(&p).apply(frame(), 0.5).unwrap();
        assert_eq!(out.as_slice(), frame().as_slice());
    }

    #[test]
    fn back_colour_adjust_and_filters_all_change_the_frame() {
        let mut p = Project::default();
        p.effects.back_color.enabled = true;
        p.effects.back_color.rgba = [255, 0, 0, 255];
        let mut hole = frame();
        hole.set(0, 0, [0.0; 4]);
        let out = pass(&p).apply(hole, 0.0).unwrap();
        assert_eq!(out.get(0, 0), Some([1.0, 0.0, 0.0, 1.0]));

        let mut p = Project::default();
        p.adjust.negative = true;
        let out = pass(&p).apply(frame(), 0.0).unwrap();
        assert_ne!(out.as_slice(), frame().as_slice());

        let p = Project {
            filters: vec![Filter::Blur { radius: 2 }],
            ..Project::default()
        };
        let out = pass(&p).apply(frame(), 0.0).unwrap();
        assert_ne!(out.as_slice(), frame().as_slice());
    }

    #[test]
    fn hostile_numbers_stay_finite_or_are_refused() {
        let mut p = Project::default();
        p.adjust.brightness = f64::NAN;
        assert!(pass(&p).apply(frame(), 0.0).is_err());

        let mut p = Project::default();
        p.effects.radial.enabled = true;
        p.effects.radial.intensity = f64::INFINITY;
        let out = pass(&p).apply(frame(), f64::NAN).unwrap();
        assert!(out
            .as_slice()
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
    }

    #[test]
    fn a_missing_effect_image_is_an_error() {
        let mut p = Project::default();
        p.effects.foreground.enabled = true;
        assert!(matches!(
            pass(&p).apply(frame(), 0.0),
            Err(RenderError::MissingEffectImage(_))
        ));
    }

    #[test]
    fn deterministic() {
        let mut p = Project {
            filters: vec![Filter::Gaussian { sigma: 1.0 }],
            ..Project::default()
        };
        p.effects.radial.enabled = true;
        let a = pass(&p).apply(frame(), 0.3).unwrap();
        let b = pass(&p).apply(frame(), 0.3).unwrap();
        assert_eq!(a.as_slice(), b.as_slice());
    }
}
