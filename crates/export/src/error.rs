use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum ExportError {
    NoFrames,
    BadSize {
        width: u32,
        height: u32,
    },
    BadFrameLength,
    BadFps(f64),
    /// The frames would not fit in memory (the limit is [`crate::MAX_BUFFERED_BYTES`]).
    TooMuchData {
        frames: usize,
        width: u32,
        height: u32,
    },
    /// The format cannot hold frames this large.
    TooLarge {
        format: &'static str,
        max: u32,
    },
    Encode(String),
    /// The `ffmpeg` program could not be started or failed.
    Video(String),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoFrames => write!(f, "there are no frames to export"),
            Self::BadSize { width, height } => {
                write!(f, "a {width} x {height} frame cannot be exported")
            }
            Self::BadFrameLength => write!(f, "a frame's pixel data does not match the frame size"),
            Self::BadFps(v) => write!(f, "frame rate {v} must be between 1 and 60"),
            Self::TooMuchData {
                frames,
                width,
                height,
            } => write!(
                f,
                "{frames} frames of {width} x {height} are too much to hold in memory for an animated image; export fewer frames, shrink the output, or use export.video"
            ),
            Self::TooLarge { format, max } => {
                write!(f, "{format} frames can be at most {max} pixels on a side")
            }
            Self::Encode(m) => write!(f, "could not encode the animation: {m}"),
            Self::Video(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ExportError {}
