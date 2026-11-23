//! The service's work (T2.5): build the index of each NTFS volume, keep it
//! current from the USN journal, answer searches.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use fx_index::protocol::{RemoteHit, Request, Response, Status};
use fx_index::{Index, JournalWatcher};

/// How often the USN journals are read (a created file shows up in search
/// within about this long).
const WATCH_EVERY: Duration = Duration::from_millis(250);

struct Volume {
    /// `C:`
    letter: String,
    index: Index,
}

pub struct Engine {
    volumes: RwLock<Vec<Volume>>,
    ready: AtomicBool,
    /// Lowercase profile folders of the other users, with a trailing `\`.
    hidden: Vec<String>,
    /// The latest changes on the disks (T6.2).
    recent: std::sync::Mutex<fx_index::recent::Recent>,
}

impl Engine {
    pub fn new(user_sid: &str) -> Self {
        let hidden = fx_index::system::profiles()
            .into_iter()
            .filter(|(sid, _)| sid != user_sid)
            .map(|(_, path)| prefix(&path))
            .collect();
        Self::with_hidden(hidden)
    }

    fn with_hidden(hidden: Vec<String>) -> Self {
        Self {
            volumes: RwLock::new(Vec::new()),
            ready: AtomicBool::new(false),
            hidden,
            recent: std::sync::Mutex::default(),
        }
    }

    /// Reads every volume, then applies their journals until `stop`.
    pub fn build_and_watch(&self, stop: &AtomicBool, verbose: bool) {
        let mut watchers = Vec::new();
        for letter in fx_index::system::ntfs_volumes() {
            // The watcher starts first: nothing changed during the read is missed
            // (re-applying a change the read already saw is harmless).
            let Ok(watcher) = JournalWatcher::start(&letter) else {
                continue;
            };
            let started = Instant::now();
            match fx_index::read_volume(&letter) {
                Ok(mut index) => {
                    index.shrink_to_fit();
                    if verbose {
                        let mb = index.heap_bytes() as f64 / 1_048_576.0;
                        let n = index.len();
                        println!(
                            "{letter} {n} itens em {:.1} s, {mb:.1} MB ({:.1} MB por milhão)",
                            started.elapsed().as_secs_f64(),
                            mb * 1_000_000.0 / n.max(1) as f64
                        );
                    }
                    self.write().push(Volume { letter, index });
                    watchers.push(watcher);
                }
                Err(e) if verbose => println!("{letter}: {e}"),
                Err(_) => {}
            }
        }
        fx_index::system::trim_heap();
        self.ready.store(true, Ordering::SeqCst);
        while !stop.load(Ordering::SeqCst) {
            std::thread::sleep(WATCH_EVERY);
            for (k, watcher) in watchers.iter_mut().enumerate() {
                let mut volumes = self.write();
                if let Some(v) = volumes.get_mut(k) {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs() as i64);
                    let mut record = |path, kind, is_dir| {
                        self.recent.lock().unwrap_or_else(|e| e.into_inner()).push(
                            fx_index::recent::Change {
                                path,
                                kind,
                                is_dir,
                                at: now,
                            },
                        );
                    };
                    // A journal error (e.g. it wrapped) only delays changes.
                    let _ = watcher.apply(&mut v.index, &mut record);
                }
            }
        }
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Vec<Volume>> {
        self.volumes.write().unwrap_or_else(|e| e.into_inner())
    }

    /// Answers one request. An empty query only reports the state.
    pub fn answer(&self, request: &Request) -> Response {
        let volumes = self.volumes.read().unwrap_or_else(|e| e.into_inner());
        let items = volumes.iter().map(|v| v.index.len() as u64).sum();
        let index_bytes = volumes.iter().map(|v| v.index.heap_bytes() as u64).sum();
        let status = if self.ready.load(Ordering::SeqCst) {
            Status::Ready
        } else {
            Status::Indexing
        };
        if let Some(since) = request.recent_since {
            use std::os::windows::ffi::OsStrExt;
            let recent = self.recent.lock().unwrap_or_else(|e| e.into_inner());
            let hits = recent
                .since(since, request.limit as usize, |c| !self.is_hidden(&c.path))
                .into_iter()
                .map(|c| RemoteHit {
                    path: c.path.as_os_str().encode_wide().collect(),
                    is_dir: c.is_dir,
                    size: 0,
                    modified: filetime_of(c.at),
                    change: Some(c.kind),
                })
                .collect();
            return Response {
                status,
                items,
                index_bytes,
                hits,
            };
        }
        if !request.sizes_of.is_empty() {
            let hits = self.folder_totals(&volumes, &request.sizes_of);
            return Response {
                status,
                items,
                index_bytes,
                hits,
            };
        }
        let filters = request.filter.to_filters();
        let text = request.query.trim();
        if (text.is_empty() && !filters.is_active()) || request.limit == 0 {
            return Response {
                status,
                items,
                index_bytes,
                hits: Vec::new(),
            };
        }
        let never = AtomicBool::new(false);
        let mut scored: Vec<(i32, RemoteHit)> = Vec::new();
        for v in volumes.iter() {
            let index = &v.index;
            // Filters run inside the scan. Without a name, the newest first.
            let keep = |slot: u32| {
                let modified = unix_seconds(index.modified(slot));
                filters
                    .matches(
                        index.name(slot),
                        index.is_dir(slot),
                        index.size(slot),
                        modified,
                    )
                    .then(|| {
                        if text.is_empty() {
                            (modified / 60).clamp(0, i64::from(i32::MAX)) as i32
                        } else {
                            0
                        }
                    })
            };
            let Some(hits) = fx_index::search_where(index, text, &never, None, &keep) else {
                continue;
            };
            for h in hits {
                let Some(rel) = v.index.path(h.slot) else {
                    continue;
                };
                let path = PathBuf::from(format!("{}\\", v.letter)).join(rel);
                if self.is_hidden(&path) {
                    continue;
                }
                use std::os::windows::ffi::OsStrExt;
                scored.push((
                    h.score,
                    RemoteHit {
                        path: path.as_os_str().encode_wide().collect(),
                        is_dir: v.index.is_dir(h.slot),
                        size: v.index.size(h.slot),
                        modified: v.index.modified(h.slot),
                        change: None,
                    },
                ));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.path.len().cmp(&b.1.path.len())));
        scored.truncate(request.limit as usize);
        Response {
            status,
            items,
            index_bytes,
            hits: scored.into_iter().map(|(_, h)| h).collect(),
        }
    }

    /// Totals of the subfolders of `folder` (absolute, UTF-16), as hits;
    /// nothing for a folder outside the index or inside a hidden profile.
    fn folder_totals(&self, volumes: &[Volume], folder: &[u16]) -> Vec<RemoteHit> {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let folder = PathBuf::from(std::ffi::OsString::from_wide(folder));
        if self.is_hidden(&folder) {
            return Vec::new();
        }
        let text = folder.to_string_lossy();
        let Some(v) = volumes
            .iter()
            .find(|v| text.len() >= 2 && text[..2].eq_ignore_ascii_case(&v.letter))
        else {
            return Vec::new();
        };
        let rel = PathBuf::from(text[2..].trim_start_matches('\\'));
        let Some(totals) = v.index.folder_totals(&rel) else {
            return Vec::new();
        };
        totals
            .into_iter()
            .map(|(slot, bytes)| (folder.join(v.index.name(slot)), slot, bytes))
            .filter(|(path, _, _)| !self.is_hidden(path))
            .map(|(path, slot, bytes)| RemoteHit {
                path: path.as_os_str().encode_wide().collect(),
                is_dir: true,
                size: bytes,
                modified: v.index.modified(slot),
                change: None,
            })
            .collect()
    }

    fn is_hidden(&self, path: &std::path::Path) -> bool {
        let p = prefix(path);
        self.hidden.iter().any(|h| p.starts_with(h.as_str()))
    }
}

/// Unix seconds to FILETIME ticks.
fn filetime_of(unix: i64) -> u64 {
    u64::try_from(unix + 11_644_473_600).unwrap_or(0) * 10_000_000
}

/// FILETIME ticks to Unix seconds.
fn unix_seconds(filetime: u64) -> i64 {
    (filetime / 10_000_000) as i64 - 11_644_473_600
}

/// Lowercase path with a trailing `\` (so `C:\Users\an` does not hide
/// `C:\Users\ana`).
fn prefix(path: &std::path::Path) -> String {
    let mut s = path.to_string_lossy().to_lowercase();
    if !s.ends_with('\\') {
        s.push('\\');
    }
    s
}

/// The service body: builds and watches on one thread, serves the pipe on
/// this one, until `stop`.
pub fn serve(user_sid: &str, stop: &'static AtomicBool) {
    let engine = Arc::new(Engine::new(user_sid));
    let builder = engine.clone();
    let _ = std::thread::Builder::new()
        .name("fx-indexd-watch".into())
        .spawn(move || builder.build_and_watch(stop, false));
    let sddl = fx_index::pipe::pipe_sddl(user_sid);
    let _ = fx_index::pipe::serve(&sddl, stop, Arc::new(move |r| engine.answer(&r)));
}

/// `fx-indexd run`: the same in the foreground, printing what it read.
pub fn run_foreground(user_sid: &str) -> std::io::Result<()> {
    static STOP: AtomicBool = AtomicBool::new(false);
    let engine = Arc::new(Engine::new(user_sid));
    let builder = engine.clone();
    std::thread::Builder::new()
        .name("fx-indexd-watch".into())
        .spawn(move || builder.build_and_watch(&STOP, true))?;
    println!("servindo em {} (Ctrl+C sai)", fx_index::pipe::PIPE_NAME);
    let sddl = fx_index::pipe::pipe_sddl(user_sid);
    fx_index::pipe::serve(&sddl, &STOP, Arc::new(move |r| engine.answer(&r)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fx_index::Record;

    /// T6.2: the changes since a time, newest first, without other users'.
    #[test]
    fn recent_changes_are_answered_newest_first() {
        let e = Engine::with_hidden(vec![prefix(std::path::Path::new(r"C:\Users\Ana"))]);
        for (path, at) in [
            (r"C:\Users\Bia\Downloads\a.zip", 100),
            (r"C:\Users\Ana\segredo.txt", 150),
            (r"C:\Users\Bia\notas.txt", 200),
        ] {
            e.recent.lock().unwrap().push(fx_index::recent::Change {
                path: path.into(),
                kind: fx_index::protocol::ChangeKind::Created,
                is_dir: false,
                at,
            });
        }
        let r = e.answer(&Request {
            limit: 10,
            recent_since: Some(90),
            ..Default::default()
        });
        let paths: Vec<String> = r
            .hits
            .iter()
            .map(|h| String::from_utf16_lossy(&h.path))
            .collect();
        assert_eq!(
            paths,
            [r"C:\Users\Bia\notas.txt", r"C:\Users\Bia\Downloads\a.zip"]
        );
        assert_eq!(
            r.hits[0].change,
            Some(fx_index::protocol::ChangeKind::Created)
        );
        assert_eq!(unix_seconds(r.hits[0].modified), 200);
    }

    const ROOT: u64 = 5;

    fn record(id: u64, parent: u64, name: &str, is_dir: bool) -> Record {
        Record {
            id,
            parent,
            name: name.encode_utf16().collect(),
            is_dir,
            size: id * 10,
            modified: 7,
        }
    }

    /// C: with Users\ana\notas.txt, Users\bruno\notas.txt and Dados\notas.md.
    fn engine() -> Engine {
        let mut index = Index::new(ROOT);
        for r in [
            record(10, ROOT, "Users", true),
            record(11, 10, "ana", true),
            record(12, 11, "notas.txt", false),
            record(13, 10, "bruno", true),
            record(14, 13, "notas.txt", false),
            record(15, ROOT, "Dados", true),
            record(16, 15, "notas.md", false),
            record(17, 10, "anabela", true),
            record(18, 17, "notas.doc", false),
        ] {
            index.insert(&r).unwrap();
        }
        let e = Engine::with_hidden(vec![prefix(std::path::Path::new(r"C:\Users\Ana"))]);
        e.write().push(Volume {
            letter: "C:".into(),
            index,
        });
        e.ready.store(true, Ordering::SeqCst);
        e
    }

    fn paths(r: &Response) -> Vec<String> {
        let mut p: Vec<String> = r
            .hits
            .iter()
            .map(|h| String::from_utf16_lossy(&h.path))
            .collect();
        p.sort();
        p
    }

    #[test]
    fn other_users_profiles_are_left_out() {
        let r = engine().answer(&Request {
            query: "notas".into(),
            limit: 50,
            ..Default::default()
        });
        assert_eq!(r.status, Status::Ready);
        assert_eq!(
            paths(&r),
            vec![
                r"C:\Dados\notas.md".to_owned(),
                r"C:\Users\anabela\notas.doc".to_owned(),
                r"C:\Users\bruno\notas.txt".to_owned(),
            ],
            "C:\\Users\\ana is hidden, case-insensitive, but not anabela"
        );
        let first = &r
            .hits
            .iter()
            .find(|h| String::from_utf16_lossy(&h.path).ends_with("notas.md"))
            .unwrap();
        assert!(!first.is_dir);
        assert_eq!(first.size, 160);
    }

    #[test]
    fn filters_apply_inside_the_search_and_alone() {
        use fx_index::protocol::SearchFilter;
        let e = engine();
        let docs = SearchFilter {
            exts: vec!["md".into(), "doc".into()],
            ..Default::default()
        };
        let r = e.answer(&Request {
            query: "notas".into(),
            limit: 50,
            filter: docs.clone(),
            sizes_of: Vec::new(),
            recent_since: None,
        });
        assert_eq!(
            paths(&r),
            vec![
                r"C:\Dados\notas.md".to_owned(),
                r"C:\Users\anabela\notas.doc".to_owned()
            ]
        );
        // No name, only the filter: still answered (and still no hidden profile).
        let r = e.answer(&Request {
            query: String::new(),
            limit: 50,
            filter: SearchFilter {
                dirs: Some(true),
                ..Default::default()
            },
            sizes_of: Vec::new(),
            recent_since: None,
        });
        assert_eq!(
            paths(&r),
            vec![
                r"C:\Dados".to_owned(),
                r"C:\Users".to_owned(),
                r"C:\Users\anabela".to_owned(),
                r"C:\Users\bruno".to_owned(),
            ]
        );
        let big = SearchFilter {
            size_min: Some(150),
            ..Default::default()
        };
        let r = e.answer(&Request {
            query: String::new(),
            limit: 50,
            filter: big,
            sizes_of: Vec::new(),
            recent_since: None,
        });
        assert_eq!(
            paths(&r),
            vec![
                r"C:\Dados\notas.md".to_owned(),
                r"C:\Users\anabela\notas.doc".to_owned()
            ]
        );
    }

    #[test]
    fn folder_totals_through_the_service_hide_other_profiles() {
        let e = engine();
        let ask = |p: &str| {
            let r = e.answer(&Request {
                sizes_of: p.encode_utf16().collect(),
                recent_since: None,
                ..Default::default()
            });
            let mut v: Vec<(String, u64)> = r
                .hits
                .iter()
                .map(|h| (String::from_utf16_lossy(&h.path), h.size))
                .collect();
            v.sort();
            v
        };
        assert_eq!(
            ask(r"c:\Users"),
            vec![
                (r"c:\Users\anabela".to_owned(), 180),
                (r"c:\Users\bruno".to_owned(), 140),
            ],
            "ana is another user's profile: left out"
        );
        assert!(ask(r"C:\Users\Ana").is_empty());
        assert_eq!(ask(r"C:\").len(), 2);
        assert!(ask(r"Z:\x").is_empty());
    }

    #[test]
    fn limit_status_and_empty_queries() {
        let e = engine();
        let r = e.answer(&Request {
            query: "notas".into(),
            limit: 1,
            ..Default::default()
        });
        assert_eq!(r.hits.len(), 1);
        let r = e.answer(&Request {
            query: String::new(),
            limit: 10,
            ..Default::default()
        });
        assert!(r.hits.is_empty());
        assert_eq!(r.items, 9);
        e.ready.store(false, Ordering::SeqCst);
        assert_eq!(
            e.answer(&Request {
                query: "x".into(),
                limit: 1,
                ..Default::default()
            })
            .status,
            Status::Indexing
        );
    }
}
