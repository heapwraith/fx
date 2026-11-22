//! Persistence (task T1.17): settings in TOML, the session in SQLite.
//!
//! - `%APPDATA%\fx\settings.toml`: preferences (row density, sidebar,
//!   whether to restore the session). Written atomically (temporary file,
//!   then rename), so a crash leaves the old or the new file, never half.
//! - `%LOCALAPPDATA%\fx\state.db`: windows (position), tabs (folder, scroll,
//!   selection, cursor, grid, history) and quick access. SQLite in WAL mode;
//!   each save is one transaction, so killing the process at any moment
//!   leaves the last complete save.
//!
//! Saving runs on its own thread (`spawn_writer`); the UI only builds a
//! [`Session`] value. Paths are stored as text (lossy only for names that are
//! not valid Unicode, which Windows practically never has).

#[allow(unused_imports)]
use fx_core::{tr, trf};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

/// User preferences (`settings.toml`). Unknown or missing keys fall back to
/// the defaults, so an old or hand-edited file always loads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "compacta", "padrão" or "confortável".
    pub densidade: String,
    pub barra_lateral: bool,
    /// Reopen the last windows and tabs when fx starts without a folder.
    pub restaurar_sessao: bool,
    /// Show hidden and system files (Ctrl+H, T1.21).
    pub mostrar_ocultos: bool,
    /// Interface language (T3.7): "pt", "en" or "" (the Windows language).
    pub idioma: String,
}

/// The part of the settings a window changes (density, sidebar, hidden files).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiSettings {
    pub density: String,
    pub sidebar: bool,
    pub show_hidden: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            densidade: "padrão".into(),
            barra_lateral: true,
            restaurar_sessao: true,
            mostrar_ocultos: false,
            idioma: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TabState {
    pub path: PathBuf,
    pub scroll: f32,
    pub grid: bool,
    pub selected: Vec<String>,
    pub cursor: Option<String>,
    pub back: Vec<PathBuf>,
    pub forward: Vec<PathBuf>,
    /// Sort column (`SortKey::id`) and direction; `None`: name, A→Z.
    pub sort: Option<(String, bool)>,
    /// Widths of the Size, Type and Date columns; `None`: defaults.
    pub widths: Option<[f32; 3]>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WindowState {
    /// x, y, width, height (logical pixels).
    pub bounds: Option<[f32; 4]>,
    pub active: usize,
    pub tabs: Vec<TabState>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Session {
    pub windows: Vec<WindowState>,
    /// Quick access; `None` before it was ever saved (use the defaults).
    pub pinned: Option<Vec<PathBuf>>,
    pub settings: Settings,
}

/// `%APPDATA%\fx` or `%LOCALAPPDATA%\fx`; `FX_DATA_DIR` replaces both (tests).
fn app_dir(var: &str) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FX_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os(var).map(|d| PathBuf::from(d).join("fx"))
}

pub fn settings_path() -> Option<PathBuf> {
    app_dir("APPDATA").map(|d| d.join("settings.toml"))
}

/// Journal of the copies in progress (T2.4): kept even with `FX_NO_STATE`,
/// since it protects the user's disk, not the app's state.
pub fn ops_journal_dir() -> Option<PathBuf> {
    app_dir("LOCALAPPDATA").map(|d| d.join("ops"))
}

pub fn state_path() -> Option<PathBuf> {
    app_dir("LOCALAPPDATA").map(|d| d.join("state.db"))
}

/// Settings from `path`; defaults if it is missing or unreadable.
pub fn load_settings(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

/// Writes the settings atomically (temporary file in the same folder, then
/// rename over the old one).
pub fn save_settings(path: &Path, settings: &Settings) -> io::Result<()> {
    let text = toml::to_string_pretty(settings).map_err(io::Error::other)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let temp = dir.join(format!(".settings.{}.tmp", std::process::id()));
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, path)
}

/// The session database.
pub struct Db {
    conn: Connection,
}

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS pinned (pos INTEGER NOT NULL, path TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS windows (
        id INTEGER PRIMARY KEY, x REAL, y REAL, w REAL, h REAL, active INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS tabs (
        win INTEGER NOT NULL, pos INTEGER NOT NULL, path TEXT NOT NULL,
        scroll REAL NOT NULL, grid INTEGER NOT NULL, cursor TEXT);
    CREATE TABLE IF NOT EXISTS tab_selected (
        win INTEGER NOT NULL, tab INTEGER NOT NULL, name TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS tab_history (
        win INTEGER NOT NULL, tab INTEGER NOT NULL, forward INTEGER NOT NULL,
        pos INTEGER NOT NULL, path TEXT NOT NULL);
    -- T1.20. A table of its own: databases saved before it need no migration.
    CREATE TABLE IF NOT EXISTS tab_view (
        win INTEGER NOT NULL, tab INTEGER NOT NULL, sort TEXT, descending INTEGER,
        size_w REAL, type_w REAL, date_w REAL);
";

fn text(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let conn = Connection::open(path)?;
        // WAL: a killed writer leaves the last committed transaction intact.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Replaces the saved session, in one transaction.
    pub fn save(&mut self, s: &Session) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute_batch(
            "DELETE FROM windows; DELETE FROM tabs; DELETE FROM tab_selected; DELETE FROM tab_history; DELETE FROM tab_view;",
        )?;
        for (w, win) in s.windows.iter().enumerate() {
            let b = win.bounds;
            tx.execute(
                "INSERT INTO windows (id, x, y, w, h, active) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    w as i64,
                    b.map(|b| b[0]),
                    b.map(|b| b[1]),
                    b.map(|b| b[2]),
                    b.map(|b| b[3]),
                    win.active as i64
                ],
            )?;
            for (t, tab) in win.tabs.iter().enumerate() {
                tx.execute(
                    "INSERT INTO tabs (win, pos, path, scroll, grid, cursor) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![w as i64, t as i64, text(&tab.path), tab.scroll, tab.grid, tab.cursor],
                )?;
                if tab.sort.is_some() || tab.widths.is_some() {
                    let w3 = tab.widths;
                    tx.execute(
                        "INSERT INTO tab_view (win, tab, sort, descending, size_w, type_w, date_w) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            w as i64,
                            t as i64,
                            tab.sort.as_ref().map(|s| s.0.clone()),
                            tab.sort.as_ref().map(|s| s.1),
                            w3.map(|v| v[0]),
                            w3.map(|v| v[1]),
                            w3.map(|v| v[2])
                        ],
                    )?;
                }
                for name in &tab.selected {
                    tx.execute(
                        "INSERT INTO tab_selected (win, tab, name) VALUES (?1, ?2, ?3)",
                        params![w as i64, t as i64, name],
                    )?;
                }
                for (forward, list) in [(false, &tab.back), (true, &tab.forward)] {
                    for (k, p) in list.iter().enumerate() {
                        tx.execute(
                            "INSERT INTO tab_history (win, tab, forward, pos, path) VALUES (?1, ?2, ?3, ?4, ?5)",
                            params![w as i64, t as i64, forward, k as i64, text(p)],
                        )?;
                    }
                }
            }
        }
        if let Some(pinned) = &s.pinned {
            tx.execute("DELETE FROM pinned", [])?;
            for (k, p) in pinned.iter().enumerate() {
                tx.execute(
                    "INSERT INTO pinned (pos, path) VALUES (?1, ?2)",
                    params![k as i64, text(p)],
                )?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('pinned_saved', '1')",
                [],
            )?;
        }
        tx.commit()
    }

    pub fn load(&self) -> rusqlite::Result<Session> {
        let mut windows = Vec::new();
        let mut q = self
            .conn
            .prepare("SELECT id, x, y, w, h, active FROM windows ORDER BY id")?;
        let rows = q.query_map([], |r| {
            let id: i64 = r.get(0)?;
            let b: [Option<f32>; 4] = [r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?];
            let active: i64 = r.get(5)?;
            Ok((id, b, active))
        })?;
        for row in rows {
            let (id, b, active) = row?;
            let bounds = match b {
                [Some(x), Some(y), Some(w), Some(h)] => Some([x, y, w, h]),
                _ => None,
            };
            windows.push((
                id,
                WindowState {
                    bounds,
                    active: active.max(0) as usize,
                    tabs: Vec::new(),
                },
            ));
        }
        for (id, win) in &mut windows {
            let mut q = self.conn.prepare(
                "SELECT pos, path, scroll, grid, cursor FROM tabs WHERE win = ?1 ORDER BY pos",
            )?;
            let tabs = q.query_map([*id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    TabState {
                        path: PathBuf::from(r.get::<_, String>(1)?),
                        scroll: r.get(2)?,
                        grid: r.get(3)?,
                        cursor: r.get(4)?,
                        ..Default::default()
                    },
                ))
            })?;
            for tab in tabs {
                let (pos, mut tab) = tab?;
                let mut sel = self
                    .conn
                    .prepare("SELECT name FROM tab_selected WHERE win = ?1 AND tab = ?2")?;
                tab.selected = sel
                    .query_map([*id, pos], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
                let mut hist = self.conn.prepare(
                    "SELECT forward, path FROM tab_history WHERE win = ?1 AND tab = ?2 ORDER BY pos",
                )?;
                for h in hist.query_map([*id, pos], |r| {
                    Ok((r.get::<_, bool>(0)?, r.get::<_, String>(1)?))
                })? {
                    let (forward, path) = h?;
                    if forward {
                        tab.forward.push(PathBuf::from(path));
                    } else {
                        tab.back.push(PathBuf::from(path));
                    }
                }
                let view = self.conn.query_row(
                    "SELECT sort, descending, size_w, type_w, date_w FROM tab_view WHERE win = ?1 AND tab = ?2",
                    [*id, pos],
                    |r| {
                        Ok((
                            r.get::<_, Option<String>>(0)?,
                            r.get::<_, Option<bool>>(1)?,
                            r.get::<_, Option<f32>>(2)?,
                            r.get::<_, Option<f32>>(3)?,
                            r.get::<_, Option<f32>>(4)?,
                        ))
                    },
                );
                match view {
                    Ok((sort, desc, a, b, c)) => {
                        tab.sort = sort.map(|k| (k, desc.unwrap_or(false)));
                        tab.widths = match (a, b, c) {
                            (Some(a), Some(b), Some(c)) => Some([a, b, c]),
                            _ => None,
                        };
                    }
                    Err(rusqlite::Error::QueryReturnedNoRows) => {}
                    Err(e) => return Err(e),
                }
                win.tabs.push(tab);
            }
        }
        let saved: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'pinned_saved'",
                [],
                |r| r.get(0),
            )
            .ok();
        let pinned = match saved {
            Some(_) => {
                let mut q = self.conn.prepare("SELECT path FROM pinned ORDER BY pos")?;
                Some(
                    q.query_map([], |r| r.get::<_, String>(0))?
                        .map(|p| p.map(PathBuf::from))
                        .collect::<rusqlite::Result<_>>()?,
                )
            }
            None => None,
        };
        Ok(Session {
            windows: windows.into_iter().map(|(_, w)| w).collect(),
            pinned,
            settings: Settings::default(),
        })
    }

    #[cfg(test)]
    fn integrity(&self) -> rusqlite::Result<String> {
        self.conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
    }
}

/// Saves sessions on a thread of its own; only the latest pending one is
/// written. Settings go to their TOML file when they change.
pub fn spawn_writer(db: PathBuf, settings: PathBuf) -> io::Result<Sender<Session>> {
    let (tx, rx) = channel::<Session>();
    std::thread::Builder::new()
        .name("fx-store".into())
        .spawn(move || write_loop(&rx, &db, &settings))?;
    Ok(tx)
}

fn write_loop(rx: &Receiver<Session>, db: &Path, settings: &Path) {
    let Ok(mut store) = Db::open(db) else { return };
    let mut saved_settings: Option<Settings> = None;
    while let Ok(mut session) = rx.recv() {
        while let Ok(newer) = rx.try_recv() {
            session = newer;
        }
        let _ = store.save(&session);
        if saved_settings.as_ref() != Some(&session.settings)
            && save_settings(settings, &session.settings).is_ok()
        {
            saved_settings = Some(session.settings.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn session(n: u32) -> Session {
        Session {
            windows: vec![WindowState {
                bounds: Some([10.0, 20.0, 900.0, 640.0]),
                active: 1,
                tabs: vec![
                    TabState {
                        path: PathBuf::from(r"C:\Fotos"),
                        scroll: -(n as f32),
                        grid: true,
                        selected: vec!["praia ☀.jpg".into(), "b.png".into()],
                        cursor: Some("b.png".into()),
                        back: vec![PathBuf::from(r"C:\"), PathBuf::from(r"C:\Users")],
                        forward: vec![PathBuf::from(r"C:\Fotos\2026")],
                        sort: Some(("modified".into(), true)),
                        widths: Some([90.0, 140.0, 150.0]),
                    },
                    TabState {
                        path: PathBuf::from(r"D:\Projetos"),
                        ..Default::default()
                    },
                ],
            }],
            pinned: Some(vec![PathBuf::from(r"C:\Fotos"), PathBuf::from(r"D:\")]),
            settings: Settings::default(),
        }
    }

    #[test]
    fn session_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.db");
        let mut db = Db::open(&path).unwrap();
        let empty = db.load().unwrap();
        assert!(empty.windows.is_empty() && empty.pinned.is_none());
        db.save(&session(7)).unwrap();
        drop(db);
        let back = Db::open(&path).unwrap().load().unwrap();
        assert_eq!(back, session(7));
    }

    #[test]
    fn settings_round_trip_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fx").join("settings.toml");
        assert_eq!(load_settings(&path), Settings::default(), "missing file");
        let s = Settings {
            densidade: "compacta".into(),
            barra_lateral: false,
            restaurar_sessao: true,
            mostrar_ocultos: true,
            idioma: "en".into(),
        };
        save_settings(&path, &s).unwrap();
        assert_eq!(load_settings(&path), s);
        std::fs::write(&path, "barra_lateral = false\nchave_desconhecida = 3\n").unwrap();
        let partial = load_settings(&path);
        assert!(!partial.barra_lateral);
        assert_eq!(partial.densidade, "padrão", "missing keys take defaults");
        std::fs::write(&path, "isto não é toml [[[").unwrap();
        assert_eq!(load_settings(&path), Settings::default(), "broken file");
    }

    /// Child process for the kill test: saves sessions forever.
    #[test]
    #[ignore]
    fn child_writes_forever() {
        let Some(path) = std::env::var_os("FX_STORE_CHILD_DB") else {
            return;
        };
        let mut db = Db::open(Path::new(&path)).unwrap();
        let start = db
            .load()
            .ok()
            .and_then(|s| {
                s.windows
                    .first()
                    .and_then(|w| w.tabs.first())
                    .map(|t| -t.scroll as u32)
            })
            .unwrap_or(0);
        let mut n = start;
        loop {
            n += 1;
            db.save(&session(n)).unwrap();
        }
    }

    /// Killing the process in the middle of saving leaves the database
    /// intact with the last committed session (5 kills).
    #[test]
    fn killed_writer_leaves_the_last_saved_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.db");
        let exe = std::env::current_exe().unwrap();
        let mut last = 0;
        for round in 0..5 {
            let mut child = std::process::Command::new(&exe)
                .args([
                    "store::tests::child_writes_forever",
                    "--exact",
                    "--ignored",
                    "--test-threads=1",
                ])
                .env("FX_STORE_CHILD_DB", &path)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            std::thread::sleep(Duration::from_millis(300 + 70 * round));
            child.kill().unwrap(); // TerminateProcess: no chance to clean up
            child.wait().unwrap();
            let db = Db::open(&path).unwrap();
            assert_eq!(db.integrity().unwrap(), "ok", "round {round}");
            let s = db.load().unwrap();
            let n = -s.windows[0].tabs[0].scroll as u32;
            assert!(n > last, "round {round}: progress saved ({n} after {last})");
            assert_eq!(s, session(n), "a whole session, never a partial one");
            last = n;
        }
    }
}
