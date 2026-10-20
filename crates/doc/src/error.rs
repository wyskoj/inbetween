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
    /// A line between dots breaks a rule (the text says which).
    BadLine(&'static str),
    /// Output width or height is zero or above the limit.
    BadOutputSize { width: u32, height: u32 },
    /// The movie's frames per second is not a number from 1 to 60.
    BadFps,
    /// A curve breaks the rules (the text says which).
    BadCurve(&'static str),
    /// An effect setting is out of range (the text says which).
    BadEffect(&'static str),
    /// A camera setting is out of range (the text says which).
    BadCamera(&'static str),
    /// A track curve setting is out of range (the text says which).
    BadTrack(&'static str),
    /// The face model breaks a rule (the text says which).
    BadFace(&'static str),
    /// An image edit setting is out of range (the text says which).
    BadEdit(&'static str),
    /// A caption setting is out of range (the text says which).
    BadCaption(&'static str),
    /// A sound setting is out of range (the text says which).
    BadSound(&'static str),
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
            Self::BadLine(why) => write!(f, "invalid line: {why}"),
            Self::BadCurve(why) => write!(f, "invalid curve: {why}"),
            Self::BadEffect(why) => write!(f, "invalid effect: {why}"),
            Self::BadCamera(why) => write!(f, "invalid camera: {why}"),
            Self::BadTrack(why) => write!(f, "invalid track: {why}"),
            Self::BadFace(why) => write!(f, "invalid face model: {why}"),
            Self::BadEdit(why) => write!(f, "invalid image edit: {why}"),
            Self::BadCaption(why) => write!(f, "invalid caption: {why}"),
            Self::BadSound(why) => write!(f, "invalid sound: {why}"),
            Self::BadFps => write!(f, "frames per second must be a number from 1 to 60"),
            Self::BadOutputSize { width, height } => write!(
                f,
                "output size {width} x {height} is not allowed (each side 1 to {})",
                crate::MAX_SIDE
            ),
        }
    }
}

impl std::error::Error for DocError {}
