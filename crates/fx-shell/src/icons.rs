//! File icons (task T1.6). Platform-neutral part: what an icon is cached by,
//! the request queue with a per-request time limit, and a tiny PNG encoder
//! (the UI hands PNG bytes to GPUI, which decodes them off the UI thread).
//!
//! Extraction runs on a worker thread (a COM STA thread on Windows). Shell
//! extensions are third-party code and can hang: if one request takes longer
//! than the time limit, the worker is abandoned (left blocked), the request
//! is answered with no icon and a fresh worker takes the next requests.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

/// What an icon is cached by. Most files share their extension's icon;
/// executables, shortcuts and icon files have their own.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IconKey {
    Folder,
    /// Lowercase extension without the dot ("" for no extension).
    Extension(String),
    File(PathBuf),
}

/// Extensions whose icon lives in the file itself.
const PER_FILE: [&str; 8] = ["exe", "lnk", "ico", "url", "scr", "cpl", "msc", "appref-ms"];

/// Cache key of an entry named `name` inside `dir`.
pub fn icon_key(dir: &Path, name: &str, is_dir: bool) -> IconKey {
    if is_dir {
        return IconKey::Folder;
    }
    let ext = match name.rfind('.') {
        Some(i) if i > 0 => name[i + 1..].to_lowercase(),
        _ => String::new(),
    };
    if PER_FILE.contains(&ext.as_str()) {
        IconKey::File(dir.join(name))
    } else {
        IconKey::Extension(ext)
    }
}

/// An answer: PNG bytes, or `None` when the icon could not be made in time.
#[derive(Debug, Clone)]
pub struct IconReady {
    pub key: IconKey,
    pub png: Option<Arc<[u8]>>,
}

/// Turns a key into PNG bytes (runs on the worker thread).
pub type Extractor = Arc<dyn Fn(&IconKey) -> Option<Vec<u8>> + Send + Sync>;
/// Runs once at the start of each worker thread (COM initialization).
pub type WorkerInit = fn();

/// Default time limit of one request.
pub const TIME_LIMIT: Duration = Duration::from_secs(2);

/// Queue of icon requests; answers arrive on the channel given to `start`.
#[derive(Debug, Clone)]
pub struct IconService {
    tx: Sender<IconKey>,
}

impl IconService {
    /// Asks for an icon; the answer comes later on the replies channel.
    pub fn request(&self, key: IconKey) {
        let _ = self.tx.send(key);
    }

    /// A service with any extractor (tests, and the Windows one in `start`).
    pub fn with_extractor(
        extract: Extractor,
        init: WorkerInit,
        limit: Duration,
        replies: Sender<IconReady>,
    ) -> std::io::Result<Self> {
        let (tx, rx) = channel::<IconKey>();
        std::thread::Builder::new()
            .name("fx-icons".into())
            .spawn(move || dispatch(&rx, &extract, init, limit, &replies))?;
        Ok(Self { tx })
    }
}

struct Worker {
    tx: Sender<IconKey>,
    rx: Receiver<Option<Vec<u8>>>,
}

fn spawn_worker(extract: &Extractor, init: WorkerInit) -> Option<Worker> {
    let (req_tx, req_rx) = channel::<IconKey>();
    let (res_tx, res_rx) = channel();
    let extract = extract.clone();
    std::thread::Builder::new()
        .name("fx-icons-sta".into())
        .spawn(move || {
            init();
            // Ends when the dispatcher drops the sender (or abandoned us).
            for key in req_rx {
                if res_tx.send(extract(&key)).is_err() {
                    return;
                }
            }
        })
        .ok()?;
    Some(Worker {
        tx: req_tx,
        rx: res_rx,
    })
}

/// Hands requests to the worker one at a time, waiting at most `limit` each.
fn dispatch(
    rx: &Receiver<IconKey>,
    extract: &Extractor,
    init: WorkerInit,
    limit: Duration,
    replies: &Sender<IconReady>,
) {
    let mut worker = spawn_worker(extract, init);
    for key in rx {
        let png = match &worker {
            Some(w) if w.tx.send(key.clone()).is_ok() => match w.rx.recv_timeout(limit) {
                Ok(png) => png,
                Err(_) => {
                    // Stuck (or died): leave it and start over with a new one.
                    worker = spawn_worker(extract, init);
                    None
                }
            },
            _ => {
                worker = spawn_worker(extract, init);
                None
            }
        };
        let ready = IconReady {
            key,
            png: png.map(Arc::from),
        };
        if replies.send(ready).is_err() {
            return;
        }
    }
}

/// Encodes straight-alpha RGBA pixels as PNG (stored deflate blocks: icons
/// are tiny, so compression is not worth the code).
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    debug_assert_eq!(rgba.len(), (width * height * 4) as usize);
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for row in rgba.chunks(width as usize * 4) {
        raw.push(0); // filter: none
        raw.extend_from_slice(row);
    }
    let mut zlib = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    if blocks.peek().is_none() {
        zlib.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(block) = blocks.next() {
        let len = block.len() as u16;
        zlib.push(u8::from(blocks.peek().is_none()));
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    let mut png = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
    for (kind, data) in [(b"IHDR", &ihdr), (b"IDAT", &zlib), (b"IEND", &Vec::new())] {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = png.len();
        png.extend_from_slice(kind);
        png.extend_from_slice(data);
        let crc = crc32(&png[start..]);
        png.extend_from_slice(&crc.to_be_bytes());
    }
    png
}

/// Encodes straight-alpha BGRA pixels (top-down rows) as a 32-bit BMP with
/// an alpha mask (BITMAPV4HEADER, `BI_BITFIELDS`). For large Quick Look
/// images (T1.22): no checksums or compression, so building it costs about a
/// copy of the pixels.
pub fn encode_bmp(width: u32, height: u32, bgra: &[u8]) -> Vec<u8> {
    debug_assert_eq!(bgra.len(), (width * height * 4) as usize);
    const FILE_HEADER: u32 = 14;
    const V4_HEADER: u32 = 108;
    let offset = FILE_HEADER + V4_HEADER;
    let mut bmp = Vec::with_capacity(offset as usize + bgra.len());
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(offset + bgra.len() as u32).to_le_bytes());
    bmp.extend_from_slice(&[0; 4]); // reserved
    bmp.extend_from_slice(&offset.to_le_bytes());
    bmp.extend_from_slice(&V4_HEADER.to_le_bytes());
    bmp.extend_from_slice(&(width as i32).to_le_bytes());
    bmp.extend_from_slice(&(-(height as i32)).to_le_bytes()); // top-down
    bmp.extend_from_slice(&1u16.to_le_bytes()); // planes
    bmp.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    bmp.extend_from_slice(&3u32.to_le_bytes()); // BI_BITFIELDS
    bmp.extend_from_slice(&(bgra.len() as u32).to_le_bytes());
    bmp.extend_from_slice(&[0; 8]); // resolution
    bmp.extend_from_slice(&[0; 8]); // palette
    for mask in [0x00FF_0000u32, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000] {
        bmp.extend_from_slice(&mask.to_le_bytes()); // R, G, B, A
    }
    bmp.extend_from_slice(b"BGRs"); // LCS_sRGB, stored little-endian
    bmp.extend_from_slice(&[0; 36 + 12]); // endpoints, gamma
    debug_assert_eq!(bmp.len(), offset as usize);
    bmp.extend_from_slice(bgra);
    bmp
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &x in data {
        crc ^= u32::from(x);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn bmp_header_describes_the_pixels() {
        let bgra = [1u8, 2, 3, 255, 4, 5, 6, 128];
        let bmp = encode_bmp(2, 1, &bgra);
        assert_eq!(&bmp[..2], b"BM");
        let u32_at = |i: usize| u32::from_le_bytes(bmp[i..i + 4].try_into().unwrap());
        assert_eq!(u32_at(2) as usize, bmp.len());
        assert_eq!(u32_at(10), 122); // pixels start
        assert_eq!(u32_at(18), 2); // width
        assert_eq!(u32_at(22) as i32, -1); // top-down
        assert_eq!(u32_at(30), 3); // BI_BITFIELDS
        assert_eq!(u32_at(66), 0xFF00_0000); // alpha mask
        assert_eq!(&bmp[122..], &bgra);
    }

    #[test]
    fn keys_share_by_extension_except_per_file_icons() {
        let d = Path::new(r"C:\x");
        assert_eq!(
            icon_key(d, "a.TXT", false),
            IconKey::Extension("txt".into())
        );
        assert_eq!(
            icon_key(d, "b.txt", false),
            IconKey::Extension("txt".into())
        );
        assert_eq!(
            icon_key(d, "README", false),
            IconKey::Extension(String::new())
        );
        assert_eq!(
            icon_key(d, ".gitignore", false),
            IconKey::Extension(String::new())
        );
        assert_eq!(icon_key(d, "src", true), IconKey::Folder);
        assert_eq!(
            icon_key(d, "app.EXE", false),
            IconKey::File(d.join("app.EXE"))
        );
        assert_eq!(
            icon_key(d, "Atalho.lnk", false),
            IconKey::File(d.join("Atalho.lnk"))
        );
    }

    #[test]
    fn png_has_valid_structure() {
        let png = encode_png(2, 1, &[255, 0, 0, 255, 0, 0, 255, 128]);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
        assert_eq!(crc32(b"IEND"), 0xAE42_6082, "known CRC of an empty IEND");
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    fn no_init() {}

    /// A hung shell extension costs one request (answered empty after the
    /// time limit); the next requests are served by a fresh worker.
    #[test]
    fn a_hung_extraction_is_abandoned_after_the_time_limit() {
        let extract: Extractor = Arc::new(|key| match key {
            IconKey::Extension(e) if e == "hang" => loop {
                std::thread::sleep(Duration::from_secs(60));
            },
            IconKey::Extension(e) => Some(e.as_bytes().to_vec()),
            _ => None,
        });
        let (tx, rx) = channel();
        let limit = Duration::from_millis(200);
        let service = IconService::with_extractor(extract, no_init, limit, tx).unwrap();
        let started = Instant::now();
        for e in ["txt", "hang", "pdf"] {
            service.request(IconKey::Extension(e.into()));
        }
        let answers: Vec<IconReady> = (0..3)
            .map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap())
            .collect();
        assert_eq!(answers[0].png.as_deref(), Some(&b"txt"[..]));
        assert_eq!(answers[1].key, IconKey::Extension("hang".into()));
        assert!(answers[1].png.is_none(), "hung request answered empty");
        assert_eq!(
            answers[2].png.as_deref(),
            Some(&b"pdf"[..]),
            "next one served"
        );
        let took = started.elapsed();
        assert!(took >= limit && took < Duration::from_secs(2), "{took:?}");
    }
}
