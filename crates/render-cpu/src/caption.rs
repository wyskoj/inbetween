//! Drawing captions.
//!
//! A caption is drawn into a transparent layer the size of the frame: shadow, then border,
//! then the text, composited with "over" in premultiplied space. The layer is static, so
//! callers draw it once and lay it over every frame it belongs to.
//!
//! Text is laid out with `ab_glyph`. Bold and italic are synthesised (the glyph is drawn a few
//! times a fraction of an em apart; italic shears each glyph about its baseline), so any
//! `.ttf` or `.otf` file works whatever styles it carries. Everything is deterministic: the
//! same caption, font and frame size give the same bytes.

use crate::RenderError;
use ab_glyph::{point, Font as _, FontArc, Glyph, PxScale, ScaleFont as _};
use inbetween_doc::{Align, Caption};
use inbetween_raster::{filter, over, Image, Pixel};
use rayon::prelude::*;

/// The bundled font (Roboto Regular, Apache-2.0; see `ATTRIBUTION.md`).
pub const DEFAULT_FONT: &[u8] = include_bytes!("../assets/Roboto-Regular.ttf");

/// Largest font file accepted, in bytes.
pub const MAX_FONT_BYTES: usize = 32 * 1024 * 1024;

/// Italic slant: how far a point one pixel above the baseline moves right.
const SHEAR: f32 = 0.2;
/// Bold smear, as a fraction of the font size (a 1/24 em stroke widening split over x and y).
const BOLD: f32 = 1.0 / 24.0;
/// Most pixels of glyph area drawn per caption, so absurd sizes stay fast.
const DRAW_BUDGET: u64 = 400_000_000;
/// Largest border or shadow reach drawn, in pixels.
const MAX_REACH: f64 = 256.0;

/// A parsed font.
#[derive(Clone)]
pub struct Font(FontArc);

impl std::fmt::Debug for Font {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Font")
    }
}

impl Font {
    /// The bundled font.
    pub fn bundled() -> Result<Self, RenderError> {
        FontArc::try_from_slice(DEFAULT_FONT)
            .map(Self)
            .map_err(|e| RenderError::Font(e.to_string()))
    }

    /// Parse a `.ttf` or `.otf` file's bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, RenderError> {
        if bytes.len() > MAX_FONT_BYTES {
            return Err(RenderError::Font(format!(
                "the font file is {} MiB, more than the {} MiB limit",
                bytes.len() >> 20,
                MAX_FONT_BYTES >> 20
            )));
        }
        FontArc::try_from_vec(bytes).map(Self).map_err(|_| {
            RenderError::Font("the file is not a TrueType or OpenType font".to_string())
        })
    }
}

struct Line {
    text: String,
    x: f32,
    baseline: f32,
}

struct Layout {
    lines: Vec<Line>,
    scale: PxScale,
    /// Left, top, right, bottom of the text block, in frame pixels.
    bounds: [f32; 4],
}

fn advance(font: &FontArc, scale: PxScale, text: &str, bold: f32) -> f32 {
    let sf = font.as_scaled(scale);
    let mut width = 0.0;
    let mut previous = None;
    for c in text.chars() {
        let id = sf.glyph_id(c);
        if let Some(p) = previous {
            width += sf.kern(p, id);
        }
        width += sf.h_advance(id) + bold;
        previous = Some(id);
    }
    width
}

fn layout(caption: &Caption, font: &FontArc, frame: (u32, u32)) -> Layout {
    let size = caption.size as f32;
    let scale = PxScale::from(size);
    let sf = font.as_scaled(scale);
    let bold = if caption.bold { size * BOLD } else { 0.0 };
    let line_height = (sf.ascent() - sf.descent() + sf.line_gap()).max(1.0);
    let texts: Vec<&str> = caption.text.split('\n').collect();
    let block_h = line_height * texts.len() as f32;
    let ax = caption.x as f32 * frame.0 as f32;
    let top = caption.y as f32 * frame.1 as f32 - block_h / 2.0;
    let mut lines = Vec::with_capacity(texts.len());
    let (mut left, mut right) = (f32::MAX, f32::MIN);
    for (i, raw) in texts.iter().enumerate() {
        let text = raw.trim_end_matches('\r').to_string();
        let width = advance(font, scale, &text, bold);
        let x = match caption.align {
            Align::Left => ax,
            Align::Center => ax - width / 2.0,
            Align::Right => ax - width,
        };
        left = left.min(x);
        right = right.max(x + width);
        lines.push(Line {
            text,
            x,
            baseline: top + line_height * i as f32 + sf.ascent(),
        });
    }
    if lines.is_empty() {
        left = ax;
        right = ax;
    }
    Layout {
        lines,
        scale,
        bounds: [left, top, right, top + block_h],
    }
}

/// The rectangle `[left, top, right, bottom]` the caption's text occupies in a frame of the
/// given size, before border and shadow. `None` when there is no text. Used to hit-test drags.
pub fn caption_bounds(caption: &Caption, font: &Font, frame: (u32, u32)) -> Option<[f64; 4]> {
    if caption.text.trim().is_empty() || caption.validate().is_err() {
        return None;
    }
    let l = layout(caption, &font.0, frame);
    let b = l.bounds.map(f64::from);
    b.iter().all(|v| v.is_finite()).then_some(b)
}

/// Coverage mask of the text, with `margin` extra pixels on every side.
struct Mask {
    w: usize,
    h: usize,
    margin: i64,
    data: Vec<f32>,
}

fn rasterise(caption: &Caption, font: &FontArc, frame: (u32, u32), margin: i64) -> Mask {
    let (w, h) = (
        frame.0 as usize + 2 * margin as usize,
        frame.1 as usize + 2 * margin as usize,
    );
    let mut mask = Mask {
        w,
        h,
        margin,
        data: vec![0.0; w * h],
    };
    let l = layout(caption, font, frame);
    let size = caption.size as f32;
    let shear = if caption.italic { SHEAR } else { 0.0 };
    let b = size * BOLD;
    let offsets: &[(f32, f32)] = if caption.bold {
        &[(0.0, 0.0), (b, 0.0), (0.0, b)]
    } else {
        &[(0.0, 0.0)]
    };
    let bold_step = if caption.bold { b } else { 0.0 };
    let sf = font.as_scaled(l.scale);
    let (min_x, min_y) = (-(margin as f32), -(margin as f32));
    let (max_x, max_y) = (
        frame.0 as f32 + margin as f32,
        frame.1 as f32 + margin as f32,
    );
    let mut budget = DRAW_BUDGET;
    for line in &l.lines {
        let mut pen = line.x;
        let mut previous = None;
        for c in line.text.chars() {
            let id = sf.glyph_id(c);
            if let Some(p) = previous {
                pen += sf.kern(p, id);
            }
            previous = Some(id);
            let here = pen;
            pen += sf.h_advance(id) + bold_step;
            for &(ox, oy) in offsets {
                let glyph = Glyph {
                    id,
                    scale: l.scale,
                    position: point(here + ox, line.baseline + oy),
                };
                let Some(outlined) = font.outline_glyph(glyph) else {
                    continue;
                };
                let bounds = outlined.px_bounds();
                let slant = shear * size;
                if bounds.max.x + slant < min_x
                    || bounds.min.x > max_x + slant
                    || bounds.max.y < min_y
                    || bounds.min.y > max_y
                {
                    continue;
                }
                let area = (f64::from(bounds.width()) * f64::from(bounds.height())) as u64;
                if area > budget {
                    return mask;
                }
                budget -= area;
                let baseline = line.baseline + oy;
                outlined.draw(|gx, gy, cover| {
                    if cover <= 0.0 {
                        return;
                    }
                    let px = bounds.min.x + gx as f32;
                    let py = bounds.min.y + gy as f32;
                    let xf = px + shear * (baseline - (py + 0.5));
                    let x0 = xf.floor();
                    let frac = xf - x0;
                    let iy = py as i64 + margin;
                    if iy < 0 || iy >= h as i64 {
                        return;
                    }
                    for (dx, amount) in [(0i64, 1.0 - frac), (1, frac)] {
                        let ix = x0 as i64 + dx + margin;
                        if ix < 0 || ix >= w as i64 || amount <= 0.0 {
                            continue;
                        }
                        let i = iy as usize * w + ix as usize;
                        if let Some(v) = mask.data.get_mut(i) {
                            *v = (*v + cover * amount).min(1.0);
                        }
                    }
                });
            }
        }
    }
    mask
}

/// Sliding-window maximum of each row over `[x - hw, x + hw]`.
fn row_max(data: &[f32], w: usize, hw: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; data.len()];
    out.par_chunks_mut(w)
        .zip(data.par_chunks(w))
        .for_each(|(dst, row)| {
            let mut deque: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
            let mut next = 0usize;
            for x in 0..w {
                let hi = (x + hw).min(w - 1);
                while next <= hi {
                    let v = row.get(next).copied().unwrap_or(0.0);
                    while deque
                        .back()
                        .is_some_and(|&b| row.get(b).copied().unwrap_or(0.0) <= v)
                    {
                        deque.pop_back();
                    }
                    deque.push_back(next);
                    next += 1;
                }
                let lo = x.saturating_sub(hw);
                while deque.front().is_some_and(|&f| f < lo) {
                    deque.pop_front();
                }
                if let (Some(d), Some(&f)) = (dst.get_mut(x), deque.front()) {
                    *d = row.get(f).copied().unwrap_or(0.0);
                }
            }
        });
    out
}

/// Grow the covered area by a disc of `radius` pixels.
fn dilate(mask: &Mask, radius: f64) -> Vec<f32> {
    let r = radius.clamp(0.0, MAX_REACH);
    if r <= 0.0 {
        return mask.data.clone();
    }
    let reach = r.ceil() as i64;
    let (w, h) = (mask.w, mask.h);
    let mut out = mask.data.clone();
    for dy in 0..=reach {
        let span = (r * r - (dy * dy) as f64).max(0.0).sqrt() + 0.5;
        let hw = span.floor() as usize;
        if (dy as f64) > r + 0.5 {
            break;
        }
        let rows = row_max(&mask.data, w, hw);
        for sign in [-1i64, 1] {
            if dy == 0 && sign == 1 {
                continue;
            }
            let shift = dy * sign;
            out.par_chunks_mut(w).enumerate().for_each(|(y, dst)| {
                let sy = y as i64 + shift;
                if sy < 0 || sy >= h as i64 {
                    return;
                }
                let from = sy as usize * w;
                if let Some(src) = rows.get(from..from + w) {
                    for (d, s) in dst.iter_mut().zip(src) {
                        if *s > *d {
                            *d = *s;
                        }
                    }
                }
            });
        }
    }
    out
}

fn premultiplied(color: [u8; 4], cover: f32) -> Pixel {
    let a = f32::from(color[3]) / 255.0 * cover;
    [
        f32::from(color[0]) / 255.0 * a,
        f32::from(color[1]) / 255.0 * a,
        f32::from(color[2]) / 255.0 * a,
        a,
    ]
}

/// Draw `caption` into a transparent layer of the frame's size. `None` when the caption draws
/// nothing (off, empty or fully transparent text). `font` is the font to use (the caption's
/// own font file, loaded by the caller, or [`Font::bundled`]).
pub fn render_layer(
    caption: &Caption,
    font: &Font,
    frame: (u32, u32),
) -> Result<Option<Image>, RenderError> {
    caption.validate()?;
    if !caption.is_visible() {
        return Ok(None);
    }
    let shadow = &caption.shadow;
    let shadow_on = shadow.size > 0.0 && shadow.color[3] > 0;
    let reach = if shadow_on {
        shadow.distance.abs() + shadow.size * 2.0
    } else {
        0.0
    } + caption.border_size;
    let margin = reach.clamp(0.0, MAX_REACH).ceil() as i64;
    let mask = rasterise(caption, &font.0, frame, margin);
    let stroke = (caption.border_size > 0.0 && caption.border_color[3] > 0)
        .then(|| dilate(&mask, caption.border_size));
    let (w, h) = (mask.w, mask.h);

    // The shadow follows the outline of text plus border.
    let shadow_layer = if shadow_on {
        let silhouette = stroke.as_deref().unwrap_or(&mask.data);
        let rad = shadow.angle.to_radians();
        let dx = (shadow.distance * rad.cos()).round() as i64;
        let dy = (-shadow.distance * rad.sin()).round() as i64;
        let mut img = Image::new(w as u32, h as u32)?;
        img.as_mut_slice()
            .par_chunks_mut(w * 4)
            .enumerate()
            .for_each(|(y, row)| {
                let sy = y as i64 - dy;
                if sy < 0 || sy >= h as i64 {
                    return;
                }
                for (x, px) in row.chunks_exact_mut(4).enumerate() {
                    let sx = x as i64 - dx;
                    if sx < 0 || sx >= w as i64 {
                        continue;
                    }
                    let cover = silhouette
                        .get(sy as usize * w + sx as usize)
                        .copied()
                        .unwrap_or(0.0);
                    if cover > 0.0 {
                        px.copy_from_slice(&premultiplied(shadow.color, cover));
                    }
                }
            });
        Some(filter::gaussian(&img, shadow.size / 2.0)?)
    } else {
        None
    };

    let mut layer = Image::new(frame.0, frame.1)?;
    let m = margin as usize;
    let fw = frame.0 as usize;
    layer
        .as_mut_slice()
        .par_chunks_mut(fw * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let i = (y + m) * w + x + m;
                let mut out: Pixel = [0.0; 4];
                if let Some(s) = &shadow_layer {
                    if let Some(v) = s.as_slice().get(i * 4..i * 4 + 4) {
                        out = [v[0], v[1], v[2], v[3]];
                    }
                }
                if let Some(b) = &stroke {
                    let cover = b.get(i).copied().unwrap_or(0.0);
                    if cover > 0.0 {
                        out = over(premultiplied(caption.border_color, cover), out);
                    }
                }
                let cover = mask.data.get(i).copied().unwrap_or(0.0);
                if cover > 0.0 {
                    out = over(premultiplied(caption.color, cover), out);
                }
                px.copy_from_slice(&out);
            }
        });
    let _ = (h, mask.margin);
    Ok(Some(layer))
}

/// Lay `layer` over `image` (same size). A size mismatch is ignored rather than an error: the
/// layer then belongs to a different frame size and drawing it would misplace the text.
pub fn composite(image: &mut Image, layer: &Image) {
    if image.size() != layer.size() {
        return;
    }
    for (dst, src) in image
        .as_mut_slice()
        .chunks_exact_mut(4)
        .zip(layer.as_slice().chunks_exact(4))
    {
        if src[3] > 0.0 {
            let px = over(
                [src[0], src[1], src[2], src[3]],
                [dst[0], dst[1], dst[2], dst[3]],
            );
            dst.copy_from_slice(&px);
        }
    }
}

/// Draw `caption` straight onto `image`.
pub fn draw_caption(image: &mut Image, caption: &Caption, font: &Font) -> Result<(), RenderError> {
    if let Some(layer) = render_layer(caption, font, image.size())? {
        composite(image, &layer);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::Shadow;

    fn caption(text: &str) -> Caption {
        Caption {
            text: text.into(),
            size: 24.0,
            x: 0.5,
            y: 0.5,
            ..Caption::default()
        }
    }

    fn font() -> Font {
        Font::bundled().unwrap()
    }

    fn layer(c: &Caption, size: (u32, u32)) -> Image {
        render_layer(c, &font(), size).unwrap().unwrap()
    }

    fn coverage(img: &Image) -> f32 {
        img.as_slice().chunks_exact(4).map(|p| p[3]).sum()
    }

    fn checksum(img: &Image) -> u64 {
        // FNV-1a over the 8-bit pixels.
        img.to_rgba8()
            .iter()
            .fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
            })
    }

    #[test]
    fn the_bundled_font_parses_and_text_draws_inside_its_bounds() {
        let c = caption("Hello");
        let l = layer(&c, (200, 80));
        assert!(coverage(&l) > 20.0);
        let b = caption_bounds(&c, &font(), (200, 80)).unwrap();
        // Everything drawn lies inside the reported block (give or take antialiasing).
        for (i, p) in l.as_slice().chunks_exact(4).enumerate() {
            if p[3] > 0.0 {
                let (x, y) = ((i % 200) as f64, (i / 200) as f64);
                assert!(x >= b[0] - 2.0 && x <= b[2] + 2.0 && y >= b[1] - 2.0 && y <= b[3] + 2.0);
            }
        }
        assert!(b[2] - b[0] > 30.0 && b[3] - b[1] > 15.0);
    }

    #[test]
    fn drawing_is_deterministic_and_pinned_by_a_checksum() {
        let c = Caption {
            border_size: 2.0,
            shadow: Shadow {
                size: 4.0,
                ..Shadow::default()
            },
            ..caption("Pinned\ntext")
        };
        let a = layer(&c, (160, 100));
        assert_eq!(a, layer(&c, (160, 100)));
        // Update this value deliberately when the text renderer is meant to change.
        let pinned = Caption {
            border_size: 2.0,
            ..caption(
                "Pinned
text",
            )
        };
        assert_eq!(
            checksum(&layer(&pinned, (160, 100))),
            8_087_400_900_267_906_064
        );
        let plain = layer(&caption("Pinned\ntext"), (160, 100));
        assert_ne!(checksum(&a), checksum(&plain));
    }

    #[test]
    fn empty_off_and_clear_captions_draw_nothing() {
        let f = font();
        assert!(render_layer(&caption(""), &f, (50, 50)).unwrap().is_none());
        assert!(render_layer(&caption("   \n "), &f, (50, 50))
            .unwrap()
            .is_none());
        let off = Caption {
            enabled: false,
            ..caption("x")
        };
        assert!(render_layer(&off, &f, (50, 50)).unwrap().is_none());
        let clear = Caption {
            color: [255, 255, 255, 0],
            ..caption("x")
        };
        assert!(render_layer(&clear, &f, (50, 50)).unwrap().is_none());
        assert!(caption_bounds(&caption(""), &f, (50, 50)).is_none());
    }

    #[test]
    fn hostile_captions_do_not_panic() {
        let f = font();
        for (c, size) in [
            (caption("x"), (1, 1)),
            (caption("\u{1F600}\u{0}\t\r\n\n\n"), (3, 2)),
            (
                Caption {
                    size: 4000.0,
                    ..caption("Giant")
                },
                (64, 48),
            ),
            (
                Caption {
                    size: 4000.0,
                    border_size: 4000.0,
                    ..caption(&"W".repeat(500))
                },
                (64, 48),
            ),
            (
                Caption {
                    x: -10.0,
                    y: 10.0,
                    ..caption("far outside")
                },
                (64, 48),
            ),
            (
                Caption {
                    x: 5.0,
                    ..caption("outside right")
                },
                (64, 48),
            ),
            (
                Caption {
                    bold: true,
                    italic: true,
                    border_size: 3.0,
                    ..caption("styled")
                },
                (1, 1),
            ),
            (
                Caption {
                    shadow: Shadow {
                        size: 4000.0,
                        distance: 4000.0,
                        ..Shadow::default()
                    },
                    ..caption("shadow")
                },
                (40, 30),
            ),
        ] {
            let r = render_layer(&c, &f, size);
            if let Ok(Some(l)) = r {
                assert!(l
                    .as_slice()
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0.0 && *v <= 1.0001));
            }
        }
        let bad = Caption {
            size: f64::NAN,
            ..caption("x")
        };
        assert!(render_layer(&bad, &f, (10, 10)).is_err());
    }

    #[test]
    fn a_bad_font_file_is_an_actionable_error() {
        let err = Font::from_bytes(b"not a font at all".to_vec())
            .unwrap_err()
            .to_string();
        assert!(err.contains("TrueType or OpenType"), "{err}");
        assert!(Font::from_bytes(Vec::new()).is_err());
        assert!(Font::from_bytes(DEFAULT_FONT.to_vec()).is_ok());
    }

    #[test]
    fn alignment_moves_the_bounds() {
        let f = font();
        let at = |align| {
            let c = Caption {
                align,
                x: 0.5,
                ..caption("Aligned")
            };
            caption_bounds(&c, &f, (400, 100)).unwrap()
        };
        let (l, c, r) = (at(Align::Left), at(Align::Center), at(Align::Right));
        assert!((l[0] - 200.0).abs() < 1.0, "left edge at the anchor");
        assert!(
            ((c[0] + c[2]) / 2.0 - 200.0).abs() < 1.0,
            "centred on the anchor"
        );
        assert!((r[2] - 200.0).abs() < 1.0, "right edge at the anchor");
        assert!(l[0] > c[0] && c[0] > r[0]);
        // The anchor moves the block.
        let moved = Caption {
            x: 0.25,
            y: 0.25,
            ..caption("Aligned")
        };
        let m = caption_bounds(&moved, &f, (400, 100)).unwrap();
        let base = caption_bounds(&caption("Aligned"), &f, (400, 100)).unwrap();
        assert!(m[0] < base[0] && m[1] < base[1]);
    }

    #[test]
    fn multi_line_text_is_taller_and_lines_align_separately() {
        let f = font();
        let one = caption_bounds(&caption("ab"), &f, (300, 200)).unwrap();
        let two = caption_bounds(&caption("ab\nabcdef"), &f, (300, 200)).unwrap();
        assert!((two[3] - two[1]) > (one[3] - one[1]) * 1.8);
        assert!((two[2] - two[0]) > (one[2] - one[0]));
    }

    #[test]
    fn bold_border_and_italic_change_the_picture() {
        let base = layer(&caption("Style"), (200, 80));
        let bold = layer(
            &Caption {
                bold: true,
                ..caption("Style")
            },
            (200, 80),
        );
        let border = layer(
            &Caption {
                border_size: 3.0,
                ..caption("Style")
            },
            (200, 80),
        );
        let italic = layer(
            &Caption {
                italic: true,
                ..caption("Style")
            },
            (200, 80),
        );
        assert!(coverage(&bold) > coverage(&base) * 1.05);
        assert!(coverage(&border) > coverage(&base) * 1.3);
        assert_ne!(italic, base);
        // Italic keeps about the same ink.
        assert!((coverage(&italic) / coverage(&base) - 1.0).abs() < 0.1);
        // The border is its own colour.
        let c = Caption {
            border_size: 3.0,
            border_color: [255, 0, 0, 255],
            color: [0, 0, 255, 255],
            ..caption("Style")
        };
        let l = layer(&c, (200, 80));
        assert!(l
            .as_slice()
            .chunks_exact(4)
            .any(|p| p[0] > 0.9 && p[2] < 0.1));
        assert!(l
            .as_slice()
            .chunks_exact(4)
            .any(|p| p[2] > 0.9 && p[0] < 0.1));
    }

    #[test]
    fn a_shadow_of_size_zero_draws_nothing_and_a_real_one_falls_down_right() {
        let none = Caption {
            shadow: Shadow {
                size: 0.0,
                distance: 10.0,
                ..Shadow::default()
            },
            ..caption("Shade")
        };
        assert_eq!(layer(&none, (200, 80)), layer(&caption("Shade"), (200, 80)));
        let some = Caption {
            shadow: Shadow {
                size: 2.0,
                distance: 8.0,
                angle: -45.0,
                color: [0, 0, 0, 255],
            },
            ..caption("Shade")
        };
        let l = layer(&some, (200, 80));
        assert!(coverage(&l) > coverage(&layer(&caption("Shade"), (200, 80))));
        // The shadow reaches beyond the text's own extent toward the lower right.
        let bounds = caption_bounds(&some, &font(), (200, 80)).unwrap();
        let lowest = l
            .as_slice()
            .chunks_exact(4)
            .enumerate()
            .filter(|(_, p)| p[3] > 0.05)
            .map(|(i, _)| i / 200)
            .max()
            .unwrap();
        assert!(lowest as f64 > bounds[3] - 6.0);
    }

    #[test]
    fn composite_lays_the_layer_over_the_frame() {
        let mut img = Image::filled(200, 80, [0.0, 0.0, 0.0, 1.0]).unwrap();
        draw_caption(&mut img, &caption("Over"), &font()).unwrap();
        assert!(img.as_slice().chunks_exact(4).any(|p| p[0] > 0.9));
        assert!(img.as_slice().chunks_exact(4).all(|p| p[3] == 1.0));
        // A mismatched layer is ignored.
        let other = Image::filled(5, 5, [1.0; 4]).unwrap();
        let before = img.clone();
        composite(&mut img, &other);
        assert_eq!(img, before);
    }
}
