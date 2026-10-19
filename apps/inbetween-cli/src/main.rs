//! `ib`: render, inspect and drive Inbetween projects without a window.
//!
//! This is a thin shell. Every subcommand opens a [`Session`] and runs engine commands by
//! id, exactly as the desktop app and MCP do.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod text;

use clap::{Parser, Subcommand};
use inbetween_engine::{Registry, Session};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "ib",
    version,
    about = "Inbetween: image morphing from the command line"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Render every frame of a sequence to numbered PNG files
    Render {
        /// The .inbetween project file
        project: PathBuf,
        /// Folder for the frames (created if missing)
        #[arg(long)]
        out: PathBuf,
        /// Render only this sequence (numbered from 0); by default every visible sequence is
        /// rendered one after another
        #[arg(long)]
        sequence: Option<u32>,
        /// File name prefix: PREFIX_0001.png, ...
        #[arg(long)]
        prefix: Option<String>,
    },
    /// Render one frame to an image file (.png or .jpg)
    Frame {
        /// The .inbetween project file
        project: PathBuf,
        /// Image file to write
        #[arg(long)]
        out: PathBuf,
        /// Morph time: 0 is Image 1, 1 is Image 2
        #[arg(long, conflicts_with = "frame")]
        t: Option<f64>,
        /// Frame number, starting at 0
        #[arg(long)]
        frame: Option<u32>,
        /// Sequence number, starting at 0
        #[arg(long, default_value_t = 0)]
        sequence: u32,
    },
    /// Show a project's sequences, sizes and dot counts
    Info {
        project: PathBuf,
        /// Print machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// List every command with its parameters
    Commands {
        /// Print machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Run any command by id, with JSON parameters
    Run {
        /// Command id, e.g. dots.export
        id: String,
        /// JSON object of parameters
        #[arg(long)]
        params: Option<String>,
        /// Open this project first
        #[arg(long)]
        project: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    // clap exits with status 2 for usage errors and prints --help / --version itself.
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

struct Engine {
    registry: Registry,
    session: Session,
}

impl Engine {
    fn new() -> Self {
        Self {
            registry: Registry::standard(),
            session: Session::new(),
        }
    }

    fn run(&mut self, id: &str, params: Value) -> Result<Value, String> {
        self.registry
            .run_caught(&mut self.session, id, &params)
            .map_err(|e| e.to_string())
    }

    fn open(&mut self, project: &Path) -> Result<(), String> {
        self.run("project.open", json!({ "path": path_str(project)? }))
            .map(drop)
    }
}

fn path_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("{} is not a valid UTF-8 path", path.display()))
}

fn run(command: Cmd) -> Result<String, String> {
    let mut engine = Engine::new();
    match command {
        Cmd::Render {
            project,
            out,
            sequence,
            prefix,
        } => {
            engine.open(&project)?;
            let mut params = json!({ "out_dir": path_str(&out)? });
            if let Some(prefix) = prefix {
                params["prefix"] = Value::from(prefix);
            }
            let id = match sequence {
                Some(n) => {
                    params["sequence"] = json!(n);
                    "export.sequence"
                }
                None => "export.movie",
            };
            let r = engine.run(id, params)?;
            Ok(text::render_summary(&r))
        }
        Cmd::Frame {
            project,
            out,
            t,
            frame,
            sequence,
        } => {
            engine.open(&project)?;
            let mut params = json!({ "out": path_str(&out)?, "sequence": sequence });
            if let Some(t) = t {
                params["t"] = json!(t);
            }
            if let Some(frame) = frame {
                params["frame"] = json!(frame);
            }
            let r = engine.run("export.frame", params)?;
            Ok(text::frame_summary(&r))
        }
        Cmd::Info { project, json } => {
            engine.open(&project)?;
            let info = engine.run("project.info", Value::Null)?;
            if json {
                pretty(&info)
            } else {
                Ok(text::info(&info))
            }
        }
        Cmd::Commands { json } => {
            let described = engine.registry.describe();
            if json {
                pretty(&described)
            } else {
                Ok(text::commands(&described))
            }
        }
        Cmd::Run {
            id,
            params,
            project,
        } => {
            if let Some(project) = project {
                engine.open(&project)?;
            }
            let params = match params {
                Some(text) => serde_json::from_str(&text)
                    .map_err(|e| format!("--params is not valid JSON: {e}"))?,
                None => Value::Null,
            };
            pretty(&engine.run(&id, params)?)
        }
    }
}

fn pretty<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| format!("could not format output: {e}"))
}
