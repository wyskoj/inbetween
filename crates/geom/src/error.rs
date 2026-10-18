use std::fmt;

/// Everything that can go wrong in geometry code.
#[derive(Clone, Debug, PartialEq)]
pub enum GeomError {
    /// A triangle has (almost) zero area, so no affine map or barycentric frame exists.
    DegenerateTriangle,
    /// A coordinate is NaN or infinite. Carries the dot index when known.
    NonFinite(Option<usize>),
    /// A dot's coordinates are too large or too small in magnitude to triangulate.
    OutOfRange(usize),
    /// Two dots sit at the same position.
    DuplicateDot { first: usize, second: usize },
    /// The output size is not a positive finite number.
    InvalidSize { width: f64, height: f64 },
    /// More dots than [`crate::MAX_DOTS`].
    TooManyDots { count: usize, max: usize },
    /// The triangulation backend failed unexpectedly.
    Internal(String),
}

impl fmt::Display for GeomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DegenerateTriangle => write!(f, "triangle has zero area"),
            Self::NonFinite(Some(i)) => write!(f, "dot {i} has a NaN or infinite coordinate"),
            Self::NonFinite(None) => write!(f, "a coordinate is NaN or infinite"),
            Self::OutOfRange(i) => {
                write!(
                    f,
                    "dot {i} is too far from the origin (or too close to it) to triangulate"
                )
            }
            Self::DuplicateDot { first, second } => {
                write!(
                    f,
                    "dots {first} and {second} are at the same position; move or delete one"
                )
            }
            Self::InvalidSize { width, height } => {
                write!(
                    f,
                    "output size {width} x {height} must be positive and finite"
                )
            }
            Self::TooManyDots { count, max } => {
                write!(f, "{count} dots exceeds the limit of {max}")
            }
            Self::Internal(msg) => write!(f, "triangulation failed: {msg}"),
        }
    }
}

impl std::error::Error for GeomError {}
