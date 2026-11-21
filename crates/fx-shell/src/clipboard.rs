//! Files on the clipboard (task T1.14), in the format the Explorer uses:
//! `CF_HDROP` (a `DROPFILES` header followed by the paths) plus
//! "Preferred DropEffect" (copy or cut). Platform-neutral part: encoding and
//! decoding of the `DROPFILES` block.

use std::path::PathBuf;

/// Size of the `DROPFILES` header: pFiles, pt.x, pt.y, fNC, fWide.
const HEADER: usize = 20;

/// `DROPEFFECT_COPY` and `DROPEFFECT_MOVE` (the "cut" of Ctrl+X).
pub const EFFECT_COPY: u32 = 1;
pub const EFFECT_MOVE: u32 = 2;

/// Files on the clipboard and whether they were cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipFiles {
    pub paths: Vec<PathBuf>,
    pub cut: bool,
}

/// A `DROPFILES` block: header, then each path in UTF-16 with its NUL, then
/// one more NUL.
pub fn encode_drop_files(paths: &[PathBuf]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + paths.len() * 64);
    out.extend_from_slice(&(HEADER as u32).to_le_bytes()); // pFiles
    out.extend_from_slice(&0i32.to_le_bytes()); // pt.x
    out.extend_from_slice(&0i32.to_le_bytes()); // pt.y
    out.extend_from_slice(&0i32.to_le_bytes()); // fNC
    out.extend_from_slice(&1i32.to_le_bytes()); // fWide
    for p in paths {
        for unit in p.as_os_str().to_string_lossy().encode_utf16().chain([0]) {
            out.extend_from_slice(&unit.to_le_bytes());
        }
    }
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// Paths in a `DROPFILES` block (wide or ANSI), `None` if malformed.
pub fn decode_drop_files(bytes: &[u8]) -> Option<Vec<PathBuf>> {
    let u32_at = |at: usize| -> Option<u32> {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let start = u32_at(0)? as usize;
    let wide = u32_at(16)? != 0;
    let list = bytes.get(start..)?;
    let mut paths = Vec::new();
    if wide {
        let (units, _) = list.as_chunks::<2>();
        let units: Vec<u16> = units.iter().map(|&u| u16::from_le_bytes(u)).collect();
        for part in units.split(|&u| u == 0) {
            if part.is_empty() {
                break; // the closing double NUL
            }
            paths.push(PathBuf::from(String::from_utf16_lossy(part)));
        }
    } else {
        for part in list.split(|&b| b == 0) {
            if part.is_empty() {
                break;
            }
            paths.push(PathBuf::from(String::from_utf8_lossy(part).into_owned()));
        }
    }
    Some(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_files_round_trip() {
        let paths = vec![
            PathBuf::from(r"C:\Fotos\praia ☀.jpg"),
            PathBuf::from(r"D:\relatório final.pdf"),
        ];
        let block = encode_drop_files(&paths);
        assert_eq!(&block[..4], &20u32.to_le_bytes());
        assert_eq!(&block[block.len() - 4..], &[0, 0, 0, 0], "double NUL");
        assert_eq!(decode_drop_files(&block), Some(paths));
        assert_eq!(decode_drop_files(&encode_drop_files(&[])), Some(Vec::new()));
        assert_eq!(decode_drop_files(&[1, 2]), None);
    }

    #[test]
    fn ansi_blocks_from_old_apps_are_read() {
        let mut block = vec![0u8; 20];
        block[0] = 20;
        block.extend_from_slice(b"C:\\a.txt\0C:\\b.txt\0\0");
        assert_eq!(
            decode_drop_files(&block),
            Some(vec![PathBuf::from(r"C:\a.txt"), PathBuf::from(r"C:\b.txt")])
        );
    }
}
