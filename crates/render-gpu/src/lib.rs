//! The wgpu renderer. It must match the CPU renderer (`inbetween-render-cpu`), which is the
//! oracle, within the tolerance stated in `docs/decisions.md` (D24); the oracle is never
//! changed to match this one.
//!
//! The CPU plans the frame exactly as the oracle does (`inbetween-morph`), then turns each
//! mesh triangle into three vertices that carry the output position and the matching
//! position on each source image. The GPU rasterises the triangles in order, so later
//! triangles overwrite earlier ones as in the oracle, and the fragment shader samples the
//! source images with the same bilinear, edge-clamped, premultiplied maths.
//!
//! Source images are uploaded once per [`GpuRenderer`]; a frame costs one small vertex
//! upload and one draw call. [`GpuRenderer::draw`] leaves the result in a GPU texture for a
//! preview to show directly; [`GpuRenderer::render_t`] reads it back into an `Image`.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod context;
mod error;
mod renderer;
mod vertices;

pub use context::GpuContext;
pub use error::GpuError;
pub use renderer::GpuRenderer;
pub use vertices::{build_vertices, Vertex};
