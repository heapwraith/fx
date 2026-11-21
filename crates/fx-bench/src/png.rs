//! Minimal PNG encoder for fixture thumbnails: 8-bit RGB, no compression (zlib
//! "stored" blocks). Written by hand to avoid a dependency for test data only.

const SIDE: usize = 64;

/// Encodes a 64×64 RGB image whose colors are derived from `seed` (a gradient
/// with a seed-dependent hue, so thumbnails are visibly different).
pub fn thumbnail_64(seed: u64) -> Vec<u8> {
    let base = seed.to_le_bytes();
    let mut raw = Vec::with_capacity(SIDE * (1 + SIDE * 3));
    for y in 0..SIDE {
        raw.push(0); // filter type: none
        for x in 0..SIDE {
            raw.push(base[0].wrapping_add((x * 4) as u8));
            raw.push(base[1].wrapping_add((y * 4) as u8));
            raw.push(base[2].wrapping_add(((x + y) * 2) as u8));
        }
    }
    encode_rgb(SIDE as u32, SIDE as u32, &raw)
}

/// `scanlines` = for each row, a filter byte followed by `width * 3` bytes.
fn encode_rgb(width: u32, height: u32, scanlines: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(scanlines.len() + 128);
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // depth 8, RGB, deflate, no filter, no interlace
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(scanlines));
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut blocks = data.chunks(0xFFFF).peekable();
    if blocks.peek().is_none() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(block) = blocks.next() {
        out.push(u8::from(blocks.peek().is_none())); // BFINAL, BTYPE = 00 (stored)
        let len = block.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn thumbnail_has_valid_structure() {
        let png = thumbnail_64(1);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes([png[16], png[17], png[18], png[19]]), 64);
        assert!(png.ends_with(&[b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]));
        // 64 rows × (1 + 64×3) bytes, stored in a single deflate block.
        let idat_len = u32::from_be_bytes([png[33], png[34], png[35], png[36]]) as usize;
        assert_eq!(idat_len, 2 + 5 + 64 * (1 + 64 * 3) + 4);
        assert_ne!(thumbnail_64(1), thumbnail_64(2));
        assert_eq!(thumbnail_64(1), thumbnail_64(1));
    }
}
