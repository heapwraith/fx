//! Listing a ZIP archive for Quick Look (task T2.9): reads only the central
//! directory at the end of the file, never decompresses anything. Handles
//! ZIP64 sizes and the UTF-8 flag; other names are read as CP437, like
//! Windows does for old archives.

/// One entry of the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub compressed: u64,
}

/// The listing: entries in archive order, and whether the list was cut at
/// `max` entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipListing {
    pub entries: Vec<ZipEntry>,
    pub total: u64,
    pub truncated: bool,
}

const EOCD: u32 = 0x0605_4b50;
const EOCD64_LOCATOR: u32 = 0x0706_4b50;
const EOCD64: u32 = 0x0606_4b50;
const CENTRAL: u32 = 0x0201_4b50;

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(i..i + 8)?.try_into().ok()?))
}

/// Where the central directory is: `(offset, size, entries)`, from the last
/// up-to-64 KiB of the file (`tail`, which starts at `tail_offset`).
/// `read_at` reads the ZIP64 record when needed.
pub fn central_directory(
    tail: &[u8],
    tail_offset: u64,
    read_at: &mut dyn FnMut(u64, usize) -> Option<Vec<u8>>,
) -> Option<(u64, u64, u64)> {
    let at = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| u32_at(tail, i) == Some(EOCD))?;
    let mut entries = u64::from(u16_at(tail, at + 10)?);
    let mut size = u64::from(u32_at(tail, at + 12)?);
    let mut offset = u64::from(u32_at(tail, at + 16)?);
    if entries == 0xFFFF || size == 0xFFFF_FFFF || offset == 0xFFFF_FFFF {
        // ZIP64: the locator sits right before the classic record.
        let loc = at.checked_sub(20)?;
        if u32_at(tail, loc) != Some(EOCD64_LOCATOR) {
            return None;
        }
        let rec_offset = u64_at(tail, loc + 8)?;
        let rec = if rec_offset >= tail_offset {
            tail.get((rec_offset - tail_offset) as usize..)?.to_vec()
        } else {
            read_at(rec_offset, 56)?
        };
        if u32_at(&rec, 0) != Some(EOCD64) {
            return None;
        }
        entries = u64_at(&rec, 32)?;
        size = u64_at(&rec, 40)?;
        offset = u64_at(&rec, 48)?;
    }
    Some((offset, size, entries))
}

/// Parses the central directory bytes; at most `max` entries are kept.
pub fn parse_central(dir: &[u8], total: u64, max: usize) -> Option<ZipListing> {
    let mut entries = Vec::new();
    let mut i = 0;
    while i + 46 <= dir.len() && entries.len() < max {
        if u32_at(dir, i)? != CENTRAL {
            break;
        }
        let flags = u16_at(dir, i + 8)?;
        let mut compressed = u64::from(u32_at(dir, i + 20)?);
        let mut size = u64::from(u32_at(dir, i + 24)?);
        let name_len = usize::from(u16_at(dir, i + 28)?);
        let extra_len = usize::from(u16_at(dir, i + 30)?);
        let comment_len = usize::from(u16_at(dir, i + 32)?);
        let name_bytes = dir.get(i + 46..i + 46 + name_len)?;
        let name = if flags & 0x0800 != 0 {
            String::from_utf8_lossy(name_bytes).into_owned()
        } else {
            cp437(name_bytes)
        };
        // ZIP64 extra field (id 1): the 64-bit sizes, in this order, only for
        // the fields set to 0xFFFFFFFF.
        let extra = dir.get(i + 46 + name_len..i + 46 + name_len + extra_len)?;
        let mut e = 0;
        while e + 4 <= extra.len() {
            let id = u16_at(extra, e)?;
            let len = usize::from(u16_at(extra, e + 2)?);
            if id == 1 {
                let mut p = e + 4;
                if size == 0xFFFF_FFFF {
                    size = u64_at(extra, p)?;
                    p += 8;
                }
                if compressed == 0xFFFF_FFFF {
                    compressed = u64_at(extra, p)?;
                }
            }
            e += 4 + len;
        }
        let is_dir = name.ends_with('/') || name.ends_with('\\');
        entries.push(ZipEntry {
            name,
            is_dir,
            size,
            compressed,
        });
        i += 46 + name_len + extra_len + comment_len;
    }
    let truncated = (entries.len() as u64) < total;
    Some(ZipListing {
        entries,
        total,
        truncated,
    })
}

/// IBM code page 437 (old ZIP tools on Windows): ASCII plus box drawing and
/// accented letters.
fn cp437(bytes: &[u8]) -> String {
    const HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";
    let high: Vec<char> = HIGH.chars().collect();
    bytes
        .iter()
        .map(|&b| {
            if b < 0x80 {
                b as char
            } else {
                high[(b - 0x80) as usize]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stored (uncompressed) ZIP built by hand.
    fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in files {
            let offset = out.len() as u32;
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            central.extend_from_slice(&CENTRAL.to_le_bytes());
            central.extend_from_slice(&[20, 0, 20, 0, 0, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&EOCD.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&[0; 2]);
        out
    }

    fn list(bytes: &[u8], max: usize) -> ZipListing {
        let (offset, size, total) = central_directory(bytes, 0, &mut |_, _| None).unwrap();
        let dir = &bytes[offset as usize..(offset + size) as usize];
        parse_central(dir, total, max).unwrap()
    }

    #[test]
    fn lists_names_sizes_and_folders() {
        let z = zip(&[
            ("pasta/", b""),
            ("pasta/relatório.txt", b"12345"),
            ("foto.jpg", b"xy"),
        ]);
        let l = list(&z, 100);
        assert_eq!(l.total, 3);
        assert!(!l.truncated);
        assert!(l.entries[0].is_dir);
        assert_eq!(l.entries[1].name, "pasta/relatório.txt");
        assert_eq!(l.entries[1].size, 5);
        assert_eq!(l.entries[2].size, 2);
        let l = list(&z, 2);
        assert!(l.truncated);
        assert_eq!(l.entries.len(), 2);
    }

    #[test]
    fn not_a_zip_or_damaged() {
        assert!(central_directory(b"hello world, not a zip at all", 0, &mut |_, _| None).is_none());
        let mut z = zip(&[("a", b"1")]);
        let len = z.len();
        z.truncate(len - 30);
        assert!(central_directory(&z, 0, &mut |_, _| None).is_none());
        assert!(
            parse_central(&[0x50, 0x4b, 0x01, 0x02], 1, 10)
                .unwrap()
                .entries
                .is_empty()
        );
    }

    #[test]
    fn cp437_names() {
        assert_eq!(cp437(b"caf\x82"), "café");
        assert_eq!(cp437(b"plain"), "plain");
    }
}
