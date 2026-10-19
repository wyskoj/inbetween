use crate::{check_fps, flatten, AnimationOptions, ExportError, Frames, Loop};

/// Encode an animated PNG with full 8-bit alpha.
pub fn encode_apng(frames: Frames, opts: &AnimationOptions) -> Result<Vec<u8>, ExportError> {
    let fps = check_fps(opts.fps)?;
    let count = u32::try_from(frames.data.len()).map_err(|_| ExportError::NoFrames)?;
    let err = |e: png::EncodingError| ExportError::Encode(e.to_string());
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, frames.width, frames.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_animated(
            count,
            match opts.looping {
                Loop::Forever => 0,
                Loop::Once => 1,
            },
        )
        .map_err(err)?;
        // delay = 1000 / (fps * 1000) seconds.
        enc.set_frame_delay(1000, (fps * 1000.0).round() as u16)
            .map_err(err)?;
        let mut writer = enc.write_header().map_err(err)?;
        for data in frames.data {
            if opts.transparent {
                writer.write_image_data(data).map_err(err)?;
            } else {
                let mut flat = Vec::with_capacity(data.len());
                for px in data.chunks_exact(4) {
                    let [r, g, b] = flatten(px, opts.matte);
                    flat.extend_from_slice(&[r, g, b, 255]);
                }
                writer.write_image_data(&flat).map_err(err)?;
            }
        }
        writer.finish().map_err(err)?;
    }
    Ok(out)
}
