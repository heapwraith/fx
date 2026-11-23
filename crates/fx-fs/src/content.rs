//! Searching inside files (T6.8, D-068): whether a text file contains a
//! piece of text, ignoring case. Read in 1 MiB blocks (never the whole file
//! in memory), with an overlap so a match across two blocks is found.
//! UTF-8 (and ASCII/ANSI for plain letters) and UTF-16 with a BOM are read;
//! a file with a NUL byte in its first 8 KiB and no UTF-16 BOM is binary and
//! skipped. Files over `MAX_SIZE` are skipped too.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

const BLOCK: usize = 1 << 20;
/// Larger files are not read.
pub const MAX_SIZE: u64 = 64 << 20;

/// The needle as compared (lowercase).
pub fn fold_needle(text: &str) -> String {
    text.to_lowercase()
}

/// Whether the file at `path` contains `needle` (from [`fold_needle`]).
pub fn file_contains(path: &Path, needle: &str, cancel: &AtomicBool) -> io::Result<bool> {
    if needle.is_empty() {
        return Ok(false);
    }
    let mut f = File::open(path)?;
    if f.metadata()?.len() > MAX_SIZE {
        return Ok(false);
    }
    let mut buf = vec![0u8; BLOCK];
    let n = read_full(&mut f, &mut buf)?;
    let head = &buf[..n];
    let utf16 = match head {
        [0xFF, 0xFE, ..] => Some(false),
        [0xFE, 0xFF, ..] => Some(true),
        _ => None,
    };
    if utf16.is_none() && head[..head.len().min(8192)].contains(&0) {
        return Ok(false);
    }
    // Characters of overlap kept from the previous block.
    let keep = needle.chars().count();
    let mut carry = String::new();
    let mut first = true;
    let mut pending: Vec<u8> = Vec::new();
    let mut len = n;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        pending.extend_from_slice(&buf[..len]);
        let at_end = len < BLOCK;
        let text = match utf16 {
            Some(big) => {
                let skip = if first { 2 } else { 0 };
                let body = &pending[skip.min(pending.len())..];
                let whole = body.len() / 2 * 2;
                let units: Vec<u16> = body[..whole]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| {
                        if big {
                            u16::from_be_bytes([c[0], c[1]])
                        } else {
                            u16::from_le_bytes([c[0], c[1]])
                        }
                    })
                    .collect();
                let rest = body[whole..].to_vec();
                pending = rest;
                String::from_utf16_lossy(&units)
            }
            None => {
                // Keep an incomplete UTF-8 sequence at the end for the next block.
                let cut = if at_end {
                    pending.len()
                } else {
                    utf8_cut(&pending)
                };
                let text = String::from_utf8_lossy(&pending[..cut]).into_owned();
                pending = pending[cut..].to_vec();
                text
            }
        };
        first = false;
        let hay = carry.clone() + &text.to_lowercase();
        if hay.contains(needle) {
            return Ok(true);
        }
        if at_end {
            return Ok(false);
        }
        let tail: Vec<char> = hay.chars().rev().take(keep).collect();
        carry = tail.into_iter().rev().collect();
        len = read_full(&mut f, &mut buf)?;
        if len == 0 && pending.is_empty() {
            return Ok(false);
        }
    }
}

/// Where a complete UTF-8 prefix of `b` ends.
fn utf8_cut(b: &[u8]) -> usize {
    let n = b.len();
    for back in 1..=3.min(n) {
        let c = b[n - back];
        if c & 0xC0 != 0x80 {
            let need = match c {
                c if c >= 0xF0 => 4,
                c if c >= 0xE0 => 3,
                c if c >= 0xC0 => 2,
                _ => 1,
            };
            return if need > back { n - back } else { n };
        }
    }
    n
}

fn read_full(f: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_text_in_utf8_utf16_and_across_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let no = AtomicBool::new(false);
        let needle = fold_needle("Nota Fiscal");
        std::fs::write(d.join("a.txt"), "x\nA NOTA FISCAL nº 12\n").unwrap();
        assert!(file_contains(&d.join("a.txt"), &needle, &no).unwrap());
        assert!(!file_contains(&d.join("a.txt"), &fold_needle("recibo"), &no).unwrap());
        let mut u16le = vec![0xFF, 0xFE];
        u16le.extend(
            "ação: nota fiscal"
                .encode_utf16()
                .flat_map(u16::to_le_bytes),
        );
        std::fs::write(d.join("b.txt"), &u16le).unwrap();
        assert!(file_contains(&d.join("b.txt"), &needle, &no).unwrap());
        assert!(file_contains(&d.join("b.txt"), &fold_needle("AÇÃO"), &no).unwrap());
        // Across the 1 MiB boundary, with a multi-byte letter split there.
        let mut big = vec![b'a'; BLOCK - 6];
        big.extend_from_slice("çnota fiscal".as_bytes());
        std::fs::write(d.join("c.txt"), &big).unwrap();
        assert!(file_contains(&d.join("c.txt"), &needle, &no).unwrap());
        assert!(file_contains(&d.join("c.txt"), &fold_needle("aÇnota"), &no).unwrap());
        // Binary: skipped.
        std::fs::write(d.join("d.bin"), b"nota fiscal\0\0\0").unwrap();
        assert!(!file_contains(&d.join("d.bin"), &needle, &no).unwrap());
    }
}
