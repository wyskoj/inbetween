//! Image file read/write. Standalone: no workspace dependencies.
//!
//! Reads PNG, JPEG, BMP, TIFF, TGA, GIF (first frame) and PSD (flattened composite);
//! writes PNG and JPEG.
//!
//! Pixels cross this boundary as straight (non-premultiplied) 8-bit sRGB RGBA, four bytes
//! per pixel, rows top to bottom. PNG keeps alpha; JPEG has none (see [`encode_jpeg`]).
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub(crate) mod psd;

use image::{ExtendedColorType, ImageEncoder, ImageFormat, ImageReader, Limits};
use std::fmt;
use std::io::Cursor;

/// Longest side accepted, in pixels.
pub const MAX_DIMENSION: u32 = 32_768;
/// File extensions (lowercase, no dot) of every image format Inbetween reads.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "bmp", "tif", "tiff", "tga", "gif", "psd",
];

/// Most pixels accepted. Checked from the file header before any pixel data is allocated.
pub const MAX_PIXELS: u64 = 100_000_000;

/// A decoded or to-be-encoded image: straight 8-bit sRGB RGBA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8Image {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Bmp,
    Tiff,
    Tga,
    Gif,
    Psd,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodecError {
    /// The file is not a supported image, or uses a feature we do not read.
    Unsupported(String),
    /// The data is corrupt or truncated.
    Decode(String),
    Encode(String),
    /// Zero-sized, or beyond [`MAX_DIMENSION`] / [`MAX_PIXELS`].
    BadSize {
        width: u32,
        height: u32,
    },
    /// The pixel buffer length does not match the size.
    BadBuffer {
        expected: usize,
        got: usize,
    },
    /// Reading or writing the file failed (missing, no permission, disk full...).
    Io(String),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(m) => write!(f, "unsupported image: {m}"),
            Self::Decode(m) => write!(f, "could not decode image: {m}"),
            Self::Encode(m) => write!(f, "could not encode image: {m}"),
            Self::BadSize { width, height } => write!(
                f,
                "image size {width} x {height} is not allowed (sides 1 to {MAX_DIMENSION}, at most {MAX_PIXELS} pixels)"
            ),
            Self::BadBuffer { expected, got } => {
                write!(f, "pixel buffer has {got} bytes, expected {expected}")
            }
            Self::Io(m) => write!(f, "file error: {m}"),
        }
    }
}

impl std::error::Error for CodecError {}

fn check_size(width: u32, height: u32) -> Result<(), CodecError> {
    let bad = CodecError::BadSize { width, height };
    let ok = width > 0
        && height > 0
        && width <= MAX_DIMENSION
        && height <= MAX_DIMENSION
        && u64::from(width) * u64::from(height) <= MAX_PIXELS;
    if ok {
        Ok(())
    } else {
        Err(bad)
    }
}

fn check_buffer(width: u32, height: u32, data: &[u8]) -> Result<(), CodecError> {
    check_size(width, height)?;
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|p| p.checked_mul(4));
    match expected {
        Some(e) if e == data.len() => Ok(()),
        Some(e) => Err(CodecError::BadBuffer {
            expected: e,
            got: data.len(),
        }),
        None => Err(CodecError::BadSize { width, height }),
    }
}

/// A reader over `bytes`. TGA has no magic bytes, so it is only chosen from `ext`.
fn reader<'a>(
    bytes: &'a [u8],
    ext: Option<&str>,
) -> Result<ImageReader<Cursor<&'a [u8]>>, CodecError> {
    let mut r = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| CodecError::Decode(e.to_string()))?;
    if r.format().is_none() && ext.is_some_and(|e| e.eq_ignore_ascii_case("tga")) {
        r.set_format(ImageFormat::Tga);
    }
    Ok(r)
}

/// Which format `bytes` start with, if it is one we read. TGA files are not recognised
/// here (no magic bytes); see [`detect_with_ext`].
pub fn detect(bytes: &[u8]) -> Option<Format> {
    detect_with_ext(bytes, None)
}

/// Like [`detect`], with the file extension (no dot) as a hint for TGA.
pub fn detect_with_ext(bytes: &[u8], ext: Option<&str>) -> Option<Format> {
    if psd::is_psd(bytes) {
        return Some(Format::Psd);
    }
    match reader(bytes, ext).ok()?.format()? {
        ImageFormat::Png => Some(Format::Png),
        ImageFormat::Jpeg => Some(Format::Jpeg),
        ImageFormat::Bmp => Some(Format::Bmp),
        ImageFormat::Tiff => Some(Format::Tiff),
        ImageFormat::Tga => Some(Format::Tga),
        ImageFormat::Gif => Some(Format::Gif),
        _ => None,
    }
}

/// Read the size from the header without decoding pixels.
pub fn read_size(bytes: &[u8]) -> Result<(u32, u32), CodecError> {
    read_size_with_ext(bytes, None)
}

/// Like [`read_size`], with the file extension as a hint for TGA.
pub fn read_size_with_ext(bytes: &[u8], ext: Option<&str>) -> Result<(u32, u32), CodecError> {
    match detect_with_ext(bytes, ext) {
        None => Err(CodecError::Unsupported(
            "not a PNG, JPEG, BMP, TIFF, TGA, GIF or PSD file".into(),
        )),
        Some(Format::Psd) => psd::read_size(bytes),
        Some(_) => {
            let (w, h) = reader(bytes, ext)?
                .into_dimensions()
                .map_err(|e| CodecError::Decode(e.to_string()))?;
            check_size(w, h)?;
            Ok((w, h))
        }
    }
}

/// Decode an image. Sixteen-bit images are reduced to 8 bits; greyscale and palette
/// images are expanded; images without alpha come out fully opaque.
pub fn decode(bytes: &[u8]) -> Result<Rgba8Image, CodecError> {
    decode_with_ext(bytes, None)
}

/// Like [`decode`], with the file extension as a hint for TGA.
pub fn decode_with_ext(bytes: &[u8], ext: Option<&str>) -> Result<Rgba8Image, CodecError> {
    let (width, height) = read_size_with_ext(bytes, ext)?;
    if psd::is_psd(bytes) {
        return psd::decode(bytes);
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_PIXELS * 8);
    let mut r = reader(bytes, ext)?;
    r.limits(limits);
    let img = r.decode().map_err(|e| CodecError::Decode(e.to_string()))?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    if (w, h) != (width, height) {
        return Err(CodecError::Decode(
            "image size changed while decoding".into(),
        ));
    }
    Ok(Rgba8Image {
        width: w,
        height: h,
        data: rgba.into_raw(),
    })
}

/// Encode as PNG, keeping alpha.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, CodecError> {
    check_buffer(width, height, rgba)?;
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|e| CodecError::Encode(e.to_string()))?;
    Ok(out)
}

/// Encode as JPEG at `quality` (clamped to 1..=100). JPEG has no alpha, so pixels are
/// composited over white first.
pub fn encode_jpeg(
    width: u32,
    height: u32,
    rgba: &[u8],
    quality: u8,
) -> Result<Vec<u8>, CodecError> {
    check_buffer(width, height, rgba)?;
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for px in rgba.chunks_exact(4) {
        let a = u32::from(px[3]);
        for &c in &px[..3] {
            // c * a/255 + 255 * (1 - a/255), rounded.
            rgb.push(((u32::from(c) * a + 255 * (255 - a) + 127) / 255) as u8);
        }
    }
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100))
        .write_image(&rgb, width, height, ExtendedColorType::Rgb8)
        .map_err(|e| CodecError::Encode(e.to_string()))?;
    Ok(out)
}

impl Rgba8Image {
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Result<Self, CodecError> {
        check_buffer(width, height, &data)?;
        Ok(Self {
            width,
            height,
            data,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod files {
    use super::*;
    use std::path::Path;

    /// Read and decode an image file.
    pub fn read_file(path: &Path) -> Result<Rgba8Image, CodecError> {
        let io = |e: std::io::Error| CodecError::Io(format!("{}: {e}", path.display()));
        // Only regular files: opening a device such as `CON` could block forever.
        if !std::fs::metadata(path).map_err(io)?.is_file() {
            return Err(CodecError::Io(format!(
                "{}: not a regular file",
                path.display()
            )));
        }
        let bytes = std::fs::read(path).map_err(io)?;
        let ext = path.extension().and_then(|e| e.to_str());
        decode_with_ext(&bytes, ext)
    }

    /// Encode and write an image, choosing PNG or JPEG from the extension. JPEG uses
    /// quality 92.
    pub fn write_file(path: &Path, img: &Rgba8Image) -> Result<(), CodecError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        let bytes = match ext.as_deref() {
            Some("png") => encode_png(img.width, img.height, &img.data)?,
            Some("jpg" | "jpeg") => encode_jpeg(img.width, img.height, &img.data, 92)?,
            _ => {
                return Err(CodecError::Unsupported(format!(
                    "{}: use a .png, .jpg or .jpeg extension",
                    path.display()
                )))
            }
        };
        std::fs::write(path, bytes).map_err(|e| CodecError::Io(format!("{}: {e}", path.display())))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use files::{read_file, write_file};

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(w: u32, h: u32) -> Vec<u8> {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&[
                    (x * 7) as u8,
                    (y * 11) as u8,
                    (x + y) as u8,
                    255 - (x * 3) as u8,
                ]);
            }
        }
        v
    }

    #[test]
    fn png_round_trip_is_exact_and_keeps_alpha() {
        let data = sample(17, 9);
        let png = encode_png(17, 9, &data).unwrap();
        assert_eq!(detect(&png), Some(Format::Png));
        let back = decode(&png).unwrap();
        assert_eq!((back.width, back.height), (17, 9));
        assert_eq!(back.data, data);
    }

    #[test]
    fn jpeg_round_trip_is_close() {
        // Smooth opaque gradient: JPEG should stay within a few levels.
        let (w, h) = (32u32, 32u32);
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[(x * 8) as u8, (y * 8) as u8, 128, 255]);
            }
        }
        let jpg = encode_jpeg(w, h, &data, 95).unwrap();
        assert_eq!(detect(&jpg), Some(Format::Jpeg));
        let back = decode(&jpg).unwrap();
        assert_eq!((back.width, back.height), (w, h));
        let worst = data
            .iter()
            .zip(&back.data)
            .map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs())
            .max()
            .unwrap();
        assert!(worst <= 12, "worst channel error {worst}");
        assert!(back.data.chunks_exact(4).all(|p| p[3] == 255));
    }

    #[test]
    fn jpeg_flattens_alpha_over_white() {
        let jpg = encode_jpeg(8, 8, &[0, 0, 0, 0].repeat(64), 95).unwrap();
        let back = decode(&jpg).unwrap();
        assert!(back
            .data
            .chunks_exact(4)
            .all(|p| p[0] >= 250 && p[1] >= 250 && p[2] >= 250));
    }

    fn alpha_sample() -> Vec<u8> {
        let mut v = Vec::new();
        for i in 0..12u32 {
            v.extend_from_slice(&[(i * 20) as u8, 255 - (i * 9) as u8, 40, (i * 21) as u8]);
        }
        v
    }

    #[test]
    fn bmp_tiff_tga_keep_alpha_and_round_trip() {
        let data = alpha_sample();
        let mut bmp = Vec::new();
        image::codecs::bmp::BmpEncoder::new(&mut bmp)
            .write_image(&data, 4, 3, ExtendedColorType::Rgba8)
            .unwrap();
        let mut tiff = Cursor::new(Vec::new());
        image::codecs::tiff::TiffEncoder::new(&mut tiff)
            .write_image(&data, 4, 3, ExtendedColorType::Rgba8)
            .unwrap();
        let mut tga = Vec::new();
        image::codecs::tga::TgaEncoder::new(&mut tga)
            .write_image(&data, 4, 3, ExtendedColorType::Rgba8)
            .unwrap();
        let png = encode_png(4, 3, &data).unwrap();

        assert_eq!(detect(&bmp), Some(Format::Bmp));
        assert_eq!(detect(tiff.get_ref()), Some(Format::Tiff));
        assert_eq!(detect(&png), Some(Format::Png));
        // TGA has no magic: only the extension hint finds it.
        assert_eq!(detect(&tga), None);
        assert_eq!(detect_with_ext(&tga, Some("TGA")), Some(Format::Tga));
        assert_eq!(read_size_with_ext(&tga, Some("tga")), Ok((4, 3)));

        for (name, got) in [
            ("bmp", decode(&bmp).unwrap()),
            ("tiff", decode(tiff.get_ref()).unwrap()),
            ("tga", decode_with_ext(&tga, Some("tga")).unwrap()),
            ("png", decode(&png).unwrap()),
        ] {
            assert_eq!((got.width, got.height), (4, 3), "{name}");
            assert_eq!(got.data, data, "{name}");
        }
    }

    #[test]
    fn gif_reads_the_first_frame() {
        let mut gif = Vec::new();
        {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut gif);
            let red = image::RgbaImage::from_pixel(5, 4, image::Rgba([255, 0, 0, 255]));
            let blue = image::RgbaImage::from_pixel(5, 4, image::Rgba([0, 0, 255, 255]));
            enc.encode_frame(image::Frame::new(red)).unwrap();
            enc.encode_frame(image::Frame::new(blue)).unwrap();
        }
        assert_eq!(detect(&gif), Some(Format::Gif));
        let img = decode(&gif).unwrap();
        assert_eq!((img.width, img.height), (5, 4));
        assert_eq!(&img.data[..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn psd_goes_through_decode_and_keeps_alpha() {
        let planes = vec![vec![1, 2], vec![3, 4], vec![5, 6], vec![0, 200]];
        let bytes = psd::tests::build(2, 1, 3, &planes, true);
        assert_eq!(detect(&bytes), Some(Format::Psd));
        assert_eq!(read_size(&bytes), Ok((2, 1)));
        let img = decode(&bytes).unwrap();
        assert_eq!(img.data, [1, 3, 5, 0, 2, 4, 6, 200]);
    }

    #[test]
    fn every_listed_extension_is_lowercase() {
        assert!(IMAGE_EXTENSIONS
            .iter()
            .all(|e| *e == e.to_ascii_lowercase()));
        assert!(IMAGE_EXTENSIONS.contains(&"psd"));
    }

    #[test]
    fn garbage_in_new_formats_is_an_error_not_a_panic() {
        for magic in [&b"BM"[..], b"II*\0", b"MM\0*", b"GIF89a", b"8BPS"] {
            assert!(decode(magic).is_err());
        }
        assert!(decode_with_ext(b"\x00\x01\x02", Some("tga")).is_err());
    }

    #[test]
    fn read_size_matches_without_decoding() {
        let png = encode_png(5, 3, &sample(5, 3)).unwrap();
        assert_eq!(read_size(&png), Ok((5, 3)));
    }

    #[test]
    fn encoders_validate_buffers() {
        assert_eq!(
            encode_png(2, 2, &[0; 15]),
            Err(CodecError::BadBuffer {
                expected: 16,
                got: 15
            })
        );
        assert!(matches!(
            encode_png(0, 4, &[]),
            Err(CodecError::BadSize { .. })
        ));
        assert!(matches!(
            encode_jpeg(u32::MAX, u32::MAX, &[], 90),
            Err(CodecError::BadSize { .. })
        ));
        assert!(matches!(
            Rgba8Image::new(2, 2, vec![0; 3]),
            Err(CodecError::BadBuffer { .. })
        ));
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(matches!(decode(&[]), Err(CodecError::Unsupported(_))));
        assert!(matches!(
            decode(b"definitely not an image"),
            Err(CodecError::Unsupported(_))
        ));
        // PNG signature, nothing else.
        assert!(decode(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]).is_err());
        // JPEG magic, nothing else.
        assert!(decode(&[0xFF, 0xD8, 0xFF, 0xE0]).is_err());
    }

    #[test]
    fn truncated_files_are_errors() {
        let png = encode_png(16, 16, &sample(16, 16)).unwrap();
        // Cutting only the trailing IEND bytes leaves all pixel data intact, so stop short of that.
        for cut in [10, png.len() / 2, png.len() * 3 / 4] {
            assert!(decode(&png[..cut]).is_err(), "png cut at {cut}");
        }
        let jpg = encode_jpeg(16, 16, &sample(16, 16), 90).unwrap();
        for cut in [10, jpg.len() / 2] {
            // A truncated JPEG may be rejected or partially decoded; it must not panic.
            let _ = decode(&jpg[..cut]);
        }
    }

    #[test]
    fn oversized_header_is_rejected_before_allocating() {
        // A valid PNG whose header claims a huge size: IHDR width/height patched, CRC wrong.
        let mut png = encode_png(2, 2, &sample(2, 2)).unwrap();
        png[16..20].copy_from_slice(&0x7FFF_FFFFu32.to_be_bytes());
        png[20..24].copy_from_slice(&0x7FFF_FFFFu32.to_be_bytes());
        assert!(decode(&png).is_err());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn files_round_trip_and_report_io_errors() {
        let dir = std::env::temp_dir().join(format!("inbetween-codecs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let img = Rgba8Image::new(4, 4, sample(4, 4)).unwrap();
        let p = dir.join("a.png");
        write_file(&p, &img).unwrap();
        assert_eq!(read_file(&p).unwrap(), img);
        let j = dir.join("b.JPG");
        write_file(&j, &img).unwrap();
        assert!(read_file(&j).is_ok());
        assert!(matches!(
            write_file(&dir.join("c.bmp"), &img),
            Err(CodecError::Unsupported(_))
        ));
        assert!(matches!(
            read_file(&dir.join("missing.png")),
            Err(CodecError::Io(_))
        ));
        assert!(matches!(
            write_file(&dir.join("no/such/dir/x.png"), &img),
            Err(CodecError::Io(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod device_tests {
    use super::*;
    use std::path::Path;

    /// Regression: on Windows `CON` is the console, and reading it blocked forever.
    #[test]
    fn devices_and_directories_are_errors_not_hangs() {
        let mut paths: Vec<&str> = vec!["."];
        if cfg!(windows) {
            paths.extend(["CON", "NUL", "AUX", "PRN", "COM1"]);
        } else {
            paths.extend(["/dev/null", "/dev/zero"]);
        }
        for p in paths {
            assert!(
                matches!(read_file(Path::new(p)), Err(CodecError::Io(_))),
                "{p} should be rejected"
            );
        }
    }
}
