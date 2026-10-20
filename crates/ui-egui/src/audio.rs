//! Playing the movie's sound along with the preview.
//!
//! The mapping from movie time to samples ([`fill`]) is a pure function so it can be tested
//! without a sound device. [`Player`] wraps it in a `cpal` output stream; a missing device or
//! a failed start is an error message for the status bar, never a panic.

use inbetween_audio::{Envelope, Pcm};
use std::sync::{Arc, Mutex};

/// A decoded sound with the rules for how it sits under the movie.
#[derive(Clone, Debug)]
pub struct Track {
    pub pcm: Arc<Pcm>,
    pub envelope: Envelope,
    /// Length of the movie in seconds.
    pub movie_seconds: f64,
}

/// Movie time in seconds of movie frame `frame` (counting from 0) at `fps`.
pub fn movie_time(frame: u64, fps: f64) -> f64 {
    if fps.is_finite() && fps > 0.0 {
        frame as f64 / fps
    } else {
        0.0
    }
}

/// Whether the audio clock has wandered far enough from the picture to need a seek.
pub fn needs_resync(audio_clock: f64, expected: f64) -> bool {
    !(audio_clock - expected).abs().le(&0.2)
}

/// The sample of `pcm` at `position` seconds for output channel `channel`, interpolated
/// linearly. A mono sound plays on every channel; a stereo one maps channel for channel and
/// repeats its last channel on extra outputs.
fn sample_at(pcm: &Pcm, position: f64, channel: usize) -> f32 {
    let channels = usize::from(pcm.channels.max(1));
    let c = channel.min(channels - 1);
    let at = position * f64::from(pcm.rate);
    if !at.is_finite() || at < 0.0 {
        return 0.0;
    }
    let i = at.floor() as usize;
    let frac = (at - at.floor()) as f32;
    let get = |frame: usize| pcm.samples.get(frame * channels + c).copied();
    match (get(i), get(i + 1)) {
        (Some(a), Some(b)) => a + (b - a) * frac,
        (Some(a), None) => a,
        _ => 0.0,
    }
}

/// Fill `out` (interleaved, `channels` per frame) with the sound as heard from movie time
/// `start` on at output rate `rate`. Silence where the sound does not play. `gain` is the
/// listener's volume (0 when muted).
pub fn fill(track: &Track, start: f64, rate: u32, channels: usize, gain: f32, out: &mut [f32]) {
    let channels = channels.max(1);
    let audio_seconds = track.pcm.duration();
    for (i, frame) in out.chunks_mut(channels).enumerate() {
        let t = start + i as f64 / f64::from(rate.max(1));
        match track.envelope.at(t, audio_seconds, track.movie_seconds) {
            Some((position, fade)) => {
                for (c, slot) in frame.iter_mut().enumerate() {
                    *slot = sample_at(&track.pcm, position, c) * fade * gain;
                }
            }
            None => frame.fill(0.0),
        }
    }
}

/// What the audio callback reads.
#[derive(Default)]
struct Shared {
    track: Option<Track>,
    /// Movie time of the next sample to play.
    clock: f64,
    playing: bool,
    muted: bool,
}

/// Plays a [`Track`] from a given movie time. The stream is opened on first use.
#[derive(Default)]
pub struct Player {
    shared: Arc<Mutex<Shared>>,
    stream: Option<cpal::Stream>,
    /// Why there is no sound, so it is reported once rather than every frame.
    failed: bool,
}

fn lock(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|p| p.into_inner())
}

impl Player {
    /// Use `track` from now on (`None` for no sound).
    pub fn set_track(&mut self, track: Option<Track>) {
        let mut s = lock(&self.shared);
        s.track = track;
        if s.track.is_none() {
            s.playing = false;
        }
    }

    pub fn has_track(&self) -> bool {
        lock(&self.shared).track.is_some()
    }

    pub fn set_muted(&mut self, muted: bool) {
        lock(&self.shared).muted = muted;
    }

    /// Where the audio is, in movie seconds.
    pub fn clock(&self) -> f64 {
        lock(&self.shared).clock
    }

    pub fn is_playing(&self) -> bool {
        lock(&self.shared).playing
    }

    pub fn stop(&mut self) {
        lock(&self.shared).playing = false;
    }

    /// Start (or jump to) movie time `at`. Returns an error message when there is no usable
    /// sound device; it is returned once, later calls stay quiet.
    pub fn play_at(&mut self, at: f64) -> Result<(), String> {
        if self.failed {
            return Ok(());
        }
        if self.stream.is_none() {
            match self.open() {
                Ok(stream) => self.stream = Some(stream),
                Err(e) => {
                    self.failed = true;
                    return Err(e);
                }
            }
        }
        let mut s = lock(&self.shared);
        s.clock = at;
        s.playing = true;
        Ok(())
    }

    fn open(&self) -> Result<cpal::Stream, String> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("no sound output device was found, so the sound is not played")?;
        let config = device
            .default_output_config()
            .map_err(|e| format!("the sound device cannot be used: {e}"))?;
        let rate = config.sample_rate().0;
        let channels = usize::from(config.channels());
        let format = config.sample_format();
        let shared = Arc::clone(&self.shared);
        let mut scratch: Vec<f32> = Vec::new();
        let render = move |out: &mut [f32]| {
            let mut s = lock(&shared);
            match (&s.track, s.playing) {
                (Some(track), true) => {
                    let gain = if s.muted { 0.0 } else { 1.0 };
                    fill(track, s.clock, rate, channels, gain, out);
                    s.clock += (out.len() / channels.max(1)) as f64 / f64::from(rate.max(1));
                }
                _ => out.fill(0.0),
            }
        };
        let on_error = |e: cpal::StreamError| eprintln!("sound output error: {e}");
        let stream = match format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config.into(),
                move |out: &mut [f32], _| render(out),
                on_error,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                &config.into(),
                move |out: &mut [i16], _| {
                    scratch.resize(out.len(), 0.0);
                    render(&mut scratch);
                    for (o, v) in out.iter_mut().zip(&scratch) {
                        *o = (v.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
                    }
                },
                on_error,
                None,
            ),
            other => {
                return Err(format!(
                    "the sound device uses a sample format ({other:?}) that is not supported"
                ))
            }
        }
        .map_err(|e| format!("could not start the sound: {e}"))?;
        stream
            .play()
            .map_err(|e| format!("could not start the sound: {e}"))?;
        Ok(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(looped: bool) -> Track {
        // One second of a ramp 0 .. 1 at 10 Hz, mono.
        let samples: Vec<f32> = (0..10).map(|i| i as f32 / 10.0).collect();
        Track {
            pcm: Arc::new(Pcm {
                rate: 10,
                channels: 1,
                samples,
            }),
            envelope: Envelope {
                looped,
                cut_at_end: true,
                fade_in: 0.0,
                fade_out: 0.0,
            },
            movie_seconds: 2.0,
        }
    }

    #[test]
    fn movie_time_follows_frames_and_survives_bad_rates() {
        assert_eq!(movie_time(48, 24.0), 2.0);
        assert_eq!(movie_time(5, 0.0), 0.0);
        assert_eq!(movie_time(5, f64::NAN), 0.0);
    }

    #[test]
    fn small_drift_is_left_alone_and_big_drift_or_nan_resyncs() {
        assert!(!needs_resync(1.0, 1.1));
        assert!(needs_resync(1.0, 1.5));
        assert!(needs_resync(1.0, f64::NAN));
    }

    #[test]
    fn samples_come_from_the_right_place_and_are_interpolated() {
        let t = track(false);
        let mut out = [0.0f32; 4];
        // Output at 20 Hz from movie time 0.1: the ramp, twice as dense.
        fill(&t, 0.1, 20, 1, 1.0, &mut out);
        assert!((out[0] - 0.1).abs() < 1e-6, "{out:?}");
        assert!((out[1] - 0.15).abs() < 1e-6);
        assert!((out[2] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn mono_plays_on_both_channels_and_mute_is_silence() {
        let t = track(false);
        let mut out = [1.0f32; 4];
        fill(&t, 0.5, 10, 2, 1.0, &mut out);
        assert_eq!(out[0], out[1]);
        assert!((out[0] - 0.5).abs() < 1e-6);
        fill(&t, 0.5, 10, 2, 0.0, &mut out);
        assert_eq!(out, [0.0; 4]);
    }

    #[test]
    fn silence_past_the_sound_and_loops_repeat() {
        let mut out = [1.0f32; 2];
        fill(&track(false), 1.5, 10, 1, 1.0, &mut out);
        assert_eq!(out, [0.0, 0.0]);
        fill(&track(true), 1.5, 10, 1, 1.0, &mut out);
        assert!((out[0] - 0.5).abs() < 1e-6, "{out:?}");
        // Past the end of the movie the cut makes silence even when looping.
        fill(&track(true), 2.5, 10, 1, 1.0, &mut out);
        assert_eq!(out, [0.0, 0.0]);
    }

    #[test]
    fn hostile_times_make_silence_not_panics() {
        let t = track(true);
        let mut out = [1.0f32; 8];
        for start in [f64::NAN, f64::INFINITY, -5.0, 1e300] {
            fill(&t, start, 10, 2, 1.0, &mut out);
            assert!(out.iter().all(|v| v.is_finite()));
        }
        fill(&t, 0.0, 0, 0, 1.0, &mut out);
    }
}
