//! A small reader for the flattened composite of a Photoshop (`.psd`) file.
//!
//! Layers are ignored: only the merged image data at the end of the file is decoded.
//! RGB and greyscale, 8 or 16 bits per channel, raw or PackBits-compressed. Everything
//! else (CMYK, Lab, indexed, bitmap, 32-bit, the large-document `.psb`, ZIP-compressed
//! data) is an "unsupported" error. Every length in the file is hostile: all reads go
//! through bounds-checked accessors.

use crate::{check_size, CodecError, Rgba8Image};

const MAX_CHANNELS: u16 = 56;

/// True when `bytes` start with the PSD signature.
pub fn is_psd(bytes: &[u8]) -> bool {
    bytes.starts_with(b"8BPS")
}

fn truncated() -> CodecError {
    CodecError::Decode("PSD file is truncated or corrupt".into())
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], CodecError> {
        let end = self.pos.checked_add(n).ok_or_else(truncated)?;
        let s = self.bytes.get(self.pos..end).ok_or_else(truncated)?;
        self.pos = end;
        Ok(s)
    }

    fn u16(&mut self) -> Result<u16, CodecError> {
        let mut a = [0u8; 2];
        a.copy_from_slice(self.take(2)?);
        Ok(u16::from_be_bytes(a))
    }

    fn u32(&mut self) -> Result<u32, CodecError> {
        let mut a = [0u8; 4];
        a.copy_from_slice(self.take(4)?);
        Ok(u32::from_be_bytes(a))
    }

    fn skip_section(&mut self) -> Result<(), CodecError> {
        let len = self.u32()? as usize;
        self.take(len).map(|_| ())
    }
}

struct Header {
    channels: u16,
    width: u32,
    height: u32,
    depth: u16,
    gray: bool,
}

fn header(r: &mut Reader) -> Result<Header, CodecError> {
    if r.take(4)? != b"8BPS" {
        return Err(CodecError::Unsupported("not a PSD file".into()));
    }
    match r.u16()? {
        1 => {}
        2 => return Err(CodecError::Unsupported("PSB (large document) files".into())),
        v => return Err(CodecError::Unsupported(format!("PSD version {v}"))),
    }
    r.take(6)?;
    let channels = r.u16()?;
    let height = r.u32()?;
    let width = r.u32()?;
    let depth = r.u16()?;
    let mode = r.u16()?;
    if channels == 0 || channels > MAX_CHANNELS {
        return Err(CodecError::Decode(format!("PSD has {channels} channels")));
    }
    if depth != 8 && depth != 16 {
        return Err(CodecError::Unsupported(format!(
            "PSD with {depth} bits per channel (8 and 16 are read)"
        )));
    }
    let gray = match mode {
        1 => true,
        3 => false,
        m => {
            return Err(CodecError::Unsupported(format!(
                "PSD colour mode {m} (only RGB and greyscale are read)"
            )))
        }
    };
    if !gray && channels < 3 {
        return Err(CodecError::Decode("an RGB PSD needs 3 channels".into()));
    }
    check_size(width, height)?;
    Ok(Header {
        channels,
        width,
        height,
        depth,
        gray,
    })
}

/// Width and height from the header alone.
pub fn read_size(bytes: &[u8]) -> Result<(u32, u32), CodecError> {
    let h = header(&mut Reader { bytes, pos: 0 })?;
    Ok((h.width, h.height))
}

/// Expand one PackBits scanline into exactly `row.len()` bytes.
fn unpack(src: &[u8], row: &mut [u8]) -> Result<(), CodecError> {
    let bad = || CodecError::Decode("PSD run-length data is corrupt".into());
    let (mut i, mut o) = (0usize, 0usize);
    while o < row.len() {
        let n = *src.get(i).ok_or_else(bad)? as i8;
        i += 1;
        if n >= 0 {
            let len = n as usize + 1;
            let from = src.get(i..i + len).ok_or_else(bad)?;
            row.get_mut(o..o + len)
                .ok_or_else(bad)?
                .copy_from_slice(from);
            i += len;
            o += len;
        } else if n != -128 {
            let len = (1 - i16::from(n)) as usize;
            let v = *src.get(i).ok_or_else(bad)?;
            row.get_mut(o..o + len).ok_or_else(bad)?.fill(v);
            i += 1;
            o += len;
        }
    }
    Ok(())
}

/// Decode the merged image of a PSD file.
pub fn decode(bytes: &[u8]) -> Result<Rgba8Image, CodecError> {
    let mut r = Reader { bytes, pos: 0 };
    let h = header(&mut r)?;
    r.skip_section()?; // colour mode data
    r.skip_section()?; // image resources
    r.skip_section()?; // layer and mask information
    let compression = r.u16()?;

    let (w, ht) = (h.width as usize, h.height as usize);
    let bps = usize::from(h.depth / 8);
    let row_bytes = w.checked_mul(bps).ok_or_else(truncated)?;
    let channels = usize::from(h.channels);
    let used = channels.min(if h.gray { 2 } else { 4 });
    let pixels = w.checked_mul(ht).ok_or_else(truncated)?;
    let total = pixels.checked_mul(4).ok_or_else(truncated)?;

    // Check the data is present before allocating the output.
    let rest = bytes.len().saturating_sub(r.pos);
    let needed = match compression {
        0 => row_bytes
            .checked_mul(ht)
            .and_then(|p| p.checked_mul(used))
            .ok_or_else(truncated)?,
        1 => ht
            .checked_mul(channels)
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(truncated)?,
        c => {
            return Err(CodecError::Unsupported(format!(
                "PSD compression {c} (raw and run-length are read)"
            )))
        }
    };
    if rest < needed {
        return Err(truncated());
    }

    let mut out = vec![255u8; total];
    let mut row = vec![0u8; row_bytes];
    let start = r.pos;
    // Run-length data follows the table of byte counts.
    let mut data_at = start.checked_add(needed).ok_or_else(truncated)?;
    for c in 0..used {
        for y in 0..ht {
            if compression == 0 {
                let at = c
                    .checked_mul(ht)
                    .and_then(|v| v.checked_add(y))
                    .and_then(|v| v.checked_mul(row_bytes))
                    .and_then(|v| v.checked_add(start))
                    .ok_or_else(truncated)?;
                let end = at.checked_add(row_bytes).ok_or_else(truncated)?;
                row.copy_from_slice(bytes.get(at..end).ok_or_else(truncated)?);
            } else {
                let ci = start + 2 * (c * ht + y);
                let cb = bytes.get(ci..ci + 2).ok_or_else(truncated)?;
                let len = usize::from(u16::from_be_bytes([cb[0], cb[1]]));
                let end = data_at.checked_add(len).ok_or_else(truncated)?;
                let src = bytes.get(data_at..end).ok_or_else(truncated)?;
                data_at = end;
                unpack(src, &mut row)?;
            }
            for x in 0..w {
                // The high byte of a 16-bit sample is the 8-bit value.
                let v = row.get(x * bps).copied().unwrap_or(0);
                let base = (y * w + x) * 4;
                if h.gray && c == 0 {
                    if let Some(px) = out.get_mut(base..base + 3) {
                        px.fill(v);
                    }
                } else {
                    let dest = if h.gray { 3 } else { c };
                    if let Some(p) = out.get_mut(base + dest) {
                        *p = v;
                    }
                }
            }
        }
    }
    Rgba8Image::new(h.width, h.height, out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A PSD with the given composite planes (one `Vec<u8>` per channel, 8-bit).
    pub(crate) fn build(w: u32, h: u32, mode: u16, planes: &[Vec<u8>], rle: bool) -> Vec<u8> {
        let mut v = b"8BPS".to_vec();
        v.extend_from_slice(&1u16.to_be_bytes());
        v.extend_from_slice(&[0; 6]);
        v.extend_from_slice(&(planes.len() as u16).to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&8u16.to_be_bytes());
        v.extend_from_slice(&mode.to_be_bytes());
        v.extend_from_slice(&0u32.to_be_bytes()); // colour mode data
        v.extend_from_slice(&4u32.to_be_bytes()); // resources: junk
        v.extend_from_slice(&[1, 2, 3, 4]);
        v.extend_from_slice(&0u32.to_be_bytes()); // layers
        if !rle {
            v.extend_from_slice(&0u16.to_be_bytes());
            for p in planes {
                v.extend_from_slice(p);
            }
            return v;
        }
        v.extend_from_slice(&1u16.to_be_bytes());
        // Literal-only PackBits rows.
        let mut rows = Vec::new();
        let mut counts = Vec::new();
        for p in planes {
            for row in p.chunks(w as usize) {
                let mut enc = vec![(row.len() - 1) as u8];
                enc.extend_from_slice(row);
                counts.extend_from_slice(&(enc.len() as u16).to_be_bytes());
                rows.extend_from_slice(&enc);
            }
        }
        v.extend_from_slice(&counts);
        v.extend_from_slice(&rows);
        v
    }

    fn planes() -> Vec<Vec<u8>> {
        vec![
            vec![10, 20, 30, 40, 50, 60],
            vec![1, 2, 3, 4, 5, 6],
            vec![100, 110, 120, 130, 140, 150],
        ]
    }

    #[test]
    fn raw_and_rle_rgb_decode_the_same() {
        for rle in [false, true] {
            let img = decode(&build(3, 2, 3, &planes(), rle)).unwrap();
            assert_eq!((img.width, img.height), (3, 2));
            assert_eq!(&img.data[..8], &[10, 1, 100, 255, 20, 2, 110, 255]);
            assert_eq!(&img.data[20..24], &[60, 6, 150, 255]);
        }
    }

    #[test]
    fn alpha_channel_is_kept_and_extra_channels_ignored() {
        let mut p = planes();
        p.push(vec![0, 64, 128, 255, 7, 8]);
        p.push(vec![9; 6]); // a spot channel
        for rle in [false, true] {
            let img = decode(&build(3, 2, 3, &p, rle)).unwrap();
            let alpha: Vec<u8> = img.data.chunks(4).map(|px| px[3]).collect();
            assert_eq!(alpha, [0, 64, 128, 255, 7, 8]);
        }
    }

    #[test]
    fn greyscale_with_and_without_alpha() {
        let g = vec![vec![5, 6, 7, 8, 9, 10]];
        let img = decode(&build(3, 2, 1, &g, false)).unwrap();
        assert_eq!(&img.data[..8], &[5, 5, 5, 255, 6, 6, 6, 255]);
        let ga = vec![vec![5, 6, 7, 8, 9, 10], vec![1, 2, 3, 4, 5, 6]];
        let img = decode(&build(3, 2, 1, &ga, true)).unwrap();
        assert_eq!(&img.data[..8], &[5, 5, 5, 1, 6, 6, 6, 2]);
    }

    #[test]
    fn sixteen_bit_takes_the_high_byte() {
        let mut v = build(1, 1, 1, &[vec![0, 0]], false);
        // Patch depth to 16 (bytes 22..24) and give the single pixel 0xAB12.
        v[22..24].copy_from_slice(&16u16.to_be_bytes());
        let n = v.len();
        v[n - 2..].copy_from_slice(&[0xAB, 0x12]);
        let img = decode(&v).unwrap();
        assert_eq!(&img.data[..3], &[0xAB; 3]);
    }

    #[test]
    fn header_only_size() {
        assert_eq!(read_size(&build(3, 2, 3, &planes(), false)), Ok((3, 2)));
    }

    #[test]
    fn unsupported_modes_are_refused() {
        let cmyk = build(3, 2, 4, &planes(), false);
        assert!(matches!(decode(&cmyk), Err(CodecError::Unsupported(_))));
        let mut deep = build(3, 2, 3, &planes(), false);
        deep[22..24].copy_from_slice(&32u16.to_be_bytes());
        assert!(matches!(decode(&deep), Err(CodecError::Unsupported(_))));
        let mut psb = build(3, 2, 3, &planes(), false);
        psb[4..6].copy_from_slice(&2u16.to_be_bytes());
        assert!(matches!(decode(&psb), Err(CodecError::Unsupported(_))));
        let mut zip = build(3, 2, 3, &planes(), false);
        let at = zip.len() - 18 - 2;
        zip[at..at + 2].copy_from_slice(&2u16.to_be_bytes());
        assert!(matches!(decode(&zip), Err(CodecError::Unsupported(_))));
    }

    #[test]
    fn truncation_never_panics() {
        for rle in [false, true] {
            let full = build(3, 2, 3, &planes(), rle);
            for cut in 0..full.len() {
                assert!(decode(&full[..cut]).is_err(), "rle {rle} cut at {cut}");
            }
        }
    }

    #[test]
    fn huge_dimensions_are_rejected_before_allocating() {
        let mut v = build(3, 2, 3, &planes(), false);
        v[14..18].copy_from_slice(&u32::MAX.to_be_bytes());
        v[18..22].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(decode(&v), Err(CodecError::BadSize { .. })));
        // Within limits but the file holds no pixel data.
        let mut v = build(3, 2, 3, &planes(), false);
        v[14..18].copy_from_slice(&30_000u32.to_be_bytes());
        v[18..22].copy_from_slice(&3_000u32.to_be_bytes());
        assert!(decode(&v).is_err());
    }

    #[test]
    fn bad_run_length_data_is_an_error() {
        let mut v = build(3, 2, 3, &planes(), true);
        let n = v.len();
        // Turn the first row's literal count into one that overruns the row.
        v[n - 6 * 4] = 100;
        assert!(decode(&v).is_err());
        // Claimed row length beyond the file.
        let mut v = build(3, 2, 3, &planes(), true);
        let table = v.len() - 6 * 4 - 6 * 2;
        v[table..table + 2].copy_from_slice(&u16::MAX.to_be_bytes());
        assert!(decode(&v).is_err());
    }
}
