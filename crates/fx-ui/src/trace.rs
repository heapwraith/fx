//! `FX_TRACE=1` instrumentation (task T0.5): time from process start to the
//! first frame with rows (P2) and frame intervals while scrolling (P7).
//!
//! Output: `bench/results/trace-<unix ms>-<pid>.txt`, one `key value` per line,
//! read by `fx-bench`. With `FX_TRACE_EXIT=1` the app writes the trace and quits
//! right after the first frame with rows (used to measure 10 openings).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use fx_core::format::millis;

/// Scrolling counts as "active" until this long after the last offset change.
const ACTIVE_AFTER_SCROLL: Duration = Duration::from_millis(150);

#[derive(Debug)]
pub struct Trace {
    pub exit_after_first_frame: bool,
    path: String,
    process_start: Option<SystemTime>,
    /// Named milestones (ms since process start), in the order they happened.
    marks: Vec<(&'static str, f64)>,
    first_frame_ms: Option<f64>,
    first_frame_items: usize,
    loaded_ms: Option<f64>,
    max_rows_built: usize,
    last_offset: f32,
    last_change: Option<Instant>,
    prev_frame: Option<Instant>,
    /// Frame intervals, grouped by continuous scrolling session.
    sessions: Vec<Vec<f64>>,
}

impl Trace {
    /// `Some` when `FX_TRACE=1`.
    pub fn from_env(path: &str) -> Option<Self> {
        if std::env::var_os("FX_TRACE").is_none_or(|v| v != "1") {
            return None;
        }
        Some(Self {
            exit_after_first_frame: std::env::var_os("FX_TRACE_EXIT").is_some_and(|v| v == "1"),
            path: path.to_owned(),
            process_start: fx_shell::process_start_time(),
            marks: Vec::new(),
            first_frame_ms: None,
            first_frame_items: 0,
            loaded_ms: None,
            max_rows_built: 0,
            last_offset: 0.0,
            last_change: None,
            prev_frame: None,
            sessions: Vec::new(),
        })
    }

    fn since_start_ms(&self) -> f64 {
        self.process_start
            .and_then(|s| SystemTime::now().duration_since(s).ok())
            .map_or(f64::NAN, millis)
    }

    /// Records a named milestone once (diagnostics for the startup path).
    pub fn mark(&mut self, name: &'static str) {
        if !self.marks.iter().any(|(n, _)| *n == name) {
            let ms = self.since_start_ms();
            self.marks.push((name, ms));
        }
    }

    pub fn has_first_frame(&self) -> bool {
        self.first_frame_ms.is_some()
    }

    /// Called at the start of the frame after the first render that had rows,
    /// i.e. once that frame has been drawn and presented.
    pub fn record_first_frame(&mut self, items: usize) {
        if self.first_frame_ms.is_none() {
            self.first_frame_ms = Some(self.since_start_ms());
            self.first_frame_items = items;
        }
    }

    pub fn record_loaded(&mut self) {
        self.loaded_ms = Some(self.since_start_ms());
    }

    pub fn record_rows_built(&mut self, rows: usize) {
        self.max_rows_built = self.max_rows_built.max(rows);
    }

    /// Called on every render with the list's scroll offset. Returns `true` while
    /// scrolling is active: the caller then keeps requesting frames and calls
    /// [`Trace::record_frame`] at the start of each.
    pub fn observe_scroll(&mut self, offset: f32) -> bool {
        let now = Instant::now();
        if offset != self.last_offset {
            self.last_offset = offset;
            self.last_change = Some(now);
        }
        let active = self
            .last_change
            .is_some_and(|t| now.duration_since(t) < ACTIVE_AFTER_SCROLL);
        if !active {
            self.prev_frame = None;
        }
        active
    }

    /// Start of a frame during active scrolling; records the interval since the
    /// previous one (frames are paced by the display, so an interval above one
    /// refresh period is a dropped frame).
    pub fn record_frame(&mut self) {
        let now = Instant::now();
        match self.prev_frame {
            Some(prev) => {
                if let Some(session) = self.sessions.last_mut() {
                    session.push(millis(now.duration_since(prev)));
                }
            }
            None => self.sessions.push(Vec::new()),
        }
        self.prev_frame = Some(now);
    }

    pub fn write(&self) {
        if let Err(e) = self.try_write() {
            eprintln!("fx: could not write trace: {e}");
        }
    }

    fn try_write(&self) -> std::io::Result<()> {
        let mut out = String::new();
        let opt = |v: Option<f64>| v.map_or_else(|| "none".to_owned(), |v| format!("{v:.3}"));
        let _ = writeln!(out, "path {}", self.path);
        let _ = writeln!(out, "pid {}", std::process::id());
        for (name, ms) in &self.marks {
            let _ = writeln!(out, "mark_{name}_ms {ms:.3}");
        }
        let _ = writeln!(out, "first_frame_ms {}", opt(self.first_frame_ms));
        let _ = writeln!(out, "first_frame_items {}", self.first_frame_items);
        let _ = writeln!(out, "loaded_ms {}", opt(self.loaded_ms));
        let _ = writeln!(out, "max_rows_built {}", self.max_rows_built);
        // Scroll position when the trace was written (negative = scrolled down).
        let _ = writeln!(out, "final_offset_px {:.1}", self.last_offset);
        for session in self.sessions.iter().filter(|s| !s.is_empty()) {
            let _ = writeln!(out, "session {}", session.len());
            for f in session {
                let _ = writeln!(out, "frame_ms {f:.3}");
            }
        }
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let file = PathBuf::from("bench")
            .join("results")
            .join(format!("trace-{stamp}-{}.txt", std::process::id()));
        std::fs::write(file, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace() -> Trace {
        Trace {
            exit_after_first_frame: false,
            path: String::new(),
            process_start: Some(SystemTime::now()),
            marks: Vec::new(),
            first_frame_ms: None,
            first_frame_items: 0,
            loaded_ms: None,
            max_rows_built: 0,
            last_offset: 0.0,
            last_change: None,
            prev_frame: None,
            sessions: Vec::new(),
        }
    }

    #[test]
    fn frames_are_recorded_only_while_scrolling() {
        let mut t = trace();
        assert!(!t.observe_scroll(0.0), "no scroll yet");
        assert!(t.observe_scroll(-24.0));
        t.record_frame();
        t.record_frame();
        assert_eq!(t.sessions.len(), 1);
        assert_eq!(t.sessions[0].len(), 1, "first frame has no predecessor");
        std::thread::sleep(ACTIVE_AFTER_SCROLL + Duration::from_millis(20));
        assert!(!t.observe_scroll(-24.0), "idle after the scroll stops");
        assert!(t.observe_scroll(-48.0), "scrolling again");
        t.record_frame();
        t.record_frame();
        assert_eq!(t.sessions.len(), 2, "an idle gap starts a new session");
        assert_eq!(t.sessions[1].len(), 1);
    }

    #[test]
    fn first_frame_is_recorded_once() {
        let mut t = trace();
        t.record_first_frame(100);
        let first = t.first_frame_ms;
        t.record_first_frame(5);
        assert_eq!(t.first_frame_ms, first);
        assert_eq!(t.first_frame_items, 100);
        t.record_rows_built(40);
        t.record_rows_built(12);
        assert_eq!(t.max_rows_built, 40);
    }
}
