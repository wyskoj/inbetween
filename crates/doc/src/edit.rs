//! Non-destructive edits to a source image.
//!
//! Each image of a sequence carries one [`ImageEdit`]. It is applied in a fixed order:
//! geometry (quarter turns, then flips, then the crop rectangle with its angle), then colour
//! adjustment, then the filter list, then the caption. The file on disk is never changed.
//! This module is pure data; `render-cpu` applies it.

use crate::{Caption, DocError};
use serde::{Deserialize, Serialize};

/// Most filters one image may carry.
pub const MAX_FILTERS: usize = 16;
/// Largest edited image side, in pixels.
pub const MAX_EDIT_SIDE: f64 = 32_768.0;
/// Largest edited image area, in pixels (the raster limit).
pub const MAX_EDIT_PIXELS: f64 = 100_000_000.0;
/// Largest distance a crop may sit from the image, in pixels.
pub const MAX_CROP_OFFSET: f64 = 1_000_000.0;

/// An affine map `[a, b, c, d, e, f]`: `(x, y)` goes to `(a x + b y + c, d x + e y + f)`.
pub type Affine = [f64; 6];

pub const IDENTITY: Affine = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

/// `a` after `b`: applies `b` first.
pub fn affine_mul(a: &Affine, b: &Affine) -> Affine {
    [
        a[0] * b[0] + a[1] * b[3],
        a[0] * b[1] + a[1] * b[4],
        a[0] * b[2] + a[1] * b[5] + a[2],
        a[3] * b[0] + a[4] * b[3],
        a[3] * b[1] + a[4] * b[4],
        a[3] * b[2] + a[4] * b[5] + a[5],
    ]
}

/// The inverse map, or `None` when `m` is singular or not finite.
pub fn affine_inv(m: &Affine) -> Option<Affine> {
    let det = m[0] * m[4] - m[1] * m[3];
    if !det.is_finite() || det.abs() < 1e-12 || m.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let (a, b, d, e) = (m[4] / det, -m[1] / det, -m[3] / det, m[0] / det);
    Some([a, b, -(a * m[2] + b * m[5]), d, e, -(d * m[2] + e * m[5])])
}

/// Apply `m` to a point.
pub fn affine_apply(m: &Affine, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5])
}

/// A crop rectangle in pixels of the image after turning and flipping. It may extend past the
/// image; the part outside is transparent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crop {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Colour adjustment settings. Brightness, contrast, the three channels and saturation run
/// -100 to 100; hue runs -180 to 180 degrees. All zero and both flags off is "no adjustment".
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdjustSettings {
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

impl AdjustSettings {
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(&self) -> Result<(), DocError> {
        let fields = [
            (self.brightness, 100.0, "brightness must be -100 to 100"),
            (self.contrast, 100.0, "contrast must be -100 to 100"),
            (self.red, 100.0, "red must be -100 to 100"),
            (self.green, 100.0, "green must be -100 to 100"),
            (self.blue, 100.0, "blue must be -100 to 100"),
            (self.hue, 180.0, "hue must be -180 to 180"),
            (self.saturation, 100.0, "saturation must be -100 to 100"),
        ];
        for (v, limit, why) in fields {
            if !v.is_finite() || v.abs() > limit {
                return Err(DocError::BadEdit(why));
            }
        }
        Ok(())
    }
}

/// One filter in an image's filter list. Centres are fractions of the image (0.5 is the middle).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Filter {
    /// A 3x3 weighted mean mixed in by `strength` (0 to 1).
    Smooth { strength: f64 },
    /// Box blur by `radius` pixels (1 to 100).
    Blur { radius: u32 },
    /// Gaussian blur with standard deviation `sigma` pixels (0.3 to 100).
    Gaussian { sigma: f64 },
    /// Blur toward a centre by `amount` (0 to 1).
    ZoomBlur { cx: f64, cy: f64, amount: f64 },
    /// Blur around a centre through `angle` degrees (0 to 360).
    RadialBlur { cx: f64, cy: f64, angle: f64 },
    /// Unsharp mask: `amount` (0 to 5) over a blur of `radius` pixels (0.3 to 100).
    Sharpen { amount: f64, radius: f64 },
    /// Relief lit from `angle` degrees, `depth` 0 to 10, over a grey `base` (0 to 1).
    Emboss { angle: f64, depth: f64, base: f64 },
    /// Blocks of `cell` pixels (2 to 512).
    Mosaic { cell: u32 },
    /// Oil paint: `radius` 1 to 16 pixels, `levels` 8 to 256.
    OilPaint { radius: u32, levels: u32 },
    /// Scatter pixels up to `radius` pixels (1 to 100); the same `seed` scatters the same way.
    Splash { radius: u32, seed: u64 },
    /// Bend around a cylinder; `amount` -1 to 1.
    Cylinder { vertical: bool, amount: f64 },
    /// Twist within `radius` pixels by up to `angle` degrees (-720 to 720).
    Whirl {
        cx: f64,
        cy: f64,
        radius: f64,
        angle: f64,
    },
    /// Pinch (positive) or bulge (negative) within `radius` pixels; `amount` -1 to 1.
    Pinch {
        cx: f64,
        cy: f64,
        radius: f64,
        amount: f64,
    },
}

fn within(v: f64, lo: f64, hi: f64, why: &'static str) -> Result<(), DocError> {
    if v.is_finite() && (lo..=hi).contains(&v) {
        Ok(())
    } else {
        Err(DocError::BadEdit(why))
    }
}

fn within_u(v: u32, lo: u32, hi: u32, why: &'static str) -> Result<(), DocError> {
    if (lo..=hi).contains(&v) {
        Ok(())
    } else {
        Err(DocError::BadEdit(why))
    }
}

impl Filter {
    /// The command-style name of the filter (`gaussian`, `zoomBlur`, ...).
    pub fn name(&self) -> &'static str {
        match self {
            Self::Smooth { .. } => "smooth",
            Self::Blur { .. } => "blur",
            Self::Gaussian { .. } => "gaussian",
            Self::ZoomBlur { .. } => "zoomBlur",
            Self::RadialBlur { .. } => "radialBlur",
            Self::Sharpen { .. } => "sharpen",
            Self::Emboss { .. } => "emboss",
            Self::Mosaic { .. } => "mosaic",
            Self::OilPaint { .. } => "oilPaint",
            Self::Splash { .. } => "splash",
            Self::Cylinder { .. } => "cylinder",
            Self::Whirl { .. } => "whirl",
            Self::Pinch { .. } => "pinch",
        }
    }

    pub fn validate(&self) -> Result<(), DocError> {
        const CENTRE: &str = "a filter centre must be between -1 and 2";
        match *self {
            Self::Smooth { strength } => {
                within(strength, 0.0, 1.0, "smooth strength must be 0 to 1")
            }
            Self::Blur { radius } => within_u(radius, 1, 100, "blur radius must be 1 to 100"),
            Self::Gaussian { sigma } => {
                within(sigma, 0.3, 100.0, "gaussian sigma must be 0.3 to 100")
            }
            Self::ZoomBlur { cx, cy, amount } => {
                within(cx, -1.0, 2.0, CENTRE)?;
                within(cy, -1.0, 2.0, CENTRE)?;
                within(amount, 0.0, 1.0, "zoom blur amount must be 0 to 1")
            }
            Self::RadialBlur { cx, cy, angle } => {
                within(cx, -1.0, 2.0, CENTRE)?;
                within(cy, -1.0, 2.0, CENTRE)?;
                within(angle, 0.0, 360.0, "radial blur angle must be 0 to 360")
            }
            Self::Sharpen { amount, radius } => {
                within(amount, 0.0, 5.0, "sharpen amount must be 0 to 5")?;
                within(radius, 0.3, 100.0, "sharpen radius must be 0.3 to 100")
            }
            Self::Emboss { angle, depth, base } => {
                within(angle, -360.0, 360.0, "emboss angle must be -360 to 360")?;
                within(depth, 0.0, 10.0, "emboss depth must be 0 to 10")?;
                within(base, 0.0, 1.0, "emboss base must be 0 to 1")
            }
            Self::Mosaic { cell } => within_u(cell, 2, 512, "mosaic cell must be 2 to 512"),
            Self::OilPaint { radius, levels } => {
                within_u(radius, 1, 16, "oil paint radius must be 1 to 16")?;
                within_u(levels, 8, 256, "oil paint levels must be 8 to 256")
            }
            Self::Splash { radius, .. } => {
                within_u(radius, 1, 100, "splash radius must be 1 to 100")
            }
            Self::Cylinder { amount, .. } => {
                within(amount, -1.0, 1.0, "cylinder amount must be -1 to 1")
            }
            Self::Whirl {
                cx,
                cy,
                radius,
                angle,
            } => {
                within(cx, -1.0, 2.0, CENTRE)?;
                within(cy, -1.0, 2.0, CENTRE)?;
                within(radius, 0.0, 100_000.0, "whirl radius must be 0 to 100000")?;
                within(angle, -720.0, 720.0, "whirl angle must be -720 to 720")
            }
            Self::Pinch {
                cx,
                cy,
                radius,
                amount,
            } => {
                within(cx, -1.0, 2.0, CENTRE)?;
                within(cy, -1.0, 2.0, CENTRE)?;
                within(radius, 0.0, 100_000.0, "pinch radius must be 0 to 100000")?;
                within(amount, -1.0, 1.0, "pinch amount must be -1 to 1")
            }
        }
    }
}

/// The result of [`ImageEdit::geometry`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// Maps a position in the edited image to the same place in the source image.
    pub edited_to_source: Affine,
    /// Size of the edited image in pixels.
    pub width: u32,
    pub height: u32,
}

impl Geometry {
    /// Maps a source position to the edited image, or `None` if the map is singular.
    pub fn source_to_edited(&self) -> Option<Affine> {
        affine_inv(&self.edited_to_source)
    }
}

/// Everything done to one source image before it is morphed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageEdit {
    /// Clockwise quarter turns, 0 to 3. Applied first.
    pub quarter_turns: u8,
    /// Mirror left to right, after turning.
    pub flip_h: bool,
    /// Mirror top to bottom, after turning.
    pub flip_v: bool,
    /// The crop rectangle, in pixels of the turned and flipped image. `None` keeps it all.
    pub crop: Option<Crop>,
    /// Rotates the content clockwise about the crop centre (the image centre with no crop).
    pub angle: f64,
    /// Grow the canvas so the rotated crop fits entirely.
    pub expand: bool,
    pub adjust: AdjustSettings,
    pub filters: Vec<Filter>,
    pub caption: Option<Caption>,
}

impl Default for ImageEdit {
    fn default() -> Self {
        Self {
            quarter_turns: 0,
            flip_h: false,
            flip_v: false,
            crop: None,
            angle: 0.0,
            expand: false,
            adjust: AdjustSettings::default(),
            filters: Vec::new(),
            caption: None,
        }
    }
}

impl ImageEdit {
    /// Whether the edit changes nothing, so the raw source can be used as it is.
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    /// Whether the geometry part (turns, flips, crop, angle) changes nothing.
    pub fn geometry_is_identity(&self) -> bool {
        self.quarter_turns % 4 == 0
            && !self.flip_h
            && !self.flip_v
            && self.crop.is_none()
            && self.angle == 0.0
    }

    /// Whether the geometry is only turns and flips, which are exact pixel permutations.
    pub fn geometry_is_exact(&self) -> bool {
        self.crop.is_none() && self.angle == 0.0
    }

    pub fn validate(&self) -> Result<(), DocError> {
        if self.quarter_turns > 3 {
            return Err(DocError::BadEdit("quarter turns must be 0 to 3"));
        }
        if let Some(c) = self.crop {
            within(
                c.x,
                -MAX_CROP_OFFSET,
                MAX_CROP_OFFSET,
                "crop x is out of range",
            )?;
            within(
                c.y,
                -MAX_CROP_OFFSET,
                MAX_CROP_OFFSET,
                "crop y is out of range",
            )?;
            within(
                c.w,
                1.0,
                MAX_EDIT_SIDE,
                "crop width must be 1 to 32768 pixels",
            )?;
            within(
                c.h,
                1.0,
                MAX_EDIT_SIDE,
                "crop height must be 1 to 32768 pixels",
            )?;
            if c.w * c.h > MAX_EDIT_PIXELS {
                return Err(DocError::BadEdit("crop covers too many pixels"));
            }
        }
        within(self.angle, -360.0, 360.0, "angle must be -360 to 360")?;
        self.adjust.validate()?;
        if self.filters.len() > MAX_FILTERS {
            return Err(DocError::BadEdit("at most 16 filters per image"));
        }
        self.filters.iter().try_for_each(Filter::validate)?;
        if let Some(c) = &self.caption {
            c.validate()?;
        }
        Ok(())
    }

    /// The size of the edited image and the map from it back to the source, for a source of
    /// `src_w x src_h` pixels. This is the single definition of the geometry, used for both
    /// the pixels and for moving dots with their image.
    pub fn geometry(&self, src_w: u32, src_h: u32) -> Result<Geometry, DocError> {
        self.validate()?;
        let (sw, sh) = (f64::from(src_w), f64::from(src_h));
        let turns = self.quarter_turns % 4;
        let (tw, th) = if turns % 2 == 1 { (sh, sw) } else { (sw, sh) };
        let crop = self.crop.unwrap_or(Crop {
            x: 0.0,
            y: 0.0,
            w: tw,
            h: th,
        });
        let (sin, cos) = self.angle.to_radians().sin_cos();
        let (fw, fh) = if self.expand && self.angle != 0.0 {
            (
                crop.w * cos.abs() + crop.h * sin.abs(),
                crop.w * sin.abs() + crop.h * cos.abs(),
            )
        } else {
            (crop.w, crop.h)
        };
        let (ow, oh) = (fw.round().max(1.0), fh.round().max(1.0));
        if ow > MAX_EDIT_SIDE || oh > MAX_EDIT_SIDE || ow * oh > MAX_EDIT_PIXELS {
            return Err(DocError::BadEdit("the edited image would be too large"));
        }
        // Edited -> turned-and-flipped image: rotate about the crop centre.
        let (ccx, ccy) = (crop.x + crop.w / 2.0, crop.y + crop.h / 2.0);
        let (ox, oy) = (ow / 2.0, oh / 2.0);
        let to_turned: Affine = [
            cos,
            sin,
            ccx - cos * ox - sin * oy,
            -sin,
            cos,
            ccy + sin * ox - cos * oy,
        ];
        // Turned-and-flipped -> turned: undo the mirrors.
        let unflip: Affine = [
            if self.flip_h { -1.0 } else { 1.0 },
            0.0,
            if self.flip_h { tw } else { 0.0 },
            0.0,
            if self.flip_v { -1.0 } else { 1.0 },
            if self.flip_v { th } else { 0.0 },
        ];
        // Turned -> source: undo the clockwise quarter turns.
        let unturn: Affine = match turns {
            1 => [0.0, 1.0, 0.0, -1.0, 0.0, sh],
            2 => [-1.0, 0.0, sw, 0.0, -1.0, sh],
            3 => [0.0, -1.0, sw, 1.0, 0.0, 0.0],
            _ => IDENTITY,
        };
        let map = affine_mul(&unturn, &affine_mul(&unflip, &to_turned));
        Ok(Geometry {
            edited_to_source: map,
            width: ow as u32,
            height: oh as u32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn default_is_identity_and_geometry_is_exact() {
        let e = ImageEdit::default();
        assert!(e.is_identity() && e.geometry_is_identity() && e.validate().is_ok());
        let g = e.geometry(40, 30).unwrap();
        assert_eq!((g.width, g.height), (40, 30));
        assert_eq!(g.edited_to_source, IDENTITY);
    }

    #[test]
    fn turns_swap_the_size_and_map_corners() {
        let e = ImageEdit {
            quarter_turns: 1,
            ..ImageEdit::default()
        };
        let g = e.geometry(40, 30).unwrap();
        assert_eq!((g.width, g.height), (30, 40));
        // The top-right corner of a clockwise-turned image is the source's top-left corner.
        assert!(close(
            affine_apply(&g.edited_to_source, 30.0, 0.0),
            (0.0, 0.0)
        ));
        assert!(close(
            affine_apply(&g.edited_to_source, 0.0, 0.0),
            (0.0, 30.0)
        ));
        let ccw = ImageEdit {
            quarter_turns: 3,
            ..ImageEdit::default()
        };
        let g = ccw.geometry(40, 30).unwrap();
        assert!(close(
            affine_apply(&g.edited_to_source, 0.0, 0.0),
            (40.0, 0.0)
        ));
        let half = ImageEdit {
            quarter_turns: 2,
            ..ImageEdit::default()
        };
        let g = half.geometry(40, 30).unwrap();
        assert!(close(
            affine_apply(&g.edited_to_source, 0.0, 0.0),
            (40.0, 30.0)
        ));
    }

    #[test]
    fn flip_mirrors_after_the_turn() {
        let e = ImageEdit {
            flip_h: true,
            ..ImageEdit::default()
        };
        let g = e.geometry(40, 30).unwrap();
        assert!(close(
            affine_apply(&g.edited_to_source, 0.0, 5.0),
            (40.0, 5.0)
        ));
        let e = ImageEdit {
            quarter_turns: 1,
            flip_v: true,
            ..ImageEdit::default()
        };
        let g = e.geometry(40, 30).unwrap();
        // Turned image is 30 x 40; flipping it vertically sends (0, 0) to (0, 40) in turned
        // space, which is source (40, 30 - 0).
        assert!(close(
            affine_apply(&g.edited_to_source, 0.0, 0.0),
            (40.0, 30.0)
        ));
    }

    #[test]
    fn crop_offsets_and_may_leave_the_image() {
        let e = ImageEdit {
            crop: Some(Crop {
                x: -10.0,
                y: 5.0,
                w: 100.0,
                h: 20.0,
            }),
            ..ImageEdit::default()
        };
        let g = e.geometry(40, 30).unwrap();
        assert_eq!((g.width, g.height), (100, 20));
        assert!(close(
            affine_apply(&g.edited_to_source, 0.0, 0.0),
            (-10.0, 5.0)
        ));
    }

    #[test]
    fn angle_rotates_clockwise_about_the_centre_and_expand_grows() {
        let e = ImageEdit {
            angle: 90.0,
            ..ImageEdit::default()
        };
        let g = e.geometry(40, 40).unwrap();
        // A clockwise content turn moves the source's right-middle to the output's bottom-middle,
        // so the output's bottom-middle reads the source's right-middle.
        assert!(close(
            affine_apply(&g.edited_to_source, 20.0, 40.0),
            (40.0, 20.0)
        ));
        let e = ImageEdit {
            angle: 90.0,
            expand: true,
            ..ImageEdit::default()
        };
        let g = e.geometry(40, 20).unwrap();
        assert_eq!((g.width, g.height), (20, 40));
        let e = ImageEdit {
            angle: 45.0,
            expand: true,
            ..ImageEdit::default()
        };
        let g = e.geometry(100, 100).unwrap();
        assert_eq!((g.width, g.height), (141, 141));
    }

    #[test]
    fn inverse_round_trips() {
        let e = ImageEdit {
            quarter_turns: 3,
            flip_h: true,
            crop: Some(Crop {
                x: 3.0,
                y: -4.0,
                w: 31.0,
                h: 17.0,
            }),
            angle: 17.5,
            ..ImageEdit::default()
        };
        let g = e.geometry(40, 30).unwrap();
        let back = g.source_to_edited().unwrap();
        let (sx, sy) = affine_apply(&g.edited_to_source, 7.25, 3.5);
        assert!(close(affine_apply(&back, sx, sy), (7.25, 3.5)));
    }

    #[test]
    fn bounds_are_refused() {
        let bad = |f: fn(&mut ImageEdit)| {
            let mut e = ImageEdit::default();
            f(&mut e);
            e.validate().is_err() && e.geometry(10, 10).is_err()
        };
        assert!(bad(|e| e.quarter_turns = 4));
        assert!(bad(|e| e.angle = f64::NAN));
        assert!(bad(|e| e.crop = Some(Crop {
            x: f64::NAN,
            y: 0.0,
            w: 5.0,
            h: 5.0
        })));
        assert!(bad(|e| e.crop = Some(Crop {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 5.0
        })));
        assert!(bad(|e| e.crop = Some(Crop {
            x: 0.0,
            y: 0.0,
            w: 32768.0,
            h: 32768.0
        })));
        assert!(bad(|e| e.adjust.brightness = 101.0));
        assert!(bad(
            |e| e.filters = vec![Filter::Blur { radius: 1 }; MAX_FILTERS + 1]
        ));
        assert!(bad(
            |e| e.filters = vec![Filter::Gaussian { sigma: f64::NAN }]
        ));
        assert!(bad(|e| e.filters = vec![Filter::Blur { radius: 0 }]));
        assert!(bad(|e| e.filters = vec![Filter::Mosaic { cell: 1_000_000 }]));
        assert!(bad(|e| e.filters = vec![Filter::Whirl {
            cx: 0.5,
            cy: 0.5,
            radius: 1e9,
            angle: 0.0
        }]));
        // A large-but-allowed expand that would overflow the size limit is refused by geometry.
        let e = ImageEdit {
            angle: 45.0,
            expand: true,
            crop: Some(Crop {
                x: 0.0,
                y: 0.0,
                w: 32768.0,
                h: 3000.0,
            }),
            ..ImageEdit::default()
        };
        assert!(e.geometry(10, 10).is_err());
    }

    #[test]
    fn filter_json_is_tagged() {
        let f: Filter = serde_json::from_str(r#"{"kind":"gaussian","sigma":4}"#).unwrap();
        assert_eq!(f, Filter::Gaussian { sigma: 4.0 });
        let text = serde_json::to_string(&Filter::ZoomBlur {
            cx: 0.5,
            cy: 0.5,
            amount: 0.25,
        })
        .unwrap();
        assert!(text.contains(r#""kind":"zoomBlur""#));
        assert!(serde_json::from_str::<Filter>(r#"{"kind":"nope"}"#).is_err());
    }
}
