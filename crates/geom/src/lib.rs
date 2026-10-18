//! Points, triangles, barycentric and affine math, and Delaunay triangulation.
//!
//! All geometry is `f64`. Degenerate input (duplicate dots, zero-area triangles, non-finite
//! numbers) is reported through [`GeomError`] and never panics.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod affine;
mod bounds;
mod delaunay;
mod error;
mod point;
mod triangle;

pub use affine::Affine;
pub use bounds::Bounds;
pub use delaunay::{triangulate, Triangulation, MAX_DOTS};
pub use error::GeomError;
pub use point::Point;
pub use triangle::Triangle;
