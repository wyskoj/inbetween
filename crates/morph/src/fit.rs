use crate::MorphError;
use inbetween_doc::FitMode;
use inbetween_geom::Affine;

/// The map from output-frame coordinates to coordinates in a `src_w` x `src_h` source image
/// that has been fitted to a `out_w` x `out_h` frame.
pub fn fit_transform(
    src_w: u32,
    src_h: u32,
    out_w: u32,
    out_h: u32,
    mode: FitMode,
) -> Result<Affine, MorphError> {
    if src_w == 0 || src_h == 0 || out_w == 0 || out_h == 0 {
        return Err(MorphError::BadSize);
    }
    let (sw, sh, ow, oh) = (
        f64::from(src_w),
        f64::from(src_h),
        f64::from(out_w),
        f64::from(out_h),
    );
    let (sx, sy) = match mode {
        FitMode::Stretch => (ow / sw, oh / sh),
        FitMode::Contain => {
            let s = (ow / sw).min(oh / sh);
            (s, s)
        }
        FitMode::Cover => {
            let s = (ow / sw).max(oh / sh);
            (s, s)
        }
    };
    // Content is centred: output = source * s + offset.
    let off_x = (ow - sw * sx) / 2.0;
    let off_y = (oh - sh * sy) / 2.0;
    Ok(Affine {
        a: 1.0 / sx,
        b: 0.0,
        c: 0.0,
        d: 1.0 / sy,
        tx: -off_x / sx,
        ty: -off_y / sy,
    })
}

/// Whether positions outside the source image read as transparent (letterbox bars) rather
/// than repeating the edge pixels.
pub fn transparent_outside(mode: FitMode) -> bool {
    matches!(mode, FitMode::Contain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_geom::Point;

    fn map(src: (u32, u32), out: (u32, u32), mode: FitMode, p: (f64, f64)) -> Point {
        fit_transform(src.0, src.1, out.0, out.1, mode)
            .unwrap()
            .apply(Point::new(p.0, p.1))
    }

    #[test]
    fn same_size_is_exactly_identity() {
        for mode in [FitMode::Stretch, FitMode::Contain, FitMode::Cover] {
            let m = fit_transform(640, 480, 640, 480, mode).unwrap();
            assert_eq!(m, Affine::IDENTITY, "{mode:?}");
        }
    }

    #[test]
    fn stretch_scales_each_axis() {
        // 100x50 source into 200x200: corners map to corners.
        let p = map((100, 50), (200, 200), FitMode::Stretch, (200.0, 200.0));
        assert_eq!(p, Point::new(100.0, 50.0));
        assert_eq!(
            map((100, 50), (200, 200), FitMode::Stretch, (0.0, 0.0)),
            Point::new(0.0, 0.0)
        );
    }

    #[test]
    fn contain_letterboxes_and_centres() {
        // 100x50 into 200x200: scale 2, content 200x100 at y offset 50.
        let top = map((100, 50), (200, 200), FitMode::Contain, (0.0, 50.0));
        let bottom = map((100, 50), (200, 200), FitMode::Contain, (200.0, 150.0));
        assert_eq!(top, Point::new(0.0, 0.0));
        assert_eq!(bottom, Point::new(100.0, 50.0));
        // The bar above the content maps to negative source y.
        assert!(map((100, 50), (200, 200), FitMode::Contain, (100.0, 10.0)).y < 0.0);
    }

    #[test]
    fn cover_crops_and_centres() {
        // 100x50 into 200x200: scale 4, content 400x200, 100px cropped each side.
        let left = map((100, 50), (200, 200), FitMode::Cover, (0.0, 0.0));
        assert_eq!(left, Point::new(25.0, 0.0));
        let right = map((100, 50), (200, 200), FitMode::Cover, (200.0, 200.0));
        assert_eq!(right, Point::new(75.0, 50.0));
    }

    #[test]
    fn zero_sizes_are_errors() {
        assert_eq!(
            fit_transform(0, 5, 5, 5, FitMode::Stretch),
            Err(MorphError::BadSize)
        );
        assert_eq!(
            fit_transform(5, 5, 5, 0, FitMode::Cover),
            Err(MorphError::BadSize)
        );
    }

    #[test]
    fn only_contain_has_transparent_bars() {
        assert!(transparent_outside(FitMode::Contain));
        assert!(!transparent_outside(FitMode::Stretch));
        assert!(!transparent_outside(FitMode::Cover));
    }
}
