//! Windows implementation.

use std::time::{Duration, SystemTime};

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

/// When the current process was created (for "time since process start"
/// measurements, docs/PERFORMANCE.md). `None` if Windows refuses to say.
pub fn process_start_time() -> Option<SystemTime> {
    let (mut creation, mut exit, mut kernel, mut user) = Default::default();
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing; the
    // four out-pointers are valid, writable FILETIMEs on this stack frame.
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    }
    .ok()?;
    Some(filetime_to_system_time(creation))
}

fn filetime_to_system_time(ft: FILETIME) -> SystemTime {
    const TICKS_1601_TO_1970: u64 = 116_444_736_000_000_000;
    let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    let since_1970 = ticks.saturating_sub(TICKS_1601_TO_1970);
    SystemTime::UNIX_EPOCH + Duration::from_nanos(since_1970.saturating_mul(100))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_started_recently_and_in_the_past() {
        let start = process_start_time().unwrap();
        let age = SystemTime::now().duration_since(start).unwrap();
        assert!(age < Duration::from_secs(600), "{age:?}");
    }
}
