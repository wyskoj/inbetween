use crate::{
    Camera, Caption, Curve, DocError, DotSet, Effects, Filter, ImageEdit, Track, MAX_FILTERS,
};
use inbetween_geom::MAX_DOTS;
use serde::{Deserialize, Serialize};

/// Most frames one sequence may hold, hold frames included.
pub const MAX_FRAMES: u32 = 100_000;
/// Most hold frames at either end of a sequence.
pub const MAX_HOLD: u32 = 999;

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
    /// Like Morph, but Image 1 never fades: Image 2 is laid over it by the morph amount. With
    /// an Image 2 that has transparent areas, Image 1 stays visible there (a partial morph).
    MorphHold,
    /// Image 1 deforms into Image 2's shape. Image 2's colours never appear.
    Warp1,
    /// Image 2 deforms from Image 1's shape. Image 1's colours never appear.
    Warp2,
    /// Image 1 fades to black. No warping, so the dots are not used.
    FadeOut1,
    /// Image 2 fades in from black. No warping, so the dots are not used.
    FadeIn2,
    /// No morph: Image 1 shows until the last frame, where Image 2 cuts in.
    Direct,
}

impl MorphType {
    pub const ALL: [Self; 7] = [
        Self::Morph,
        Self::MorphHold,
        Self::Warp1,
        Self::Warp2,
        Self::FadeOut1,
        Self::FadeIn2,
        Self::Direct,
    ];

    /// Whether Image 2's pixels can contribute to the output (so it must be loaded).
    pub fn uses_image2(self) -> bool {
        !matches!(self, Self::Warp1 | Self::FadeOut1)
    }

    /// Whether the dots deform the images. Fades and the direct cut leave the shape alone.
    pub fn warps(self) -> bool {
        !matches!(self, Self::FadeOut1 | Self::FadeIn2 | Self::Direct)
    }

    /// The name used in project files and commands.
    pub fn id(self) -> &'static str {
        match self {
            Self::Morph => "morph",
            Self::MorphHold => "morph_hold",
            Self::Warp1 => "warp1",
            Self::Warp2 => "warp2",
            Self::FadeOut1 => "fade_out1",
            Self::FadeIn2 => "fade_in2",
            Self::Direct => "direct",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == id)
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

impl FitMode {
    pub const ALL: [Self; 3] = [Self::Stretch, Self::Contain, Self::Cover];

    pub fn id(self) -> &'static str {
        match self {
            Self::Stretch => "stretch",
            Self::Contain => "contain",
            Self::Cover => "cover",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }
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

impl MeshPolicy {
    pub const ALL: [Self; 2] = [Self::Frame0, Self::PerFrame];

    pub fn id(self) -> &'static str {
        match self {
            Self::Frame0 => "frame0",
            Self::PerFrame => "per_frame",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }
}

/// One morph: two images, the dots pairing them, and how to render in between.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sequence {
    pub name: String,
    pub image1: ImageRef,
    pub image2: ImageRef,
    pub dots: DotSet,
    /// Morph frames, including the first (t = 0) and last (t = 1) of the morph itself.
    pub frames: u32,
    /// Extra frames before the morph, all showing t = 0.
    pub hold_start: u32,
    /// Extra frames after the morph, all showing t = 1.
    pub hold_end: u32,
    pub morph: MorphType,
    pub fit: FitMode,
    pub mesh: MeshPolicy,
    /// Discard the mesh triangles that touch the four frame corners, so only the area inside
    /// the dots' outline is drawn and the rest of the frame stays transparent (a backdrop or
    /// back colour then shows through).
    pub drop_corner_tris: bool,
    /// Remaps morph time into the colour-dissolve amount.
    pub feature_curve: Curve,
    /// Remaps morph time into the shape-interpolation amount (how far the dots have moved).
    pub shape_curve: Curve,
    /// Hidden sequences are skipped by playback and export.
    pub visible: bool,
    /// Back colour, background, mask, foreground, transition, light, trail and radial.
    pub effects: Effects,
    /// Moves the rendered morph over time.
    pub camera: Camera,
    /// The sequence's overall opacity over time (a fade in or out). Its ends may sit anywhere.
    pub master_curve: Curve,
    /// Bends the path every dot travels.
    pub track: Track,
    /// Edits applied to Image 1 and Image 2 before they are morphed.
    #[serde(skip_serializing_if = "ImageEdit::is_identity")]
    pub edit1: ImageEdit,
    #[serde(skip_serializing_if = "ImageEdit::is_identity")]
    pub edit2: ImageEdit,
    /// A caption drawn over every frame of this sequence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<Caption>,
    /// Filters run over every finished frame of this sequence (after the camera).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
}

impl Default for Sequence {
    fn default() -> Self {
        Self {
            name: String::new(),
            image1: ImageRef::default(),
            image2: ImageRef::default(),
            dots: DotSet::default(),
            frames: 30,
            hold_start: 0,
            hold_end: 0,
            morph: MorphType::default(),
            fit: FitMode::default(),
            mesh: MeshPolicy::default(),
            drop_corner_tris: false,
            feature_curve: Curve::default(),
            shape_curve: Curve::default(),
            visible: true,
            effects: Effects::default(),
            camera: Camera::default(),
            master_curve: Curve::opaque(),
            track: Track::default(),
            edit1: ImageEdit::default(),
            edit2: ImageEdit::default(),
            caption: None,
            filters: Vec::new(),
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

    /// The edit of Image 1 (`which` 1) or Image 2 (any other value).
    pub fn edit(&self, which: u8) -> &ImageEdit {
        if which == 1 {
            &self.edit1
        } else {
            &self.edit2
        }
    }

    pub fn edit_mut(&mut self, which: u8) -> &mut ImageEdit {
        if which == 1 {
            &mut self.edit1
        } else {
            &mut self.edit2
        }
    }

    /// Every file this sequence refers to: both images, the effect images and caption fonts.
    pub fn all_paths(&self) -> Vec<&ImageRef> {
        let mut out = vec![&self.image1, &self.image2];
        out.extend(self.effects.all_images());
        let captions = [&self.edit1.caption, &self.edit2.caption, &self.caption];
        out.extend(
            captions
                .into_iter()
                .flatten()
                .filter_map(|c| c.font.as_ref()),
        );
        out
    }

    /// Mutable form of [`Sequence::all_paths`], for rebasing paths.
    pub fn all_paths_mut(&mut self) -> Vec<&mut ImageRef> {
        let mut out = vec![&mut self.image1, &mut self.image2];
        out.extend(self.effects.all_images_mut());
        let captions = [
            &mut self.edit1.caption,
            &mut self.edit2.caption,
            &mut self.caption,
        ];
        out.extend(
            captions
                .into_iter()
                .flatten()
                .filter_map(|c| c.font.as_mut()),
        );
        out
    }

    /// Whether the frame needs more than the plain morph that the GPU renderer draws: any
    /// effect, a camera move, a master fade or a caption. (The track bends the mesh, so the GPU draws it.)
    pub fn needs_post_pass(&self) -> bool {
        self.effects.any_active()
            || self.camera.is_active()
            || !self.filters.is_empty()
            || !self.master_curve.is_opaque()
            || self.caption.as_ref().is_some_and(Caption::is_visible)
    }

    /// Every frame the sequence renders: hold, morph, hold.
    pub fn total_frames(&self) -> u32 {
        self.hold_start
            .saturating_add(self.frames)
            .saturating_add(self.hold_end)
    }

    /// Morph time of frame `index`: 0 through the start hold, then rising evenly to 1 across the
    /// morph frames, then 1 through the end hold. Indexes past the end give 1; a one-frame morph
    /// is just t = 0 (until its end hold).
    pub fn frame_time(&self, index: u32) -> f64 {
        let Some(in_morph) = index.checked_sub(self.hold_start) else {
            return 0.0;
        };
        if self.frames <= 1 {
            return if in_morph == 0 { 0.0 } else { 1.0 };
        }
        if in_morph >= self.frames - 1 {
            1.0
        } else {
            f64::from(in_morph) / f64::from(self.frames - 1)
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
        if self.hold_start > MAX_HOLD || self.hold_end > MAX_HOLD {
            return Err(DocError::BadHold {
                hold: self.hold_start.max(self.hold_end),
                max: MAX_HOLD,
            });
        }
        if self.total_frames() > MAX_FRAMES {
            return Err(DocError::BadFrameCount {
                frames: self.total_frames(),
                max: MAX_FRAMES,
            });
        }
        if self.dots.len() > MAX_DOTS {
            return Err(DocError::TooManyDots {
                count: self.dots.len(),
                max: MAX_DOTS,
            });
        }
        self.feature_curve.validate()?;
        self.shape_curve.validate()?;
        self.master_curve.validate_free()?;
        self.camera.validate()?;
        self.track.validate()?;
        self.effects.validate()?;
        if self.filters.len() > MAX_FILTERS {
            return Err(DocError::BadEdit("at most 16 filters per sequence"));
        }
        self.filters.iter().try_for_each(Filter::validate)?;
        self.edit1.validate()?;
        self.edit2.validate()?;
        if let Some(c) = &self.caption {
            c.validate()?;
        }
        for (i, p) in self.dots.pairs.iter().enumerate() {
            if !(p.first.is_finite() && p.second.is_finite()) {
                return Err(DocError::NonFiniteDot(i));
            }
        }
        self.dots.validate_lines()?;
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
        assert_eq!((s.hold_start, s.hold_end), (0, 0));
        assert_eq!(s.total_frames(), 30);
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
    fn hold_frames_are_limited() {
        let mut s = Sequence {
            hold_start: MAX_HOLD,
            hold_end: MAX_HOLD,
            ..Sequence::default()
        };
        assert!(s.validate().is_ok());
        s.hold_end = MAX_HOLD + 1;
        assert!(matches!(s.validate(), Err(DocError::BadHold { .. })));
        s.hold_end = 0;
        s.hold_start = u32::MAX;
        assert!(matches!(s.validate(), Err(DocError::BadHold { .. })));
        // Hold plus morph must still fit the frame limit.
        let s = Sequence {
            frames: MAX_FRAMES,
            hold_end: 1,
            ..Sequence::default()
        };
        assert!(matches!(s.validate(), Err(DocError::BadFrameCount { .. })));
    }

    #[test]
    fn frame_time_holds_then_morphs_then_holds() {
        let s = Sequence {
            frames: 5,
            hold_start: 2,
            hold_end: 3,
            ..Sequence::default()
        };
        assert_eq!(s.total_frames(), 10);
        let times: Vec<f64> = (0..10).map(|i| s.frame_time(i)).collect();
        assert_eq!(times, [0.0, 0.0, 0.0, 0.25, 0.5, 0.75, 1.0, 1.0, 1.0, 1.0]);
        // Past the end stays at 1; the start hold never goes below 0.
        assert_eq!(s.frame_time(1_000_000), 1.0);
        assert_eq!(s.frame_time(u32::MAX), 1.0);
    }

    #[test]
    fn frame_time_without_holds_matches_the_old_rule() {
        let s = Sequence {
            frames: 5,
            ..Sequence::default()
        };
        let times: Vec<f64> = (0..5).map(|i| s.frame_time(i)).collect();
        assert_eq!(times, [0.0, 0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn one_frame_morph_is_t_zero_until_its_hold_ends() {
        let single = Sequence {
            frames: 1,
            ..Sequence::default()
        };
        assert_eq!((single.frame_time(0), single.frame_time(5)), (0.0, 1.0));
        let held = Sequence {
            frames: 1,
            hold_start: 1,
            hold_end: 1,
            ..Sequence::default()
        };
        assert_eq!(
            (0..3).map(|i| held.frame_time(i)).collect::<Vec<_>>(),
            [0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn morph_types_know_what_they_need() {
        use MorphType::*;
        for t in [Morph, MorphHold, Warp2, FadeIn2, Direct] {
            assert!(t.uses_image2(), "{t:?}");
        }
        for t in [Warp1, FadeOut1] {
            assert!(!t.uses_image2(), "{t:?}");
        }
        for t in [Morph, MorphHold, Warp1, Warp2] {
            assert!(t.warps(), "{t:?}");
        }
        for t in [FadeOut1, FadeIn2, Direct] {
            assert!(!t.warps(), "{t:?}");
        }
    }

    #[test]
    fn ids_round_trip_and_match_serde_names() {
        for t in MorphType::ALL {
            assert_eq!(MorphType::from_id(t.id()), Some(t));
            let json = serde_json::to_string(&t).unwrap();
            assert_eq!(json, format!("\"{}\"", t.id()));
        }
        for t in FitMode::ALL {
            assert_eq!(FitMode::from_id(t.id()), Some(t));
            assert_eq!(
                serde_json::to_string(&t).unwrap(),
                format!("\"{}\"", t.id())
            );
        }
        for t in MeshPolicy::ALL {
            assert_eq!(MeshPolicy::from_id(t.id()), Some(t));
            assert_eq!(
                serde_json::to_string(&t).unwrap(),
                format!("\"{}\"", t.id())
            );
        }
        assert_eq!(MorphType::from_id("nope"), None);
    }

    #[test]
    fn older_sequences_get_identity_curves_and_stay_visible() {
        let s: Sequence = serde_json::from_str(r#"{"frames": 12}"#).unwrap();
        assert!(s.feature_curve.is_identity() && s.shape_curve.is_identity());
        assert!(s.visible);
    }

    #[test]
    fn validate_checks_curves() {
        let mut s = Sequence::default();
        s.shape_curve.points.clear();
        assert!(matches!(s.validate(), Err(DocError::BadCurve(_))));
    }

    #[test]
    fn older_sequences_without_hold_fields_still_load() {
        let s: Sequence = serde_json::from_str(r#"{"frames": 12, "morph": "warp1"}"#).unwrap();
        assert_eq!((s.frames, s.hold_start, s.hold_end), (12, 0, 0));
        assert_eq!(s.morph, MorphType::Warp1);
    }
}
