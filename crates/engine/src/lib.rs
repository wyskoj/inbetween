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

mod dots_cmds;
mod edit_cmds;
mod error;
mod export_cmds;
mod params;
mod project_cmds;
mod registry;
mod sequence_cmds;
mod session;

pub use error::CommandError;
pub use params::{ParamKind, ParamSpec, Params, PathUse, MAX_STRING_BYTES};
pub use registry::{CommandInfo, CommandSpec, Registry};
pub use session::{Prepared, Session};
