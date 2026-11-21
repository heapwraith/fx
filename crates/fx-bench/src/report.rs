//! Statistics and result files (docs/PERFORMANCE.md, "Saída").

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Median and p95 (nearest-rank) of `samples` in milliseconds. `samples` must
/// not be empty.
pub fn median_p95(samples: &[Duration]) -> (f64, f64) {
    let mut ms: Vec<f64> = samples.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    ms.sort_by(f64::total_cmp);
    let n = ms.len();
    let median = if n % 2 == 1 {
        ms[n / 2]
    } else {
        (ms[n / 2 - 1] + ms[n / 2]) / 2.0
    };
    let rank = (n * 95).div_ceil(100).max(1);
    (median, ms[rank - 1])
}

/// `YYYY-MM-DD` (UTC) for `t`.
pub fn utc_date(t: SystemTime) -> String {
    let days = t
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    let (year, month, day) = fx_core::format::civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Commit the binary was built from (embedded by build.rs).
pub fn commit() -> &'static str {
    env!("FX_COMMIT")
}

/// CPU, logical cores and Windows version of this machine.
pub fn machine() -> String {
    let cpu = std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown CPU".into());
    let cores = std::thread::available_parallelism().map_or(0, |n| n.get());
    let os = std::process::Command::new("cmd")
        .args(["/C", "ver"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown OS".into());
    format!("{cpu}; {cores} logical cores; {os}")
}

pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `bench/results` under the current directory (the repository root).
pub fn results_dir() -> std::io::Result<PathBuf> {
    let dir = Path::new("bench").join("results");
    if dir.is_dir() {
        Ok(dir)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "bench/results not found: run fx-bench from the repository root",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: &[u64]) -> Vec<Duration> {
        v.iter().map(|&x| Duration::from_millis(x)).collect()
    }

    #[test]
    fn median_and_p95() {
        assert_eq!(median_p95(&ms(&[5])), (5.0, 5.0));
        assert_eq!(median_p95(&ms(&[3, 1, 2])), (2.0, 3.0));
        // 10 samples: median = mean of 5th and 6th, p95 = 10th (nearest rank).
        assert_eq!(
            median_p95(&ms(&[10, 1, 9, 2, 8, 3, 7, 4, 6, 5])),
            (5.5, 10.0)
        );
        let twenty: Vec<u64> = (1..=20).collect();
        assert_eq!(median_p95(&ms(&twenty)).1, 19.0);
    }

    #[test]
    fn dates() {
        let day = |d: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(d * 86_400 + 3_600);
        assert_eq!(utc_date(day(0)), "1970-01-01");
        assert_eq!(utc_date(day(11_016)), "2000-02-29");
        assert_eq!(utc_date(day(20_778)), "2026-11-21");
    }

    #[test]
    fn json_escaping() {
        assert_eq!(json_string(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(json_string("x\ny\u{1}"), "\"x\\ny\\u0001\"");
        assert_eq!(json_string("ç日"), "\"ç日\"");
    }
}
