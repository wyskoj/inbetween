//! The engine: a [`Session`] (the open project) and a [`Registry`] of commands.
//!
//! Every user-visible action is a command with an id, parameter schema and `enabled`
//! check. The desktop app, the `ib` CLI and MCP all drive the engine by id, so a feature
//! added here is available everywhere at once. New commands go in their own
//! `<area>_cmds.rs` module with a `specs()` function.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod anim_cmds;
mod camera_cmds;
mod caption_cmds;
mod curve_cmds;
mod dots_cmds;
mod edit_cmds;
mod effect_cmds;
mod error;
mod export_cmds;
mod face_cmds;
#[cfg(not(target_arch = "wasm32"))]
mod face_detect;
#[cfg(not(target_arch = "wasm32"))]
mod face_media;
#[cfg(not(target_arch = "wasm32"))]
mod file_cmds;
mod filter_cmds;
mod fx_cmds;
mod image_cmds;
mod layer_cmds;
mod lines_cmds;
mod list_cmds;
#[cfg(not(target_arch = "wasm32"))]
mod match_cmds;
#[cfg(not(target_arch = "wasm32"))]
mod movie;
mod movie_cmds;
mod movie_finish;
mod params;
mod project_cmds;
mod registry;
mod select_cmds;
mod sequence_cmds;
mod session;
mod state;
mod track_cmds;
mod view_cmds;

pub use error::CommandError;
pub use params::{ParamKind, ParamSpec, Params, PathUse, MAX_LIST_ENTRIES, MAX_STRING_BYTES};
pub use registry::{CommandInfo, CommandSpec, Registry};
pub use movie_finish::MovieFinish;
pub use session::{Prepared, Session};
pub use state::{DisplayPrefs, DotClip, DotColorMode, DotStyle};
