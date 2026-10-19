//! Row-parallel image building shared by the pixel operations.

use crate::{Image, Pixel, RasterError};
use rayon::prelude::*;

/// Build a `width x height` image where pixel `(x, y)` is `f(x, y)`, one row per rayon task.
pub(crate) fn build(
    width: u32,
    height: u32,
    f: impl Fn(u32, u32) -> Pixel + Sync,
) -> Result<Image, RasterError> {
    let mut out = Image::new(width, height)?;
    let row_len = (width as usize) * 4;
    out.as_mut_slice()
        .par_chunks_mut(row_len)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                px.copy_from_slice(&f(x as u32, y as u32));
            }
        });
    Ok(out)
}

/// Apply `f` to every pixel of `img` (same size out).
pub(crate) fn map(img: &Image, f: impl Fn(Pixel) -> Pixel + Sync) -> Result<Image, RasterError> {
    let mut out = img.clone();
    out.as_mut_slice().par_chunks_mut(16_384).for_each(|block| {
        for px in block.chunks_exact_mut(4) {
            if let [r, g, b, a] = *px {
                px.copy_from_slice(&f([r, g, b, a]));
            }
        }
    });
    Ok(out)
}
