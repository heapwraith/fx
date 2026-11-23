//! Color tags of files and folders (task T3.4): kept in
//! `%LOCALAPPDATA%\fx\tags.db` (SQLite), mirrored in memory for drawing.
//! Loading and every write happen on a worker thread; the UI only reads the
//! in-memory map (a lock and a hash lookup). With `FX_NO_STATE=1` the tags
//! live in memory only.
//!
//! Keys are full paths in lower case (NTFS ignores case); the path as
//! written is kept for display. Renames and moves done by fx carry the tags
//! along (`moved`), folders with everything below: by path components, never
//! by cutting text, since lower case can change a name's length ("İ" becomes
//! two characters; review, D-066).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock, PoisonError};

use rusqlite::{Connection, params};

enum Write {
    /// Key, path as written, mask (0: delete).
    Set(String, String, u8),
    /// The user's name for tag `k` (`None`: the built-in one; T5.5).
    Name(usize, Option<String>),
}

struct Store {
    /// Lower-case path → (mask, path as written).
    map: HashMap<String, (u8, PathBuf)>,
    writer: Option<Sender<Write>>,
}

fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| {
        let writer = db_path().and_then(|db| spawn_worker(db).ok());
        Mutex::new(Store {
            map: HashMap::new(),
            writer,
        })
    })
}

fn db_path() -> Option<PathBuf> {
    if std::env::var_os("FX_NO_STATE").is_some_and(|v| v == "1") {
        return None;
    }
    crate::store::state_path().map(|p| p.with_file_name("tags.db"))
}

/// Starts loading the saved tags (call once at startup).
pub fn init() {
    let _ = store();
}

/// The lookup key of `path`: lower case, exact. A path that is not valid
/// Unicode (a lone surrogate) keeps its odd bytes escaped, so two such paths
/// never share a key.
fn key(path: &Path) -> String {
    if let Some(text) = path.to_str() {
        return text.to_lowercase();
    }
    let mut out = String::new();
    for chunk in path.as_os_str().as_encoded_bytes().utf8_chunks() {
        out.push_str(&chunk.valid().to_lowercase());
        for b in chunk.invalid() {
            out.push_str(&format!("\\x{b:02x}"));
        }
    }
    out
}

fn lock() -> std::sync::MutexGuard<'static, Store> {
    store().lock().unwrap_or_else(PoisonError::into_inner)
}

/// The tag mask of `path` (0: none).
pub fn mask_of(path: &Path) -> u8 {
    lock().map.get(&key(path)).map_or(0, |(m, _)| *m)
}

/// Every tagged path that has all the tags of `mask`.
pub fn tagged(mask: u8) -> Vec<PathBuf> {
    lock()
        .map
        .values()
        .filter(|(m, _)| *m & mask == mask)
        .map(|(_, p)| p.clone())
        .collect()
}

/// Turns tag `bit` on for every path, or off for all of them when every one
/// already has it (like toggling a selection).
pub fn toggle(paths: &[PathBuf], bit: u8) {
    let mut s = lock();
    let all_have = paths
        .iter()
        .all(|p| s.map.get(&key(p)).is_some_and(|(m, _)| m & bit != 0));
    for p in paths {
        let old = s.map.get(&key(p)).map_or(0, |(m, _)| *m);
        let new = if all_have { old & !bit } else { old | bit };
        set_locked(&mut s, p, new);
    }
}

/// Removes every tag from these paths.
pub fn clear(paths: &[PathBuf]) {
    let mut s = lock();
    for p in paths {
        set_locked(&mut s, p, 0);
    }
}

fn set_locked(s: &mut Store, path: &Path, mask: u8) {
    let k = key(path);
    if mask == 0 {
        s.map.remove(&k);
    } else {
        s.map.insert(k.clone(), (mask, path.to_path_buf()));
    }
    if let Some(w) = &s.writer {
        let _ = w.send(Write::Set(k, path.to_string_lossy().into_owned(), mask));
    }
}

/// Gives tag `k` its own name (empty: back to the built-in one), kept with
/// the tags.
pub fn rename(k: usize, name: &str) {
    let name = Some(name.trim().to_owned()).filter(|n| !n.is_empty());
    fx_core::tags::set_name(k, name.clone());
    if let Some(w) = &lock().writer {
        let _ = w.send(Write::Name(k, name));
    }
}

/// `old` was renamed or moved to `new` (by fx): its tags, and those of
/// everything below it, follow.
pub fn moved(old: &Path, new: &Path) {
    let old_key = key(old);
    let prefix = format!("{old_key}\\");
    let depth = old.components().count();
    let mut s = lock();
    let moving: Vec<(String, u8, PathBuf)> = s
        .map
        .iter()
        .filter(|(k, _)| **k == old_key || k.starts_with(&prefix))
        .map(|(k, (m, p))| (k.clone(), *m, p.clone()))
        .collect();
    for (k, m, p) in moving {
        // What is below the moved item keeps its own spelling.
        let mut to = new.to_path_buf();
        to.extend(p.components().skip(depth));
        s.map.remove(&k);
        if let Some(w) = &s.writer {
            let _ = w.send(Write::Set(k, String::new(), 0));
        }
        set_locked(&mut s, &to, m);
    }
}

fn spawn_worker(db: PathBuf) -> std::io::Result<Sender<Write>> {
    let (tx, rx) = channel::<Write>();
    std::thread::Builder::new()
        .name("fx-tags".into())
        .spawn(move || {
            if let Some(dir) = db.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let conn = match Connection::open(&db) {
                Ok(conn) => conn,
                Err(e) => {
                    log::warn!("tags database could not open: {e}");
                    return;
                }
            };
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS tags \
                 (key TEXT PRIMARY KEY, path TEXT NOT NULL, mask INTEGER NOT NULL); \
                 CREATE TABLE IF NOT EXISTS tag_names (bit INTEGER PRIMARY KEY, name TEXT NOT NULL);",
            );
            if let Ok(mut stmt) = conn.prepare("SELECT bit, name FROM tag_names")
                && let Ok(rows) =
                    stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            {
                for (k, name) in rows.flatten() {
                    fx_core::tags::set_name(k.max(0) as usize, Some(name));
                }
            }
            // Load, under what the UI may already have changed.
            if let Ok(mut stmt) = conn.prepare("SELECT key, path, mask FROM tags")
                && let Ok(rows) = stmt.query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                })
            {
                let loaded: Vec<(String, String, i64)> = rows.flatten().collect();
                let mut s = lock();
                for (k, p, m) in loaded {
                    s.map.entry(k).or_insert((m as u8, PathBuf::from(p)));
                }
            }
            for w in rx {
                let _ = match w {
                    Write::Set(k, _, 0) => {
                        conn.execute("DELETE FROM tags WHERE key = ?1", params![k])
                    }
                    Write::Set(k, path, mask) => conn.execute(
                        "INSERT INTO tags (key, path, mask) VALUES (?1, ?2, ?3) \
                         ON CONFLICT(key) DO UPDATE SET path = ?2, mask = ?3",
                        params![k, path, i64::from(mask)],
                    ),
                    Write::Name(k, None) => {
                        conn.execute("DELETE FROM tag_names WHERE bit = ?1", params![k as i64])
                    }
                    Write::Name(k, Some(name)) => conn.execute(
                        "INSERT INTO tag_names (bit, name) VALUES (?1, ?2) \
                         ON CONFLICT(bit) DO UPDATE SET name = ?2",
                        params![k as i64, name],
                    ),
                };
            }
        })?;
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name whose lower case is longer ("İ" → "i̇") still moves right.
    #[test]
    fn moving_a_folder_with_a_turkish_i_keeps_the_names_below() {
        let inside = PathBuf::from(r"C:\fx-test-tags-i\İstanbul\fotos\a.jpg");
        toggle(std::slice::from_ref(&inside), 1 << 2);
        moved(
            Path::new(r"C:\fx-test-tags-i\İstanbul"),
            Path::new(r"C:\fx-test-tags-i\Ankara"),
        );
        let after = PathBuf::from(r"C:\fx-test-tags-i\Ankara\fotos\a.jpg");
        assert_eq!(mask_of(&after), 1 << 2);
        assert_eq!(mask_of(&inside), 0);
        assert!(tagged(1 << 2).contains(&after), "{:?}", tagged(1 << 2));
        toggle(std::slice::from_ref(&after), 1 << 2);
    }

    #[cfg(windows)]
    #[test]
    fn paths_that_are_not_unicode_get_their_own_keys() {
        use std::os::windows::ffi::OsStringExt;
        let a = PathBuf::from(std::ffi::OsString::from_wide(&[0x43, 0x3A, 0x5C, 0xD800]));
        let b = PathBuf::from(std::ffi::OsString::from_wide(&[0x43, 0x3A, 0x5C, 0xD801]));
        assert_ne!(key(&a), key(&b));
        assert_eq!(key(Path::new(r"C:\Pasta")), r"c:\pasta");
    }

    // The process-wide store; FX_NO_STATE=1 in test runs keeps it in memory.
    #[test]
    fn toggle_clear_move_and_list() {
        let a = PathBuf::from(r"C:\fx-test-tags\Pasta\a.txt");
        let b = PathBuf::from(r"C:\fx-test-tags\Pasta\B.txt");
        toggle(&[a.clone(), b.clone()], 1 << 3);
        assert_eq!(mask_of(&a), 1 << 3);
        assert_eq!(
            mask_of(Path::new(r"c:\FX-TEST-TAGS\pasta\A.TXT")),
            1 << 3,
            "case-insensitive"
        );
        assert!(tagged(1 << 3).contains(&b), "listed with its own spelling");
        // Toggling when all have it turns it off for all.
        toggle(&[a.clone(), b.clone()], 1 << 3);
        assert_eq!(mask_of(&a), 0);
        toggle(std::slice::from_ref(&a), 1);
        toggle(std::slice::from_ref(&a), 1 << 4);
        assert_eq!(mask_of(&a), 1 | (1 << 4));
        // Moving the folder carries the tags of what is inside.
        moved(
            Path::new(r"C:\fx-test-tags\Pasta"),
            Path::new(r"C:\fx-test-tags\Nova"),
        );
        assert_eq!(mask_of(&a), 0);
        let moved_a = PathBuf::from(r"C:\fx-test-tags\Nova\a.txt");
        assert_eq!(mask_of(&moved_a), 1 | (1 << 4));
        assert!(tagged(1).contains(&moved_a));
        // A sibling with a longer name is not part of the folder.
        toggle(&[PathBuf::from(r"C:\fx-test-tags\Nova2\x")], 1);
        moved(
            Path::new(r"C:\fx-test-tags\Nova"),
            Path::new(r"C:\fx-test-tags\N"),
        );
        assert_eq!(mask_of(Path::new(r"C:\fx-test-tags\Nova2\x")), 1);
        clear(&[PathBuf::from(r"C:\fx-test-tags\N\a.txt")]);
        assert_eq!(mask_of(Path::new(r"C:\fx-test-tags\N\a.txt")), 0);
    }
}
