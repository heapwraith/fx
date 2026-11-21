//! Display formatting for list columns.

use std::time::{Duration, SystemTime};

/// Explorer-style size: bytes are shown in whole KB, rounded up ("0 KB" only for
/// empty files), with thousands separators: `1 KB`, `12.345 KB`.
pub fn size_kb(bytes: u64) -> String {
    let kb = bytes.div_ceil(1024);
    format!("{} KB", group_thousands(kb))
}

fn group_thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Days since 1970-01-01 to (year, month, day) — Howard Hinnant's
/// `civil_from_days`, valid for the whole proleptic Gregorian calendar.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// `DD/MM/YYYY HH:MM` in UTC (the local time zone is a later task).
pub fn date_time_utc(t: SystemTime) -> String {
    let secs = match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs_f64().ceil() as i64),
    };
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!(
        "{d:02}/{m:02}/{y:04} {:02}:{:02}",
        s / 3600,
        (s % 3600) / 60
    )
}

/// Milliseconds as `f64`, for traces and reports.
pub fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_round_up_to_kb() {
        assert_eq!(size_kb(0), "0 KB");
        assert_eq!(size_kb(1), "1 KB");
        assert_eq!(size_kb(1024), "1 KB");
        assert_eq!(size_kb(1025), "2 KB");
        assert_eq!(size_kb(12_345 * 1024), "12.345 KB");
        assert_eq!(size_kb(1_234_567 * 1024), "1.234.567 KB");
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(20_778), (2026, 11, 21));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn date_time_format() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(20_778 * 86_400 + 13 * 3600 + 7 * 60);
        assert_eq!(date_time_utc(t), "21/11/2026 13:07");
        assert_eq!(date_time_utc(SystemTime::UNIX_EPOCH), "01/01/1970 00:00");
        assert_eq!(millis(Duration::from_micros(1500)), 1.5);
    }
}
