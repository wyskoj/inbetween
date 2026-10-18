//! Round trips across raster and codecs using the synthetic generators.

use inbetween_codecs::{decode, encode_jpeg, encode_png};
use inbetween_raster::Image;
use inbetween_testkit::{alpha_gradient, checkerboard, gradient, labelled_grid};

fn scenes() -> Vec<(&'static str, Image)> {
    vec![
        ("gradient", gradient(64, 48).unwrap()),
        ("alpha gradient", alpha_gradient(64, 48).unwrap()),
        (
            "checkerboard",
            checkerboard(64, 48, 8, [250, 20, 20, 255], [20, 20, 250, 128]).unwrap(),
        ),
        ("labelled grid", labelled_grid(96, 64, 32).unwrap()),
        ("one pixel", gradient(1, 1).unwrap()),
        ("one row", gradient(33, 1).unwrap()),
    ]
}

#[test]
fn png_round_trip_through_f32_is_lossless() {
    for (name, img) in scenes() {
        let rgba = img.to_rgba8();
        let png = encode_png(img.width(), img.height(), &rgba).unwrap();
        let dec = decode(&png).unwrap();
        assert_eq!((dec.width, dec.height), img.size(), "{name}");
        assert_eq!(dec.data, rgba, "{name}: pixels changed");
        let back = Image::from_rgba8(dec.width, dec.height, &dec.data).unwrap();
        assert_eq!(back, img, "{name}: f32 image changed");
    }
}

#[test]
fn png_preserves_alpha_channel() {
    let img = alpha_gradient(64, 4).unwrap();
    let rgba = img.to_rgba8();
    let png = encode_png(64, 4, &rgba).unwrap();
    let dec = decode(&png).unwrap();
    let alphas: Vec<u8> = dec.data.chunks_exact(4).take(64).map(|p| p[3]).collect();
    assert_eq!(alphas[0], 0);
    assert_eq!(alphas[63], 255);
    assert!(alphas.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn jpeg_round_trip_stays_close_on_opaque_images() {
    {
        let (name, img) = ("gradient", gradient(64, 48).unwrap());
        let rgba = img.to_rgba8();
        let jpg = encode_jpeg(img.width(), img.height(), &rgba, 95).unwrap();
        let dec = decode(&jpg).unwrap();
        assert_eq!((dec.width, dec.height), img.size(), "{name}");
        let total: u64 = rgba
            .iter()
            .zip(&dec.data)
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        let mean = total as f64 / rgba.len() as f64;
        assert!(mean < 3.0, "{name}: mean error {mean}");
    }
}

#[test]
fn bilinear_sampling_at_centres_reproduces_decoded_image() {
    let img = labelled_grid(40, 24, 8).unwrap();
    let png = encode_png(40, 24, &img.to_rgba8()).unwrap();
    let dec = decode(&png).unwrap();
    let back = Image::from_rgba8(dec.width, dec.height, &dec.data).unwrap();
    for y in 0..24 {
        for x in 0..40 {
            assert_eq!(
                back.sample_bilinear(x as f32 + 0.5, y as f32 + 0.5),
                back.get(x, y).unwrap()
            );
        }
    }
}
