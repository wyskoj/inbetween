//! The CPU renderer: the reference oracle for every other renderer.
//!
//! A frame is built triangle by triangle. For each output pixel centre inside a mesh
//! triangle, the inverse affine map finds the matching point on each source image, which is
//! sampled bilinearly; the two samples are then cross-dissolved. Triangles render in
//! parallel (rayon) into separate buffers that are merged in triangle order, so the result
//! is the same bytes on every run and every thread count.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use inbetween_doc::Sequence;
use inbetween_geom::{Affine, Point};
use inbetween_morph::{
    fit_transform, frame_t, transparent_outside, FramePlan, MeshTri, MorphError, Planner,
};
use inbetween_raster::{clamp_valid, lerp, Image, Pixel, RasterError, TRANSPARENT};
use rayon::prelude::*;
use std::fmt;

/// Slack when deciding whether a pixel centre lies on a shared triangle edge, so edge
/// pixels are never dropped by rounding.
const EDGE_EPS: f64 = 1e-9;

#[derive(Clone, Debug, PartialEq)]
pub enum RenderError {
    Morph(MorphError),
    Raster(RasterError),
    /// The morph type needs Image 2 but none was given.
    MissingImage2,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Morph(e) => write!(f, "{e}"),
            Self::Raster(e) => write!(f, "{e}"),
            Self::MissingImage2 => write!(f, "this morph type needs Image 2, which is not loaded"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<MorphError> for RenderError {
    fn from(e: MorphError) -> Self {
        Self::Morph(e)
    }
}

impl From<RasterError> for RenderError {
    fn from(e: RasterError) -> Self {
        Self::Raster(e)
    }
}

/// One source image with the map from output coordinates into it.
struct Source<'a> {
    image: &'a Image,
    /// Output-frame coordinates -> source-image coordinates.
    fit: Affine,
    transparent_outside: bool,
}

impl Source<'_> {
    fn sample(&self, q: Point) -> Pixel {
        let s = self.fit.apply(q);
        if self.transparent_outside {
            let (w, h) = (
                f64::from(self.image.width()),
                f64::from(self.image.height()),
            );
            if s.x < -EDGE_EPS || s.y < -EDGE_EPS || s.x > w + EDGE_EPS || s.y > h + EDGE_EPS {
                return TRANSPARENT;
            }
        }
        self.image.sample_bilinear(s.x as f32, s.y as f32)
    }
}

/// Renders the frames of one sequence. Build it once; render any number of frames.
pub struct Renderer<'a> {
    planner: Planner,
    frames: u32,
    width: u32,
    height: u32,
    source1: Source<'a>,
    source2: Option<Source<'a>>,
}

impl<'a> Renderer<'a> {
    /// `output` is the frame size in pixels. `image2` may be `None` only for morph types
    /// that never show Image 2 (such as Warp 1).
    pub fn new(
        seq: &Sequence,
        output: (u32, u32),
        image1: &'a Image,
        image2: Option<&'a Image>,
    ) -> Result<Self, RenderError> {
        let (width, height) = output;
        // Reject bad output sizes (and absurd ones) before anything is allocated.
        Image::new(width, height)?;
        let planner = Planner::new(seq, width, height)?;
        let source = |image: &'a Image| -> Result<Source<'a>, RenderError> {
            Ok(Source {
                image,
                fit: fit_transform(image.width(), image.height(), width, height, seq.fit)?,
                transparent_outside: transparent_outside(seq.fit),
            })
        };
        let source1 = source(image1)?;
        let source2 = match image2 {
            Some(img) => Some(source(img)?),
            None if seq.morph.uses_image2() => return Err(RenderError::MissingImage2),
            None => None,
        };
        Ok(Self {
            planner,
            frames: seq.frames,
            width,
            height,
            source1,
            source2,
        })
    }

    pub fn frame_count(&self) -> u32 {
        self.frames
    }

    /// Render frame `index` (clamped to the last frame).
    pub fn render_frame(&self, index: u32) -> Result<Image, RenderError> {
        self.render_t(frame_t(index, self.frames))
    }

    /// Render at morph time `t` (0 = Image 1, 1 = Image 2).
    pub fn render_t(&self, t: f64) -> Result<Image, RenderError> {
        let plan = self.planner.plan(t)?;
        self.draw(&plan)
    }

    fn draw(&self, plan: &FramePlan) -> Result<Image, RenderError> {
        // Per-triangle setup runs in parallel; `collect` keeps triangle order.
        let setups: Vec<TriSetup> = plan
            .tris
            .par_iter()
            .filter_map(|tri| TriSetup::new(tri, self.width, self.height))
            .collect();

        // Rows are shaded in independent bands. Inside a band, triangles claim pixels in
        // triangle order (the last claim wins), so the result does not depend on how the
        // bands are scheduled across threads.
        let mut out = Image::new(self.width, self.height)?;
        let row_len = self.width as usize * 4;
        out.as_mut_slice()
            .par_chunks_mut(row_len * BAND_ROWS as usize)
            .enumerate()
            .for_each(|(band, pixels)| {
                self.shade_band(pixels, band as u32 * BAND_ROWS, &setups, plan.dissolve);
            });
        Ok(out)
    }

    /// Shade the rows held in `pixels`, which start at output row `y_start`.
    fn shade_band(&self, pixels: &mut [f32], y_start: u32, setups: &[TriSetup], dissolve: f32) {
        let width = self.width as usize;
        let rows = (pixels.len() / (width * 4)) as u32;
        let y_end = y_start + rows;

        // Pass 1: which triangle owns each pixel centre.
        let mut owner = vec![NO_OWNER; width * rows as usize];
        for (index, s) in setups.iter().enumerate() {
            if s.y1 < y_start || s.y0 >= y_end {
                continue;
            }
            for y in s.y0.max(y_start)..=s.y1.min(y_end - 1) {
                let base = (y - y_start) as usize * width;
                for x in s.x0..=s.x1 {
                    if s.covers(Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5)) {
                        if let Some(slot) = owner.get_mut(base + x as usize) {
                            *slot = index as u32;
                        }
                    }
                }
            }
        }

        // Pass 2: colour. Pixels no triangle claimed (folded meshes can leave slivers) are
        // sampled unwarped.
        for (i, (px, &who)) in pixels.chunks_exact_mut(4).zip(&owner).enumerate() {
            let x = (i % width) as u32;
            let y = y_start + (i / width) as u32;
            let p = Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            let colour = match setups.get(who as usize) {
                Some(s) => {
                    let q1 = s.to1.map_or(p, |m| m.apply(p));
                    let q2 = s.to2.map_or(p, |m| m.apply(p));
                    self.colour(q1, q2, dissolve)
                }
                None => self.colour(p, p, dissolve),
            };
            px.copy_from_slice(&colour);
        }
    }

    /// Colour for an output pixel whose source positions on the two images are `q1`, `q2`.
    fn colour(&self, q1: Point, q2: Point, dissolve: f32) -> Pixel {
        let a = self.source1.sample(q1);
        let px = match &self.source2 {
            Some(s2) if dissolve > 0.0 => lerp(a, s2.sample(q2), dissolve),
            _ => a,
        };
        clamp_valid(px)
    }
}

/// Rows per parallel band: small enough to balance threads, large enough to amortise setup.
const BAND_ROWS: u32 = 16;
/// Owner value for pixels no triangle claims.
const NO_OWNER: u32 = u32::MAX;

/// Everything about one mesh triangle that pixel shading needs, computed once per frame.
struct TriSetup {
    /// Pixel range the triangle can touch (inclusive).
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
    a: Point,
    e1: Point,
    e2: Point,
    det: f64,
    /// Maps from this frame's triangle to each source triangle. `None` when the source
    /// triangle is a sliver; that image is then sampled unwarped here.
    to1: Option<Affine>,
    to2: Option<Affine>,
}

impl TriSetup {
    fn new(tri: &MeshTri, width: u32, height: u32) -> Option<Self> {
        let dst = tri.dst;
        if dst.is_degenerate() {
            return None;
        }
        let b = dst.bounds()?;
        let clamp_range = |lo: f64, hi: f64, limit: u32| -> Option<(u32, u32)> {
            // Pixel x has its centre at x + 0.5.
            let first = (lo - 0.5 - EDGE_EPS).ceil().max(0.0);
            let last = (hi - 0.5 + EDGE_EPS).floor().min(f64::from(limit) - 1.0);
            (first <= last).then_some((first as u32, last as u32))
        };
        let (x0, x1) = clamp_range(b.min.x, b.max.x, width)?;
        let (y0, y1) = clamp_range(b.min.y, b.max.y, height)?;
        let (e1, e2) = (dst.b - dst.a, dst.c - dst.a);
        Some(Self {
            x0,
            x1,
            y0,
            y1,
            a: dst.a,
            e1,
            e2,
            det: e1.cross(e2),
            to1: Affine::from_triangles(&dst, &tri.src1).ok(),
            to2: Affine::from_triangles(&dst, &tri.src2).ok(),
        })
    }

    /// Whether pixel centre `p` lies inside the triangle (edges count, with slack so shared
    /// edges leave no gaps).
    fn covers(&self, p: Point) -> bool {
        let d = p - self.a;
        let wb = d.cross(self.e2) / self.det;
        let wc = self.e1.cross(d) / self.det;
        let wa = 1.0 - wb - wc;
        !(wa < -EDGE_EPS || wb < -EDGE_EPS || wc < -EDGE_EPS)
    }
}

/// Render one frame of `seq`. Convenience over [`Renderer`] when you only need one.
pub fn render_frame(
    seq: &Sequence,
    output: (u32, u32),
    image1: &Image,
    image2: Option<&Image>,
    index: u32,
) -> Result<Image, RenderError> {
    Renderer::new(seq, output, image1, image2)?.render_frame(index)
}
