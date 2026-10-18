//! `inbetween`: the desktop app.
//!
//! ```text
//! inbetween [PROJECT.inbetween] [--screenshot OUT.png [--frames N]] [--size WxH]
//! ```
//!
//! `--screenshot` opens the window, lets it draw N frames (default 12), saves what is on screen
//! to a PNG and exits. It is how the real GPU path is checked without a person at the keyboard.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use inbetween_ui_egui::{InbetweenApp, ScreenshotPlan, ViewState};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

struct Args {
    project: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    frames: u32,
    size: (f32, f32),
}

const USAGE: &str =
    "usage: inbetween [PROJECT.inbetween] [--screenshot OUT.png [--frames N]] [--size WxH]";

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut out = Args {
        project: None,
        screenshot: None,
        frames: 12,
        size: (1440.0, 900.0),
    };
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .ok_or_else(|| format!("{name} needs a value\n{USAGE}"))
        };
        match arg.as_str() {
            "--screenshot" => out.screenshot = Some(PathBuf::from(value("--screenshot")?)),
            "--frames" => {
                out.frames = value("--frames")?
                    .parse()
                    .map_err(|_| format!("--frames must be a whole number\n{USAGE}"))?;
            }
            "--size" => {
                let v = value("--size")?;
                let (w, h) = v
                    .split_once('x')
                    .ok_or_else(|| format!("--size looks like 1440x900\n{USAGE}"))?;
                let parse = |s: &str| {
                    s.parse::<f32>()
                        .ok()
                        .filter(|n| (200.0..=8000.0).contains(n))
                };
                out.size = parse(w)
                    .zip(parse(h))
                    .ok_or_else(|| format!("--size must be between 200 and 8000\n{USAGE}"))?;
            }
            "--help" | "-h" => return Err(USAGE.to_string()),
            flag if flag.starts_with("--") => {
                return Err(format!("unknown option {flag}\n{USAGE}"))
            }
            path => {
                if out.project.replace(PathBuf::from(path)).is_some() {
                    return Err(format!("only one project file can be opened\n{USAGE}"));
                }
            }
        }
    }
    Ok(out)
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let outcome = Arc::new(AtomicU8::new(0));
    let screenshot_mode = args.screenshot.is_some();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([args.size.0, args.size.1])
            .with_min_inner_size([760.0, 520.0])
            .with_title("Inbetween"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    let shared = Arc::clone(&outcome);
    let result = eframe::run_native(
        "Inbetween",
        options,
        Box::new(move |cc| {
            let mut app = InbetweenApp::from_creation_context(cc);
            if screenshot_mode {
                // Screenshots must not depend on whatever layout the last session saved.
                app.state = ViewState::default();
            }
            if let Some(path) = &args.project {
                if let Err(e) = app.run_command(
                    "project.open",
                    &inbetween_ui_egui::json!({ "path": path.display().to_string() }),
                ) {
                    eprintln!("could not open {}: {e}", path.display());
                }
            }
            if let Some(path) = args.screenshot.clone() {
                app.request_screenshot(ScreenshotPlan::new(path, args.frames, shared));
            }
            Ok(Box::new(app))
        }),
    );
    if let Err(e) = result {
        eprintln!("could not start the window: {e}");
        return ExitCode::FAILURE;
    }
    if screenshot_mode && outcome.load(Ordering::SeqCst) != 1 {
        eprintln!("the screenshot was not saved");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
