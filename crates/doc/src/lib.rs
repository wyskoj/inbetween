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
mod caption;
mod curve;
mod dots;
mod edit;
mod effects;
mod error;
mod face;
mod project;
mod sequence;
mod sound;
mod track;

pub use camera::{
    Camera, CameraAspect, CameraKey, MAX_CAMERA_KEYS, MAX_OFFSET, MAX_SCALE, MIN_SCALE,
};
pub use caption::{Align, Caption, Shadow, MAX_CAPTION_ANCHOR, MAX_CAPTION_SIZE, MAX_CAPTION_TEXT};
pub use curve::{Curve, CurvePoint, MAX_CURVE_POINTS};
pub use dots::{DotPair, DotSet, Line, Side};
pub use edit::{
    affine_apply, affine_inv, affine_mul, AdjustSettings, Affine, Crop, Filter, Geometry,
    ImageEdit, IDENTITY, MAX_CROP_OFFSET, MAX_EDIT_PIXELS, MAX_EDIT_SIDE, MAX_FILTERS,
};
pub use effects::{BackColor, Effects, ImageLayer, Light, Mask, Radial, Trail, Transition};
pub use error::DocError;
pub use face::{FaceDot, FaceModel, FACE_PRESETS, MAX_FACE_DOTS, MAX_FACE_GUIDES, MAX_FACE_NAME};
pub use project::{OutputSettings, Project, ProjectType, DEFAULT_FPS, MAX_SIDE};
pub use sequence::{FitMode, ImageRef, MeshPolicy, MorphType, Sequence, MAX_FRAMES, MAX_HOLD};
pub use sound::{Sound, MAX_FADE};
pub use track::{Track, TrackPoint, MAX_TENSION, MAX_TRACK_POINTS, MAX_TRACK_REACH};
