use crate::MorphError;
use inbetween_doc::{MeshPolicy, Sequence};
use inbetween_geom::{triangulate, GeomError, Point, Triangle, Triangulation};

/// Morph time of frame `index` in a sequence of `frames`: 0 for the first frame, 1 for the
/// last, evenly spaced between. A one-frame sequence is just Image 1.
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

/// Everything a renderer needs to draw one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct FramePlan {
    /// Morph time, 0 to 1.
    pub t: f64,
    /// How much of Image 2 shows: `t` for a morph, 0 when Image 2's colours never appear.
    pub dissolve: f32,
    pub tris: Vec<MeshTri>,
}

/// Builds frame plans for one sequence. The expensive triangulations happen once, in
/// [`Planner::new`], so planning a frame afterwards is cheap.
#[derive(Clone, Debug)]
pub struct Planner {
    width: u32,
    height: u32,
    uses_image2: bool,
    policy: MeshPolicy,
    dot_count: usize,
    /// Triangulation of Image 1's dots: the mesh shared by every frame.
    base: Triangulation,
    /// Triangulation of Image 2's dots: used for the last frame so it matches Image 2.
    end: Triangulation,
    first: Vec<Point>,
    second: Vec<Point>,
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
    /// Validate `seq` and triangulate both dot sets for an output of `width` x `height`.
    pub fn new(seq: &Sequence, width: u32, height: u32) -> Result<Self, MorphError> {
        if width == 0 || height == 0 {
            return Err(MorphError::BadSize);
        }
        seq.validate()?;
        let first = seq.dots.first_positions();
        let second = seq.dots.second_positions();
        let (w, h) = (f64::from(width), f64::from(height));
        let base = triangulate(&first, w, h).map_err(|e| map_geom(e, 1))?;
        let end = triangulate(&second, w, h).map_err(|e| map_geom(e, 2))?;
        Ok(Self {
            width,
            height,
            uses_image2: seq.morph.uses_image2(),
            policy: seq.mesh,
            dot_count: first.len(),
            first,
            second,
            base,
            end,
        })
    }

    /// Plan the frame at morph time `t` (clamped to 0..=1; NaN counts as 0).
    pub fn plan(&self, t: f64) -> Result<FramePlan, MorphError> {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };

        // Dots at time t, then the frame corners (which never move).
        let mut now: Vec<Point> = self
            .first
            .iter()
            .zip(&self.second)
            .map(|(a, b)| a.lerp(*b, t))
            .collect();
        let per_frame;
        let connectivity: &[[usize; 3]] = if t >= 1.0 {
            &self.end.triangles
        } else if t > 0.0 && self.policy == MeshPolicy::PerFrame {
            // If two dots cross exactly at this t, keep the shared mesh for this frame.
            match triangulate(&now, f64::from(self.width), f64::from(self.height)) {
                Ok(tr) => {
                    per_frame = tr.triangles;
                    &per_frame
                }
                Err(_) => &self.base.triangles,
            }
        } else {
            &self.base.triangles
        };
        now.extend(self.base.vertices.iter().skip(self.dot_count));

        let tris = connectivity
            .iter()
            .filter_map(|&idx| {
                Some(MeshTri {
                    src1: tri(&self.base.vertices, idx)?,
                    src2: tri(&self.end.vertices, idx)?,
                    dst: tri(&now, idx)?,
                })
            })
            .collect();
        let dissolve = if self.uses_image2 { t as f32 } else { 0.0 };
        Ok(FramePlan { t, dissolve, tris })
    }

    /// Plan frame `index` of a `frames`-frame sequence.
    pub fn plan_frame(&self, index: u32, frames: u32) -> Result<FramePlan, MorphError> {
        self.plan(frame_t(index, frames))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::{DotPair, DotSet, MorphType};

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
        let s = seq(&[((30.0, 20.0), (40.0, 30.0)), ((70.0, 30.0), (60.0, 20.0))]);
        let p = Planner::new(&s, 100, 50).unwrap();
        for t in [0.0, 1.0] {
            let plan = p.plan(t).unwrap();
            for tr in &plan.tris {
                let want = if t == 0.0 { tr.src1 } else { tr.src2 };
                assert_eq!(tr.dst, want, "t = {t}");
            }
        }
        assert_eq!(p.plan(0.0).unwrap().dissolve, 0.0);
        assert_eq!(p.plan(1.0).unwrap().dissolve, 1.0);
    }

    #[test]
    fn mesh_is_shared_between_frames_by_default() {
        let s = seq(&[((30.0, 20.0), (40.0, 30.0)), ((70.0, 30.0), (60.0, 20.0))]);
        let p = Planner::new(&s, 100, 50).unwrap();
        let a = p.plan(0.25).unwrap();
        let b = p.plan(0.75).unwrap();
        assert_eq!(a.tris.len(), b.tris.len());
        for (x, y) in a.tris.iter().zip(&b.tris) {
            assert_eq!(x.src1, y.src1);
            assert_eq!(x.src2, y.src2);
        }
    }

    #[test]
    fn warp_never_dissolves() {
        let mut s = seq(&[((30.0, 20.0), (40.0, 30.0))]);
        s.morph = MorphType::Warp1;
        let p = Planner::new(&s, 100, 50).unwrap();
        assert_eq!(p.plan(0.8).unwrap().dissolve, 0.0);
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
    }

    #[test]
    fn plan_clamps_t() {
        let p = Planner::new(&seq(&[((30.0, 20.0), (40.0, 30.0))]), 100, 50).unwrap();
        assert_eq!(p.plan(-4.0).unwrap().t, 0.0);
        assert_eq!(p.plan(9.0).unwrap().t, 1.0);
        assert_eq!(p.plan(f64::NAN).unwrap().t, 0.0);
    }
}
