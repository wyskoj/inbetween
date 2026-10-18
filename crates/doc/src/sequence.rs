use crate::{DocError, DotSet};
use inbetween_geom::MAX_DOTS;
use serde::{Deserialize, Serialize};

/// Most frames one sequence may hold.
pub const MAX_FRAMES: u32 = 100_000;

/// Where an image lives. Paths are relative to the project file; this crate never opens them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ImageRef(pub String);

impl ImageRef {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }
}

/// How the two images combine over a sequence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MorphType {
    /// Image 1 morphs into Image 2: shape deforms and colours dissolve.
    #[default]
    Morph,
    /// Image 1 deforms into Image 2's shape. Image 2's colours never appear.
    Warp1,
}

impl MorphType {
    /// Whether Image 2's pixels contribute to the output.
    pub fn uses_image2(self) -> bool {
        match self {
            Self::Morph => true,
            Self::Warp1 => false,
        }
    }
}

/// How a source image of a different size is fitted to the output frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitMode {
    /// Scale each axis independently to fill the frame (may distort).
    #[default]
    Stretch,
    /// Scale uniformly to fit inside the frame; the leftover bars are transparent.
    Contain,
    /// Scale uniformly to cover the frame; the overflow is cropped.
    Cover,
}

/// When the morph mesh is triangulated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshPolicy {
    /// Once, from the dots on Image 1, and reused for every frame (the last frame uses a
    /// mesh from Image 2's dots so it always matches Image 2).
    #[default]
    Frame0,
    /// Re-triangulate the interpolated dots on every frame.
    PerFrame,
}

/// One morph: two images, the dots pairing them, and how to render in between.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sequence {
    pub name: String,
    pub image1: ImageRef,
    pub image2: ImageRef,
    pub dots: DotSet,
    /// Total frames, including the first (t = 0) and last (t = 1).
    pub frames: u32,
    pub morph: MorphType,
    pub fit: FitMode,
    pub mesh: MeshPolicy,
}

impl Default for Sequence {
    fn default() -> Self {
        Self {
            name: String::new(),
            image1: ImageRef::default(),
            image2: ImageRef::default(),
            dots: DotSet::default(),
            frames: 30,
            morph: MorphType::default(),
            fit: FitMode::default(),
            mesh: MeshPolicy::default(),
        }
    }
}

impl Sequence {
    pub fn new(image1: ImageRef, image2: ImageRef) -> Self {
        Self {
            image1,
            image2,
            ..Self::default()
        }
    }

    /// Check the limits that keep later stages safe. Duplicate or collinear dots are not
    /// errors here; the morph stage reports those with more context.
    pub fn validate(&self) -> Result<(), DocError> {
        if self.frames == 0 || self.frames > MAX_FRAMES {
            return Err(DocError::BadFrameCount {
                frames: self.frames,
                max: MAX_FRAMES,
            });
        }
        if self.dots.len() > MAX_DOTS {
            return Err(DocError::TooManyDots {
                count: self.dots.len(),
                max: MAX_DOTS,
            });
        }
        for (i, p) in self.dots.pairs.iter().enumerate() {
            if !(p.first.is_finite() && p.second.is_finite()) {
                return Err(DocError::NonFiniteDot(i));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DotPair;
    use inbetween_geom::Point;

    #[test]
    fn defaults_are_valid() {
        let s = Sequence::new(ImageRef::new("a.png"), ImageRef::new("b.png"));
        assert_eq!(s.frames, 30);
        assert_eq!(s.morph, MorphType::Morph);
        assert_eq!(s.fit, FitMode::Stretch);
        assert_eq!(s.mesh, MeshPolicy::Frame0);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn validate_rejects_bad_frame_counts_and_dots() {
        let mut s = Sequence {
            frames: 0,
            ..Sequence::default()
        };
        assert!(matches!(s.validate(), Err(DocError::BadFrameCount { .. })));
        s.frames = MAX_FRAMES + 1;
        assert!(matches!(s.validate(), Err(DocError::BadFrameCount { .. })));
        s.frames = 1;
        assert!(s.validate().is_ok());
        s.dots.pairs.push(DotPair::new(
            Point::new(0.0, 0.0),
            Point::new(f64::NAN, 0.0),
        ));
        assert_eq!(s.validate(), Err(DocError::NonFiniteDot(0)));
    }

    #[test]
    fn morph_type_image_use() {
        assert!(MorphType::Morph.uses_image2());
        assert!(!MorphType::Warp1.uses_image2());
    }
}
