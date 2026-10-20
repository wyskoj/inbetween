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
    /// A sound to mux in; `None` writes a silent video.
    pub audio: Option<AudioMux>,
}

/// A sound file laid under the video.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioMux {
    pub path: PathBuf,
    pub looped: bool,
    /// End the file where the video ends when the sound is longer.
    pub cut_at_end: bool,
    pub fade_in: f64,
    pub fade_out: f64,
    /// Length of the sound file in seconds (the fade-out of an uncut sound ends there).
    pub audio_seconds: f64,
    /// Length of the video in seconds.
    pub movie_seconds: f64,
}

impl AudioMux {
    /// The `ffmpeg` arguments that go before the `-i` of the sound.
    fn input_args(&self) -> Vec<String> {
        if self.looped {
            vec!["-stream_loop".into(), "-1".into()]
        } else {
            Vec::new()
        }
    }

    /// The `-af` filter text, or `None` when there is no fade.
    fn filter(&self) -> Option<String> {
        let mut parts = Vec::new();
        if self.fade_in > 0.0 {
            parts.push(format!("afade=t=in:st=0:d={}", self.fade_in));
        }
        if self.fade_out > 0.0 {
            let end = if self.cut_at_end || self.looped {
                self.movie_seconds
            } else {
                self.audio_seconds
            };
            let start = (end - self.fade_out).max(0.0);
            parts.push(format!("afade=t=out:st={start}:d={}", self.fade_out));
        }
        (!parts.is_empty()).then(|| parts.join(","))
    }
}

/// Every `ffmpeg` argument for a video of `width` x `height` at `fps`, written to `out`.
pub fn ffmpeg_args(
    width: u32,
    height: u32,
    fps: f64,
    out: &Path,
    opts: &VideoOptions,
) -> Vec<String> {
    let mut a: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
    ]
    .map(String::from)
    .to_vec();
    a.extend([
        "-s".into(),
        format!("{width}x{height}"),
        "-r".into(),
        format!("{fps}"),
        "-i".into(),
        "-".into(),
    ]);
    match &opts.audio {
        None => a.push("-an".into()),
        Some(mux) => {
            a.extend(mux.input_args());
            a.extend(["-i".into(), mux.path.display().to_string()]);
            a.extend(["-map", "0:v", "-map", "1:a"].map(String::from));
        }
    }
    a.extend(opts.format.encoder_args().iter().map(|s| (*s).to_string()));
    if let Some(mux) = &opts.audio {
        let codec = if opts.format == VideoFormat::WebM {
            "libopus"
        } else {
            "aac"
        };
        a.extend(["-c:a".into(), codec.into()]);
        if let Some(f) = mux.filter() {
            a.extend(["-af".into(), f]);
        }
        if mux.cut_at_end || mux.looped {
            a.extend(["-t".into(), format!("{}", mux.movie_seconds)]);
        }
    }
    a.push(out.display().to_string());
    a
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
        cmd.args(ffmpeg_args(width, height, fps, out, opts))
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

    fn mux(looped: bool, cut: bool) -> AudioMux {
        AudioMux {
            path: PathBuf::from("s.wav"),
            looped,
            cut_at_end: cut,
            fade_in: 1.0,
            fade_out: 2.0,
            audio_seconds: 4.0,
            movie_seconds: 10.0,
        }
    }

    fn args(format: VideoFormat, audio: Option<AudioMux>) -> Vec<String> {
        let opts = VideoOptions {
            format,
            fps: 24.0,
            matte: [0; 3],
            ffmpeg: None,
            audio,
        };
        ffmpeg_args(8, 6, 24.0, Path::new("o.out"), &opts)
    }

    fn has(a: &[String], words: &[&str]) -> bool {
        a.windows(words.len())
            .any(|w| w.iter().zip(words).all(|(x, y)| x == y))
    }

    #[test]
    fn silent_video_has_no_audio_input() {
        let a = args(VideoFormat::Mp4, None);
        assert!(a.contains(&"-an".to_string()));
        assert!(!a.contains(&"-map".to_string()) && !a.contains(&"-c:a".to_string()));
        assert_eq!(a.last().map(String::as_str), Some("o.out"));
    }

    #[test]
    fn sound_is_mapped_encoded_faded_and_cut() {
        let a = args(VideoFormat::Mp4, Some(mux(false, true)));
        assert!(!a.contains(&"-an".to_string()));
        assert!(!a.contains(&"-stream_loop".to_string()));
        assert!(has(&a, &["-i", "s.wav", "-map", "0:v", "-map", "1:a"]));
        assert!(has(&a, &["-c:a", "aac"]));
        assert!(has(
            &a,
            &["-af", "afade=t=in:st=0:d=1,afade=t=out:st=8:d=2"]
        ));
        assert!(has(&a, &["-t", "10", "o.out"]));
        // Loops come before the sound's own -i; webm uses opus.
        let a = args(VideoFormat::WebM, Some(mux(true, true)));
        assert!(has(&a, &["-stream_loop", "-1", "-i", "s.wav"]));
        assert!(has(&a, &["-c:a", "libopus"]));
        assert!(!has(&a, &["-c:a", "aac"]));
        let a = args(VideoFormat::ProRes, Some(mux(false, true)));
        assert!(has(&a, &["-c:a", "aac"]));
    }

    #[test]
    fn an_uncut_sound_fades_out_at_its_own_end_and_is_not_trimmed() {
        let a = args(VideoFormat::Mp4, Some(mux(false, false)));
        assert!(has(
            &a,
            &["-af", "afade=t=in:st=0:d=1,afade=t=out:st=2:d=2"]
        ));
        assert!(!a.contains(&"-t".to_string()));
        let mut m = mux(false, true);
        m.fade_in = 0.0;
        m.fade_out = 0.0;
        assert!(!args(VideoFormat::Mp4, Some(m)).contains(&"-af".to_string()));
    }

    #[test]
    fn missing_program_is_an_actionable_error() {
        let opts = VideoOptions {
            format: VideoFormat::Mp4,
            fps: 24.0,
            matte: [0; 3],
            ffmpeg: Some(PathBuf::from("definitely-not-a-real-ffmpeg-binary")),
            audio: None,
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
            audio: None,
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
