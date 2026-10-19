use inbetween_codecs::CodecError;
use inbetween_doc::DocError;
use inbetween_export::ExportError;
use inbetween_format::FormatError;
use inbetween_raster::RasterError;
use inbetween_render_cpu::RenderError;
use std::fmt;

/// Everything a command can fail with. Messages are written for the person (or agent)
/// who ran the command: they say what to change.
#[derive(Clone, Debug, PartialEq)]
pub enum CommandError {
    /// No command has this id.
    UnknownCommand(String),
    /// The parameters do not match the command's schema.
    BadParams(String),
    /// The command cannot run in the current state (for example, no sequences).
    Disabled {
        id: &'static str,
        reason: &'static str,
    },
    /// `sequence` points past the end of the project.
    NoSuchSequence {
        index: usize,
        count: usize,
    },
    /// An image path is empty or the image could not be used.
    Image(String),
    /// A model limit was broken by the edit.
    Doc(DocError),
    Format(FormatError),
    Codec(CodecError),
    Raster(RasterError),
    Render(RenderError),
    Export(ExportError),
    Io(String),
    /// A command panicked. Never expected; reported instead of crashing the shell.
    Internal(String),
}

impl CommandError {
    /// A short stable code for scripts and agents.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownCommand(_) => "unknown_command",
            Self::BadParams(_) => "bad_params",
            Self::Disabled { .. } => "disabled",
            Self::NoSuchSequence { .. } => "no_such_sequence",
            Self::Image(_) => "image",
            Self::Doc(_) => "invalid_project",
            Self::Format(_) => "file_format",
            Self::Codec(_) => "image_file",
            Self::Raster(_) => "image_size",
            Self::Render(_) => "render",
            Self::Export(_) => "export",
            Self::Io(_) => "io",
            Self::Internal(_) => "internal",
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCommand(id) => {
                write!(
                    f,
                    "no command named `{id}` (run `ib commands` for the list)"
                )
            }
            Self::BadParams(m) => write!(f, "bad parameters: {m}"),
            Self::Disabled { id, reason } => write!(f, "`{id}` is unavailable: {reason}"),
            Self::NoSuchSequence { index, count } => {
                write!(
                    f,
                    "there is no sequence {index} (the project has {count}; numbering starts at 0)"
                )
            }
            Self::Image(m) => write!(f, "{m}"),
            Self::Doc(e) => write!(f, "{e}"),
            Self::Format(e) => write!(f, "{e}"),
            Self::Codec(e) => write!(f, "{e}"),
            Self::Raster(e) => write!(f, "{e}"),
            Self::Render(e) => write!(f, "{e}"),
            Self::Export(e) => write!(f, "{e}"),
            Self::Io(m) => write!(f, "file error: {m}"),
            Self::Internal(m) => write!(f, "internal error: {m}"),
        }
    }
}

impl std::error::Error for CommandError {}

macro_rules! from_error {
    ($($ty:ty => $variant:ident),* $(,)?) => {$(
        impl From<$ty> for CommandError {
            fn from(e: $ty) -> Self { Self::$variant(e) }
        }
    )*};
}

from_error! {
    DocError => Doc,
    FormatError => Format,
    CodecError => Codec,
    RasterError => Raster,
    RenderError => Render,
    ExportError => Export,
}
