//! Turning engine images into egui textures, and showing a source fitted to the frame.

use egui::{ColorImage, Pos2, Rect, Vec2};
use inbetween_geom::Affine;
use inbetween_raster::Image;

/// Convert a premultiplied f32 image to an egui texture image. egui also works with sRGB-encoded
/// premultiplied colour, which is what the working space holds, so no colour maths is needed.
/// Images wider or taller than `max_side` are sampled down by a whole factor to fit the GPU.
pub fn to_color_image(image: &Image, max_side: usize) -> ColorImage {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let step = w.max(h).div_ceil(max_side.max(1)).max(1);
    let (ow, oh) = (w.div_ceil(step), h.div_ceil(step));
    let data = image.as_slice();
    let q = |v: f32| (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    let mut rgba = Vec::with_capacity(ow * oh * 4);
    for y in 0..oh {
        for x in 0..ow {
            let i = ((y * step) * w + x * step) * 4;
            match data.get(i..i + 4) {
                Some(p) => rgba.extend([q(p[0]), q(p[1]), q(p[2]), q(p[3])]),
                None => rgba.extend([0, 0, 0, 0]),
            }
        }
    }
    ColorImage::from_rgba_premultiplied([ow, oh], &rgba)
}

/// Where a source image lands in the output frame, and which part of the texture shows there.
///
/// `fit` maps frame coordinates into source-image coordinates (see `morph::fit_transform`).
/// Returns the covered rectangle in frame coordinates and the matching texture coordinates
/// (0 to 1). For Stretch this is the whole frame; Contain leaves bars outside the rectangle;
/// Cover shows only the part of the image that fits.
pub fn content_mapping(fit: &Affine, source: Vec2, frame: Vec2) -> (Rect, Rect) {
    // The fit is a scale plus an offset on each axis, so it inverts per axis.
    let to_frame = |s: f32, scale: f64, offset: f64| ((f64::from(s) - offset) / scale) as f32;
    let (x0, x1) = (
        to_frame(0.0, fit.a, fit.tx),
        to_frame(source.x, fit.a, fit.tx),
    );
    let (y0, y1) = (
        to_frame(0.0, fit.d, fit.ty),
        to_frame(source.y, fit.d, fit.ty),
    );
    let covered = Rect::from_min_max(
        Pos2::new(x0.min(x1), y0.min(y1)),
        Pos2::new(x0.max(x1), y0.max(y1)),
    )
    .intersect(Rect::from_min_size(Pos2::ZERO, frame));
    let uv_of = |p: Pos2| {
        let s = fit.apply(inbetween_geom::Point::new(f64::from(p.x), f64::from(p.y)));
        Pos2::new(
            s.x as f32 / source.x.max(1.0),
            s.y as f32 / source.y.max(1.0),
        )
    };
    let (a, b) = (uv_of(covered.min), uv_of(covered.max));
    (covered, Rect::from_min_max(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::FitMode;
    use inbetween_morph::fit_transform;

    fn mapping(src: (u32, u32), out: (u32, u32), mode: FitMode) -> (Rect, Rect) {
        let fit = fit_transform(src.0, src.1, out.0, out.1, mode).unwrap();
        content_mapping(
            &fit,
            Vec2::new(src.0 as f32, src.1 as f32),
            Vec2::new(out.0 as f32, out.1 as f32),
        )
    }

    fn close(a: Rect, b: Rect) -> bool {
        (a.min - b.min).length() < 1e-3 && (a.max - b.max).length() < 1e-3
    }

    #[test]
    fn stretch_shows_the_whole_image_over_the_whole_frame() {
        let (frame, uv) = mapping((100, 50), (200, 200), FitMode::Stretch);
        assert!(close(
            frame,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(200.0, 200.0))
        ));
        assert!(close(
            uv,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
        ));
    }

    #[test]
    fn contain_leaves_bars_and_shows_the_whole_image() {
        let (frame, uv) = mapping((100, 50), (200, 200), FitMode::Contain);
        assert!(close(
            frame,
            Rect::from_min_max(Pos2::new(0.0, 50.0), Pos2::new(200.0, 150.0))
        ));
        assert!(close(
            uv,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
        ));
    }

    #[test]
    fn cover_fills_the_frame_with_part_of_the_image() {
        let (frame, uv) = mapping((100, 50), (200, 200), FitMode::Cover);
        assert!(close(
            frame,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(200.0, 200.0))
        ));
        assert!(close(
            uv,
            Rect::from_min_max(Pos2::new(0.25, 0.0), Pos2::new(0.75, 1.0))
        ));
    }

    #[test]
    fn colour_image_keeps_size_and_values() {
        let img = Image::from_rgba8(2, 1, &[255, 0, 0, 255, 0, 0, 255, 128]).unwrap();
        let c = to_color_image(&img, 4096);
        assert_eq!(c.size, [2, 1]);
        assert_eq!(c.pixels[0].to_array(), [255, 0, 0, 255]);
        // Premultiplied storage: blue at half alpha is (0, 0, 128, 128).
        assert_eq!(c.pixels[1].to_array(), [0, 0, 128, 128]);
    }

    #[test]
    fn big_images_are_sampled_down_to_the_texture_limit() {
        let img = Image::new(1000, 300).unwrap();
        let c = to_color_image(&img, 256);
        assert!(c.size[0] <= 256 && c.size[1] <= 256, "{:?}", c.size);
        assert_eq!(c.size, [250, 75]);
        let tiny = to_color_image(&Image::new(3, 3).unwrap(), 0);
        assert!(tiny.size[0] >= 1);
    }
}
