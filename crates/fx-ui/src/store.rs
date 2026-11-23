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
    /// Keep a local crash report (T3.10); nothing is ever sent by fx.
    pub relatorio_falhas: bool,
    /// Ask GitHub once a day for a newer version (T4.1).
    pub verificar_atualizacoes: bool,
    /// "Como usar o fx" was already shown once (T4.6).
    pub guia_visto: bool,
    /// Ask before sending items to the Recycle Bin.
    pub confirmar_lixeira: bool,
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
            relatorio_falhas: true,
            verificar_atualizacoes: true,
            guia_visto: false,
            confirmar_lixeira: true,
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

/// An operation that can be undone, saved for the next session (T6.6):
/// what the history panel says, when, and both jobs (`Job::encode`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UndoRow {
    pub label: String,
    /// Unix seconds.
    pub at: i64,
    pub forward: Vec<u8>,
    pub backward: Vec<u8>,
}

/// A named set of tabs (T6.13): the folders and which one is active.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WorkspaceState {
    pub name: String,
    pub tabs: Vec<PathBuf>,
    pub active: usize,
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
    /// Folders opened, for "ir para" que aprende (T6.5); `None` while they
    /// are still loading (the saved ones are then left as they are).
    pub visits: Option<Vec<fx_core::frecency::Visit>>,
    /// Workspaces (T6.13); `None` while they are still loading.
    pub workspaces: Option<Vec<WorkspaceState>>,
    /// Undo history (T6.6), oldest first; `None` until this session's history
    /// took in the saved one.
    pub undo: Option<Vec<UndoRow>>,
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
    -- T6.5: how often and how recently each folder was opened.
    CREATE TABLE IF NOT EXISTS visits (
        path TEXT PRIMARY KEY, rank REAL NOT NULL, last INTEGER NOT NULL);
    -- T6.6: the undo history, oldest first.
    CREATE TABLE IF NOT EXISTS undo (
        seq INTEGER NOT NULL, at INTEGER NOT NULL, label TEXT NOT NULL,
        forward BLOB NOT NULL, backward BLOB NOT NULL);
    -- T6.13: named sets of tabs, one row per tab.
    CREATE TABLE IF NOT EXISTS workspaces (
        name TEXT NOT NULL, pos INTEGER NOT NULL, path TEXT NOT NULL, active INTEGER NOT NULL);
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
        // Several connections (the session writer, the visits loader, the
        // last save on quit): wait a moment for a busy one instead of failing.
        conn.busy_timeout(std::time::Duration::from_millis(500))?;
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
        if let Some(rows) = &s.undo {
            tx.execute("DELETE FROM undo", [])?;
            for (k, r) in rows.iter().enumerate() {
                tx.execute(
                    "INSERT INTO undo (seq, at, label, forward, backward) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![k as i64, r.at, r.label, r.forward, r.backward],
                )?;
            }
        }
        if let Some(spaces) = &s.workspaces {
            tx.execute("DELETE FROM workspaces", [])?;
            for ws in spaces {
                for (k, path) in ws.tabs.iter().enumerate() {
                    // Only exact paths are kept (nothing converted with loss).
                    if let Some(path) = path.to_str() {
                        tx.execute(
                            "INSERT INTO workspaces (name, pos, path, active) VALUES (?1, ?2, ?3, ?4)",
                            params![ws.name, k as i64, path, ws.active as i64],
                        )?;
                    }
                }
            }
        }
        if let Some(visits) = &s.visits {
            tx.execute("DELETE FROM visits", [])?;
            for v in visits {
                // Only exact paths are kept (nothing converted with loss).
                if let Some(path) = v.path.to_str() {
                    tx.execute(
                        "INSERT OR REPLACE INTO visits (path, rank, last) VALUES (?1, ?2, ?3)",
                        params![path, v.rank, v.last],
                    )?;
                }
            }
        }
        tx.commit()
    }

    /// The saved undo history (T6.6), oldest first.
    pub fn load_undo(&self) -> rusqlite::Result<Vec<UndoRow>> {
        let mut q = self
            .conn
            .prepare("SELECT label, at, forward, backward FROM undo ORDER BY seq")?;
        let rows = q.query_map([], |r| {
            Ok(UndoRow {
                label: r.get(0)?,
                at: r.get(1)?,
                forward: r.get(2)?,
                backward: r.get(3)?,
            })
        })?;
        rows.collect()
    }

    /// The workspaces (T6.13), by name.
    pub fn load_workspaces(&self) -> rusqlite::Result<Vec<WorkspaceState>> {
        let mut q = self
            .conn
            .prepare("SELECT name, path, active FROM workspaces ORDER BY name, pos")?;
        let rows = q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                PathBuf::from(r.get::<_, String>(1)?),
                r.get::<_, i64>(2)?,
            ))
        })?;
        let mut out: Vec<WorkspaceState> = Vec::new();
        for row in rows {
            let (name, path, active) = row?;
            match out.last_mut().filter(|w| w.name == name) {
                Some(w) => w.tabs.push(path),
                None => out.push(WorkspaceState {
                    name,
                    tabs: vec![path],
                    active: usize::try_from(active).unwrap_or(0),
                }),
            }
        }
        Ok(out)
    }

    /// The folders opened (T6.5), read on their own: they count even when
    /// the session is not restored.
    pub fn load_visits(&self) -> rusqlite::Result<Vec<fx_core::frecency::Visit>> {
        let mut q = self.conn.prepare("SELECT path, rank, last FROM visits")?;
        let rows = q.query_map([], |r| {
            Ok(fx_core::frecency::Visit {
                path: PathBuf::from(r.get::<_, String>(0)?),
                rank: r.get(1)?,
                last: r.get(2)?,
            })
        })?;
        rows.collect()
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
            visits: None,
            workspaces: None,
            undo: None,
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

    #[test]
    fn visits_round_trip_and_are_kept_while_loading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.db");
        let mut db = Db::open(&path).unwrap();
        let visit = |p: &str, rank: f64, last: i64| fx_core::frecency::Visit {
            path: PathBuf::from(p),
            rank,
            last,
        };
        let mut s = session(1);
        s.visits = Some(vec![
            visit(r"C:\Projetos\fx", 3.5, 100),
            visit(r"D:\Fotos", 1.0, 50),
        ]);
        db.save(&s).unwrap();
        let mut back = db.load_visits().unwrap();
        back.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(
            back,
            vec![
                visit(r"C:\Projetos\fx", 3.5, 100),
                visit(r"D:\Fotos", 1.0, 50)
            ]
        );
        // Saved before the visits loaded: they stay as they were.
        s.visits = None;
        db.save(&s).unwrap();
        assert_eq!(db.load_visits().unwrap().len(), 2);
    }

    #[test]
    fn undo_rows_round_trip_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Db::open(&dir.path().join("state.db")).unwrap();
        let row = |k: u8| UndoRow {
            label: format!("op {k}"),
            at: i64::from(k),
            forward: vec![k, 1],
            backward: vec![k, 2],
        };
        let mut s = session(1);
        s.undo = Some(vec![row(2), row(1)]);
        db.save(&s).unwrap();
        assert_eq!(db.load_undo().unwrap(), vec![row(2), row(1)]);
        s.undo = None;
        db.save(&s).unwrap();
        assert_eq!(db.load_undo().unwrap().len(), 2, "kept until merged");
    }

    #[test]
    fn workspaces_round_trip_in_tab_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Db::open(&dir.path().join("state.db")).unwrap();
        let ws = |name: &str, tabs: &[&str], active| WorkspaceState {
            name: name.into(),
            tabs: tabs.iter().map(PathBuf::from).collect(),
            active,
        };
        let mut s = session(1);
        s.workspaces = Some(vec![
            ws("Trabalho", &[r"C:\Projetos\fx", r"C:\Projetos\fx\docs"], 1),
            ws("Fotos", &[r"D:\Fotos"], 0),
        ]);
        db.save(&s).unwrap();
        assert_eq!(
            db.load_workspaces().unwrap(),
            vec![
                ws("Fotos", &[r"D:\Fotos"], 0),
                ws("Trabalho", &[r"C:\Projetos\fx", r"C:\Projetos\fx\docs"], 1),
            ]
        );
        s.workspaces = None;
        db.save(&s).unwrap();
        assert_eq!(db.load_workspaces().unwrap().len(), 2, "kept while loading");
    }

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
            visits: None,
            workspaces: None,
            undo: None,
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
            relatorio_falhas: false,
            verificar_atualizacoes: false,
            guia_visto: true,
            confirmar_lixeira: false,
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
