//! Each encoder's output is read back with an independent decoder.

use inbetween_export::{
    encode_apng, encode_gif, encode_webp, AnimationOptions, ExportError, Frames, Loop,
};
use std::io::Cursor;

const W: u32 = 6;
const H: u32 = 4;

/// Frame `i`: a solid colour that changes per frame, transparent in the left column.
fn frames(n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|i| {
            let c = (40 * i + 20) as u8;
            (0..W * H)
                .flat_map(|p| {
                    if p % W == 0 {
                        [0, 0, 0, 0]
                    } else {
                        [c, 200, 100, 255]
                    }
                })
                .collect()
        })
        .collect()
}

fn opts() -> AnimationOptions {
    AnimationOptions {
        fps: 10.0,
        ..AnimationOptions::default()
    }
}

#[test]
fn gif_round_trips_frames_delay_and_transparency() {
    let data = frames(3);
    let bytes = encode_gif(Frames::new(W, H, &data).unwrap(), &opts()).unwrap();
    assert_eq!(&bytes[..6], b"GIF89a");
    let mut dec = gif::DecodeOptions::new();
    dec.set_color_output(gif::ColorOutput::RGBA);
    let mut dec = dec.read_info(Cursor::new(bytes)).unwrap();
    let mut seen = 0;
    while let Some(frame) = dec.read_next_frame().unwrap() {
        assert_eq!(frame.delay, 10); // 10 fps = 10 hundredths
        assert_eq!(frame.buffer[3], 0, "left column is transparent");
        assert_eq!(frame.buffer[7], 255);
        seen += 1;
    }
    assert_eq!(seen, 3);
}

#[test]
fn gif_can_be_opaque_over_a_matte() {
    let data = frames(2);
    let o = AnimationOptions {
        transparent: false,
        matte: [255, 0, 0],
        looping: Loop::Once,
        ..opts()
    };
    let bytes = encode_gif(Frames::new(W, H, &data).unwrap(), &o).unwrap();
    // No NETSCAPE looping extension when it plays once... or a finite count; either way no
    // infinite-loop marker with count 0.
    let mut dec = gif::DecodeOptions::new();
    dec.set_color_output(gif::ColorOutput::RGBA);
    let mut dec = dec.read_info(Cursor::new(bytes)).unwrap();
    let frame = dec.read_next_frame().unwrap().unwrap();
    assert_eq!(&frame.buffer[..4], &[255, 0, 0, 255], "flattened over red");
}

#[test]
fn apng_round_trips_with_alpha_and_timing() {
    let data = frames(4);
    let bytes = encode_apng(Frames::new(W, H, &data).unwrap(), &opts()).unwrap();
    let dec = png::Decoder::new(Cursor::new(bytes));
    let mut reader = dec.read_info().unwrap();
    let ctl = reader.info().animation_control().copied().unwrap();
    assert_eq!((ctl.num_frames, ctl.num_plays), (4, 0));
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    for (i, frame) in data.iter().enumerate() {
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!(
            &buf[..info.buffer_size()],
            &frame[..],
            "frame {i} is lossless"
        );
        let fc = reader.info().frame_control().copied().unwrap();
        assert_eq!((fc.delay_num, fc.delay_den), (1000, 10_000));
    }
}

#[test]
fn apng_flattens_when_not_transparent() {
    let data = frames(1);
    let o = AnimationOptions {
        transparent: false,
        matte: [9, 8, 7],
        looping: Loop::Once,
        ..opts()
    };
    let bytes = encode_apng(Frames::new(W, H, &data).unwrap(), &o).unwrap();
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut buf).unwrap();
    assert_eq!(&buf[..4], &[9, 8, 7, 255]);
}

#[test]
fn webp_is_a_valid_animation_with_every_frame_lossless() {
    let data = frames(3);
    let bytes = encode_webp(Frames::new(W, H, &data).unwrap(), &opts()).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
    let riff = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    assert_eq!(riff + 8, bytes.len(), "RIFF size covers the file");
    let mut dec = image_webp::WebPDecoder::new(Cursor::new(bytes)).unwrap();
    assert!(dec.is_animated());
    assert_eq!(dec.num_frames(), 3);
    assert_eq!(dec.dimensions(), (W, H));
    let mut buf = vec![0; dec.output_buffer_size().unwrap()];
    for (i, want) in data.iter().enumerate() {
        let dur = dec.read_frame(&mut buf).unwrap();
        assert_eq!(dur, 100, "10 fps is 100 ms");
        // Compare visible pixels exactly; fully transparent pixels may lose their colour.
        for (got, want) in buf.chunks(4).zip(want.chunks(4)) {
            if want[3] == 0 {
                assert_eq!(got[3], 0);
            } else {
                assert_eq!(got, want, "frame {i}");
            }
        }
    }
}

#[test]
fn hostile_inputs_are_errors_not_panics() {
    let data = frames(1);
    let good = Frames::new(W, H, &data).unwrap();
    for fps in [0.0, f64::NAN, 1e9, -1.0] {
        let o = AnimationOptions { fps, ..opts() };
        assert!(matches!(encode_gif(good, &o), Err(ExportError::BadFps(_))));
        assert!(matches!(encode_apng(good, &o), Err(ExportError::BadFps(_))));
        assert!(matches!(encode_webp(good, &o), Err(ExportError::BadFps(_))));
    }
    // Wider than a GIF or WebP can describe: refused before any pixel work.
    let wide = vec![vec![0u8; 70_000 * 4]];
    let f = Frames::new(70_000, 1, &wide).unwrap();
    assert!(matches!(
        encode_gif(f, &opts()),
        Err(ExportError::TooLarge { .. })
    ));
    assert!(matches!(
        encode_webp(f, &opts()),
        Err(ExportError::TooLarge { .. })
    ));
    // One-pixel frames still work.
    let one = vec![vec![1, 2, 3, 255]];
    let f = Frames::new(1, 1, &one).unwrap();
    assert!(encode_gif(f, &opts()).is_ok());
    assert!(encode_apng(f, &opts()).is_ok());
    assert!(encode_webp(f, &opts()).is_ok());
}
