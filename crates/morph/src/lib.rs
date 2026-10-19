//! Morph planning: frame times, dot interpolation, the shared triangulation and fit math.
//!
//! This crate decides *what* each frame is made of (which source triangle lands on which
//! output triangle); `render-cpu` and `render-gpu` decide *how* to draw it.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod error;
mod fit;
mod plan;

pub use error::MorphError;
pub use fit::{fit_transform, transparent_outside};
pub use plan::{frame_t, FramePlan, MeshTri, Mix, MixMode, Planner};
