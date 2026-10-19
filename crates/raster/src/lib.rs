//! RGBA images in f32 premultiplied working storage, sampling and blend math.
//!
//! Colour values are sRGB-encoded (not linearised) and premultiplied by alpha. 8-bit sRGB
//! RGBA converts in and out at the I/O boundary only. See `docs/decisions.md` (D8).
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod adjust;
mod blend;
mod error;
pub mod filter;
mod image;
mod par;
pub mod transform;

pub use adjust::{adjust, luminance, Adjust};
pub use blend::{clamp_valid, lerp, over, scale, Pixel, TRANSPARENT};
pub use error::RasterError;
pub use image::{Image, MAX_PIXELS};
