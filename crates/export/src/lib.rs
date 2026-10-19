//! Animated image and video export.
//!
//! Frames arrive as straight (non-premultiplied) RGBA8 buffers. GIF, APNG and animated WebP
//! are written in pure Rust; video goes through an `ffmpeg` the user has installed (see
//! [`video`]). Nothing here panics on hostile sizes or frame counts: every limit is checked
//! and reported as an [`ExportError`].
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod apng;
mod error;
mod gif;
mod webp;

#[cfg(not(target_arch = "wasm32"))]
pub mod video;

pub use apng::encode_apng;
pub use error::ExportError;
pub use gif::encode_gif;
pub use webp::encode_webp;

/// Most bytes of frames an animated image export may hold in memory at once.
pub const MAX_BUFFERED_BYTES: u64 = 1 << 30;

/// How many times an animation plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loop {
    Forever,
    Once,
}

/// Settings shared by the animated image formats.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationOptions {
    /// Frames per second, 1 to 60 (29.97 is fine).
    pub fps: f64,
    pub looping: Loop,
    /// Keep transparency where the format can (GIF: on or off, APNG and WebP: full alpha).
    /// When false, frames are flattened over `matte`.
    pub transparent: bool,
    /// Colour that transparent areas are flattened over when `transparent` is false.
    pub matte: [u8; 3],
}

impl Default for AnimationOptions {
    fn default() -> Self {
        Self {
            fps: 15.0,
            looping: Loop::Forever,
            transparent: true,
            matte: [255, 255, 255],
        }
    }
}

/// A checked set of equally sized frames.
#[derive(Clone, Copy, Debug)]
pub struct Frames<'a> {
    pub width: u32,
    pub height: u32,
    pub data: &'a [Vec<u8>],
}

impl<'a> Frames<'a> {
    pub fn new(width: u32, height: u32, data: &'a [Vec<u8>]) -> Result<Self, ExportError> {
        let bytes = u64::from(width)
            .saturating_mul(u64::from(height))
            .saturating_mul(4);
        if width == 0 || height == 0 || bytes > MAX_BUFFERED_BYTES {
            return Err(ExportError::BadSize { width, height });
        }
        if data.is_empty() {
            return Err(ExportError::NoFrames);
        }
        if (data.len() as u64).saturating_mul(bytes) > MAX_BUFFERED_BYTES {
            return Err(ExportError::TooMuchData {
                frames: data.len(),
                width,
                height,
            });
        }
        if data.iter().any(|f| f.len() as u64 != bytes) {
            return Err(ExportError::BadFrameLength);
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }
}

/// Check `fps` is usable: finite and 1 to 60.
pub fn check_fps(fps: f64) -> Result<f64, ExportError> {
    if fps.is_finite() && (1.0..=60.0).contains(&fps) {
        Ok(fps)
    } else {
        Err(ExportError::BadFps(fps))
    }
}

/// Composite one straight RGBA pixel over an opaque `matte`, giving an opaque RGB triple.
pub fn flatten(px: &[u8], matte: [u8; 3]) -> [u8; 3] {
    let a = u32::from(px.get(3).copied().unwrap_or(255));
    let mix = |c: usize| {
        let v = u32::from(px.get(c).copied().unwrap_or(0));
        ((v * a + u32::from(matte[c]) * (255 - a) + 127) / 255) as u8
    };
    [mix(0), mix(1), mix(2)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_blends_over_matte() {
        assert_eq!(flatten(&[10, 20, 30, 255], [255; 3]), [10, 20, 30]);
        assert_eq!(flatten(&[10, 20, 30, 0], [1, 2, 3]), [1, 2, 3]);
        assert_eq!(flatten(&[0, 0, 0, 128], [255; 3]), [127, 127, 127]);
    }

    #[test]
    fn frames_are_checked() {
        let one = vec![vec![0u8; 16]];
        assert!(Frames::new(2, 2, &one).is_ok());
        assert!(matches!(
            Frames::new(0, 2, &one),
            Err(ExportError::BadSize { .. })
        ));
        assert!(matches!(Frames::new(2, 2, &[]), Err(ExportError::NoFrames)));
        assert!(matches!(
            Frames::new(3, 2, &one),
            Err(ExportError::BadFrameLength)
        ));
        assert!(matches!(
            Frames::new(u32::MAX, u32::MAX, &one),
            Err(ExportError::BadSize { .. })
        ));
    }

    #[test]
    fn fps_limits() {
        assert!(check_fps(29.97).is_ok());
        for bad in [0.0, 0.5, 61.0, f64::NAN, f64::INFINITY, -3.0] {
            assert!(check_fps(bad).is_err(), "{bad}");
        }
    }
}
