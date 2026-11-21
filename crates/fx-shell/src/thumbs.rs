//! Thumbnails for the grid view (task T1.7). Platform-neutral part: a queue
//! that always follows what is on screen.
//!
//! The UI calls [`ThumbService::want`] with the items it shows; that replaces
//! the queue, so items scrolled away are dropped before they are extracted
//! (cancellation). Each item goes through two passes: first the system cache
//! only (fast), then, after every visible item had its cache pass, a real
//! extraction. Extraction runs on a low-priority worker with a time limit per
//! item; a worker stuck in a thumbnail handler is abandoned like in
//! `crate::icons`.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

/// Thumbnails are at most this many pixels wide and tall.
pub const THUMB_SIZE: u32 = 96;
/// Time limit of one extraction (cache lookups use the same limit).
pub const THUMB_TIME_LIMIT: Duration = Duration::from_secs(5);

/// A final answer for one item: PNG bytes, or `None` (no thumbnail).
#[derive(Debug, Clone)]
pub struct ThumbReady {
    pub path: PathBuf,
    pub png: Option<Arc<[u8]>>,
}

/// `(path, cache_only)` -> PNG bytes; runs on the worker thread.
pub type ThumbExtractor = Arc<dyn Fn(&Path, bool) -> Option<Vec<u8>> + Send + Sync>;

#[derive(Debug, Default)]
struct Queue {
    cache: VecDeque<PathBuf>,
    extract: VecDeque<PathBuf>,
    /// Had their cache pass.
    tried: HashSet<PathBuf>,
    /// Got a final answer.
    done: HashSet<PathBuf>,
    current: Option<PathBuf>,
    closed: bool,
}

#[derive(Debug, Default)]
struct Shared {
    queue: Mutex<Queue>,
    changed: Condvar,
}

/// Handle to the thumbnail worker. Dropping every handle stops it.
#[derive(Debug)]
pub struct ThumbService {
    shared: Arc<Shared>,
}

impl Drop for ThumbService {
    fn drop(&mut self) {
        self.lock().closed = true;
        self.shared.changed.notify_all();
    }
}

impl ThumbService {
    pub fn with_extractor(
        extract: ThumbExtractor,
        init: fn(),
        limit: Duration,
        replies: Sender<ThumbReady>,
    ) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let worker_shared = shared.clone();
        std::thread::Builder::new()
            .name("fx-thumbs".into())
            .spawn(move || dispatch(&worker_shared, &extract, init, limit, &replies))?;
        Ok(Self { shared })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.shared
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The items on screen now, in priority order. Replaces the previous
    /// request: anything not listed and not started yet is cancelled.
    pub fn want(&self, paths: Vec<PathBuf>) {
        let mut q = self.lock();
        let busy = q.current.clone();
        let pending: Vec<PathBuf> = paths
            .into_iter()
            .filter(|p| !q.done.contains(p) && busy.as_ref() != Some(p))
            .collect();
        let (extract, cache): (Vec<PathBuf>, Vec<PathBuf>) =
            pending.into_iter().partition(|p| q.tried.contains(p));
        q.cache = cache.into();
        q.extract = extract.into();
        drop(q);
        self.shared.changed.notify_all();
    }
}

impl ThumbService {
    /// The UI dropped these thumbnails (cache limit): they may be asked again.
    pub fn forget(&self, paths: &[PathBuf]) {
        let mut q = self.lock();
        for p in paths {
            q.done.remove(p);
            q.tried.remove(p);
        }
    }
}

struct Worker {
    tx: Sender<(PathBuf, bool)>,
    rx: Receiver<Option<Vec<u8>>>,
}

fn spawn_worker(extract: &ThumbExtractor, init: fn()) -> Option<Worker> {
    let (req_tx, req_rx) = channel::<(PathBuf, bool)>();
    let (res_tx, res_rx) = channel();
    let extract = extract.clone();
    std::thread::Builder::new()
        .name("fx-thumbs-sta".into())
        .spawn(move || {
            init();
            for (path, cache_only) in req_rx {
                if res_tx.send(extract(&path, cache_only)).is_err() {
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

fn dispatch(
    shared: &Shared,
    extract: &ThumbExtractor,
    init: fn(),
    limit: Duration,
    replies: &Sender<ThumbReady>,
) {
    let lock = || shared.queue.lock().unwrap_or_else(PoisonError::into_inner);
    let mut worker = spawn_worker(extract, init);
    loop {
        // Next item: cache passes first, then extractions.
        let (path, cache_only) = {
            let mut q = lock();
            loop {
                if q.closed {
                    return;
                }
                if let Some(p) = q.cache.pop_front() {
                    break (p, true);
                }
                if let Some(p) = q.extract.pop_front() {
                    break (p, false);
                }
                q = shared
                    .changed
                    .wait(q)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };
        lock().current = Some(path.clone());
        let mut timed_out = false;
        let png = match &worker {
            Some(w) if w.tx.send((path.clone(), cache_only)).is_ok() => {
                match w.rx.recv_timeout(limit) {
                    Ok(png) => png,
                    Err(_) => {
                        timed_out = true;
                        worker = spawn_worker(extract, init);
                        None
                    }
                }
            }
            _ => {
                worker = spawn_worker(extract, init);
                None
            }
        };
        let mut q = lock();
        q.current = None;
        // A hung item is not retried: it would block the worker again.
        if cache_only && png.is_none() && !timed_out {
            // Not cached: extract it later, if it is still wanted (want() puts
            // it back in `extract` on the next call otherwise).
            q.tried.insert(path.clone());
            q.extract.push_back(path);
            continue;
        }
        q.done.insert(path.clone());
        drop(q);
        let ready = ThumbReady {
            path,
            png: png.map(Arc::from),
        };
        if replies.send(ready).is_err() {
            return;
        }
    }
}

/// Shrinks RGBA pixels to fit in `max`×`max` keeping the aspect ratio (box
/// filter: each output pixel averages the input pixels it covers). Images
/// already small enough are returned as they are.
pub fn fit_rgba(width: u32, height: u32, rgba: Vec<u8>, max: u32) -> (u32, u32, Vec<u8>) {
    if width <= max && height <= max || width == 0 || height == 0 {
        return (width, height, rgba);
    }
    let scale = f64::from(max) / f64::from(width.max(height));
    let ow = ((f64::from(width) * scale).round() as u32).max(1);
    let oh = ((f64::from(height) * scale).round() as u32).max(1);
    let mut out = vec![0u8; (ow * oh * 4) as usize];
    for oy in 0..oh {
        let y0 = oy * height / oh;
        let y1 = ((oy + 1) * height / oh).max(y0 + 1);
        for ox in 0..ow {
            let x0 = ox * width / ow;
            let x1 = ((ox + 1) * width / ow).max(x0 + 1);
            let mut sum = [0u32; 4];
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = ((y * width + x) * 4) as usize;
                    for c in 0..4 {
                        sum[c] += u32::from(rgba[i + c]);
                    }
                }
            }
            let n = (y1 - y0) * (x1 - x0);
            let o = ((oy * ow + ox) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (sum[c] / n) as u8;
            }
        }
    }
    (ow, oh, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn no_init() {}

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    /// Cached items come back first; the rest are extracted afterwards.
    #[test]
    fn cache_pass_then_extraction() {
        let extract: ThumbExtractor = Arc::new(|path: &Path, cache_only| {
            let cached = path.to_string_lossy().contains("cached");
            (cached || !cache_only).then(|| path.to_string_lossy().as_bytes().to_vec())
        });
        let (tx, rx) = channel();
        let s = ThumbService::with_extractor(extract, no_init, Duration::from_secs(1), tx).unwrap();
        s.want(vec![p("a"), p("cached-b"), p("c")]);
        let order: Vec<PathBuf> = (0..3)
            .map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap().path)
            .collect();
        assert_eq!(order, [p("cached-b"), p("a"), p("c")]);
    }

    /// Items no longer wanted are never extracted.
    #[test]
    fn want_cancels_items_scrolled_away() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let extract: ThumbExtractor = Arc::new(move |path: &Path, _| {
            seen.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(50));
            Some(path.to_string_lossy().as_bytes().to_vec())
        });
        let (tx, rx) = channel();
        let s = ThumbService::with_extractor(extract, no_init, Duration::from_secs(1), tx).unwrap();
        let first: Vec<PathBuf> = (0..50).map(|i| p(&format!("old-{i}"))).collect();
        s.want(first);
        std::thread::sleep(Duration::from_millis(20));
        s.want(vec![p("new-1"), p("new-2")]);
        let mut got = Vec::new();
        while let Ok(r) = rx.recv_timeout(Duration::from_millis(500)) {
            got.push(r.path);
        }
        assert!(got.contains(&p("new-1")) && got.contains(&p("new-2")));
        let old = got.iter().filter(|g| g.starts_with("old")).count();
        assert!(old <= 1, "at most the one in flight: {got:?}");
        assert!(calls.load(Ordering::SeqCst) <= 4);
    }

    /// A stuck thumbnail handler costs one item; the next ones still come.
    #[test]
    fn a_stuck_extraction_is_abandoned() {
        let extract: ThumbExtractor = Arc::new(|path: &Path, _| {
            if path == Path::new("hang") {
                loop {
                    std::thread::sleep(Duration::from_secs(60));
                }
            }
            Some(vec![1])
        });
        let (tx, rx) = channel();
        let limit = Duration::from_millis(200);
        let s = ThumbService::with_extractor(extract, no_init, limit, tx).unwrap();
        s.want(vec![p("hang"), p("ok")]);
        let a = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let b = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!((a.path, a.png.is_none()), (p("hang"), true));
        assert_eq!((b.path, b.png.is_some()), (p("ok"), true));
    }

    #[test]
    fn fit_keeps_aspect_and_averages() {
        let px = |v: u8| [v, v, v, 255];
        // 4x2 -> fits 2x1: each output pixel averages a 2x2 block.
        let mut img = Vec::new();
        for v in [0, 100, 200, 250, 0, 100, 200, 250] {
            img.extend_from_slice(&px(v));
        }
        let (w, h, out) = fit_rgba(4, 2, img.clone(), 2);
        assert_eq!((w, h), (2, 1));
        assert_eq!(&out[..4], &px(50));
        assert_eq!(&out[4..], &px(225));
        let (w, h, same) = fit_rgba(4, 2, img.clone(), 96);
        assert_eq!((w, h, same), (4, 2, img));
        assert_eq!(fit_rgba(256, 128, vec![0; 256 * 128 * 4], 96).0, 96);
    }
}
