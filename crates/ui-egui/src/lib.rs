//! The egui shell: two source panels, a live preview, dot editing, zoom and pan.
//!
//! This crate is thin. Every action that changes the project goes through an engine command,
//! and what is left here is view state (see [`state::ViewState`]) and drawing.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod actions;
pub mod app;
pub mod audio;
pub mod camera_tool;
pub mod crop;
pub mod curves;
pub mod dragdrop;
pub mod fonts;
pub mod form;
pub mod hit;
pub mod imaging;
pub mod kit;
pub mod menus;
pub mod new_two;
pub mod open_visual;
pub mod panels;
pub mod playback;
pub mod preview;
pub mod seqlist;
pub mod shortcuts;
pub mod state;
pub mod theme;
pub mod view;

pub use app::{
    FilePicker, InbetweenApp, Layout, PromptChoice, ScreenshotPlan, Status, REPLACES_PROJECT,
};
pub use preview::GpuHandle;
pub use serde_json::json;
pub use state::ViewState;
