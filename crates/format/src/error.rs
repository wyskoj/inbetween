use inbetween_doc::DocError;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum FormatError {
    /// Reading or writing the file failed (missing, no permission, disk full, ...).
    Io(String),
    /// The text is not valid JSON, or does not have the expected shape.
    Parse(String),
    /// The JSON is fine but it is not this kind of Inbetween file.
    WrongFormat {
        expected: &'static str,
        found: Option<String>,
    },
    /// Saved by a newer Inbetween than this build understands.
    NewerVersion { found: u32, max: u32 },
    /// A version number below 1.
    BadVersion(i64),
    /// The file is larger than the reader accepts.
    TooLarge { bytes: usize, max: usize },
    /// The content breaks a model limit (frame count, dot count, output size...).
    Invalid(DocError),
    /// A project holds more sequences than the reader accepts.
    TooManySequences { count: usize, max: usize },
    /// An image path is absolute or otherwise not portable.
    BadPath { path: String, reason: &'static str },
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(m) => write!(f, "file error: {m}"),
            Self::Parse(m) => write!(f, "could not read the file: {m}"),
            Self::WrongFormat { expected, found: Some(found) } => {
                write!(f, "this is a \"{found}\" file, expected \"{expected}\"")
            }
            Self::WrongFormat { expected, found: None } => {
                write!(f, "this does not look like an Inbetween file (no \"{expected}\" marker)")
            }
            Self::NewerVersion { found, max } => write!(
                f,
                "this file was saved by a newer Inbetween (file version {found}, this build reads up to {max}); update Inbetween to open it"
            ),
            Self::BadVersion(v) => write!(f, "invalid file version {v}"),
            Self::TooLarge { bytes, max } => {
                write!(f, "file is {bytes} bytes; the limit is {max}")
            }
            Self::Invalid(e) => write!(f, "invalid content: {e}"),
            Self::TooManySequences { count, max } => {
                write!(f, "{count} sequences exceeds the limit of {max}")
            }
            Self::BadPath { path, reason } => {
                write!(f, "image path \"{path}\" is not allowed: {reason}")
            }
        }
    }
}

impl std::error::Error for FormatError {}

impl From<DocError> for FormatError {
    fn from(e: DocError) -> Self {
        Self::Invalid(e)
    }
}
