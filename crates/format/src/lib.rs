//! The `.inbetween` project file and the `.inbetweendots` dot-set file.
//!
//! Both are JSON wrapped in a small envelope that names the format and its version:
//!
//! ```json
//! { "format": "inbetween-project", "version": 1, "project": { ... } }
//! ```
//!
//! Compatibility rules:
//! - New optional fields never bump the version. Every struct uses `#[serde(default)]`, so
//!   an older file simply lacks them, and unknown fields in a newer file are ignored.
//! - A breaking change bumps the version. Files with a higher version than this build
//!   knows are refused with a message saying so, never half-read.
//!
//! Reading never panics: bad JSON, wrong shapes, absurd sizes and unsafe paths all come
//! back as [`FormatError`].
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod camera;
mod dots;
mod error;
mod face;
mod paths;
mod project;

pub use camera::{camera_from_bytes, camera_to_string, CAMERA_FORMAT, CAMERA_VERSION};
pub use dots::{dots_from_bytes, dots_from_str, dots_to_string, DOTS_FORMAT, DOTS_VERSION};
pub use error::FormatError;
pub use face::{
    face_model_from_bytes, face_model_to_string, landmarks_from_bytes, landmarks_to_string,
    mix_recipe_from_bytes, mix_recipe_to_string, MixRecipe, FACE_FILE_VERSION, FACE_MODEL_FORMAT,
    LANDMARKS_FORMAT, MAX_LANDMARKS, MAX_MIX_INPUTS, MIX_FORMAT,
};
pub use paths::{check_image_path, make_relative, MAX_PATH_CHARS};
pub use project::{
    project_from_bytes, project_from_str, project_to_string, project_to_string_with_thumbnail,
    thumbnail_from_bytes, Thumbnail, MAX_FILE_BYTES, MAX_SEQUENCES, MAX_THUMBNAIL_PIXELS,
    MAX_THUMBNAIL_PNG_BYTES, PROJECT_FORMAT, PROJECT_VERSION,
};

#[cfg(not(target_arch = "wasm32"))]
mod files;
#[cfg(not(target_arch = "wasm32"))]
pub use files::{
    read_camera, read_dots, read_face_model, read_landmarks, read_mix_recipe, read_project,
    read_thumbnail, write_camera, write_dots, write_face_model, write_landmarks, write_mix_recipe,
    write_project, write_project_with_thumbnail,
};

/// Strip a UTF-8 byte-order mark and decode, so files saved by editors that add one load.
pub(crate) fn decode_text(bytes: &[u8], limit: usize) -> Result<&str, FormatError> {
    if bytes.len() > limit {
        return Err(FormatError::TooLarge {
            bytes: bytes.len(),
            max: limit,
        });
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    std::str::from_utf8(bytes).map_err(|e| FormatError::Parse(format!("not valid UTF-8: {e}")))
}

/// What every envelope starts with, read first so a wrong or newer file gets a clear
/// message instead of a confusing shape error.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
pub(crate) struct Header {
    format: Option<String>,
    version: Option<u32>,
}

pub(crate) fn check_header(
    text: &str,
    expected: &'static str,
    max_version: u32,
) -> Result<(), FormatError> {
    let header: Header = serde_json::from_str(text).map_err(parse_error)?;
    match header.format.as_deref() {
        Some(f) if f == expected => {}
        Some(other) => {
            return Err(FormatError::WrongFormat {
                expected,
                found: Some(other.to_string()),
            })
        }
        None => {
            return Err(FormatError::WrongFormat {
                expected,
                found: None,
            })
        }
    }
    match header.version {
        Some(v) if (1..=max_version).contains(&v) => Ok(()),
        Some(v) if v > max_version => Err(FormatError::NewerVersion {
            found: v,
            max: max_version,
        }),
        Some(v) => Err(FormatError::BadVersion(i64::from(v))),
        None => Err(FormatError::Parse("missing \"version\"".into())),
    }
}

pub(crate) fn parse_error(e: serde_json::Error) -> FormatError {
    FormatError::Parse(format!("line {}, column {}: {e}", e.line(), e.column()))
}
