//! Picture sources that are not plain image files: a video stands for its first or last frame.
//!
//! A sequence whose Image 1 is a video morphs from the clip's *last* frame; one whose Image 2
//! is a video morphs to the clip's *first* frame. Exports can also play the clips before and
//! after the morph (see `clips`). Decoding goes through an installed ffmpeg (D59, D64).

use crate::CommandError;
use inbetween_codecs::Rgba8Image;
use inbetween_export::source::{self, Tools, Which};
use std::path::Path;

/// Every file extension a sequence's image may have: the image formats and the video ones.
pub const SOURCE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "bmp", "tif", "tiff", "tga", "gif", "psd", "mp4", "mov", "m4v", "webm",
    "mkv", "avi",
];

/// Where ffmpeg is: the `INBETWEEN_FFMPEG` environment variable, else `PATH`.
pub(crate) fn tools() -> Tools {
    Tools {
        ffmpeg: std::env::var_os("INBETWEEN_FFMPEG")
            .filter(|v| !v.is_empty())
            .map(Into::into),
    }
}

/// The frame of the video at `path` that image number `which` uses: the last for Image 1, the
/// first for any other.
pub(crate) fn video_frame(path: &Path, which: u8) -> Result<Rgba8Image, CommandError> {
    let tools = tools();
    let info = source::probe(&tools, path)?;
    let end = if which == 1 {
        Which::Last
    } else {
        Which::First
    };
    let data = source::read_frame(&tools, path, end, &info)?;
    Ok(Rgba8Image::new(info.width, info.height, data)?)
}

/// Read a picture source: an image file, or the first frame of a video (for checking that a
/// file can be used and for its size).
pub fn read_any(path: &Path) -> Result<Rgba8Image, CommandError> {
    if source::is_video_source(&path.display().to_string()) {
        video_frame(path, 2)
    } else {
        Ok(inbetween_codecs::read_file(path)?)
    }
}
