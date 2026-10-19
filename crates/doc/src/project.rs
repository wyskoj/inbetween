use crate::{DocError, Sequence};
use serde::{Deserialize, Serialize};

/// Largest output side in pixels.
pub const MAX_SIDE: u32 = 32_768;

/// Output frame size shared by every sequence in a project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputSettings {
    pub width: u32,
    pub height: u32,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
        }
    }
}

/// A whole project: output settings and its sequences.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub output: OutputSettings,
    pub sequences: Vec<Sequence>,
}

impl Project {
    /// Frames in the whole movie: the visible sequences play one after another.
    pub fn total_frames(&self) -> u64 {
        self.sequences
            .iter()
            .filter(|s| s.visible)
            .map(|s| u64::from(s.total_frames()))
            .sum()
    }

    /// Which sequence shows movie frame `frame` (counting from 0 across the visible sequences)
    /// and which of its own frames that is. `None` past the end.
    pub fn locate(&self, frame: u64) -> Option<(usize, u32)> {
        let mut start = 0u64;
        for (i, s) in self.sequences.iter().enumerate().filter(|(_, s)| s.visible) {
            let len = u64::from(s.total_frames());
            if frame < start + len {
                return u32::try_from(frame - start).ok().map(|f| (i, f));
            }
            start += len;
        }
        None
    }

    pub fn validate(&self) -> Result<(), DocError> {
        let OutputSettings { width, height } = self.output;
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return Err(DocError::BadOutputSize { width, height });
        }
        self.sequences.iter().try_for_each(Sequence::validate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_project_is_valid() {
        assert!(Project::default().validate().is_ok());
    }

    #[test]
    fn visible_sequences_play_back_to_back() {
        let seq = |frames, visible| Sequence {
            frames,
            visible,
            ..Sequence::default()
        };
        let p = Project {
            sequences: vec![seq(3, true), seq(5, false), seq(2, true)],
            ..Project::default()
        };
        assert_eq!(p.total_frames(), 5);
        let all: Vec<_> = (0..6).map(|f| p.locate(f)).collect();
        assert_eq!(
            all,
            [
                Some((0, 0)),
                Some((0, 1)),
                Some((0, 2)),
                Some((2, 0)),
                Some((2, 1)),
                None
            ]
        );
        assert_eq!(Project::default().locate(0), None);
        assert_eq!(p.locate(u64::MAX), None);
    }

    #[test]
    fn output_size_limits() {
        for (w, h) in [(0, 10), (10, 0), (MAX_SIDE + 1, 10), (10, u32::MAX)] {
            let p = Project {
                output: OutputSettings {
                    width: w,
                    height: h,
                },
                sequences: vec![],
            };
            assert!(
                matches!(p.validate(), Err(DocError::BadOutputSize { .. })),
                "{w}x{h}"
            );
        }
    }

    #[test]
    fn sequence_errors_surface() {
        let p = Project {
            sequences: vec![Sequence {
                frames: 0,
                ..Sequence::default()
            }],
            ..Project::default()
        };
        assert!(matches!(p.validate(), Err(DocError::BadFrameCount { .. })));
    }
}
