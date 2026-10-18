use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RasterError {
    /// Width or height is zero, or the pixel count exceeds [`crate::MAX_PIXELS`].
    InvalidSize { width: u32, height: u32 },
    /// A pixel buffer does not match `width * height * channels`.
    BufferMismatch { expected: usize, got: usize },
    /// Two images that must match in size do not.
    SizeMismatch { left: (u32, u32), right: (u32, u32) },
}

impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize { width, height } => write!(
                f,
                "image size {width} x {height} is not allowed (each side must be at least 1 and the total at most {} pixels)",
                crate::MAX_PIXELS
            ),
            Self::BufferMismatch { expected, got } => {
                write!(f, "pixel buffer has {got} values, expected {expected}")
            }
            Self::SizeMismatch { left, right } => write!(
                f,
                "image sizes differ: {} x {} vs {} x {}",
                left.0, left.1, right.0, right.1
            ),
        }
    }
}

impl std::error::Error for RasterError {}
