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

mod camera;
mod curve;
mod dots;
mod effects;
mod error;
mod face;
mod project;
mod sequence;
mod track;

pub use camera::{
    Camera, CameraAspect, CameraKey, MAX_CAMERA_KEYS, MAX_OFFSET, MAX_SCALE, MIN_SCALE,
};
pub use curve::{Curve, CurvePoint, MAX_CURVE_POINTS};
pub use dots::{DotPair, DotSet};
pub use effects::{BackColor, Effects, ImageLayer, Light, Mask, Radial, Trail, Transition};
pub use error::DocError;
pub use face::{FaceDot, FaceModel, FACE_PRESETS, MAX_FACE_DOTS, MAX_FACE_GUIDES, MAX_FACE_NAME};
pub use project::{OutputSettings, Project, ProjectType, MAX_SIDE};
pub use sequence::{FitMode, ImageRef, MeshPolicy, MorphType, Sequence, MAX_FRAMES, MAX_HOLD};
pub use track::{Track, TrackPoint, MAX_TENSION, MAX_TRACK_POINTS, MAX_TRACK_REACH};
