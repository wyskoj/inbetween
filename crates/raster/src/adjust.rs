//! Colour adjustment: brightness, contrast, channel balance, hue, saturation, negative and
//! grayscale. Works on straight colour; alpha is untouched.

use crate::par::map;
use crate::{Image, Pixel, RasterError};

/// Adjustment settings. Brightness, contrast, red, green, blue and saturation run
/// `-100..=100`, hue `-180..=180` degrees. All zero and both flags off is the identity.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Adjust {
    pub brightness: f64,
    pub contrast: f64,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub hue: f64,
    pub saturation: f64,
    pub negative: bool,
    pub grayscale: bool,
}

impl Adjust {
    /// True when applying this changes nothing.
    pub fn is_identity(&self) -> bool {
        *self == Adjust::default()
    }
}

/// Brightness (Rec. 709 luma) of a premultiplied pixel on a black backdrop, in `0..=1`.
pub fn luminance(p: Pixel) -> f32 {
    (0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]).clamp(0.0, 1.0)
}

fn finite(v: f64, lo: f64, hi: f64) -> f32 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(lo, hi) as f32
    }
}

/// Apply `a` to every pixel.
pub fn adjust(img: &Image, a: &Adjust) -> Result<Image, RasterError> {
    if a.is_identity() {
        return Ok(img.clone());
    }
    let brightness = finite(a.brightness, -100.0, 100.0) / 100.0;
    let k = finite(a.contrast, -100.0, 100.0) / 100.0;
    let contrast = if k >= 0.0 {
        1.0 / (1.0 - k * 0.99)
    } else {
        1.0 + k
    };
    let shift = [
        finite(a.red, -100.0, 100.0) / 100.0,
        finite(a.green, -100.0, 100.0) / 100.0,
        finite(a.blue, -100.0, 100.0) / 100.0,
    ];
    let hue = finite(a.hue, -180.0, 180.0).to_radians();
    let (hs, hc) = hue.sin_cos();
    let sat = 1.0 + finite(a.saturation, -100.0, 100.0) / 100.0;
    // Hue rotation about the grey axis (the standard colour-matrix form).
    let m = [
        [
            0.213 + hc * 0.787 - hs * 0.213,
            0.715 - hc * 0.715 - hs * 0.715,
            0.072 - hc * 0.072 + hs * 0.928,
        ],
        [
            0.213 - hc * 0.213 + hs * 0.143,
            0.715 + hc * 0.285 + hs * 0.140,
            0.072 - hc * 0.072 - hs * 0.283,
        ],
        [
            0.213 - hc * 0.213 - hs * 0.787,
            0.715 - hc * 0.715 + hs * 0.715,
            0.072 + hc * 0.928 + hs * 0.072,
        ],
    ];
    let (negative, grayscale) = (a.negative, a.grayscale);
    map(img, move |p| {
        let al = p[3];
        if al.is_nan() || al <= 0.0 {
            return p;
        }
        let mut c = [p[0] / al, p[1] / al, p[2] / al];
        for (v, s) in c.iter_mut().zip(shift) {
            *v = ((*v + brightness + s).clamp(0.0, 1.0) - 0.5) * contrast + 0.5;
            *v = v.clamp(0.0, 1.0);
        }
        if hue != 0.0 {
            let src = c;
            for (v, row) in c.iter_mut().zip(m) {
                *v = row[0] * src[0] + row[1] * src[1] + row[2] * src[2];
            }
        }
        if sat != 1.0 {
            let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            for v in &mut c {
                *v = l + (*v - l) * sat;
            }
        }
        for v in &mut c {
            *v = if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
        }
        if negative {
            for v in &mut c {
                *v = 1.0 - *v;
            }
        }
        if grayscale {
            let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            c = [l, l, l];
        }
        [c[0] * al, c[1] * al, c[2] * al, al]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(r: f32, g: f32, b: f32, a: f32) -> Image {
        Image::from_premultiplied(1, 1, vec![r * a, g * a, b * a, a]).unwrap()
    }

    fn first(img: &Image) -> Pixel {
        img.get(0, 0).unwrap()
    }

    #[test]
    fn identity_is_exact() {
        let img = px(0.3, 0.6, 0.9, 0.5);
        assert_eq!(adjust(&img, &Adjust::default()).unwrap(), img);
    }

    #[test]
    fn brightness_negative_grayscale() {
        let img = px(0.2, 0.2, 0.2, 1.0);
        let b = adjust(
            &img,
            &Adjust {
                brightness: 50.0,
                ..Adjust::default()
            },
        )
        .unwrap();
        assert!((first(&b)[0] - 0.7).abs() < 1e-5);
        let n = adjust(
            &img,
            &Adjust {
                negative: true,
                ..Adjust::default()
            },
        )
        .unwrap();
        assert!((first(&n)[0] - 0.8).abs() < 1e-5);
        let g = adjust(
            &px(1.0, 0.0, 0.0, 1.0),
            &Adjust {
                grayscale: true,
                ..Adjust::default()
            },
        )
        .unwrap();
        let p = first(&g);
        assert!((p[0] - 0.2126).abs() < 1e-5 && p[0] == p[1] && p[1] == p[2]);
    }

    #[test]
    fn alpha_untouched_and_premultiplied() {
        let img = px(0.5, 0.5, 0.5, 0.5);
        let out = adjust(
            &img,
            &Adjust {
                brightness: 100.0,
                ..Adjust::default()
            },
        )
        .unwrap();
        let p = first(&out);
        assert_eq!(p[3], 0.5);
        assert!((p[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn hostile_values_stay_valid() {
        let img = px(0.1, 0.9, 0.4, 1.0);
        let a = Adjust {
            brightness: f64::NAN,
            contrast: 1e9,
            red: f64::INFINITY,
            hue: -1e9,
            saturation: 1e9,
            ..Adjust::default()
        };
        let out = adjust(&img, &a).unwrap();
        assert!(out
            .as_slice()
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0 && *v <= 1.0));
    }

    #[test]
    fn full_desaturation_matches_luma() {
        let out = adjust(
            &px(1.0, 0.0, 0.0, 1.0),
            &Adjust {
                saturation: -100.0,
                ..Adjust::default()
            },
        )
        .unwrap();
        let p = first(&out);
        assert!((p[0] - 0.2126).abs() < 1e-5 && (p[1] - 0.2126).abs() < 1e-5);
    }
}
