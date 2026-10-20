//! The movie's sound track: one audio file played under the movie.

use crate::{DocError, ImageRef};
use serde::{Deserialize, Serialize};

/// Longest fade in or out, in seconds.
pub const MAX_FADE: f64 = 60.0;

/// An audio file laid under the whole movie. Exports that carry sound mux it in; the shell
/// plays it along with the preview.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sound {
    pub enabled: bool,
    /// A `.wav` or `.mp3` file. (`ImageRef` is the project's general file reference, so save,
    /// rebase and append treat it like any other path.)
    pub path: ImageRef,
    /// Repeat the audio when it is shorter than the movie.
    pub looped: bool,
    /// Stop the audio where the movie ends when it is longer than the movie.
    pub cut_at_end: bool,
    /// Seconds of fade-in at the start, 0 to 60.
    pub fade_in: f64,
    /// Seconds of fade-out at the end, 0 to 60.
    pub fade_out: f64,
}

impl Default for Sound {
    fn default() -> Self {
        Self {
            enabled: true,
            path: ImageRef::default(),
            looped: false,
            cut_at_end: true,
            fade_in: 0.0,
            fade_out: 0.0,
        }
    }
}

impl Sound {
    pub fn validate(&self) -> Result<(), DocError> {
        for v in [self.fade_in, self.fade_out] {
            if !(v.is_finite() && (0.0..=MAX_FADE).contains(&v)) {
                return Err(DocError::BadSound("fades must be 0 to 60 seconds"));
            }
        }
        Ok(())
    }

    /// Whether the sound is on and has a file.
    pub fn is_active(&self) -> bool {
        self.enabled && !self.path.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fades_are_checked() {
        assert!(Sound::default().validate().is_ok());
        for bad in [f64::NAN, f64::INFINITY, -1.0, 60.5] {
            let s = Sound {
                fade_in: bad,
                ..Sound::default()
            };
            assert!(s.validate().is_err(), "{bad}");
            let s = Sound {
                fade_out: bad,
                ..Sound::default()
            };
            assert!(s.validate().is_err(), "{bad}");
        }
    }

    #[test]
    fn needs_a_path_to_be_active() {
        let mut s = Sound::default();
        assert!(!s.is_active());
        s.path = ImageRef::new("a.wav");
        assert!(s.is_active());
        s.enabled = false;
        assert!(!s.is_active());
    }
}
