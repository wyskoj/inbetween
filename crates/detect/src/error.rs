use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DetectError {
    /// The input image is unusable (empty, non-finite, too large).
    BadInput(String),
    /// A model could not be loaded or produced tensors of an unexpected shape.
    Model(String),
}

impl fmt::Display for DetectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadInput(why) => write!(f, "cannot detect faces in this image: {why}"),
            Self::Model(why) => write!(f, "the face detector failed: {why}"),
        }
    }
}

impl std::error::Error for DetectError {}
