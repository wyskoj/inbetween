//! Face detection support: anchors, box decoding, letterbox and ROI geometry, and the table
//! that maps a dense face mesh onto Inbetween's named face landmarks.
//!
//! Everything here is pure maths on `geom` and `raster` types. The neural-network runtime is
//! added behind the same API; "no face found" is an ordinary empty result, never a panic.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod anchors;
mod detector;
mod error;
pub mod geometry;
pub mod mesh_map;

pub use detector::{Bounds, Detector, Face, MAX_FACES, MAX_SIDE, MESH_POINTS, MIN_SCORE};
pub use error::DetectError;
