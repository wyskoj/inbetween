use inbetween_morph::MorphError;
use inbetween_raster::RasterError;
use std::fmt;

#[derive(Debug)]
pub enum GpuError {
    /// No graphics adapter is available (no GPU, or no driver).
    NoAdapter,
    /// The adapter refused to create a device.
    Device(String),
    Morph(MorphError),
    Raster(RasterError),
    /// A source or output image is bigger than the GPU's texture limit.
    TooLarge {
        width: u32,
        height: u32,
        max: u32,
    },
    /// The morph type needs Image 2 but none was given.
    MissingImage2,
    /// Copying the result back from the GPU failed.
    Readback(String),
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter => write!(f, "no graphics adapter found; the GPU renderer needs a GPU or a software Vulkan/DirectX driver"),
            Self::Device(m) => write!(f, "could not open the GPU: {m}"),
            Self::Morph(e) => write!(f, "{e}"),
            Self::Raster(e) => write!(f, "{e}"),
            Self::TooLarge { width, height, max } => write!(
                f,
                "a {width} x {height} image is larger than this GPU's limit of {max} pixels per side"
            ),
            Self::MissingImage2 => write!(f, "this morph type needs Image 2, which is not loaded"),
            Self::Readback(m) => write!(f, "could not read the frame back from the GPU: {m}"),
        }
    }
}

impl std::error::Error for GpuError {}

impl From<MorphError> for GpuError {
    fn from(e: MorphError) -> Self {
        Self::Morph(e)
    }
}

impl From<RasterError> for GpuError {
    fn from(e: RasterError) -> Self {
        Self::Raster(e)
    }
}
