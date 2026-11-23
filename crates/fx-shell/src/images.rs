//! Image quick actions (T6.16, D-069): a copy of an image in another format
//! or at a smaller size, and the date a photo was taken, with Rust crates
//! that are the same on Windows and Linux: `image` (the version GPUI already
//! builds) to decode, `jpeg-encoder` (SIMD) to write JPEG and
//! `kamadak-exif` for the date; halving is a 2×2 average done here. The original is never touched; run on a worker thread.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom};
use std::path::Path;

use image::ImageDecoder;

/// The format of the copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Jpeg,
}

impl ImageKind {
    pub fn extension(self) -> &'static str {
        match self {
            ImageKind::Png => "png",
            ImageKind::Jpeg => "jpg",
        }
    }
}

/// JPEG quality of the copies (the usual "high" setting of photo tools).
const JPEG_QUALITY: u8 = 90;

/// Writes `src` as `kind` to `dst` (created or replaced: the caller picks a
/// name nobody uses), at half the width and height when `half`, turned
/// upright (the EXIF orientation is applied, since the copy keeps no
/// metadata). Reads JPEG, PNG, GIF, BMP, WebP, TIFF and ICO. A failed write
/// leaves nothing at `dst`.
pub fn convert_image(src: &Path, dst: &Path, kind: ImageKind, half: bool) -> io::Result<()> {
    let result = write(src, dst, kind, half);
    if result.is_err() {
        let _ = std::fs::remove_file(dst);
    }
    result
}

fn invalid(e: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}

/// The image and its EXIF orientation, from one decoder.
fn decode_with(
    mut d: impl ImageDecoder,
) -> image::ImageResult<(image::DynamicImage, image::metadata::Orientation)> {
    let orientation = d.orientation()?;
    Ok((image::DynamicImage::from_decoder(d)?, orientation))
}

/// Decodes the formats fx offers, chosen by the file's first bytes; only
/// these decoders end up in the program.
fn decode(src: &Path) -> io::Result<image::DynamicImage> {
    use image::ImageFormat as F;
    use image::codecs as c;
    let mut file = File::open(src)?;
    let mut head = [0u8; 32];
    let n = file.read(&mut head)?;
    file.seek(SeekFrom::Start(0))?;
    let r = BufReader::new(file);
    let format = image::guess_format(&head[..n]).map_err(invalid)?;
    let (mut img, orientation) = match format {
        F::Jpeg => decode_with(c::jpeg::JpegDecoder::new(r).map_err(invalid)?),
        F::Png => decode_with(c::png::PngDecoder::new(r).map_err(invalid)?),
        F::Gif => decode_with(c::gif::GifDecoder::new(r).map_err(invalid)?),
        F::Bmp => decode_with(c::bmp::BmpDecoder::new(r).map_err(invalid)?),
        F::WebP => decode_with(c::webp::WebPDecoder::new(r).map_err(invalid)?),
        F::Tiff => decode_with(c::tiff::TiffDecoder::new(r).map_err(invalid)?),
        F::Ico => decode_with(c::ico::IcoDecoder::new(r).map_err(invalid)?),
        other => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("{other:?} images are not supported"),
            ));
        }
    }
    .map_err(invalid)?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// Half the width and height (rounded up): each pixel the average of a 2×2
/// block (the right column and bottom row repeat when odd), the right filter
/// for an exact halving.
fn halve(pixels: &[u8], w: u32, h: u32, channels: usize) -> (Vec<u8>, u32, u32) {
    let (w, h) = (w as usize, h as usize);
    let (nw, nh) = (w.div_ceil(2), h.div_ceil(2));
    let mut out = vec![0u8; nw * nh * channels];
    for y in 0..nh {
        let (y0, y1) = (2 * y, (2 * y + 1).min(h - 1));
        for x in 0..nw {
            let (x0, x1) = (2 * x, (2 * x + 1).min(w - 1));
            for c in 0..channels {
                let at = |xx: usize, yy: usize| u32::from(pixels[(yy * w + xx) * channels + c]);
                let sum = at(x0, y0) + at(x1, y0) + at(x0, y1) + at(x1, y1);
                out[(y * nw + x) * channels + c] = ((sum + 2) / 4) as u8;
            }
        }
    }
    (out, nw as u32, nh as u32)
}

fn write(src: &Path, dst: &Path, kind: ImageKind, half: bool) -> io::Result<()> {
    let img = decode(src)?;
    // JPEG has no transparency; a PNG keeps it when the source has it.
    let alpha = kind == ImageKind::Png && img.color().has_alpha();
    let (mut w, mut h) = (img.width(), img.height());
    let channels = if alpha { 4 } else { 3 };
    let mut pixels = if alpha {
        img.into_rgba8().into_raw()
    } else {
        img.into_rgb8().into_raw()
    };
    if half {
        (pixels, w, h) = halve(&pixels, w, h, channels);
    }
    let mut out = BufWriter::new(File::create(dst)?);
    match kind {
        ImageKind::Png => {
            use image::ImageEncoder;
            let color = if alpha {
                image::ExtendedColorType::Rgba8
            } else {
                image::ExtendedColorType::Rgb8
            };
            image::codecs::png::PngEncoder::new(&mut out)
                .write_image(&pixels, w, h, color)
                .map_err(invalid)?;
        }
        ImageKind::Jpeg => {
            let too_big = || invalid("image too large for JPEG");
            let (jw, jh) = (
                u16::try_from(w).map_err(|_| too_big())?,
                u16::try_from(h).map_err(|_| too_big())?,
            );
            jpeg_encoder::Encoder::new(&mut out, JPEG_QUALITY)
                .encode(&pixels, jw, jh, jpeg_encoder::ColorType::Rgb)
                .map_err(invalid)?;
        }
    }
    out.into_inner()
        .map_err(io::IntoInnerError::into_error)?
        .sync_all()
}

/// When a photo was taken, as local Unix seconds (EXIF keeps the camera's
/// clock, with no time zone): `DateTimeOriginal`, else `DateTimeDigitized`.
/// `None` when the file has no such date or no EXIF at all.
pub fn photo_taken(path: &Path) -> io::Result<Option<i64>> {
    let mut reader = BufReader::new(File::open(path)?);
    let Ok(exif) = exif::Reader::new().read_from_container(&mut reader) else {
        return Ok(None);
    };
    let date = [exif::Tag::DateTimeOriginal, exif::Tag::DateTimeDigitized]
        .into_iter()
        .find_map(|tag| {
            let field = exif.get_field(tag, exif::In::PRIMARY)?;
            let exif::Value::Ascii(ref parts) = field.value else {
                return None;
            };
            exif::DateTime::from_ascii(parts.first()?).ok()
        });
    Ok(date.and_then(|d| {
        let valid = (1..=12).contains(&d.month)
            && (1..=31).contains(&d.day)
            && d.hour < 24
            && d.minute < 60
            && d.second < 61;
        valid.then(|| {
            days_from_civil(i64::from(d.year), u32::from(d.month), u32::from(d.day)) * 86_400
                + i64::from(d.hour) * 3600
                + i64::from(d.minute) * 60
                + i64::from(d.second)
        })
    }))
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bmp(w: u32, h: u32) -> Vec<u8> {
        let row = (w * 3).div_ceil(4) * 4;
        let mut b = Vec::new();
        b.extend_from_slice(b"BM");
        b.extend_from_slice(&(54 + row * h).to_le_bytes());
        b.extend_from_slice(&[0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0]);
        b.extend_from_slice(&w.to_le_bytes());
        b.extend_from_slice(&h.to_le_bytes());
        b.extend_from_slice(&[1, 0, 24, 0]);
        b.extend_from_slice(&[0; 24]);
        b.extend(std::iter::repeat_n(0x80u8, (row * h) as usize));
        b
    }

    /// A 4×2 BMP written by hand, converted to PNG and to a half-size JPEG.
    #[test]
    fn converts_and_scales() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("x.bmp");
        std::fs::write(&src, bmp(4, 2)).unwrap();
        let png = dir.path().join("x.png");
        convert_image(&src, &png, ImageKind::Png, false).unwrap();
        assert_eq!(&std::fs::read(&png).unwrap()[1..4], b"PNG");
        let jpg = dir.path().join("x.jpg");
        convert_image(&src, &jpg, ImageKind::Jpeg, true).unwrap();
        assert_eq!(&std::fs::read(&jpg).unwrap()[..2], &[0xFF, 0xD8]);
        let half = image::open(&jpg).unwrap();
        assert_eq!((half.width(), half.height()), (2, 1));
        let bad = dir.path().join("bad.png");
        assert!(convert_image(&dir.path().join("none.bmp"), &bad, ImageKind::Png, false).is_err());
        assert!(!bad.exists(), "nothing left behind");
    }

    #[test]
    fn halving_averages_2x2_blocks() {
        // 3×2 gray: columns 0, 100, 200 on top; 50, 150, 250 below.
        let px = [0, 100, 200, 50, 150, 250];
        let (out, w, h) = halve(&px, 3, 2, 1);
        assert_eq!((w, h), (2, 1));
        assert_eq!(out, [75, 225], "(0+100+50+150)/4, odd column repeated");
    }

    /// A JPEG with an EXIF block written by hand (DateTimeOriginal
    /// 2024:07:15 14:30:22, the camera's local time).
    #[test]
    fn photo_date_taken_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("x.bmp");
        std::fs::write(&src, bmp(4, 2)).unwrap();
        let plain = dir.path().join("plain.jpg");
        convert_image(&src, &plain, ImageKind::Jpeg, false).unwrap();
        assert_eq!(photo_taken(&plain).unwrap(), None);
        // TIFF (little endian): IFD0 with the Exif IFD pointer, then the
        // Exif IFD with DateTimeOriginal (ASCII, 20 bytes).
        let mut t: Vec<u8> = b"II*\0".to_vec();
        t.extend_from_slice(&8u32.to_le_bytes());
        let entry = |tag: u16, kind: u16, count: u32, value: u32| {
            let mut e = Vec::new();
            e.extend_from_slice(&tag.to_le_bytes());
            e.extend_from_slice(&kind.to_le_bytes());
            e.extend_from_slice(&count.to_le_bytes());
            e.extend_from_slice(&value.to_le_bytes());
            e
        };
        // IFD0 at 8: 1 entry (2 + 12 + 4 = 18 bytes) → Exif IFD at 26.
        t.extend_from_slice(&1u16.to_le_bytes());
        t.extend(entry(0x8769, 4, 1, 26));
        t.extend_from_slice(&0u32.to_le_bytes());
        // Exif IFD at 26: 1 entry → the string at 44.
        t.extend_from_slice(&1u16.to_le_bytes());
        t.extend(entry(0x9003, 2, 20, 44));
        t.extend_from_slice(&0u32.to_le_bytes());
        t.extend_from_slice(b"2024:07:15 14:30:22\0");
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend(t);
        let jpeg = std::fs::read(&plain).unwrap();
        let mut out = jpeg[..2].to_vec();
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
        out.extend(app1);
        out.extend_from_slice(&jpeg[2..]);
        let photo = dir.path().join("foto.jpg");
        std::fs::write(&photo, out).unwrap();
        assert_eq!(photo_taken(&photo).unwrap(), Some(1_721_053_822));
        assert!(photo_taken(&dir.path().join("none.jpg")).is_err());
    }
}
