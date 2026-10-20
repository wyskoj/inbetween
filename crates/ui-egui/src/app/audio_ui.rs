//! Keeping the sound player in step with the transport.
//!
//! While the preview plays, the sound plays from the matching movie time; it stops with the
//! preview. The audio clock and the picture are compared every frame and the sound jumps when
//! they drift apart (a loop, a seek, a stall).

use super::*;
use crate::audio::{self, Track};
use inbetween_audio::Envelope;

impl InbetweenApp {
    /// The movie frame the transport is on (counting from 0).
    fn audio_movie_frame(&self) -> u64 {
        if self.movie_scope() || self.layer_movie() {
            u64::from(self.transport_pos())
        } else {
            u64::from(self.sequence_start()) + u64::from(self.state.frame)
        }
    }

    /// Start, stop and re-aim the sound for this frame of the interface.
    pub(super) fn sync_audio(&mut self) {
        let sound = self
            .session
            .project()
            .sound
            .clone()
            .filter(|s| s.is_active());
        let Some(sound) = sound else {
            self.audio.stop();
            self.audio_key = None;
            self.audio_pcm = None;
            self.audio.set_track(None);
            return;
        };
        self.audio.set_muted(self.state.muted);
        if !self.state.playing || self.state.muted {
            self.audio.stop();
            return;
        }
        // Decode when the file changed (once; a failure is reported once).
        let full = self.session.resolve_image_path(&sound.path.0);
        let key = full.display().to_string();
        if self.audio_key.as_deref() != Some(&key) {
            self.audio_key = Some(key);
            self.audio_pcm = match inbetween_audio::read_file(&full) {
                Ok(pcm) => Some(Arc::new(pcm)),
                Err(e) => {
                    self.status = Some(Status {
                        text: format!("Sound not played: {e}"),
                        is_error: true,
                    });
                    None
                }
            };
        }
        let Some(pcm) = self.audio_pcm.clone() else {
            self.audio.stop();
            return;
        };
        let project = self.session.project();
        let fps = project.fps;
        self.audio.set_track(Some(Track {
            pcm,
            envelope: Envelope {
                looped: sound.looped,
                cut_at_end: sound.cut_at_end,
                fade_in: sound.fade_in,
                fade_out: sound.fade_out,
            },
            movie_seconds: audio::movie_time(project.total_frames(), fps),
        }));
        let expected = audio::movie_time(self.audio_movie_frame(), fps);
        if !self.audio.is_playing() || audio::needs_resync(self.audio.clock(), expected) {
            if let Err(message) = self.audio.play_at(expected) {
                self.status = Some(Status {
                    text: message,
                    is_error: true,
                });
            }
        }
    }
}
