use crate::MorphError;
use inbetween_doc::{MeshPolicy, MorphType, Sequence};
use inbetween_geom::{triangulate, GeomError, Point, Triangle, Triangulation};

/// Morph time of frame `index` in a sequence of `frames` with no hold frames: 0 for the first
/// frame, 1 for the last, evenly spaced between. A one-frame sequence is just t = 0. Sequences
/// with hold frames use [`inbetween_doc::Sequence::frame_time`].
pub fn frame_t(index: u32, frames: u32) -> f64 {
    if frames <= 1 {
        0.0
    } else if index >= frames - 1 {
        1.0
    } else {
        f64::from(index) / f64::from(frames - 1)
    }
}

/// One mesh triangle in all three coordinate systems, in output-frame pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshTri {
    /// The triangle on Image 1.
    pub src1: Triangle,
    /// The same triangle on Image 2.
    pub src2: Triangle,
    /// Where it sits in this frame (dots interpolated to time `t`).
    pub dst: Triangle,
}

/// How the two warped samples of a pixel combine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixMode {
    /// `w1 * s1 + w2 * s2` on premultiplied colour: a cross-dissolve when the weights sum to 1.
    Add,
    /// Image 2, scaled by `w2`, laid over Image 1 scaled by `w1`. Image 1 shows through wherever
    /// Image 2 is transparent, so it never fades.
    Over,
}

/// The per-pixel recipe for one frame, chosen by the morph type and the morph time. Both
/// renderers apply the same recipe:
///
/// 1. Each sample keeps its alpha; its colour is multiplied by `bright` (1 = unchanged,
///    0 = black), which is how a fade to or from black works.
/// 2. The two samples are combined by `mode` with the weights `w1` and `w2`. A weight of
///    zero means that image is never sampled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mix {
    pub mode: MixMode,
    pub w1: f32,
    pub w2: f32,
    pub bright1: f32,
    pub bright2: f32,
}

impl Mix {
    const fn add(w1: f32, w2: f32) -> Self {
        Self {
            mode: MixMode::Add,
            w1,
            w2,
            bright1: 1.0,
            bright2: 1.0,
        }
    }

    /// The recipe for `morph` at morph time `t` (0 to 1; anything else is clamped).
    pub fn for_type(morph: MorphType, t: f64) -> Self {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) } as f32;
        match morph {
            MorphType::Morph => Self::add(1.0 - t, t),
            MorphType::MorphHold => Self {
                mode: MixMode::Over,
                w1: 1.0,
                w2: t,
                ..Self::add(0.0, 0.0)
            },
            MorphType::Warp1 => Self::add(1.0, 0.0),
            MorphType::Warp2 => Self::add(0.0, 1.0),
            MorphType::FadeOut1 => Self {
                bright1: 1.0 - t,
                ..Self::add(1.0, 0.0)
            },
            MorphType::FadeIn2 => Self {
                bright2: t,
                ..Self::add(0.0, 1.0)
            },
            MorphType::Direct => {
                if t >= 1.0 {
                    Self::add(0.0, 1.0)
                } else {
                    Self::add(1.0, 0.0)
                }
            }
        }
    }

    pub fn uses_image1(&self) -> bool {
        self.w1 != 0.0
    }

    pub fn uses_image2(&self) -> bool {
        self.w2 != 0.0
    }
}

/// Everything a renderer needs to draw one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct FramePlan {
    /// Morph time, 0 to 1.
    pub t: f64,
    pub mix: Mix,
    pub tris: Vec<MeshTri>,
}

/// What the planner keeps between frames.
#[derive(Clone, Debug)]
enum Kind {
    /// The dots deform the images.
    Warp {
        policy: MeshPolicy,
        dot_count: usize,
        /// Triangulation of Image 1's dots: the mesh shared by every frame.
        base: Triangulation,
        /// Triangulation of Image 2's dots: used for the last frame so it matches Image 2.
        end: Triangulation,
        first: Vec<Point>,
        second: Vec<Point>,
    },
    /// Fades and the direct cut: the whole frame as two triangles, mapped to itself.
    Still(Vec<MeshTri>),
}

/// Builds frame plans for one sequence. The expensive triangulations happen once, in
/// [`Planner::new`], so planning a frame afterwards is cheap.
#[derive(Clone, Debug)]
pub struct Planner {
    width: u32,
    height: u32,
    morph: MorphType,
    /// The sequence's timing only (frames and holds); no dots.
    timing: Sequence,
    kind: Kind,
}

fn tri(points: &[Point], idx: [usize; 3]) -> Option<Triangle> {
    Some(Triangle::new(
        *points.get(idx[0])?,
        *points.get(idx[1])?,
        *points.get(idx[2])?,
    ))
}

fn map_geom(e: GeomError, image: u8) -> MorphError {
    match e {
        GeomError::DuplicateDot { first, second } => MorphError::DuplicateDot {
            image,
            first,
            second,
        },
        other => MorphError::Geom(other),
    }
}

impl Planner {
    /// Validate `seq` and, for morph types that warp, triangulate both dot sets for an output of
    /// `width` x `height`. Types that never warp ignore the dots, so bad dots cannot stop them.
    pub fn new(seq: &Sequence, width: u32, height: u32) -> Result<Self, MorphError> {
        if width == 0 || height == 0 {
            return Err(MorphError::BadSize);
        }
        seq.validate()?;
        let (w, h) = (f64::from(width), f64::from(height));
        let kind = if seq.morph.warps() {
            let first = seq.dots.first_positions();
            let second = seq.dots.second_positions();
            let base = triangulate(&first, w, h).map_err(|e| map_geom(e, 1))?;
            let end = triangulate(&second, w, h).map_err(|e| map_geom(e, 2))?;
            Kind::Warp {
                policy: seq.mesh,
                dot_count: first.len(),
                first,
                second,
                base,
                end,
            }
        } else {
            let (a, b, c, d) = (
                Point::new(0.0, 0.0),
                Point::new(w, 0.0),
                Point::new(w, h),
                Point::new(0.0, h),
            );
            let still = |t: Triangle| MeshTri {
                src1: t,
                src2: t,
                dst: t,
            };
            Kind::Still(vec![
                still(Triangle::new(a, b, c)),
                still(Triangle::new(a, c, d)),
            ])
        };
        Ok(Self {
            width,
            height,
            morph: seq.morph,
            timing: Sequence {
                frames: seq.frames,
                hold_start: seq.hold_start,
                hold_end: seq.hold_end,
                ..Sequence::default()
            },
            kind,
        })
    }

    /// Every frame of the sequence: hold, morph, hold.
    pub fn total_frames(&self) -> u32 {
        self.timing.total_frames()
    }

    /// Plan the frame at morph time `t` (clamped to 0..=1; NaN counts as 0).
    pub fn plan(&self, t: f64) -> Result<FramePlan, MorphError> {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        let mix = Mix::for_type(self.morph, t);
        let (dot_count, policy, base, end, first, second) = match &self.kind {
            Kind::Still(tris) => {
                return Ok(FramePlan {
                    t,
                    mix,
                    tris: tris.clone(),
                })
            }
            Kind::Warp {
                dot_count,
                policy,
                base,
                end,
                first,
                second,
            } => (*dot_count, *policy, base, end, first, second),
        };

        // Dots at time t, then the frame corners (which never move).
        let mut now: Vec<Point> = first
            .iter()
            .zip(second)
            .map(|(a, b)| a.lerp(*b, t))
            .collect();
        let per_frame;
        let connectivity: &[[usize; 3]] = if t >= 1.0 {
            &end.triangles
        } else if t > 0.0 && policy == MeshPolicy::PerFrame {
            // If two dots cross exactly at this t, keep the shared mesh for this frame.
            match triangulate(&now, f64::from(self.width), f64::from(self.height)) {
                Ok(tr) => {
                    per_frame = tr.triangles;
                    &per_frame
                }
                Err(_) => &base.triangles,
            }
        } else {
            &base.triangles
        };
        now.extend(base.vertices.iter().skip(dot_count));

        let tris = connectivity
            .iter()
            .filter_map(|&idx| {
                Some(MeshTri {
                    src1: tri(&base.vertices, idx)?,
                    src2: tri(&end.vertices, idx)?,
                    dst: tri(&now, idx)?,
                })
            })
            .collect();
        Ok(FramePlan { t, mix, tris })
    }

    /// Plan frame `index` of the sequence, counting hold frames (an index past the end gives
    /// the last frame).
    pub fn plan_frame(&self, index: u32) -> Result<FramePlan, MorphError> {
        self.plan(self.timing.frame_time(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::{DotPair, DotSet};

    type Xy = (f64, f64);

    fn seq(pairs: &[(Xy, Xy)]) -> Sequence {
        let pairs = pairs
            .iter()
            .map(|&((a, b), (c, d))| DotPair::new(Point::new(a, b), Point::new(c, d)))
            .collect();
        Sequence {
            dots: DotSet::new(pairs),
            frames: 5,
            ..Sequence::default()
        }
    }

    fn two_dots() -> Sequence {
        seq(&[((30.0, 20.0), (40.0, 30.0)), ((70.0, 30.0), (60.0, 20.0))])
    }

    #[test]
    fn frame_times_span_zero_to_one() {
        assert_eq!(frame_t(0, 5), 0.0);
        assert_eq!(frame_t(2, 5), 0.5);
        assert_eq!(frame_t(4, 5), 1.0);
        assert_eq!(frame_t(99, 5), 1.0);
        assert_eq!(frame_t(0, 1), 0.0);
        assert_eq!(frame_t(7, 1), 0.0);
        assert_eq!(frame_t(0, 0), 0.0);
        assert_eq!(frame_t(u32::MAX, u32::MAX), 1.0);
    }

    #[test]
    fn no_dots_still_plans_two_triangles() {
        let p = Planner::new(&seq(&[]), 100, 50).unwrap();
        assert_eq!(p.plan(0.5).unwrap().tris.len(), 2);
    }

    #[test]
    fn ends_of_the_plan_match_the_sources() {
        let p = Planner::new(&two_dots(), 100, 50).unwrap();
        for t in [0.0, 1.0] {
            let plan = p.plan(t).unwrap();
            for tr in &plan.tris {
                let want = if t == 0.0 { tr.src1 } else { tr.src2 };
                assert_eq!(tr.dst, want, "t = {t}");
            }
        }
    }

    #[test]
    fn mesh_is_shared_between_frames_by_default() {
        let p = Planner::new(&two_dots(), 100, 50).unwrap();
        let a = p.plan(0.25).unwrap();
        let b = p.plan(0.75).unwrap();
        assert_eq!(a.tris.len(), b.tris.len());
        for (x, y) in a.tris.iter().zip(&b.tris) {
            assert_eq!(x.src1, y.src1);
            assert_eq!(x.src2, y.src2);
        }
    }

    #[test]
    fn per_frame_policy_retriangulates_but_survives_crossing_dots() {
        let mut s = seq(&[((30.0, 20.0), (70.0, 20.0)), ((70.0, 20.0), (30.0, 20.0))]);
        s.mesh = MeshPolicy::PerFrame;
        let p = Planner::new(&s, 100, 50).unwrap();
        // At t = 0.5 both dots land on (50, 20): triangulation fails, plan falls back.
        assert!(!p.plan(0.5).unwrap().tris.is_empty());
        assert!(!p.plan(0.3).unwrap().tris.is_empty());
    }

    #[test]
    fn duplicate_dots_name_the_image() {
        let s = seq(&[((10.0, 10.0), (10.0, 10.0)), ((10.0, 10.0), (20.0, 20.0))]);
        assert_eq!(
            Planner::new(&s, 100, 50).unwrap_err(),
            MorphError::DuplicateDot {
                image: 1,
                first: 0,
                second: 1
            }
        );
        let s = seq(&[((10.0, 10.0), (20.0, 20.0)), ((30.0, 10.0), (20.0, 20.0))]);
        assert_eq!(
            Planner::new(&s, 100, 50).unwrap_err(),
            MorphError::DuplicateDot {
                image: 2,
                first: 0,
                second: 1
            }
        );
    }

    #[test]
    fn bad_input_is_an_error() {
        assert_eq!(
            Planner::new(&seq(&[]), 0, 50).unwrap_err(),
            MorphError::BadSize
        );
        let mut s = seq(&[]);
        s.frames = 0;
        assert!(matches!(Planner::new(&s, 10, 10), Err(MorphError::Doc(_))));
        let s = seq(&[((f64::NAN, 0.0), (1.0, 1.0))]);
        assert!(matches!(Planner::new(&s, 10, 10), Err(MorphError::Doc(_))));
        let mut s = seq(&[]);
        s.hold_start = 5000;
        assert!(matches!(Planner::new(&s, 10, 10), Err(MorphError::Doc(_))));
    }

    #[test]
    fn plan_clamps_t() {
        let p = Planner::new(&seq(&[((30.0, 20.0), (40.0, 30.0))]), 100, 50).unwrap();
        assert_eq!(p.plan(-4.0).unwrap().t, 0.0);
        assert_eq!(p.plan(9.0).unwrap().t, 1.0);
        assert_eq!(p.plan(f64::NAN).unwrap().t, 0.0);
    }

    // ---- mixes and morph types ----

    #[test]
    fn each_morph_type_has_the_documented_recipe() {
        use MorphType::*;
        let at = |m, t| Mix::for_type(m, t);
        assert_eq!(at(Morph, 0.25), Mix::add(0.75, 0.25));
        assert_eq!(at(Warp1, 0.9), Mix::add(1.0, 0.0));
        assert_eq!(at(Warp2, 0.1), Mix::add(0.0, 1.0));
        let hold = at(MorphHold, 0.4);
        assert_eq!((hold.mode, hold.w1, hold.w2), (MixMode::Over, 1.0, 0.4));
        let out = at(FadeOut1, 0.25);
        assert_eq!((out.w1, out.w2, out.bright1), (1.0, 0.0, 0.75));
        let fin = at(FadeIn2, 0.25);
        assert_eq!((fin.w1, fin.w2, fin.bright2), (0.0, 1.0, 0.25));
        assert_eq!(at(Direct, 0.99), Mix::add(1.0, 0.0));
        assert_eq!(at(Direct, 1.0), Mix::add(0.0, 1.0));
    }

    #[test]
    fn endpoints_of_every_type_are_what_the_spec_says() {
        use MorphType::*;
        let first = |m| Mix::for_type(m, 0.0);
        let last = |m| Mix::for_type(m, 1.0);
        // t = 0 shows Image 1 untouched for every type that starts from it.
        for m in [Morph, MorphHold, Warp1, FadeOut1, Direct] {
            assert_eq!(
                (first(m).w1, first(m).w2, first(m).bright1),
                (1.0, 0.0, 1.0),
                "{m:?}"
            );
        }
        // t = 1 shows Image 2 untouched for every type that ends on it.
        for m in [Morph, Warp2, FadeIn2, Direct] {
            assert_eq!(
                (last(m).w1, last(m).w2, last(m).bright2),
                (0.0, 1.0, 1.0),
                "{m:?}"
            );
        }
        // Fade out ends black; fade in starts black; Warp 2 starts as pure Image 2.
        assert_eq!(last(FadeOut1).bright1, 0.0);
        assert_eq!(first(FadeIn2).bright2, 0.0);
        assert_eq!((first(Warp2).w1, first(Warp2).w2), (0.0, 1.0));
        // Morph Hold keeps Image 1 at full weight all the way.
        assert_eq!(last(MorphHold).w1, 1.0);
        assert_eq!(last(MorphHold).w2, 1.0);
    }

    #[test]
    fn mixes_never_go_out_of_range() {
        for m in MorphType::ALL {
            for t in [-5.0, 0.0, 0.3, 1.0, 7.0, f64::NAN, f64::INFINITY] {
                let x = Mix::for_type(m, t);
                for v in [x.w1, x.w2, x.bright1, x.bright2] {
                    assert!((0.0..=1.0).contains(&v), "{m:?} t={t}: {x:?}");
                }
            }
        }
    }

    #[test]
    fn fades_and_direct_ignore_the_dots_entirely() {
        for m in [MorphType::FadeOut1, MorphType::FadeIn2, MorphType::Direct] {
            // Duplicate dots would be an error for a warping type.
            let mut s = seq(&[((5.0, 5.0), (6.0, 6.0)), ((5.0, 5.0), (9.0, 9.0))]);
            s.morph = m;
            let p = Planner::new(&s, 100, 50).unwrap();
            let plan = p.plan(0.5).unwrap();
            assert_eq!(plan.tris.len(), 2);
            for tr in &plan.tris {
                assert_eq!((tr.dst, tr.src2), (tr.src1, tr.src1), "{m:?} must not warp");
            }
            let area: f64 = plan.tris.iter().map(|t| t.dst.area()).sum();
            assert!(
                (area - 5000.0).abs() < 1e-9,
                "the two triangles cover the frame"
            );
        }
        let mut s = seq(&[((5.0, 5.0), (6.0, 6.0)), ((5.0, 5.0), (9.0, 9.0))]);
        s.morph = MorphType::Morph;
        assert!(Planner::new(&s, 100, 50).is_err());
    }

    #[test]
    fn warping_types_all_use_the_dot_mesh() {
        for m in [
            MorphType::Morph,
            MorphType::MorphHold,
            MorphType::Warp1,
            MorphType::Warp2,
        ] {
            let mut s = two_dots();
            s.morph = m;
            let p = Planner::new(&s, 100, 50).unwrap();
            assert!(p.plan(0.5).unwrap().tris.len() > 2, "{m:?}");
        }
    }

    #[test]
    fn hold_frames_repeat_the_end_plans() {
        let mut s = two_dots();
        s.frames = 5;
        s.hold_start = 2;
        s.hold_end = 3;
        let p = Planner::new(&s, 100, 50).unwrap();
        assert_eq!(p.total_frames(), 10);
        let t_of = |i| p.plan_frame(i).unwrap().t;
        assert_eq!([t_of(0), t_of(1), t_of(2)], [0.0, 0.0, 0.0]);
        assert_eq!(t_of(4), 0.5);
        assert_eq!([t_of(6), t_of(7), t_of(9), t_of(500)], [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(p.plan_frame(0).unwrap(), p.plan_frame(1).unwrap());
        assert_eq!(p.plan_frame(6).unwrap(), p.plan_frame(9).unwrap());
    }
}
