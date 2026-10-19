use crate::{check_fps, flatten, AnimationOptions, ExportError, Frames, Loop};
use gif::{DisposalMethod, Encoder, Frame, Repeat};

/// Alpha at or above this counts as opaque in a transparent GIF (GIF has 1-bit transparency).
const OPAQUE_FROM: u8 = 128;

/// Encode an animated GIF. Delays are whole hundredths of a second, so 60 fps plays as 50.
pub fn encode_gif(frames: Frames, opts: &AnimationOptions) -> Result<Vec<u8>, ExportError> {
    let fps = check_fps(opts.fps)?;
    let (w, h) = match (u16::try_from(frames.width), u16::try_from(frames.height)) {
        (Ok(w), Ok(h)) => (w, h),
        _ => {
            return Err(ExportError::TooLarge {
                format: "GIF",
                max: u32::from(u16::MAX),
            })
        }
    };
    let delay = ((100.0 / fps).round() as u16).max(1);
    let err = |e: gif::EncodingError| ExportError::Encode(e.to_string());
    let mut out = Vec::new();
    {
        let mut enc = Encoder::new(&mut out, w, h, &[]).map_err(err)?;
        enc.set_repeat(match opts.looping {
            Loop::Forever => Repeat::Infinite,
            Loop::Once => Repeat::Finite(0),
        })
        .map_err(err)?;
        for data in frames.data {
            let mut rgba = Vec::with_capacity(data.len());
            for px in data.chunks_exact(4) {
                if opts.transparent {
                    let on = px[3] >= OPAQUE_FROM;
                    rgba.extend_from_slice(&[px[0], px[1], px[2], if on { 255 } else { 0 }]);
                } else {
                    let [r, g, b] = flatten(px, opts.matte);
                    rgba.extend_from_slice(&[r, g, b, 255]);
                }
            }
            let mut frame = Frame::from_rgba_speed(w, h, &mut rgba, 10);
            frame.delay = delay;
            if opts.transparent {
                // Each frame replaces the last, so cleared pixels must not show the old frame.
                frame.dispose = DisposalMethod::Background;
            }
            enc.write_frame(&frame).map_err(err)?;
        }
    }
    Ok(out)
}
