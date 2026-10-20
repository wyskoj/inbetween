use crate::{Caption, DocError, FaceModel, ImageRef, Sequence};
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

/// How the sequences of a project combine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    /// Visible sequences play one after another.
    #[default]
    Sequence,
    /// Visible sequences play at once as stacked layers, the first in the list on top. The
    /// movie is as long as the longest layer; a shorter layer holds its last frame.
    Layer,
}

impl ProjectType {
    pub const ALL: [Self; 2] = [Self::Sequence, Self::Layer];

    pub fn id(self) -> &'static str {
        match self {
            Self::Sequence => "sequence",
            Self::Layer => "layer",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }
}

/// A whole project: output settings and its sequences.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub kind: ProjectType,
    pub output: OutputSettings,
    /// Frames per second of the movie (1 to 60). Playback and exports default to it, and
    /// `sequence.length` converts seconds with it. Written only when not the default.
    #[serde(skip_serializing_if = "is_default_fps")]
    pub fps: f64,
    pub sequences: Vec<Sequence>,
    /// The face dot model the face tools place and fit.
    pub face: FaceModel,
    /// A caption drawn over every frame of the movie.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<Caption>,
}

/// Frames per second a new project plays and exports at.
pub const DEFAULT_FPS: f64 = 24.0;

fn is_default_fps(fps: &f64) -> bool {
    *fps == DEFAULT_FPS
}

impl Default for Project {
    fn default() -> Self {
        Self {
            kind: ProjectType::default(),
            output: OutputSettings::default(),
            fps: DEFAULT_FPS,
            sequences: Vec::new(),
            face: FaceModel::default(),
            caption: None,
        }
    }
}

impl Project {
    /// Frames in the whole movie: the visible sequences play one after another, or (layer
    /// projects) the longest of them.
    pub fn total_frames(&self) -> u64 {
        let lengths = self
            .sequences
            .iter()
            .filter(|s| s.visible)
            .map(|s| u64::from(s.total_frames()));
        match self.kind {
            ProjectType::Sequence => lengths.sum(),
            ProjectType::Layer => lengths.max().unwrap_or(0),
        }
    }

    /// The layers showing movie frame `frame` (counting from 0) in a layer project, bottom
    /// layer first: `(sequence index, that sequence's own frame)`. A layer shorter than the
    /// movie holds its last frame. Empty past the end or in a sequence project.
    pub fn layers_at(&self, frame: u64) -> Vec<(usize, u32)> {
        if self.kind != ProjectType::Layer || frame >= self.total_frames() {
            return Vec::new();
        }
        self.sequences
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, s)| s.visible && s.total_frames() > 0)
            .filter_map(|(i, s)| {
                let last = u64::from(s.total_frames()) - 1;
                u32::try_from(frame.min(last)).ok().map(|f| (i, f))
            })
            .collect()
    }

    /// Which sequence shows movie frame `frame` (counting from 0 across the visible sequences)
    /// and which of its own frames that is. `None` past the end. In a layer project this is
    /// the top layer.
    pub fn locate(&self, frame: u64) -> Option<(usize, u32)> {
        if self.kind == ProjectType::Layer {
            return self.layers_at(frame).last().copied();
        }
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

    /// Every file the project refers to: images, effect images and caption fonts.
    pub fn all_paths(&self) -> Vec<&ImageRef> {
        let mut out: Vec<&ImageRef> = self
            .sequences
            .iter()
            .flat_map(Sequence::all_paths)
            .collect();
        out.extend(self.caption.iter().filter_map(|c| c.font.as_ref()));
        out
    }

    /// Mutable form of [`Project::all_paths`], for rebasing paths.
    pub fn all_paths_mut(&mut self) -> Vec<&mut ImageRef> {
        let mut out: Vec<&mut ImageRef> = self
            .sequences
            .iter_mut()
            .flat_map(Sequence::all_paths_mut)
            .collect();
        out.extend(self.caption.iter_mut().filter_map(|c| c.font.as_mut()));
        out
    }

    pub fn validate(&self) -> Result<(), DocError> {
        let OutputSettings { width, height } = self.output;
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return Err(DocError::BadOutputSize { width, height });
        }
        if !(self.fps.is_finite() && (1.0..=60.0).contains(&self.fps)) {
            return Err(DocError::BadFps);
        }
        self.face.validate()?;
        if let Some(c) = &self.caption {
            c.validate()?;
        }
        self.sequences.iter().try_for_each(Sequence::validate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fps_must_be_1_to_60_and_default_is_not_written() {
        for bad in [0.0, 0.5, 61.0, f64::NAN, f64::INFINITY] {
            let p = Project {
                fps: bad,
                ..Project::default()
            };
            assert!(matches!(p.validate(), Err(DocError::BadFps)), "{bad}");
        }
        let p = Project {
            fps: 29.97,
            ..Project::default()
        };
        assert!(p.validate().is_ok());
        let text = serde_json::to_string(&Project::default()).unwrap();
        assert!(!text.contains("fps"));
        let back: Project = serde_json::from_str("{}").unwrap();
        assert_eq!(back.fps, DEFAULT_FPS);
    }

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
    fn layer_projects_play_everything_at_once() {
        let seq = |frames, visible| Sequence {
            frames,
            visible,
            ..Sequence::default()
        };
        let p = Project {
            kind: ProjectType::Layer,
            sequences: vec![seq(3, true), seq(5, true), seq(9, false)],
            ..Project::default()
        };
        assert_eq!(p.total_frames(), 5);
        // Bottom layer first: the second sequence, then the top (first) one.
        assert_eq!(p.layers_at(0), [(1, 0), (0, 0)]);
        assert_eq!(p.layers_at(4), [(1, 4), (0, 2)]);
        assert!(p.layers_at(5).is_empty());
        assert_eq!(p.locate(4), Some((0, 2)));
        assert_eq!(p.locate(5), None);
        assert!(Project::default().layers_at(0).is_empty());
        assert_eq!(ProjectType::from_id("layer"), Some(ProjectType::Layer));
    }

    #[test]
    fn output_size_limits() {
        for (w, h) in [(0, 10), (10, 0), (MAX_SIDE + 1, 10), (10, u32::MAX)] {
            let p = Project {
                output: OutputSettings {
                    width: w,
                    height: h,
                },
                ..Project::default()
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
