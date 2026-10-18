/// One premultiplied RGBA pixel: `[r, g, b, a]`, each normally in `0..=1` with `r, g, b <= a`.
pub type Pixel = [f32; 4];

pub const TRANSPARENT: Pixel = [0.0; 4];

/// Porter-Duff "over": `src` composited on top of `dst`.
pub fn over(src: Pixel, dst: Pixel) -> Pixel {
    let k = 1.0 - src[3];
    [
        src[0] + dst[0] * k,
        src[1] + dst[1] * k,
        src[2] + dst[2] * k,
        src[3] + dst[3] * k,
    ]
}

/// Linear blend of two premultiplied pixels: `a` at `t = 0`, `b` at `t = 1`.
/// Premultiplied blending is alpha-correct: a transparent pixel adds no colour.
/// `t` is clamped to `0..=1`; NaN counts as 0.
pub fn lerp(a: Pixel, b: Pixel, t: f32) -> Pixel {
    let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
    let u = 1.0 - t;
    [
        a[0] * u + b[0] * t,
        a[1] * u + b[1] * t,
        a[2] * u + b[2] * t,
        a[3] * u + b[3] * t,
    ]
}

/// Multiply every channel by `k` (fade toward transparent).
pub fn scale(p: Pixel, k: f32) -> Pixel {
    [p[0] * k, p[1] * k, p[2] * k, p[3] * k]
}

/// Force a pixel into the valid premultiplied range: no NaN, alpha in `0..=1`, and each
/// colour channel in `0..=alpha`.
pub fn clamp_valid(p: Pixel) -> Pixel {
    let fix = |v: f32| if v.is_nan() { 0.0 } else { v };
    let a = fix(p[3]).clamp(0.0, 1.0);
    [
        fix(p[0]).clamp(0.0, a),
        fix(p[1]).clamp(0.0, a),
        fix(p[2]).clamp(0.0, a),
        a,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn over_opaque_source_hides_destination() {
        let s = [0.2, 0.4, 0.6, 1.0];
        assert_eq!(over(s, [0.9, 0.9, 0.9, 1.0]), s);
    }

    #[test]
    fn over_transparent_source_keeps_destination() {
        let d = [0.1, 0.2, 0.3, 0.5];
        assert_eq!(over(TRANSPARENT, d), d);
    }

    #[test]
    fn over_half_alpha_white_on_black() {
        let out = over([0.5, 0.5, 0.5, 0.5], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(out, [0.5, 0.5, 0.5, 1.0]);
    }

    #[test]
    fn lerp_endpoints_and_midpoint() {
        let a = [0.0, 0.0, 0.0, 1.0];
        let b = [1.0, 0.5, 0.0, 1.0];
        assert_eq!(lerp(a, b, 0.0), a);
        assert_eq!(lerp(a, b, 1.0), b);
        assert_eq!(lerp(a, b, 0.5), [0.5, 0.25, 0.0, 1.0]);
    }

    #[test]
    fn lerp_is_alpha_correct_against_transparent() {
        // Dissolving a red pixel into nothing keeps it red (premultiplied: scales to zero).
        let red = [1.0, 0.0, 0.0, 1.0];
        let mid = lerp(red, TRANSPARENT, 0.5);
        assert_eq!(mid, [0.5, 0.0, 0.0, 0.5]);
        // Un-premultiplied that is still pure red at half alpha.
        assert_eq!(mid[0] / mid[3], 1.0);
    }

    #[test]
    fn lerp_clamps_t_and_ignores_nan() {
        let a = [0.0; 4];
        let b = [1.0; 4];
        assert_eq!(lerp(a, b, -3.0), a);
        assert_eq!(lerp(a, b, 7.0), b);
        assert_eq!(lerp(a, b, f32::NAN), a);
    }

    #[test]
    fn clamp_valid_repairs_bad_pixels() {
        assert_eq!(
            clamp_valid([f32::NAN, 2.0, -1.0, 0.5]),
            [0.0, 0.5, 0.0, 0.5]
        );
        assert_eq!(clamp_valid([0.3, 0.3, 0.3, 7.0]), [0.3, 0.3, 0.3, 1.0]);
        assert_eq!(clamp_valid([1.0, 1.0, 1.0, f32::NAN]), TRANSPARENT);
    }

    #[test]
    fn scale_fades() {
        assert_eq!(scale([1.0, 0.5, 0.0, 1.0], 0.5), [0.5, 0.25, 0.0, 0.5]);
    }
}
