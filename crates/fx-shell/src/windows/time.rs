//! Local time zone offset for a given instant (task T1.19): Windows rules,
//! daylight saving included, for that date (the same conversion Explorer
//! shows in its Date column).

use std::time::SystemTime;

use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::System::Time::{
    FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTimeEx,
};

/// 100 ns ticks between 1601-01-01 (FILETIME) and 1970-01-01.
const EPOCH_TICKS: i64 = 116_444_736_000_000_000;
const TICKS_PER_SEC: i64 = 10_000_000;

/// Seconds to add to UTC to get the local time at `t` (0 if Windows cannot
/// convert it, e.g. dates before 1601).
pub fn utc_offset_at(t: SystemTime) -> i64 {
    let secs = match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    let Some(ticks) = secs
        .checked_mul(TICKS_PER_SEC)
        .and_then(|t| t.checked_add(EPOCH_TICKS))
        .filter(|t| *t >= 0)
    else {
        return 0;
    };
    let utc = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    local_ticks(&utc).map_or(0, |local| (local - ticks) / TICKS_PER_SEC)
}

fn local_ticks(utc: &FILETIME) -> Option<i64> {
    let mut st = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    let mut out = FILETIME::default();
    // SAFETY: every pointer is to a live local of the type the function
    // expects; `None` selects the current time zone.
    unsafe {
        FileTimeToSystemTime(utc, &mut st).ok()?;
        SystemTimeToTzSpecificLocalTimeEx(None, &st, &mut local).ok()?;
        SystemTimeToFileTime(&local, &mut out).ok()?;
    }
    Some((i64::from(out.dwHighDateTime) << 32) | i64::from(out.dwLowDateTime))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn offset_is_whole_minutes_and_plausible() {
        let now = SystemTime::now();
        let off = utc_offset_at(now);
        assert!(off.abs() <= 14 * 3600, "{off}");
        assert_eq!(off % 60, 0);
        // Out-of-range dates do not panic.
        // (A FILETIME cannot go below 1601; take the earliest instant there is.)
        let early = (1..=400).rev().find_map(|y| {
            SystemTime::UNIX_EPOCH.checked_sub(Duration::from_secs(y * 365 * 86_400))
        });
        if let Some(t) = early {
            let _ = utc_offset_at(t);
        }
    }
}
