//! Display formatting for list columns.

use std::time::{Duration, SystemTime};

use crate::locale::english;

/// File size for lists: whole KB below 1 MB, rounded up like Explorer ("0 KB"
/// only for empty files), then MB, GB or TB (binary units) with one decimal
/// below 100: `14 KB`, `17,6 MB`, `250 GB`.
pub fn size(bytes: u64) -> String {
    let kb = bytes.div_ceil(1024);
    if kb < 1024 {
        return format!("{kb} KB");
    }
    const UNITS: [&str; 3] = ["MB", "GB", "TB"];
    // From the rounded-up KB, so 1 byte short of 1 MB reads "1,0 MB".
    let mut value = kb as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let text = if value < 100.0 {
        // Truncated, not rounded: 99.96 MB must not read "100,0 MB".
        let t = format!("{:.1}", (value * 10.0).floor() / 10.0);
        if english() { t } else { t.replace('.', ",") }
    } else {
        group_thousands(value.floor() as u64, english())
    };
    format!("{text} {}", UNITS[unit])
}

/// Short size for capacities: "931 GB", "1,8 TB", "512 MB" (binary units,
/// Brazilian decimal comma; one decimal below 10).
pub fn size_short(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let text = if unit > 0 && value < 10.0 {
        let t = format!("{value:.1}");
        if english() { t } else { t.replace('.', ",") }
    } else {
        format!("{value:.0}")
    };
    format!("{text} {}", UNITS[unit])
}

/// Item count with the Brazilian thousands separator: "1 item", "100.000 itens".
pub fn items(n: usize) -> String {
    let en = english();
    match (n, en) {
        (1, _) => "1 item".to_owned(),
        (n, false) => format!("{} itens", group_thousands(n as u64, en)),
        (n, true) => format!("{} items", group_thousands(n as u64, en)),
    }
}

/// A number with the Brazilian thousands separator: `100.000`.
pub fn count(n: usize) -> String {
    group_thousands(n as u64, english())
}

/// Thousands separator: "." in Portuguese, "," in English.
fn group_thousands(n: u64, en: bool) -> String {
    let sep = if en { ',' } else { '.' };
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
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

/// `DD/MM/YYYY HH:MM` (`MM/DD/YYYY` in English), shifted by `offset` seconds from UTC (the local
/// offset for that instant comes from the platform: fx-shell `utc_offset_at`).
pub fn date_time(t: SystemTime, offset: i64) -> String {
    let secs = match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs_f64().ceil() as i64),
    } + offset;
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    let (h, min) = (s / 3600, (s % 3600) / 60);
    if english() {
        format!("{m:02}/{d:02}/{y:04} {h:02}:{min:02}")
    } else {
        format!("{d:02}/{m:02}/{y:04} {h:02}:{min:02}")
    }
}

/// Milliseconds as `f64`, for traces and reports.
pub fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_climb_from_kb_to_tb() {
        const KB: u64 = 1024;
        const MB: u64 = KB * KB;
        const GB: u64 = MB * KB;
        assert_eq!(size(0), "0 KB");
        assert_eq!(size(1), "1 KB");
        assert_eq!(size(KB), "1 KB");
        assert_eq!(size(KB + 1), "2 KB");
        assert_eq!(size(MB - KB - 1), "1023 KB");
        assert_eq!(size(MB - 1), "1,0 MB");
        assert_eq!(size(MB), "1,0 MB");
        assert_eq!(size(18_068 * KB), "17,6 MB");
        assert_eq!(size(100 * MB - 100 * KB), "99,9 MB");
        assert_eq!(size(250 * MB), "250 MB");
        assert_eq!(size(1023 * MB), "1.023 MB");
        assert_eq!(size(GB + GB / 5), "1,2 GB");
        assert_eq!(size(250 * GB), "250 GB");
        assert_eq!(size(1_979_120_929_996), "1,8 TB");
        assert_eq!(size(5000 * GB * KB), "5.000 TB");
    }

    #[test]
    fn english_separators() {
        assert_eq!(group_thousands(1_234_567, true), "1,234,567");
        assert_eq!(group_thousands(1_234_567, false), "1.234.567");
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
        assert_eq!(date_time(t, 0), "21/11/2026 13:07");
        assert_eq!(date_time(SystemTime::UNIX_EPOCH, 0), "01/01/1970 00:00");
        // Brasília (UTC-3) crosses midnight backwards.
        assert_eq!(
            date_time(SystemTime::UNIX_EPOCH, -3 * 3600),
            "31/12/1969 21:00"
        );
        assert_eq!(millis(Duration::from_micros(1500)), 1.5);
    }

    #[test]
    fn counts() {
        assert_eq!(items(0), "0 itens");
        assert_eq!(items(1), "1 item");
        assert_eq!(items(999), "999 itens");
        assert_eq!(items(100_000), "100.000 itens");
        assert_eq!(count(1_234_567), "1.234.567");
    }

    #[test]
    fn short_sizes() {
        assert_eq!(size_short(0), "0 B");
        assert_eq!(size_short(1023), "1023 B");
        assert_eq!(size_short(1536), "1,5 KB");
        assert_eq!(size_short(512 * 1024 * 1024), "512 MB");
        assert_eq!(size_short(931 * 1024 * 1024 * 1024), "931 GB");
        assert_eq!(size_short(1_979_120_929_996), "1,8 TB");
    }
}
