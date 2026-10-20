//! Audio for the movie's sound track: decode WAV and MP3 to interleaved `f32` samples, and the
//! pure rules for which part of the sound plays when (looping, cutting, fades).
//!
//! Decoding works from bytes, so it is wasm-clean; reading a file sits behind
//! `cfg(not(target_arch = "wasm32"))`. Malformed input is an [`AudioError`], never a panic.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::fmt;

/// Longest sound that is decoded, in seconds.
pub const MAX_SECONDS: f64 = 1800.0;
/// Most samples (all channels together) that are decoded: 1 GiB of `f32`.
pub const MAX_SAMPLES: usize = (1 << 30) / 4;
/// Largest file that is read.
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioError {
    /// The file is neither a WAV nor an MP3 file.
    Unsupported(String),
    /// The data is damaged.
    Malformed(String),
    /// The sound is longer or bigger than the limits.
    TooLarge(String),
    /// The file could not be read.
    Io(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(m) | Self::Malformed(m) | Self::TooLarge(m) | Self::Io(m) => {
                write!(f, "{m}")
            }
        }
    }
}

impl std::error::Error for AudioError {}

/// Decoded sound: interleaved samples in -1 to 1.
#[derive(Clone, Debug, PartialEq)]
pub struct Pcm {
    pub rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl Pcm {
    /// Samples per channel.
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }

    pub fn duration(&self) -> f64 {
        self.frames() as f64 / f64::from(self.rate.max(1))
    }
}

/// Length and format of a sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Probe {
    pub duration: f64,
    pub rate: u32,
    pub channels: u16,
}

fn too_big(seconds: f64) -> AudioError {
    AudioError::TooLarge(format!(
        "the sound is longer than {} minutes ({seconds:.0} s or more)",
        MAX_SECONDS / 60.0
    ))
}

/// Decode a `.wav` or `.mp3` file's bytes (the format is read from the data).
pub fn decode(bytes: &[u8]) -> Result<Pcm, AudioError> {
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        decode_wav(bytes)
    } else if looks_like_mp3(bytes) {
        // The decoder is third-party; whatever it does with hostile data, we answer with an error.
        std::panic::catch_unwind(|| decode_mp3(bytes))
            .unwrap_or_else(|_| Err(AudioError::Malformed("the MP3 data is damaged".into())))
    } else {
        Err(AudioError::Unsupported(
            "only WAV and MP3 sound files are supported".into(),
        ))
    }
}

fn looks_like_mp3(b: &[u8]) -> bool {
    b.starts_with(b"ID3") || (b.len() > 2 && b[0] == 0xFF && b[1] & 0xE0 == 0xE0)
}

fn decode_wav(bytes: &[u8]) -> Result<Pcm, AudioError> {
    let bad = |e: hound::Error| AudioError::Malformed(format!("the WAV file is damaged: {e}"));
    let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes)).map_err(bad)?;
    let spec = reader.spec();
    if spec.channels == 0 || spec.channels > 8 || spec.sample_rate == 0 {
        return Err(AudioError::Malformed(
            "the WAV file has no channels or no sample rate".into(),
        ));
    }
    let total = reader.len() as usize;
    let seconds = total as f64 / f64::from(spec.channels) / f64::from(spec.sample_rate);
    if total > MAX_SAMPLES || seconds > MAX_SECONDS {
        return Err(too_big(seconds));
    }
    let mut samples = Vec::with_capacity(total);
    match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => {
            for s in reader.samples::<f32>() {
                samples.push(s.map_err(bad)?);
            }
        }
        (hound::SampleFormat::Int, bits @ 1..=32) => {
            let scale = 1.0 / (2.0f64.powi(i32::from(bits) - 1));
            for s in reader.samples::<i32>() {
                samples.push((f64::from(s.map_err(bad)?) * scale) as f32);
            }
        }
        _ => {
            return Err(AudioError::Unsupported(
                "this WAV sample format is not supported (use 16-bit, 24-bit or float)".into(),
            ))
        }
    }
    clean(&mut samples);
    Ok(Pcm {
        rate: spec.sample_rate,
        channels: spec.channels,
        samples,
    })
}

fn decode_mp3(bytes: &[u8]) -> Result<Pcm, AudioError> {
    let mut decoder = puremp3::Mp3Decoder::new(std::io::Cursor::new(bytes));
    let mut samples: Vec<f32> = Vec::new();
    let mut format: Option<(u32, u16)> = None;
    while let Ok(frame) = decoder.next_frame() {
        let rate = frame.header.sample_rate.hz();
        let channels: u16 = if frame.header.channels == puremp3::Channels::Mono {
            1
        } else {
            2
        };
        let (rate0, channels0) = *format.get_or_insert((rate, channels));
        if (rate, channels) != (rate0, channels0) {
            break; // a change of format mid-stream: keep what came before
        }
        let n = frame.num_samples.min(1152);
        for i in 0..n {
            for c in 0..usize::from(channels) {
                samples.push(frame.samples[c][i]);
            }
        }
        let seconds = samples.len() as f64 / f64::from(channels) / f64::from(rate.max(1));
        if samples.len() > MAX_SAMPLES || seconds > MAX_SECONDS {
            return Err(too_big(seconds));
        }
    }
    let Some((rate, channels)) = format else {
        return Err(AudioError::Malformed(
            "no MP3 audio was found in the file".into(),
        ));
    };
    clean(&mut samples);
    Ok(Pcm {
        rate,
        channels,
        samples,
    })
}

/// Samples must be finite and within -1 to 1.
fn clean(samples: &mut [f32]) {
    for s in samples {
        *s = if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
    }
}

/// Length and format of a sound without keeping its samples around.
pub fn probe(bytes: &[u8]) -> Result<Probe, AudioError> {
    let pcm = decode(bytes)?;
    Ok(Probe {
        duration: pcm.duration(),
        rate: pcm.rate,
        channels: pcm.channels,
    })
}

/// Read and decode a sound file.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_file(path: &std::path::Path) -> Result<Pcm, AudioError> {
    decode(&read_bytes(path)?)
}

/// Read and probe a sound file.
#[cfg(not(target_arch = "wasm32"))]
pub fn probe_file(path: &std::path::Path) -> Result<Probe, AudioError> {
    probe(&read_bytes(path)?)
}

#[cfg(not(target_arch = "wasm32"))]
fn read_bytes(path: &std::path::Path) -> Result<Vec<u8>, AudioError> {
    let io = |e: std::io::Error| AudioError::Io(format!("{}: {e}", path.display()));
    let meta = std::fs::metadata(path).map_err(io)?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(AudioError::TooLarge(format!(
            "{}: the file is larger than {} MiB",
            path.display(),
            MAX_FILE_BYTES >> 20
        )));
    }
    std::fs::read(path).map_err(io)
}

/// How the sound sits under the movie.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Envelope {
    pub looped: bool,
    pub cut_at_end: bool,
    pub fade_in: f64,
    pub fade_out: f64,
}

impl Envelope {
    /// Where in the sound to be at movie time `movie_seconds`, for a sound `audio_seconds`
    /// long under a movie `movie_seconds_total` long. `None` means silence (before the start,
    /// past the end of a sound that does not loop, or past a cut).
    pub fn source_time(
        &self,
        movie_seconds: f64,
        audio_seconds: f64,
        movie_seconds_total: f64,
    ) -> Option<f64> {
        if !(movie_seconds.is_finite() && audio_seconds.is_finite() && audio_seconds > 0.0) {
            return None;
        }
        if movie_seconds < 0.0 || (self.cut_at_end && movie_seconds >= movie_seconds_total) {
            return None;
        }
        if movie_seconds < audio_seconds {
            Some(movie_seconds)
        } else if self.looped {
            Some(movie_seconds % audio_seconds)
        } else {
            None
        }
    }

    /// The fade gain, 0 to 1, at movie time `movie_seconds`. The fade-out ends where the sound
    /// ends: at the movie's end when the sound is cut or loops, else at the end of the sound.
    pub fn gain(&self, movie_seconds: f64, audio_seconds: f64, movie_seconds_total: f64) -> f32 {
        let end = if self.cut_at_end || self.looped {
            movie_seconds_total
        } else {
            audio_seconds.max(movie_seconds_total.min(audio_seconds))
        };
        let ramp = |len: f64, into: f64| {
            if len > 0.0 {
                (into / len).clamp(0.0, 1.0)
            } else {
                1.0
            }
        };
        let g = ramp(self.fade_in, movie_seconds) * ramp(self.fade_out, end - movie_seconds);
        if g.is_finite() {
            g as f32
        } else {
            0.0
        }
    }

    /// Where to play and how loud, in one call: `(position in the sound, gain)`.
    pub fn at(
        &self,
        movie_seconds: f64,
        audio_seconds: f64,
        movie_seconds_total: f64,
    ) -> Option<(f64, f32)> {
        let pos = self.source_time(movie_seconds, audio_seconds, movie_seconds_total)?;
        Some((
            pos,
            self.gain(movie_seconds, audio_seconds, movie_seconds_total),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(rate: u32, channels: u16, frames: usize, float: bool) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: if float { 32 } else { 16 },
            sample_format: if float {
                hound::SampleFormat::Float
            } else {
                hound::SampleFormat::Int
            },
        };
        let mut out = Vec::new();
        {
            let mut w = hound::WavWriter::new(std::io::Cursor::new(&mut out), spec).unwrap();
            for i in 0..frames {
                for _ in 0..channels {
                    let v = ((i as f32) * 0.05).sin();
                    if float {
                        w.write_sample(v).unwrap();
                    } else {
                        w.write_sample((v * 20000.0) as i16).unwrap();
                    }
                }
            }
            w.finalize().unwrap();
        }
        out
    }

    #[test]
    fn wav_decodes_with_length_and_format() {
        for float in [false, true] {
            let pcm = decode(&wav(8000, 2, 4000, float)).unwrap();
            assert_eq!((pcm.rate, pcm.channels, pcm.frames()), (8000, 2, 4000));
            assert!((pcm.duration() - 0.5).abs() < 1e-9);
            assert!(pcm.samples.iter().all(|s| s.abs() <= 1.0));
            assert!(pcm.samples.iter().any(|s| s.abs() > 0.1));
        }
        let p = probe(&wav(44100, 1, 44100, false)).unwrap();
        assert_eq!((p.rate, p.channels), (44100, 1));
        assert!((p.duration - 1.0).abs() < 1e-9);
    }

    #[test]
    fn junk_is_an_error_not_a_panic() {
        for bytes in [
            &b""[..],
            b"RIFF",
            b"RIFF\0\0\0\0WAVE",
            b"hello world, this is not sound",
            b"ID3\x04\0\0\0\0\0\0",
            &[0xFF, 0xFB, 0x90, 0x00, 1, 2, 3],
        ] {
            assert!(decode(bytes).is_err(), "{bytes:?}");
        }
    }

    #[test]
    fn mutated_files_never_panic() {
        let good = wav(8000, 2, 400, false);
        let mut seed = 0x1234_5678_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let scale: usize = std::env::var("INBETWEEN_FUZZ_SCALE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        for _ in 0..3000 / scale.max(1) {
            let mut b = good.clone();
            for _ in 0..1 + next() % 4 {
                let i = next() as usize % b.len();
                b[i] = next() as u8;
            }
            if next() % 4 == 0 {
                b.truncate(next() as usize % b.len());
            }
            let _ = decode(&b);
        }
        // Stray bytes behind an MP3 header.
        for n in 0..200 {
            let mut b = vec![0xFF, 0xFB, 0x90, 0x64];
            b.extend((0..n * 7).map(|_| next() as u8));
            let _ = decode(&b);
        }
    }

    #[test]
    fn huge_claims_are_refused() {
        // A header that claims an hour of audio but carries almost none.
        let mut b = wav(8000, 1, 10, false);
        let big = (8000u32 * 3600 * 2).to_le_bytes();
        b[40..44].copy_from_slice(&big);
        assert!(decode(&b).is_err());
    }

    #[test]
    fn envelope_loops_cuts_and_fades() {
        let e = Envelope {
            looped: false,
            cut_at_end: true,
            fade_in: 1.0,
            fade_out: 2.0,
        };
        // A 4 s sound under a 10 s movie plays once, then is silent.
        assert_eq!(e.source_time(1.0, 4.0, 10.0), Some(1.0));
        assert_eq!(e.source_time(5.0, 4.0, 10.0), None);
        let looped = Envelope { looped: true, ..e };
        assert_eq!(looped.source_time(5.0, 4.0, 10.0), Some(1.0));
        // A 20 s sound is cut at the movie's end, or not.
        assert_eq!(e.source_time(10.0, 20.0, 10.0), None);
        let uncut = Envelope {
            cut_at_end: false,
            ..e
        };
        assert_eq!(uncut.source_time(12.0, 20.0, 10.0), Some(12.0));
        // Fades: half way in, half way out, full in the middle, silent at both ends.
        assert_eq!(e.gain(0.5, 20.0, 10.0), 0.5);
        assert_eq!(e.gain(5.0, 20.0, 10.0), 1.0);
        assert_eq!(e.gain(9.0, 20.0, 10.0), 0.5);
        assert_eq!(e.gain(0.0, 20.0, 10.0), 0.0);
        assert_eq!(e.gain(10.0, 20.0, 10.0), 0.0);
        // Uncut, the fade-out ends where the sound ends.
        assert_eq!(uncut.gain(19.0, 20.0, 10.0), 0.5);
        // Hostile input is silence or a valid gain.
        assert_eq!(e.source_time(f64::NAN, 4.0, 10.0), None);
        assert_eq!(e.source_time(1.0, 0.0, 10.0), None);
        assert!((0.0..=1.0).contains(&e.gain(f64::NAN, f64::NAN, f64::INFINITY)));
        assert_eq!(e.at(1.0, 4.0, 10.0), Some((1.0, 1.0)));
    }
}
