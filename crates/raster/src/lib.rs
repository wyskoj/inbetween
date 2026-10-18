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

mod blend;
mod error;
mod image;

pub use blend::{clamp_valid, lerp, over, scale, Pixel, TRANSPARENT};
pub use error::RasterError;
pub use image::{Image, MAX_PIXELS};
