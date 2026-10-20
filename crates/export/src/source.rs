//! Video files as picture sources: read the first or last frame of a clip for a morph to start
//! from or end on, and stream a whole clip, fitted to the movie's frame, to put before or after
//! the morph in an export.
//!
//! Decoding is done by an installed `ffmpeg` and `ffprobe` (see D59): the program is started
//! with `-f rawvideo`, and the frames are read from its output. Nothing here panics on odd
//! files: a clip that cannot be read comes back as an [`ExportError::Video`] saying why.

use crate::ExportError;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::thread::JoinHandle;

/// File extensions a sequence's image may have to be a video.
pub const VIDEO_SOURCE_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "webm", "mkv", "avi"];

/// Most pixels a decoded frame may have (so a hostile file cannot ask for gigabytes).
pub const MAX_FRAME_PIXELS: u64 = 1 << 28;

/// Whether `path` names a video by its extension.
pub fn is_video_source(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| {
            VIDEO_SOURCE_EXTENSIONS
                .iter()
                .any(|x| x.eq_ignore_ascii_case(e))
        })
}

/// Where the programs are.
#[derive(Clone, Debug, Default)]
pub struct Tools {
    /// The `ffmpeg` program; `None` looks for `ffmpeg` on `PATH`.
    pub ffmpeg: Option<PathBuf>,
}

impl Tools {
    fn ffmpeg(&self) -> String {
        self.ffmpeg
            .as_deref()
            .map_or_else(|| "ffmpeg".to_string(), |p| p.display().to_string())
    }

    /// `ffprobe` beside a given `ffmpeg`, or on `PATH`.
    fn ffprobe(&self) -> String {
        match self.ffmpeg.as_deref() {
            Some(p) => {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("ffmpeg");
                let probe = name.replacen("ffmpeg", "ffprobe", 1);
                p.with_file_name(probe).display().to_string()
            }
            None => "ffprobe".to_string(),
        }
    }
}

fn start_error(program: &str, e: &std::io::Error) -> ExportError {
    ExportError::Video(if e.kind() == std::io::ErrorKind::NotFound {
        format!(
            "using a video as a picture needs ffmpeg, which was not found ({program}); install it and put it on PATH"
        )
    } else {
        format!("could not start {program}: {e}")
    })
}

/// What a clip is like.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub seconds: f64,
}

impl VideoInfo {
    /// How many frames the clip has when played at `fps` frames per second.
    pub fn frames_at(&self, fps: f64) -> u64 {
        if !(fps.is_finite() && fps > 0.0 && self.seconds.is_finite() && self.seconds > 0.0) {
            return 0;
        }
        (self.seconds * fps).round().max(1.0) as u64
    }
}

/// `"30000/1001"` or `"25"` as a rate.
fn parse_rate(text: &str) -> Option<f64> {
    match text.split_once('/') {
        Some((n, d)) => {
            let (n, d): (f64, f64) = (n.trim().parse().ok()?, d.trim().parse().ok()?);
            (d != 0.0 && n.is_finite()).then_some(n / d)
        }
        None => text.trim().parse().ok(),
    }
}

/// Read size, rate and length of the first video stream of `path`.
pub fn probe(tools: &Tools, path: &Path) -> Result<VideoInfo, ExportError> {
    let program = tools.ffprobe();
    let out = Command::new(&program)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate,duration:format=duration",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| start_error(&program, &e))?;
    if !out.status.success() {
        return Err(ExportError::Video(format!(
            "{} could not read {}: {}",
            program,
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut info = (None, None, None, None, None);
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "width" => info.0 = value.trim().parse::<u32>().ok(),
            "height" => info.1 = value.trim().parse::<u32>().ok(),
            "r_frame_rate" => info.2 = parse_rate(value),
            // The stream's own length wins over the container's when both are given.
            "duration" => {
                let v = value.trim().parse::<f64>().ok().filter(|v| v.is_finite());
                if info.3.is_none() {
                    info.3 = v;
                } else if info.4.is_none() {
                    info.4 = v;
                }
            }
            _ => {}
        }
    }
    let (Some(width), Some(height)) = (info.0, info.1) else {
        return Err(ExportError::Video(format!(
            "{} has no video picture",
            path.display()
        )));
    };
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_FRAME_PIXELS {
        return Err(ExportError::BadSize { width, height });
    }
    let seconds = info.3.or(info.4).filter(|s| *s > 0.0).ok_or_else(|| {
        ExportError::Video(format!("could not tell how long {} is", path.display()))
    })?;
    Ok(VideoInfo {
        width,
        height,
        fps: info.2.filter(|f| *f > 0.0).unwrap_or(25.0),
        seconds,
    })
}

/// Which end of a clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    First,
    Last,
}

fn frame_bytes(width: u32, height: u32) -> Result<usize, ExportError> {
    let px = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || px > MAX_FRAME_PIXELS {
        return Err(ExportError::BadSize { width, height });
    }
    usize::try_from(px * 4).map_err(|_| ExportError::BadSize { width, height })
}

/// Drain a pipe on a thread so a chatty program never blocks on a full one.
fn drain(mut pipe: impl Read + Send + 'static) -> JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).trim().to_string()
    })
}

/// Read as many bytes as there are, up to `buf.len()`; returns how many.
fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match r.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(got)
}

/// The first or last frame of the clip at its own size, as straight RGBA8.
pub fn read_frame(
    tools: &Tools,
    path: &Path,
    which: Which,
    info: &VideoInfo,
) -> Result<Vec<u8>, ExportError> {
    let size = frame_bytes(info.width, info.height)?;
    let program = tools.ffmpeg();
    let mut cmd = Command::new(&program);
    cmd.args(["-v", "error"]);
    if which == Which::Last {
        // Only the last second is decoded; the last frame of it is the clip's.
        cmd.args(["-sseof", "-1"]);
    }
    cmd.arg("-i")
        .arg(path)
        .args(["-an", "-sn", "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| start_error(&program, &e))?;
    let log = child.stderr.take().map(drain);
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| ExportError::Video("could not read from ffmpeg".into()))?;
    let mut latest: Option<Vec<u8>> = None;
    let mut buf = vec![0u8; size];
    loop {
        let got = read_full(&mut stdout, &mut buf)
            .map_err(|e| ExportError::Video(format!("could not read from ffmpeg: {e}")))?;
        if got < size {
            break;
        }
        latest = Some(buf.clone());
        if which == Which::First {
            break;
        }
    }
    drop(stdout);
    if which == Which::First {
        let _ = child.kill();
    }
    let _ = child.wait();
    let log = log.and_then(|h| h.join().ok()).unwrap_or_default();
    latest.ok_or_else(|| {
        ExportError::Video(format!(
            "{} gave no frames{}",
            path.display(),
            if log.is_empty() {
                String::new()
            } else {
                format!(": {log}")
            }
        ))
    })
}

/// How a clip's picture is fitted to the movie's frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Stretched to the frame.
    Stretch,
    /// Whole picture inside the frame, transparent bars around it.
    Contain,
    /// Frame filled, the overflow cropped.
    Cover,
}

impl Fit {
    fn filter(self, width: u32, height: u32) -> String {
        match self {
            Self::Stretch => format!("scale={width}:{height}"),
            Self::Contain => format!(
                "scale={width}:{height}:force_original_aspect_ratio=decrease,format=rgba,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=black@0"
            ),
            Self::Cover => format!(
                "scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height}"
            ),
        }
    }
}

/// A clip being decoded, one frame at a time, at the movie's size and frame rate.
pub struct ClipReader {
    child: Child,
    stdout: Option<ChildStdout>,
    log: Option<JoinHandle<String>>,
    size: usize,
    path: PathBuf,
}

impl ClipReader {
    pub fn start(
        tools: &Tools,
        path: &Path,
        width: u32,
        height: u32,
        fps: f64,
        fit: Fit,
    ) -> Result<Self, ExportError> {
        let size = frame_bytes(width, height)?;
        let fps = crate::check_fps(fps)?;
        let program = tools.ffmpeg();
        let filter = format!("fps={fps},{},format=rgba", fit.filter(width, height));
        let mut child = Command::new(&program)
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-an", "-sn", "-vf", &filter])
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| start_error(&program, &e))?;
        let log = child.stderr.take().map(drain);
        let stdout = child.stdout.take();
        Ok(Self {
            child,
            stdout,
            log,
            size,
            path: path.to_path_buf(),
        })
    }

    /// The next frame as straight RGBA8, or `None` at the end of the clip.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, ExportError> {
        let Some(stdout) = self.stdout.as_mut() else {
            return Ok(None);
        };
        let mut buf = vec![0u8; self.size];
        let got = read_full(stdout, &mut buf)
            .map_err(|e| ExportError::Video(format!("could not read from ffmpeg: {e}")))?;
        if got == self.size {
            return Ok(Some(buf));
        }
        // The end: wait for the program and say so if it failed.
        self.stdout = None;
        let status = self.child.wait();
        let log = self
            .log
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        match status {
            Ok(s) if s.success() => Ok(None),
            Ok(s) => Err(ExportError::Video(format!(
                "ffmpeg failed reading {} ({s}){}",
                self.path.display(),
                if log.is_empty() {
                    String::new()
                } else {
                    format!(": {log}")
                }
            ))),
            Err(e) => Err(ExportError::Video(format!(
                "could not wait for ffmpeg: {e}"
            ))),
        }
    }
}

impl Drop for ClipReader {
    fn drop(&mut self) {
        // An export stopped early must not leave a decoder running.
        self.stdout = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_and_rates() {
        assert!(is_video_source("a/b/clip.MP4"));
        assert!(is_video_source("x.webm"));
        assert!(!is_video_source("x.png"));
        assert!(!is_video_source("mp4"));
        assert_eq!(parse_rate("25"), Some(25.0));
        assert!((parse_rate("30000/1001").unwrap() - 29.97).abs() < 0.01);
        assert_eq!(parse_rate("0/0"), None);
        assert_eq!(parse_rate("x"), None);
    }

    #[test]
    fn frame_counts_follow_the_rate_asked_for() {
        let info = VideoInfo {
            width: 4,
            height: 4,
            fps: 10.0,
            seconds: 1.5,
        };
        assert_eq!(info.frames_at(24.0), 36);
        assert_eq!(info.frames_at(f64::NAN), 0);
        assert_eq!(info.frames_at(-1.0), 0);
        assert_eq!(
            VideoInfo {
                seconds: 0.001,
                ..info
            }
            .frames_at(24.0),
            1
        );
    }

    #[test]
    fn a_missing_program_or_file_is_an_error_not_a_panic() {
        let tools = Tools {
            ffmpeg: Some(PathBuf::from("definitely-not-ffmpeg")),
        };
        assert!(matches!(
            probe(&tools, Path::new("x.mp4")),
            Err(ExportError::Video(_))
        ));
        assert!(ClipReader::start(&tools, Path::new("x.mp4"), 8, 8, 24.0, Fit::Stretch).is_err());
        assert!(ClipReader::start(
            &Tools::default(),
            Path::new("x.mp4"),
            0,
            8,
            24.0,
            Fit::Stretch
        )
        .is_err());
        assert!(ClipReader::start(
            &Tools::default(),
            Path::new("x.mp4"),
            8,
            8,
            f64::NAN,
            Fit::Stretch
        )
        .is_err());
    }

    #[test]
    fn ffprobe_sits_beside_a_given_ffmpeg() {
        let t = Tools {
            ffmpeg: Some(PathBuf::from("/opt/x/ffmpeg.exe")),
        };
        assert!(t.ffprobe().ends_with("ffprobe.exe"), "{}", t.ffprobe());
        assert_eq!(Tools::default().ffprobe(), "ffprobe");
    }
}
