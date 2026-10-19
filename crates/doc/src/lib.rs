//! The project model as pure data: sequences, image references, paired dot sets.
//!
//! Nothing here loads pixels or does I/O. Every struct is `serde`-ready, and fields added
//! later carry `#[serde(default)]` so older files keep loading.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod curve;
mod dots;
mod error;
mod project;
mod sequence;

pub use curve::{Curve, CurvePoint, MAX_CURVE_POINTS};
pub use dots::{DotPair, DotSet};
pub use error::DocError;
pub use project::{OutputSettings, Project, MAX_SIDE};
pub use sequence::{FitMode, ImageRef, MeshPolicy, MorphType, Sequence, MAX_FRAMES, MAX_HOLD};
