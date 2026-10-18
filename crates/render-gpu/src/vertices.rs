use bytemuck::{Pod, Zeroable};
use inbetween_geom::{Affine, Point};
use inbetween_morph::FramePlan;

/// One mesh vertex: where it sits in the output frame and where it reads from on each
/// source image (in that image's own pixel coordinates, after fitting).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub s1: [f32; 2],
    pub s2: [f32; 2],
}

fn pt(p: Point) -> [f32; 2] {
    [p.x as f32, p.y as f32]
}

/// Vertices for one frame: first a full-frame quad showing both sources unwarped (the
/// oracle's fallback for pixels no triangle claims), then three vertices per mesh triangle in
/// plan order. `fit1` and `fit2` map output coordinates into source-image coordinates.
pub fn build_vertices(
    plan: &FramePlan,
    fit1: &Affine,
    fit2: &Affine,
    width: u32,
    height: u32,
) -> Vec<Vertex> {
    let mut out = Vec::with_capacity(6 + plan.tris.len() * 3);
    let (w, h) = (f64::from(width), f64::from(height));
    let corner = |x: f64, y: f64| {
        let p = Point::new(x, y);
        Vertex {
            pos: pt(p),
            s1: pt(fit1.apply(p)),
            s2: pt(fit2.apply(p)),
        }
    };
    let (a, b, c, d) = (
        corner(0.0, 0.0),
        corner(w, 0.0),
        corner(w, h),
        corner(0.0, h),
    );
    out.extend([a, b, c, a, c, d]);

    for tri in &plan.tris {
        let dst = tri.dst;
        if dst.is_degenerate() {
            continue;
        }
        // Inverse maps: this frame's triangle -> each source triangle. A sliver source
        // triangle has none, and that image is then sampled unwarped (as in the oracle).
        let to1 = Affine::from_triangles(&dst, &tri.src1).ok();
        let to2 = Affine::from_triangles(&dst, &tri.src2).ok();
        for p in [dst.a, dst.b, dst.c] {
            let q1 = to1.map_or(p, |m| m.apply(p));
            let q2 = to2.map_or(p, |m| m.apply(p));
            out.push(Vertex {
                pos: pt(p),
                s1: pt(fit1.apply(q1)),
                s2: pt(fit2.apply(q2)),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::{DotPair, DotSet, Sequence};
    use inbetween_morph::Planner;

    fn plan(t: f64) -> FramePlan {
        let seq = Sequence {
            dots: DotSet::new(vec![DotPair::new(
                Point::new(30.0, 20.0),
                Point::new(50.0, 30.0),
            )]),
            ..Sequence::default()
        };
        Planner::new(&seq, 100, 60).and_then(|p| p.plan(t)).unwrap()
    }

    #[test]
    fn quad_comes_first_then_three_vertices_per_triangle() {
        let p = plan(0.5);
        let v = build_vertices(&p, &Affine::IDENTITY, &Affine::IDENTITY, 100, 60);
        assert_eq!(v.len(), 6 + p.tris.len() * 3);
        assert_eq!(v[0].pos, [0.0, 0.0]);
        assert_eq!(v[2].pos, [100.0, 60.0]);
        assert_eq!(v[5].pos, [0.0, 60.0]);
    }

    #[test]
    fn at_t0_sources_equal_positions() {
        // At t = 0 the frame triangle is Image 1's triangle, so the map is the identity.
        let p = plan(0.0);
        let v = build_vertices(&p, &Affine::IDENTITY, &Affine::IDENTITY, 100, 60);
        for vert in &v[6..] {
            assert!(
                (vert.pos[0] - vert.s1[0]).abs() < 1e-3 && (vert.pos[1] - vert.s1[1]).abs() < 1e-3
            );
        }
    }

    #[test]
    fn at_t1_second_source_equals_positions() {
        let p = plan(1.0);
        let v = build_vertices(&p, &Affine::IDENTITY, &Affine::IDENTITY, 100, 60);
        for vert in &v[6..] {
            assert!(
                (vert.pos[0] - vert.s2[0]).abs() < 1e-3 && (vert.pos[1] - vert.s2[1]).abs() < 1e-3
            );
        }
    }

    #[test]
    fn fit_is_applied_to_every_vertex() {
        let fit = Affine {
            a: 2.0,
            b: 0.0,
            c: 0.0,
            d: 2.0,
            tx: 5.0,
            ty: 0.0,
        };
        let p = plan(0.0);
        let v = build_vertices(&p, &fit, &Affine::IDENTITY, 100, 60);
        assert_eq!(v[1].s1, [205.0, 0.0]);
        assert_eq!(v[1].s2, [100.0, 0.0]);
    }

    #[test]
    fn vertex_is_tightly_packed() {
        assert_eq!(std::mem::size_of::<Vertex>(), 24);
    }
}
