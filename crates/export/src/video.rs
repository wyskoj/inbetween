//! Video export through an installed `ffmpeg`.
//!
//! Inbetween does not bundle a video encoder: H.264 and friends carry patent and licence
//! baggage the project avoids. Instead frames are piped as raw RGBA to the user's own
//! `ffmpeg` (found on `PATH`, or given explicitly), which does the encoding.

use crate::{check_fps, flatten, ExportError};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread::JoinHandle;

/// A video container and codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoFormat {
    /// H.264 in MP4. No alpha: frames are flattened over the matte colour.
    Mp4,
    /// VP9 in WebM, with alpha.
    WebM,
    /// ProRes 4444 in MOV, with alpha.
    ProRes,
}

impl VideoFormat {
    pub const ALL: [Self; 3] = [Self::Mp4, Self::WebM, Self::ProRes];

    pub fn id(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::WebM => "webm",
            Self::ProRes => "prores",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id() == id)
    }

    pub fn has_alpha(self) -> bool {
        !matches!(self, Self::Mp4)
    }

    /// The file extension a file of this format should have.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::WebM => "webm",
            Self::ProRes => "mov",
        }
    }

    /// The `ffmpeg` arguments after the input, before the output path.
    fn encoder_args(self) -> &'static [&'static str] {
        match self {
            Self::Mp4 => &[
                "-vf",
                // H.264 in yuv420p needs even sides.
                "scale=trunc(iw/2)*2:trunc(ih/2)*2",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-crf",
                "18",
                "-movflags",
                "+faststart",
            ],
            Self::WebM => &[
                "-c:v",
                "libvpx-vp9",
                "-pix_fmt",
                "yuva420p",
                "-crf",
                "28",
                "-b:v",
                "0",
                "-auto-alt-ref",
                "0",
            ],
            Self::ProRes => &[
                "-c:v",
                "prores_ks",
                "-profile:v",
                "4444",
                "-pix_fmt",
                "yuva444p10le",
            ],
        }
    }
}

#[derive(Clone, Debug)]
pub struct VideoOptions {
    pub format: VideoFormat,
    pub fps: f64,
    /// Colour that transparency is flattened over for formats without alpha.
    pub matte: [u8; 3],
    /// The `ffmpeg` program; `None` looks for `ffmpeg` on `PATH`.
    pub ffmpeg: Option<PathBuf>,
}

/// An `ffmpeg` process taking frames on its standard input.
pub struct VideoWriter {
    child: Child,
    stdin: Option<ChildStdin>,
    stderr: Option<JoinHandle<String>>,
    width: u32,
    height: u32,
    frame_bytes: usize,
    flatten: Option<[u8; 3]>,
    program: String,
}

impl VideoWriter {
    /// Start `ffmpeg` writing `out` (overwritten if it exists).
    pub fn start(
        width: u32,
        height: u32,
        out: &Path,
        opts: &VideoOptions,
    ) -> Result<Self, ExportError> {
        let fps = check_fps(opts.fps)?;
        let frame_bytes = usize::try_from(
            u64::from(width)
                .saturating_mul(u64::from(height))
                .saturating_mul(4),
        )
        .ok()
        .filter(|_| width > 0 && height > 0)
        .ok_or(ExportError::BadSize { width, height })?;
        let program = opts
            .ffmpeg
            .as_deref()
            .map_or_else(|| "ffmpeg".to_string(), |p| p.display().to_string());
        let mut cmd = Command::new(&program);
        cmd.args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgba"])
            .args(["-s", &format!("{width}x{height}")])
            .args(["-r", &format!("{fps}")])
            .args(["-i", "-", "-an"])
            .args(opts.format.encoder_args())
            .arg(out)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| {
            ExportError::Video(if e.kind() == std::io::ErrorKind::NotFound {
                format!(
                    "video export needs ffmpeg, which was not found ({program}); install it and put it on PATH, or pass its location in `ffmpeg`"
                )
            } else {
                format!("could not start {program}: {e}")
            })
        })?;
        let stdin = child.stdin.take();
        // Drain stderr on a thread so a chatty encoder can never block on a full pipe.
        let stderr = child.stderr.take().map(|mut pipe| {
            std::thread::spawn(move || {
                let mut text = String::new();
                let mut bytes = Vec::new();
                if pipe.read_to_end(&mut bytes).is_ok() {
                    text = String::from_utf8_lossy(&bytes).trim().to_string();
                }
                text
            })
        });
        Ok(Self {
            child,
            stdin,
            stderr,
            width,
            height,
            frame_bytes,
            flatten: (!opts.format.has_alpha()).then_some(opts.matte),
            program,
        })
    }

    /// Send one straight-alpha RGBA8 frame.
    pub fn push(&mut self, rgba: &[u8]) -> Result<(), ExportError> {
        if rgba.len() != self.frame_bytes {
            return Err(ExportError::BadFrameLength);
        }
        let flat;
        let bytes = match self.flatten {
            Some(matte) => {
                let mut v = Vec::with_capacity(rgba.len());
                for px in rgba.chunks_exact(4) {
                    let [r, g, b] = flatten(px, matte);
                    v.extend_from_slice(&[r, g, b, 255]);
                }
                flat = v;
                &flat[..]
            }
            None => rgba,
        };
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(ExportError::Video(
                "the video encoder already closed".into(),
            ));
        };
        if let Err(e) = stdin.write_all(bytes) {
            // The encoder quit early; its message says why.
            return Err(self.fail(&format!("could not send a frame ({e})")));
        }
        Ok(())
    }

    /// Close the input and wait for `ffmpeg` to finish writing the file.
    pub fn finish(mut self) -> Result<(), ExportError> {
        drop(self.stdin.take());
        let status = self
            .child
            .wait()
            .map_err(|e| ExportError::Video(format!("could not wait for {}: {e}", self.program)))?;
        let log = self.take_log();
        if status.success() {
            Ok(())
        } else {
            Err(ExportError::Video(format!(
                "{} failed ({status}){}",
                self.program,
                if log.is_empty() {
                    String::new()
                } else {
                    format!(": {log}")
                }
            )))
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn take_log(&mut self) -> String {
        self.stderr
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default()
    }

    fn fail(&mut self, what: &str) -> ExportError {
        drop(self.stdin.take());
        let _ = self.child.wait();
        let log = self.take_log();
        ExportError::Video(if log.is_empty() {
            format!("{} stopped early: {what}", self.program)
        } else {
            format!("{} stopped early: {log}", self.program)
        })
    }
}

impl Drop for VideoWriter {
    fn drop(&mut self) {
        // Abandoned writer (an error elsewhere): do not leave an encoder running.
        if self.stdin.is_some() {
            drop(self.stdin.take());
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_ids_round_trip() {
        for f in VideoFormat::ALL {
            assert_eq!(VideoFormat::from_id(f.id()), Some(f));
        }
        assert_eq!(VideoFormat::from_id("avi"), None);
    }

    #[test]
    fn missing_program_is_an_actionable_error() {
        let opts = VideoOptions {
            format: VideoFormat::Mp4,
            fps: 24.0,
            matte: [0; 3],
            ffmpeg: Some(PathBuf::from("definitely-not-a-real-ffmpeg-binary")),
        };
        let err = VideoWriter::start(4, 4, Path::new("out.mp4"), &opts).err();
        let text = err.map(|e| e.to_string()).unwrap_or_default();
        assert!(text.contains("ffmpeg"), "{text}");
    }

    #[test]
    fn bad_sizes_and_rates_are_refused_before_spawning() {
        let mut opts = VideoOptions {
            format: VideoFormat::WebM,
            fps: 24.0,
            matte: [0; 3],
            ffmpeg: Some(PathBuf::from("definitely-not-a-real-ffmpeg-binary")),
        };
        assert!(matches!(
            VideoWriter::start(0, 4, Path::new("o.webm"), &opts),
            Err(ExportError::BadSize { .. })
        ));
        opts.fps = f64::NAN;
        assert!(matches!(
            VideoWriter::start(4, 4, Path::new("o.webm"), &opts),
            Err(ExportError::BadFps(_))
        ));
    }
}
