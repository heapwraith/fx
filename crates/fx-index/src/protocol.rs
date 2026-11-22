//! Messages between the app and `fx-indexd` over its named pipe (T2.5).
//!
//! The service runs as SYSTEM and only parses this tiny format, with fixed
//! limits, so a malformed or hostile request cannot make it allocate much or
//! read past the message. All integers are little-endian.
//!
//! Request:  `FXQ1` · limit u32 · query length u32 · query (UTF-8)
//! Response: total length u32 · `FXR1` · status u8 · items u64 · index bytes u64
//!           · count u32 · hits
//! Hit:      flags u8 (bit 0: folder) · size u64 · modified u64 (FILETIME)
//!           · path length u32 (UTF-16 units) · path (UTF-16LE)

use std::io::{self, Read, Write};

const REQUEST_MAGIC: &[u8; 4] = b"FXQ1";
const RESPONSE_MAGIC: &[u8; 4] = b"FXR1";
/// Longest query accepted, in bytes.
pub const MAX_QUERY: usize = 1024;
/// Most hits a response carries.
pub const MAX_HITS: u32 = 1000;
/// Longest path in a hit (the Windows limit), in UTF-16 units.
const MAX_PATH_UNITS: u32 = 32_767;
/// Largest response accepted by the client.
const MAX_RESPONSE: u32 = 64 * 1024 * 1024;

/// A search for `query`, at most `limit` hits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub query: String,
    pub limit: u32,
}

/// Whether the service could answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ready,
    /// Still reading the volumes: no results yet.
    Indexing,
}

/// One result: an absolute path (`C:\...`) as UTF-16 units, so any Windows
/// name round-trips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteHit {
    pub path: Vec<u16>,
    pub is_dir: bool,
    pub size: u64,
    /// FILETIME ticks.
    pub modified: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: Status,
    /// Entries in the index (all volumes).
    pub items: u64,
    /// Memory held by the indexes, in bytes (P9).
    pub index_bytes: u64,
    pub hits: Vec<RemoteHit>,
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("fx-indexd protocol: {what}"),
    )
}

fn read_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(r: &mut impl Read) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

impl Request {
    pub fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        let query = self.query.as_bytes();
        if query.len() > MAX_QUERY {
            return Err(invalid("query too long"));
        }
        let mut buf = Vec::with_capacity(12 + query.len());
        buf.extend_from_slice(REQUEST_MAGIC);
        buf.extend_from_slice(&self.limit.min(MAX_HITS).to_le_bytes());
        buf.extend_from_slice(&(query.len() as u32).to_le_bytes());
        buf.extend_from_slice(query);
        w.write_all(&buf)?;
        w.flush()
    }

    /// Reads one request, rejecting anything outside the limits before
    /// allocating for it.
    pub fn read_from(r: &mut impl Read) -> io::Result<Self> {
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;
        if &magic != REQUEST_MAGIC {
            return Err(invalid("bad request"));
        }
        let limit = read_u32(r)?.min(MAX_HITS);
        let len = read_u32(r)? as usize;
        if len > MAX_QUERY {
            return Err(invalid("query too long"));
        }
        let mut query = vec![0u8; len];
        r.read_exact(&mut query)?;
        let query = String::from_utf8(query).map_err(|_| invalid("query is not UTF-8"))?;
        Ok(Self { query, limit })
    }
}

impl Response {
    pub fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        let hits = &self.hits[..self.hits.len().min(MAX_HITS as usize)];
        let mut body = Vec::with_capacity(16 + hits.len() * 96);
        body.extend_from_slice(RESPONSE_MAGIC);
        body.push(match self.status {
            Status::Ready => 0,
            Status::Indexing => 1,
        });
        body.extend_from_slice(&self.items.to_le_bytes());
        body.extend_from_slice(&self.index_bytes.to_le_bytes());
        body.extend_from_slice(&(hits.len() as u32).to_le_bytes());
        for h in hits {
            let path = &h.path[..h.path.len().min(MAX_PATH_UNITS as usize)];
            body.push(u8::from(h.is_dir));
            body.extend_from_slice(&h.size.to_le_bytes());
            body.extend_from_slice(&h.modified.to_le_bytes());
            body.extend_from_slice(&(path.len() as u32).to_le_bytes());
            for u in path {
                body.extend_from_slice(&u.to_le_bytes());
            }
        }
        w.write_all(&(body.len() as u32).to_le_bytes())?;
        w.write_all(&body)?;
        w.flush()
    }

    pub fn read_from(r: &mut impl Read) -> io::Result<Self> {
        let total = read_u32(r)?;
        if total > MAX_RESPONSE {
            return Err(invalid("response too large"));
        }
        let mut body = vec![0u8; total as usize];
        r.read_exact(&mut body)?;
        let mut b = body.as_slice();
        let mut magic = [0u8; 4];
        b.read_exact(&mut magic)?;
        if &magic != RESPONSE_MAGIC {
            return Err(invalid("bad response"));
        }
        let mut status = [0u8; 1];
        b.read_exact(&mut status)?;
        let status = match status[0] {
            0 => Status::Ready,
            1 => Status::Indexing,
            _ => return Err(invalid("bad status")),
        };
        let items = read_u64(&mut b)?;
        let index_bytes = read_u64(&mut b)?;
        let count = read_u32(&mut b)?;
        if count > MAX_HITS {
            return Err(invalid("too many hits"));
        }
        let mut hits = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let mut flags = [0u8; 1];
            b.read_exact(&mut flags)?;
            let size = read_u64(&mut b)?;
            let modified = read_u64(&mut b)?;
            let units = read_u32(&mut b)?;
            if units > MAX_PATH_UNITS || units as usize * 2 > b.len() {
                return Err(invalid("bad path"));
            }
            let mut path = Vec::with_capacity(units as usize);
            for _ in 0..units {
                let mut u = [0u8; 2];
                b.read_exact(&mut u)?;
                path.push(u16::from_le_bytes(u));
            }
            hits.push(RemoteHit {
                path,
                is_dir: flags[0] & 1 != 0,
                size,
                modified,
            });
        }
        Ok(Self {
            status,
            items,
            index_bytes,
            hits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(path: &str, is_dir: bool) -> RemoteHit {
        RemoteHit {
            path: path.encode_utf16().collect(),
            is_dir,
            size: 1 << 33,
            modified: 133_000_000_000_000_000,
        }
    }

    #[test]
    fn request_and_response_round_trip() {
        let req = Request {
            query: "relatório 2026".into(),
            limit: 50,
        };
        let mut buf = Vec::new();
        req.write_to(&mut buf).unwrap();
        assert_eq!(Request::read_from(&mut buf.as_slice()).unwrap(), req);

        let mut odd = hit(r"C:\x", false);
        odd.path.push(0xD800); // lone surrogate: still a valid Windows name
        let resp = Response {
            status: Status::Ready,
            items: 763_000,
            index_bytes: 54 << 20,
            hits: vec![hit(r"C:\Users\ana\日本語", true), odd],
        };
        let mut buf = Vec::new();
        resp.write_to(&mut buf).unwrap();
        assert_eq!(Response::read_from(&mut buf.as_slice()).unwrap(), resp);
    }

    #[test]
    fn limits_are_enforced_on_both_sides() {
        let long = Request {
            query: "x".repeat(MAX_QUERY + 1),
            limit: 1,
        };
        assert!(long.write_to(&mut Vec::new()).is_err());
        // A hand-made request claiming a huge query is refused before reading it.
        let mut evil = Vec::new();
        evil.extend_from_slice(REQUEST_MAGIC);
        evil.extend_from_slice(&5u32.to_le_bytes());
        evil.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(Request::read_from(&mut evil.as_slice()).is_err());
        // A limit above the maximum is clamped.
        let mut buf = Vec::new();
        buf.extend_from_slice(REQUEST_MAGIC);
        buf.extend_from_slice(&u32::MAX.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.push(b'a');
        assert_eq!(
            Request::read_from(&mut buf.as_slice()).unwrap().limit,
            MAX_HITS
        );
    }

    #[test]
    fn malformed_messages_are_errors_not_panics() {
        assert!(Request::read_from(&mut &b"XXXX"[..]).is_err());
        assert!(Request::read_from(&mut &b""[..]).is_err());
        // Response whose hit claims more path than the message holds.
        let mut body = Vec::new();
        body.extend_from_slice(RESPONSE_MAGIC);
        body.push(0);
        body.extend_from_slice(&[0; 16]);
        body.extend_from_slice(&1u32.to_le_bytes());
        body.push(0);
        body.extend_from_slice(&[0; 16]);
        body.extend_from_slice(&1000u32.to_le_bytes());
        let mut msg = (body.len() as u32).to_le_bytes().to_vec();
        msg.extend_from_slice(&body);
        assert!(Response::read_from(&mut msg.as_slice()).is_err());
        // Bad status.
        let mut body = RESPONSE_MAGIC.to_vec();
        body.push(9);
        body.extend_from_slice(&[0; 16]);
        body.extend_from_slice(&0u32.to_le_bytes());
        let mut msg = (body.len() as u32).to_le_bytes().to_vec();
        msg.extend_from_slice(&body);
        assert!(Response::read_from(&mut msg.as_slice()).is_err());
    }
}
