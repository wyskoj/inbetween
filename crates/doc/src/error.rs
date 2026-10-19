use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocError {
    /// Frame count is zero or above the limit.
    BadFrameCount { frames: u32, max: u32 },
    /// More hold frames than allowed at one end of a sequence.
    BadHold { hold: u32, max: u32 },
    /// More dot pairs than the triangulator accepts.
    TooManyDots { count: usize, max: usize },
    /// A dot has a NaN or infinite coordinate (the pair index).
    NonFiniteDot(usize),
    /// Output width or height is zero or above the limit.
    BadOutputSize { width: u32, height: u32 },
    /// A curve breaks the rules (the text says which).
    BadCurve(&'static str),
}

impl fmt::Display for DocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadFrameCount { frames, max } => {
                write!(f, "frame count {frames} must be between 1 and {max}")
            }
            Self::BadHold { hold, max } => {
                write!(
                    f,
                    "{hold} hold frames is too many (at most {max} at each end)"
                )
            }
            Self::TooManyDots { count, max } => {
                write!(f, "{count} dot pairs exceeds the limit of {max}")
            }
            Self::NonFiniteDot(i) => write!(f, "dot pair {i} has a NaN or infinite coordinate"),
            Self::BadCurve(why) => write!(f, "invalid curve: {why}"),
            Self::BadOutputSize { width, height } => write!(
                f,
                "output size {width} x {height} is not allowed (each side 1 to {})",
                crate::MAX_SIDE
            ),
        }
    }
}

impl std::error::Error for DocError {}
