//! Local time zone offset for a given instant (task T1.19, D-069): the IANA
//! rules of the system's time zone, daylight saving included, for that
//! date, by `jiff` (bundled rules on Windows, the system's on Linux).

use std::time::SystemTime;

/// Seconds to add to UTC to get the local time at `t` (0 when the instant is
/// out of range or the time zone is unknown).
pub fn utc_offset_at(t: SystemTime) -> i64 {
    let Ok(ts) = jiff::Timestamp::try_from(t) else {
        return 0;
    };
    // `system()` is cached by jiff and re-checked now and then.
    jiff::tz::TimeZone::try_system().map_or(0, |tz| i64::from(tz.to_offset(ts).seconds()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn offset_is_whole_minutes_and_plausible() {
        let off = utc_offset_at(SystemTime::now());
        assert!(off.abs() <= 14 * 3600, "{off}");
        assert_eq!(off % 60, 0);
        // The earliest instant the platform has (Windows stops at 1601).
        let early = (1..=400).rev().find_map(|y| {
            SystemTime::UNIX_EPOCH.checked_sub(Duration::from_secs(y * 365 * 86_400))
        });
        if let Some(t) = early {
            let _ = utc_offset_at(t);
        }
    }

    /// The same offsets Windows gives for its own time zone, over the last
    /// 20 years (every 2 weeks, so daylight saving changes are crossed).
    #[cfg(windows)]
    #[test]
    fn agrees_with_windows() {
        use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
        use windows::Win32::System::Time::{
            FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTimeEx,
        };
        const EPOCH_TICKS: i64 = 116_444_736_000_000_000;
        let windows_offset = |secs: i64| {
            let ticks = secs * 10_000_000 + EPOCH_TICKS;
            let utc = FILETIME {
                dwLowDateTime: ticks as u32,
                dwHighDateTime: (ticks >> 32) as u32,
            };
            let (mut st, mut local, mut out) = (
                SYSTEMTIME::default(),
                SYSTEMTIME::default(),
                FILETIME::default(),
            );
            // SAFETY: pointers to live locals of the expected types; `None`
            // is the current time zone.
            unsafe {
                FileTimeToSystemTime(&utc, &mut st).unwrap();
                SystemTimeToTzSpecificLocalTimeEx(None, &st, &mut local).unwrap();
                SystemTimeToFileTime(&local, &mut out).unwrap();
            }
            let local = (i64::from(out.dwHighDateTime) << 32) | i64::from(out.dwLowDateTime);
            (local - ticks) / 10_000_000
        };
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let mut differ = Vec::new();
        let mut secs = now - 20 * 365 * 86_400;
        while secs < now {
            let t = SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64);
            let (ours, theirs) = (utc_offset_at(t), windows_offset(secs));
            if ours != theirs {
                differ.push((secs, ours, theirs));
            }
            secs += 14 * 86_400;
        }
        // Windows only keeps a few years of past rule changes for some zones;
        // allow a handful of old instants to differ, none in the last 5 years.
        let recent = now - 5 * 365 * 86_400;
        assert!(
            differ.iter().all(|(s, ..)| *s < recent) && differ.len() <= 26,
            "{differ:?}"
        );
    }
}
