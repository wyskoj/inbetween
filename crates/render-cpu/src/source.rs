//! Applying an [`ImageEdit`] to a loaded source image.
//!
//! The order is fixed: geometry, then colour adjustment, then the filter list, then the
//! caption. An identity edit returns `None` so the caller keeps the original image untouched.

use crate::RenderError;
use inbetween_doc::{AdjustSettings, Filter, ImageEdit};
use inbetween_raster::{filter, transform, Adjust, Image};

fn adjust_settings(a: &AdjustSettings) -> Adjust {
    Adjust {
        brightness: a.brightness,
        contrast: a.contrast,
        red: a.red,
        green: a.green,
        blue: a.blue,
        hue: a.hue,
        saturation: a.saturation,
        negative: a.negative,
        grayscale: a.grayscale,
    }
}

/// Run one filter over `img`.
pub fn apply_filter(img: &Image, f: &Filter) -> Result<Image, RenderError> {
    Ok(match *f {
        Filter::Smooth { strength } => filter::smooth(img, strength)?,
        Filter::Blur { radius } => filter::box_blur(img, radius)?,
        Filter::Gaussian { sigma } => filter::gaussian(img, sigma)?,
        Filter::ZoomBlur { cx, cy, amount } => filter::zoom_blur(img, cx, cy, amount)?,
        Filter::RadialBlur { cx, cy, angle } => filter::radial_blur(img, cx, cy, angle)?,
        Filter::Sharpen { amount, radius } => filter::sharpen(img, amount, radius)?,
        Filter::Emboss { angle, depth, base } => filter::emboss(img, angle, depth, base)?,
        Filter::Mosaic { cell } => filter::mosaic(img, cell)?,
        Filter::OilPaint { radius, levels } => filter::oil_paint(img, radius, levels)?,
        Filter::Splash { radius, seed } => filter::splash(img, radius, seed)?,
        Filter::Cylinder { vertical, amount } => filter::cylinder(img, vertical, amount)?,
        Filter::Whirl {
            cx,
            cy,
            radius,
            angle,
        } => filter::whirl(img, cx, cy, radius, angle)?,
        Filter::Pinch {
            cx,
            cy,
            radius,
            amount,
        } => filter::pinch(img, cx, cy, radius, amount)?,
    })
}

/// The edited image, or `None` when `edit` is the identity.
pub fn apply_edit(img: &Image, edit: &ImageEdit) -> Result<Option<Image>, RenderError> {
    if edit.is_identity() {
        return Ok(None);
    }
    edit.validate()?;
    let mut cur: Option<Image> = None;
    if !edit.geometry_is_identity() {
        let next = if edit.geometry_is_exact() {
            let turned = transform::rotate_quarter(img, edit.quarter_turns)?;
            transform::flip(&turned, edit.flip_h, edit.flip_v)?
        } else {
            let g = edit.geometry(img.width(), img.height())?;
            transform::resample(img, g.width, g.height, g.edited_to_source)?
        };
        cur = Some(next);
    }
    if !edit.adjust.is_identity() {
        let next =
            inbetween_raster::adjust(cur.as_ref().unwrap_or(img), &adjust_settings(&edit.adjust))?;
        cur = Some(next);
    }
    for f in &edit.filters {
        let next = apply_filter(cur.as_ref().unwrap_or(img), f)?;
        cur = Some(next);
    }
    Ok(cur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::Crop;

    fn ramp(w: u32, h: u32) -> Image {
        let mut img = Image::new(w, h).unwrap();
        for y in 0..h {
            for x in 0..w {
                let v = (y * w + x + 1) as f32 / (w * h + 1) as f32;
                img.set(x, y, [v, v * 0.5, v * 0.25, 1.0]);
            }
        }
        img
    }

    #[test]
    fn identity_returns_none() {
        assert!(apply_edit(&ramp(4, 3), &ImageEdit::default())
            .unwrap()
            .is_none());
    }

    #[test]
    fn turns_and_flips_are_the_exact_raster_ops() {
        let img = ramp(6, 4);
        let edit = ImageEdit {
            quarter_turns: 1,
            flip_h: true,
            ..ImageEdit::default()
        };
        let out = apply_edit(&img, &edit).unwrap().unwrap();
        let want =
            transform::flip(&transform::rotate_quarter(&img, 1).unwrap(), true, false).unwrap();
        assert_eq!(out, want);
    }

    #[test]
    fn the_geometry_map_and_the_pixels_agree() {
        // The same edit through the general (resample) path equals the exact path.
        let img = ramp(7, 5);
        for turns in 0..4u8 {
            for (fh, fv) in [(false, false), (true, false), (false, true), (true, true)] {
                let exact = ImageEdit {
                    quarter_turns: turns,
                    flip_h: fh,
                    flip_v: fv,
                    ..ImageEdit::default()
                };
                let Some(want) = apply_edit(&img, &exact).unwrap() else {
                    continue;
                };
                let g = exact.geometry(7, 5).unwrap();
                let got = transform::resample(&img, g.width, g.height, g.edited_to_source).unwrap();
                assert_eq!(got, want, "turns {turns} flip {fh} {fv}");
            }
        }
    }

    #[test]
    fn crop_selects_pixels_and_outside_is_transparent() {
        let img = ramp(6, 4);
        let edit = ImageEdit {
            crop: Some(Crop {
                x: 2.0,
                y: 1.0,
                w: 3.0,
                h: 2.0,
            }),
            ..ImageEdit::default()
        };
        let out = apply_edit(&img, &edit).unwrap().unwrap();
        assert_eq!(out.size(), (3, 2));
        assert_eq!(out.get(0, 0), img.get(2, 1));
        let past = ImageEdit {
            crop: Some(Crop {
                x: 4.0,
                y: 0.0,
                w: 4.0,
                h: 4.0,
            }),
            ..ImageEdit::default()
        };
        let out = apply_edit(&img, &past).unwrap().unwrap();
        assert_eq!(out.get(0, 0), img.get(4, 0));
        assert_eq!(out.get(3, 0), Some([0.0; 4]));
    }

    #[test]
    fn adjust_then_filters_in_order() {
        let img = ramp(8, 8);
        let edit = ImageEdit {
            adjust: AdjustSettings {
                negative: true,
                ..AdjustSettings::default()
            },
            filters: vec![Filter::Blur { radius: 1 }],
            ..ImageEdit::default()
        };
        let out = apply_edit(&img, &edit).unwrap().unwrap();
        let want = filter::box_blur(
            &inbetween_raster::adjust(
                &img,
                &Adjust {
                    negative: true,
                    ..Adjust::default()
                },
            )
            .unwrap(),
            1,
        )
        .unwrap();
        assert_eq!(out, want);
    }

    #[test]
    fn hostile_edits_are_errors_not_panics() {
        let img = ramp(3, 3);
        let bad = ImageEdit {
            angle: f64::NAN,
            ..ImageEdit::default()
        };
        assert!(apply_edit(&img, &bad).is_err());
        let giant = ImageEdit {
            crop: Some(Crop {
                x: 0.0,
                y: 0.0,
                w: 32768.0,
                h: 32768.0,
            }),
            ..ImageEdit::default()
        };
        assert!(apply_edit(&img, &giant).is_err());
    }
}
