//! Animated WebP: each frame is a lossless still from `image-webp`, and its bitstream chunk
//! is re-wrapped in the animation container (VP8X, ANIM, one ANMF per frame).

use crate::{check_fps, flatten, AnimationOptions, ExportError, Frames, Loop};
use image_webp::{ColorType, WebPEncoder};

/// Most pixels on a side the WebP container can describe.
const MAX_SIDE: u32 = 16_383;

fn put_u24(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes()[..3]);
}

fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], payload: &[u8]) -> Result<(), ExportError> {
    let len = u32::try_from(payload.len())
        .map_err(|_| ExportError::Encode("a WebP chunk exceeds 4 GiB".into()))?;
    out.extend_from_slice(tag);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
    Ok(())
}

/// The payload of the first chunk named `tag` in a still WebP file.
fn find_chunk<'a>(file: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let mut rest = file.get(12..)?; // after RIFF, size, WEBP
    while rest.len() >= 8 {
        let len = u32::from_le_bytes(rest.get(4..8)?.try_into().ok()?) as usize;
        let payload = rest.get(8..8usize.checked_add(len)?)?;
        if rest.get(..4)? == tag {
            return Some(payload);
        }
        rest = rest.get(8usize.checked_add(len)?.checked_add(len % 2)?..)?;
    }
    None
}

pub fn encode_webp(frames: Frames, opts: &AnimationOptions) -> Result<Vec<u8>, ExportError> {
    let fps = check_fps(opts.fps)?;
    if frames.width > MAX_SIDE || frames.height > MAX_SIDE {
        return Err(ExportError::TooLarge {
            format: "WebP",
            max: MAX_SIDE,
        });
    }
    let duration = ((1000.0 / fps).round() as u32).min(0x00FF_FFFF);
    let mut body = Vec::new();
    body.extend_from_slice(b"WEBP");

    let mut vp8x = vec![0b0001_0010, 0, 0, 0]; // flags: animation + alpha, then reserved
    put_u24(&mut vp8x, frames.width - 1);
    put_u24(&mut vp8x, frames.height - 1);
    chunk(&mut body, b"VP8X", &vp8x)?;

    let mut anim = vec![0, 0, 0, 0]; // background colour: transparent
    let loops: u16 = match opts.looping {
        Loop::Forever => 0,
        Loop::Once => 1,
    };
    anim.extend_from_slice(&loops.to_le_bytes());
    chunk(&mut body, b"ANIM", &anim)?;

    let err = |e: image_webp::EncodingError| ExportError::Encode(e.to_string());
    for data in frames.data {
        let flat;
        let pixels: &[u8] = if opts.transparent {
            data
        } else {
            let mut v = Vec::with_capacity(data.len());
            for px in data.chunks_exact(4) {
                let [r, g, b] = flatten(px, opts.matte);
                v.extend_from_slice(&[r, g, b, 255]);
            }
            flat = v;
            &flat
        };
        let mut still = Vec::new();
        WebPEncoder::new(&mut still)
            .encode(pixels, frames.width, frames.height, ColorType::Rgba8)
            .map_err(err)?;
        let bitstream = find_chunk(&still, b"VP8L")
            .ok_or_else(|| ExportError::Encode("the WebP encoder produced no VP8L data".into()))?;

        let mut anmf = Vec::new();
        put_u24(&mut anmf, 0); // x / 2
        put_u24(&mut anmf, 0); // y / 2
        put_u24(&mut anmf, frames.width - 1);
        put_u24(&mut anmf, frames.height - 1);
        put_u24(&mut anmf, duration);
        // Replace the canvas rather than blend, and leave the frame in place afterwards.
        anmf.push(0b0000_0010);
        chunk(&mut anmf, b"VP8L", bitstream)?;
        chunk(&mut body, b"ANMF", &anmf)?;
    }

    let size = u32::try_from(body.len())
        .map_err(|_| ExportError::Encode("the animation exceeds the 4 GiB WebP limit".into()))?;
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}
