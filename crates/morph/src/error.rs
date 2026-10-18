use inbetween_doc::DocError;
use inbetween_geom::GeomError;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum MorphError {
    /// The sequence or output settings break a limit.
    Doc(DocError),
    /// Two dots sit at the same position on one image (`image` is 1 or 2).
    DuplicateDot {
        image: u8,
        first: usize,
        second: usize,
    },
    /// Triangulation failed for another reason.
    Geom(GeomError),
    /// Output or source size is zero or not finite.
    BadSize,
}

impl fmt::Display for MorphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Doc(e) => write!(f, "{e}"),
            Self::DuplicateDot {
                image,
                first,
                second,
            } => write!(
                f,
                "dots {} and {} are at the same position on Image {image}; move or delete one",
                first + 1,
                second + 1
            ),
            Self::Geom(e) => write!(f, "{e}"),
            Self::BadSize => write!(f, "image and output sizes must be positive"),
        }
    }
}

impl std::error::Error for MorphError {}

impl From<DocError> for MorphError {
    fn from(e: DocError) -> Self {
        Self::Doc(e)
    }
}
