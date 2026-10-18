//! Synthetic images for tests and demos: gradients, checkerboards and labelled grids.
//! Everything here is generated, never photographic.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod demo;
pub mod perf;

use inbetween_raster::{Image, RasterError};

/// Opaque gradient: red grows left to right, green grows top to bottom, blue is constant.
pub fn gradient(width: u32, height: u32) -> Result<Image, RasterError> {
    check(width, height)?;
    let mut buf = Vec::new();
    for y in 0..height {
        for x in 0..width {
            buf.extend_from_slice(&[ramp(x, width), ramp(y, height), 128, 255]);
        }
    }
    finish(width, height, buf)
}

/// Orange that fades from fully transparent on the left to fully opaque on the right.
pub fn alpha_gradient(width: u32, height: u32) -> Result<Image, RasterError> {
    check(width, height)?;
    let mut buf = Vec::new();
    for _ in 0..height {
        for x in 0..width {
            buf.extend_from_slice(&[255, 140, 0, ramp(x, width)]);
        }
    }
    finish(width, height, buf)
}

/// Two-colour checkerboard with square cells of `cell` pixels (at least 1).
pub fn checkerboard(
    width: u32,
    height: u32,
    cell: u32,
    a: [u8; 4],
    b: [u8; 4],
) -> Result<Image, RasterError> {
    check(width, height)?;
    let cell = cell.max(1);
    let mut buf = Vec::new();
    for y in 0..height {
        for x in 0..width {
            buf.extend_from_slice(if (x / cell + y / cell) % 2 == 0 {
                &a
            } else {
                &b
            });
        }
    }
    finish(width, height, buf)
}

/// Grid of `cell`-pixel squares, each tinted by position, with dark grid lines and the
/// cell number (row-major) drawn in it. Handy for seeing exactly how a warp moved pixels.
pub fn labelled_grid(width: u32, height: u32, cell: u32) -> Result<Image, RasterError> {
    check(width, height)?;
    let cell = cell.max(2);
    let (w, h) = (width, height);
    let cols = w.div_ceil(cell);
    let mut buf = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let (cx, cy) = (x / cell, y / cell);
            let on_line = x % cell == 0 || y % cell == 0;
            let px = if on_line {
                [30, 30, 40, 255]
            } else {
                [
                    (170 + cx.wrapping_mul(37) % 80) as u8,
                    (170 + cy.wrapping_mul(53) % 80) as u8,
                    (190 + (cx + cy).wrapping_mul(29) % 60) as u8,
                    255,
                ]
            };
            buf.extend_from_slice(&px);
        }
    }
    // Capped so a huge cell size cannot make the glyph loops enormous.
    let scale = (cell / 12).clamp(1, 8);
    for cy in 0..h.div_ceil(cell) {
        for cx in 0..cols {
            let text = (u64::from(cy) * u64::from(cols) + u64::from(cx)).to_string();
            let x0 = (cx * cell).saturating_add(2).saturating_add(scale);
            let y0 = (cy * cell).saturating_add(2).saturating_add(scale);
            draw_text(&mut buf, w, h, x0, y0, scale, &text);
        }
    }
    finish(width, height, buf)
}

/// Reject bad sizes before allocating anything.
fn check(width: u32, height: u32) -> Result<(), RasterError> {
    Image::new(width, height).map(drop)
}

fn ramp(i: u32, n: u32) -> u8 {
    if n <= 1 {
        0
    } else {
        ((u64::from(i) * 255 + u64::from(n - 1) / 2) / u64::from(n - 1)).min(255) as u8
    }
}

fn finish(width: u32, height: u32, buf: Vec<u8>) -> Result<Image, RasterError> {
    Image::from_rgba8(width, height, &buf)
}

/// 3x5 digit glyphs, one row per entry, high bit on the left.
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b001, 0b001],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

fn draw_text(buf: &mut [u8], w: u32, h: u32, x0: u32, y0: u32, scale: u32, text: &str) {
    let mut x = x0;
    for ch in text.chars() {
        let Some(glyph) = ch.to_digit(10).and_then(|d| DIGITS.get(d as usize)) else {
            continue;
        };
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..3u32 {
                if bits >> (2 - col) & 1 == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px = x.saturating_add(col * scale).saturating_add(dx);
                        let py = y0.saturating_add(row as u32 * scale).saturating_add(dy);
                        if px < w && py < h {
                            let i = (py as usize * w as usize + px as usize) * 4;
                            if let Some(s) = buf.get_mut(i..i + 4) {
                                s.copy_from_slice(&[20, 20, 30, 255]);
                            }
                        }
                    }
                }
            }
        }
        x = x.saturating_add(scale.saturating_mul(4));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_hits_corners() {
        let img = gradient(16, 8).unwrap();
        let rgba = img.to_rgba8();
        assert_eq!(&rgba[..4], &[0, 0, 128, 255]);
        let last = rgba.len() - 4;
        assert_eq!(&rgba[last..], &[255, 255, 128, 255]);
    }

    #[test]
    fn alpha_gradient_spans_transparent_to_opaque() {
        let img = alpha_gradient(10, 2).unwrap();
        assert_eq!(img.get(0, 0).map(|p| p[3]), Some(0.0));
        assert_eq!(img.get(9, 1).map(|p| p[3]), Some(1.0));
    }

    #[test]
    fn checkerboard_alternates() {
        let img = checkerboard(8, 8, 2, [255, 255, 255, 255], [0, 0, 0, 255]).unwrap();
        assert_eq!(img.get(0, 0).map(|p| p[0]), Some(1.0));
        assert_eq!(img.get(2, 0).map(|p| p[0]), Some(0.0));
        assert_eq!(img.get(2, 2).map(|p| p[0]), Some(1.0));
    }

    #[test]
    fn labelled_grid_has_lines_and_glyph_ink() {
        let img = labelled_grid(64, 64, 32).unwrap();
        assert_eq!(img.size(), (64, 64));
        let line = img.get(32, 10).unwrap();
        assert!(line[0] < 0.2);
        let dark = img
            .to_rgba8()
            .chunks_exact(4)
            .filter(|p| p[0] == 20)
            .count();
        assert!(dark > 0, "no label pixels drawn");
    }

    #[test]
    fn generators_are_deterministic_and_validate_size() {
        assert_eq!(
            labelled_grid(40, 30, 8).unwrap(),
            labelled_grid(40, 30, 8).unwrap()
        );
        assert!(gradient(0, 4).is_err());
        assert!(checkerboard(4, 0, 2, [0; 4], [0; 4]).is_err());
        assert!(labelled_grid(0, 0, 0).is_err());
        assert!(alpha_gradient(u32::MAX, u32::MAX).is_err());
    }

    #[test]
    fn degenerate_cell_sizes_do_not_panic() {
        assert!(checkerboard(5, 5, 0, [1; 4], [2; 4]).is_ok());
        assert!(labelled_grid(5, 5, 0).is_ok());
        assert!(labelled_grid(5, 5, u32::MAX).is_ok());
    }
}
