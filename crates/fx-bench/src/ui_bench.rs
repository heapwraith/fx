//! UI benchmarks for task T0.5, driving the real `fx.exe` with `FX_TRACE=1`:
//! - `fx-bench open`: P2, 10 openings of `flat-100k` (first frame with rows).
//! - `fx-bench scroll`: P7, frame intervals while `bench/scroll-p7.ps1` scrolls.
//! - `fx-bench responsive <folder>`: the window keeps answering while it loads.
//! - `fx-bench type`: P4, key -> frame latency while typing to filter (T0.6).

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant, SystemTime};

use crate::fixtures;
use crate::report::{commit, json_string, machine, median_p95, results_dir, utc_date};

const OPENINGS: usize = 10;
const TARGET_P2_MS: f64 = 100.0;
/// One frame at the 239 Hz display of the reference machine (user's choice).
const TARGET_P7_MS: f64 = 4.2;

/// A trace written by fx (`key value` lines; see fx-ui/src/trace.rs).
#[derive(Debug, Default)]
pub struct TraceFile {
    lines: Vec<(String, String)>,
}

impl TraceFile {
    pub fn parse(text: &str) -> Self {
        let lines = text
            .lines()
            .filter_map(|l| l.split_once(' '))
            .map(|(k, v)| (k.to_owned(), v.trim().to_owned()))
            .collect();
        Self { lines }
    }

    pub fn number(&self, key: &str) -> Option<f64> {
        self.lines
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| v.parse().ok())
    }

    /// Every value of a repeated key (e.g. `input_ms`), in file order.
    pub fn numbers(&self, key: &str) -> Vec<f64> {
        self.lines
            .iter()
            .filter(|(k, _)| k == key)
            .filter_map(|(_, v)| v.parse().ok())
            .collect()
    }

    /// Frame intervals grouped by scrolling session.
    pub fn sessions(&self) -> Vec<Vec<f64>> {
        let mut sessions: Vec<Vec<f64>> = Vec::new();
        for (k, v) in &self.lines {
            match k.as_str() {
                "session" => sessions.push(Vec::new()),
                "frame_ms" => {
                    if let (Some(s), Ok(ms)) = (sessions.last_mut(), v.parse()) {
                        s.push(ms);
                    }
                }
                _ => {}
            }
        }
        sessions
    }
}

fn fx_exe() -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?.with_file_name("fx.exe");
    if exe.is_file() {
        Ok(exe)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} not found: build fx-ui in release first", exe.display()),
        ))
    }
}

fn flat_100k() -> std::io::Result<PathBuf> {
    let dir = fixtures::default_root().join("flat-100k");
    if dir.is_dir() {
        Ok(dir)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} missing: run `fx-bench fixtures` first", dir.display()),
        ))
    }
}

fn spawn_fx(folder: &Path, exit_after_first_frame: bool) -> std::io::Result<Child> {
    let mut cmd = Command::new(fx_exe()?);
    cmd.arg(folder).env("FX_TRACE", "1");
    if exit_after_first_frame {
        cmd.env("FX_TRACE_EXIT", "1");
    } else {
        cmd.env_remove("FX_TRACE_EXIT");
    }
    cmd.spawn()
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> std::io::Result<()> {
    let start = Instant::now();
    while child.try_wait()?.is_none() {
        if start.elapsed() > timeout {
            child.kill()?;
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "fx did not exit in time",
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

/// The trace fx wrote for process `pid`.
fn read_trace(pid: u32) -> std::io::Result<TraceFile> {
    let suffix = format!("-{pid}.txt");
    for entry in std::fs::read_dir(results_dir()?)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("trace-") && name.ends_with(&suffix) {
            return Ok(TraceFile::parse(&std::fs::read_to_string(path)?));
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("no trace for fx process {pid}"),
    ))
}

fn ms_samples(v: &[f64]) -> Vec<Duration> {
    v.iter()
        .map(|&ms| Duration::from_secs_f64(ms / 1000.0))
        .collect()
}

/// P2: 10 openings, each a new process that quits after its first frame with rows.
pub fn open() -> std::io::Result<()> {
    let folder = flat_100k()?;
    let mut from_process = Vec::with_capacity(OPENINGS);
    let mut from_request = Vec::with_capacity(OPENINGS);
    for i in 0..OPENINGS {
        let mut child = spawn_fx(&folder, true)?;
        wait_with_timeout(&mut child, Duration::from_secs(30))?;
        let trace = read_trace(child.id())?;
        let first = trace
            .number("first_frame_ms")
            .ok_or_else(|| std::io::Error::other(format!("opening {i}: no first frame")))?;
        let requested = trace.number("mark_folder_requested_ms").unwrap_or(f64::NAN);
        from_process.push(first);
        from_request.push(first - requested);
    }
    let (proc_med, proc_p95) = median_p95(&ms_samples(&from_process));
    let (req_med, req_p95) = median_p95(&ms_samples(&from_request));
    let pass_process = proc_med <= TARGET_P2_MS;
    let pass_request = req_med <= TARGET_P2_MS;

    println!(
        "P2: first frame with rows, flat-100k, {OPENINGS} openings (new process each), release"
    );
    println!(
        "  (a) from process start      median {proc_med:>7.1} ms  p95 {proc_p95:>7.1} ms  {}",
        if pass_process { "pass" } else { "FAIL" }
    );
    println!(
        "  (b) from folder request     median {req_med:>7.1} ms  p95 {req_p95:>7.1} ms  {}   (app already running)",
        if pass_request { "pass" } else { "FAIL" }
    );
    let date = utc_date(SystemTime::now());
    let json = format!(
        concat!(
            "{{\n  \"id\": \"P2\",\n  \"task\": \"T0.5\",\n  \"folder\": \"flat-100k\",\n",
            "  \"openings\": {n},\n  \"target_ms\": {t},\n",
            "  \"from_process_start_ms\": {{ \"median\": {pm:.3}, \"p95\": {pp:.3}, \"pass\": {pa} }},\n",
            "  \"from_folder_request_ms\": {{ \"median\": {rm:.3}, \"p95\": {rp:.3}, \"pass\": {ra} }},\n",
            "  \"machine\": {machine},\n  \"commit\": {commit},\n  \"date\": {date}\n}}\n"
        ),
        n = OPENINGS,
        t = TARGET_P2_MS,
        pm = proc_med,
        pp = proc_p95,
        pa = pass_process,
        rm = req_med,
        rp = req_p95,
        ra = pass_request,
        machine = json_string(&machine()),
        commit = json_string(commit()),
        date = json_string(&date),
    );
    std::fs::write(results_dir()?.join(format!("{date}-T0.5-P2.json")), json)?;
    Ok(())
}

struct FrameStats {
    frames: usize,
    median: f64,
    p95: f64,
    max: f64,
    over_target: usize,
    /// Intervals of 1.5 refresh periods or more: a frame was skipped.
    dropped: usize,
}

fn frame_stats(frames: &[f64]) -> Option<FrameStats> {
    if frames.is_empty() {
        return None;
    }
    let (median, p95) = median_p95(&ms_samples(frames));
    Some(FrameStats {
        frames: frames.len(),
        median,
        p95,
        max: frames.iter().copied().fold(0.0, f64::max),
        over_target: frames.iter().filter(|&&f| f > TARGET_P7_MS).count(),
        dropped: frames.iter().filter(|&&f| f >= TARGET_P7_MS * 1.5).count(),
    })
}

fn stats_json(s: &FrameStats) -> String {
    format!(
        "{{ \"frames\": {}, \"median\": {:.3}, \"p95\": {:.3}, \"max\": {:.3}, \"over_target\": {}, \"dropped\": {} }}",
        s.frames, s.median, s.p95, s.max, s.over_target, s.dropped
    )
}

/// P7: scroll with the wheel to the end, then drag the thumb end to end.
pub fn scroll() -> std::io::Result<()> {
    let folder = flat_100k()?;
    let mut child = spawn_fx(&folder, false)?;
    // Let the folder load and sort before scrolling.
    std::thread::sleep(Duration::from_secs(2));
    let script = Path::new("bench").join("scroll-p7.ps1");
    let out = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .args(["-ProcessId", &child.id().to_string()])
        .output()?;
    if !out.status.success() {
        let _ = child.kill();
        return Err(std::io::Error::other(format!(
            "scroll script failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let window = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    // Close the window normally so fx writes its trace.
    Command::new("powershell")
        .args(["-NoProfile", "-Command"])
        .arg(format!(
            "(Get-Process -Id {}).CloseMainWindow() | Out-Null",
            child.id()
        ))
        .status()?;
    wait_with_timeout(&mut child, Duration::from_secs(10))?;
    let trace = read_trace(child.id())?;

    let sessions = trace.sessions();
    let all: Vec<f64> = sessions.iter().flatten().copied().collect();
    let Some(total) = frame_stats(&all) else {
        return Err(std::io::Error::other("no frames recorded while scrolling"));
    };
    let rows_built = trace.number("max_rows_built").unwrap_or(f64::NAN);
    // The wheel phase runs last, so the final offset shows whether it reached the
    // end of the list (rows are 24 px; the viewport is ~585 px tall).
    let list_px = fixtures::Spec::FULL.flat_files as f64 * 24.0;
    let reached = -trace.number("final_offset_px").unwrap_or(0.0) / list_px;
    let pass = total.p95 <= TARGET_P7_MS;

    println!(
        "P7: frame intervals while scrolling flat-100k (display: 239 Hz, target {TARGET_P7_MS} ms), release"
    );
    println!(
        "  window: {window}; max rows built per frame: {rows_built}; wheel reached {:.1}% of the list",
        reached * 100.0
    );
    let show = |label: &str, s: &FrameStats| {
        println!(
            "  {label:<10} {:>5} frames  median {:>5.2} ms  p95 {:>5.2} ms  max {:>6.2} ms  >{TARGET_P7_MS} ms: {}  dropped: {}",
            s.frames, s.median, s.p95, s.max, s.over_target, s.dropped
        );
    };
    let mut session_json = Vec::new();
    for (i, s) in sessions.iter().enumerate() {
        if let Some(st) = frame_stats(s) {
            show(&format!("session {}", i + 1), &st);
            session_json.push(stats_json(&st));
        }
    }
    show("all", &total);
    println!("  {}", if pass { "pass" } else { "FAIL" });

    let date = utc_date(SystemTime::now());
    let json = format!(
        concat!(
            "{{\n  \"id\": \"P7\",\n  \"task\": \"T0.5\",\n  \"folder\": \"flat-100k\",\n",
            "  \"display_hz\": 239,\n  \"target_ms\": {t},\n  \"input\": \"bench/scroll-p7.ps1 (wheel to the end, then thumb drag both ways)\",\n",
            "  \"max_rows_built\": {rows},\n  \"wheel_reached_fraction\": {reached:.4},\n  \"all\": {all},\n  \"sessions\": [{sessions}],\n  \"pass\": {pass},\n",
            "  \"machine\": {machine},\n  \"commit\": {commit},\n  \"date\": {date}\n}}\n"
        ),
        t = TARGET_P7_MS,
        rows = rows_built,
        reached = reached,
        all = stats_json(&total),
        sessions = session_json.join(", "),
        pass = pass,
        machine = json_string(&machine()),
        commit = json_string(commit()),
        date = json_string(&date),
    );
    std::fs::write(results_dir()?.join(format!("{date}-T0.5-P7.json")), json)?;
    Ok(())
}

/// Window must answer within this while the folder loads (no "not responding").
const TARGET_RESPONSE_MS: f64 = 100.0;

/// Opens `folder` (e.g. a slow network path) and, while it loads, moves and
/// resizes the window and measures how fast it answers (bench/responsive.ps1).
pub fn responsive(folder: &Path, seconds: u32) -> std::io::Result<()> {
    let mut child = spawn_fx(folder, false)?;
    let script = Path::new("bench").join("responsive.ps1");
    let out = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .args(["-ProcessId", &child.id().to_string()])
        .args(["-Seconds", &seconds.to_string()])
        .output()?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    Command::new("powershell")
        .args(["-NoProfile", "-Command"])
        .arg(format!(
            "(Get-Process -Id {}).CloseMainWindow() | Out-Null",
            child.id()
        ))
        .status()?;
    wait_with_timeout(&mut child, Duration::from_secs(10))?;
    if !out.status.success() {
        return Err(std::io::Error::other(format!(
            "responsive script failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let fields: Vec<f64> = stdout
        .split_whitespace()
        .filter_map(|f| f.parse().ok())
        .collect();
    let [samples, max_ms, p95_ms, timeouts] = fields[..] else {
        return Err(std::io::Error::other(format!(
            "unexpected script output: {stdout}"
        )));
    };
    let trace = read_trace(child.id())?;
    let first = trace.number("first_frame_ms").unwrap_or(f64::NAN);
    let loaded = trace.number("loaded_ms");
    let items = trace.number("first_frame_items").unwrap_or(f64::NAN);
    let still_loading_at_end = loaded.is_none_or(|l| l > f64::from(seconds) * 1000.0);
    let pass = timeouts == 0.0 && max_ms <= TARGET_RESPONSE_MS;

    println!(
        "Responsiveness while loading {} (release)",
        folder.display()
    );
    println!(
        "  first frame {first:.0} ms ({items} items); loaded {}",
        loaded.map_or_else(|| "not finished".to_owned(), |l| format!("{l:.0} ms"))
    );
    println!(
        "  {samples} move/resize + WM_NULL probes over {seconds} s: max {max_ms:.1} ms, p95 {p95_ms:.1} ms, timeouts {timeouts}   target ≤ {TARGET_RESPONSE_MS} ms  {}",
        if pass { "pass" } else { "FAIL" }
    );
    if !still_loading_at_end {
        println!("  note: loading finished during the probe window");
    }
    let date = utc_date(SystemTime::now());
    let json = format!(
        concat!(
            "{{\n  \"id\": \"T0.5-responsive\",\n  \"folder\": {folder},\n",
            "  \"first_frame_ms\": {first:.3},\n  \"loaded_ms\": {loaded},\n",
            "  \"probes\": {samples},\n  \"probe_seconds\": {seconds},\n",
            "  \"response_ms\": {{ \"max\": {max:.3}, \"p95\": {p95:.3}, \"timeouts\": {timeouts}, \"target\": {t} }},\n",
            "  \"pass\": {pass},\n  \"machine\": {machine},\n  \"commit\": {commit},\n  \"date\": {date}\n}}\n"
        ),
        folder = json_string(&folder.display().to_string()),
        first = first,
        loaded = loaded.map_or_else(|| "null".to_owned(), |l| format!("{l:.3}")),
        samples = samples,
        seconds = seconds,
        max = max_ms,
        p95 = p95_ms,
        timeouts = timeouts,
        t = TARGET_RESPONSE_MS,
        pass = pass,
        machine = json_string(&machine()),
        commit = json_string(commit()),
        date = json_string(&date),
    );
    std::fs::write(
        results_dir()?.join(format!("{date}-T0.5-responsive.json")),
        json,
    )?;
    Ok(())
}

const TARGET_P4_MS: f64 = 16.0;
const TYPE_ROUNDS: usize = 3;

/// P4: types "report-123" (10 keys, 50 ms apart) into fx on `flat-100k`, 3 rounds.
pub fn type_keys() -> std::io::Result<()> {
    let folder = flat_100k()?;
    let mut child = spawn_fx(&folder, false)?;
    // Let the folder load and sort first, so typing filters the full list.
    std::thread::sleep(Duration::from_secs(2));
    let out = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(Path::new("bench").join("type-p4.ps1"))
        .args(["-ProcessId", &child.id().to_string()])
        .args(["-Rounds", &TYPE_ROUNDS.to_string()])
        .output()?;
    Command::new("powershell")
        .args(["-NoProfile", "-Command"])
        .arg(format!(
            "(Get-Process -Id {}).CloseMainWindow() | Out-Null",
            child.id()
        ))
        .status()?;
    wait_with_timeout(&mut child, Duration::from_secs(10))?;
    if !out.status.success() {
        return Err(std::io::Error::other(format!(
            "typing script failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let samples = read_trace(child.id())?.numbers("input_ms");
    if samples.is_empty() {
        return Err(std::io::Error::other("no key latency recorded"));
    }
    let (median, p95) = median_p95(&ms_samples(&samples));
    let max = samples.iter().copied().fold(0.0, f64::max);
    let pass = p95 <= TARGET_P4_MS;
    println!("P4: key -> frame presented while typing to filter flat-100k (release)");
    println!(
        "  {} keys ({TYPE_ROUNDS} x \"report-123\" + Escape): median {median:.2} ms  p95 {p95:.2} ms  max {max:.2} ms   target ≤ {TARGET_P4_MS} ms  {}",
        samples.len(),
        if pass { "pass" } else { "FAIL" }
    );
    let date = utc_date(SystemTime::now());
    let json = format!(
        concat!(
            "{{\n  \"id\": \"P4\",\n  \"task\": \"T0.6\",\n  \"folder\": \"flat-100k\",\n",
            "  \"input\": \"bench/type-p4.ps1: 3 x 'report-123' + Escape, 50 ms between keys\",\n",
            "  \"keys\": {n},\n  \"latency_ms\": {{ \"median\": {m:.3}, \"p95\": {p:.3}, \"max\": {x:.3}, \"target\": {t} }},\n",
            "  \"pass\": {pass},\n  \"machine\": {machine},\n  \"commit\": {commit},\n  \"date\": {date}\n}}\n"
        ),
        n = samples.len(),
        m = median,
        p = p95,
        x = max,
        t = TARGET_P4_MS,
        pass = pass,
        machine = json_string(&machine()),
        commit = json_string(commit()),
        date = json_string(&date),
    );
    std::fs::write(results_dir()?.join(format!("{date}-T0.6-P4.json")), json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_trace_numbers_and_sessions() {
        let t = TraceFile::parse(
            "path C:\\x y\nfirst_frame_ms 390.5\nloaded_ms none\nsession 2\nframe_ms 4.1\nframe_ms 8.4\nsession 1\nframe_ms 4.2\n",
        );
        assert_eq!(t.number("first_frame_ms"), Some(390.5));
        assert_eq!(t.number("loaded_ms"), None);
        assert_eq!(t.number("missing"), None);
        assert_eq!(t.sessions(), vec![vec![4.1, 8.4], vec![4.2]]);
        let t = TraceFile::parse("input_ms 3.5\nframe_ms 4.1\ninput_ms 7.25\n");
        assert_eq!(t.numbers("input_ms"), vec![3.5, 7.25]);
    }

    #[test]
    fn frame_stats_count_drops() {
        let s = frame_stats(&[4.1, 4.2, 4.3, 8.4, 4.1]).unwrap();
        assert_eq!(s.frames, 5);
        assert_eq!(s.over_target, 2);
        assert_eq!(s.dropped, 1);
        assert_eq!(s.max, 8.4);
        assert!(frame_stats(&[]).is_none());
    }
}
