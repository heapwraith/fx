//! A saved form of a job (T6.6, D-068): the reversal of an operation, kept
//! across sessions in `state.db` so "desfazer a limpeza de ontem" works. Exact:
//! a path is stored as its UTF-16 units on Windows (its bytes elsewhere), so a
//! name that is not valid Unicode comes back the same. Anything damaged is
//! refused (`None`), never guessed; the reversal itself still runs through the
//! engine, which never overwrites.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use crate::{Job, OpKind, Recycled, Resolution};

const MAGIC: &[u8; 4] = b"FXJ1";
/// Most items in a saved job, and units in a path: a damaged length can never
/// ask for gigabytes.
const MAX_ITEMS: usize = 1 << 20;
const MAX_UNITS: usize = 32_767;

impl Job {
    /// The job as bytes, for [`Job::decode`].
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        out.extend_from_slice(MAGIC);
        out.push(kind_code(self.kind));
        out.push(resolution_code(self.on_conflict));
        out.push(u8::from(self.confirmed));
        put_count(&mut out, self.sources.len());
        for s in &self.sources {
            put_os(&mut out, s.as_os_str());
        }
        put_os(&mut out, self.dest.as_os_str());
        put_os(&mut out, &self.name);
        put_count(&mut out, self.pairs.len());
        for (from, to) in &self.pairs {
            put_os(&mut out, from.as_os_str());
            put_os(&mut out, to.as_os_str());
        }
        put_count(&mut out, self.restore.len());
        for r in &self.restore {
            put_os(&mut out, r.original.as_os_str());
            put_os(&mut out, r.info.as_os_str());
            put_os(&mut out, r.data.as_os_str());
            out.extend_from_slice(&r.deleted_at.to_le_bytes());
        }
        out
    }

    /// A job from [`Job::encode`]'s bytes; `None` if they are not exactly that.
    pub fn decode(bytes: &[u8]) -> Option<Job> {
        let mut r = Reader { b: bytes, i: 0 };
        if r.take(4)? != MAGIC {
            return None;
        }
        let kind = kind_from(r.u8()?)?;
        let on_conflict = resolution_from(r.u8()?)?;
        let confirmed = match r.u8()? {
            0 => false,
            1 => true,
            _ => return None,
        };
        let n = r.count()?;
        let mut sources = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            sources.push(PathBuf::from(r.os()?));
        }
        let dest = PathBuf::from(r.os()?);
        let name = r.os()?;
        let n = r.count()?;
        let mut pairs = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            pairs.push((PathBuf::from(r.os()?), PathBuf::from(r.os()?)));
        }
        let n = r.count()?;
        let mut restore = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            restore.push(Recycled {
                original: PathBuf::from(r.os()?),
                info: PathBuf::from(r.os()?),
                data: PathBuf::from(r.os()?),
                deleted_at: r.u64()?,
            });
        }
        if r.i != bytes.len() {
            return None;
        }
        let mut job = Job::new(kind, sources, dest);
        job.on_conflict = on_conflict;
        job.name = name;
        job.pairs = pairs;
        job.restore = restore;
        job.confirmed = confirmed;
        Some(job)
    }
}

fn kind_code(k: OpKind) -> u8 {
    match k {
        OpKind::Copy => 0,
        OpKind::Move => 1,
        OpKind::MoveBack => 2,
        OpKind::Recycle => 3,
        OpKind::DeletePermanent => 4,
        OpKind::Rename => 5,
        OpKind::BatchRename => 6,
        OpKind::NewFolder => 7,
        OpKind::Restore => 8,
    }
}

fn kind_from(c: u8) -> Option<OpKind> {
    Some(match c {
        0 => OpKind::Copy,
        1 => OpKind::Move,
        2 => OpKind::MoveBack,
        3 => OpKind::Recycle,
        4 => OpKind::DeletePermanent,
        5 => OpKind::Rename,
        6 => OpKind::BatchRename,
        7 => OpKind::NewFolder,
        8 => OpKind::Restore,
        _ => return None,
    })
}

fn resolution_code(r: Resolution) -> u8 {
    match r {
        Resolution::KeepBoth => 0,
        Resolution::Replace => 1,
        Resolution::Skip => 2,
        Resolution::Ask => 3,
    }
}

fn resolution_from(c: u8) -> Option<Resolution> {
    Some(match c {
        0 => Resolution::KeepBoth,
        1 => Resolution::Replace,
        2 => Resolution::Skip,
        3 => Resolution::Ask,
        _ => return None,
    })
}

fn put_count(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u32).to_le_bytes());
}

#[cfg(windows)]
fn put_os(out: &mut Vec<u8>, s: &OsStr) {
    use std::os::windows::ffi::OsStrExt;
    let units: Vec<u16> = s.encode_wide().collect();
    put_count(out, units.len());
    for u in units {
        out.extend_from_slice(&u.to_le_bytes());
    }
}

#[cfg(not(windows))]
fn put_os(out: &mut Vec<u8>, s: &OsStr) {
    let bytes = s.as_encoded_bytes();
    put_count(out, bytes.len());
    out.extend_from_slice(bytes);
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.i.checked_add(n).filter(|&e| e <= self.b.len())?;
        let s = &self.b[self.i..end];
        self.i = end;
        Some(s)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|s| s[0])
    }

    fn u64(&mut self) -> Option<u64> {
        self.take(8)?.try_into().ok().map(u64::from_le_bytes)
    }

    fn count(&mut self) -> Option<usize> {
        let n = u32::from_le_bytes(self.take(4)?.try_into().ok()?) as usize;
        (n <= MAX_ITEMS).then_some(n)
    }

    #[cfg(windows)]
    fn os(&mut self) -> Option<OsString> {
        use std::os::windows::ffi::OsStringExt;
        let n = self.count()?;
        if n > MAX_UNITS {
            return None;
        }
        let (pairs, _) = self.take(n.checked_mul(2)?)?.as_chunks::<2>();
        let units: Vec<u16> = pairs.iter().map(|&c| u16::from_le_bytes(c)).collect();
        Some(OsString::from_wide(&units))
    }

    #[cfg(not(windows))]
    fn os(&mut self) -> Option<OsString> {
        let n = self.count()?;
        if n > MAX_UNITS * 3 {
            return None;
        }
        let bytes = self.take(n)?;
        std::str::from_utf8(bytes).ok().map(OsString::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(job: &Job) -> Job {
        let bytes = job.encode();
        let back = Job::decode(&bytes).expect("decodes");
        assert_eq!(back.encode(), bytes, "the same bytes again");
        back
    }

    #[test]
    fn every_kind_of_job_comes_back_the_same() {
        let a = PathBuf::from(r"C:\Fotos\praia ☀.jpg");
        let jobs = vec![
            Job::copy(vec![a.clone()], PathBuf::from(r"D:\Cópias")),
            Job::move_to(vec![a.clone()], PathBuf::from(r"D:\")).on_conflict(Resolution::Skip),
            Job::move_back(vec![(a.clone(), PathBuf::from(r"D:\praia ☀.jpg"))]),
            Job::recycle(vec![a.clone()]),
            Job::rename(a.clone(), "nova.jpg"),
            Job::batch_rename(vec![(a.clone(), "x.jpg".to_owned())]),
            Job::new_folder(PathBuf::from(r"C:\Fotos"), "Nova pasta".into()),
            Job::restore(vec![Recycled {
                original: a.clone(),
                info: PathBuf::from(r"C:\$Recycle.Bin\S-1\$IABC.jpg"),
                data: PathBuf::from(r"C:\$Recycle.Bin\S-1\$RABC.jpg"),
                deleted_at: 133_000_000_000_000_000,
            }])
            .confirm_system(),
        ];
        for job in &jobs {
            let back = round_trip(job);
            assert_eq!(back.kind(), job.kind());
        }
    }

    #[cfg(windows)]
    #[test]
    fn names_that_are_not_unicode_survive() {
        use std::os::windows::ffi::OsStringExt;
        let odd = PathBuf::from(OsString::from_wide(&[0x43, 0x3A, 0x5C, 0xD800, 0x61]));
        let job = Job::rename(odd.clone(), OsStr::new("b"));
        let back = round_trip(&job);
        assert_eq!(back.sources, vec![odd]);
    }

    #[test]
    fn damaged_bytes_are_refused() {
        let bytes = Job::recycle(vec![PathBuf::from(r"C:\a")]).encode();
        assert!(
            Job::decode(&bytes[..bytes.len() - 1]).is_none(),
            "cut short"
        );
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(Job::decode(&extra).is_none(), "trailing bytes");
        let mut kind = bytes.clone();
        kind[4] = 99;
        assert!(Job::decode(&kind).is_none(), "unknown kind");
        assert!(Job::decode(b"FXJ1").is_none());
        assert!(Job::decode(b"nope").is_none());
        // Random garbage never panics (xorshift, fixed seed).
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..3000 {
            let mut v = bytes.clone();
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let at = (x as usize) % v.len();
            v[at] = (x >> 32) as u8;
            v.truncate(((x >> 16) as usize) % (v.len() + 1));
            let _ = Job::decode(&v);
        }
    }
}
