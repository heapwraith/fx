//! The installer package (task T4.3), pure: `fx-setup.exe` is the setup
//! program followed by a payload and a footer:
//!
//! ```text
//! [setup program][payload][offset u64][length u64]["FXSETUP1"]
//! payload = "FXPK0001" | version (u16 len + UTF-8) | count u32
//!           | count × (name u16 len + UTF-8, size u64, SHA-256 [32])
//!           | compressed size u64 | compressed blob (all files, in order)
//! ```
//!
//! Compression and hashing are done by the caller (Windows APIs in
//! fx-shell); this module only lays the bytes out and checks them.

const PAYLOAD_MAGIC: &[u8; 8] = b"FXPK0001";
const FOOTER_MAGIC: &[u8; 8] = b"FXSETUP1";
const FOOTER_LEN: usize = 24;
/// Sanity limits for a package read from disk.
const MAX_FILES: u32 = 256;
const MAX_TOTAL: u64 = 1 << 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// A plain file name (no folders).
    pub name: String,
    pub size: u64,
    pub sha256: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub version: String,
    pub files: Vec<FileEntry>,
}

impl Manifest {
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// Whether `name` is a plain file name, safe to join to the install folder.
pub fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':', '\0'])
        && !name.ends_with(['.', ' '])
}

/// The payload bytes for `manifest` and the compressed concatenation of its
/// files (`blob`).
pub fn encode(manifest: &Manifest, blob: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(blob.len() + 1024);
    out.extend_from_slice(PAYLOAD_MAGIC);
    put_str(&mut out, &manifest.version);
    out.extend_from_slice(&(manifest.files.len() as u32).to_le_bytes());
    for f in &manifest.files {
        put_str(&mut out, &f.name);
        out.extend_from_slice(&f.size.to_le_bytes());
        out.extend_from_slice(&f.sha256);
    }
    out.extend_from_slice(&(blob.len() as u64).to_le_bytes());
    out.extend_from_slice(blob);
    out
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u16).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// Reads a payload: its manifest and the compressed blob.
pub fn decode(payload: &[u8]) -> Result<(Manifest, &[u8]), String> {
    let mut r = Reader { b: payload, i: 0 };
    if r.take(8)? != PAYLOAD_MAGIC {
        return Err("not an fx package".into());
    }
    let version = r.string()?;
    let count = r.u32()?;
    if count > MAX_FILES {
        return Err("too many files".into());
    }
    let mut files = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let name = r.string()?;
        if !is_plain_name(&name) {
            return Err(format!("unsafe file name: {name:?}"));
        }
        let size = r.u64()?;
        let mut sha256 = [0u8; 32];
        sha256.copy_from_slice(r.take(32)?);
        files.push(FileEntry { name, size, sha256 });
    }
    let manifest = Manifest { version, files };
    if manifest.total_size() > MAX_TOTAL {
        return Err("package too large".into());
    }
    let len = r.u64()? as usize;
    let blob = r.take(len)?;
    if r.i != payload.len() {
        return Err("trailing bytes in the package".into());
    }
    Ok((manifest, blob))
}

/// `stub` (the setup program) with `payload` and the footer appended.
pub fn attach(stub: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(stub.len() + payload.len() + FOOTER_LEN);
    out.extend_from_slice(stub);
    out.extend_from_slice(payload);
    out.extend_from_slice(&(stub.len() as u64).to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(FOOTER_MAGIC);
    out
}

/// Splits a setup file into the program and the payload; `None` for a
/// setup program without a package (the installed uninstaller).
pub fn detach(file: &[u8]) -> Option<(&[u8], &[u8])> {
    let n = file.len();
    if n < FOOTER_LEN || &file[n - 8..] != FOOTER_MAGIC {
        return None;
    }
    let num = |at: usize| u64::from_le_bytes(file[at..at + 8].try_into().unwrap_or([0; 8]));
    let (offset, len) = (
        num(n - FOOTER_LEN) as usize,
        num(n - FOOTER_LEN + 8) as usize,
    );
    if offset.checked_add(len)? != n - FOOTER_LEN {
        return None;
    }
    Some((&file[..offset], &file[offset..offset + len]))
}

/// Splits the decompressed blob back into files, checking sizes and hashes
/// with `hash`. Returns `(name, bytes)` in package order.
pub fn split<'a>(
    manifest: &'a Manifest,
    data: &'a [u8],
    hash: impl Fn(&[u8]) -> [u8; 32],
) -> Result<Vec<(&'a str, &'a [u8])>, String> {
    if data.len() as u64 != manifest.total_size() {
        return Err("package size does not match its list".into());
    }
    let mut at = 0usize;
    let mut out = Vec::with_capacity(manifest.files.len());
    for f in &manifest.files {
        let bytes = &data[at..at + f.size as usize];
        if hash(bytes) != f.sha256 {
            return Err(format!("{} is damaged", f.name));
        }
        out.push((f.name.as_str(), bytes));
        at += f.size as usize;
    }
    Ok(out)
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.i.checked_add(n).filter(|&e| e <= self.b.len());
        let end = end.ok_or_else(|| "package cut short".to_string())?;
        let s = &self.b[self.i..end];
        self.i = end;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().unwrap_or([0; 2]),
        ))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().unwrap_or([0; 4]),
        ))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().unwrap_or([0; 8]),
        ))
    }
    fn string(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "bad text in the package".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A toy hash for the tests (the real one is SHA-256 from fx-shell).
    fn h(b: &[u8]) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (k, x) in b.iter().enumerate() {
            out[k % 32] = out[k % 32].wrapping_mul(31).wrapping_add(*x);
        }
        out[31] ^= b.len() as u8;
        out
    }

    fn sample() -> (Manifest, Vec<u8>) {
        let files: [(&str, &[u8]); 3] =
            [("fx.exe", b"MZ fx"), ("LEIA-ME.md", b"ola"), ("vazio", b"")];
        let data: Vec<u8> = files.iter().flat_map(|(_, b)| b.to_vec()).collect();
        let manifest = Manifest {
            version: "0.3.0-beta+abc".into(),
            files: files
                .iter()
                .map(|(n, b)| FileEntry {
                    name: (*n).into(),
                    size: b.len() as u64,
                    sha256: h(b),
                })
                .collect(),
        };
        (manifest, data)
    }

    #[test]
    fn round_trip_through_a_setup_file() {
        let (m, data) = sample();
        // "Compression" is the identity here.
        let setup = attach(b"PROGRAM", &encode(&m, &data));
        let (stub, payload) = detach(&setup).unwrap();
        assert_eq!(stub, b"PROGRAM");
        let (m2, blob) = decode(payload).unwrap();
        assert_eq!(m2, m);
        let files = split(&m2, blob, h).unwrap();
        assert_eq!(files[0], ("fx.exe", &b"MZ fx"[..]));
        assert_eq!(files[2], ("vazio", &b""[..]));
        assert_eq!(detach(b"PROGRAM only"), None, "uninstaller: no package");
    }

    #[test]
    fn damage_is_caught() {
        let (m, mut data) = sample();
        data[1] ^= 0xFF;
        assert!(split(&m, &data, h).unwrap_err().contains("fx.exe"));
        let payload = encode(&m, &data);
        assert!(decode(&payload[..payload.len() - 1]).is_err(), "cut short");
        let mut extra = payload.clone();
        extra.push(0);
        assert!(decode(&extra).is_err(), "trailing bytes");
        let mut setup = attach(b"P", &payload);
        let n = setup.len();
        setup[n - 20] ^= 1; // offset field
        assert_eq!(detach(&setup), None);
    }

    #[test]
    fn only_plain_names() {
        assert!(is_plain_name("fx.exe"));
        for bad in ["", "..", "a\\b", "a/b", "C:x", "x.", "x "] {
            assert!(!is_plain_name(bad), "{bad:?}");
        }
        let m = Manifest {
            version: "1".into(),
            files: vec![FileEntry {
                name: "..\\evil.exe".into(),
                size: 0,
                sha256: [0; 32],
            }],
        };
        assert!(decode(&encode(&m, &[])).unwrap_err().contains("unsafe"));
    }
}
