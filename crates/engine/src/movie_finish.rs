//! What the movie does to every finished frame: its effects, colour adjustment, filters and
//! caption. The sequences render the picture; this finishes it.

use crate::CommandError;
use inbetween_doc::{AdjustSettings, Effects, Filter, Trail};
use inbetween_raster::Image;
use inbetween_render_cpu::movie_pass::MoviePass;
use inbetween_render_cpu::EffectImages;
use std::sync::Arc;

/// The movie-level settings with their images loaded. Cheap to clone (images are shared).
#[derive(Clone, Debug, Default)]
pub struct MovieFinish {
    pub(crate) effects: Effects,
    pub(crate) adjust: AdjustSettings,
    pub(crate) filters: Vec<Filter>,
    pub(crate) background: Option<Arc<Image>>,
    pub(crate) mask: Option<Arc<Image>>,
    pub(crate) foreground: Option<Arc<Image>>,
    pub(crate) light: Option<Arc<Image>>,
    /// The movie caption, drawn at the output size.
    pub(crate) caption: Option<Arc<Image>>,
}

/// Movie progress, 0 to 1, of movie frame `n` (counting from 0) in a movie of `total` frames.
pub(crate) fn movie_progress(n: u64, total: u64) -> f64 {
    if total <= 1 {
        0.0
    } else {
        (n as f64 / (total - 1) as f64).clamp(0.0, 1.0)
    }
}

impl MovieFinish {
    /// Whether anything besides the trail changes a frame.
    pub fn is_identity(&self) -> bool {
        self.caption.is_none() && self.pass_is_identity()
    }

    fn pass_is_identity(&self) -> bool {
        !self.effects.any_active() && self.adjust.is_identity() && self.filters.is_empty()
    }

    /// The movie trail, when it is switched on.
    pub fn trail(&self) -> Option<Trail> {
        Some(self.effects.trail).filter(|t| t.enabled)
    }

    /// Run the movie pass (not the trail) and lay the caption over `image`. `progress` is the
    /// movie's position, 0 to 1.
    pub fn apply(&self, image: Image, progress: f64) -> Result<Image, CommandError> {
        let mut image = if self.pass_is_identity() {
            image
        } else {
            let pass = MoviePass {
                effects: &self.effects,
                images: EffectImages {
                    background: self.background.as_deref(),
                    mask: self.mask.as_deref(),
                    foreground: self.foreground.as_deref(),
                    transition: None,
                    light: self.light.as_deref(),
                },
                adjust: &self.adjust,
                filters: &self.filters,
            };
            pass.apply(image, progress)?
        };
        if let Some(layer) = &self.caption {
            inbetween_render_cpu::caption::composite(&mut image, layer);
        }
        Ok(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_runs_zero_to_one_and_survives_short_movies() {
        assert_eq!(movie_progress(0, 5), 0.0);
        assert_eq!(movie_progress(4, 5), 1.0);
        assert_eq!(movie_progress(2, 5), 0.5);
        assert_eq!(movie_progress(0, 0), 0.0);
        assert_eq!(movie_progress(7, 1), 0.0);
        assert_eq!(movie_progress(99, 5), 1.0);
    }

    #[test]
    fn an_empty_finish_returns_the_frame_untouched() {
        let f = MovieFinish::default();
        assert!(f.is_identity() && f.trail().is_none());
        let img = Image::filled(3, 2, [0.1, 0.2, 0.3, 1.0]).unwrap();
        assert_eq!(f.apply(img.clone(), 0.5).ok(), Some(img));
    }
}
