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

pub mod camera;
pub mod caption;
mod effects;
pub mod movie_pass;
pub mod source;
pub mod trail;

pub use effects::{light_pixel, mask_factor, ray_strength, transition_amount, EffectImages};

use inbetween_doc::{Camera, Curve, Effects, Filter, MorphType, Sequence, Trail};
use inbetween_geom::{Affine, Point};
use inbetween_morph::{
    fit_transform, transparent_outside, FramePlan, MeshTri, Mix, MixMode, MorphError, Planner,
};
use inbetween_raster::{clamp_valid, over, scale, Image, Pixel, RasterError, TRANSPARENT};
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
    /// An effect is switched on but its image was not given.
    MissingEffectImage(&'static str),
    /// An image edit is out of range.
    Edit(inbetween_doc::DocError),
    /// A caption font file could not be used.
    Font(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Morph(e) => write!(f, "{e}"),
            Self::Raster(e) => write!(f, "{e}"),
            Self::MissingImage2 => write!(f, "this morph type needs Image 2, which is not loaded"),
            Self::MissingEffectImage(which) => {
                write!(f, "the {which} effect is on but its image is not loaded")
            }
            Self::Edit(e) => write!(f, "{e}"),
            Self::Font(why) => write!(f, "caption font: {why}"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<MorphError> for RenderError {
    fn from(e: MorphError) -> Self {
        Self::Morph(e)
    }
}

impl From<inbetween_doc::DocError> for RenderError {
    fn from(e: inbetween_doc::DocError) -> Self {
        Self::Edit(e)
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
    width: u32,
    height: u32,
    source1: Source<'a>,
    source2: Option<Source<'a>>,
    effects: Effects,
    effect_images: EffectImages<'a>,
    /// The sequence's caption, drawn once at the output size.
    caption: Option<&'a Image>,
    morph: MorphType,
    /// Frame counts and holds only, for frame times.
    timing: Sequence,
    feature: Curve,
    camera: Camera,
    master: Curve,
    filters: Vec<Filter>,
}

/// The transition effect for one frame: the image and the colour-dissolve time.
struct TransitionCtx<'i> {
    image: &'i Image,
    softness: f64,
    invert: bool,
    t: f64,
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
            width,
            height,
            source1,
            source2,
            effects: seq.effects.clone(),
            effect_images: EffectImages::default(),
            caption: None,
            morph: seq.morph,
            timing: Sequence {
                frames: seq.frames,
                hold_start: seq.hold_start,
                hold_end: seq.hold_end,
                ..Sequence::default()
            },
            feature: seq.feature_curve.clone(),
            camera: seq.camera.clone(),
            master: seq.master_curve.clone(),
            filters: seq.filters.clone(),
        })
    }

    /// Give the renderer the images of the sequence's enabled effects. Rendering fails with
    /// `MissingEffectImage` if an enabled effect has none.
    pub fn with_effect_images(mut self, images: EffectImages<'a>) -> Self {
        self.effect_images = images;
        self
    }

    /// Lay a drawn caption layer (see [`caption::render_layer`], at the output size) over every
    /// frame, after the effects and before the master fade.
    pub fn with_caption(mut self, layer: Option<&'a Image>) -> Self {
        self.caption = layer;
        self
    }

    /// Every frame of the sequence, hold frames included.
    pub fn frame_count(&self) -> u32 {
        self.planner.total_frames()
    }

    /// Render frame `index`, counting hold frames (clamped to the last frame). This is the one
    /// call that knows the frames before it, so it is the one that draws the trail.
    pub fn render_frame(&self, index: u32) -> Result<Image, RenderError> {
        let t = self.timing.frame_time(index);
        self.render_at(t, Some(index))
    }

    /// Render at morph time `t` (0 = Image 1, 1 = Image 2). No trail: that needs a frame index.
    pub fn render_t(&self, t: f64) -> Result<Image, RenderError> {
        self.render_at(t, None)
    }

    fn render_at(&self, t: f64, index: Option<u32>) -> Result<Image, RenderError> {
        // Check first so a missing effect image fails before the expensive morph.
        effects::check(&self.effects, &self.effect_images)?;
        let plan = self.planner.plan(t)?;
        // Effects sit under the camera (they move with it), so the trail is taken from the
        // morph in the sequence's own frame and the camera moves the finished composite.
        let mut image = self.draw(&plan)?;
        if let Some(index) = index.filter(|_| self.effects.trail.enabled) {
            image = self.with_trail(image, index, plan.t)?;
        }
        effects::apply(&self.effects, &self.effect_images, &mut image, plan.t)?;
        let mut image = camera::apply(&self.camera, plan.t, image)?;
        for f in &self.filters {
            image = source::apply_filter(&image, f)?;
        }
        if let Some(layer) = self.caption {
            caption::composite(&mut image, layer);
        }
        if !self.master.is_opaque() {
            let k = self.master.eval(plan.t) as f32;
            for v in image.as_mut_slice() {
                *v *= k;
            }
        }
        Ok(image)
    }

    /// Mix in the frames before `index` as the trail effect says.
    fn with_trail(&self, current: Image, index: u32, t: f64) -> Result<Image, RenderError> {
        let settings: Trail = self.effects.trail;
        let count = trail::past_frames(settings.intensity);
        let mut past = Vec::new();
        for back in 1..=count {
            let Some(earlier) = index.checked_sub(back) else {
                break;
            };
            let at = self.timing.frame_time(earlier);
            // A hold repeats the picture; it adds nothing to a trail.
            if at != t {
                past.push(self.draw(&self.planner.plan(at)?)?);
            }
        }
        trail::combine(&settings, current, &past)
    }

    fn transition(&self, plan: &FramePlan) -> Option<TransitionCtx<'_>> {
        let tr = &self.effects.transition;
        let image = self.effect_images.transition.filter(|_| tr.enabled)?;
        Some(TransitionCtx {
            image,
            softness: tr.softness,
            invert: tr.invert,
            t: self.feature.eval(plan.t),
        })
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
        let transition = self.transition(plan);
        let mut out = Image::new(self.width, self.height)?;
        let row_len = self.width as usize * 4;
        out.as_mut_slice()
            .par_chunks_mut(row_len * BAND_ROWS as usize)
            .enumerate()
            .for_each(|(band, pixels)| {
                let y = band as u32 * BAND_ROWS;
                self.shade_band(pixels, y, &setups, plan, transition.as_ref());
            });
        Ok(out)
    }

    /// Shade the rows held in `pixels`, which start at output row `y_start`.
    fn shade_band(
        &self,
        pixels: &mut [f32],
        y_start: u32,
        setups: &[TriSetup],
        plan: &FramePlan,
        transition: Option<&TransitionCtx>,
    ) {
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
        // sampled unwarped, unless the corner triangles were dropped: then they are transparent.
        let mix = &plan.mix;
        for (i, (px, &who)) in pixels.chunks_exact_mut(4).zip(&owner).enumerate() {
            let x = (i % width) as u32;
            let y = y_start + (i / width) as u32;
            let p = Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            // With a transition every pixel dissolves on its own schedule.
            let own;
            let mix = match transition {
                Some(tr) => {
                    let light = effects::luminance(effects::sample(
                        tr.image,
                        x,
                        y,
                        self.width,
                        self.height,
                    ));
                    let amount = transition_amount(light, tr.t, tr.softness, tr.invert);
                    own = Mix::for_type(self.morph, amount);
                    &own
                }
                None => mix,
            };
            let colour = match setups.get(who as usize) {
                Some(s) => {
                    let q1 = s.to1.map_or(p, |m| m.apply(p));
                    let q2 = s.to2.map_or(p, |m| m.apply(p));
                    self.colour(q1, q2, mix)
                }
                None if plan.backdrop => TRANSPARENT,
                None => self.colour(p, p, mix),
            };
            px.copy_from_slice(&colour);
        }
    }

    /// Colour for an output pixel whose source positions on the two images are `q1`, `q2`.
    fn colour(&self, q1: Point, q2: Point, mix: &Mix) -> Pixel {
        // An image whose weight is zero is never sampled.
        let s1 = if mix.uses_image1() {
            self.source1.sample(q1)
        } else {
            TRANSPARENT
        };
        let s2 = match &self.source2 {
            Some(s) if mix.uses_image2() => s.sample(q2),
            _ => TRANSPARENT,
        };
        clamp_valid(mix_pixels(mix, s1, s2))
    }
}

/// Combine one pixel's two samples as the frame's [`Mix`] says: dim each colour (alpha kept),
/// then add them with their weights or lay Image 2 over Image 1.
pub fn mix_pixels(mix: &Mix, s1: Pixel, s2: Pixel) -> Pixel {
    let dim = |p: Pixel, b: f32| [p[0] * b, p[1] * b, p[2] * b, p[3]];
    let (a, b) = (dim(s1, mix.bright1), dim(s2, mix.bright2));
    match mix.mode {
        MixMode::Add => [
            a[0] * mix.w1 + b[0] * mix.w2,
            a[1] * mix.w1 + b[1] * mix.w2,
            a[2] * mix.w1 + b[2] * mix.w2,
            a[3] * mix.w1 + b[3] * mix.w2,
        ],
        MixMode::Over => {
            let top = scale(b, mix.w2);
            let under = scale(a, mix.w1);
            over(top, under)
        }
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
