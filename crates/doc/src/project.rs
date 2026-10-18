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
