// Mesh renderer. The CPU computes, per vertex, the position in the output frame and the
// matching position on each source image (already fitted to the output size). Because the
// map from a frame triangle to a source triangle is affine, the GPU's linear interpolation
// of those source positions reproduces the per-pixel inverse map the CPU oracle uses.
//
// Sampling mirrors `Image::sample_bilinear` in `inbetween-raster`: pixel centres sit at +0.5,
// edges clamp, and interpolation happens on premultiplied values.

struct Params {
    out_size: vec2<f32>,
    dims1: vec2<f32>,
    dims2: vec2<f32>,
    // Per-frame recipe, as `Mix` in `inbetween-morph`: dim each sample (alpha kept), then
    // add them with weights w1, w2 (mode 0) or lay Image 2 over Image 1 (mode 1).
    w1: f32,
    w2: f32,
    bright1: f32,
    bright2: f32,
    mode: u32,
    // bit 0: Image 1 reads transparent outside its bounds (Contain)
    // bit 1: Image 2 reads transparent outside its bounds
    // bit 2: Image 2 is present
    flags: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var source1: texture_2d<f32>;
@group(0) @binding(2) var source2: texture_2d<f32>;

struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) s1: vec2<f32>,
    @location(2) s2: vec2<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) s1: vec2<f32>,
    @location(1) s2: vec2<f32>,
}

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    let ndc = vec2<f32>(
        v.pos.x / params.out_size.x * 2.0 - 1.0,
        1.0 - v.pos.y / params.out_size.y * 2.0,
    );
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.s1 = v.s1;
    out.s2 = v.s2;
    return out;
}

const EDGE_EPS: f32 = 1e-9;
const LIMIT: f32 = 1.0e9;

fn fetch1(x: i32, y: i32) -> vec4<f32> {
    let hi = vec2<i32>(params.dims1) - vec2<i32>(1, 1);
    return textureLoad(source1, vec2<i32>(clamp(x, 0, hi.x), clamp(y, 0, hi.y)), 0);
}

fn fetch2(x: i32, y: i32) -> vec4<f32> {
    let hi = vec2<i32>(params.dims2) - vec2<i32>(1, 1);
    return textureLoad(source2, vec2<i32>(clamp(x, 0, hi.x), clamp(y, 0, hi.y)), 0);
}

fn sample1(s_in: vec2<f32>) -> vec4<f32> {
    let s = clamp(s_in, vec2<f32>(-LIMIT), vec2<f32>(LIMIT));
    if ((params.flags & 1u) != 0u) {
        if (s.x < -EDGE_EPS || s.y < -EDGE_EPS || s.x > params.dims1.x + EDGE_EPS || s.y > params.dims1.y + EDGE_EPS) {
            return vec4<f32>(0.0);
        }
    }
    let f = s - vec2<f32>(0.5);
    let base = floor(f);
    let t = f - base;
    let x0 = i32(base.x);
    let y0 = i32(base.y);
    let top = mix(fetch1(x0, y0), fetch1(x0 + 1, y0), t.x);
    let bottom = mix(fetch1(x0, y0 + 1), fetch1(x0 + 1, y0 + 1), t.x);
    return mix(top, bottom, t.y);
}

fn sample2(s_in: vec2<f32>) -> vec4<f32> {
    let s = clamp(s_in, vec2<f32>(-LIMIT), vec2<f32>(LIMIT));
    if ((params.flags & 2u) != 0u) {
        if (s.x < -EDGE_EPS || s.y < -EDGE_EPS || s.x > params.dims2.x + EDGE_EPS || s.y > params.dims2.y + EDGE_EPS) {
            return vec4<f32>(0.0);
        }
    }
    let f = s - vec2<f32>(0.5);
    let base = floor(f);
    let t = f - base;
    let x0 = i32(base.x);
    let y0 = i32(base.y);
    let top = mix(fetch2(x0, y0), fetch2(x0 + 1, y0), t.x);
    let bottom = mix(fetch2(x0, y0 + 1), fetch2(x0 + 1, y0 + 1), t.x);
    return mix(top, bottom, t.y);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // A zero weight means that image is never sampled (the CPU does the same).
    var s1 = vec4<f32>(0.0);
    if (params.w1 != 0.0) {
        s1 = sample1(in.s1);
    }
    var s2 = vec4<f32>(0.0);
    if ((params.flags & 4u) != 0u && params.w2 != 0.0) {
        s2 = sample2(in.s2);
    }
    let a = vec4<f32>(s1.rgb * params.bright1, s1.a);
    let b = vec4<f32>(s2.rgb * params.bright2, s2.a);
    var c: vec4<f32>;
    if (params.mode == 1u) {
        let top = b * params.w2;
        c = top + a * params.w1 * (1.0 - top.a);
    } else {
        c = a * params.w1 + b * params.w2;
    }
    // Same repair as the CPU's `clamp_valid`: alpha in 0..1, colour in 0..alpha.
    let alpha = clamp(c.a, 0.0, 1.0);
    return vec4<f32>(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(alpha)), alpha);
}
